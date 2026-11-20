use serde_json::json;

use super::catalog::{Shape, parse};
use super::section::{outline, props};
use super::*;
use crate::Session;

fn mass(d: &str) -> f64 {
    let p = parse(d, false).unwrap();
    props(&outline(&p).unwrap()).mass
}

fn close(got: f64, want: f64, tol_pc: f64) -> bool {
    ((got - want) / want).abs() * 100.0 <= tol_pc
}

/// The masses computed from the drawn outlines match the published nominal masses per metre.
#[test]
fn masses_match_published_tables() {
    let cases = [
        ("IPE100", 8.1),
        ("IPE200", 22.4),
        ("IPE300", 42.2),
        ("IPE600", 122.0),
        ("HEA200", 42.3),
        ("HEA300", 88.3),
        ("HEB200", 61.3),
        ("HEB300", 117.0),
        ("HEM200", 103.0),
        ("UPN100", 10.6),
        ("UPN200", 25.3),
        ("UPN300", 46.2),
        ("UPE200", 22.8),
        ("L50x5", 3.77),
        ("L100x10", 15.0),
        ("CHS60.3x3.2", 4.51),
        ("FL50x5", 1.96),
        ("RD20", 2.47),
        ("SQ20", 3.15),
    ];
    for (d, want) in cases {
        let got = mass(d);
        assert!(close(got, want, 2.0), "{d}: {got:.3} kg/m, table {want}");
    }
}

#[test]
fn second_moments_match_published_tables() {
    let p = parse("IPE200", false).unwrap();
    let pr = props(&outline(&p).unwrap());
    assert!(close(pr.iy / 1e4, 1943.0, 1.0), "IPE200 Iy {}", pr.iy / 1e4);
    assert!(close(pr.iz / 1e4, 142.4, 1.5), "IPE200 Iz {}", pr.iz / 1e4);
    assert!(close(pr.wy / 1e3, 194.3, 1.0), "IPE200 Wy {}", pr.wy / 1e3);
    let p = parse("HEB200", false).unwrap();
    let pr = props(&outline(&p).unwrap());
    assert!(close(pr.iy / 1e4, 5696.0, 1.0), "HEB200 Iy {}", pr.iy / 1e4);
    assert!(close(pr.iz / 1e4, 2003.0, 1.0), "HEB200 Iz {}", pr.iz / 1e4);
    let p = parse("UPN200", false).unwrap();
    let pr = props(&outline(&p).unwrap());
    assert!(close(pr.iy / 1e4, 1910.0, 2.0), "UPN200 Iy {}", pr.iy / 1e4);
    assert!(close(pr.cx, 20.1, 3.0), "UPN200 centroid {}", pr.cx);
}

#[test]
fn designations_are_forgiving() {
    for d in ["ipe 200", "IPE200", " Ipe200 "] {
        assert_eq!(parse(d, false).unwrap().name, "IPE200");
    }
    assert_eq!(parse("tr 100 x 50 x 4", false).unwrap().name, "RHS100x50x4");
    assert_eq!(parse("TQ40*3", false).unwrap().name, "SHS40x3");
    assert_eq!(parse("chs 60,3x3,2", false).unwrap().name, "CHS60.3x3.2");
    assert_eq!(parse("piatto 5x50", false).unwrap().shape, Shape::Flat { b: 50.0, t: 5.0 });
    assert!(parse("IPE210", false).is_err());
    assert!(parse("ZZZ", false).is_err());
    assert!(parse("L50x30", false).is_err());
    assert!(parse("RHS10x10x6", false).is_err());
    assert!(parse("CHS1e308x1", false).is_err());
    assert!(parse("SHSnanx1", false).is_err());
    assert!(parse("", false).is_err());
}

#[test]
fn every_listed_size_builds() {
    for (_, _, sizes) in catalog::families() {
        for d in sizes {
            let p = parse(&d, false).unwrap_or_else(|e| panic!("{d}: {e}"));
            let pr = props(&outline(&p).unwrap_or_else(|e| panic!("{d}: {e}")));
            assert!(pr.area > 0.0 && pr.iy > 0.0 && pr.iz > 0.0, "{d}");
        }
    }
}

#[test]
fn steelsection_draws_outlines_and_label() {
    let mut s = Session::new();
    let r = s.execute("steelsection", &json!({"profile": "RHS100x50x4", "at": [0, 0], "label": true})).unwrap();
    assert_eq!(r["handles"].as_array().unwrap().len(), 3, "outer, inner, label");
    assert!(close(r["properties"]["mass_kg_m"].as_f64().unwrap(), 8.59, 3.0), "{r}");
    let r = s.execute("steelsection", &json!({"profile": "CHS48.3x3.2", "at": [200, 0], "rotation": 30})).unwrap();
    assert_eq!(r["handles"].as_array().unwrap().len(), 2);
    assert!(s.execute("steelsection", &json!({"profile": "IPE201", "at": [0, 0]})).is_err());
    assert!(s.execute("steelsection", &json!({"profile": "IPE200"})).is_err());
}

#[test]
fn steelsection_at_the_command_line() {
    let mut s = Session::new();
    let before = s.doc().unwrap().entity_count();
    s.cmdline("steelsection").unwrap();
    s.cmdline("heb200").unwrap();
    s.cmdline("0,0").unwrap();
    assert_eq!(s.doc().unwrap().entity_count(), before + 2, "outline and label");
    assert_eq!(s.last_used.steel_profile, "HEB200");
    assert!(s.log.iter().any(|l| l.contains("61.")), "{:?}", s.log);
}

#[test]
fn steelinfo_weights_and_cost() {
    let mut s = Session::new();
    let r = s.execute("steelinfo", &json!({"profile": "IPE200", "length": 6000, "quantity": 10, "pricePerKg": 1.2})).unwrap();
    let kg = r["totalWeight_kg"].as_f64().unwrap();
    assert!(close(kg, 22.4 * 60.0, 2.0), "{r}");
    assert!(close(r["cost"].as_f64().unwrap(), kg * 1.2, 0.1));
    assert!(s.execute("steelinfo", &json!({"profile": "IPE200", "length": -1})).is_err());
}

#[test]
fn cutlist_nests_pieces() {
    let mut s = Session::new();
    let r = s
        .execute(
            "cutlist",
            &json!({"pieces": [{"length": 2900, "quantity": 2}, {"length": 1500, "quantity": 4}, 3000], "stock": 6000, "kerf": 3, "profile": "SHS40x3"}),
        )
        .unwrap();
    // 2900+2900 (+kerfs) fit one bar, 3000 + 1500 + 1500 the next, then 1500 + 1500.
    assert_eq!(r["barCount"], 3, "{r}");
    assert_eq!(r["pieceCount"], 7);
    assert!(r["grossWeight_kg"].as_f64().unwrap() > r["netWeight_kg"].as_f64().unwrap());
    // Exact fit: two 3000 pieces in a 6000 bar, the last without a cut after it.
    let r = s.execute("cutlist", &json!({"pieces": [{"length": 2998.5, "quantity": 2}], "stock": 6000, "kerf": 3})).unwrap();
    assert_eq!(r["barCount"], 1, "{r}");
    assert!(s.execute("cutlist", &json!({"pieces": [7000]})).is_err());
    assert!(s.execute("cutlist", &json!({"pieces": [{"length": 10, "quantity": 1_000_000_000u64}]})).is_err());
    assert!(s.execute("cutlist", &json!({"pieces": []})).is_err());
    let n = s.doc().unwrap().entity_count();
    let r = s.execute("cutlist", &json!({"pieces": [1000, 2000], "at": [0, -500]})).unwrap();
    assert!(s.doc().unwrap().entity_count() > n, "{r}");
}

#[test]
fn cutlist_at_the_command_line() {
    let mut s = Session::new();
    s.cmdline("cutlist").unwrap();
    s.cmdline("6000").unwrap();
    s.cmdline("3").unwrap();
    s.cmdline("1250*4").unwrap();
    s.cmdline("2000").unwrap();
    s.cmdline("").unwrap();
    assert!(s.log.iter().any(|l| l.contains("5 pieces") && l.contains("2 bars")), "{:?}", s.log);
}

#[test]
fn beamcheck_reports_and_suggests() {
    let mut s = Session::new();
    // IPE200, 4 m simply supported, 10 kN/m: M ≈ 20 kNm, σ ≈ 104 MPa, f ≈ 8.3 mm.
    let r = s.execute("beamcheck", &json!({"profile": "IPE200", "span": 4000, "q": 10, "selfWeight": false})).unwrap();
    assert!(close(r["M_kNm"].as_f64().unwrap(), 20.0, 0.1), "{r}");
    assert!(close(r["sigma_MPa"].as_f64().unwrap(), 103.0, 2.0), "{r}");
    assert!(close(r["deflection_mm"].as_f64().unwrap(), 8.17, 2.0), "{r}");
    assert_eq!(r["ok"], true);
    let r = s.execute("beamcheck", &json!({"profile": "IPE100", "span": 6000, "q": 15, "suggest": true})).unwrap();
    assert_eq!(r["ok"], false);
    let sug = r["suggested"].as_str().unwrap();
    assert!(sug.starts_with("IPE") && sug != "IPE100", "{r}");
    assert!(s.execute("beamcheck", &json!({"profile": "IPE200"})).is_err());
    assert!(s.execute("beamcheck", &json!({"profile": "IPE200", "span": 4000, "grade": "S999"})).is_err());
}

#[test]
fn hostile_input_does_not_panic() {
    let mut s = Session::new();
    for p in [
        json!({"profile": "L1e9x1e9x1"}),
        json!({"profile": "RHS5000x5000x2499"}),
        json!({"profile": "CHS0.0001x0.00001", "at": [0, 0]}),
        json!({"profile": "SQ1e-300", "at": [0, 0]}),
        json!({"profile": 7, "at": [0, 0]}),
        json!({"profile": "x".repeat(10_000), "at": [0, 0]}),
    ] {
        let _ = s.execute("steelsection", &p);
        let _ = s.execute("steelinfo", &p);
        let _ = s.execute("beamcheck", &json!({"profile": p["profile"], "span": 1e300}));
    }
    let _ = s.execute("cutlist", &json!({"pieces": [{"length": 1e-300, "quantity": 5}], "stock": 1e300}));
    let _ = s.execute("cutlist", &json!({"pieces": "nope"}));
}

/// The command-line examples in docs/steel.md work as written.
#[test]
fn documented_command_lines_work() {
    let mut s = Session::new();
    let n = s.doc().unwrap().entity_count();
    s.cmdline("steelsection heb200 0,0").unwrap();
    s.cmdline("steelsection l80x8 r 90 500,0").unwrap();
    assert_eq!(s.doc().unwrap().entity_count(), n + 4, "{:?}", s.log);
    s.cmdline("steelinfo ipe240 6000*12").unwrap();
    s.cmdline("").unwrap();
    assert!(s.log.iter().any(|l| l.starts_with("12 x 6000 mm")), "{:?}", s.log);
    s.cmdline("cutlist 6000 3 1250*4 2400*2").unwrap();
    s.cmdline("").unwrap();
    assert!(s.log.iter().any(|l| l.contains("6 pieces")), "{:?}", s.log);
    s.cmdline("beamcheck ipe200 5000 8 0").unwrap();
    assert!(s.log.iter().any(|l| l.contains("deflection =")), "{:?}", s.log);
    assert!(s.log.iter().any(|l| l.contains("lightest IPE")), "{:?}", s.log);
}

/// Scripts split the same inputs on spaces (a free-text prompt would take the rest of the line).
#[test]
fn documented_lines_work_in_scripts() {
    let mut s = Session::new();
    let n = s.doc().unwrap().entity_count();
    s.script("steelsection upn200 560,0\nsteelsection l100x10 760,0\ncutlist 6000 3 1250*4 2400*2\n\n").unwrap();
    assert_eq!(s.doc().unwrap().entity_count(), n + 4, "{:?}", s.log);
    assert!(s.log.iter().any(|l| l.contains("6 pieces")), "{:?}", s.log);
}
