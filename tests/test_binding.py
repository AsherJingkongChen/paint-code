"""The Python face of the engine. The engine itself is tested in Rust (crates/paintcode/tests);
these check what crosses the boundary: arguments, bytes, errors, files."""
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


def test_the_float_canvas_has_its_shape():
    w, h, data = paintcode.render_f32(SCENE, scale=0.5)
    img = np.frombuffer(data, "<f4").reshape(h, w, 3)
    assert (w, h) == (80, 45) and 0 <= img.min() and img.max() <= 1


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


def test_options_are_keyword_only():
    with pytest.raises(TypeError):
        paintcode.render_png(SCENE, 2.0)


def test_the_presets_are_listed():
    assert {"ink", "filbert", "blender"} <= set(paintcode.PRESETS)


def test_the_command_line(tmp_path):
    scene = tmp_path / "s.json"
    scene.write_text(json.dumps(SCENE))
    outs = [tmp_path / n for n in ("a.png", "a.petal", "a.ply")]
    cmd = [sys.executable, "-m", "paintcode", str(scene)] + [a for o in outs for a in ("-o", str(o))]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stderr
    assert all(o.exists() and o.stat().st_size > 0 for o in outs)
    assert "160x90" in r.stdout and "splats" in r.stdout
    # the project renders like the scene
    again = tmp_path / "b.png"
    r = subprocess.run([sys.executable, "-m", "paintcode", str(outs[1]), "-o", str(again)],
                       capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stderr
    assert again.read_bytes() == outs[0].read_bytes()


def test_the_command_line_says_what_is_wrong(tmp_path):
    r = subprocess.run([sys.executable, "-m", "paintcode", "x.json", "-o", "x.jpg"],
                       capture_output=True, text=True, timeout=60)
    assert r.returncode == 2 and "unknown output format" in r.stderr
    assert paintcode.main(["--help"]) == 0
