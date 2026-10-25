"""The Python face of the engine. The engine itself is tested in Rust (crates/paintcode/tests);
these check what crosses the boundary: arguments, bytes, errors, files."""
import ast
import inspect
import io
import json
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

import paintcode

ROOT = Path(__file__).resolve().parents[1]
GOLDEN = ROOT / "tests" / "golden"
SCENE = json.loads((GOLDEN / "features.json").read_text())


def pixels(png: bytes) -> np.ndarray:
    return np.asarray(Image.open(io.BytesIO(png)).convert("RGB"), np.int16)


def test_the_png_is_the_golden_painting():
    got = pixels(paintcode.render_png(SCENE))
    want = pixels((GOLDEN / "features.png").read_bytes())
    assert np.abs(got - want).max() <= 1


def test_a_scene_may_be_json_text():
    assert paintcode.render_png(json.dumps(SCENE)) == paintcode.render_png(SCENE)


def test_a_base_image_from_bytes_or_a_path():
    base = GOLDEN / "base.png"
    by_path = paintcode.render_png(SCENE, base=base)
    by_str = paintcode.render_png(SCENE, base=str(base))
    by_bytes = paintcode.render_png(SCENE, base=base.read_bytes())
    assert by_path == by_str == by_bytes
    want = pixels((GOLDEN / "features_on_base.png").read_bytes())
    assert np.abs(pixels(by_path) - want).max() <= 1


def test_a_petal_round_trip_keeps_the_scene_and_its_key_order():
    petal = paintcode.scene_to_petal(SCENE)
    back = paintcode.petal_to_scene(petal)
    assert back == SCENE and list(back) == list(SCENE)
    assert paintcode.scene_to_petal(SCENE) == petal   # deterministic
    with __import__("zipfile").ZipFile(io.BytesIO(petal)) as z:
        assert set(z.namelist()) == {"meta.json", "scene.json", "preview.png"}
    assert "preview.png" not in __import__("zipfile").ZipFile(
        io.BytesIO(paintcode.scene_to_petal(SCENE, preview=False))).namelist()


def test_the_example_petal_opens():
    scene = paintcode.petal_to_scene((ROOT / "examples" / "kiku.petal").read_bytes())
    assert len(scene["strokes"]) == 629


def test_the_ply_has_one_splat_per_dab():
    ply = paintcode.render_ply(SCENE)
    head = ply[: ply.index(b"end_header")].decode()
    assert "comment canvas 160 90" in head
    n = int(head.split("element vertex ")[1].split()[0])
    body = len(ply) - (ply.index(b"end_header") + len(b"end_header\n"))
    assert n > 100 and body == n * 17 * 4


def test_mistakes_are_value_errors_that_name_the_stroke():
    bad = dict(SCENE, strokes=[{"brush": "ink", "sise": 3, "points": [[1, 1]]}])
    with pytest.raises(ValueError, match="stroke 0.*sise"):
        paintcode.render_png(bad)
    with pytest.raises(ValueError, match="stroke 0: stroke has no points"):
        paintcode.render_png(dict(SCENE, strokes=[{"brush": "ink"}]))
    with pytest.raises(ValueError, match="canvas.width"):
        paintcode.render_png({"canvas": {"width": 200.7, "height": 90}})
    with pytest.raises(ValueError, match="base image"):
        paintcode.render_png(SCENE, base=GOLDEN / "base.png", scale=2)
    with pytest.raises(ValueError, match="not JSON"):
        paintcode.render_png("{not json")


def test_a_huge_canvas_is_an_error_and_python_lives_on():
    # before 0.5 this was an allocation abort that took the interpreter with it
    code = ("import paintcode\n"
            "try:\n"
            "    paintcode.render_png({'canvas': {'width': 100000, 'height': 100000}})\n"
            "except ValueError as e:\n"
            "    print('caught', e)\n")
    r = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=60)
    assert r.returncode == 0 and r.stdout.startswith("caught"), r.stderr


def test_a_hostile_base_png_is_an_error_and_python_lives_on():
    # a PNG header that claims 100000 x 100000 pixels; before the fix the allocation aborted
    code = ("import struct, zlib, paintcode\n"
            "def chunk(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d))\n"
            "png = (b'\\x89PNG\\r\\n\\x1a\\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 100000, 100000, 8, 2, 0, 0, 0))\n"
            "       + chunk(b'IDAT', zlib.compress(b'')) + chunk(b'IEND', b''))\n"
            "try:\n"
            "    paintcode.render_png({'canvas': {'width': 10, 'height': 10}}, base=png)\n"
            "except ValueError as e:\n"
            "    print('caught', e)\n")
    r = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=60)
    assert r.returncode == 0 and "too large" in r.stdout, r.stdout + r.stderr


def test_a_grey_or_rgba_base_works_like_rgb():
    rgb = Image.open(GOLDEN / "base.png").convert("RGB")
    for mode in ("RGBA", "L", "P"):
        im = rgb.convert(mode)
        buf = io.BytesIO()
        im.save(buf, "PNG")
        want = paintcode.render_png(SCENE, base=_png(im.convert("RGB")))
        assert paintcode.render_png(SCENE, base=buf.getvalue()) == want, mode


def _png(im) -> bytes:
    buf = io.BytesIO()
    im.save(buf, "PNG")
    return buf.getvalue()


def test_options_are_keyword_only():
    with pytest.raises(TypeError):
        paintcode.render_png(SCENE, 2.0)


def test_the_presets_show_their_fields():
    assert {"ink", "filbert", "blender"} <= set(paintcode.PRESETS)
    filbert = paintcode.PRESETS["filbert"]
    assert filbert["size"] == 5.2 and filbert["hardness"] == 0.8 and filbert["angle"] is None
    # a preset's fields written out in the scene paint the same as naming the preset
    by_name = dict(SCENE, strokes=[{"brush": "filbert", "color": "#a33", "points": [[10, 10], [150, 80]]}])
    by_fields = dict(SCENE, strokes=[{"brush": filbert, "color": "#a33", "points": [[10, 10], [150, 80]]}])
    assert paintcode.render_png(by_name) == paintcode.render_png(by_fields)


def test_a_petal_can_reuse_a_render_as_its_preview():
    png = paintcode.render_png(SCENE)
    reused = paintcode.scene_to_petal(SCENE, preview=png)
    assert reused == paintcode.scene_to_petal(SCENE)        # the same preview, one render fewer
    with pytest.raises(ValueError, match="preview"):
        paintcode.scene_to_petal(SCENE, preview=b"not a png")


def test_a_project_renders_like_its_scene():
    petal = paintcode.scene_to_petal(SCENE)
    assert paintcode.render_png(paintcode.petal_to_scene(petal)) == paintcode.render_png(SCENE)


def test_the_stub_says_what_the_module_has():
    """_core.pyi is written by hand (pyo3-stub-gen does not support pyo3 0.30 yet), so it is
    checked here: the same functions, the same parameters, positional or keyword-only alike."""
    from paintcode import _core
    stub = ast.parse(Path(_core.__file__).with_name("_core.pyi").read_text())
    funcs = {n.name: n for n in stub.body if isinstance(n, ast.FunctionDef)}
    names = {n.target.id for n in stub.body if isinstance(n, ast.AnnAssign)}
    real = {k for k, v in vars(_core).items() if callable(v) and not k.startswith("_")}
    assert set(funcs) == real
    assert {"PRESETS", "__version__"} <= names and all(hasattr(_core, n) for n in names - {"Scene", "Base"})
    for name, node in funcs.items():
        want = [(a.arg, "POSITIONAL_OR_KEYWORD") for a in node.args.args]
        want += [(a.arg, "KEYWORD_ONLY") for a in node.args.kwonlyargs]
        got = [(p.name, p.kind.name) for p in inspect.signature(getattr(_core, name)).parameters.values()]
        assert got == want, name
