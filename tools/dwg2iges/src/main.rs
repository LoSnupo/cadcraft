//! dwg2iges: writes the 3D solids of DWG and DXF drawings as IGES 5.3 trimmed surfaces, for
//! CAM software (tube laser nesting, CNC) that imports IGES.
//!
//! Usage: `dwg2iges [options] FILE.dwg [FILE2.dxf …]` writes `FILE.igs` beside each input.
//! Dragging files onto the program on Windows does the same.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod brep;
mod geom;
mod iges;
mod sat;
mod trim;

use std::path::{Path, PathBuf};

use acadrust::entities::EntityType;

const HELP: &str = "dwg2iges — 3D solids of DWG/DXF drawings to IGES 5.3

Usage: dwg2iges [options] FILE.dwg [FILE2.dxf ...]

Writes FILE.igs beside each drawing (or the path given with -o, for one input).
  -o, --output PATH   output file (one input only)
  --tol MM            chord tolerance of the trimming edges, in mm (default 0.01)
  --no-pause          on Windows, do not wait for Enter at the end
  -h, --help          this text

Supported: solids made of planes, cylinders and cones (tubes, pipes, plates, holes,
saddle cuts). Other faces are reported and left out.";

struct Opts {
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    tol: f64,
    pause: bool,
}

fn opts() -> Result<Opts, String> {
    let mut o = Opts { inputs: Vec::new(), output: None, tol: 0.01, pause: cfg!(windows) };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => return Err(HELP.into()),
            "-o" | "--output" => o.output = Some(args.next().ok_or("-o needs a path")?.into()),
            "--tol" => {
                o.tol = args.next().and_then(|t| t.replace(',', ".").parse::<f64>().ok()).filter(|t| *t >= 1e-5 && *t <= 10.0).ok_or("--tol needs a number between 0.00001 and 10 (mm)")?;
            }
            "--no-pause" => o.pause = false,
            _ => o.inputs.push(a.into()),
        }
    }
    if o.inputs.is_empty() {
        return Err(HELP.into());
    }
    if o.output.is_some() && o.inputs.len() > 1 {
        return Err("-o takes one input file".into());
    }
    Ok(o)
}

/// The SAT text of every 3D solid and body in model space.
fn solids(path: &Path) -> Result<Vec<String>, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "sat" {
        return std::fs::read_to_string(path).map(|t| vec![t]).map_err(|e| e.to_string());
    }
    let doc = match ext.as_str() {
        "dxf" => acadrust::io::dxf::DxfReader::from_file(path).and_then(|r| r.read()).map_err(|e| format!("cannot read the DXF: {e}"))?,
        _ => {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            acadrust::io::dwg::DwgReader::from_stream(std::io::Cursor::new(bytes)).read().map_err(|e| format!("cannot read the DWG: {e}"))?
        }
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

fn convert(input: &Path, output: &Path, tol: f64) -> Result<bool, String> {
    let texts = solids(input)?;
    if texts.is_empty() {
        return Err("no 3D solids found in model space".into());
    }
    let mut w = iges::Iges::new();
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    let (mut skipped, mut vgap, mut sgap) = (Vec::new(), 0.0f64, 0.0f64);
    for (i, t) in texts.iter().enumerate() {
        let sat = match sat::parse(t) {
            Ok(s) => s,
            Err(e) => {
                skipped.push(format!("solid {}: {e}", i + 1));
                continue;
            }
        };
        let (faces, rep) = brep::faces(&sat, tol);
        vgap = vgap.max(rep.vertex_gap);
        sgap = sgap.max(rep.surface_gap);
        skipped.extend(rep.skipped.into_iter().map(|e| format!("solid {}: {e}", i + 1)));
        for f in &faces {
            match trim::trim(f) {
                Ok(t) => {
                    *counts.entry(t.kind).or_default() += 1;
                    w.trimmed(&t);
                }
                Err(e) => skipped.push(format!("solid {}: {e}", i + 1)),
            }
        }
    }
    let total: usize = counts.values().sum();
    if total == 0 {
        return Err(format!("no face could be converted: {}", skipped.join("; ")));
    }
    let name = output.file_name().and_then(|n| n.to_str()).unwrap_or("out.igs");
    let source = input.file_name().and_then(|n| n.to_str()).unwrap_or("drawing");
    // UTC, from the system clock (the IGES date field has no time zone).
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let date = chrono::DateTime::from_timestamp(i64::try_from(secs).unwrap_or(0), 0).map(|d| d.format("%Y%m%d.%H%M%S").to_string()).unwrap_or_default();
    std::fs::write(output, w.finish(name, &date, source)).map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    let kinds: Vec<String> = counts.iter().map(|(k, n)| format!("{n} {k}")).collect();
    println!("  {} solid(s), {total} faces ({})", texts.len(), kinds.join(", "));
    println!("  edges on their surfaces within {sgap:.4} mm, on their vertices within {vgap:.4} mm");
    for s in &skipped {
        println!("  WARNING: left out: {s}");
    }
    println!("  -> {}", output.display());
    Ok(skipped.is_empty())
}

fn main() {
    let o = match opts() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let mut code = 0;
    for input in &o.inputs {
        let output = o.output.clone().unwrap_or_else(|| input.with_extension("igs"));
        println!("{}", input.display());
        match convert(input, &output, o.tol) {
            Ok(true) => {}
            Ok(false) => code = code.max(1),
            Err(e) => {
                println!("  ERROR: {e}");
                code = 2;
            }
        }
    }
    if o.pause {
        println!("\nPress Enter to close.");
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }
    std::process::exit(code);
}
