//! An IGES 5.3 writer: Start, Global, Directory Entry and Parameter Data sections in fixed
//! 80-column records, with the entities a trimmed-surface model needs: 126 (B-spline curve),
//! 128 (B-spline surface), 142 (curve on a parametric surface) and 144 (trimmed surface).

use crate::geom::V3;
use crate::trim::{Boundary, Nurbs, Trimmed};

pub struct Iges {
    de: Vec<String>,
    pd: Vec<String>,
    /// Bounding box of what was written, for the Global section's maximum coordinate.
    max_coord: f64,
}

/// A real in IGES form: always with a decimal point, exponent as `E`.
pub fn real(x: f64) -> String {
    if !x.is_finite() || x.abs() < 1e-14 {
        return "0.".into();
    }
    let s = format!("{x:?}").replace('e', "E");
    if s.contains('.') || s.contains('E') { s } else { format!("{s}.") }
}

/// A string field; IGES text is ASCII, so anything else becomes `_`. Empty takes the default.
fn hollerith(s: &str) -> String {
    let s: String = s.chars().map(|c| if c.is_ascii() && !c.is_ascii_control() { c } else { '_' }).collect();
    if s.is_empty() { String::new() } else { format!("{}H{}", s.len(), s) }
}

impl Iges {
    pub fn new() -> Self {
        Iges { de: Vec::new(), pd: Vec::new(), max_coord: 1.0 }
    }

    /// Adds an entity; returns its Directory Entry pointer (the sequence number of its first
    /// DE line).
    fn add(&mut self, etype: u32, form: u32, status: &str, params: Vec<String>) -> usize {
        let de_ptr = self.de.len() + 1;
        let first_pd = self.pd.len() + 1;
        let mut line = String::new();
        let mut lines = Vec::new();
        let count = params.len();
        for (i, p) in std::iter::once(etype.to_string()).chain(params).enumerate() {
            let tok = format!("{p}{}", if i == count { ';' } else { ',' });
            if !line.is_empty() && line.len() + tok.len() > 64 {
                lines.push(std::mem::take(&mut line));
            }
            // A token longer than a line (it cannot be here: reals are short) is still written.
            line.push_str(&tok);
        }
        if !line.is_empty() {
            lines.push(line);
        }
        let nlines = lines.len();
        for l in lines {
            let seq = self.pd.len() + 1;
            self.pd.push(format!("{l:<64} {de_ptr:>7}P{seq:>7}"));
        }
        let d1 = format!("{etype:>8}{first_pd:>8}{:>8}{:>8}{:>8}{:>8}{:>8}{:>8}{status:>8}D{:>7}", 0, 0, 0, 0, 0, 0, de_ptr);
        let d2 = format!("{etype:>8}{:>8}{:>8}{nlines:>8}{form:>8}{:>8}{:>8}{:>8}{:>8}D{:>7}", 0, 0, "", "", "", 0, de_ptr + 1);
        self.de.push(d1);
        self.de.push(d2);
        de_ptr
    }

    fn track(&mut self, p: V3) {
        self.max_coord = self.max_coord.max(p.x.abs()).max(p.y.abs()).max(p.z.abs());
    }

    /// A degree-1 B-spline through points (closed when the last repeats the first).
    fn polyline(&mut self, pts: &[V3], planar: bool, status: &str) -> usize {
        let n = pts.len();
        let k = n.saturating_sub(1);
        let mut p = vec![k.to_string(), "1".into(), (planar as u8).to_string(), "1".into(), "1".into(), "0".into()];
        // Knots: 0, 0, 1, …, k-1, k, k.
        p.push(real(0.0));
        for i in 0..=k {
            p.push(real(i as f64));
        }
        p.push(real(k as f64));
        p.extend(std::iter::repeat_n(real(1.0), n));
        for q in pts {
            p.extend([real(q.x), real(q.y), real(q.z)]);
        }
        p.extend([real(0.0), real(k as f64)]);
        p.extend(if planar { [real(0.0), real(0.0), real(1.0)] } else { [real(0.0), real(0.0), real(0.0)] });
        for q in pts {
            self.track(*q);
        }
        self.add(126, 0, status, p)
    }

    fn surface(&mut self, s: &Nurbs) -> usize {
        let (k1, k2) = (s.count.0 - 1, s.count.1 - 1);
        let mut p = vec![k1.to_string(), k2.to_string(), s.deg.0.to_string(), s.deg.1.to_string()];
        p.extend(["0", "0", if s.rational { "0" } else { "1" }, "0", "0"].map(String::from));
        p.extend(s.knots_u.iter().map(|x| real(*x)));
        p.extend(s.knots_v.iter().map(|x| real(*x)));
        p.extend(s.weights.iter().map(|x| real(*x)));
        for q in &s.ctrl {
            p.extend([real(q.x), real(q.y), real(q.z)]);
            self.track(*q);
        }
        p.extend([real(s.range.0), real(s.range.1), real(s.range.2), real(s.range.3)]);
        self.add(128, 0, "00010000", p)
    }

    fn boundary(&mut self, surf: usize, b: &Boundary) -> usize {
        let uv: Vec<V3> = b.uv.iter().map(|&(u, v)| V3::new(u, v, 0.0)).collect();
        let pc = self.polyline(&uv, true, "00010500");
        let mc = self.polyline(&b.xyz, false, "00010000");
        self.add(142, 0, "00010000", vec!["1".into(), surf.to_string(), pc.to_string(), mc.to_string(), "1".into()])
    }

    pub fn trimmed(&mut self, t: &Trimmed) {
        let s = self.surface(&t.surf);
        let outer = self.boundary(s, &t.outer);
        let inner: Vec<usize> = t.inner.iter().map(|b| self.boundary(s, b)).collect();
        let mut p = vec![s.to_string(), "1".into(), inner.len().to_string(), outer.to_string()];
        p.extend(inner.iter().map(usize::to_string));
        self.add(144, 0, "00000000", p);
    }

    /// The whole file. `date` is `YYYYMMDD.HHNNSS`.
    pub fn finish(&self, file_name: &str, date: &str, source: &str) -> String {
        let mut out = String::new();
        let start = [format!("IGES 5.3 trimmed surfaces of the 3D solids in {source}"), "Written by dwg2iges (CADCraft tools).".into()];
        for (i, l) in start.iter().enumerate() {
            let l: String = l.chars().take(72).collect();
            out.push_str(&format!("{l:<72}S{:>7}\n", i + 1));
        }
        let g = [
            hollerith(","),
            hollerith(";"),
            hollerith(source),
            hollerith(file_name),
            hollerith("dwg2iges"),
            hollerith(env!("CARGO_PKG_VERSION")),
            "32".into(),
            "38".into(),
            "6".into(),
            "308".into(),
            "15".into(),
            hollerith(source),
            real(1.0),
            "2".into(),
            hollerith("MM"),
            "1".into(),
            real(1.0),
            hollerith(date),
            real(0.001),
            real((self.max_coord * 1.1).ceil()),
            hollerith(""),
            hollerith(""),
            "11".into(),
            "0".into(),
            hollerith(date),
        ];
        let mut gl = Vec::new();
        let mut line = String::new();
        for (i, f) in g.iter().enumerate() {
            let tok = format!("{f}{}", if i + 1 == g.len() { ';' } else { ',' });
            if !line.is_empty() && line.len() + tok.len() > 72 {
                gl.push(std::mem::take(&mut line));
            }
            line.push_str(&tok);
        }
        gl.push(line);
        for (i, l) in gl.iter().enumerate() {
            out.push_str(&format!("{l:<72}G{:>7}\n", i + 1));
        }
        for l in &self.de {
            out.push_str(l);
            out.push('\n');
        }
        for l in &self.pd {
            out.push_str(l);
            out.push('\n');
        }
        let t = format!("S{:>7}G{:>7}D{:>7}P{:>7}", start.len(), gl.len(), self.de.len(), self.pd.len());
        out.push_str(&format!("{t:<72}T{:>7}\n", 1));
        out
    }
}
