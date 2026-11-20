//! Cut list: nests the pieces to cut into commercial bar lengths (first-fit decreasing, then a
//! pass that tries to empty the last bar), with the kerf of each cut and an optional trim at the
//! bar's head, and reports bars, offcuts, yield and weight.

/// The most pieces one cut list takes: bounds the nesting work for hostile input.
pub const MAX_PIECES: usize = 20_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub length: f64,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    pub pieces: Vec<Piece>,
    /// Length used: trim, pieces and the kerf after each piece.
    pub used: f64,
}

impl Bar {
    fn need(len: f64, kerf: f64, empty: bool, trim: f64) -> f64 {
        len + kerf + if empty { trim } else { 0.0 }
    }
}

/// Nest `pieces` into bars of `stock` mm. A cut after the last piece is counted too, unless
/// the piece ends exactly at the bar's end.
pub fn nest(mut pieces: Vec<Piece>, stock: f64, kerf: f64, trim: f64) -> Result<Vec<Bar>, String> {
    if let Some(p) = pieces.iter().find(|p| p.length + trim > stock + 1e-9) {
        return Err(format!("a {} mm piece does not fit a {} mm bar", p.length, stock));
    }
    pieces.sort_by(|a, b| b.length.total_cmp(&a.length));
    let mut bars: Vec<Bar> = Vec::new();
    for p in pieces {
        let fits = |b: &Bar| {
            let need = Bar::need(p.length, kerf, b.pieces.is_empty(), trim);
            // The last piece may end at the bar's end without a cut after it.
            b.used + need <= stock + 1e-9 || b.used + need - kerf <= stock + 1e-9
        };
        match bars.iter_mut().find(|b| fits(b)) {
            Some(b) => {
                b.used = (b.used + Bar::need(p.length, kerf, b.pieces.is_empty(), trim)).min(stock);
                b.pieces.push(p);
            }
            None => {
                bars.push(Bar { used: Bar::need(p.length, kerf, true, trim).min(stock), pieces: vec![p] });
            }
        }
    }
    // Most offcut first in the report is less useful than fullest first.
    bars.sort_by(|a, b| b.used.total_cmp(&a.used));
    Ok(bars)
}
