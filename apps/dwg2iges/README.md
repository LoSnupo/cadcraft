# dwg2iges

Writes the 3D solids of DWG and DXF drawings as IGES 5.3 files, for CAM software that imports
IGES (tube laser nesting such as BLM ArTube, CNC, other CAD systems).

```
dwg2iges [options] FILE.dwg [FILE2.dxf ...]
```

Each drawing gets an `.igs` file beside it. On Windows, drag one or more drawings onto
`dwg2iges.exe`; the window stays open until Enter is pressed (`--no-pause` turns that off for
scripts).

| Option | |
|---|---|
| `-o PATH` | output file (one input only) |
| `--tol MM` | chord tolerance of the trimming edges, mm (default 0.01) |
| `--no-pause` | do not wait for Enter at the end on Windows |

## What it writes

Every face of every 3D solid (`3DSOLID`, `BODY`) in model space becomes an IGES trimmed
surface (entity 144):

- the surface is exact: planes as bilinear B-spline patches, cylinders and cones as rational
  B-spline surfaces (entity 128);
- the edges are curves on the surface (entity 142), with a parameter-space curve and a model
  curve (entity 126) sampled within the chord tolerance. A tube's cylinder gets one boundary
  that runs along both ends and the seam.

Units are millimetres. The tool reports, for each drawing, the faces written by type, how far
the edges are from their surfaces and vertices, and any face it had to leave out.

## Supported geometry

Planes, cylinders and cones, with edges on lines, circles, ellipses and the intersection curves
between them (saddle cuts, branch holes, inclined cuts): the geometry of tubes, pipes and
plates. Faces on other surfaces (tori, spheres, free-form spline surfaces) are reported and
left out. Solids inside block references are not read yet.

## Checked with

The output of the sample tube parts (filters DN32–DN65 on a 114.3 × 6 tube, a 406.4 × 6.3
condensate trap) imports into OpenCASCADE, and its faces sew into one closed solid whose volume
matches the part's.

## Building

```
cargo build --release
```

This is a standalone tool, not part of the CADCraft workspace: it links
[acadrust](https://crates.io/crates/acadrust) (MPL-2.0, unmodified) to read DWG files and the
solids' ACIS data. The SAT reader, the B-rep walk, the trimming and the IGES writer are this
tool's own code (MIT OR Apache-2.0), written from the public IGES 5.3 specification and the
SAT save-file format.
