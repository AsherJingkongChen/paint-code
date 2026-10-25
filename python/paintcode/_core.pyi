"""The paintcode engine, in Rust. Every function takes a scene as a dict (anything json.dumps
takes) or as JSON text, and raises ValueError with a message naming the stroke when the scene is
wrong.

A scene:

    {"canvas":  {"width": 640, "height": 640, "background": "#2a2f4a", "seed": 24},
     "brushes": {"petal": {"preset": "filbert", "size": 8.5}},
     "strokes": [
        {"brush": "petal", "color": [234, 164, 52], "points": [[330, 270, 0.3], [380, 240, 1.0]]},
        {"brush": "ink", "color": "#e8d9b0", "size": 1.4, "points": [[340, 268], [400, 246]]},
        {"brush": "blender", "color": None, "points": [[300, 300], [360, 300]]}]}

points are [x, y], [x, y, pressure] or [x, y, pressure, tilt]; pressure and tilt are 0..1.
color None loads the brush with the paint under its first touch (a smudge). Any brush field may
be set on a stroke directly. The same scene and seed always give the same pixels.
"""

import os
from typing import Any, Union

Scene = Union[str, dict[str, Any]]
"""A scene as a dict, or as JSON text."""

Base = Union[bytes, str, os.PathLike[str]]
"""A base image: PNG bytes, or a path to a PNG."""

PRESETS: dict[str, dict[str, Any]]
"""Brush presets by name (ink, pencil, nib, round, filbert, flat, airbrush, dry, blender), each
with every brush field. A stroke may name a preset and override any field, e.g.
{"brush": "filbert", "size": 12}; or copy one into the scene's "brushes" and change it there."""

__version__: str

def render_png(
    scene: Scene, *, scale: float = 1.0, supersample: int | None = None, base: Base | None = None
) -> bytes:
    """Render a scene to PNG bytes (8-bit RGB).

    scale renders larger: coordinates and brush sizes scale together.
    supersample paints at N times the size and box-filters down; the default is
    canvas.supersample (1). Use 2 or 3 for crisp hard edges.
    base paints over an existing PNG, at its own size, as dry paint: brushes do not pick it up,
    a smudge does. With base, scale and supersample must be 1.
    """

def render_ply(
    scene: Scene, *, scale: float = 1.0, supersample: int | None = None, base: Base | None = None
) -> bytes:
    """Render a scene and return every dab as a flat 3D Gaussian: a binary PLY in the 3D Gaussian
    Splatting layout (SH degree 0). Paint order becomes depth, so splat viewers composite the dabs
    in the order they were painted when looking at the plane. Options as render_png."""

def scene_to_petal(scene: Scene, *, preview: bool | bytes = True) -> bytes:
    """Pack a scene as a .petal project: a ZIP with meta.json, scene.json (exactly as given, keys
    in order) and a small preview.png. It stores strokes, never dabs; the same inputs always give
    the same bytes.

    preview True renders the scene for the preview; False leaves it out; PNG bytes (a render you
    already have, e.g. from render_png) are shrunk and used without rendering again."""

def petal_to_scene(petal: bytes) -> dict[str, Any]:
    """The scene inside a .petal project, keys in their written order."""
