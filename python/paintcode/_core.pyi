"""The paintcode engine, in Rust.

A flower is what you write: a size, a background and petals (strokes of any shape).
Painting turns each petal into dabs; all the dabs together are the coat.
The coat over the background is the canvas, the pixels you see.

A flower:

    {"width": 640, "height": 640, "background": "#2a2f4a", "seed": 24,
     "brushes": {"soft": {"brush": "filbert", "size": 8.5}},
     "petals": [
        {"brush": "soft", "color": [234, 164, 52], "points": [[330, 270, 0.3], [380, 240, 1.0]]},
        {"brush": "ink", "color": "#e8d9b0", "size": 1.4, "points": [[340, 268], [400, 246]]},
        {"brush": "blender", "color": None, "points": [[300, 300], [360, 300]]}]}

points are [x, y], [x, y, pressure] or [x, y, pressure, tilt]; pressure and tilt are 0..1.
color None loads the brush with the paint under its first touch (a smudge). Any brush field may
be set on a petal directly. The same flower always gives the same pixels.

The files: a flower is a .flower.json, or rides on its canvas as a .flower.png; a canvas is a
.png; a coat is a .coat.ply. A load_ takes the path of a file and tells formats apart by its bytes,
not its name; a save_ takes a pathname, the path without a suffix, adds the suffix of what it
writes and returns the path written. A mistake in a
flower or a file is a ValueError that names the petal or the path.

A function with more than one parameter takes them by keyword only.
"""

import os
import pathlib
from typing import Any, Union

Flower = dict[str, Any]
"""A flower, as a dict (anything json.dumps takes)."""

Path = Union[str, os.PathLike[str]]
"""A path, or for a save_ a pathname: the path without its suffix."""

BRUSHES: dict[str, dict[str, Any]]
"""The brushes by name (ink, pencil, nib, round, filbert, flat, airbrush, dry, blender), each with
every field. A petal may name a brush and change any field, e.g. {"brush": "filbert", "size": 12};
or copy one into the flower's "brushes" and change it there."""

__version__: str

class Canvas:
    """The pixels you see, 8-bit RGB, and the flower on them if one was put there."""

    @property
    def width(self) -> int: ...
    @property
    def height(self) -> int: ...
    def peek(self, *, x: int, y: int) -> tuple[int, int, int]:
        """One pixel as (r, g, b), 0..255."""
    def crop(self, *, x: int, y: int, width: int, height: int) -> "Canvas":
        """A rectangle of this canvas as a canvas of its own. The flower stays behind."""
    def pixels(self) -> bytes:
        """Every pixel, row by row, as RGB bytes:
        numpy.frombuffer(c.pixels(), "u1").reshape(c.height, c.width, 3)."""
    def png(self) -> bytes:
        """The canvas as PNG bytes, with its flower if one is on it: what save_canvas writes."""

class Coat:
    """Every dab a flower laid, in order: what coat_to_canvas lays again. len() counts the dabs."""

    @property
    def width(self) -> int:
        """The canvas it makes, in pixels."""
    @property
    def height(self) -> int: ...
    @property
    def supersample(self) -> int: ...
    def __len__(self) -> int: ...

def load_flower(path: Path) -> Flower:
    """The flower in a .flower.json, or on a .flower.png; keys in their written order. A PNG with
    no flower on it is an error."""

def save_flower(*, flower: Flower, pathname: Path) -> pathlib.Path:
    """Write a flower as JSON, one petal per line, at pathname.flower.json. A flower the engine
    would refuse is refused."""

def load_canvas(path: Path) -> Canvas:
    """The canvas in a PNG: only the pixels, whether or not a flower is on it (load_flower takes
    the flower). Any PNG opens: grey, palette, alpha and 16-bit images become 8-bit RGB."""

def save_canvas(*, canvas: Canvas, pathname: Path) -> pathlib.Path:
    """Write a canvas as a PNG at pathname.png, or with its flower on it at pathname.flower.png."""

def load_coat(path: Path) -> Coat:
    """The coat in a .coat.ply. A 3DGS file from elsewhere is not a coat and is refused."""

def save_coat(*, coat: Coat, pathname: Path) -> pathlib.Path:
    """Write a coat at pathname.coat.ply, a binary PLY in the 3D Gaussian Splatting layout (SH degree 0), which any
    splat viewer opens: paint order becomes depth, so the dabs composite in the order they were
    laid when looking at the plane. Every dab is also kept exactly, so load_coat gives the same
    coat back."""

def flower_to_canvas(*, flower: Flower, scale: float = 1.0, supersample: int = 1) -> Canvas:
    """Paint a flower: its canvas.

    scale paints larger: coordinates and brush sizes scale together.
    supersample paints at N times the size and box-filters down; 2 or 3 give crisp hard edges.
    """

def flower_to_coat(*, flower: Flower, scale: float = 1.0, supersample: int = 1) -> Coat:
    """Paint a flower and keep every dab it lays: its coat. Options as flower_to_canvas."""

def coat_to_canvas(coat: Coat) -> Canvas:
    """Lay a coat's dabs again: the same canvas, bit for bit, as painting its flower."""

def flower_on_canvas(*, flower: Flower, canvas: Canvas) -> Canvas:
    """The canvas with a flower on it, which save_canvas writes into the PNG. Any flower that was
    on it is replaced."""

def flower_off_canvas(canvas: Canvas) -> Flower:
    """The flower on a canvas. A canvas with no flower on it is an error."""
