"""Freeze the numpy engine's output as golden files, before that engine is retired.

Run once, from the repository root, at paintcode 0.4 (the last version with the numpy engine):

    uv run python tests/golden/make.py

It writes, for every case in CASES,
    <name>.png      the painting, 8-bit, as the numpy engine saved it
    <name>.u16.gz   the float canvas quantised to 16 bits (round(v * 65535)), gzip, little endian,
                    height x width x 3; the Rust tests compare to it within 1e-5 + half a step
and <ply>.ply for the PLY case, plus manifest.json with each case's options and size.
The Rust core matched this engine within 2.4e-7 on kiku.json when these were made.
"""
import gzip
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))
from paintcode import ply  # noqa: E402
from paintcode.scene import render  # noqa: E402

HERE = Path(__file__).resolve().parent
EX = ROOT / "examples"

FEATURES = {"canvas": {"width": 160, "height": 90, "background": "#203040", "seed": 5},
            "brushes": {"wet": {"preset": "flat", "pickup": 0.4, "size": 9}},
            "strokes": [
                {"brush": "round", "color": "#e0a040", "points": [[10, 20, 0.2], [60, 70, 1], [110, 30, 0.4]]},
                {"brush": "wet", "color": [64, 160, 224], "points": [[10, 80, 0.5, 0.3], [150, 10, 1, 0.8]]},
                {"brush": "pencil", "color": "#fff", "smooth": False, "points": [[20, 45], [100, 50], [140, 20]]},
                {"brush": {"preset": "nib", "angle": 10}, "color": "#111", "points": [[30, 10], [30, 80]]},
                {"brush": "blender", "color": None, "points": [[30, 30], [90, 60]]},
                {"brush": "dry", "color": "#a33", "pos_jitter": 0.3, "flow_jitter": 0.2, "points": [[5, 5], [155, 85]]},
                {"brush": "airbrush", "color": "#ffe0b0", "size": 12, "points": [[60, 10, 0.3], [120, 30, 0.9]]},
                {"brush": "ink", "color": "#300", "size": 1.2, "points": [[150, 80], [140, 60], [150, 40]]},
            ]}

PLY_SCENE = {"canvas": {"width": 120, "height": 90, "background": "#203040", "seed": 5},
             "strokes": [
                 {"brush": "round", "hardness": 0, "color": "#e0a040", "points": [[10, 20, 0.2], [60, 70, 1], [110, 30, 0.4]]},
                 {"brush": "filbert", "hardness": 0, "grain": 0, "color": "#40a0e0", "points": [[10, 80], [110, 10]]},
                 {"brush": "ink", "hardness": 0, "color": "#ffffff", "points": [[20, 45, 0.3], [100, 50, 1]]},
                 {"brush": "blender", "color": None, "points": [[30, 30], [90, 60]]},
             ]}


def base_image() -> Path:
    """A small base painting: a gradient with a dark disc, as an 8-bit RGB PNG."""
    y, x = np.mgrid[0:90, 0:160].astype(np.float32)
    img = np.stack([x / 159 * 255, y / 89 * 255, np.full_like(x, 120)], -1)
    img[(x - 50) ** 2 + (y - 45) ** 2 < 25 ** 2] = [20, 20, 30]
    path = HERE / "base.png"
    Image.fromarray(img.astype(np.uint8), "RGB").save(path)
    return path


CASES = [
    # name, scene file, render options
    ("brush_sheet", "examples/brush_sheet.json", {}),
    ("kiku", "examples/kiku.json", {}),
    ("features", "tests/golden/features.json", {}),
    ("features_scale2", "tests/golden/features.json", {"scale": 2.0}),
    ("features_ss3", "tests/golden/features.json", {"supersample": 3}),
    ("features_half_ss2", "tests/golden/features.json", {"scale": 0.5, "supersample": 2}),
    ("features_on_base", "tests/golden/features.json", {"base": "tests/golden/base.png"}),
]

# the float canvas is only kept for the small cases; the large ones keep their PNG
FLOAT = {"features", "features_scale2", "features_ss3", "features_half_ss2", "features_on_base", "ply_scene"}


def main():
    (HERE / "features.json").write_text(json.dumps(FEATURES, indent=1) + "\n")
    (HERE / "ply_scene.json").write_text(json.dumps(PLY_SCENE, indent=1) + "\n")
    base_image()
    manifest = {}
    cases = CASES + [("ply_scene", "tests/golden/ply_scene.json", {"ply": True})]
    for name, scene_file, kw in cases:
        scene = json.loads((ROOT / scene_file).read_text())
        opts = {k: v for k, v in kw.items() if k != "ply"}
        if "base" in opts:
            opts["base"] = str(ROOT / opts["base"])
        c = render(scene, record_dabs=bool(kw.get("ply")), **opts)
        c.to_image().save(HERE / f"{name}.png")
        entry = {"scene": scene_file, "options": kw, "width": c.width, "height": c.height}
        if name in FLOAT:
            q = np.round(np.clip(c.img, 0, 1) * 65535).astype("<u2")
            (HERE / f"{name}.u16.gz").write_bytes(gzip.compress(q.tobytes(), 9, mtime=0))
            entry["float"] = f"{name}.u16.gz"
        if kw.get("ply"):
            bg = [0x20 / 255, 0x30 / 255, 0x40 / 255]
            n = ply.write(HERE / f"{name}.ply", c.dabs, c.width, c.height, background=bg)
            entry["ply"] = f"{name}.ply"
            entry["splats"] = n
        manifest[name] = entry
        print(name, c.width, c.height, flush=True)
    (HERE / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")


if __name__ == "__main__":
    main()
