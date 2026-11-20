# Structural steel

CADCraft has a set of commands for working with commercial steel profiles: drawing their cross
sections with the real root and corner radii, reading their properties and weight, nesting cuts
into bar lengths, and running a quick check of a beam. Dimensions are in millimetres.

| Command | Menu | Aliases | What it does |
|---|---|---|---|
| `STEELSECTION` | Draw ▸ Steel ▸ Section... | `PROFILO`, `STEEL` | Draws a profile's cross-section at a point, with an optional label, and reports its properties |
| `STEELINFO` | Tools ▸ Steel ▸ Section Properties | `PESO`, `STEELWEIGHT` | Properties, weight for a length and quantity, paintable surface and cost |
| `STEELLIST` | Tools ▸ Steel ▸ Profile List | | The families and sizes in the catalogue |
| `CUTLIST` | Tools ▸ Steel ▸ Cut List | `DISTINTA`, `NESTING` | Nests pieces into bars (6 m by default) with the kerf, reports yield and offcuts, and can draw a bar diagram |
| `BEAMCHECK` | Tools ▸ Steel ▸ Beam Check | `TRAVE`, `VERIFICATRAVE` | Elastic bending, shear and deflection check of a single-span beam, and the lightest size of the family that passes |

## Profiles

Type a designation with or without spaces, in any case. `x`, `×` and `*` separate sizes, and a
decimal comma works too (`CHS 60,3x3,2`).

| Family | Designation | Source of the dimensions |
|---|---|---|
| IPE | `IPE80` … `IPE600` | EN 10365 nominal dimensions |
| HEA, HEB | `HEA100` … `HEA600`, `HEB100` … `HEB600` | EN 10365 |
| HEM | `HEM100` … `HEM300` | EN 10365 |
| UPN | `UPN80` … `UPN300` (8 % flange taper) | EN 10365 |
| UPE | `UPE80` … `UPE300` | EN 10365 |
| Angle | `L50x5` (equal), `L100x50x8` (unequal) | Any size; root radius by leg as EN 10056-1, toe radius half of it |
| Square hollow | `SHS40x3` or `SHS40x40x3`, alias `TQ` | Any size; cold-formed corner radii as EN 10219-2, or hot-finished (EN 10210-2) with `hot: true` |
| Rectangular hollow | `RHS100x50x4`, alias `TR` | As SHS |
| Circular hollow | `CHS60.3x3.2`, alias `TT` | Any size |
| Flat | `FL50x5`, alias `PIATTO` | Any size |
| Round, square bar | `RD20` (alias `TONDO`), `SQ20` (alias `QUADRO`) | Any size |

Properties are computed from the drawn outline (its arcs finely subdivided), so the numbers and
the drawing always agree. Masses use a density of 7850 kg/m³; they match the published tables
to within about 1–2 %. `y` is the horizontal axis through the centroid (the strong axis of an
upright I section) and `z` the vertical one. Angles report their geometric axes, not the
principal ones.

## Examples

At the command line:

```text
steelsection heb200 0,0          an HEB 200 centred at the origin, labelled
steelsection l80x8 r 90 500,0    an angle rotated 90°
steelinfo ipe240 6000*12         properties, then the weight of 12 bars of 6 m
cutlist 6000 3 1250*4 2400*2     bar 6000 mm, kerf 3 mm, pieces; Enter to calculate
beamcheck ipe200 5000 8 0        IPE 200, 5 m span, 8 kN/m, no point load
```

As JSON (control channel, MCP, CLI):

```json
{"command": "steelsection", "params": {"profile": "RHS100x50x4", "at": [0, 0], "rotation": 90, "base": "centroid", "label": true}}
{"command": "steelinfo", "params": {"profile": "IPE200", "length": 6000, "quantity": 10, "pricePerKg": 1.15}}
{"command": "cutlist", "params": {"pieces": [{"length": 2900, "quantity": 2}, {"length": 1500, "quantity": 4}], "stock": 6000, "kerf": 3, "profile": "SHS40x3", "at": [0, -1000]}}
{"command": "beamcheck", "params": {"profile": "IPE160", "span": 4500, "support": "simple", "q": 6, "p": 5, "grade": "S275", "limit": 250, "suggest": true}}
```

## Beam check

`BEAMCHECK` uses the elastic formulas for a uniform load and a point load at mid-span (at the
tip for a cantilever) on a simply supported, cantilever or fixed-ended beam. The self-weight is
added unless `selfWeight` is false. Bending stress is checked against fy/γM0 (γM0 = 1.05), the
mean shear stress against fy/(√3·γM0), and the deflection against span/`limit` (250 by default).
It is a sizing aid only: lateral-torsional buckling, local buckling, connections and load
combinations are not checked, and a structural design remains the engineer's.
