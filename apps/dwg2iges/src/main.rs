//! dwg2iges: writes the 3D solids of DWG and DXF drawings as IGES 5.3 trimmed surfaces, for
//! CAM software (tube laser nesting, CNC) that imports IGES. The same conversion is CADCraft's
//! IGESEXPORT command.
//!
//! Usage: `dwg2iges [options] FILE.dwg [FILE2.dxf …]` writes `FILE.igs` beside each input.
//! Dragging files onto the program on Windows does the same.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::PathBuf;

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
    let mut o = Opts { inputs: Vec::new(), output: None, tol: cadcraft_iges::DEFAULT_TOL, pause: cfg!(windows) };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => return Err(HELP.into()),
            "-o" | "--output" => o.output = Some(args.next().ok_or("-o needs a path")?.into()),
            "--tol" => {
                o.tol = args
                    .next()
                    .and_then(|t| t.replace(',', ".").parse::<f64>().ok())
                    .filter(|t| (1e-5..=10.0).contains(t))
                    .ok_or("--tol needs a number between 0.00001 and 10 (mm)")?;
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
        let name = output.file_name().and_then(|n| n.to_str()).unwrap_or("out.igs").to_string();
        match cadcraft_iges::convert_file(input, &name, o.tol)
            .and_then(|c| std::fs::write(&output, &c.iges).map(|()| c).map_err(|e| format!("cannot write {}: {e}", output.display())))
        {
            Ok(c) => {
                for l in c.summary().lines() {
                    println!("  {l}");
                }
                println!("  -> {}", output.display());
                if !c.warnings.is_empty() {
                    code = code.max(1);
                }
            }
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
