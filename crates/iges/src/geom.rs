//! Vectors, the curves an ACIS edge can lie on (lines, ellipses, B-splines) and the surfaces a
//! face can lie on (planes, cylinders and cones), with the parameterisation used for trimming.

use std::f64::consts::{PI, TAU};
use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl V3 {
    pub const fn new(x: f64, y: f64, z: f64) -> V3 {
        V3 { x, y, z }
    }
    pub fn from(a: [f64; 3]) -> V3 {
        V3::new(a[0], a[1], a[2])
    }
    pub fn dot(self, o: V3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: V3) -> V3 {
        V3::new(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    pub fn len(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn unit(self) -> Option<V3> {
        let l = self.len();
        (l > 1e-12 && l.is_finite()).then(|| self * (1.0 / l))
    }
    pub fn dist(self, o: V3) -> f64 {
        (self - o).len()
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f64> for V3 {
    type Output = V3;
    fn mul(self, k: f64) -> V3 {
        V3::new(self.x * k, self.y * k, self.z * k)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        self * -1.0
    }
}

/// An affine transform `p' = (R p) * scale + t` (the body's placement).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xform {
    pub r: [[f64; 3]; 3],
    pub t: V3,
    pub scale: f64,
}

impl Default for Xform {
    fn default() -> Self {
        Xform { r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: V3::default(), scale: 1.0 }
    }
}

impl Xform {
    /// ACIS stores the three rows of the rotation as the images of the axes (row vectors).
    pub fn point(&self, p: V3) -> V3 {
        self.dir(p) * self.scale + self.t
    }
    pub fn dir(&self, p: V3) -> V3 {
        let r = &self.r;
        V3::new(
            p.x * r[0][0] + p.y * r[1][0] + p.z * r[2][0],
            p.x * r[0][1] + p.y * r[1][1] + p.z * r[2][1],
            p.x * r[0][2] + p.y * r[1][2] + p.z * r[2][2],
        )
    }
}

/// A B-spline with a clamped knot vector (`ctrl.len() + degree + 1` knots).
#[derive(Clone, Debug, PartialEq)]
pub struct BSpline {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub ctrl: Vec<V3>,
    pub weights: Option<Vec<f64>>,
}

impl BSpline {
    pub fn domain(&self) -> (f64, f64) {
        let d = self.degree;
        (self.knots.get(d).copied().unwrap_or(0.0), self.knots.get(self.ctrl.len()).copied().unwrap_or(0.0))
    }
    /// De Boor's algorithm (homogeneous for a rational spline).
    pub fn eval(&self, t: f64) -> Option<V3> {
        let p = self.degree;
        let n = self.ctrl.len();
        let (a, b) = self.domain();
        let t = t.clamp(a, b);
        // The span: knots[k] <= t < knots[k+1], within p..n-1.
        let mut k = p;
        while k + 1 < n && self.knots.get(k + 1).is_some_and(|&kn| t >= kn) {
            k += 1;
        }
        let w = |i: usize| self.weights.as_ref().and_then(|w| w.get(i).copied()).unwrap_or(1.0);
        let mut d: Vec<(V3, f64)> = (0..=p)
            .map(|j| {
                let i = k + j - p;
                let c = self.ctrl.get(i).copied().unwrap_or_default();
                (c * w(i), w(i))
            })
            .collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i = k + j - p;
                let (k0, k1) = (*self.knots.get(i)?, *self.knots.get(i + p + 1 - r)?);
                let alpha = if k1 - k0 == 0.0 { 0.0 } else { (t - k0) / (k1 - k0) };
                let (prev, cur) = (d[j - 1], d[j]);
                d[j] = (prev.0 * (1.0 - alpha) + cur.0 * alpha, prev.1 * (1.0 - alpha) + cur.1 * alpha);
            }
        }
        let (pt, wt) = d[p];
        (wt.abs() > 1e-300).then(|| pt * (1.0 / wt))
    }
}

/// A curve an edge lies on.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Line {
        root: V3,
        dir: V3,
    },
    /// `center + major cos t + minor sin t`, `minor = ratio · (normal × major)`.
    Ellipse {
        center: V3,
        major: V3,
        minor: V3,
    },
    Spline(BSpline),
}

impl Curve {
    pub fn eval(&self, t: f64) -> Option<V3> {
        match self {
            Curve::Line { root, dir } => Some(*root + *dir * t),
            Curve::Ellipse { center, major, minor } => Some(*center + *major * t.cos() + *minor * t.sin()),
            Curve::Spline(s) => {
                let (a, b) = s.domain();
                let period = b - a;
                // A periodic spline's parameter may run past its domain.
                let t = if period > 0.0 && (t < a - 1e-9 || t > b + 1e-9) { a + (t - a).rem_euclid(period) } else { t };
                s.eval(t)
            }
        }
    }
    /// How many samples keep the chord within `tol` over the parameter range `span`.
    pub fn samples(&self, span: f64, tol: f64) -> usize {
        let n = match self {
            Curve::Line { .. } => 1.0,
            Curve::Ellipse { major, .. } => {
                let r = major.len().max(1e-9);
                let step = 2.0 * (2.0 * tol / r).sqrt().min(PI / 8.0);
                span.abs() / step
            }
            Curve::Spline(s) => {
                let (a, b) = s.domain();
                let spans = s.knots.windows(2).filter(|w| w[1] > w[0]).count().max(1) as f64;
                let frac = if b > a { (span.abs() / (b - a)).min(1.0) } else { 1.0 };
                spans * 16.0 * frac
            }
        };
        (n.ceil() as usize).clamp(1, 20_000)
    }
}

/// A surface a face lies on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Surface {
    Plane {
        o: V3,
        n: V3,
        e1: V3,
        e2: V3,
    },
    /// A right circular cylinder or cone: axis `n` through `c`, angle measured from `e1` towards
    /// `e2`, radius `r0 + h · tan` at height `h` along the axis.
    Cone {
        c: V3,
        n: V3,
        e1: V3,
        e2: V3,
        r0: f64,
        tan: f64,
    },
}

impl Surface {
    /// (s, t) on a plane, (angle, height) on a cone.
    pub fn param(&self, p: V3) -> (f64, f64) {
        match *self {
            Surface::Plane { o, e1, e2, .. } => ((p - o).dot(e1), (p - o).dot(e2)),
            Surface::Cone { c, n, e1, e2, .. } => {
                let d = p - c;
                (d.dot(e2).atan2(d.dot(e1)), d.dot(n))
            }
        }
    }
    pub fn eval(&self, u: f64, v: f64) -> V3 {
        match *self {
            Surface::Plane { o, e1, e2, .. } => o + e1 * u + e2 * v,
            Surface::Cone { c, n, e1, e2, r0, tan } => c + n * v + (e1 * u.cos() + e2 * u.sin()) * (r0 + v * tan),
        }
    }
    /// Distance from `p` to the surface.
    pub fn residual(&self, p: V3) -> f64 {
        match *self {
            Surface::Plane { o, n, .. } => (p - o).dot(n).abs(),
            Surface::Cone { c, n, r0, tan, .. } => {
                let d = p - c;
                let h = d.dot(n);
                let radial = (d - n * h).len();
                (radial - (r0 + h * tan)).abs() / (1.0 + tan * tan).sqrt()
            }
        }
    }
}

/// The angle `phi` (0..2π from the seam) as the parameter of the rational quadratic circle of
/// nine control points whose knots are 0, π/2, π, 3π/2, 2π: inside each quarter the parameter
/// is not proportional to the angle, so it is found by bisection.
pub fn circle_param(phi: f64) -> f64 {
    let phi = phi.clamp(0.0, TAU);
    let q = ((phi / (PI / 2.0)).floor() as usize).min(3);
    let psi = phi - q as f64 * PI / 2.0;
    let w = std::f64::consts::FRAC_1_SQRT_2;
    let angle = |s: f64| {
        let (b0, b1, b2) = ((1.0 - s) * (1.0 - s), 2.0 * s * (1.0 - s) * w, s * s);
        // P0 = (1, 0), P1 = (1, 1), P2 = (0, 1).
        let (x, y) = (b0 + b1, b1 + b2);
        y.atan2(x)
    };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if angle(mid) < psi {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (q as f64 + (lo + hi) / 2.0) * PI / 2.0
}
