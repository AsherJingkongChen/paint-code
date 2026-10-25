"""A yellow chrysanthemum at dusk. Painted by Kiku, stroke by stroke, from code."""
import math
import random
from pathlib import Path

from paintcode import flower_to_canvas, save_canvas, save_flower

W, H = 640, 640
rnd = random.Random(1024)
S = []


def stroke(brush, color, pts, **kw):
    S.append({"brush": brush, "color": color, "points": [[round(v, 2) for v in p] for p in pts], **kw})


def mix(a, b, t):
    return [round(a[i] + (b[i] - a[i]) * t) for i in range(3)]


def jit(c, n=10):
    return [max(0, min(255, v + rnd.randint(-n, n))) for v in c]


# ---- sky: soft airbrush bands, then a scumble of short flat strokes on top
TOP, MID, LOW = [34, 40, 72], [92, 70, 96], [196, 128, 92]
for i in range(14):
    y = i * H / 13
    t = y / H
    c = mix(TOP, MID, t / 0.55) if t < 0.55 else mix(MID, LOW, (t - 0.55) / 0.45)
    stroke("airbrush", c, [[-60, y], [W + 60, y + rnd.uniform(-10, 10)]], size=70, flow=0.5)
for i in range(260):
    y = rnd.uniform(0, H)
    x = rnd.uniform(-20, W)
    t = y / H
    c = mix(TOP, MID, t / 0.55) if t < 0.55 else mix(MID, LOW, (t - 0.55) / 0.45)
    ln = rnd.uniform(30, 90)
    stroke("flat", jit(c, 6), [[x, y, 0.5], [x + ln * 0.5, y + rnd.uniform(-4, 4), 0.9], [x + ln, y + rnd.uniform(-6, 6), 0.4]],
           size=rnd.uniform(5.5, 10), flow=0.22, grain=0.5)

CX, CY = 330, 280

# ---- stem and leaves
stroke("round", [52, 74, 50], [[CX + 10, CY + 60, 1], [CX + 30, CY + 200, 0.9], [CX + 15, CY + 330, 0.8], [CX + 40, H + 20, 0.7]],
       size=5.3, hardness=0.4)
for (lx, ly, ang, ln) in [(CX + 28, CY + 225, 205, 120), (CX + 32, CY + 295, -28, 135), (CX + 22, CY + 385, 215, 105)]:
    a = math.radians(ang)
    ux, uy = math.cos(a), math.sin(a)          # along the midrib
    nx, ny = -uy, ux                           # across it

    def width(t):   # half-width of a lobed chrysanthemum leaf
        return 0.32 * ln * math.sin(math.pi * t) ** 0.7 * (1 + 0.28 * math.sin(t * 3 * math.pi + 0.6))
    for side in (-1, 1):
        for k in range(16):
            t = 0.04 + 0.92 * k / 15
            mx, my = lx + ux * ln * t, ly + uy * ln * t
            w = width(t)
            ex, ey = mx + side * nx * w + ux * w * 0.55, my + side * ny * w + uy * w * 0.55
            shade = 0.25 + 0.5 * t + (0.15 if side > 0 else 0)
            col = mix([34, 60, 40], [86, 120, 70], shade)
            stroke("filbert", jit(col, 6), [[mx, my, 0.9], [(mx + ex) / 2, (my + ey) / 2, 1.0], [ex, ey, 0.35]],
                   size=4.3, half_life=None, pickup=0.08)
    stroke("blender", None, [[lx + ux * 8, ly + uy * 8], [lx + ux * ln * 0.9, ly + uy * ln * 0.9]], size=10, pickup=0.35, flow=0.25)
    stroke("ink", [140, 170, 110], [[lx, ly, 0.8], [lx + ux * ln * 0.5, ly + uy * ln * 0.5, 0.6], [lx + ux * ln * 0.92, ly + uy * ln * 0.92, 0.05]],
           size=0.92, flow=0.5)

# ---- the flower: back petals first, darker and longer; then lighter, shorter, curling in
LAYERS = [  # count, length, colour, size, curl
    (34, 165, [170, 92, 28], 4.9, 0.10),
    (30, 145, [210, 128, 36], 4.9, 0.16),
    (28, 122, [234, 164, 52], 4.9, 0.24),
    (24, 96, [246, 194, 76], 4.6, 0.34),
    (20, 70, [250, 214, 110], 4.3, 0.48),
    (18, 46, [252, 226, 140], 4.0, 0.62),
    (12, 26, [250, 210, 120], 3.4, 0.80),
]
TILT = 0.78   # the flower faces us slightly from above: squash the vertical axis
for li, (n, L, col, size, curl) in enumerate(LAYERS):
    for i in range(n):
        a = 2 * math.pi * (i + 0.5 * (li % 2)) / n + rnd.uniform(-0.06, 0.06)
        length = L * rnd.uniform(0.85, 1.1) * (1.0 - 0.12 * max(0.0, math.sin(a)))   # petals toward us read shorter
        bend = rnd.choice([-1, 1]) * rnd.uniform(0.05, 0.18)
        pts = []
        for k in range(6):
            s = k / 5
            r = (12 if li < 4 else 3) + s * length
            # curl: the tip turns back toward the centre for inner layers
            rr = r - curl * length * max(0.0, s - 0.55) * 1.6
            aa = a + bend * s * s
            p = 0.3 + 0.7 * math.sin(math.pi * 0.5 * min(1.0, s / 0.7)) if k < 5 else 0.45
            pts.append([CX + rr * math.cos(aa), CY + rr * math.sin(aa) * TILT, p])
        stroke("filbert", jit(col, 12), pts, size=size)
        if li >= 2 and rnd.random() < 0.55:   # a light ridge down the middle of the petal
            hl = [mix(col, [255, 240, 190], 0.4)]
            stroke("ink", hl[0], [[q[0], q[1], q[2] * 0.7] for q in pts[1:5]], size=0.92, flow=0.3)

# ---- the heart: tight dabs, then a smudge so it reads as one mass
for i in range(30):
    r = abs(rnd.gauss(0, 4))
    a = rnd.uniform(0, 2 * math.pi)
    x, y = CX + r * math.cos(a), CY + r * math.sin(a) * TILT
    stroke("round", jit([214, 150, 54], 14), [[x, y, rnd.uniform(0.4, 0.9)]], size=2)

flower = {"width": W, "height": H, "background": "#2a2f4a", "seed": 24, "petals": S}
out = Path(__file__).with_suffix("")
save_flower(flower=flower, path=out.with_suffix(".flower.json"))
save_canvas(canvas=flower_to_canvas(flower=flower, supersample=2), path=out.with_suffix(".png"))
print(len(S), "petals")
