"""The Rust core: the same engine, faster. Same scene in, the same pixels out (to 1e-5).

    from paintcode import core
    png = core.render_png(scene, scale=2)        # scene is a dict or a JSON string
    img = core.render_array(scene)               # float32 (height, width, 3) in 0..1

What the core cannot do yet (PLY, .petal, painting over a base image) stays with the Python
engine in this package, which is also the reference the core is tested against.
"""

from __future__ import annotations

import json

import numpy as np

try:
    from . import _core
except ImportError as e:  # a source checkout that was never built
    raise ImportError("paintcode._core is not built; run `uv sync` or `pip install .` (needs Rust)") from e


def _json(scene) -> str:
    return scene if isinstance(scene, str) else json.dumps(scene)


def render_png(scene, *, scale: float = 1.0, supersample: int | None = None) -> bytes:
    """Render a scene to PNG bytes."""
    return _core.render_png(_json(scene), scale=scale, supersample=supersample)


def render_array(scene, *, scale: float = 1.0, supersample: int | None = None) -> np.ndarray:
    """Render a scene to its float canvas, float32 (height, width, 3) in 0..1."""
    w, h, raw = _core.render_f32(_json(scene), scale=scale, supersample=supersample)
    return np.frombuffer(raw, np.float32).reshape(h, w, 3)
