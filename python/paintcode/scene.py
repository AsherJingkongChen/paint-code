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


def render(scene: dict, base=None, scale: float = 1.0, record_dabs: bool = False,
           supersample: int | None = None) -> Canvas:
    """Paint a scene. supersample N paints at N times the size and box-filters down, so hard
    brush edges come out crisp without stair-steps. It defaults to canvas.supersample (1)."""
    cfg = scene.get("canvas", {})
    seed = int(cfg.get("seed", 0))
    ss = int(supersample if supersample is not None else cfg.get("supersample", 1))
    if ss < 1:
        raise ValueError("supersample must be >= 1")
    if base is not None:
        canvas = Canvas.from_image(base, seed=seed, record_dabs=record_dabs)
        scale, ss = 1.0, 1
    else:
        w, h = int(cfg.get("width", 800)), int(cfg.get("height", 600))
        canvas = Canvas(round(w * scale) * ss, round(h * scale) * ss, cfg.get("background", [255, 255, 255]),
                        seed=seed, record_dabs=record_dabs)
    out_scale = scale
    scale = scale * ss

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
    if ss > 1:
        canvas.downsample(ss)
    canvas.scale = out_scale
    return canvas


def load(path) -> dict:
    """Read a scene from a .json file or a .petal project."""
    if Path(path).suffix.lower() == ".petal":
        from .project import load as load_project
        return load_project(path)
    return json.loads(Path(path).read_text())
