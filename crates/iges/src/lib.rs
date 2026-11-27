//! IGES 5.3 and STEP export of 3D solids. The ACIS data of `3DSOLID` and `BODY` entities (as SAT text)
//! becomes IGES trimmed surfaces (144) on exact NURBS surfaces (128), bounded by curves on the
//! surface (142) given in parameter and model space (126). Planes, cylinders and cones with
//! line, circle, ellipse and intersection-curve edges are supported: the geometry of tubes,
//! pipes and plates, with saddle cuts, branch holes and inclined cuts.
//!
//! The same faces also become a STEP file (AP214, [`step_from_sat`]): closed solids with exact
//! planes, cylinders and cones, for software that wants solids rather than surfaces.
//!
//! [`from_sat`] is the pure conversion; [`convert_file`] and [`convert_file_step`] read the
//! solids of a DWG or DXF file first (native builds only).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod brep;
mod geom;
mod sat;
mod step;
mod trim;
mod views;
mod writer;

pub use views::View;

use std::collections::BTreeMap;

/// Default chord tolerance of the trimming edges, mm.
pub const DEFAULT_TOL: f64 = 0.01;

/// An IGES file and what went into it.
#[derive(Clone, Debug, Default)]
pub struct Conversion {
    /// The IGES file (empty for a STEP conversion).
    pub iges: String,
    /// The STEP file (empty for an IGES conversion).
    pub step: String,
    /// STEP: shells written as closed solids, and shells written as surfaces because their
    /// faces do not close a volume.
    pub closed: usize,
    pub open: usize,
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
        if !self.step.is_empty() {
            s.push_str(&format!("\n{} closed solid(s)", self.closed));
            if self.open > 0 {
                s.push_str(&format!(", {} written as surfaces (not closed)", self.open));
            }
        }
        for w in &self.warnings {
            s.push_str(&format!("\nleft out: {w}"));
        }
        s
    }
}

/// The current UTC time as STEP writes dates (ISO 8601).
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    chrono::DateTime::from_timestamp(i64::try_from(secs).unwrap_or(0), 0).map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()).unwrap_or_default()
}

/// Convert solids given as SAT text to STEP. `file_name` and `source` go into the header.
pub fn step_from_sat(texts: &[String], file_name: &str, source: &str, date: &str, tol: f64) -> Result<Conversion, String> {
    if texts.is_empty() {
        return Err("no 3D solids found in model space".into());
    }
    let tol = if tol.is_finite() && tol > 0.0 { tol.clamp(1e-5, 10.0) } else { DEFAULT_TOL };
    let mut c = Conversion { solids: texts.len(), ..Default::default() };
    let mut solids = Vec::new();
    for (i, t) in texts.iter().enumerate() {
        let sat = match sat::parse(t) {
            Ok(s) => s,
            Err(e) => {
                c.warnings.push(format!("solid {}: {e}", i + 1));
                solids.push((Vec::new(), BTreeMap::new()));
                continue;
            }
        };
        let (faces, rep) = brep::faces(&sat, tol);
        c.vertex_gap = c.vertex_gap.max(rep.vertex_gap);
        c.surface_gap = c.surface_gap.max(rep.surface_gap);
        c.warnings.extend(rep.skipped.into_iter().map(|e| format!("solid {}: {e}", i + 1)));
        solids.push((faces, rep.edge_geom));
    }
    let (text, w) = step::write(&solids, file_name, source, date, tol);
    c.faces = w.faces;
    c.warnings.extend(w.warnings);
    c.closed = w.breps.len();
    c.open = w.surfaces.len();
    if c.face_count() == 0 {
        return Err(format!("no face could be converted: {}", c.warnings.join("; ")));
    }
    c.step = text;
    Ok(c)
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

/// Front, right, top and isometric views of solids given as SAT text, and the solids or faces
/// that could not be read.
pub fn views_from_sat(texts: &[String], tol: f64) -> Result<(Vec<View>, Vec<String>), String> {
    if texts.is_empty() {
        return Err("no 3D solids found in model space".into());
    }
    let tol = if tol.is_finite() && tol > 0.0 { tol.clamp(1e-5, 10.0) } else { DEFAULT_TOL };
    let (mut edges, mut faces, mut warnings) = (Vec::new(), Vec::new(), Vec::new());
    for (i, t) in texts.iter().enumerate() {
        let sat = match sat::parse(t) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("solid {}: {e}", i + 1));
                continue;
            }
        };
        let (fs, rep) = brep::faces(&sat, tol);
        warnings.extend(rep.skipped.iter().map(|e| format!("solid {}: {e}", i + 1)));
        edges.extend(rep.edges);
        for f in &fs {
            match trim::trim(f) {
                Ok(t) => faces.push(t),
                Err(e) => warnings.push(format!("solid {}: {e}", i + 1)),
            }
        }
    }
    if edges.is_empty() {
        return Err(format!("no edges could be read: {}", warnings.join("; ")));
    }
    Ok((views::project(&edges, &faces), warnings))
}

/// The views of the solids of a DWG, DXF or SAT file.
#[cfg(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))))]
pub fn views_of_file(input: &std::path::Path, tol: f64) -> Result<(Vec<View>, Vec<String>), String> {
    views_from_sat(&solids_in_file(input)?, tol)
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
        _ => return Err("3D solids are read from .dwg, .dxf or .sat files".into()),
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

/// Convert the solids of a DWG, DXF or SAT file to STEP; `output_name` goes into the header.
#[cfg(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))))]
pub fn convert_file_step(input: &std::path::Path, output_name: &str, tol: f64) -> Result<Conversion, String> {
    let texts = solids_in_file(input)?;
    let source = input.file_name().and_then(|n| n.to_str()).unwrap_or("drawing");
    step_from_sat(&texts, output_name, source, &now_iso(), tol)
}

/// Whether this build can read solids from files.
pub const READS_FILES: bool = cfg!(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))));
