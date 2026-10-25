"""python -m paintcode SCENE -o OUT [-o OUT ...]

SCENE is a .json scene or a .petal project (or - for JSON on stdin).
Each -o picks its format by extension:
    .png    the rendered painting
    .petal  the project: strokes, losslessly compressed, plus a small preview
    .ply    every dab as a flat 3D Gaussian (3DGS layout, SH degree 0)
"""

import argparse
import json
import sys
import time
from pathlib import Path

from . import ply, project
from .canvas import parse_color
from .scene import load, render

FORMATS = (".png", ".petal", ".ply")


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="paintcode", description="Render a paintcode scene.",
                                 formatter_class=argparse.RawDescriptionHelpFormatter, epilog=__doc__)
    ap.add_argument("scene", help=".json scene, .petal project, or - for JSON on stdin")
    ap.add_argument("-o", "--out", action="append", required=True, help="output file; repeat for several formats")
    ap.add_argument("--base", help="paint on top of this image instead of a blank canvas")
    ap.add_argument("--scale", type=float, default=1.0, help="render larger; coordinates and brush sizes scale with it")
    ap.add_argument("--ss", type=int, help="supersampling factor (overrides canvas.supersample)")
    a = ap.parse_args(argv)

    outs = [Path(o) for o in a.out]
    bad = [str(o) for o in outs if o.suffix.lower() not in FORMATS]
    if bad:
        ap.error(f"unknown output format {bad}; use {', '.join(FORMATS)}")

    scene = json.load(sys.stdin) if a.scene == "-" else load(a.scene)
    t = time.perf_counter()
    want_ply = any(o.suffix.lower() == ".ply" for o in outs)
    canvas = render(scene, base=a.base, scale=a.scale, supersample=a.ss, record_dabs=want_ply)
    image = canvas.to_image()
    for o in outs:
        ext = o.suffix.lower()
        if ext == ".png":
            image.save(o)
            print(f"{o}  {canvas.width}x{canvas.height}")
        elif ext == ".petal":
            project.save(o, scene, preview=image)
            print(f"{o}  {o.stat().st_size / 1024:.1f} KB, {len(scene.get('strokes', []))} strokes")
        elif ext == ".ply":
            bg = None if a.base else parse_color(scene.get("canvas", {}).get("background", [255, 255, 255]))
            n = ply.write(o, canvas.dabs, canvas.width, canvas.height, background=bg)
            print(f"{o}  {n} splats")
    print(f"rendered in {time.perf_counter() - t:.1f}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
