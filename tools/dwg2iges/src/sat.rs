//! A reader for SAT text (the ACIS save format): header, then records ending in `#`, each a
//! type name followed by pointers (`$n`), numbers, words and counted strings (`@n text`).
//! Records are numbered by their position, which is what the pointers refer to.

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ptr(i64),
    Num(f64),
    Word(String),
    Str(String),
    Open,
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rec {
    pub kind: String,
    pub toks: Vec<Tok>,
}

#[derive(Clone, Debug)]
pub struct Sat {
    pub recs: Vec<Rec>,
}

/// The most records a solid may have: bounds the work on hostile input.
const MAX_RECORDS: usize = 2_000_000;

fn token(word: &str) -> Tok {
    match word {
        "{" => Tok::Open,
        "}" => Tok::Close,
        w if w.starts_with('$') => w[1..].parse::<i64>().map(Tok::Ptr).unwrap_or_else(|_| Tok::Word(w.into())),
        w => match w.parse::<f64>() {
            Ok(v) if v.is_finite() => Tok::Num(v),
            _ => Tok::Word(w.into()),
        },
    }
}

pub fn parse(text: &str) -> Result<Sat, String> {
    let mut lines = text.lines();
    let version = lines
        .next()
        .and_then(|l| l.split_whitespace().next())
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or("the solid's data has no SAT header")?;
    if version < 700 {
        return Err(format!("ACIS version {version} is older than 7.0 and not supported yet"));
    }
    // Product line and tolerances.
    lines.next();
    lines.next();
    let body: String = lines.collect::<Vec<_>>().join("\n");
    let mut recs = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut kind: Option<String> = None;
    let mut rest = body.as_str();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, after) = rest.split_at(end);
        rest = after;
        if word == "End-of-ACIS-data" || word == "End-of-ASM-data" {
            break;
        }
        // A counted string: `@<len> <text>`, whose text may hold spaces.
        if let Some(n) = word.strip_prefix('@').and_then(|n| n.parse::<usize>().ok()) {
            let s = rest.strip_prefix(' ').unwrap_or(rest);
            let take = s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len());
            cur.push(Tok::Str(s[..take].to_string()));
            rest = &s[take..];
            continue;
        }
        if word.ends_with('#') {
            let head = &word[..word.len() - 1];
            if !head.is_empty() && kind.is_some() {
                cur.push(token(head));
            }
            if let Some(k) = kind.take() {
                recs.push(Rec { kind: k, toks: std::mem::take(&mut cur) });
                if recs.len() > MAX_RECORDS {
                    return Err("too many records".into());
                }
            }
            continue;
        }
        match &kind {
            None => {
                // Records of 7.0 may start with an explicit `-<index>`.
                if word.starts_with('-') && word[1..].chars().all(|c| c.is_ascii_digit()) && word.len() > 1 {
                    continue;
                }
                kind = Some(word.to_string());
            }
            Some(_) => cur.push(token(word)),
        }
    }
    Ok(Sat { recs })
}

impl Sat {
    pub fn rec(&self, p: i64) -> Option<&Rec> {
        usize::try_from(p).ok().and_then(|i| self.recs.get(i))
    }
}

impl Rec {
    /// The fields after the attribute pointer, the `-1` id and (except for transforms) the
    /// history pointer.
    pub fn fields(&self) -> &[Tok] {
        let skip = if self.kind == "transform" { 2 } else { 3 };
        self.toks.get(skip..).unwrap_or(&[])
    }
    pub fn ptr(&self, i: usize) -> Option<i64> {
        match self.fields().get(i) {
            Some(Tok::Ptr(p)) if *p >= 0 => Some(*p),
            _ => None,
        }
    }
    pub fn num(&self, i: usize) -> Option<f64> {
        match self.fields().get(i) {
            Some(Tok::Num(v)) => Some(*v),
            _ => None,
        }
    }
    pub fn nums<const N: usize>(&self, start: usize) -> Option<[f64; N]> {
        let mut out = [0.0; N];
        for (k, o) in out.iter_mut().enumerate() {
            *o = self.num(start + k)?;
        }
        Some(out)
    }
    pub fn word(&self, i: usize) -> Option<&str> {
        match self.fields().get(i) {
            Some(Tok::Word(w)) => Some(w),
            _ => None,
        }
    }
    /// The base type: `cone` for `cone-surface`, `intcurve` for `intcurve-curve`.
    pub fn base(&self) -> &str {
        self.kind.split('-').next().unwrap_or(&self.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_counted_strings() {
        let t = "700 0 1 0\n@4 test\n1 1e-06 1e-10\nbody $-1 -1 $-1 $1 $-1 $-1 #\nedge $-1 -1 $-1 $2 0 $2 6.28 $3 $4 forward @7 unknown #\npoint $-1 -1 $-1 1 2 3#\nEnd-of-ACIS-data\n";
        let s = parse(t).unwrap();
        assert_eq!(s.recs.len(), 3);
        assert_eq!(s.recs[1].ptr(0), Some(2));
        assert_eq!(s.recs[1].num(3), Some(6.28));
        assert_eq!(s.recs[1].word(6), Some("forward"));
        assert_eq!(s.recs[1].fields().last(), Some(&Tok::Str("unknown".into())));
        assert_eq!(s.recs[2].nums::<3>(0), Some([1.0, 2.0, 3.0]));
        assert!(parse("400 0 1 0\n").is_err());
        assert!(parse("").is_err());
    }
}
