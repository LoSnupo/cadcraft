//! Faces as IGES trimmed surfaces: an exact NURBS surface (a bilinear patch for a plane, a
//! rational circle swept along the axis for a cylinder or cone) and boundaries in its parameter
//! space. A cylinder face between two closed edges (a tube) gets one outer boundary that runs
//! along the bottom edge, up the seam, back along the top edge and down the seam.

use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_4, TAU};

use crate::brep::Face;
use crate::geom::{Surface, V3, circle_param};

/// A NURBS surface in IGES 128 terms: control points with `u` varying fastest.
#[derive(Clone, Debug)]
pub struct Nurbs {
    pub deg: (usize, usize),
    pub count: (usize, usize),
    pub knots_u: Vec<f64>,
    pub knots_v: Vec<f64>,
    pub weights: Vec<f64>,
    pub ctrl: Vec<V3>,
    pub range: (f64, f64, f64, f64),
    pub rational: bool,
}

/// A boundary: points in the surface's parameter space and the same points on the surface.
#[derive(Clone, Debug)]
pub struct Boundary {
    pub uv: Vec<(f64, f64)>,
    pub xyz: Vec<V3>,
}

#[derive(Clone, Debug)]
pub struct Trimmed {
    pub surf: Nurbs,
    /// The face's surface and, on a cylinder or cone, the seam angle the parameters start from.
    pub surface: Surface,
    pub seam: f64,
    pub outer: Boundary,
    pub inner: Vec<Boundary>,
    pub kind: &'static str,
}

fn area(p: &[(f64, f64)]) -> f64 {
    p.windows(2).map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1).sum::<f64>() / 2.0
}

fn bounds(loops: &[Vec<(f64, f64)>]) -> (f64, f64, f64, f64) {
    let mut b = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &(u, v) in loops.iter().flatten() {
        b = (b.0.min(u), b.1.max(u), b.2.min(v), b.3.max(v));
    }
    b
}

/// Outer = the largest loop, made counter-clockwise; the others clockwise.
fn classify(mut loops: Vec<Vec<(f64, f64)>>) -> Result<(Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>), String> {
    let i = (0..loops.len()).max_by(|&a, &b| area(&loops[a]).abs().total_cmp(&area(&loops[b]).abs())).ok_or("a face has no loops")?;
    let mut outer = loops.swap_remove(i);
    if area(&outer) < 0.0 {
        outer.reverse();
    }
    for l in &mut loops {
        if area(l) > 0.0 {
            l.reverse();
        }
    }
    Ok((outer, loops))
}

fn wrap(d: f64) -> f64 {
    let d = d.rem_euclid(TAU);
    if d > std::f64::consts::PI { d - TAU } else { d }
}

/// Angles made continuous along a closed loop.
fn unwrapped(s: &Surface, pts: &[V3]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for p in pts {
        let (a, h) = s.param(*p);
        let a = match out.last() {
            Some(&(prev, _)) => prev + wrap(a - prev),
            None => a,
        };
        out.push((a, h));
    }
    out
}

/// A loop that goes once around the axis, cut at the seam angle `a` and given angles from the
/// seam: from 0 to 2π, or from 2π to 0.
fn cut_at_seam(l: &[(f64, f64)], a: f64) -> Result<Vec<(f64, f64)>, String> {
    let cell = |t: f64| ((t - a) / TAU).floor();
    let i = (0..l.len().saturating_sub(1))
        .find(|&i| cell(l[i].0) != cell(l[i + 1].0))
        .ok_or_else(|| format!("a closed edge does not cross the seam (seam {a:.4}, from {:?} to {:?}, n={})", l.first(), l.last(), l.len()))?;
    let (p, q) = (l[i], l[i + 1]);
    let k = if q.0 > p.0 { cell(q.0) } else { cell(p.0) };
    let seam = a + k * TAU;
    let f = if (q.0 - p.0).abs() > 0.0 { (seam - p.0) / (q.0 - p.0) } else { 0.0 };
    let x = (seam, p.1 + (q.1 - p.1) * f);
    // From the crossing round to it again (the loop's last point repeats its first).
    let mut seq = vec![x];
    seq.extend_from_slice(&l[i + 1..l.len() - 1]);
    seq.extend_from_slice(&l[..=i]);
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(seq.len() + 1);
    for (t, h) in seq {
        let t = match out.last() {
            Some(&(prev, _)) => prev + wrap(t - prev),
            None => t,
        };
        out.push((t, h));
    }
    let last = out.last().map(|p| p.0).unwrap_or(x.0);
    let end = if last > x.0 { x.0 + TAU } else { x.0 - TAU };
    out.push((end, x.1));
    let base = if end > x.0 { x.0 } else { end };
    Ok(out.into_iter().map(|(t, h)| (t - base, h)).collect())
}

/// A seam angle as far as possible from the loops that do not go around the axis.
fn seam_angle(intervals: &[(f64, f64)]) -> f64 {
    if intervals.is_empty() {
        return 0.0;
    }
    let covered = |t: f64| -> f64 {
        intervals
            .iter()
            .map(|&(lo, hi)| {
                let x = lo + (t - lo).rem_euclid(TAU);
                if x <= hi { 0.0 } else { (x - hi).min(lo + TAU - x) }
            })
            .fold(f64::MAX, f64::min)
    };
    (0..720).map(|k| k as f64 * TAU / 720.0).max_by(|a, b| covered(*a).total_cmp(&covered(*b))).unwrap_or(0.0)
}

fn plane(face: &Face, s: &Surface) -> Result<Trimmed, String> {
    let loops: Vec<Vec<(f64, f64)>> = face.loops.iter().map(|l| l.iter().map(|p| s.param(*p)).collect()).collect();
    let (u0, u1, v0, v1) = bounds(&loops);
    let m = ((u1 - u0).max(v1 - v0) * 0.01).max(0.1);
    let (u0, u1, v0, v1) = (u0 - m, u1 + m, v0 - m, v1 + m);
    let surf = Nurbs {
        deg: (1, 1),
        count: (2, 2),
        knots_u: vec![u0, u0, u1, u1],
        knots_v: vec![v0, v0, v1, v1],
        weights: vec![1.0; 4],
        ctrl: vec![s.eval(u0, v0), s.eval(u1, v0), s.eval(u0, v1), s.eval(u1, v1)],
        range: (u0, u1, v0, v1),
        rational: false,
    };
    let (outer, inner) = classify(loops)?;
    let b = |uv: Vec<(f64, f64)>| Boundary { xyz: uv.iter().map(|&(u, v)| s.eval(u, v)).collect(), uv };
    Ok(Trimmed { surf, surface: *s, seam: 0.0, outer: b(outer), inner: inner.into_iter().map(b).collect(), kind: "plane" })
}

fn cone(face: &Face, s: &Surface) -> Result<Trimmed, String> {
    let Surface::Cone { c, n, e1, e2, r0, tan } = *s else { return Err("not a cone".into()) };
    let loops: Vec<Vec<(f64, f64)>> = face.loops.iter().map(|l| unwrapped(s, l)).collect();
    let turns = |l: &Vec<(f64, f64)>| match (l.first(), l.last()) {
        (Some(a), Some(b)) => ((b.0 - a.0) / TAU).round() as i64,
        _ => 0,
    };
    let (around, local): (Vec<_>, Vec<_>) = loops.into_iter().partition(|l| turns(l) != 0);
    let intervals: Vec<(f64, f64)> = local
        .iter()
        .map(|l| {
            let lo = l.iter().map(|p| p.0).fold(f64::MAX, f64::min);
            let hi = l.iter().map(|p| p.0).fold(f64::MIN, f64::max);
            (lo.rem_euclid(TAU), lo.rem_euclid(TAU) + (hi - lo))
        })
        .collect();
    // The seam must not pass exactly through a point of an edge going round the axis, or the
    // edge would start and end on it and never cross it.
    let mut a = seam_angle(&intervals);
    for _ in 0..16 {
        let on_seam = around.iter().flatten().any(|p| {
            let d = (p.0 - a).rem_euclid(TAU);
            d.min(TAU - d) < 1e-7
        });
        if !on_seam {
            break;
        }
        a += 1.234_567e-4;
    }
    // Angles from the seam, 0..2π.
    let mut phi_loops: Vec<Vec<(f64, f64)>> = local
        .iter()
        .map(|l| {
            let lo = l.iter().map(|p| p.0).fold(f64::MAX, f64::min);
            let k = ((lo - a) / TAU).floor();
            l.iter().map(|&(t, h)| (t - a - k * TAU, h)).collect()
        })
        .collect();
    let (outer, inner) = match around.len() {
        0 => classify(std::mem::take(&mut phi_loops))?,
        2 => {
            let mean = |l: &Vec<(f64, f64)>| l.iter().map(|p| p.1).sum::<f64>() / l.len().max(1) as f64;
            let (mut lo, mut hi) = (cut_at_seam(&around[0], a)?, cut_at_seam(&around[1], a)?);
            if mean(&lo) > mean(&hi) {
                std::mem::swap(&mut lo, &mut hi);
            }
            let rising = |l: &Vec<(f64, f64)>| l.last().map(|p| p.0).unwrap_or(0.0) > l.first().map(|p| p.0).unwrap_or(0.0);
            if !rising(&lo) {
                lo.reverse();
            }
            if rising(&hi) {
                hi.reverse();
            }
            let mut outer = lo;
            outer.extend(hi);
            if let Some(&f) = outer.first() {
                outer.push(f);
            }
            for l in &mut phi_loops {
                if area(l) > 0.0 {
                    l.reverse();
                }
            }
            (outer, std::mem::take(&mut phi_loops))
        }
        k => return Err(format!("a cylinder face with {k} edges around its axis is not supported yet")),
    };
    let all: Vec<Vec<(f64, f64)>> = std::iter::once(outer.clone()).chain(inner.iter().cloned()).collect();
    let (_, _, h0, h1) = bounds(&all);
    let m = ((h1 - h0) * 0.01).max(0.1);
    let (h0, h1) = (h0 - m, h1 + m);
    let mut ctrl = Vec::with_capacity(18);
    let mut weights = Vec::with_capacity(18);
    for h in [h0, h1] {
        let r = r0 + h * tan;
        for k in 0..9 {
            let ang = a + k as f64 * FRAC_PI_4;
            let (rr, w) = if k % 2 == 1 { (r / FRAC_1_SQRT_2, FRAC_1_SQRT_2) } else { (r, 1.0) };
            ctrl.push(c + n * h + (e1 * ang.cos() + e2 * ang.sin()) * rr);
            weights.push(w);
        }
    }
    let q = std::f64::consts::FRAC_PI_2;
    let surf = Nurbs {
        deg: (2, 1),
        count: (9, 2),
        knots_u: vec![0.0, 0.0, 0.0, q, q, 2.0 * q, 2.0 * q, 3.0 * q, 3.0 * q, TAU, TAU, TAU],
        knots_v: vec![h0, h0, h1, h1],
        weights,
        ctrl,
        range: (0.0, TAU, h0, h1),
        rational: true,
    };
    let b = |l: Vec<(f64, f64)>| Boundary {
        xyz: l.iter().map(|&(p, h)| s.eval(a + p, h)).collect(),
        uv: l.iter().map(|&(p, h)| (circle_param(p), h)).collect(),
    };
    let kind = if tan == 0.0 { "cylinder" } else { "cone" };
    Ok(Trimmed { surf, surface: *s, seam: a, outer: b(outer), inner: inner.into_iter().map(b).collect(), kind })
}

pub fn trim(face: &Face) -> Result<Trimmed, String> {
    match face.surface {
        s @ Surface::Plane { .. } => plane(face, &s),
        s @ Surface::Cone { .. } => cone(face, &s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::BSpline;

    /// A boundary point's surface parameters land on the boundary's own 3D point: the rational
    /// circle's parameter is found from the angle correctly in every quarter.
    #[test]
    fn cone_parameters_match_points() {
        let s = Surface::Cone {
            c: V3::new(10.0, 20.0, 5.0),
            n: V3::new(0.0, 0.0, 1.0),
            e1: V3::new(1.0, 0.0, 0.0),
            e2: V3::new(0.0, 1.0, 0.0),
            r0: 30.0,
            tan: 0.0,
        };
        let ring = |h: f64, rev: bool| -> Vec<V3> {
            let mut v: Vec<V3> = (0..=200).map(|k| s.eval(TAU * k as f64 / 200.0 + 0.3, h)).collect();
            if rev {
                v.reverse();
            }
            v
        };
        let hole: Vec<V3> = (0..=100)
            .map(|k| {
                let t = TAU * k as f64 / 100.0;
                s.eval(2.0 + 0.2 * t.cos(), 50.0 + 8.0 * t.sin())
            })
            .collect();
        let face = Face { surface: s, loops: vec![ring(0.0, true), ring(100.0, false), hole] };
        let t = trim(&face).unwrap();
        assert_eq!(t.inner.len(), 1);
        let row = BSpline { degree: 2, knots: t.surf.knots_u.clone(), ctrl: t.surf.ctrl[..9].to_vec(), weights: Some(t.surf.weights[..9].to_vec()) };
        let (h0, h1) = (t.surf.range.2, t.surf.range.3);
        for b in std::iter::once(&t.outer).chain(&t.inner) {
            for (&(u, v), &p) in b.uv.iter().zip(&b.xyz) {
                // The surface is ruled in v: interpolate the two circle rows.
                let lo = row.eval(u).unwrap();
                let up = lo + V3::new(0.0, 0.0, h1 - h0);
                let q = lo + (up - lo) * ((v - h0) / (h1 - h0));
                assert!(q.dist(p) < 1e-6, "u={u} v={v}: {q:?} vs {p:?}");
            }
        }
        // The outer boundary goes round once, counter-clockwise in parameter space.
        assert!(area(&t.outer.uv) > 0.0);
    }

    #[test]
    fn circle_parameter_is_monotonic_and_exact_at_quarters() {
        let mut prev = -1.0;
        for k in 0..=400 {
            let u = circle_param(TAU * k as f64 / 400.0);
            assert!(u >= prev);
            prev = u;
        }
        for q in 0..=4 {
            let a = q as f64 * std::f64::consts::FRAC_PI_2;
            assert!((circle_param(a) - a).abs() < 1e-9);
        }
    }
}
