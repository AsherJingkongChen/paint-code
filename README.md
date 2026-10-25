# paintcode

A small dab engine for painting from code.

A flower is what you write: a size, a background and petals (strokes of any shape).
Painting turns each petal into dabs; all the dabs together are the coat.
The coat over the background is the canvas, the pixels you see.

A petal is a list of control points with pressure and tilt; the engine resamples it along its arc length and lays down a row of dabs, each an **anisotropic 2D Gaussian**. Every mark you see is made of splats.

It was written so a language model can paint with its own strokes: the model writes a compact JSON flower, the engine paints it deterministically, and the model looks at the canvas and decides the next petals.

![A yellow chrysanthemum at dusk](examples/kiku.png)

`examples/kiku.flower.json` holds its 629 petals in 97 KB (23 KB gzipped). Painted, they become 87,114 dabs, which take 13 MB as a coat. The petals are the part that has to fit in a context window; the dabs never do.

## Install

```bash
pip install .               # builds the engine (needs a Rust toolchain)
uv sync                     # for development: the package, pytest, numpy and Pillow
```

## Paint

```python
import paintcode as pc

flower = {"width": 400, "height": 200, "background": "#f4efe6", "seed": 1,
          "petals": [{"brush": "nib", "color": "#223", "points": [[20, 100], [80, 140], [140, 90]]},
                     {"brush": "filbert", "size": 12, "color": [200, 120, 40],
                      "points": [[20, 170, 0.3], [200, 165, 1], [380, 175, 0.2]]}]}

canvas = pc.flower_to_canvas(flower=flower)                  # the pixels
canvas = pc.flower_to_canvas(flower=flower, scale=2)         # coordinates and brush sizes scale together
canvas = pc.flower_to_canvas(flower=flower, supersample=3)   # supersample hard edges
coat = pc.flower_to_coat(flower=flower)                      # every dab; same options
canvas = pc.coat_to_canvas(coat)                             # the same canvas, bit for bit

pc.save_flower(flower=flower, path="sketch.flower.json")
pc.save_canvas(canvas=canvas, path="sketch.png")
pc.save_canvas(canvas=pc.flower_on_canvas(flower=flower, canvas=canvas), path="sketch.flower.png")
pc.save_coat(coat=coat, path="sketch.coat.ply")

flower = pc.load_flower("sketch.flower.png")                 # the flower, from its file or its canvas
canvas = pc.load_canvas("sketch.flower.png")                 # only the pixels
canvas.peek(x=20, y=100), canvas.crop(x=0, y=0, width=200, height=100)
```

`_to_` turns one thing into another, `_on_` and `_off_` put a flower on a canvas and take it off, and `load_` and `save_` take paths; the files are read and written in Rust. A function with more than one parameter takes them by keyword. Mistakes raise `ValueError` with a message that names the petal, the field or the file. `help(paintcode._core)` and the type stub `paintcode/_core.pyi` describe every function; the package has no dependencies of its own. There is no command line: the engine is a library, and Python (later JavaScript) is how it is driven.

| file | what it is |
|---|---|
| `.flower.json` | a flower, one petal per line |
| `.png` | a canvas, 8-bit RGB |
| `.flower.png` | a canvas with its flower on it, in a compressed iTXt chunk; any image viewer shows the canvas |
| `.coat.ply` | every dab as a flat 3D Gaussian in the 3D Gaussian Splatting layout, readable by splat viewers, and kept exactly |

Files are told apart by their bytes, not their names: `load_flower` reads JSON, or the flower on a PNG (a PNG without one is an error); `load_canvas` takes any PNG and only its pixels.

The same flower always produces the same pixels. Jitter is counter-based: every random number is a hash of (seed, petal index, dab index, slot), so editing or lengthening one petal never moves the jitter of another, and appending petals leaves the ones before them alone. To keep working on a painting, load its flower, append petals and paint it again.

Version 0.6 paints the same pixels as 0.5 under new names: a scene is a flower, a stroke is a petal, `PRESETS` is `BRUSHES`, and the flower is flat (`width`, `height`, `background` and `seed` at the top; `supersample` is an option of the paint). The `.petal` ZIP is gone: a `.flower.json` holds the flower and a `.flower.png` carries it on its canvas. Painting over a base image is gone too. A `.coat.ply` keeps every dab exactly, so it is larger than 0.5's `.ply`.

Version 0.5 paints the same pixels as 0.4 (within 1/255 on the PNG, 1e-5 on the float canvas). It is stricter about mistakes: canvas `width`, `height`, `seed` and `supersample` must be whole numbers written as numbers (0.4 also took `"640"` and rounded `640.7`), a control point may not lie more than 16 canvas sizes off the canvas (the base image's size, when there is one), and a canvas or a base PNG is at most 2^28 pixels after scale and supersample; each of these is an error naming the stroke or field, where 0.4 rounded quietly, hung or crashed. A base image is PNG only; 0.4 took anything Pillow opens. 0.4's command line (`python -m paintcode`) is gone; every option it had is a keyword of `render_png` and `render_ply`. A 16-bit base PNG keeps its tones (0.4 clipped 16-bit grey to white). Version 0.4 changed the random numbers (jitter and paper tooth), so a scene from 0.3 renders with slightly different jitter.

## Flower format

```json
{
  "width": 640, "height": 640, "background": "#2a2f4a", "seed": 24,
  "brushes": { "soft": { "preset": "filbert", "size": 8.5 } },
  "petals": [
    { "brush": "soft", "color": [234, 164, 52],
      "points": [[330, 270, 0.3], [380, 240, 1.0], [420, 250, 0.45]] },
    { "brush": "ink", "color": "#e8d9b0", "size": 1.4, "flow": 0.3,
      "points": [[340, 268], [400, 246]] },
    { "brush": "blender", "color": null, "points": [[300, 300], [360, 300]] }
  ]
}
```

- **points** are `[x, y]`, `[x, y, pressure]` or `[x, y, pressure, tilt]`, with pressure and tilt in 0..1 (defaults 1 and 0). Coordinates are floats.
- **brush** is a name from `paintcode.BRUSHES`, a name from `brushes`, or an inline object `{"preset": ..., ...}`. Any brush field can also be overridden directly on the petal.
- **color** is `[r, g, b]` (0..255) or `"#rrggbb"`. `null` loads the brush with whatever is under its first touch, which makes it a smudge.
- **smooth** (default `true`) runs a centripetal Catmull-Rom spline through the points; `false` keeps straight segments.

## The dab

Each dab is

$$\alpha(\mathbf{x}) = \text{flow}\cdot\exp\!\left(-\tfrac12\, q^{\,n}\right),\qquad n = \frac{1}{1-h},\qquad q = \left(\tfrac{u}{\sigma_x}\right)^2 + \left(\tfrac{v}{\sigma_y}\right)^2$$

where $(u, v)$ is the pixel offset rotated into the dab's frame and $h$ is hardness. The profile reaches $e^{-2}$ at $\sigma\, 2^{1/n}$, so the engine sets $\sigma = \text{radius}\cdot 2^{-1/n}$: the visible edge stays at the brush radius whatever the hardness, with $\sigma = r/2$ for a Gaussian and $\sigma = r$ for a hard ellipse. With $h = 0$ it is exactly a 2D Gaussian; as $h \to 1$ it becomes a hard-edged ellipse one sigma across. Sigmas below half a pixel are raised to 0.5 px, dabs are cut off at $q > 9$, and they are composited with plain "over". Dab geometry is computed in float64; the per-pixel kernel and the compositing are float32.

### Sharpness

A Gaussian is a low-pass filter, so a stroke made of Gaussians has soft edges by nature; hardness is the cure. A stroke of size 16 keeps a half-coverage width of 26 to 30 px from hardness 0 to 0.95, while its 10–90% edge narrows from about 14 px to 1 px. Hard brushes alias, so set `supersample=` to paint at N times the size and box-filter down; the dabs and the jitter stay the same, only the sampling improves.

### As Gaussian splats

`.coat.ply` writes one Gaussian per dab: position on the image plane (x right, y down, the longer side spanning $[-1, 1]$), scales $\log\sigma$ with a vanishing third axis, a quaternion for the dab angle about $z$, colour as the SH DC term, opacity as a logit. Paint order becomes depth: the first dab is farthest and the last is nearest to a camera on the $-z$ side, so depth-sorted splatting composites them in the order they were painted. The stack is only 0.05 deep, so this holds facing the plane: turned away, the splats sort by where they sit instead of when they were painted, and the wide early sky strokes drift in front of the flower. The files open in [SuperSplat](https://superspl.at/editor) through File > Import. After the 3DGS fields come `pc_*` fields that keep each dab exactly (position, sigmas, angle, opacity, hardness and grain as doubles, its colour as floats), and a `paintcode` header comment keeps the size, supersample, seed and background, so `load_coat` gives back a coat that paints the same canvas bit for bit.

The tests carry a separate, plain rasteriser that reads only the 3DGS fields of the PLY. For a painting made with hardness 0 and no grain it reproduces the canvas to within 2/255 (`a_soft_painting_round_trips_through_the_ply` in `crates/paintcode/tests/engine.rs`). The 3DGS kernel has no hardness or grain, so other paintings come back softer:

![The painting, and the same dabs rendered back from the PLY](examples/png_vs_ply.png)

## Brush fields

| field | meaning |
|---|---|
| `size` | visible radius in px at full pressure; the same size looks the same at every hardness |
| `aspect` | minor / major axis ratio |
| `angle` | `null`: the major axis lies across the stroke and turns with it; a number: fixed nib angle in degrees |
| `hardness` | profile exponent $1/(1-h)$: 0 is a Gaussian, 0.9 is nearly a hard ellipse |
| `flow` | opacity built up at the default spacing |
| `spacing` | dab distance as a fraction of the footprint; flow is normalised to it, so this only trades speed for smoothness |
| `min_size`, `size_gamma` | radius $= \text{size}\cdot\text{lerp}(\text{min\_size}, 1, p^{\gamma})$ |
| `min_flow`, `flow_gamma` | the same curve for flow |
| `tilt_stretch` | tilt 1 stretches the major axis by $1 + \text{tilt\_stretch}$ and thins the film |
| `size_jitter`, `angle_jitter`, `pos_jitter`, `flow_jitter` | uniform noise from the canvas seed |
| `load`, `half_life`, `min_load` | paint on the brush, and how many px of travel it takes to halve |
| `pickup` | share of the wet paint under the brush mixed into the carried colour, per radius of travel |
| `grain`, `grain_scale` | paper tooth fixed to the canvas, modulating every dab |

Brushes: `ink`, `pencil`, `nib`, `round`, `filbert`, `flat`, `airbrush`, `dry`, `blender`. `paintcode.BRUSHES` holds each one with every field, written as a flower would write it.

![Every brush drawing the same curve](examples/brush_sheet.png)

### Wet paint

The canvas keeps a wetness map. Every dab makes the area under it wetter, and `pickup` only takes colour from wet paint, read from a snapshot taken when the petal starts. So a brush never picks up the dry background, never re-picks its own trail, and the result does not depend on dab spacing.

## The engine

The engine is Rust: one library, `crates/paintcode`, with one module per part of the engine. `bindings/python` builds it into the Python package as `paintcode._core`; the rest of the package only re-exports it. Inside the library, dependencies point one way, from the leaf up, and `tests/layering.rs` fails if a module reaches back down the list.

| module | part | may use | does |
|---|---|---|---|
| `math` | leaf | nothing | counter-based RNG, dab profile and geometry |
| `flower` | bones | math | flower, petal and brush types, brushes, colours |
| `render` | render | math | dabs to pixels, coats, paper grain, downsampling |
| `petal` | flesh | math, flower, render | control points to dabs, carrying paint |
| `io` | files | render | encoders and decoders, in pairs: flower JSON, PNG, PLY |

Up to 0.4 a numpy engine in this package was the reference, and the core was checked against it at 1e-5 on the float canvas (2.4e-7 at worst on kiku). 0.5 retired it: its output for the examples and a feature matrix is frozen in `tests/golden/`, and the core is checked against those files.

A petal is painted in two parallel phases: every dab's kernel and the wet paint under it are evaluated independently (pickup reads the canvas as it was when the petal began), then the dabs are composited in order, one band of rows per thread. The fast path also skips the parts of each dab's square that lie outside its $q \le 9$ ellipse. None of this changes a bit: the tests render with the reference path (whole squares, `powf` everywhere, one thread) and require identical output.

kiku, 640², supersample 2, on a 2-core VM:

| engine | time |
|---|---|
| numpy (0.4) | 52 s |
| Rust, 1 thread | 7.0 s |
| Rust, 2 threads | 3.5 s |

```bash
cargo run --release --example bench -- examples/kiku.flower.json --ss 2   # time a paint
```

## Tests

```bash
cargo test --release -p paintcode -- --include-ignored   # the engine: behaviour, golden output, coats, fast path = reference path, layering
uv run pytest                                            # the Python face: arguments, files, errors
```

`tests/golden/make.py` made the golden files with the numpy engine at 0.4; it does not run on 0.5, which no longer has that engine.

## Credits

The ideas of paint depletion, colour pickup and short-stroke painting were learned from [paint-mcp](https://github.com/joeyballentine/paint-mcp). No code was taken from it.
