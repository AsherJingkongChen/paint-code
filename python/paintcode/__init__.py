"""paintcode: a flower goes in, a canvas comes out.

A flower is what you write: a size, a background and petals (strokes of any shape).
Painting turns each petal into dabs; all the dabs together are the coat.
The coat over the background is the canvas, the pixels you see.

    import paintcode as pc
    flower = pc.load_flower("kiku.flower.json")         # or a .flower.png
    canvas = pc.flower_to_canvas(flower=flower)
    pc.save_canvas(canvas=pc.flower_on_canvas(flower=flower, canvas=canvas), path="kiku.flower.png")
    pc.save_coat(coat=pc.flower_to_coat(flower=flower), path="kiku.coat.ply")

The engine is Rust (paintcode._core); this package is its Python face. See help(paintcode._core)
or the README for the flower format.
"""

from ._core import (
    BRUSHES,
    Canvas,
    Coat,
    __version__,
    coat_to_canvas,
    flower_off_canvas,
    flower_on_canvas,
    flower_to_canvas,
    flower_to_coat,
    load_canvas,
    load_coat,
    load_flower,
    save_canvas,
    save_coat,
    save_flower,
)

__all__ = [
    "BRUSHES",
    "Canvas",
    "Coat",
    "coat_to_canvas",
    "flower_off_canvas",
    "flower_on_canvas",
    "flower_to_canvas",
    "flower_to_coat",
    "load_canvas",
    "load_coat",
    "load_flower",
    "save_canvas",
    "save_coat",
    "save_flower",
]
