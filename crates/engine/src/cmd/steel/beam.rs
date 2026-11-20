//! A quick elastic check of a single-span steel beam: bending stress, shear and deflection
//! against the steel grade's yield strength and a deflection limit. A sizing aid, not a
//! structural design: lateral-torsional buckling, local buckling and load combinations are the
//! engineer's.

use super::catalog::Shape;
use super::section::Props;

/// Young's modulus of steel, N/mm².
pub const E: f64 = 210_000.0;
/// Partial factor for the cross-section resistance (EN 1993-1-1 recommended value).
pub const GAMMA_M0: f64 = 1.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    Simple,
    Cantilever,
    Fixed,
}

impl Support {
    pub fn parse(s: &str) -> Option<Support> {
        match s.trim().to_ascii_lowercase().as_str() {
            "simple" | "s" | "appoggiata" | "pinned" => Some(Support::Simple),
            "cantilever" | "c" | "mensola" | "sbalzo" => Some(Support::Cantilever),
            "fixed" | "f" | "incastrata" | "built-in" => Some(Support::Fixed),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Support::Simple => "simply supported",
            Support::Cantilever => "cantilever",
            Support::Fixed => "fixed at both ends",
        }
    }
}

/// Yield strength (N/mm², thickness ≤ 40 mm) of a grade name such as `S275` or `S355JR`.
pub fn yield_strength(grade: &str) -> Option<f64> {
    let g = grade.trim().to_ascii_uppercase();
    let digits: String = g.trim_start_matches('S').chars().take_while(char::is_ascii_digit).collect();
    match digits.as_str() {
        "235" => Some(235.0),
        "275" => Some(275.0),
        "355" => Some(355.0),
        "420" => Some(420.0),
        "460" => Some(460.0),
        _ => None,
    }
}

/// Shear area (mm²) for a load along the section's vertical axis.
pub fn shear_area(shape: &Shape, area: f64) -> f64 {
    match *shape {
        Shape::I { h, tw, .. } | Shape::Channel { h, tw, .. } => h * tw,
        Shape::Rhs { h, b, .. } => area * h / (h + b),
        Shape::Chs { .. } => 2.0 * area / std::f64::consts::PI,
        Shape::Angle { a, t, .. } => a * t,
        Shape::Flat { .. } | Shape::Round { .. } | Shape::Square { .. } => area,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    /// Span (mm).
    pub span: f64,
    pub support: Support,
    /// Uniform load including self-weight (N/mm = kN/m).
    pub q: f64,
    /// Point load at mid-span (at the tip for a cantilever), N.
    pub p: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Check {
    /// Largest bending moment, N·mm.
    pub m: f64,
    /// Largest shear, N.
    pub v: f64,
    /// Bending stress, N/mm².
    pub sigma: f64,
    /// Mean shear stress, N/mm².
    pub tau: f64,
    /// Largest deflection, mm.
    pub deflection: f64,
    pub bending_ratio: f64,
    pub shear_ratio: f64,
}

/// Moments, shear and deflection for the load; stresses from the moduli about the bending axis.
pub fn check(load: &Load, i: f64, w: f64, av: f64, fy: f64) -> Check {
    let (l, q, p) = (load.span, load.q, load.p);
    let (m, v, d) = match load.support {
        Support::Simple => {
            (q * l * l / 8.0 + p * l / 4.0, q * l / 2.0 + p / 2.0, 5.0 * q * l.powi(4) / (384.0 * E * i) + p * l.powi(3) / (48.0 * E * i))
        }
        Support::Cantilever => (q * l * l / 2.0 + p * l, q * l + p, q * l.powi(4) / (8.0 * E * i) + p * l.powi(3) / (3.0 * E * i)),
        Support::Fixed => (q * l * l / 12.0 + p * l / 8.0, q * l / 2.0 + p / 2.0, q * l.powi(4) / (384.0 * E * i) + p * l.powi(3) / (192.0 * E * i)),
    };
    let sigma = if w > 0.0 { m / w } else { f64::INFINITY };
    let tau = if av > 0.0 { v / av } else { f64::INFINITY };
    Check { m, v, sigma, tau, deflection: d, bending_ratio: sigma / (fy / GAMMA_M0), shear_ratio: tau / (fy / (3f64.sqrt() * GAMMA_M0)) }
}

/// Second moment and elastic modulus about the bending axis: `strong` bends about the
/// horizontal axis (load vertical on an upright section).
pub fn axis(pr: &Props, strong: bool) -> (f64, f64) {
    if strong { (pr.iy, pr.wy) } else { (pr.iz, pr.wz) }
}
