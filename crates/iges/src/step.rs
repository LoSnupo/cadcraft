//! STEP (ISO 10303-21, AP214) export of 3D solids as boundary representations: each ACIS shell
//! whose faces all convert and whose edges each join two faces becomes a closed solid
//! (`MANIFOLD_SOLID_BREP`); any other shell is written as surfaces (`SHELL_BASED_SURFACE_MODEL`)
//! and reported. Faces keep their exact surfaces (planes, cylinders, cones); lines, circles and
//! ellipses stay exact, and intersection curves (saddle cuts) are written as polylines within
//! the chord tolerance.

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::TAU;

use crate::brep::{EdgeGeom, Face};
use crate::geom::{Curve, Surface, V3};
use crate::trim::{area, unwrapped};

/// A real in STEP form: always with a decimal point before any exponent.
fn real(x: f64) -> String {
    if !x.is_finite() || x.abs() < 1e-14 {
        return "0.".into();
    }
    let s = format!("{x:?}");
    let (m, e) = match s.split_once('e') {
        Some((m, e)) => (m.to_string(), Some(e.to_string())),
        None => (s, None),
    };
    let m = if m.contains('.') { m } else { format!("{m}.") };
    match e {
        Some(e) => format!("{m}E{e}"),
        None => m,
    }
}

/// A string in STEP form: ASCII only, quotes doubled, backslashes escaped.
fn text(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        match c {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            _ => out.push('_'),
        }
    }
    out.push('\'');
    out
}

fn flag(b: bool) -> &'static str {
    if b { ".T." } else { ".F." }
}

fn list(ids: &[usize]) -> String {
    let v: Vec<String> = ids.iter().map(|i| format!("#{i}")).collect();
    format!("({})", v.join(","))
}

/// The DATA section being written: entity instances numbered from 1.
#[derive(Default)]
struct Data {
    lines: Vec<String>,
}

impl Data {
    fn add(&mut self, body: impl AsRef<str>) -> usize {
        let id = self.lines.len() + 1;
        self.lines.push(format!("#{id}={};", body.as_ref()));
        id
    }
    fn point(&mut self, p: V3) -> usize {
        self.add(format!("CARTESIAN_POINT('',({},{},{}))", real(p.x), real(p.y), real(p.z)))
    }
    fn dir(&mut self, d: V3) -> usize {
        self.add(format!("DIRECTION('',({},{},{}))", real(d.x), real(d.y), real(d.z)))
    }
    /// A placement: origin, axis and a reference direction made perpendicular to the axis.
    fn placement(&mut self, o: V3, z: V3, x: V3) -> Result<usize, String> {
        let z = z.unit().ok_or("an axis has no direction")?;
        let x = (x - z * x.dot(z)).unit().ok_or("a reference direction lies along its axis")?;
        let (po, pz, px) = (self.point(o), self.dir(z), self.dir(x));
        Ok(self.add(format!("AXIS2_PLACEMENT_3D('',#{po},#{pz},#{px})")))
    }
}

/// Whether the face's normal (the side its loops have it on the left) agrees with the normal
/// STEP gives its surface: a plane's axis, or away from the axis on a cylinder or cone.
pub(crate) fn face_sense(face: &Face) -> Result<bool, String> {
    match face.surface {
        Surface::Plane { .. } => {
            let a: f64 = face.loops.iter().map(|l| area(&l.iter().map(|p| face.surface.param(*p)).collect::<Vec<_>>())).sum();
            if a.abs() < 1e-12 {
                return Err("a plane face has no area".into());
            }
            Ok(a > 0.0)
        }
        Surface::Cone { .. } => {
            let loops: Vec<Vec<(f64, f64)>> = face.loops.iter().map(|l| unwrapped(&face.surface, l)).collect();
            let turns = |l: &Vec<(f64, f64)>| match (l.first(), l.last()) {
                (Some(a), Some(b)) => ((b.0 - a.0) / TAU).round() as i64,
                _ => 0,
            };
            let mean = |l: &Vec<(f64, f64)>| l.iter().map(|p| p.1).sum::<f64>() / l.len().max(1) as f64;
            // A loop going round the axis: the face is on its left, so it runs forwards (the
            // angle growing) when the face lies above it and the normals agree.
            let lowest = (0..loops.len()).filter(|&i| loops.get(i).is_some_and(|l| turns(l) != 0)).min_by(|&a, &b| {
                let (la, lb) = (loops.get(a).map(mean).unwrap_or(0.0), loops.get(b).map(mean).unwrap_or(0.0));
                la.total_cmp(&lb)
            });
            match lowest.and_then(|i| loops.get(i).map(|l| (i, l))) {
                Some((i, l)) => {
                    let others: Vec<f64> = loops.iter().enumerate().filter(|(k, _)| *k != i).flat_map(|(_, o)| o.iter().map(|p| p.1)).collect();
                    if others.is_empty() {
                        return Err("a cylinder or cone face bounded by a single edge round its axis is not supported yet".into());
                    }
                    let above = others.iter().sum::<f64>() / others.len() as f64 > mean(l);
                    Ok((turns(l) > 0) == above)
                }
                None => {
                    let a: f64 = loops.iter().map(|l| area(l)).sum();
                    if a.abs() < 1e-12 {
                        return Err("a cylinder or cone face has no area".into());
                    }
                    Ok(a > 0.0)
                }
            }
        }
    }
}

/// The face's surface as a STEP entity, and its kind for the report.
fn surface(d: &mut Data, face: &Face) -> Result<(usize, &'static str), String> {
    match face.surface {
        Surface::Plane { o, n, e1, .. } => {
            let a = d.placement(o, n, e1)?;
            Ok((d.add(format!("PLANE('',#{a})")), "plane"))
        }
        Surface::Cone { c, n, e1, r0, tan, .. } => {
            if tan == 0.0 {
                if r0.is_nan() || r0 <= 0.0 {
                    return Err("a cylinder has no radius".into());
                }
                let a = d.placement(c, n, e1)?;
                return Ok((d.add(format!("CYLINDRICAL_SURFACE('',#{a},{})", real(r0))), "cylinder"));
            }
            // STEP places a cone where its radius is positive, with a positive half-angle: at
            // the middle height of the face, the axis turned towards the wider end.
            let hs: Vec<f64> = face.loops.iter().flatten().map(|p| (*p - c).dot(n)).collect();
            let hm = hs.iter().sum::<f64>() / hs.len().max(1) as f64;
            let r = r0 + hm * tan;
            if r.is_nan() || r <= 0.0 {
                return Err("a cone face reaches its apex".into());
            }
            let axis = if tan > 0.0 { n } else { -n };
            let a = d.placement(c + n * hm, axis, e1)?;
            Ok((d.add(format!("CONICAL_SURFACE('',#{a},{},{})", real(r), real(tan.abs().atan()))), "cone"))
        }
    }
}

/// The edge's curve as a STEP entity and whether it runs the edge's way.
fn curve(d: &mut Data, e: &EdgeGeom) -> Result<(usize, bool), String> {
    let step = match (e.pts.first(), e.pts.get(1)) {
        (Some(a), Some(b)) => *b - *a,
        _ => return Err("an edge has no points".into()),
    };
    match &e.curve {
        Some(Curve::Line { .. }) => {
            let v = e.p1 - e.p0;
            let u = v.unit().ok_or("a straight edge has no length")?;
            let (p, dir) = (d.point(e.p0), d.dir(u));
            let vec = d.add(format!("VECTOR('',#{dir},{})", real(v.len())));
            Ok((d.add(format!("LINE('',#{p},#{vec})")), true))
        }
        Some(Curve::Ellipse { center, major, minor }) => {
            let (a, b) = (major.len(), minor.len());
            let normal = major.cross(*minor);
            if !(a > 1e-9 && b > 1e-9) || normal.unit().is_none() {
                return Err("a degenerate circle or ellipse".into());
            }
            let pl = d.placement(*center, normal, *major)?;
            let id = if (a - b).abs() <= 1e-9 * a.max(1.0) {
                d.add(format!("CIRCLE('',#{pl},{})", real(a)))
            } else {
                d.add(format!("ELLIPSE('',#{pl},{},{})", real(a), real(b)))
            };
            // The curve runs from the major axis towards the minor one; compare its tangent at
            // the edge's start with the edge's first step.
            let q = e.p0 - *center;
            let t = (q.dot(*minor) / (b * b)).atan2(q.dot(*major) / (a * a));
            let tangent = *minor * t.cos() - *major * t.sin();
            Ok((id, tangent.dot(step) >= 0.0))
        }
        Some(Curve::Spline(_)) => {
            // A polyline through the edge's points, its ends on the vertices.
            let mut pts = e.pts.clone();
            if let Some(f) = pts.first_mut() {
                *f = e.p0;
            }
            if let Some(l) = pts.last_mut() {
                *l = e.p1;
            }
            if pts.len() < 2 {
                return Err("an edge has too few points".into());
            }
            let ids: Vec<usize> = pts.iter().map(|p| d.point(*p)).collect();
            let n = ids.len();
            let mut mult = vec!["2".to_string()];
            mult.extend(std::iter::repeat_n("1".to_string(), n.saturating_sub(2)));
            mult.push("2".into());
            let knots: Vec<String> = (0..n).map(|k| real(k as f64)).collect();
            let closed = e.p0.dist(e.p1) < 1e-9;
            Ok((
                d.add(format!(
                    "B_SPLINE_CURVE_WITH_KNOTS('',1,{},.POLYLINE_FORM.,{},.F.,({}),({}),.UNSPECIFIED.)",
                    list(&ids),
                    flag(closed),
                    mult.join(","),
                    knots.join(",")
                )),
                true,
            ))
        }
        None => Err("an edge without a curve (a cone's apex) is not supported yet".into()),
    }
}

/// Every edge of the faces used exactly twice, once each way: the faces close a volume.
fn is_closed(faces: &[&Face]) -> bool {
    let mut uses: HashMap<i64, (u32, u32)> = HashMap::new();
    for f in faces {
        for &(e, rev) in f.topo.iter().flatten() {
            let u = uses.entry(e).or_default();
            if rev {
                u.1 += 1;
            } else {
                u.0 += 1;
            }
        }
    }
    !uses.is_empty() && uses.values().all(|&u| u == (1, 1))
}

/// What one solid's shells became.
#[derive(Default)]
pub(crate) struct Written {
    pub breps: Vec<usize>,
    pub surfaces: Vec<usize>,
    pub faces: BTreeMap<&'static str, usize>,
    pub warnings: Vec<String>,
}

/// Writes the faces of one solid (`edges` from its SAT data), shell by shell.
fn solid(d: &mut Data, index: usize, faces: &[Face], edges: &BTreeMap<i64, EdgeGeom>) -> Written {
    let mut w = Written::default();
    let mut vertices: HashMap<i64, usize> = HashMap::new();
    let mut curves: HashMap<i64, usize> = HashMap::new();
    let mut shells: BTreeMap<usize, Vec<&Face>> = BTreeMap::new();
    for f in faces {
        shells.entry(f.shell).or_default().push(f);
    }
    for (_, fs) in shells {
        let mut ids = Vec::new();
        let mut ok = Vec::new();
        for f in fs {
            match one_face(d, f, edges, &mut vertices, &mut curves) {
                Ok((id, kind)) => {
                    ids.push(id);
                    ok.push(f);
                    *w.faces.entry(kind).or_default() += 1;
                }
                Err(e) => w.warnings.push(format!("solid {index}: {e}")),
            }
        }
        if ids.is_empty() {
            continue;
        }
        // A face left out (here or when reading) leaves edges used once: the shell is open.
        if is_closed(&ok) {
            let sh = d.add(format!("CLOSED_SHELL('',{})", list(&ids)));
            w.breps.push(d.add(format!("MANIFOLD_SOLID_BREP('Solid {index}',#{sh})")));
        } else {
            let sh = d.add(format!("OPEN_SHELL('',{})", list(&ids)));
            w.surfaces.push(d.add(format!("SHELL_BASED_SURFACE_MODEL('Solid {index}',(#{sh}))")));
            w.warnings.push(format!("solid {index}: its faces do not close a volume, so it was written as surfaces"));
        }
    }
    w
}

fn one_face(
    d: &mut Data,
    f: &Face,
    edges: &BTreeMap<i64, EdgeGeom>,
    vertices: &mut HashMap<i64, usize>,
    curves: &mut HashMap<i64, usize>,
) -> Result<(usize, &'static str), String> {
    if f.topo.len() != f.loops.len() || f.topo.iter().any(Vec::is_empty) {
        return Err("a face's loops have no edges".into());
    }
    let same = face_sense(f)?;
    // Check every edge before writing anything for the face.
    for &(e, _) in f.topo.iter().flatten() {
        let g = edges.get(&e).ok_or("an edge is missing")?;
        if g.curve.is_none() {
            return Err("an edge without a curve (a cone's apex) is not supported yet".into());
        }
    }
    let (surf, kind) = surface(d, f)?;
    let mut bounds = Vec::new();
    for lp in &f.topo {
        let mut oriented = Vec::new();
        for &(e, rev) in lp {
            let g = edges.get(&e).ok_or("an edge is missing")?;
            let ec = match curves.get(&e) {
                Some(&id) => id,
                None => {
                    let mut vertex = |v: i64, p: V3, d: &mut Data| match vertices.get(&v) {
                        Some(&id) => id,
                        None => {
                            let pt = d.point(p);
                            let id = d.add(format!("VERTEX_POINT('',#{pt})"));
                            vertices.insert(v, id);
                            id
                        }
                    };
                    let v0 = vertex(g.v0, g.p0, d);
                    let v1 = vertex(g.v1, g.p1, d);
                    let (c, sense) = curve(d, g)?;
                    let id = d.add(format!("EDGE_CURVE('',#{v0},#{v1},#{c},{})", flag(sense)));
                    curves.insert(e, id);
                    id
                }
            };
            oriented.push(d.add(format!("ORIENTED_EDGE('',*,*,#{ec},{})", flag(!rev))));
        }
        let lid = d.add(format!("EDGE_LOOP('',{})", list(&oriented)));
        bounds.push(d.add(format!("FACE_BOUND('',#{lid},.T.)")));
    }
    Ok((d.add(format!("ADVANCED_FACE('',{},#{surf},{})", list(&bounds), flag(same))), kind))
}

/// The whole file, from each solid's faces and edges. `date` is ISO 8601.
pub(crate) fn write(solids: &[(Vec<Face>, BTreeMap<i64, EdgeGeom>)], file_name: &str, source: &str, date: &str, tol: f64) -> (String, Written) {
    let mut d = Data::default();
    let mut all = Written::default();
    for (i, (faces, edges)) in solids.iter().enumerate() {
        let w = solid(&mut d, i + 1, faces, edges);
        all.breps.extend(w.breps);
        all.surfaces.extend(w.surfaces);
        for (k, n) in w.faces {
            *all.faces.entry(k).or_default() += n;
        }
        all.warnings.extend(w.warnings);
    }
    let product = std::path::Path::new(file_name).file_stem().and_then(|s| s.to_str()).unwrap_or("part").to_string();
    // Units (millimetres, radians), accuracy and the representation of the solids.
    let mm = d.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))");
    let rad = d.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))");
    let sr = d.add("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())");
    let unc = d.add(format!("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE({}),#{mm},'distance_accuracy_value','edge chord tolerance')", real(tol)));
    let ctx = d.add(format!(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{unc}))GLOBAL_UNIT_ASSIGNED_CONTEXT((#{mm},#{rad},#{sr}))REPRESENTATION_CONTEXT('Context','3D'))"
    ));
    let origin = d.placement(V3::default(), V3::new(0.0, 0.0, 1.0), V3::new(1.0, 0.0, 0.0)).unwrap_or(0);
    let mut items = vec![origin];
    items.extend(&all.breps);
    let rep = d.add(format!("ADVANCED_BREP_SHAPE_REPRESENTATION({},{},#{ctx})", text(&product), list(&items)));
    if !all.surfaces.is_empty() {
        let mut items = vec![origin];
        items.extend(&all.surfaces);
        let srf = d.add(format!("MANIFOLD_SURFACE_SHAPE_REPRESENTATION('surfaces',{},#{ctx})", list(&items)));
        d.add(format!("SHAPE_REPRESENTATION_RELATIONSHIP('','',#{rep},#{srf})"));
    }
    // The product the shape belongs to.
    let app = d.add("APPLICATION_CONTEXT('automotive design')");
    d.add(format!("APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#{app})"));
    let pc = d.add(format!("PRODUCT_CONTEXT('',#{app},'mechanical')"));
    let prod = d.add(format!("PRODUCT({0},{0},{1},(#{pc}))", text(&product), text(&format!("3D solids of {source}"))));
    d.add(format!("PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#{prod}))"));
    let pdf = d.add(format!("PRODUCT_DEFINITION_FORMATION('','',#{prod})"));
    let pdc = d.add(format!("PRODUCT_DEFINITION_CONTEXT('part definition',#{app},'design')"));
    let pd = d.add(format!("PRODUCT_DEFINITION('design','',#{pdf},#{pdc})"));
    let pds = d.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{pd})"));
    d.add(format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{rep})"));

    let mut out = String::from("ISO-10303-21;\nHEADER;\n");
    out.push_str(&format!("FILE_DESCRIPTION(({}),'2;1');\n", text(&format!("3D solids of {source}"))));
    out.push_str(&format!(
        "FILE_NAME({},{},(''),(''),{},'CADCraft','');\n",
        text(file_name),
        text(date),
        text(&format!("CADCraft {}", env!("CARGO_PKG_VERSION")))
    ));
    out.push_str("FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n");
    for l in &d.lines {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    (out, all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reals_always_have_a_decimal_point() {
        for (x, s) in [(1.0, "1.0"), (-2.5, "-2.5"), (1e-5, "1.E-5"), (0.0, "0."), (1.5e20, "1.5E20")] {
            assert_eq!(real(x), s);
        }
        assert_eq!(text("it's a\\b è"), "'it''s a\\\\b _'");
    }

    /// A solid written as SAT text from its faces: each face is a surface, a sense and loops
    /// of (edge, reversed); edges are lines or circles between vertices.
    enum Crv {
        Line,
        /// Centre, normal, radius vector (the circle's start, from the centre).
        Circle(V3, V3, V3),
    }
    enum Srf {
        Plane(V3, V3, V3),
        /// Centre, axis, radius vector.
        Cyl(V3, V3, V3),
    }
    struct Model {
        points: Vec<V3>,
        edges: Vec<(usize, usize, Crv)>,
        faces: Vec<(Srf, bool, Vec<Vec<(usize, bool)>>)>,
    }

    fn sat(m: &Model) -> String {
        let nf = m.faces.len();
        let nl: usize = m.faces.iter().map(|f| f.2.len()).sum();
        let nc: usize = m.faces.iter().flat_map(|f| f.2.iter()).map(Vec::len).sum();
        let (ne, nv) = (m.edges.len(), m.points.len());
        // body, lump, shell, faces, loops, coedges, surfaces, edges, curves, vertices, points.
        let (f0, l0) = (3, 3 + nf);
        let c0 = l0 + nl;
        let s0 = c0 + nc;
        let e0 = s0 + nf;
        let k0 = e0 + ne;
        let v0 = k0 + ne;
        let p0 = v0 + nv;
        let v = |p: V3| format!("{} {} {}", p.x, p.y, p.z);
        let mut t = String::from("700 0 1 0\n@4 test\n1 1e-06 1e-10\n");
        t += "body $-1 -1 $-1 $1 $-1 $-1 #\nlump $-1 -1 $-1 $-1 $2 $0 #\n";
        t += &format!("shell $-1 -1 $-1 $-1 $-1 ${f0} $-1 $1 #\n");
        let mut li = l0;
        for (i, f) in m.faces.iter().enumerate() {
            let next = if i + 1 < nf { (f0 + i + 1) as i64 } else { -1 };
            t += &format!("face $-1 -1 $-1 ${next} ${li} $2 $-1 ${} {} single #\n", s0 + i, if f.1 { "forward" } else { "reversed" });
            li += f.2.len();
        }
        let mut ci = c0;
        let mut li = l0;
        let mut co = Vec::new();
        for (i, f) in m.faces.iter().enumerate() {
            for (k, lp) in f.2.iter().enumerate() {
                let next = if k + 1 < f.2.len() { (li + 1) as i64 } else { -1 };
                t += &format!("loop $-1 -1 $-1 ${next} ${ci} ${} #\n", f0 + i);
                for (j, &(e, rev)) in lp.iter().enumerate() {
                    let n = ci + (j + 1) % lp.len();
                    let pr = ci + (j + lp.len() - 1) % lp.len();
                    co.push(format!("coedge $-1 -1 $-1 ${n} ${pr} $-1 ${} {} ${li} $-1 #\n", e0 + e, if rev { "reversed" } else { "forward" }));
                }
                ci += lp.len();
                li += 1;
            }
        }
        t += &co.concat();
        for f in &m.faces {
            t += &match f.0 {
                Srf::Plane(o, n, e) => format!("plane-surface $-1 -1 $-1 {} {} {} forward_v I I I I #\n", v(o), v(n), v(e)),
                Srf::Cyl(c, n, r) => format!("cone-surface $-1 -1 $-1 {} {} {} 1 I I 0 1 {} forward I I I I #\n", v(c), v(n), v(r), r.len()),
            };
        }
        for (i, (a, b, c)) in m.edges.iter().enumerate() {
            let t1 = match c {
                Crv::Line => m.points[*a].dist(m.points[*b]),
                Crv::Circle(..) => TAU,
            };
            t += &format!("edge $-1 -1 $-1 ${} 0 ${} {t1} $-1 ${} forward @7 unknown #\n", v0 + a, v0 + b, k0 + i);
        }
        for (a, b, c) in &m.edges {
            t += &match c {
                Crv::Line => format!("straight-curve $-1 -1 $-1 {} {} I I #\n", v(m.points[*a]), v((m.points[*b] - m.points[*a]).unit().unwrap())),
                Crv::Circle(c, n, r) => format!("ellipse-curve $-1 -1 $-1 {} {} {} 1 I I #\n", v(*c), v(*n), v(*r)),
            };
        }
        for i in 0..nv {
            let e = m.edges.iter().position(|x| x.0 == i || x.1 == i).unwrap();
            t += &format!("vertex $-1 -1 $-1 ${} ${} #\n", e0 + e, p0 + i);
        }
        for p in &m.points {
            t += &format!("point $-1 -1 $-1 {} #\n", v(*p));
        }
        t + "End-of-ACIS-data\n"
    }

    /// A 10 x 20 x 30 box, each face's loop counter-clockwise seen from outside.
    fn cube() -> Model {
        let points: Vec<V3> = (0..8).map(|i| V3::new(10.0 * (i & 1) as f64, 20.0 * ((i >> 1) & 1) as f64, 30.0 * (i >> 2) as f64)).collect();
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let mut edges: Vec<(usize, usize, Crv)> = Vec::new();
        let mut faces = Vec::new();
        for q in quads {
            let mut lp = Vec::new();
            for k in 0..4 {
                let (a, b) = (q[k], q[(k + 1) % 4]);
                let e = match edges.iter().position(|x| (x.0, x.1) == (a, b) || (x.0, x.1) == (b, a)) {
                    Some(e) => e,
                    None => {
                        edges.push((a, b, Crv::Line));
                        edges.len() - 1
                    }
                };
                lp.push((e, edges[e].0 != a));
            }
            let (pa, pb, pc) = (points[q[0]], points[q[1]], points[q[3]]);
            let n = (pb - pa).cross(pc - pa).unit().unwrap();
            // Half the faces keep their normal on the surface, half on a reversed face.
            let flip = q[0] == 0;
            let sn = if flip { -n } else { n };
            faces.push((Srf::Plane(pa, sn, (pb - pa).unit().unwrap()), !flip, vec![lp]));
        }
        Model { points, edges, faces }
    }

    /// A tube: outer radius 30, inner 26, 100 long along z.
    fn tube() -> Model {
        let z = V3::new(0.0, 0.0, 1.0);
        let points = vec![V3::new(30.0, 0.0, 0.0), V3::new(26.0, 0.0, 0.0), V3::new(30.0, 0.0, 100.0), V3::new(26.0, 0.0, 100.0)];
        let edges = points.iter().enumerate().map(|(i, p)| (i, i, Crv::Circle(V3::new(0.0, 0.0, p.z), z, V3::new(p.x, 0.0, 0.0)))).collect();
        let (o, x) = (V3::default(), V3::new(1.0, 0.0, 0.0));
        let faces = vec![
            (Srf::Plane(o, z, x), false, vec![vec![(0, true)], vec![(1, false)]]),
            (Srf::Plane(V3::new(0.0, 0.0, 100.0), z, x), true, vec![vec![(2, false)], vec![(3, true)]]),
            (Srf::Cyl(o, z, V3::new(30.0, 0.0, 0.0)), true, vec![vec![(0, false)], vec![(2, true)]]),
            (Srf::Cyl(o, z, V3::new(26.0, 0.0, 0.0)), false, vec![vec![(1, true)], vec![(3, false)]]),
        ];
        Model { points, edges, faces }
    }

    /// Entity bodies by number, from a written file.
    fn entities(text: &str) -> BTreeMap<usize, String> {
        text.lines()
            .filter_map(|l| l.strip_prefix('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(n, b)| (n.parse().unwrap(), b.trim_end_matches(';').to_string()))
            .collect()
    }

    fn refs(body: &str) -> Vec<usize> {
        body.split('#').skip(1).map(|s| s.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap()).collect()
    }

    /// The file is complete and the solid closed: every reference resolves, every edge is used
    /// by two faces once each way, and every face's normal points out of the material.
    fn check(m: &Model, outward: impl Fn(V3, V3, bool) -> bool) -> String {
        let c = crate::step_from_sat(&[sat(m)], "part.stp", "part.dwg", "2026-01-01T00:00:00", 0.01).unwrap();
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!((c.closed, c.open), (1, 0));
        assert_eq!(c.face_count(), m.faces.len());
        let ents = entities(&c.step);
        for b in ents.values() {
            for r in refs(b) {
                assert!(ents.contains_key(&r), "#{r} in {b}");
            }
        }
        let mut uses: HashMap<usize, Vec<bool>> = HashMap::new();
        for b in ents.values().filter(|b| b.starts_with("ORIENTED_EDGE")) {
            uses.entry(refs(b)[0]).or_default().push(b.ends_with(".T.)"));
        }
        assert_eq!(uses.len(), m.edges.len());
        assert!(uses.values().all(|u| u.len() == 2 && u[0] != u[1]), "{uses:?}");
        // Each face of the SAT data against the sense written for it.
        let s = crate::sat::parse(&sat(m)).unwrap();
        let (faces, _) = crate::brep::faces(&s, 0.01);
        for f in &faces {
            let same = face_sense(f).unwrap();
            let p = f.loops[0][0];
            let n = match f.surface {
                Surface::Plane { n, .. } => n,
                Surface::Cone { c, n, .. } => {
                    let d = p - c;
                    d - n * d.dot(n)
                }
            };
            let plane = matches!(f.surface, Surface::Plane { .. });
            assert!(outward(p, if same { n } else { -n }, plane), "face at {p:?}");
        }
        c.step
    }

    #[test]
    fn a_box_is_a_closed_solid_with_outward_faces() {
        let text = check(&cube(), |p, n, _| (p - V3::new(5.0, 10.0, 15.0)).dot(n) > 0.0);
        assert!(text.starts_with("ISO-10303-21;"));
        assert!(text.trim_end().ends_with("END-ISO-10303-21;"));
        assert_eq!(text.matches("PLANE(").count(), 6);
        assert_eq!(text.matches("LINE(").count(), 12);
        assert!(text.contains("MANIFOLD_SOLID_BREP") && text.contains("CLOSED_SHELL"));
    }

    #[test]
    fn a_tube_keeps_exact_cylinders_and_circles() {
        // Outward: away from the axis on the outer wall, towards it on the inner one, along z
        // on the ends.
        let text = check(&tube(), |p, n, plane| {
            if plane {
                if p.z < 50.0 { n.z < 0.0 } else { n.z > 0.0 }
            } else {
                let r = V3::new(p.x, p.y, 0.0);
                if r.len() > 28.0 { r.dot(n) > 0.0 } else { r.dot(n) < 0.0 }
            }
        });
        assert_eq!(text.matches("CYLINDRICAL_SURFACE(").count(), 2);
        assert_eq!(text.matches("CIRCLE(").count(), 4);
    }

    /// A face left out leaves the others as surfaces, and says so.
    #[test]
    fn an_open_shell_is_written_as_surfaces() {
        let mut m = cube();
        m.faces.pop();
        let c = crate::step_from_sat(&[sat(&m)], "part.stp", "part.dwg", "2026-01-01T00:00:00", 0.01).unwrap();
        assert_eq!((c.closed, c.open), (0, 1));
        assert!(c.step.contains("SHELL_BASED_SURFACE_MODEL") && c.step.contains("SHAPE_REPRESENTATION_RELATIONSHIP"));
        assert!(!c.step.contains("MANIFOLD_SOLID_BREP"));
        assert!(c.summary().contains("not closed"), "{}", c.summary());
    }
}
