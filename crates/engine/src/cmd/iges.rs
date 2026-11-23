//! IGESEXPORT: the 3D solids of the drawing's file as an IGES 5.3 file of trimmed surfaces, for
//! CAM software (tube laser, CNC) that imports IGES. CADCraft keeps solids as opaque data, so
//! the solids are read from the file on disk (the drawing as last saved, or `source`).

use serde_json::{Value, json};

use super::{CommandSpec, bad, f64_or, str_param};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("igesexport", "Export IGES (3D Solids)", run)
            .menu(&["File", "Export IGES (3D Solids)"])
            .alias(&["iges", "esportaiges"])
            .params(
                "{path?: output .igs (default: beside the drawing's file), source?: .dwg/.dxf/.sat file (default: the drawing's file as saved), \
                 tolerance?: mm chord tolerance of the edges (default 0.01)} → {path, solids, faces, warnings, message}",
            )
            .noundo(),
    ]
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    if !cadcraft_iges::READS_FILES {
        return Err(bad("igesexport", "IGES export is not available in this build"));
    }
    let (source, dirty) = match str_param(p, "source") {
        Some(src) => (src.to_string(), false),
        None => {
            let st = s.state()?;
            let path = st.path.clone().ok_or_else(|| bad("igesexport", "save the drawing as DWG or DXF first, or give `source`"))?;
            (path, !std::sync::Arc::ptr_eq(&st.doc, &st.saved))
        }
    };
    let src = std::path::Path::new(&source);
    let out = match str_param(p, "path") {
        Some(o) => o.to_string(),
        None => src.with_extension("igs").to_string_lossy().into_owned(),
    };
    let name = std::path::Path::new(&out).file_name().and_then(|n| n.to_str()).unwrap_or("out.igs").to_string();
    let tol = f64_or(p, "tolerance", cadcraft_iges::DEFAULT_TOL);
    let c = convert(src, &name, tol)?;
    super::exchange::write_file("igesexport", &out, c.iges.as_bytes())?;
    let mut msg = format!("{}\nIGES written to {out}", c.summary());
    if dirty {
        msg.push_str("\n(solids read from the drawing as last saved)");
    }
    Ok(json!({
        "path": out,
        "solids": c.solids,
        "faces": c.faces,
        "warnings": c.warnings,
        "message": msg,
    }))
}

#[cfg(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))))]
fn convert(src: &std::path::Path, name: &str, tol: f64) -> Result<cadcraft_iges::Conversion> {
    cadcraft_iges::convert_file(src, name, tol).map_err(|e| bad("igesexport", e))
}

#[cfg(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64")))]
fn convert(_src: &std::path::Path, _name: &str, _tol: f64) -> Result<cadcraft_iges::Conversion> {
    Err(bad("igesexport", "IGES export is not available in this build"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    /// A tube (two end faces, outer and inner cylinder) as SAT text.
    fn tube_sat() -> String {
        let r = |n: &str| format!("{n} #\n");
        let mut t = String::from("700 0 1 0\n@4 test\n1 1e-06 1e-10\n");
        // 0 body, 1 lump, 2 shell, faces 3..6, loops 7..14, surfaces 15..18, coedges 19..26,
        // edges 27..30, vertices 31..34, points 35..38, circles 39..42.
        t += &r("body $-1 -1 $-1 $1 $-1 $-1");
        t += &r("lump $-1 -1 $-1 $-1 $2 $0");
        t += &r("shell $-1 -1 $-1 $-1 $-1 $3 $-1 $1");
        // Bottom plane (z = 0), top plane (z = 100), outer cylinder r 30, inner cylinder r 26.
        t += &r("face $-1 -1 $-1 $4 $7 $2 $-1 $15 reversed single");
        t += &r("face $-1 -1 $-1 $5 $9 $2 $-1 $16 forward single");
        t += &r("face $-1 -1 $-1 $6 $11 $2 $-1 $17 forward single");
        t += &r("face $-1 -1 $-1 $-1 $13 $2 $-1 $18 reversed single");
        // Loops: each face has two (one per circle at each end).
        let loops = [(8, 19, 3), (-1, 20, 3), (10, 21, 4), (-1, 22, 4), (12, 23, 5), (-1, 24, 5), (14, 25, 6), (-1, 26, 6)];
        for (next, ce, face) in loops {
            t += &r(&format!("loop $-1 -1 $-1 ${next} ${ce} ${face}"));
        }
        t += &r("plane-surface $-1 -1 $-1 0 0 0 0 0 1 1 0 0 forward_v I I I I");
        t += &r("plane-surface $-1 -1 $-1 0 0 100 0 0 1 1 0 0 forward_v I I I I");
        t += &r("cone-surface $-1 -1 $-1 0 0 0 0 0 1 30 0 0 1 I I 0 1 30 forward I I I I");
        t += &r("cone-surface $-1 -1 $-1 0 0 0 0 0 1 26 0 0 1 I I 0 1 26 forward I I I I");
        // Coedges: (edge, loop, partner) — bottom outer 27, bottom inner 28, top outer 29, top inner 30.
        let co = [(27, 7, 23), (28, 8, 25), (29, 9, 24), (30, 10, 26), (27, 11, 19), (29, 12, 21), (28, 13, 20), (30, 14, 22)];
        for (edge, lp, partner) in co {
            let me = 19 + co.iter().position(|c| c.1 == lp).unwrap_or(0);
            t += &r(&format!("coedge $-1 -1 $-1 ${me} ${me} ${partner} ${edge} forward ${lp} $-1"));
        }
        for k in 0..4 {
            t += &r(&format!("edge $-1 -1 $-1 ${} 0 ${} 6.283185307179586 $-1 ${} forward @7 unknown", 31 + k, 31 + k, 39 + k));
        }
        for k in 0..4 {
            t += &r(&format!("vertex $-1 -1 $-1 ${} ${}", 27 + k, 35 + k));
        }
        for (rad, z) in [(30.0, 0.0), (26.0, 0.0), (30.0, 100.0), (26.0, 100.0)] {
            t += &r(&format!("point $-1 -1 $-1 {rad} 0 {z}"));
        }
        for (rad, z) in [(30.0, 0.0), (26.0, 0.0), (30.0, 100.0), (26.0, 100.0)] {
            t += &r(&format!("ellipse-curve $-1 -1 $-1 0 0 {z} 0 0 1 {rad} 0 0 1 I I"));
        }
        t + "End-of-ACIS-data\n"
    }

    #[test]
    fn igesexport_writes_a_tube() {
        let dir = std::env::temp_dir().join(format!("cadcraft-iges-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("tube.sat");
        std::fs::write(&src, tube_sat()).unwrap();
        let out = dir.join("tube.igs");
        let mut s = Session::new();
        let r = s.execute("igesexport", &json!({"source": src.to_string_lossy(), "path": out.to_string_lossy()})).unwrap();
        assert_eq!(r["faces"]["plane"], 2, "{r}");
        assert_eq!(r["faces"]["cylinder"], 2, "{r}");
        assert!(r["warnings"].as_array().unwrap().is_empty(), "{r}");
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.lines().all(|l| l.len() == 80), "fixed 80-column records");
        // Two Directory Entry lines per trimmed surface, one per face.
        assert_eq!(text.lines().filter(|l| l.starts_with("     144") && l.as_bytes().get(72) == Some(&b'D')).count(), 8);
        // Without a saved file and without `source` the command explains what it needs.
        assert!(Session::new().execute("igesexport", &json!({})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
