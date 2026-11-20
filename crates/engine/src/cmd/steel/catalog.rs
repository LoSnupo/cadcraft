//! Steel section catalogue: designations such as `IPE200`, `HEB 240`, `UPN100`, `L50x5`,
//! `RHS 100x50x4`, `CHS 60.3x3.2`, `FL 50x5`, `RD 20` and `SQ 20`, parsed into dimensions.
//!
//! Rolled I, H and channel dimensions are the nominal values published in the European product
//! standards (EN 10365 for IPE, HEA, HEB, HEM, UPN and UPE). Angles, hollow sections and bars
//! are parametric: any size can be asked for, with the corner radii the product standards give
//! (EN 10056-1 root radii by leg size, EN 10219-2 / EN 10210-2 corner radii by thickness).

/// The cross-section geometry of a profile, in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// Doubly symmetric I or H section: height, flange width, web and flange thickness, root radius.
    I { h: f64, b: f64, tw: f64, tf: f64, r: f64 },
    /// Channel with its web on the left. `taper` is the inner-flange slope (0.08 for UPN, 0 for
    /// UPE); `tf` is measured half-way along the flange. `r2` rounds the toes.
    Channel { h: f64, b: f64, tw: f64, tf: f64, r1: f64, r2: f64, taper: f64 },
    /// Angle: vertical leg `a`, horizontal leg `b`, thickness `t`, root radius `r1`, toe radius `r2`.
    Angle { a: f64, b: f64, t: f64, r1: f64, r2: f64 },
    /// Rectangular or square hollow section: height, width, wall, outer and inner corner radii.
    Rhs { h: f64, b: f64, t: f64, ro: f64, ri: f64 },
    /// Circular hollow section: outside diameter and wall.
    Chs { d: f64, t: f64 },
    /// Flat bar: width and thickness (drawn with the width horizontal).
    Flat { b: f64, t: f64 },
    /// Round bar: diameter.
    Round { d: f64 },
    /// Square bar: side.
    Square { a: f64 },
}

/// A parsed profile: its normalised designation, family and geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub family: &'static str,
    pub shape: Shape,
}

/// The largest dimension accepted for a parametric profile (mm): beyond this a typing error is
/// far more likely than a real section, and it keeps generated geometry bounded.
const MAX_DIM: f64 = 5000.0;

/// IPE: h, b, tw, tf, r (EN 10365).
const IPE: &[(u32, [f64; 5])] = &[
    (80, [80.0, 46.0, 3.8, 5.2, 5.0]),
    (100, [100.0, 55.0, 4.1, 5.7, 7.0]),
    (120, [120.0, 64.0, 4.4, 6.3, 7.0]),
    (140, [140.0, 73.0, 4.7, 6.9, 7.0]),
    (160, [160.0, 82.0, 5.0, 7.4, 9.0]),
    (180, [180.0, 91.0, 5.3, 8.0, 9.0]),
    (200, [200.0, 100.0, 5.6, 8.5, 12.0]),
    (220, [220.0, 110.0, 5.9, 9.2, 12.0]),
    (240, [240.0, 120.0, 6.2, 9.8, 15.0]),
    (270, [270.0, 135.0, 6.6, 10.2, 15.0]),
    (300, [300.0, 150.0, 7.1, 10.7, 15.0]),
    (330, [330.0, 160.0, 7.5, 11.5, 18.0]),
    (360, [360.0, 170.0, 8.0, 12.7, 18.0]),
    (400, [400.0, 180.0, 8.6, 13.5, 21.0]),
    (450, [450.0, 190.0, 9.4, 14.6, 21.0]),
    (500, [500.0, 200.0, 10.2, 16.0, 21.0]),
    (550, [550.0, 210.0, 11.1, 17.2, 24.0]),
    (600, [600.0, 220.0, 12.0, 19.0, 24.0]),
];

/// HEA (IPBl): h, b, tw, tf, r.
const HEA: &[(u32, [f64; 5])] = &[
    (100, [96.0, 100.0, 5.0, 8.0, 12.0]),
    (120, [114.0, 120.0, 5.0, 8.0, 12.0]),
    (140, [133.0, 140.0, 5.5, 8.5, 12.0]),
    (160, [152.0, 160.0, 6.0, 9.0, 15.0]),
    (180, [171.0, 180.0, 6.0, 9.5, 15.0]),
    (200, [190.0, 200.0, 6.5, 10.0, 18.0]),
    (220, [210.0, 220.0, 7.0, 11.0, 18.0]),
    (240, [230.0, 240.0, 7.5, 12.0, 21.0]),
    (260, [250.0, 260.0, 7.5, 12.5, 24.0]),
    (280, [270.0, 280.0, 8.0, 13.0, 24.0]),
    (300, [290.0, 300.0, 8.5, 14.0, 27.0]),
    (320, [310.0, 300.0, 9.0, 15.5, 27.0]),
    (340, [330.0, 300.0, 9.5, 16.5, 27.0]),
    (360, [350.0, 300.0, 10.0, 17.5, 27.0]),
    (400, [390.0, 300.0, 11.0, 19.0, 27.0]),
    (450, [440.0, 300.0, 11.5, 21.0, 27.0]),
    (500, [490.0, 300.0, 12.0, 23.0, 27.0]),
    (550, [540.0, 300.0, 12.5, 24.0, 27.0]),
    (600, [590.0, 300.0, 13.0, 25.0, 27.0]),
];

/// HEB (IPB): h, b, tw, tf, r.
const HEB: &[(u32, [f64; 5])] = &[
    (100, [100.0, 100.0, 6.0, 10.0, 12.0]),
    (120, [120.0, 120.0, 6.5, 11.0, 12.0]),
    (140, [140.0, 140.0, 7.0, 12.0, 12.0]),
    (160, [160.0, 160.0, 8.0, 13.0, 15.0]),
    (180, [180.0, 180.0, 8.5, 14.0, 15.0]),
    (200, [200.0, 200.0, 9.0, 15.0, 18.0]),
    (220, [220.0, 220.0, 9.5, 16.0, 18.0]),
    (240, [240.0, 240.0, 10.0, 17.0, 21.0]),
    (260, [260.0, 260.0, 10.0, 17.5, 24.0]),
    (280, [280.0, 280.0, 10.5, 18.0, 24.0]),
    (300, [300.0, 300.0, 11.0, 19.0, 27.0]),
    (320, [320.0, 300.0, 11.5, 20.5, 27.0]),
    (340, [340.0, 300.0, 12.0, 21.5, 27.0]),
    (360, [360.0, 300.0, 12.5, 22.5, 27.0]),
    (400, [400.0, 300.0, 13.5, 24.0, 27.0]),
    (450, [450.0, 300.0, 14.0, 26.0, 27.0]),
    (500, [500.0, 300.0, 14.5, 28.0, 27.0]),
    (550, [550.0, 300.0, 15.0, 29.0, 27.0]),
    (600, [600.0, 300.0, 15.5, 30.0, 27.0]),
];

/// HEM (IPBv): h, b, tw, tf, r.
const HEM: &[(u32, [f64; 5])] = &[
    (100, [120.0, 106.0, 12.0, 20.0, 12.0]),
    (120, [140.0, 126.0, 12.5, 21.0, 12.0]),
    (140, [160.0, 146.0, 13.0, 22.0, 12.0]),
    (160, [180.0, 166.0, 14.0, 23.0, 15.0]),
    (180, [200.0, 186.0, 14.5, 24.0, 15.0]),
    (200, [220.0, 206.0, 15.0, 25.0, 18.0]),
    (220, [240.0, 226.0, 15.5, 26.0, 18.0]),
    (240, [270.0, 248.0, 18.0, 32.0, 21.0]),
    (260, [290.0, 268.0, 18.0, 32.5, 24.0]),
    (280, [310.0, 288.0, 18.5, 33.0, 24.0]),
    (300, [340.0, 310.0, 21.0, 39.0, 27.0]),
];

/// UPN (taper 8 %): h, b, tw, tf, r1, r2.
const UPN: &[(u32, [f64; 6])] = &[
    (80, [80.0, 45.0, 6.0, 8.0, 8.0, 4.0]),
    (100, [100.0, 50.0, 6.0, 8.5, 8.5, 4.5]),
    (120, [120.0, 55.0, 7.0, 9.0, 9.0, 4.5]),
    (140, [140.0, 60.0, 7.0, 10.0, 10.0, 5.0]),
    (160, [160.0, 65.0, 7.5, 10.5, 10.5, 5.5]),
    (180, [180.0, 70.0, 8.0, 11.0, 11.0, 5.5]),
    (200, [200.0, 75.0, 8.5, 11.5, 11.5, 6.0]),
    (220, [220.0, 80.0, 9.0, 12.5, 12.5, 6.5]),
    (240, [240.0, 85.0, 9.5, 13.0, 13.0, 6.5]),
    (260, [260.0, 90.0, 10.0, 14.0, 14.0, 7.0]),
    (280, [280.0, 95.0, 10.0, 15.0, 15.0, 7.5]),
    (300, [300.0, 100.0, 10.0, 16.0, 16.0, 8.0]),
];

/// UPE (parallel flanges): h, b, tw, tf, r.
const UPE: &[(u32, [f64; 5])] = &[
    (80, [80.0, 50.0, 4.0, 7.0, 10.0]),
    (100, [100.0, 55.0, 4.5, 7.5, 10.0]),
    (120, [120.0, 60.0, 5.0, 8.0, 12.0]),
    (140, [140.0, 65.0, 5.0, 9.0, 12.0]),
    (160, [160.0, 70.0, 5.5, 9.5, 12.0]),
    (180, [180.0, 75.0, 5.5, 10.5, 12.0]),
    (200, [200.0, 80.0, 6.0, 11.0, 13.0]),
    (220, [220.0, 85.0, 6.5, 12.0, 13.0]),
    (240, [240.0, 90.0, 7.0, 12.5, 15.0]),
    (270, [270.0, 95.0, 7.5, 13.5, 15.0]),
    (300, [300.0, 100.0, 9.5, 15.0, 15.0]),
];

/// Angle root radius by the longer leg (EN 10056-1); the toe radius is half of it.
const ANGLE_R1: &[(f64, f64)] = &[
    (20.0, 3.5),
    (25.0, 3.5),
    (30.0, 5.0),
    (35.0, 5.0),
    (40.0, 6.0),
    (45.0, 7.0),
    (50.0, 7.0),
    (60.0, 8.0),
    (70.0, 9.0),
    (80.0, 10.0),
    (90.0, 11.0),
    (100.0, 12.0),
    (120.0, 13.0),
    (150.0, 16.0),
    (180.0, 18.0),
];

/// The families and the sizes a family lists (parametric families list typical sizes).
pub fn families() -> Vec<(&'static str, &'static str, Vec<String>)> {
    let sizes = |t: &[(u32, [f64; 5])], f: &str| t.iter().map(|(n, _)| format!("{f}{n}")).collect::<Vec<_>>();
    vec![
        ("IPE", "I-beam, parallel flanges (EN 10365)", sizes(IPE, "IPE")),
        ("HEA", "Wide flange H-beam, light (EN 10365)", sizes(HEA, "HEA")),
        ("HEB", "Wide flange H-beam (EN 10365)", sizes(HEB, "HEB")),
        ("HEM", "Wide flange H-beam, heavy (EN 10365)", sizes(HEM, "HEM")),
        ("UPN", "Channel, tapered flanges (EN 10365)", UPN.iter().map(|(n, _)| format!("UPN{n}")).collect()),
        ("UPE", "Channel, parallel flanges (EN 10365)", sizes(UPE, "UPE")),
        (
            "L",
            "Angle, equal L<a>x<t> or unequal L<a>x<b>x<t> (EN 10056)",
            ["L30x3", "L40x4", "L50x5", "L60x6", "L80x8", "L100x10", "L100x50x8"].map(String::from).to_vec(),
        ),
        ("SHS", "Square hollow section SHS<a>x<t> (alias TQ)", ["SHS40x3", "SHS50x3", "SHS60x3", "SHS80x4", "SHS100x5"].map(String::from).to_vec()),
        (
            "RHS",
            "Rectangular hollow section RHS<h>x<b>x<t> (alias TR)",
            ["RHS60x40x3", "RHS80x40x3", "RHS100x50x4", "RHS120x60x4", "RHS150x100x5"].map(String::from).to_vec(),
        ),
        (
            "CHS",
            "Circular hollow section CHS<d>x<t> (alias TT)",
            ["CHS33.7x2.6", "CHS48.3x3.2", "CHS60.3x3.2", "CHS88.9x4", "CHS114.3x5"].map(String::from).to_vec(),
        ),
        ("FL", "Flat bar FL<b>x<t> (alias PIATTO)", ["FL30x5", "FL40x5", "FL50x5", "FL60x8", "FL100x10"].map(String::from).to_vec()),
        ("RD", "Round bar RD<d> (alias TONDO)", ["RD10", "RD16", "RD20", "RD30", "RD50"].map(String::from).to_vec()),
        ("SQ", "Square bar SQ<a> (alias QUADRO)", ["SQ10", "SQ16", "SQ20", "SQ30", "SQ40"].map(String::from).to_vec()),
    ]
}

/// Family prefixes, longest first so `PIATTO` is not read as `P…` and `HEA` not as `HE`.
const PREFIXES: &[(&str, &str)] = &[
    ("PIATTO", "FL"),
    ("QUADRO", "SQ"),
    ("TONDO", "RD"),
    ("IPE", "IPE"),
    ("HEA", "HEA"),
    ("HEB", "HEB"),
    ("HEM", "HEM"),
    ("UPN", "UPN"),
    ("UPE", "UPE"),
    ("SHS", "SHS"),
    ("RHS", "RHS"),
    ("CHS", "CHS"),
    ("TQ", "SHS"),
    ("TR", "RHS"),
    ("TT", "CHS"),
    ("FL", "FL"),
    ("FB", "FL"),
    ("RD", "RD"),
    ("SQ", "SQ"),
    ("L", "L"),
];

fn table5(t: &[(u32, [f64; 5])], n: u32) -> Option<[f64; 5]> {
    t.iter().find(|(k, _)| *k == n).map(|(_, d)| *d)
}

fn nums(rest: &str) -> Result<Vec<f64>, String> {
    if rest.is_empty() {
        return Err("the size is missing".into());
    }
    let v: Vec<f64> = rest.split('X').map(|t| t.parse::<f64>().map_err(|_| format!("`{t}` is not a number"))).collect::<Result<_, _>>()?;
    if v.iter().any(|x| !x.is_finite() || *x <= 0.0 || *x > MAX_DIM) {
        return Err(format!("sizes must be between 0 and {MAX_DIM} mm"));
    }
    Ok(v)
}

fn fmt_num(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 { format!("{}", x.round() as i64) } else { format!("{x}") }
}

/// Angle root radius for the longer leg.
fn angle_r1(leg: f64) -> f64 {
    ANGLE_R1.iter().rev().find(|(l, _)| leg >= *l - 1e-9).map(|(_, r)| *r).unwrap_or(3.0)
}

/// Hollow-section corner radii: cold formed (EN 10219-2, mid values) or hot finished (EN 10210-2).
pub fn hollow_radii(t: f64, hot: bool) -> (f64, f64) {
    if hot {
        return (1.5 * t, t);
    }
    let ro = if t <= 6.0 {
        2.0 * t
    } else if t <= 10.0 {
        2.5 * t
    } else {
        3.0 * t
    };
    (ro, ro - t)
}

/// Parse a designation. `hot` picks hot-finished corner radii for SHS/RHS.
pub fn parse(designation: &str, hot: bool) -> Result<Profile, String> {
    let norm: String = designation
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '×' | '*' | 'x' => 'X',
            ',' => '.',
            c => c.to_ascii_uppercase(),
        })
        .collect();
    if norm.is_empty() {
        return Err("enter a profile, e.g. IPE200, HEB240, UPN100, L50x5, RHS100x50x4, CHS60.3x3.2".into());
    }
    if norm.len() > 64 {
        return Err("the designation is too long".into());
    }
    let Some((prefix, family)) = PREFIXES.iter().find(|(p, _)| norm.starts_with(p)) else {
        return Err(format!("unknown profile `{designation}`; families: IPE, HEA, HEB, HEM, UPN, UPE, L, SHS, RHS, CHS, FL, RD, SQ"));
    };
    let rest = norm.get(prefix.len()..).unwrap_or("");
    let v = nums(rest)?;
    let n = |i: usize| v.get(i).copied();
    let rolled = |t: &[(u32, [f64; 5])]| -> Result<[f64; 5], String> {
        let size = n(0).filter(|x| v.len() == 1 && (x - x.round()).abs() < 1e-9).map(|x| x.round() as u32);
        size.and_then(|s| table5(t, s)).ok_or_else(|| {
            format!("{family}{rest} is not a standard size; available: {}", t.iter().map(|(k, _)| k.to_string()).collect::<Vec<_>>().join(", "))
        })
    };
    let shape = match *family {
        "IPE" | "HEA" | "HEB" | "HEM" => {
            let t = match *family {
                "IPE" => IPE,
                "HEA" => HEA,
                "HEB" => HEB,
                _ => HEM,
            };
            let [h, b, tw, tf, r] = rolled(t)?;
            Shape::I { h, b, tw, tf, r }
        }
        "UPE" => {
            let [h, b, tw, tf, r] = rolled(UPE)?;
            Shape::Channel { h, b, tw, tf, r1: r, r2: 0.0, taper: 0.0 }
        }
        "UPN" => {
            let size = n(0).filter(|x| v.len() == 1 && (x - x.round()).abs() < 1e-9).map(|x| x.round() as u32);
            let d = size.and_then(|s| UPN.iter().find(|(k, _)| *k == s).map(|(_, d)| *d)).ok_or_else(|| {
                format!("UPN{rest} is not a standard size; available: {}", UPN.iter().map(|(k, _)| k.to_string()).collect::<Vec<_>>().join(", "))
            })?;
            let [h, b, tw, tf, r1, r2] = d;
            Shape::Channel { h, b, tw, tf, r1, r2, taper: 0.08 }
        }
        "L" => {
            let (a, b, t) = match v.as_slice() {
                [a, t] => (*a, *a, *t),
                [a, b, t] => (*a, *b, *t),
                _ => return Err("an angle is L<a>x<t> or L<a>x<b>x<t>".into()),
            };
            if t * 2.0 >= a.min(b) {
                return Err("the angle's thickness must be less than half its shorter leg".into());
            }
            let r1 = angle_r1(a.max(b)).min(t * 1.5);
            Shape::Angle { a, b, t, r1, r2: r1 / 2.0 }
        }
        "SHS" | "RHS" => {
            let (h, b, t) = match v.as_slice() {
                [a, t] => (*a, *a, *t),
                [h, b, t] => (*h, *b, *t),
                _ => return Err(format!("a hollow section is {family}<h>x<b>x<t> or {family}<a>x<t>")),
            };
            if t * 2.0 >= h.min(b) {
                return Err("the wall must be less than half the smaller side".into());
            }
            let (ro, ri) = hollow_radii(t, hot);
            // A small tube with a thick wall: keep the corners inside the sides.
            let ro = ro.min(h.min(b) / 2.0 - 1e-6);
            let ri = ri.min(ro - t).max(0.0);
            Shape::Rhs { h, b, t, ro, ri }
        }
        "CHS" => {
            let [d, t] = v.as_slice() else { return Err("a circular hollow section is CHS<d>x<t>".into()) };
            if t * 2.0 >= *d {
                return Err("the wall must be less than half the diameter".into());
            }
            Shape::Chs { d: *d, t: *t }
        }
        "FL" => {
            let [b, t] = v.as_slice() else { return Err("a flat bar is FL<b>x<t>".into()) };
            Shape::Flat { b: b.max(*t), t: b.min(*t) }
        }
        "RD" => {
            let [d] = v.as_slice() else { return Err("a round bar is RD<d>".into()) };
            Shape::Round { d: *d }
        }
        _ => {
            let [a] = v.as_slice() else { return Err("a square bar is SQ<a>".into()) };
            Shape::Square { a: *a }
        }
    };
    let name = match shape {
        Shape::I { .. } | Shape::Channel { .. } => format!("{family}{}", rest),
        _ => format!("{family}{}", v.iter().map(|x| fmt_num(*x)).collect::<Vec<_>>().join("x")),
    };
    Ok(Profile { name, family, shape })
}
