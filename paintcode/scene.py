"""JSON scene: canvas settings, named brushes, and an ordered list of strokes."""

from __future__ import annotations

import json
from pathlib import Path

from . import brush as brushes
from .canvas import Canvas
from .stroke import paint_stroke

STROKE_KEYS = {"brush", "color", "points", "smooth"}


def _scaled(b: brushes.Brush, scale: float) -> brushes.Brush:
    if scale == 1:
        return b
    kw = {"size": b.size * scale, "grain_scale": b.grain_scale * scale}
    if b.half_life:
        kw["half_life"] = b.half_life * scale
    return b.with_(**kw)


def render(scene: dict, base=None, scale: float = 1.0, record_dabs: bool = False) -> Canvas:
    cfg = scene.get("canvas", {})
    seed = int(cfg.get("seed", 0))
    if base is not None:
        canvas = Canvas.from_image(base, seed=seed, record_dabs=record_dabs)
        scale = 1.0
    else:
        w, h = int(cfg.get("width", 800)), int(cfg.get("height", 600))
        canvas = Canvas(round(w * scale), round(h * scale), cfg.get("background", [255, 255, 255]),
                        seed=seed, record_dabs=record_dabs)

    named = {name: brushes.get(spec) for name, spec in scene.get("brushes", {}).items()}

    for idx, s in enumerate(scene.get("strokes", [])):
        ref = s.get("brush", "round")
        b = named[ref] if isinstance(ref, str) and ref in named else brushes.get(ref)
        overrides = {k: v for k, v in s.items() if k not in STROKE_KEYS}
        if overrides:
            try:
                b = b.with_(**overrides)
            except ValueError as e:
                raise ValueError(f"stroke {idx}: {e}") from None
        b = _scaled(b, scale)
        pts = [[c * scale if j < 2 else c for j, c in enumerate(pt)] for pt in s["points"]]
        paint_stroke(canvas, b, s.get("color", [0, 0, 0]), pts, smooth=s.get("smooth", True))
    return canvas


def load(path) -> dict:
    return json.loads(Path(path).read_text())
