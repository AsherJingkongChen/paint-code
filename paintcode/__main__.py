"""python -m paintcode scene.json -o out.png [--base in.png] [--scale 2] [--dabs dabs.json]"""

import argparse
import json
import sys
import time

from .scene import load, render


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="paintcode", description="Render a paint-code JSON scene to PNG.")
    ap.add_argument("scene", help="scene JSON file, or - for stdin")
    ap.add_argument("-o", "--out", required=True, help="output PNG")
    ap.add_argument("--base", help="paint on top of this image instead of a blank canvas")
    ap.add_argument("--scale", type=float, default=1.0, help="render larger; coordinates and brush sizes scale with it")
    ap.add_argument("--dabs", help="also write every dab as a 2D Gaussian record (JSON)")
    a = ap.parse_args(argv)

    scene = json.load(sys.stdin) if a.scene == "-" else load(a.scene)
    t = time.perf_counter()
    canvas = render(scene, base=a.base, scale=a.scale, record_dabs=bool(a.dabs))
    canvas.save(a.out)
    if a.dabs:
        with open(a.dabs, "w") as f:
            json.dump({"width": canvas.width, "height": canvas.height, "dabs": canvas.dabs}, f)
    n = len(canvas.dabs) if canvas.dabs is not None else None
    print(f"{a.out}  {canvas.width}x{canvas.height}  {time.perf_counter() - t:.2f}s" + (f"  {n} dabs" if n is not None else ""))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
