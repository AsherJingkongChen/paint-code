"""paint-code: a small dab engine. Every brush mark is a row of anisotropic 2D Gaussians."""

from .brush import PRESETS, Brush, get as get_brush
from .canvas import Canvas, parse_color
from .scene import load as load_scene, render
from .stroke import paint_stroke, samples

__all__ = ["Brush", "PRESETS", "get_brush", "Canvas", "parse_color", "paint_stroke", "samples",
           "render", "load_scene"]
__version__ = "0.3.0"
