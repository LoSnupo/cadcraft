//! Section outlines (closed polylines with filleted corners, or circles) and the properties
//! computed from them: area, mass per metre, centroid, second moments, elastic moduli, radii of
//! gyration and paintable surface. Properties come from the drawn geometry itself (Green's
//! theorem over the outline with its arcs finely subdivided), so drawing and numbers agree.

use cadcraft_geom::{PolyVertex, Vec2, bulge_to_arc};
use serde_json::{Value, json};

use super::catalog::{Profile, Shape};

/// Steel density, kg/m³.
pub const DENSITY: f64 = 7850.0;

/// One closed loop of a section. Holes (the inside of a tube) subtract.
#[derive(Clone, Debug, PartialEq)]
pub enum Loop {
    Poly { verts: Vec<PolyVertex>, hole: bool },
    Circle { center: Vec2, r: f64, hole: bool },
}

impl Loop {
    pub fn hole(&self) -> bool {
        match self {
            Loop::Poly { hole, .. } | Loop::Circle { hole, .. } => *hole,
        }
    }
    /// The loop moved by `f` (a rigid transform: translation and rotation).
    pub fn map(&self, f: &dyn Fn(Vec2) -> Vec2) -> Loop {
        match self {
            Loop::Poly { verts, hole } => Loop::Poly { verts: verts.iter().map(|v| PolyVertex { p: f(v.p), ..*v }).collect(), hole: *hole },
            Loop::Circle { center, r, hole } => Loop::Circle { center: f(*center), r: *r, hole: *hole },
        }
    }
}

/// A closed polygon (counter-clockwise corners) with a fillet radius at each corner, as
/// polyline vertices with bulges. A corner turning right (concave, e.g. a root radius) gets a
/// clockwise arc. Errors when a fillet does not fit its edges.
pub fn filleted(corners: &[(Vec2, f64)]) -> Result<Vec<PolyVertex>, String> {
    let n = corners.len();
    let mut out = Vec::with_capacity(n * 2);
    // Tangent distances, checked against the edges afterwards.
    let mut trims = vec![(0.0, 0.0); n];
    for i in 0..n {
        let (p, r) = corners[i];
        let prev = corners[(i + n - 1) % n].0;
        let next = corners[(i + 1) % n].0;
        let (u1, u2) = ((p - prev).normalized(), (next - p).normalized());
        let turn = u1.cross(u2).atan2(u1.dot(u2));
        if r <= 0.0 || turn.abs() < 1e-9 {
            out.push(PolyVertex::new(p));
            continue;
        }
        let d = r * (turn.abs() / 2.0).tan();
        trims[i] = (d, d);
        out.push(PolyVertex::with_bulge(p - u1 * d, (turn / 4.0).tan()));
        out.push(PolyVertex::new(p + u2 * d));
    }
    for i in 0..n {
        let j = (i + 1) % n;
        let len = corners[i].0.dist(corners[j].0);
        if trims[i].1 + trims[j].0 > len + 1e-6 {
            return Err("a corner radius is too large for the section".into());
        }
    }
    Ok(out)
}

fn poly(corners: &[(Vec2, f64)], hole: bool) -> Result<Loop, String> {
    Ok(Loop::Poly { verts: filleted(corners)?, hole })
}

fn p(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}

/// The closed loops of a profile with the bounding box's lower-left corner at the origin.
pub fn outline(prof: &Profile) -> Result<Vec<Loop>, String> {
    Ok(match prof.shape {
        Shape::I { h, b, tw, tf, r } => {
            let (x0, x1, w0, w1) = (0.0, b, (b - tw) / 2.0, (b + tw) / 2.0);
            vec![poly(
                &[
                    (p(x0, 0.0), 0.0),
                    (p(x1, 0.0), 0.0),
                    (p(x1, tf), 0.0),
                    (p(w1, tf), r),
                    (p(w1, h - tf), r),
                    (p(x1, h - tf), 0.0),
                    (p(x1, h), 0.0),
                    (p(x0, h), 0.0),
                    (p(x0, h - tf), 0.0),
                    (p(w0, h - tf), r),
                    (p(w0, tf), r),
                    (p(x0, tf), 0.0),
                ],
                false,
            )?]
        }
        Shape::Channel { h, b, tw, tf, r1, r2, taper } => {
            // Inner flange face: thickness tf half-way along the flange, thinning towards the toe.
            let xm = tw + (b - tw) / 2.0;
            let y = |x: f64| tf + taper * (xm - x);
            vec![poly(
                &[
                    (p(0.0, 0.0), 0.0),
                    (p(b, 0.0), 0.0),
                    (p(b, y(b)), r2),
                    (p(tw, y(tw)), r1),
                    (p(tw, h - y(tw)), r1),
                    (p(b, h - y(b)), r2),
                    (p(b, h), 0.0),
                    (p(0.0, h), 0.0),
                ],
                false,
            )?]
        }
        Shape::Angle { a, b, t, r1, r2 } => {
            vec![poly(&[(p(0.0, 0.0), 0.0), (p(b, 0.0), 0.0), (p(b, t), r2), (p(t, t), r1), (p(t, a), r2), (p(0.0, a), 0.0)], false)?]
        }
        Shape::Rhs { h, b, t, ro, ri } => vec![
            poly(&[(p(0.0, 0.0), ro), (p(b, 0.0), ro), (p(b, h), ro), (p(0.0, h), ro)], false)?,
            poly(&[(p(t, t), ri), (p(b - t, t), ri), (p(b - t, h - t), ri), (p(t, h - t), ri)], true)?,
        ],
        Shape::Chs { d, t } => {
            let c = p(d / 2.0, d / 2.0);
            vec![Loop::Circle { center: c, r: d / 2.0, hole: false }, Loop::Circle { center: c, r: d / 2.0 - t, hole: true }]
        }
        Shape::Flat { b, t } => vec![poly(&[(p(0.0, 0.0), 0.0), (p(b, 0.0), 0.0), (p(b, t), 0.0), (p(0.0, t), 0.0)], false)?],
        Shape::Round { d } => vec![Loop::Circle { center: p(d / 2.0, d / 2.0), r: d / 2.0, hole: false }],
        Shape::Square { a } => vec![poly(&[(p(0.0, 0.0), 0.0), (p(a, 0.0), 0.0), (p(a, a), 0.0), (p(0.0, a), 0.0)], false)?],
    })
}

/// Points along a loop (arcs subdivided), counter-clockwise or not as drawn.
fn tessellate(l: &Loop) -> Vec<Vec2> {
    match l {
        Loop::Circle { center, r, .. } => (0..720).map(|i| Vec2::polar(*center, *r, cadcraft_geom::TAU * i as f64 / 720.0)).collect(),
        Loop::Poly { verts, .. } => {
            let n = verts.len();
            let mut pts = Vec::new();
            for i in 0..n {
                let (Some(a), Some(b)) = (verts.get(i), verts.get((i + 1) % n)) else { continue };
                pts.push(a.p);
                if let Some((arc, _)) = bulge_to_arc(a.p, b.p, a.bulge) {
                    let sweep = 4.0 * a.bulge.atan();
                    let a0 = arc.center.angle_to(a.p);
                    let steps = ((sweep.abs() / cadcraft_geom::TAU) * 720.0).ceil().clamp(2.0, 720.0) as usize;
                    for k in 1..steps {
                        pts.push(Vec2::polar(arc.center, arc.radius, a0 + sweep * k as f64 / steps as f64));
                    }
                }
            }
            pts
        }
    }
}

/// Area integrals of a region about the origin.
#[derive(Clone, Copy, Debug, Default)]
struct Moments {
    a: f64,
    sx: f64,  // ∫x dA
    sy: f64,  // ∫y dA
    ixx: f64, // ∫y² dA
    iyy: f64, // ∫x² dA
    ixy: f64, // ∫xy dA
}

fn polygon_moments(pts: &[Vec2]) -> Moments {
    let mut m = Moments::default();
    let n = pts.len();
    for i in 0..n {
        let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
        let c = a.x * b.y - b.x * a.y;
        m.a += c / 2.0;
        m.sx += (a.x + b.x) * c / 6.0;
        m.sy += (a.y + b.y) * c / 6.0;
        m.ixx += (a.y * a.y + a.y * b.y + b.y * b.y) * c / 12.0;
        m.iyy += (a.x * a.x + a.x * b.x + b.x * b.x) * c / 12.0;
        m.ixy += (a.x * b.y + 2.0 * a.x * a.y + 2.0 * b.x * b.y + b.x * a.y) * c / 24.0;
    }
    if m.a < 0.0 {
        m = Moments { a: -m.a, sx: -m.sx, sy: -m.sy, ixx: -m.ixx, iyy: -m.iyy, ixy: -m.ixy };
    }
    m
}

fn loop_moments(l: &Loop) -> Moments {
    match l {
        Loop::Circle { center: c, r, .. } => {
            let a = std::f64::consts::PI * r * r;
            let i0 = std::f64::consts::PI * r.powi(4) / 4.0;
            Moments { a, sx: a * c.x, sy: a * c.y, ixx: i0 + a * c.y * c.y, iyy: i0 + a * c.x * c.x, ixy: a * c.x * c.y }
        }
        Loop::Poly { .. } => polygon_moments(&tessellate(l)),
    }
}

fn perimeter(l: &Loop) -> f64 {
    match l {
        Loop::Circle { r, .. } => cadcraft_geom::TAU * r,
        Loop::Poly { .. } => {
            let pts = tessellate(l);
            let n = pts.len();
            (0..n).filter_map(|i| Some(pts.get(i)?.dist(*pts.get((i + 1) % n)?))).sum()
        }
    }
}

/// Section properties in millimetres about the centroidal axes parallel to the drawing axes:
/// `y` is the horizontal axis (the strong axis of an I section drawn upright), `z` the vertical.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Props {
    pub area: f64,
    pub mass: f64,
    pub cx: f64,
    pub cy: f64,
    pub iy: f64,
    pub iz: f64,
    pub iyz: f64,
    pub wy: f64,
    pub wz: f64,
    pub width: f64,
    pub height: f64,
    pub surface: f64,
}

pub fn props(loops: &[Loop]) -> Props {
    let mut m = Moments::default();
    let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
    let mut surface = 0.0;
    for l in loops {
        let lm = loop_moments(l);
        let s = if l.hole() { -1.0 } else { 1.0 };
        m.a += s * lm.a;
        m.sx += s * lm.sx;
        m.sy += s * lm.sy;
        m.ixx += s * lm.ixx;
        m.iyy += s * lm.iyy;
        m.ixy += s * lm.ixy;
        if !l.hole() {
            surface += perimeter(l);
            for q in tessellate(l) {
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
    }
    if m.a <= 0.0 {
        return Props::default();
    }
    let (cx, cy) = (m.sx / m.a, m.sy / m.a);
    let iy = m.ixx - m.a * cy * cy;
    let iz = m.iyy - m.a * cx * cx;
    let iyz = m.ixy - m.a * cx * cy;
    let (dy, dz) = ((hi.y - cy).max(cy - lo.y), (hi.x - cx).max(cx - lo.x));
    Props {
        area: m.a,
        mass: m.a * 1e-6 * DENSITY,
        cx,
        cy,
        iy,
        iz,
        iyz,
        wy: if dy > 0.0 { iy / dy } else { 0.0 },
        wz: if dz > 0.0 { iz / dz } else { 0.0 },
        width: hi.x - lo.x,
        height: hi.y - lo.y,
        surface: surface / 1000.0,
    }
}

fn r2(x: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (x * k).round() / k
}

/// Properties as JSON in the units steel tables use: cm², kg/m, cm⁴, cm³, cm, m²/m.
pub fn props_json(prof: &Profile, pr: &Props) -> Value {
    let i = |v: f64| if pr.area > 0.0 { (v / pr.area).max(0.0).sqrt() / 10.0 } else { 0.0 };
    json!({
        "profile": prof.name,
        "family": prof.family,
        "height_mm": r2(pr.height, 2),
        "width_mm": r2(pr.width, 2),
        "area_cm2": r2(pr.area / 100.0, 2),
        "mass_kg_m": r2(pr.mass, 2),
        "surface_m2_m": r2(pr.surface, 3),
        "centroid_mm": [r2(pr.cx, 2), r2(pr.cy, 2)],
        "Iy_cm4": r2(pr.iy / 1e4, 1),
        "Iz_cm4": r2(pr.iz / 1e4, 1),
        "Iyz_cm4": r2(pr.iyz / 1e4, 1),
        "Wy_cm3": r2(pr.wy / 1e3, 1),
        "Wz_cm3": r2(pr.wz / 1e3, 1),
        "iy_cm": r2(i(pr.iy), 2),
        "iz_cm": r2(i(pr.iz), 2),
    })
}

/// A readable summary for the command line.
pub fn props_text(prof: &Profile, pr: &Props) -> String {
    let i = |v: f64| if pr.area > 0.0 { (v / pr.area).max(0.0).sqrt() / 10.0 } else { 0.0 };
    format!(
        "{}: {:.1} x {:.1} mm   A = {:.2} cm²   G = {:.2} kg/m   surface = {:.3} m²/m\n\
         Iy = {:.1} cm⁴   Wy = {:.1} cm³   iy = {:.2} cm   (horizontal axis)\n\
         Iz = {:.1} cm⁴   Wz = {:.1} cm³   iz = {:.2} cm   (vertical axis)",
        prof.name,
        pr.width,
        pr.height,
        pr.area / 100.0,
        pr.mass,
        pr.surface,
        pr.iy / 1e4,
        pr.wy / 1e3,
        i(pr.iy),
        pr.iz / 1e4,
        pr.wz / 1e3,
        i(pr.iz),
    )
}
