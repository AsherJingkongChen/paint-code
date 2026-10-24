# paint-code

A small dab engine for painting from code. A stroke is a list of control points with pressure and tilt; the engine resamples it along its arc length and lays down a row of **anisotropic 2D Gaussians**. Every mark you see is made of splats.

It was written so a language model can paint with its own strokes: the model writes a compact JSON scene, the engine renders it deterministically, and the model looks at the PNG and decides the next strokes.

![A yellow chrysanthemum at dusk](examples/kiku.png)

`examples/kiku.json` is 629 strokes and 113 KB. Rendered, it becomes 48,927 dabs. The scene is the part that has to fit in a context window; the dabs never do.

## Install

```bash
pip install -e .            # numpy and pillow
pip install -e '.[dev]'     # plus pytest
```

## Render

```bash
python -m paintcode scene.json -o out.png
python -m paintcode scene.json -o big.png --scale 2        # coordinates and brush sizes scale together
python -m paintcode scene.json -o out.png --dabs dabs.json # also export every dab as a Gaussian record
python -m paintcode scene.json -o out.png --base old.png   # paint over an image (treated as dry paint)
```

The same scene and seed always produce the same pixels. To keep working on a painting, append strokes to the scene and render it again rather than painting over the PNG: re-rendering keeps the wet-paint state that `--base` cannot recover.

## Scene format

```json
{
  "canvas":  { "width": 640, "height": 640, "background": "#2a2f4a", "seed": 24 },
  "brushes": { "petal": { "preset": "filbert", "size": 8.5 } },
  "strokes": [
    { "brush": "petal", "color": [234, 164, 52],
      "points": [[330, 270, 0.3], [380, 240, 1.0], [420, 250, 0.45]] },
    { "brush": "ink", "color": "#e8d9b0", "size": 1.4, "flow": 0.3,
      "points": [[340, 268], [400, 246]] },
    { "brush": "blender", "color": null, "points": [[300, 300], [360, 300]] }
  ]
}
```

- **points** are `[x, y]`, `[x, y, pressure]` or `[x, y, pressure, tilt]`, with pressure and tilt in 0..1 (defaults 1 and 0). Coordinates are floats.
- **brush** is a preset name, a name from `brushes`, or an inline object `{"preset": ..., ...}`. Any brush field can also be overridden directly on the stroke.
- **color** is `[r, g, b]` (0..255) or `"#rrggbb"`. `null` loads the brush with whatever is under its first touch, which makes it a smudge.
- **smooth** (default `true`) runs a centripetal Catmull-Rom spline through the points; `false` keeps straight segments.

## The dab

Each dab is

$$\alpha(\mathbf{x}) = \text{flow}\cdot\exp\!\left(-\tfrac12\, q^{\,1+3h}\right),\qquad q = \left(\tfrac{u}{\sigma_x}\right)^2 + \left(\tfrac{v}{\sigma_y}\right)^2$$

where $(u, v)$ is the pixel offset rotated into the dab's frame and $h$ is hardness. With $h = 0$ it is exactly a 2D Gaussian; as $h$ grows the top flattens and the edge sharpens. Dabs are cut off at $q > 9$ and composited with plain "over".

`--dabs` writes `{x, y, sx, sy, theta, rgb, alpha, hardness}` per dab, in paint order. Paper grain is not part of that record.

## Brush fields

| field | meaning |
|---|---|
| `size` | radius in px at full pressure ($\sigma = \text{size}/2$) |
| `aspect` | minor / major axis ratio |
| `angle` | `null`: the major axis lies across the stroke and turns with it; a number: fixed nib angle in degrees |
| `hardness` | 0 Gaussian, toward 1 flat-topped |
| `flow` | opacity built up at the default spacing |
| `spacing` | dab distance as a fraction of the footprint; flow is normalised to it, so this only trades speed for smoothness |
| `min_size`, `size_gamma` | radius $= \text{size}\cdot\text{lerp}(\text{min\_size}, 1, p^{\gamma})$ |
| `min_flow`, `flow_gamma` | the same curve for flow |
| `tilt_stretch` | tilt 1 stretches the major axis by $1 + \text{tilt\_stretch}$ and thins the film |
| `size_jitter`, `angle_jitter`, `pos_jitter`, `flow_jitter` | uniform noise from the canvas seed |
| `load`, `half_life`, `min_load` | paint on the brush, and how many px of travel it takes to halve |
| `pickup` | share of the wet paint under the brush mixed into the carried colour, per radius of travel |
| `grain`, `grain_scale` | paper tooth fixed to the canvas, modulating every dab |

Presets: `ink`, `pencil`, `nib`, `round`, `filbert`, `flat`, `airbrush`, `dry`, `blender`.

![Every preset drawing the same curve](examples/brush_sheet.png)

### Wet paint

The canvas keeps a wetness map. Every dab makes the area under it wetter, and `pickup` only takes colour from wet paint, read from a snapshot taken when the stroke starts. So a brush never picks up the dry background, never re-picks its own trail, and the result does not depend on dab spacing.

## Python

```python
from paintcode import Canvas, get_brush, paint_stroke

c = Canvas(400, 200, background="#f4efe6", seed=1)
paint_stroke(c, get_brush("nib"), "#223", [[20, 100], [80, 140], [140, 90]])
paint_stroke(c, get_brush("filbert").with_(size=12), [200, 120, 40], [[20, 170, 0.3], [200, 165, 1], [380, 175, 0.2]])
c.save("out.png")
```

## Tests

```bash
python -m pytest
```

They cover determinism, duplicate control points, stroke direction at joints, the pressure curves, the Gaussian profile, spacing-independent pickup, dry backgrounds, and smudging.

## Credits

The ideas of paint depletion, colour pickup and short-stroke painting were learned from [paint-mcp](https://github.com/joeyballentine/paint-mcp). No code was taken from it.
