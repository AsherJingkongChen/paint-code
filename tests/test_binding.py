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
FLOWER = json.loads((GOLDEN / "features.flower.json").read_text())


def pixels(png: bytes) -> np.ndarray:
    return np.asarray(Image.open(io.BytesIO(png)).convert("RGB"), np.int16)


def test_the_canvas_is_the_golden_painting():
    got = pixels(paintcode.flower_to_canvas(flower=FLOWER).png())
    want = pixels((GOLDEN / "features.png").read_bytes())
    assert np.abs(got - want).max() <= 1


def test_a_path_may_be_a_str_or_a_pathlike(tmp_path):
    path = GOLDEN / "features.flower.json"
    assert paintcode.load_flower(path) == paintcode.load_flower(str(path)) == FLOWER


def test_a_flower_rides_on_its_canvas(tmp_path):
    canvas = paintcode.flower_to_canvas(flower=FLOWER)
    on = paintcode.flower_on_canvas(flower=FLOWER, canvas=canvas)
    path = paintcode.save_canvas(canvas=on, pathname=tmp_path / "features")
    assert path == tmp_path / "features.flower.png"
    back = paintcode.load_flower(path)
    assert back == FLOWER and list(back) == list(FLOWER)
    assert paintcode.flower_off_canvas(on) == FLOWER
    # the loader of a canvas takes only the pixels, and any PNG reader sees them
    assert paintcode.load_canvas(path).pixels() == canvas.pixels()
    assert (pixels(path.read_bytes()) == pixels(canvas.png())).all()
    with pytest.raises(ValueError, match="no flower on this canvas"):
        paintcode.flower_off_canvas(paintcode.load_canvas(path))


def test_a_flower_file_round_trip_keeps_its_key_order(tmp_path):
    path = paintcode.save_flower(flower=FLOWER, pathname=tmp_path / "features")
    assert path == tmp_path / "features.flower.json"
    back = paintcode.load_flower(path)
    assert back == FLOWER and list(back) == list(FLOWER)
    assert path.read_text().count("\n") == len(FLOWER["petals"]) + 2   # one petal per line


def test_the_example_flowers_open():
    kiku = paintcode.load_flower(ROOT / "examples" / "kiku.flower.json")
    assert len(kiku["petals"]) == 629


def test_the_coat_file_has_one_splat_per_dab_and_reads_back(tmp_path):
    coat = paintcode.flower_to_coat(flower=FLOWER, supersample=2)
    path = paintcode.save_coat(coat=coat, pathname=str(tmp_path / "features"))
    assert path == tmp_path / "features.coat.ply"
    ply = path.read_bytes()
    head = ply[: ply.index(b"end_header")].decode()
    assert "comment canvas 160 90" in head
    n = int(head.split("element vertex ")[1].split()[0])
    body = len(ply) - (ply.index(b"end_header") + len(b"end_header\n"))
    # 17 floats in the 3DGS layout, then the dab exactly: 9 doubles and its colour
    assert n == len(coat) > 100 and body == n * (17 * 4 + 9 * 8 + 3 * 4)
    again = paintcode.coat_to_canvas(paintcode.load_coat(path))
    assert again.pixels() == paintcode.flower_to_canvas(flower=FLOWER, supersample=2).pixels()


def test_mistakes_are_value_errors_that_name_the_petal_or_the_file(tmp_path):
    bad = dict(FLOWER, petals=[{"brush": "ink", "sise": 3, "points": [[1, 1]]}])
    with pytest.raises(ValueError, match="petal 0.*sise"):
        paintcode.flower_to_canvas(flower=bad)
    with pytest.raises(ValueError, match="petal 0: petal has no points"):
        paintcode.save_flower(flower=dict(FLOWER, petals=[{"brush": "ink"}]), pathname=tmp_path / "x")
    with pytest.raises(ValueError, match="width must be a whole number"):
        paintcode.flower_to_canvas(flower={"width": 200.7, "height": 90})
    (tmp_path / "bad.flower.json").write_text("{not json")
    with pytest.raises(ValueError, match="bad.flower.json: flower is not JSON"):
        paintcode.load_flower(tmp_path / "bad.flower.json")
    bare = paintcode.save_canvas(canvas=paintcode.flower_to_canvas(flower=FLOWER), pathname=tmp_path / "bare")
    assert bare.name == "bare.png"
    with pytest.raises(ValueError, match="bare.png: no flower on this PNG"):
        paintcode.load_flower(tmp_path / "bare.png")
    with pytest.raises(ValueError, match="not a PLY"):
        paintcode.load_coat(tmp_path / "bare.png")
    with pytest.raises(FileNotFoundError):
        paintcode.load_canvas(tmp_path / "missing.png")


def test_a_huge_canvas_is_an_error_and_python_lives_on():
    # before 0.5 this was an allocation abort that took the interpreter with it
    code = ("import paintcode\n"
            "try:\n"
            "    paintcode.flower_to_canvas(flower={'width': 100000, 'height': 100000})\n"
            "except ValueError as e:\n"
            "    print('caught', e)\n")
    r = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=60)
    assert r.returncode == 0 and r.stdout.startswith("caught"), r.stderr


def test_a_hostile_png_is_an_error_and_python_lives_on(tmp_path):
    # a PNG header that claims 100000 x 100000 pixels; before the fix the allocation aborted
    code = ("import struct, sys, zlib, paintcode\n"
            "def chunk(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d))\n"
            "png = (b'\\x89PNG\\r\\n\\x1a\\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 100000, 100000, 8, 2, 0, 0, 0))\n"
            "       + chunk(b'IDAT', zlib.compress(b'')) + chunk(b'IEND', b''))\n"
            "open(sys.argv[1], 'wb').write(png)\n"
            "try:\n"
            "    paintcode.load_canvas(sys.argv[1])\n"
            "except ValueError as e:\n"
            "    print('caught', e)\n")
    r = subprocess.run([sys.executable, "-c", code, str(tmp_path / "huge.png")],
                       capture_output=True, text=True, timeout=60)
    assert r.returncode == 0 and "too large" in r.stdout, r.stdout + r.stderr


def test_a_grey_or_rgba_png_opens_like_rgb(tmp_path):
    rgb = Image.open(io.BytesIO(paintcode.flower_to_canvas(flower=FLOWER).png())).convert("RGB")
    want = np.asarray(rgb).tobytes()
    for mode in ("RGBA", "L", "P"):
        im = rgb.convert(mode)
        im.save(tmp_path / f"{mode}.png")
        got = paintcode.load_canvas(tmp_path / f"{mode}.png").pixels()
        assert got == np.asarray(im.convert("RGB")).tobytes(), mode
    assert paintcode.load_canvas(tmp_path / "RGBA.png").pixels() == want


def test_more_than_one_parameter_means_keywords():
    with pytest.raises(TypeError):
        paintcode.flower_to_canvas(FLOWER)
    with pytest.raises(TypeError):
        paintcode.flower_to_canvas(flower=FLOWER).peek(1, 2)
    paintcode.load_flower(GOLDEN / "features.flower.json")    # one parameter: positional is fine


def test_a_canvas_can_be_peeked_and_cropped():
    canvas = paintcode.flower_to_canvas(flower=FLOWER)
    image = pixels(canvas.png())
    assert (canvas.width, canvas.height) == (160, 90)
    assert canvas.peek(x=31, y=7) == tuple(image[7, 31])
    part = canvas.crop(x=30, y=5, width=50, height=20)
    assert (part.width, part.height) == (50, 20)
    assert (pixels(part.png()) == image[5:25, 30:80]).all()
    for x, y, w, h in [(150, 0, 20, 10), (0, 0, 0, 10)]:
        with pytest.raises(ValueError, match="does not fit"):
            canvas.crop(x=x, y=y, width=w, height=h)
    with pytest.raises(ValueError, match="outside"):
        canvas.peek(x=160, y=0)


def test_the_brushes_show_their_fields():
    assert {"ink", "filbert", "blender"} <= set(paintcode.BRUSHES)
    filbert = paintcode.BRUSHES["filbert"]
    assert filbert["size"] == 5.2 and filbert["hardness"] == 0.8 and filbert["angle"] is None
    # a brush's fields written out in the flower paint the same as naming the brush
    by_name = dict(FLOWER, petals=[{"brush": "filbert", "color": "#a33", "points": [[10, 10], [150, 80]]}])
    by_fields = dict(FLOWER, petals=[{"brush": filbert, "color": "#a33", "points": [[10, 10], [150, 80]]}])
    assert (paintcode.flower_to_canvas(flower=by_name).pixels()
            == paintcode.flower_to_canvas(flower=by_fields).pixels())


def test_the_stub_says_what_the_module_has():
    """_core.pyi is written by hand (pyo3-stub-gen does not support pyo3 0.30 yet), so it is
    checked here: the same functions and classes, the same parameters, positional or keyword-only
    alike."""
    from paintcode import _core
    stub = ast.parse(Path(_core.__file__).with_name("_core.pyi").read_text())
    funcs = {n.name: n for n in stub.body if isinstance(n, ast.FunctionDef)}
    classes = {n.name: n for n in stub.body if isinstance(n, ast.ClassDef)}
    names = {n.target.id for n in stub.body if isinstance(n, ast.AnnAssign)}
    real = {k for k, v in vars(_core).items() if callable(v) and not k.startswith("_")}
    assert set(funcs) | set(classes) == real
    assert {"BRUSHES", "__version__"} <= names and all(hasattr(_core, n) for n in names)

    def same(node, real_fn, is_method):
        want = [(a.arg, "POSITIONAL_OR_KEYWORD") for a in node.args.args[is_method:]]
        want += [(a.arg, "KEYWORD_ONLY") for a in node.args.kwonlyargs]
        params = list(inspect.signature(real_fn).parameters.values())[is_method:]
        got = [(p.name, p.kind.name) for p in params]
        assert got == want, node.name

    for name, node in funcs.items():
        same(node, getattr(_core, name), 0)
    for name, node in classes.items():
        cls = getattr(_core, name)
        for m in node.body:
            if not isinstance(m, ast.FunctionDef):
                continue
            attr = inspect.getattr_static(cls, m.name)
            if isinstance(attr, property) or type(attr).__name__ == "getset_descriptor":
                continue
            if not m.name.startswith("__"):
                same(m, getattr(cls, m.name), 1)
