//! Structural steel: STEELSECTION draws the cross-section of a commercial profile (IPE, HEA,
//! HEB, HEM, UPN, UPE, angles, square/rectangular/circular hollow sections, flats, rounds and
//! squares) with its real root and corner radii; STEELINFO reports its properties and weight;
//! STEELLIST lists the catalogue; CUTLIST nests the pieces to cut into commercial bar lengths;
//! BEAMCHECK runs a quick elastic check of a single-span beam. Dimensions are in millimetres.

use cadcraft_doc::{EntityKind, HAlign, Text, VAlign};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::helpers::{circle, lwpoly, rect_vertices, v3};
use super::machines::number;
use super::{CommandSpec, always, bad, bool_or, f64_or, point_param, point_req, str_param};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

pub mod beam;
pub mod catalog;
pub mod cutlist;
pub mod section;

use catalog::Profile;
use section::{Loop, Props};

/// A one-word answer (a designation, `1250*4`): a space ends it, at the command line and in
/// scripts, as it does for a number, so `CUTLIST 6000 3 1250*4 2400*2` is four inputs.
const WORD: Accept = Accept { number: true, ..Accept::TEXT };

/// The steel commands' specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("steelsection", "Steel Section", run_section)
            .menu(&["Draw", "Steel", "Section..."])
            .alias(&["profilo", "steel"])
            .params(
                "{profile: IPE200 | HEA/HEB/HEM<n> | UPN/UPE<n> | L<a>x<t> | L<a>x<b>x<t> | SHS<a>x<t> | RHS<h>x<b>x<t> | CHS<d>x<t> | FL<b>x<t> | RD<d> | SQ<a>, \
                 at: [x,y], rotation?: degrees, base?: center (default)|centroid|bottomleft|bottomcenter, hot?: bool (hot-finished tube radii), label?: bool} \
                 → {handles, properties} (millimetres)",
            )
            .interactive(|s| Ok(Box::new(SectionM::new(s)))),
        CommandSpec::new("steelinfo", "Steel Section Properties", run_info)
            .menu(&["Tools", "Steel", "Section Properties"])
            .alias(&["peso", "steelweight"])
            .params("{profile, length?: mm, quantity?: n, pricePerKg?: number, hot?: bool} → properties (cm², kg/m, cm⁴, cm³) and the total weight and cost")
            .enabled(always)
            .noundo()
            .interactive(|s| Ok(Box::new(InfoM::new(s)))),
        CommandSpec::new("steellist", "Steel Profile List", run_list)
            .menu(&["Tools", "Steel", "Profile List"])
            .params("{family?: IPE|HEA|HEB|HEM|UPN|UPE|L|SHS|RHS|CHS|FL|RD|SQ} → {families: [{family, description, sizes}]}")
            .enabled(always)
            .noundo(),
        CommandSpec::new("cutlist", "Cut List", run_cutlist)
            .menu(&["Tools", "Steel", "Cut List"])
            .alias(&["distinta", "nesting"])
            .params(
                "{pieces: [{length, quantity?, label?}] | [length, ...], stock?: mm (default 6000), kerf?: mm (default 2), trim?: mm head cut (default 0), \
                 profile?: designation (adds weights), pricePerKg?, at?: [x,y] (draws the bar diagram there), rowHeight?: mm (default 60)} \
                 → {bars, barCount, yieldPercent, offcut_mm, ...}",
            )
            .enabled(always)
            .interactive(|_| Ok(Box::new(CutM::default()))),
        CommandSpec::new("beamcheck", "Beam Check", run_beam)
            .menu(&["Tools", "Steel", "Beam Check"])
            .alias(&["trave", "verificatrave"])
            .params(
                "{profile, span: mm, support?: simple (default)|cantilever|fixed, q?: kN/m uniform load (default 0), p?: kN point load at mid-span or tip (default 0), \
                 grade?: S235|S275 (default)|S355, axis?: strong (default)|weak, limit?: span/limit deflection (default 250), selfWeight?: bool (default true), \
                 suggest?: bool (find the lightest size of the family that passes)} → stresses, deflection, ratios, ok",
            )
            .enabled(always)
            .noundo()
            .interactive(|s| Ok(Box::new(BeamM::new(s)))),
    ]
}

fn profile_param(cmd: &str, p: &Value) -> Result<Profile> {
    let d = str_param(p, "profile").ok_or_else(|| bad(cmd, "`profile` is required, e.g. \"IPE200\""))?;
    catalog::parse(d, bool_or(p, "hot", false)).map_err(|e| bad(cmd, e))
}

fn analyse(cmd: &str, prof: &Profile) -> Result<(Vec<Loop>, Props)> {
    let loops = section::outline(prof).map_err(|e| bad(cmd, e))?;
    let pr = section::props(&loops);
    if pr.area <= 0.0 || !pr.area.is_finite() {
        return Err(bad(cmd, "the section has no area"));
    }
    Ok((loops, pr))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Base {
    #[default]
    Center,
    Centroid,
    BottomLeft,
    BottomCenter,
}

impl Base {
    fn parse(s: &str) -> Option<Base> {
        match s.trim().to_ascii_lowercase().as_str() {
            "center" | "centre" | "c" => Some(Base::Center),
            "centroid" | "g" | "baricentro" => Some(Base::Centroid),
            "bottomleft" | "bl" => Some(Base::BottomLeft),
            "bottomcenter" | "bc" => Some(Base::BottomCenter),
            _ => None,
        }
    }
    fn point(self, pr: &Props) -> Vec2 {
        match self {
            Base::Center => Vec2::new(pr.width / 2.0, pr.height / 2.0),
            Base::Centroid => Vec2::new(pr.cx, pr.cy),
            Base::BottomLeft => Vec2::new(0.0, 0.0),
            Base::BottomCenter => Vec2::new(pr.width / 2.0, 0.0),
        }
    }
}

/// The entities of a section placed at `at`, rotated by `rot` radians about it.
fn placed(prof: &Profile, loops: &[Loop], pr: &Props, at: Vec2, rot: f64, base: Base, label: bool) -> Vec<EntityKind> {
    let b = base.point(pr);
    let f = move |v: Vec2| at + (v - b).rotate(rot);
    let mut out: Vec<EntityKind> = loops
        .iter()
        .map(|l| match l.map(&f) {
            Loop::Poly { verts, .. } => lwpoly(verts, true),
            Loop::Circle { center, r, .. } => circle(center, r),
        })
        .collect();
    if label {
        let height = (pr.width.max(pr.height) * 0.12).max(2.5);
        let p = f(Vec2::new(pr.width / 2.0, -height * 0.8));
        out.push(EntityKind::Text(Text {
            insert: v3(p),
            align_pt: Some(v3(p)),
            height,
            value: prof.name.clone(),
            rotation: rot,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: HAlign::Center,
            valign: VAlign::Top,
        }));
    }
    out
}

fn run_section(s: &mut Session, p: &Value) -> Result<Value> {
    let prof = profile_param("steelsection", p)?;
    let at = point_req("steelsection", p, "at")?;
    let rot = f64_or(p, "rotation", 0.0).to_radians();
    let base = match str_param(p, "base") {
        None => Base::Center,
        Some(b) => Base::parse(b).ok_or_else(|| bad("steelsection", "base is center, centroid, bottomleft or bottomcenter"))?,
    };
    let (loops, pr) = analyse("steelsection", &prof)?;
    let mut handles = Vec::new();
    for k in placed(&prof, &loops, &pr, at, rot, base, bool_or(p, "label", false)) {
        handles.push(s.add_entity(k)?.hex());
    }
    Ok(json!({ "handles": handles, "properties": section::props_json(&prof, &pr), "message": section::props_text(&prof, &pr) }))
}

fn info_value(prof: &Profile, pr: &Props, length: f64, qty: f64, price: Option<f64>) -> Value {
    let mut v = section::props_json(prof, pr);
    let mut msg = section::props_text(prof, pr);
    if length > 0.0 {
        let total_m = length / 1000.0 * qty;
        let kg = pr.mass * total_m;
        v["length_mm"] = json!(length);
        v["quantity"] = json!(qty);
        v["totalLength_m"] = json!((total_m * 1000.0).round() / 1000.0);
        v["totalWeight_kg"] = json!((kg * 100.0).round() / 100.0);
        v["paintSurface_m2"] = json!((pr.surface * total_m * 1000.0).round() / 1000.0);
        msg.push_str(&format!("\n{qty} x {length} mm = {total_m:.3} m   weight = {kg:.2} kg   surface = {:.3} m²", pr.surface * total_m));
        if let Some(c) = price {
            v["cost"] = json!((kg * c * 100.0).round() / 100.0);
            msg.push_str(&format!("   cost = {:.2} (at {c}/kg)", kg * c));
        }
    }
    v["message"] = json!(msg);
    v
}

fn run_info(_s: &mut Session, p: &Value) -> Result<Value> {
    let prof = profile_param("steelinfo", p)?;
    let (_, pr) = analyse("steelinfo", &prof)?;
    let length = f64_or(p, "length", 0.0);
    let qty = f64_or(p, "quantity", 1.0);
    if !(0.0..=1e7).contains(&length) || !(0.0..=1e6).contains(&qty) {
        return Err(bad("steelinfo", "length must be 0..10,000,000 mm and quantity 0..1,000,000"));
    }
    let price = p.get("pricePerKg").and_then(Value::as_f64).filter(|c| c.is_finite() && *c >= 0.0);
    Ok(info_value(&prof, &pr, length, qty, price))
}

fn run_list(_s: &mut Session, p: &Value) -> Result<Value> {
    let want = str_param(p, "family").map(|f| f.trim().to_ascii_uppercase());
    let fams: Vec<Value> = catalog::families()
        .into_iter()
        .filter(|(f, _, _)| want.as_deref().is_none_or(|w| w == *f))
        .map(|(f, d, sizes)| json!({ "family": f, "description": d, "sizes": sizes }))
        .collect();
    if fams.is_empty() {
        return Err(bad("steellist", "unknown family"));
    }
    let msg = fams
        .iter()
        .map(|f| {
            let sizes: Vec<&str> = f["sizes"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            format!("{} — {}: {}", f["family"].as_str().unwrap_or(""), f["description"].as_str().unwrap_or(""), sizes.join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(json!({ "families": fams, "message": msg }))
}

// ---------- cut list ----------

fn cut_pieces(p: &Value) -> Result<Vec<cutlist::Piece>> {
    let arr =
        p.get("pieces").and_then(Value::as_array).ok_or_else(|| bad("cutlist", "`pieces` is required: [{length, quantity}] or [length, ...]"))?;
    let mut out = Vec::new();
    for (i, it) in arr.iter().enumerate() {
        let (len, qty, label) = match it {
            Value::Number(n) => (n.as_f64().unwrap_or(f64::NAN), 1u64, None),
            Value::Object(_) => (
                it.get("length").and_then(Value::as_f64).unwrap_or(f64::NAN),
                it.get("quantity").or_else(|| it.get("qty")).and_then(Value::as_u64).unwrap_or(1),
                it.get("label").and_then(Value::as_str).map(str::to_string),
            ),
            _ => return Err(bad("cutlist", format!("piece {} is not a length or {{length, quantity}}", i + 1))),
        };
        if !len.is_finite() || len <= 0.0 {
            return Err(bad("cutlist", format!("piece {} needs a positive length", i + 1)));
        }
        let qty = usize::try_from(qty).unwrap_or(usize::MAX);
        if out.len().saturating_add(qty) > cutlist::MAX_PIECES {
            return Err(bad("cutlist", format!("at most {} pieces", cutlist::MAX_PIECES)));
        }
        let label = label.unwrap_or_else(|| fmt_len(len));
        out.extend((0..qty).map(|_| cutlist::Piece { length: len, label: label.clone() }));
    }
    if out.is_empty() {
        return Err(bad("cutlist", "no pieces"));
    }
    Ok(out)
}

fn fmt_len(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 { format!("{}", x.round() as i64) } else { format!("{x:.1}") }
}

struct CutParams {
    stock: f64,
    kerf: f64,
    trim: f64,
}

fn cut_result(
    s: &mut Session,
    pieces: Vec<cutlist::Piece>,
    cp: &CutParams,
    prof: Option<(Profile, Props)>,
    price: Option<f64>,
    at: Option<(Vec2, f64)>,
) -> Result<Value> {
    let net: f64 = pieces.iter().map(|p| p.length).sum();
    let count = pieces.len();
    let bars = cutlist::nest(pieces, cp.stock, cp.kerf, cp.trim).map_err(|e| bad("cutlist", e))?;
    let gross = cp.stock * bars.len() as f64;
    let yield_pc = if gross > 0.0 { net / gross * 100.0 } else { 0.0 };
    let mut lines = vec![format!(
        "{count} pieces, {:.3} m net, in {} bars of {} mm: yield {yield_pc:.1} %, offcut {:.0} mm",
        net / 1000.0,
        bars.len(),
        fmt_len(cp.stock),
        gross - net
    )];
    let bars_json: Vec<Value> = bars
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let cuts: Vec<&str> = b.pieces.iter().map(|p| p.label.as_str()).collect();
            lines.push(format!("bar {}: {}   offcut {} mm", i + 1, cuts.join(" + "), fmt_len(cp.stock - b.used)));
            json!({ "pieces": b.pieces.iter().map(|p| json!({"length": p.length, "label": p.label})).collect::<Vec<_>>(), "used_mm": b.used, "offcut_mm": cp.stock - b.used })
        })
        .collect();
    let mut v = json!({
        "stock_mm": cp.stock, "kerf_mm": cp.kerf, "trim_mm": cp.trim,
        "pieceCount": count, "barCount": bars.len(), "net_mm": net, "gross_mm": gross,
        "offcut_mm": gross - net, "yieldPercent": (yield_pc * 10.0).round() / 10.0, "bars": bars_json,
    });
    if let Some((prof, pr)) = &prof {
        let (kg_net, kg_gross) = (pr.mass * net / 1000.0, pr.mass * gross / 1000.0);
        v["profile"] = json!(prof.name);
        v["netWeight_kg"] = json!((kg_net * 100.0).round() / 100.0);
        v["grossWeight_kg"] = json!((kg_gross * 100.0).round() / 100.0);
        let mut l = format!("{}: {:.2} kg/m   pieces {kg_net:.2} kg   bars to buy {kg_gross:.2} kg", prof.name, pr.mass);
        if let Some(c) = price {
            v["cost"] = json!((kg_gross * c * 100.0).round() / 100.0);
            l.push_str(&format!("   cost {:.2}", kg_gross * c));
        }
        lines.push(l);
    }
    if let Some((at, row)) = at {
        let mut handles = Vec::new();
        for (i, b) in bars.iter().enumerate().take(500) {
            let y = at.y - i as f64 * row * 1.6;
            handles.push(s.add_entity(lwpoly(rect_vertices(Vec2::new(at.x, y), Vec2::new(at.x + cp.stock, y + row)), true))?.hex());
            let mut x = at.x + cp.trim;
            for piece in &b.pieces {
                let (a, c) = (Vec2::new(x, y), Vec2::new((x + piece.length).min(at.x + cp.stock), y + row));
                handles.push(s.add_entity(lwpoly(rect_vertices(a, c), true))?.hex());
                handles.push(s.add_entity(label_text(a.mid(c), &piece.label, row * 0.35, HAlign::Middle, VAlign::Middle))?.hex());
                x += piece.length + cp.kerf;
            }
            let tag = format!("{}  ({} mm)", i + 1, fmt_len(cp.stock - b.used));
            handles
                .push(s.add_entity(label_text(Vec2::new(at.x - row * 0.3, y + row / 2.0), &tag, row * 0.35, HAlign::Right, VAlign::Middle))?.hex());
        }
        v["handles"] = json!(handles);
    }
    v["message"] = json!(lines.join("\n"));
    Ok(v)
}

fn label_text(at: Vec2, value: &str, height: f64, h: HAlign, va: VAlign) -> EntityKind {
    EntityKind::Text(Text {
        insert: v3(at),
        align_pt: Some(v3(at)),
        height,
        value: value.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: h,
        valign: va,
    })
}

fn run_cutlist(s: &mut Session, p: &Value) -> Result<Value> {
    let pieces = cut_pieces(p)?;
    let cp = CutParams { stock: f64_or(p, "stock", 6000.0), kerf: f64_or(p, "kerf", 2.0), trim: f64_or(p, "trim", 0.0) };
    if !(1.0..=1e6).contains(&cp.stock) || !(0.0..=100.0).contains(&cp.kerf) || !(0.0..cp.stock).contains(&cp.trim) {
        return Err(bad("cutlist", "stock must be 1..1,000,000 mm, kerf 0..100 mm and trim below the stock"));
    }
    let prof = match str_param(p, "profile") {
        Some(_) => {
            let prof = profile_param("cutlist", p)?;
            let (_, pr) = analyse("cutlist", &prof)?;
            Some((prof, pr))
        }
        None => None,
    };
    let price = p.get("pricePerKg").and_then(Value::as_f64).filter(|c| c.is_finite() && *c >= 0.0);
    let at = match point_param(p, "at") {
        Some(a) => {
            s.state()?;
            let row = f64_or(p, "rowHeight", 60.0);
            if !(0.01..=1e5).contains(&row) {
                return Err(bad("cutlist", "rowHeight must be positive"));
            }
            Some((a, row))
        }
        None => None,
    };
    cut_result(s, pieces, &cp, prof, price, at)
}

// ---------- beam check ----------

/// A beam case: span (mm), support, uniform load (kN/m), point load (kN), yield strength,
/// bending axis, deflection limit (span / limit) and whether self-weight is added.
#[derive(Clone, Copy)]
struct BeamCase {
    span: f64,
    support: beam::Support,
    q: f64,
    pl: f64,
    fy: f64,
    strong: bool,
    limit: f64,
    self_weight: bool,
}

fn beam_eval(prof: &Profile, pr: &Props, bc: &BeamCase) -> (beam::Check, f64, bool) {
    let BeamCase { span, support, q, pl, fy, strong, limit, self_weight } = *bc;
    let q_total = q + if self_weight { pr.mass * 9.81 / 1000.0 } else { 0.0 };
    let (i, w) = beam::axis(pr, strong);
    let av = if strong { beam::shear_area(&prof.shape, pr.area) } else { (pr.area - beam::shear_area(&prof.shape, pr.area)).max(pr.area * 0.3) };
    let c = beam::check(&beam::Load { span, support, q: q_total, p: pl * 1000.0 }, i, w, av, fy);
    let allowed = span / limit;
    let ok = c.bending_ratio <= 1.0 && c.shear_ratio <= 1.0 && c.deflection <= allowed;
    (c, allowed, ok)
}

fn run_beam(_s: &mut Session, p: &Value) -> Result<Value> {
    let prof = profile_param("beamcheck", p)?;
    let span = f64_or(p, "span", f64::NAN);
    if !span.is_finite() || span <= 0.0 || span > 1e6 {
        return Err(bad("beamcheck", "`span` (mm, positive) is required"));
    }
    let support = match str_param(p, "support") {
        None => beam::Support::Simple,
        Some(t) => beam::Support::parse(t).ok_or_else(|| bad("beamcheck", "support is simple, cantilever or fixed"))?,
    };
    let grade = str_param(p, "grade").unwrap_or("S275");
    let fy = beam::yield_strength(grade).ok_or_else(|| bad("beamcheck", "grade is S235, S275, S355, S420 or S460"))?;
    let (q, pl) = (f64_or(p, "q", 0.0), f64_or(p, "p", 0.0));
    if q < 0.0 || pl < 0.0 || q > 1e6 || pl > 1e6 {
        return Err(bad("beamcheck", "loads must be 0..1,000,000"));
    }
    let strong = !matches!(str_param(p, "axis").map(str::to_ascii_lowercase).as_deref(), Some("weak" | "z" | "debole"));
    let limit = f64_or(p, "limit", 250.0);
    if !(1.0..=10000.0).contains(&limit) {
        return Err(bad("beamcheck", "limit must be 1..10000"));
    }
    let sw = bool_or(p, "selfWeight", true);
    let (_, pr) = analyse("beamcheck", &prof)?;
    let bc = BeamCase { span, support, q, pl, fy, strong, limit, self_weight: sw };
    let (c, allowed, ok) = beam_eval(&prof, &pr, &bc);
    let mut msg = format!(
        "{} {}, span {} mm, q = {q} kN/m{}, P = {pl} kN, {grade}\n\
         M = {:.2} kNm   σ = {:.1} N/mm² ({:.0} % of fy/γM0)   V = {:.2} kN   τ = {:.1} N/mm² ({:.0} %)\n\
         deflection = {:.2} mm, limit L/{limit} = {:.2} mm ({:.0} %)  →  {}",
        prof.name,
        support.name(),
        fmt_len(span),
        if sw { format!(" + self-weight {:.2}", pr.mass * 9.81 / 1000.0) } else { String::new() },
        c.m / 1e6,
        c.sigma,
        c.bending_ratio * 100.0,
        c.v / 1000.0,
        c.tau,
        c.shear_ratio * 100.0,
        c.deflection,
        allowed,
        c.deflection / allowed * 100.0,
        if ok { "OK" } else { "NOT VERIFIED" },
    );
    let mut v = json!({
        "profile": prof.name, "support": support.name(), "span_mm": span, "grade": grade, "fy": fy,
        "M_kNm": c.m / 1e6, "V_kN": c.v / 1000.0, "sigma_MPa": c.sigma, "tau_MPa": c.tau,
        "deflection_mm": c.deflection, "deflectionLimit_mm": allowed,
        "bendingRatio": c.bending_ratio, "shearRatio": c.shear_ratio, "deflectionRatio": c.deflection / allowed, "ok": ok,
    });
    if bool_or(p, "suggest", false) {
        let found = catalog::families().into_iter().find(|(f, _, _)| *f == prof.family).and_then(|(_, _, sizes)| {
            sizes.iter().find_map(|d| {
                let cand = catalog::parse(d, bool_or(p, "hot", false)).ok()?;
                let (_, cpr) = analyse("beamcheck", &cand).ok()?;
                let (_, _, ok) = beam_eval(&cand, &cpr, &bc);
                ok.then_some((cand.name, cpr.mass))
            })
        });
        match found {
            Some((name, mass)) => {
                msg.push_str(&format!("\nlightest {} that passes: {name} ({mass:.2} kg/m)", prof.family));
                v["suggested"] = json!(name);
            }
            None => msg.push_str(&format!("\nno listed {} size passes", prof.family)),
        }
    }
    msg.push_str("\n(elastic cross-section check only: lateral-torsional buckling, local buckling and load combinations are not checked)");
    v["message"] = json!(msg);
    Ok(v)
}

// ---------- interactive ----------

fn echo_lines(s: &mut Session, v: &Value) {
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        for l in m.lines() {
            s.echo(l.to_string());
        }
    }
}

fn read_profile(s: &mut Session, i: &Input) -> Result<Option<Profile>> {
    let text = match i {
        Input::Text(t) | Input::Keyword(t) => t.clone(),
        Input::Enter => s.last_used.steel_profile.clone(),
        _ => return Ok(None),
    };
    let prof = catalog::parse(&text, false).map_err(crate::EngineError::Other)?;
    s.last_used.steel_profile = prof.name.clone();
    Ok(Some(prof))
}

fn profile_prompt(s: &Session) -> Prompt {
    Prompt::new("Enter profile (IPE200, HEB240, UPN100, L50x5, RHS100x50x4, CHS60.3x3.2, FL50x5…)", WORD).default(s.last_used.steel_profile.clone())
}

/// STEELSECTION: profile, then the insertion point (with Rotation and Base options).
pub(super) struct SectionM {
    prof: Option<(Profile, Vec<Loop>, Props)>,
    rotation: f64,
    asking_rotation: bool,
    base: Base,
}

impl SectionM {
    fn new(_s: &Session) -> Self {
        SectionM { prof: None, rotation: 0.0, asking_rotation: false, base: Base::Center }
    }
}

impl Interactive for SectionM {
    fn name(&self) -> &'static str {
        "STEELSECTION"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        match (&self.prof, self.asking_rotation) {
            (None, _) => profile_prompt(s),
            (Some(_), true) => Prompt::new("Specify rotation angle", Accept::NUMBER).default(format!("{}", self.rotation.to_degrees())),
            (Some(_), false) => Prompt::new("Specify insertion point", Accept::POINT).kw(&["Rotation", "Centroid", "Center", "Bottom"]),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.prof.is_none() {
            if let Some(prof) = read_profile(s, &i)? {
                let (loops, pr) = analyse("steelsection", &prof)?;
                s.echo(section::props_text(&prof, &pr).lines().next().unwrap_or("").to_string());
                self.prof = Some((prof, loops, pr));
            }
            return Ok(Step::Continue);
        }
        if self.asking_rotation {
            match i {
                Input::Text(t) => {
                    let a = t
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|a| a.is_finite())
                        .ok_or_else(|| crate::EngineError::Other("Requires a number.".into()))?;
                    self.rotation = a.to_radians();
                    self.asking_rotation = false;
                }
                Input::Enter => self.asking_rotation = false,
                _ => {}
            }
            return Ok(Step::Continue);
        }
        match i {
            Input::Keyword(k) => match k.as_str() {
                "Rotation" => self.asking_rotation = true,
                "Centroid" => self.base = Base::Centroid,
                "Center" => self.base = Base::Center,
                "Bottom" => self.base = Base::BottomCenter,
                _ => {}
            },
            Input::Point(at) => {
                if let Some((prof, loops, pr)) = &self.prof {
                    for k in placed(prof, loops, pr, at, self.rotation, self.base, true) {
                        s.add_entity(k)?;
                    }
                    let msg = section::props_text(prof, pr);
                    for l in msg.lines() {
                        s.echo(l.to_string());
                    }
                }
                return Ok(Step::Done);
            }
            Input::Enter | Input::Cancel => return Ok(Step::Cancel),
            _ => {}
        }
        Ok(Step::Continue)
    }
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<EntityKind> {
        match &self.prof {
            Some((prof, loops, pr)) if !self.asking_rotation => placed(prof, loops, pr, c, self.rotation, self.base, false),
            _ => Vec::new(),
        }
    }
}

/// STEELINFO: profile, then optional lengths (each reports the weight) until Enter.
pub(super) struct InfoM {
    prof: Option<(Profile, Props)>,
}

impl InfoM {
    fn new(_s: &Session) -> Self {
        InfoM { prof: None }
    }
}

impl Interactive for InfoM {
    fn name(&self) -> &'static str {
        "STEELINFO"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        match self.prof {
            None => profile_prompt(s),
            Some(_) => Prompt::new("Enter length in mm (or length*quantity) for the weight, or <done>", WORD),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match &self.prof {
            None => {
                if let Some(prof) = read_profile(s, &i)? {
                    let (_, pr) = analyse("steelinfo", &prof)?;
                    echo_lines(s, &info_value(&prof, &pr, 0.0, 1.0, None));
                    self.prof = Some((prof, pr));
                }
                Ok(Step::Continue)
            }
            Some((prof, pr)) => match i {
                Input::Text(t) => {
                    let (len, qty) = parse_len_qty(&t).ok_or_else(|| crate::EngineError::Other("Enter a length such as 6000 or 1250*4.".into()))?;
                    let v = info_value(prof, pr, len, qty as f64, None);
                    if let Some(last) = v.get("message").and_then(Value::as_str).and_then(|m| m.lines().last()) {
                        s.echo(last.to_string());
                    }
                    Ok(Step::Continue)
                }
                Input::Enter | Input::Cancel => Ok(Step::Done),
                _ => Ok(Step::Continue),
            },
        }
    }
}

/// `1250`, `1250*4` or `1250x4`: a length and a quantity.
fn parse_len_qty(t: &str) -> Option<(f64, u64)> {
    let t = t.trim().to_ascii_lowercase();
    let (l, q) = match t.split_once(['*', 'x']) {
        Some((l, q)) => (l, q.trim().parse::<u64>().ok()?),
        None => (t.as_str(), 1),
    };
    let len = number(l.trim()).filter(|x| x.is_finite() && *x > 0.0 && *x <= 1e7)?;
    (q >= 1 && q <= cutlist::MAX_PIECES as u64).then_some((len, q))
}

/// CUTLIST at the command line: bar length, kerf, then pieces (`length` or `length*quantity`)
/// until Enter; the result is echoed.
#[derive(Default)]
pub(super) struct CutM {
    stock: Option<f64>,
    kerf: Option<f64>,
    pieces: Vec<cutlist::Piece>,
}

impl Interactive for CutM {
    fn name(&self) -> &'static str {
        "CUTLIST"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match (self.stock, self.kerf) {
            (None, _) => Prompt::new("Enter bar length (mm)", Accept::NUMBER).default("6000"),
            (Some(_), None) => Prompt::new("Enter kerf per cut (mm)", Accept::NUMBER).default("2"),
            _ => Prompt::new(format!("Enter piece length or length*quantity ({} so far), or <calculate>", self.pieces.len()), WORD),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let num = |t: &str| number(t.trim()).filter(|x| x.is_finite() && *x >= 0.0);
        match (self.stock, self.kerf, i) {
            (None, _, Input::Enter) => self.stock = Some(6000.0),
            (None, _, Input::Text(t)) => {
                self.stock =
                    Some(num(&t).filter(|x| *x >= 1.0 && *x <= 1e6).ok_or_else(|| crate::EngineError::Other("Enter a bar length in mm.".into()))?)
            }
            (Some(_), None, Input::Enter) => self.kerf = Some(2.0),
            (Some(_), None, Input::Text(t)) => {
                self.kerf = Some(num(&t).filter(|x| *x <= 100.0).ok_or_else(|| crate::EngineError::Other("Enter a kerf in mm.".into()))?)
            }
            (Some(stock), Some(kerf), Input::Text(t)) => {
                let (len, qty) = parse_len_qty(&t).ok_or_else(|| crate::EngineError::Other("Enter a length such as 1250 or 1250*4.".into()))?;
                if len > stock {
                    return Err(crate::EngineError::Other(format!("{len} mm is longer than the {stock} mm bar.")));
                }
                if self.pieces.len() as u64 + qty > cutlist::MAX_PIECES as u64 {
                    return Err(crate::EngineError::Other("Too many pieces.".into()));
                }
                let _ = kerf;
                self.pieces.extend((0..qty).map(|_| cutlist::Piece { length: len, label: fmt_len(len) }));
            }
            (Some(stock), Some(kerf), Input::Enter) => {
                if self.pieces.is_empty() {
                    return Ok(Step::Cancel);
                }
                let v = cut_result(s, std::mem::take(&mut self.pieces), &CutParams { stock, kerf, trim: 0.0 }, None, None, None)?;
                echo_lines(s, &v);
                return Ok(Step::Done);
            }
            (_, _, Input::Cancel) => return Ok(Step::Cancel),
            _ => {}
        }
        Ok(Step::Continue)
    }
}

/// BEAMCHECK at the command line: profile, span, uniform load, point load (S275, simply
/// supported); the JSON form takes the other options.
pub(super) struct BeamM {
    prof: Option<Profile>,
    span: Option<f64>,
    q: Option<f64>,
}

impl BeamM {
    fn new(_s: &Session) -> Self {
        BeamM { prof: None, span: None, q: None }
    }
}

impl Interactive for BeamM {
    fn name(&self) -> &'static str {
        "BEAMCHECK"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        match (&self.prof, self.span, self.q) {
            (None, _, _) => profile_prompt(s),
            (Some(_), None, _) => Prompt::new("Enter span (mm)", Accept::NUMBER),
            (Some(_), Some(_), None) => Prompt::new("Enter uniform load q (kN/m, self-weight is added)", Accept::NUMBER).default("0"),
            _ => Prompt::new("Enter point load at mid-span P (kN)", Accept::NUMBER).default("0"),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.prof.is_none() {
            self.prof = read_profile(s, &i)?;
            return Ok(Step::Continue);
        }
        let val = match i {
            Input::Text(t) => Some(
                number(t.trim())
                    .filter(|x| x.is_finite() && *x >= 0.0)
                    .ok_or_else(|| crate::EngineError::Other("Requires a positive number.".into()))?,
            ),
            Input::Enter => None,
            Input::Cancel => return Ok(Step::Cancel),
            _ => return Ok(Step::Continue),
        };
        if self.span.is_none() {
            self.span = Some(val.filter(|v| *v > 0.0).ok_or_else(|| crate::EngineError::Other("Requires a span in mm.".into()))?);
            return Ok(Step::Continue);
        }
        if self.q.is_none() {
            self.q = Some(val.unwrap_or(0.0));
            return Ok(Step::Continue);
        }
        let name = self.prof.as_ref().map(|p| p.name.clone()).unwrap_or_default();
        let params = json!({ "profile": name, "span": self.span, "q": self.q, "p": val.unwrap_or(0.0), "suggest": true });
        let v = run_beam(s, &params)?;
        echo_lines(s, &v);
        Ok(Step::Done)
    }
}

#[cfg(test)]
mod tests;
