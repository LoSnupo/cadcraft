//! IGES 5.3 export of 3D solids. The ACIS data of `3DSOLID` and `BODY` entities (as SAT text)
//! becomes IGES trimmed surfaces (144) on exact NURBS surfaces (128), bounded by curves on the
//! surface (142) given in parameter and model space (126). Planes, cylinders and cones with
//! line, circle, ellipse and intersection-curve edges are supported: the geometry of tubes,
//! pipes and plates, with saddle cuts, branch holes and inclined cuts.
//!
//! [`from_sat`] is the pure conversion; [`convert_file`] reads the solids of a DWG or DXF file
//! first (native builds only).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod brep;
mod geom;
mod sat;
mod trim;
mod writer;

use std::collections::BTreeMap;

/// Default chord tolerance of the trimming edges, mm.
pub const DEFAULT_TOL: f64 = 0.01;

/// An IGES file and what went into it.
#[derive(Clone, Debug, Default)]
pub struct Conversion {
    pub iges: String,
    pub solids: usize,
    /// Faces written, by surface type (`plane`, `cylinder`, `cone`).
    pub faces: BTreeMap<&'static str, usize>,
    /// Faces or solids left out, and why.
    pub warnings: Vec<String>,
    /// Largest distance from a sampled edge point to its face's surface, mm.
    pub surface_gap: f64,
    /// Largest distance from an edge's ends to its vertices, mm.
    pub vertex_gap: f64,
}

impl Conversion {
    pub fn face_count(&self) -> usize {
        self.faces.values().sum()
    }
    /// A short report, one line each.
    pub fn summary(&self) -> String {
        let kinds: Vec<String> = self.faces.iter().map(|(k, n)| format!("{n} {k}")).collect();
        let mut s = format!(
            "{} solid(s), {} faces ({})\nedges on their surfaces within {:.4} mm, on their vertices within {:.4} mm",
            self.solids,
            self.face_count(),
            kinds.join(", "),
            self.surface_gap,
            self.vertex_gap
        );
        for w in &self.warnings {
            s.push_str(&format!("\nleft out: {w}"));
        }
        s
    }
}

/// The current UTC time as IGES writes dates (`YYYYMMDD.HHNNSS`).
pub fn now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    chrono::DateTime::from_timestamp(i64::try_from(secs).unwrap_or(0), 0).map(|d| d.format("%Y%m%d.%H%M%S").to_string()).unwrap_or_default()
}

/// Convert solids given as SAT text. `file_name` and `source` go into the Global section.
pub fn from_sat(texts: &[String], file_name: &str, source: &str, date: &str, tol: f64) -> Result<Conversion, String> {
    if texts.is_empty() {
        return Err("no 3D solids found in model space".into());
    }
    let tol = if tol.is_finite() && tol > 0.0 { tol.clamp(1e-5, 10.0) } else { DEFAULT_TOL };
    let mut w = writer::Iges::new();
    let mut c = Conversion { solids: texts.len(), ..Default::default() };
    for (i, t) in texts.iter().enumerate() {
        let sat = match sat::parse(t) {
            Ok(s) => s,
            Err(e) => {
                c.warnings.push(format!("solid {}: {e}", i + 1));
                continue;
            }
        };
        let (faces, rep) = brep::faces(&sat, tol);
        c.vertex_gap = c.vertex_gap.max(rep.vertex_gap);
        c.surface_gap = c.surface_gap.max(rep.surface_gap);
        c.warnings.extend(rep.skipped.into_iter().map(|e| format!("solid {}: {e}", i + 1)));
        for f in &faces {
            match trim::trim(f) {
                Ok(t) => {
                    *c.faces.entry(t.kind).or_default() += 1;
                    w.trimmed(&t);
                }
                Err(e) => c.warnings.push(format!("solid {}: {e}", i + 1)),
            }
        }
    }
    if c.face_count() == 0 {
        return Err(format!("no face could be converted: {}", c.warnings.join("; ")));
    }
    c.iges = w.finish(file_name, date, source);
    Ok(c)
}

/// The SAT text of every 3D solid and body in model space of a DWG, DXF or SAT file.
#[cfg(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))))]
pub fn solids_in_file(path: &std::path::Path) -> Result<Vec<String>, String> {
    use acadrust::entities::EntityType;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "sat" {
        return std::fs::read_to_string(path).map(|t| vec![t]).map_err(|e| e.to_string());
    }
    let doc = match ext.as_str() {
        "dxf" => acadrust::io::dxf::DxfReader::from_file(path).and_then(|r| r.read()).map_err(|e| format!("cannot read the DXF: {e}"))?,
        "dwg" => {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            acadrust::io::dwg::DwgReader::from_stream(std::io::Cursor::new(bytes)).read().map_err(|e| format!("cannot read the DWG: {e}"))?
        }
        _ => return Err("IGES export reads solids from .dwg, .dxf or .sat files".into()),
    };
    let mut out = Vec::new();
    for e in doc.entities() {
        let data = match e {
            EntityType::Solid3D(s) => &s.acis_data,
            EntityType::Body(b) => &b.acis_data,
            _ => continue,
        };
        if let Some(d) = data.parse() {
            out.push(d.to_sat_string());
        }
    }
    Ok(out)
}

/// Convert the solids of a DWG, DXF or SAT file; `output_name` goes into the file's header.
#[cfg(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))))]
pub fn convert_file(input: &std::path::Path, output_name: &str, tol: f64) -> Result<Conversion, String> {
    let texts = solids_in_file(input)?;
    let source = input.file_name().and_then(|n| n.to_str()).unwrap_or("drawing");
    from_sat(&texts, output_name, source, &now(), tol)
}

/// Whether this build can read solids from files.
pub const READS_FILES: bool = cfg!(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))));
