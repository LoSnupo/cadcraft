//! Orthographic and isometric views of 3D solids as 2D lines, for a drafting program that draws
//! in plan only: every edge of the solid, plus the outline (silhouette) lines of its cylinders
//! and cones, where the surface turns away from the viewer. Hidden lines are not removed.

use std::f64::consts::TAU;

use crate::geom::{Surface, V3, circle_param};
use crate::trim::Trimmed;

/// One view: its name and its lines in view coordinates (millimetres).
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub name: &'static str,
    pub lines: Vec<Vec<[f64; 2]>>,
}

/// The views drawn: name and the direction the viewer looks in.
pub const DIRECTIONS: [(&str, [f64; 3]); 4] =
    [("FRONT", [0.0, 1.0, 0.0]), ("RIGHT", [-1.0, 0.0, 0.0]), ("TOP", [0.0, 0.0, -1.0]), ("ISOMETRIC", [-1.0, 1.0, -1.0])];

/// Screen axes for a viewing direction: right, then up (Z stays up where it can).
fn basis(d: V3) -> (V3, V3) {
    let right = d.cross(V3::new(0.0, 0.0, 1.0)).unit().unwrap_or(V3::new(1.0, 0.0, 0.0));
    (right, right.cross(d))
}

/// The silhouette lines of a cylinder or cone face seen along `d`: the generators where the
/// surface's radial direction is square to the view, clipped to the face's boundaries.
fn silhouettes(t: &Trimmed, d: V3) -> Vec<[V3; 2]> {
    let Surface::Cone { n, e1, e2, .. } = t.surface else { return Vec::new() };
    if n.dot(d).abs() > 0.999 {
        return Vec::new();
    }
    let (a, b) = (e1.dot(d), e2.dot(d));
    let theta = (-a).atan2(b);
    let mut out = Vec::new();
    for th in [theta, theta + std::f64::consts::PI] {
        let phi = (th - t.seam).rem_euclid(TAU);
        let u = circle_param(phi);
        let mut vs: Vec<f64> = Vec::new();
        for bnd in std::iter::once(&t.outer).chain(&t.inner) {
            for w in bnd.uv.windows(2) {
                let (p, q) = (w[0], w[1]);
                if (p.0 <= u) != (q.0 <= u) && (q.0 - p.0).abs() > 0.0 {
                    vs.push(p.1 + (q.1 - p.1) * (u - p.0) / (q.0 - p.0));
                }
            }
        }
        vs.sort_by(f64::total_cmp);
        for pair in vs.chunks_exact(2) {
            out.push([t.surface.eval(t.seam + phi, pair[0]), t.surface.eval(t.seam + phi, pair[1])]);
        }
    }
    out
}

/// The views of solids given as their edges and trimmed faces.
pub fn project(edges: &[Vec<V3>], faces: &[Trimmed]) -> Vec<View> {
    DIRECTIONS
        .iter()
        .map(|&(name, dir)| {
            let d = V3::from(dir).unit().unwrap_or(V3::new(0.0, 0.0, -1.0));
            let (r, u) = basis(d);
            let p = |q: V3| [q.dot(r), q.dot(u)];
            let mut lines: Vec<Vec<[f64; 2]>> = edges.iter().map(|e| e.iter().map(|q| p(*q)).collect()).collect();
            for f in faces {
                lines.extend(silhouettes(f, d).into_iter().map(|[a, b]| vec![p(a), p(b)]));
            }
            // An edge seen end-on is a point: nothing to draw.
            lines.retain(|l| l.windows(2).any(|w| (w[0][0] - w[1][0]).hypot(w[0][1] - w[1][1]) > 1e-6));
            View { name, lines }
        })
        .collect()
}
