"""Every preset drawing the same S-curve, pressure rising then falling."""
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from PIL import ImageDraw  # noqa: E402

from paintcode import PRESETS, render  # noqa: E402

names = list(PRESETS)
W, ROW = 720, 70
strokes = []
for i, name in enumerate(names):
    y = 40 + i * ROW
    pts = []
    for k in range(9):
        t = k / 8
        pts.append([160 + 520 * t, y + 18 * math.sin(t * 2 * math.pi), round(math.sin(t * math.pi) ** 0.8, 3)])
    colour = None if name == "blender" else "#2b2d42"   # null colour: smudge with what is under the brush
    strokes.append({"brush": name, "color": colour, "points": pts})

# give the blender something to smear
blend_y = 40 + names.index("blender") * ROW
for j in range(6):
    strokes.insert(0, {"brush": "flat", "color": ["#e76f51", "#2a9d8f", "#264653"][j % 3],
                       "points": [[180 + j * 85, blend_y - 22], [180 + j * 85, blend_y + 22]], "size": 14})

scene = {"canvas": {"width": W, "height": 40 + ROW * len(names), "background": "#f4efe6", "seed": 2},
         "strokes": strokes}
out = Path(__file__).with_suffix("")
out.with_suffix(".json").write_text(json.dumps(scene, indent=1))
img = render(scene).to_image()
d = ImageDraw.Draw(img)
for i, name in enumerate(names):
    d.text((24, 40 + i * ROW - 6), name, fill=(80, 84, 100))
img.save(out.with_suffix(".png"))
print("rows:", ", ".join(names))
