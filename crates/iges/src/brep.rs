//! From SAT records to faces: each face's surface and its loops sampled as closed point lists,
//! walking body → lump → shell → face → loop → coedge → edge → curve.

use crate::geom::{BSpline, Curve, Surface, V3, Xform};
use crate::sat::{Rec, Sat, Tok};

#[derive(Clone, Debug)]
pub struct Face {
    pub surface: Surface,
    /// Closed loops (first point repeated at the end), in model space.
    pub loops: Vec<Vec<V3>>,
    /// The same loops as their edges in order: the edge's record number and whether the loop
    /// runs against the edge (a reversed coedge). Faces sharing an edge share its number.
    pub topo: Vec<Vec<(i64, bool)>>,
    /// Which shell of the solid the face belongs to, counted from 0.
    pub shell: usize,
}

/// An edge as STEP needs it: its end vertices (record numbers and points), the curve it lies
/// on and its points sampled from start to end.
#[derive(Clone, Debug)]
pub struct EdgeGeom {
    pub v0: i64,
    pub v1: i64,
    pub p0: V3,
    pub p1: V3,
    pub curve: Option<Curve>,
    pub pts: Vec<V3>,
}

#[derive(Debug, Default)]
pub struct Report {
    pub faces: usize,
    pub skipped: Vec<String>,
    /// Largest distance between an edge's ends and its vertices.
    pub vertex_gap: f64,
    /// Largest distance between a sampled edge point and its face's surface.
    pub surface_gap: f64,
    /// Every edge once, sampled from its start to its end vertex.
    pub edges: Vec<Vec<V3>>,
    /// Every edge once, by its record number.
    pub edge_geom: std::collections::BTreeMap<i64, EdgeGeom>,
    seen: std::collections::HashSet<i64>,
    shells: usize,
}

/// Bounds every walk along `next` pointers: a cycle in hostile data ends here.
const MAX_CHAIN: usize = 200_000;

fn get<'a>(sat: &'a Sat, p: Option<i64>, what: &str) -> Result<&'a Rec, String> {
    p.and_then(|p| sat.rec(p)).ok_or_else(|| format!("a {what} pointer is missing"))
}

fn transform(sat: &Sat, body: &Rec) -> Result<Xform, String> {
    let Some(t) = body.ptr(2).and_then(|p| sat.rec(p)) else { return Ok(Xform::default()) };
    if t.kind != "transform" {
        return Ok(Xform::default());
    }
    let v: [f64; 13] = t.nums(0).ok_or("the body's transform is malformed")?;
    Ok(Xform { r: [[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]], t: V3::new(v[9], v[10], v[11]), scale: v[12] })
}

fn surface(rec: &Rec, x: &Xform) -> Result<Surface, String> {
    let v9: [f64; 9] = rec.nums(0).ok_or_else(|| format!("malformed {}", rec.kind))?;
    let (o, n, d) = (x.point(V3::new(v9[0], v9[1], v9[2])), x.dir(V3::new(v9[3], v9[4], v9[5])), x.dir(V3::new(v9[6], v9[7], v9[8])));
    let n = n.unit().ok_or("a surface has no normal")?;
    match rec.base() {
        "plane" => {
            let e1 = (d - n * d.dot(n)).unit().or_else(|| perpendicular(n)).ok_or("a plane has no direction")?;
            Ok(Surface::Plane { o, n, e1, e2: n.cross(e1) })
        }
        "cone" => {
            let ratio = rec.num(9).unwrap_or(1.0);
            if (ratio.abs() - 1.0).abs() > 1e-9 {
                return Err("elliptical cylinders and cones are not supported yet".into());
            }
            let r0 = d.len() * x.scale;
            let e1 = (d - n * d.dot(n)).unit().ok_or("a cone has no radius direction")?;
            let (sin, cos) = (rec.num(12).unwrap_or(0.0), rec.num(13).unwrap_or(1.0));
            if cos.abs() < 1e-12 {
                return Err("a degenerate cone".into());
            }
            Ok(Surface::Cone { c: o, n, e1, e2: n.cross(e1), r0, tan: sin / cos })
        }
        other => Err(format!("{other} surfaces are not supported yet")),
    }
}

fn perpendicular(n: V3) -> Option<V3> {
    let a = if n.x.abs() < 0.9 { V3::new(1.0, 0.0, 0.0) } else { V3::new(0.0, 1.0, 0.0) };
    n.cross(a).unit()
}

/// The B-spline of an intcurve's data: `nubs|nurbs degree closure count (knot mult)… ctrl…`.
/// ACIS lists the end knots `degree` times; a clamped vector needs them once more.
fn spline(rec: &Rec, x: &Xform) -> Result<BSpline, String> {
    let f = rec.fields();
    let start = f
        .iter()
        .position(|t| matches!(t, Tok::Word(w) if w == "nubs" || w == "nurbs"))
        .ok_or_else(|| format!("{} has no approximating spline", rec.kind))?;
    let rational = matches!(f.get(start), Some(Tok::Word(w)) if w == "nurbs");
    let num = |i: usize| match f.get(i) {
        Some(Tok::Num(v)) => Ok(*v),
        _ => Err(format!("malformed spline in {}", rec.kind)),
    };
    let degree = num(start + 1)? as usize;
    let count = num(start + 3)? as usize;
    if !(1..=25).contains(&degree) || !(2..=100_000).contains(&count) {
        return Err("unsupported spline size".into());
    }
    let mut knots = Vec::new();
    let mut i = start + 4;
    for _ in 0..count {
        let (k, m) = (num(i)?, num(i + 1)? as usize);
        if m == 0 || m > degree + 1 {
            return Err("malformed knot multiplicity".into());
        }
        knots.extend(std::iter::repeat_n(k, m));
        i += 2;
    }
    let dim = if rational { 4 } else { 3 };
    let mut vals = Vec::new();
    while let Some(Tok::Num(v)) = f.get(i) {
        vals.push(*v);
        i += 1;
    }
    let n = vals.len() / dim;
    if n + degree + 1 == knots.len() + 2
        && let (Some(&a), Some(&b)) = (knots.first(), knots.last())
    {
        knots.insert(0, a);
        knots.push(b);
    }
    // A control point count that a periodic spline's own data sometimes stops short of.
    if n + degree + 1 != knots.len() {
        return Err(format!("spline of {} control points does not match its {} knots", n, knots.len()));
    }
    let mut ctrl = Vec::with_capacity(n);
    let mut weights = Vec::new();
    for c in vals.chunks_exact(dim) {
        ctrl.push(x.point(V3::new(c[0], c[1], c[2])));
        if rational {
            weights.push(c[3]);
        }
    }
    Ok(BSpline { degree, knots, ctrl, weights: rational.then_some(weights) })
}

/// The curve of an edge and whether its parameter runs backwards (an intcurve marked reversed).
fn curve(rec: &Rec, x: &Xform) -> Result<(Curve, bool), String> {
    match rec.base() {
        "straight" => {
            let v: [f64; 6] = rec.nums(0).ok_or("malformed straight curve")?;
            Ok((Curve::Line { root: x.point(V3::new(v[0], v[1], v[2])), dir: x.dir(V3::new(v[3], v[4], v[5])) * x.scale }, false))
        }
        "ellipse" => {
            let v: [f64; 9] = rec.nums(0).ok_or("malformed ellipse")?;
            let ratio = rec.num(9).unwrap_or(1.0);
            let n = x.dir(V3::new(v[3], v[4], v[5])).unit().ok_or("an ellipse has no normal")?;
            let major = x.dir(V3::new(v[6], v[7], v[8])) * x.scale;
            Ok((Curve::Ellipse { center: x.point(V3::new(v[0], v[1], v[2])), major, minor: n.cross(major) * ratio }, false))
        }
        "intcurve" | "exactcur" => {
            let reversed = rec.word(0) == Some("reversed");
            Ok((Curve::Spline(spline(rec, x)?), reversed))
        }
        other => Err(format!("{other} curves are not supported yet")),
    }
}

fn point(sat: &Sat, vertex: Option<i64>, x: &Xform) -> Result<V3, String> {
    let v = get(sat, vertex, "vertex")?;
    // Newer releases put a number between the edge and the point.
    let pp = v.fields().iter().rev().find_map(|t| match t {
        Tok::Ptr(p) if *p >= 0 => Some(*p),
        _ => None,
    });
    let p = get(sat, pp, "point")?;
    let c: [f64; 3] = p.nums(0).ok_or("malformed point")?;
    Ok(x.point(V3::from(c)))
}

/// An edge's points from its start vertex to its end vertex, and the curve it lies on.
fn edge_points(sat: &Sat, edge: &Rec, x: &Xform, tol: f64, rep: &mut Report) -> Result<(Vec<V3>, Option<Curve>), String> {
    let (p0, p1) = (point(sat, edge.ptr(0), x)?, point(sat, edge.ptr(2), x)?);
    let Some(crec) = edge.ptr(5).and_then(|p| sat.rec(p)) else {
        // An edge without a curve is a point.
        return Ok((vec![p0, p1], None));
    };
    let (c, curve_rev) = curve(crec, x)?;
    let (t0, t1) = (edge.num(1).ok_or("an edge has no start parameter")?, edge.num(3).ok_or("an edge has no end parameter")?);
    let edge_rev = edge.word(6) == Some("reversed");
    // The edge's parameter is the curve's, negated when the edge runs against the curve; an
    // intcurve marked reversed negates its spline's parameter once more.
    let sign = if edge_rev != curve_rev { -1.0 } else { 1.0 };
    let n = c.samples(t1 - t0, tol);
    let mut pts = Vec::with_capacity(n + 1);
    for k in 0..=n {
        let t = t0 + (t1 - t0) * k as f64 / n as f64;
        pts.push(c.eval(sign * t).ok_or("a curve could not be evaluated")?);
    }
    if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
        rep.vertex_gap = rep.vertex_gap.max(a.dist(p0)).max(b.dist(p1));
    }
    Ok((pts, Some(c)))
}

fn loop_points(sat: &Sat, lp: &Rec, x: &Xform, tol: f64, rep: &mut Report) -> Result<(Vec<V3>, Vec<(i64, bool)>), String> {
    let first = lp.ptr(1).ok_or("a loop has no coedge")?;
    let mut out: Vec<V3> = Vec::new();
    let mut topo = Vec::new();
    let mut ce = first;
    for _ in 0..MAX_CHAIN {
        let co = get(sat, Some(ce), "coedge")?;
        let edge = get(sat, co.ptr(3), "edge")?;
        let (mut pts, curve) = edge_points(sat, edge, x, tol, rep)?;
        let reversed = co.word(4) == Some("reversed");
        if let Some(e) = co.ptr(3) {
            if rep.seen.insert(e) {
                rep.edges.push(pts.clone());
                let (p0, p1) = (point(sat, edge.ptr(0), x)?, point(sat, edge.ptr(2), x)?);
                let (v0, v1) = (edge.ptr(0).unwrap_or(-1), edge.ptr(2).unwrap_or(-1));
                rep.edge_geom.insert(e, EdgeGeom { v0, v1, p0, p1, curve, pts: pts.clone() });
            }
            topo.push((e, reversed));
        }
        if reversed {
            pts.reverse();
        }
        if let (Some(last), Some(head)) = (out.last(), pts.first())
            && last.dist(*head) < 1e-9
        {
            pts.remove(0);
        }
        out.extend(pts);
        ce = co.ptr(0).ok_or("a coedge has no next")?;
        if ce == first {
            if let (Some(&a), Some(&b)) = (out.first(), out.last())
                && a.dist(b) > 1e-6
            {
                out.push(a);
            }
            return Ok((out, topo));
        }
    }
    Err("a loop does not close".into())
}

/// The faces of one solid's SAT data.
pub fn faces(sat: &Sat, tol: f64) -> (Vec<Face>, Report) {
    let mut rep = Report::default();
    let mut out = Vec::new();
    for (bi, body) in sat.recs.iter().enumerate() {
        if body.kind != "body" {
            continue;
        }
        let x = match transform(sat, body) {
            Ok(x) => x,
            Err(e) => {
                rep.skipped.push(format!("body {bi}: {e}"));
                continue;
            }
        };
        let mut lump = body.ptr(0);
        for _ in 0..MAX_CHAIN {
            let Some(l) = lump.and_then(|p| sat.rec(p)) else { break };
            let mut shell = l.ptr(1);
            for _ in 0..MAX_CHAIN {
                let Some(s) = shell.and_then(|p| sat.rec(p)) else { break };
                let shell_no = rep.shells;
                rep.shells += 1;
                let mut face = s.ptr(2);
                for _ in 0..MAX_CHAIN {
                    let Some(f) = face.and_then(|p| sat.rec(p)) else { break };
                    match one_face(sat, f, &x, tol, &mut rep) {
                        Ok(mut fc) => {
                            fc.shell = shell_no;
                            out.push(fc)
                        }
                        Err(e) => rep.skipped.push(e),
                    }
                    face = f.ptr(0);
                }
                shell = s.ptr(0);
            }
            lump = l.ptr(0);
        }
    }
    rep.faces = out.len();
    (out, rep)
}

fn one_face(sat: &Sat, f: &Rec, x: &Xform, tol: f64, rep: &mut Report) -> Result<Face, String> {
    let srec = get(sat, f.ptr(4), "surface")?;
    let mut surface = surface(srec, x)?;
    let mut loops = Vec::new();
    let mut topo = Vec::new();
    let mut lp = f.ptr(1);
    for _ in 0..MAX_CHAIN {
        let Some(l) = lp.and_then(|p| sat.rec(p)) else { break };
        let (pts, edges) = loop_points(sat, l, x, tol, rep)?;
        loops.push(pts);
        topo.push(edges);
        lp = l.ptr(0);
    }
    if loops.is_empty() {
        return Err("a face without loops (a whole closed surface) is not supported yet".into());
    }
    let gap = |s: &Surface| loops.iter().flatten().map(|p| s.residual(*p)).fold(0.0, f64::max);
    // The cone's half-angle sign is checked against the edges lying on it.
    if let Surface::Cone { c, n, e1, e2, r0, tan } = surface
        && tan != 0.0
    {
        let other = Surface::Cone { c, n, e1, e2, r0, tan: -tan };
        if gap(&other) < gap(&surface) {
            surface = other;
        }
    }
    let g = gap(&surface);
    rep.surface_gap = rep.surface_gap.max(g);
    if g > 0.5 {
        return Err(format!("a {} face's edges are {g:.3} mm off its surface", srec.base()));
    }
    Ok(Face { surface, loops, topo, shell: 0 })
}
