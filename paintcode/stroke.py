"""Turn control points into a sequence of dabs and lay them down."""

from __future__ import annotations

import math

import numpy as np

from .brush import Brush
from .canvas import Canvas, parse_color


def _normalise(points) -> np.ndarray:
    """Rows of (x, y, pressure, tilt); missing pressure is 1, missing tilt is 0."""
    out = []
    for pt in points:
        pt = list(pt)
        if len(pt) < 2 or len(pt) > 4:
            raise ValueError(f"point must be [x, y], [x, y, p] or [x, y, p, tilt], got {pt}")
        x, y = float(pt[0]), float(pt[1])
        p = float(pt[2]) if len(pt) > 2 else 1.0
        t = float(pt[3]) if len(pt) > 3 else 0.0
        out.append((x, y, min(1.0, max(0.0, p)), min(1.0, max(0.0, t))))
    if not out:
        raise ValueError("stroke has no points")
    a = np.asarray(out, dtype=np.float64)
    # drop consecutive duplicates: a zero-length segment has no direction
    keep = [0]
    for i in range(1, len(a)):
        if math.hypot(a[i, 0] - a[keep[-1], 0], a[i, 1] - a[keep[-1], 1]) > 1e-6:
            keep.append(i)
        else:
            a[keep[-1], 2:] = np.maximum(a[keep[-1], 2:], a[i, 2:])
    return a[keep]


def _catmull_rom(a: np.ndarray, smooth: bool) -> np.ndarray:
    """Dense polyline through the control points (centripetal Catmull-Rom when smooth)."""
    if len(a) < 3 or not smooth:
        dense = [a[0]]
        for i in range(1, len(a)):
            seg = math.hypot(*(a[i, :2] - a[i - 1, :2]))
            n = max(1, int(math.ceil(seg / 1.0)))
            for j in range(1, n + 1):
                dense.append(a[i - 1] + (a[i] - a[i - 1]) * (j / n))
        return np.asarray(dense)

    P = np.vstack([2 * a[0] - a[1], a, 2 * a[-1] - a[-2]])
    dense = [a[0]]
    for i in range(1, len(P) - 2):
        p0, p1, p2, p3 = P[i - 1], P[i], P[i + 1], P[i + 2]

        def knot(ti, pa, pb):
            return ti + max(1e-6, math.hypot(*(pb[:2] - pa[:2]))) ** 0.5

        t0 = 0.0
        t1 = knot(t0, p0, p1)
        t2 = knot(t1, p1, p2)
        t3 = knot(t2, p2, p3)
        seg = math.hypot(*(p2[:2] - p1[:2]))
        n = max(1, int(math.ceil(seg / 1.0)))
        for j in range(1, n + 1):
            t = t1 + (t2 - t1) * (j / n)
            A1 = (t1 - t) / (t1 - t0) * p0[:2] + (t - t0) / (t1 - t0) * p1[:2]
            A2 = (t2 - t) / (t2 - t1) * p1[:2] + (t - t1) / (t2 - t1) * p2[:2]
            A3 = (t3 - t) / (t3 - t2) * p2[:2] + (t - t2) / (t3 - t2) * p3[:2]
            B1 = (t2 - t) / (t2 - t0) * A1 + (t - t0) / (t2 - t0) * A2
            B2 = (t3 - t) / (t3 - t1) * A2 + (t - t1) / (t3 - t1) * A3
            xy = (t2 - t) / (t2 - t1) * B1 + (t - t1) / (t2 - t1) * B2
            attrs = p1[2:] + (p2[2:] - p1[2:]) * (j / n)   # pressure and tilt: linear per segment
            dense.append(np.concatenate([xy, attrs]))
    return np.asarray(dense)


def samples(points, brush: Brush, smooth: bool = True):
    """Yield (x, y, pressure, tilt, direction, travelled_px) at the brush's own spacing."""
    a = _normalise(points)
    if len(a) == 1:
        x, y, p, t = a[0]
        yield x, y, p, t, 0.0, 0.0
        return
    d = _catmull_rom(a, smooth)
    seg = np.hypot(np.diff(d[:, 0]), np.diff(d[:, 1]))
    good = seg > 1e-9
    starts, ends, seg = d[:-1][good], d[1:][good], seg[good]
    if len(seg) == 0:
        x, y, p, t = a[0]
        yield x, y, p, t, 0.0, 0.0
        return

    i, along, travelled = 0, 0.0, 0.0
    while i < len(seg):
        f = along / seg[i]
        cur = starts[i] + (ends[i] - starts[i]) * f
        direction = math.atan2(ends[i, 1] - starts[i, 1], ends[i, 0] - starts[i, 0])
        yield cur[0], cur[1], cur[2], cur[3], direction, travelled
        step = brush.step(cur[2])
        travelled += step
        along += step
        while i < len(seg) and along >= seg[i]:
            along -= seg[i]
            i += 1


def paint_stroke(canvas: Canvas, brush: Brush, color, points, smooth: bool = True) -> int:
    """Lay one stroke onto the canvas. Returns the number of dabs."""
    rng = canvas.rng
    pts = list(samples(points, brush, smooth))
    if color is None:   # a smudge: load the brush with whatever is under its first touch
        carried = canvas.sample(pts[0][0], pts[0][1], brush.radius(pts[0][2])).astype(np.float32) if pts else None
    elif isinstance(color, np.ndarray):
        carried = color.astype(np.float32)
    else:
        carried = parse_color(color)
    n = 0
    last_t = 0.0
    load = brush.load
    snap = None
    if brush.pickup > 0 and pts:
        xs = [q[0] for q in pts]
        ys = [q[1] for q in pts]
        pad = int(math.ceil(3.0 * 0.5 * brush.size * (1.0 + brush.size_jitter) * (1.0 + brush.tilt_stretch)
                            * (1.0 + brush.pos_jitter))) + 3
        ox, oy = max(0, int(min(xs)) - pad), max(0, int(min(ys)) - pad)
        ex, ey = min(canvas.width, int(max(xs)) + pad + 1), min(canvas.height, int(max(ys)) + pad + 1)
        snap = (canvas.img[oy:ey, ox:ex].copy(), canvas.wet[oy:ey, ox:ex].copy(), ox, oy)
    for x, y, p, tilt, direction, travelled in pts:
        if brush.half_life:
            load *= 0.5 ** ((travelled - last_t) / brush.half_life)
            load = max(brush.min_load, load)
        last_t = travelled

        r = brush.radius(p)
        if brush.size_jitter:
            r *= 1.0 + rng.uniform(-brush.size_jitter, brush.size_jitter)
        flow = brush.flow_at(p) * load
        if brush.flow_jitter:
            flow *= 1.0 + rng.uniform(-brush.flow_jitter, brush.flow_jitter)
        flow *= 1.0 - 0.35 * tilt   # a tilted brush lays a thinner film over a wider area
        # flow means the film laid at the default spacing; denser or sparser dabs keep the same build-up
        ref = max(0.35, 0.3 * brush.footprint_along(brush.radius(p)))
        flow = 1.0 - (1.0 - min(flow, 0.999)) ** (brush.step(p) / ref)

        if brush.angle is None:
            theta = direction + math.pi / 2.0
        else:
            theta = math.radians(brush.angle)
        if brush.angle_jitter:
            theta += math.radians(rng.uniform(-brush.angle_jitter, brush.angle_jitter))
        if brush.pos_jitter:
            x += rng.uniform(-1, 1) * brush.pos_jitter * r
            y += rng.uniform(-1, 1) * brush.pos_jitter * r

        sx = 0.5 * r * (1.0 + brush.tilt_stretch * tilt)
        sy = 0.5 * r * brush.aspect
        under = canvas.stamp(x, y, sx, sy, theta, carried, flow, brush.hardness,
                             brush.grain, brush.grain_scale, sample_src=snap)
        if under is not None and brush.pickup > 0:
            # only wet paint is picked up; the rate is per brush-radius of travel, so dab density does not change it
            colour_under, wetness = under
            mix = 1.0 - (1.0 - brush.pickup * wetness) ** (brush.step(p) / max(r, 1e-3))
            carried = carried * (1.0 - mix) + colour_under.astype(np.float32) * mix
        n += 1
    canvas.last_carried = carried   # paint left on the brush, after any pickup
    return n
