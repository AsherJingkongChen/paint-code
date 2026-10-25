"""The Rust core (paintcode.core) against the Python engine, the reference.

Both engines run the same float32 pixel arithmetic, so they agree to a few float32 ulps; the
remaining difference is numpy's vectorised exp and pow against the platform's libm. The gate is
1e-5 on the float canvas (not on the PNG, where 8-bit rounding can flip a value by 1/255).

Skipped when the core is not built (`uv sync` or `pip install .` builds it; both need Rust).
"""
import json
from pathlib import Path

import numpy as np
import pytest

from paintcode import render

core = pytest.importorskip("paintcode.core", exc_type=ImportError)

EXAMPLES = Path(__file__).resolve().parents[1] / "examples"
GATE = 1e-5


def rust(scene, **kw):
    return core.render_array(scene, **kw)


def gap(scene, **kw):
    a, b = rust(scene, **kw), render(scene, **kw).img
    assert a.shape == b.shape
    return float(np.abs(a - b).max())


def test_brush_sheet():
    assert gap(json.loads((EXAMPLES / "brush_sheet.json").read_text())) < GATE


@pytest.mark.skipif("not config.getoption('--slow', default=False)", reason="a minute in Python; run with --slow")
def test_kiku():
    assert gap(json.loads((EXAMPLES / "kiku.json").read_text())) < GATE


SCENE = {"canvas": {"width": 160, "height": 90, "background": "#203040", "seed": 5},
         "brushes": {"wet": {"preset": "flat", "pickup": 0.4, "size": 9}},
         "strokes": [
             {"brush": "round", "color": "#e0a040", "points": [[10, 20, 0.2], [60, 70, 1], [110, 30, 0.4]]},
             {"brush": "wet", "color": [64, 160, 224], "points": [[10, 80, 0.5, 0.3], [150, 10, 1, 0.8]]},
             {"brush": "pencil", "color": "#fff", "smooth": False, "points": [[20, 45], [100, 50], [140, 20]]},
             {"brush": {"preset": "nib", "angle": 10}, "color": "#111", "points": [[30, 10], [30, 80]]},
             {"brush": "blender", "color": None, "points": [[30, 30], [90, 60]]},
             {"brush": "dry", "color": "#a33", "pos_jitter": 0.3, "flow_jitter": 0.2, "points": [[5, 5], [155, 85]]},
         ]}


@pytest.mark.parametrize("kw", [{}, {"scale": 2.0}, {"supersample": 3}, {"scale": 0.5, "supersample": 2}])
def test_every_feature(kw):
    assert gap(SCENE, **kw) < GATE


def test_png_is_the_same_painting():
    from io import BytesIO
    from PIL import Image
    png = Image.open(BytesIO(core.render_png(SCENE)))
    ref = np.asarray(render(SCENE).to_image(), np.int16)
    assert np.abs(np.asarray(png, np.int16) - ref).max() <= 1


def test_errors_are_value_errors():
    bad = dict(SCENE, strokes=[{"brush": "ink", "sise": 3, "points": [[1, 1]]}])
    with pytest.raises(ValueError, match="stroke 0"):
        core.render_png(bad)


def test_keyword_only():
    with pytest.raises(TypeError):
        core.render_png(SCENE, 2.0)
