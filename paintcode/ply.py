"""Export a painting's dabs as a 3D Gaussian Splatting PLY, and read one back.

Each dab becomes one flat 3D Gaussian in the layout the original 3DGS code writes
(x y z nx ny nz f_dc_0..2 opacity scale_0..2 rot_0..3, binary little endian, SH degree 0):

    position   image plane, x right and y down (the COLMAP convention 3DGS uses), the longer
               side spanning [-1, 1]; z = depth, later dabs closer to a camera on the -z side
    scale      log sigma: the dab's two axes, and a vanishing third axis
    rotation   quaternion (w, x, y, z) for the dab angle about z
    colour     SH DC term, (rgb - 0.5) / C0
    opacity    logit of the dab's alpha

The 3DGS kernel is a plain Gaussian, so hardness and paper grain do not survive the trip;
a painting made with hardness 0 and no grain round-trips exactly (see render_splats).
"""

from __future__ import annotations

import math

import numpy as np

C0 = 0.28209479177387814
FIELDS = ["x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2", "opacity",
          "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3"]
DEPTH_SPAN = 0.05      # total depth the paint order is spread over, in scene units
THIN = 1e-5            # sigma of the flat axis, in scene units


def dabs_to_splats(dabs: list[dict], width: int, height: int) -> np.ndarray:
    n = len(dabs)
    s = 2.0 / max(width, height)               # px -> scene units
    a = np.zeros((n, len(FIELDS)), dtype=np.float32)
    for i, d in enumerate(dabs):
        alpha = min(max(d["alpha"], 1e-4), 1 - 1e-4)
        half = d["theta"] / 2.0
        a[i] = [
            (d["x"] - width / 2) * s, (d["y"] - height / 2) * s,
            DEPTH_SPAN * (1.0 - i / max(n - 1, 1)),          # first dab farthest, last dab nearest
            0, 0, 0,
            (d["rgb"][0] - 0.5) / C0, (d["rgb"][1] - 0.5) / C0, (d["rgb"][2] - 0.5) / C0,
            math.log(alpha / (1 - alpha)),
            math.log(d["sx"] * s), math.log(d["sy"] * s), math.log(THIN),
            math.cos(half), 0.0, 0.0, math.sin(half),
        ]
    return a


def write(path, dabs: list[dict], width: int, height: int, background=None) -> int:
    a = dabs_to_splats(dabs, width, height)
    header = ["ply", "format binary_little_endian 1.0",
              "comment made with paint-code: one flat Gaussian per brush dab",
              f"comment canvas {width} {height}"]
    if background is not None:
        header.append("comment background " + " ".join(f"{float(c):.6f}" for c in background))
    header.append(f"element vertex {len(a)}")
    header += [f"property float {f}" for f in FIELDS]
    header.append("end_header")
    with open(path, "wb") as f:
        f.write(("\n".join(header) + "\n").encode("ascii"))
        f.write(a.astype("<f4").tobytes())
    return len(a)


def read(path):
    """Returns (array with named columns as a dict, comments)."""
    with open(path, "rb") as f:
        lines, props, count = [], [], 0
        while True:
            line = f.readline().decode("ascii").strip()
            lines.append(line)
            if line.startswith("element vertex"):
                count = int(line.split()[-1])
            elif line.startswith("property"):
                _, typ, name = line.split()
                if typ != "float":
                    raise ValueError(f"only float properties are supported, got {line}")
                props.append(name)
            elif line == "end_header":
                break
        data = np.frombuffer(f.read(4 * len(props) * count), dtype="<f4").reshape(count, len(props))
    comments = [ln[8:] for ln in lines if ln.startswith("comment ")]
    return {name: data[:, i] for i, name in enumerate(props)}, comments


def render_splats(path, scale: float = 1.0, background=None) -> np.ndarray:
    """An independent, deliberately plain 3DGS-style rasteriser for checking an export.

    It reads only the PLY: builds each Gaussian's 3D covariance from scale and rotation,
    projects it orthographically onto the image plane, sorts far to near, and alpha-composites
    exp(-0.5 d^T Sigma^-1 d) * sigmoid(opacity) at pixel centres, cut off at three sigma.
    """
    g, comments = read(path)
    W = H = None
    bg = np.ones(3, np.float32)
    for c in comments:
        if c.startswith("canvas "):
            W, H = map(int, c.split()[1:3])
        if c.startswith("background ") and background is None:
            bg = np.array(list(map(float, c.split()[1:4])), np.float32)
    if background is not None:
        bg = np.asarray(background, np.float32)
    if W is None:
        raise ValueError("PLY has no canvas comment; cannot place the image plane")
    s = 2.0 / max(W, H) / scale
    w, h = round(W * scale), round(H * scale)
    img = np.empty((h, w, 3), np.float32)
    img[:] = bg

    q = np.stack([g["rot_0"], g["rot_1"], g["rot_2"], g["rot_3"]], 1).astype(np.float64)
    q /= np.linalg.norm(q, axis=1, keepdims=True)
    qw, qx, qy, qz = q.T
    R = np.stack([
        np.stack([1 - 2 * (qy * qy + qz * qz), 2 * (qx * qy - qw * qz), 2 * (qx * qz + qw * qy)], -1),
        np.stack([2 * (qx * qy + qw * qz), 1 - 2 * (qx * qx + qz * qz), 2 * (qy * qz - qw * qx)], -1),
        np.stack([2 * (qx * qz - qw * qy), 2 * (qy * qz + qw * qx), 1 - 2 * (qx * qx + qy * qy)], -1),
    ], 1)
    S = np.exp(np.stack([g["scale_0"], g["scale_1"], g["scale_2"]], 1).astype(np.float64))
    M = R * S[:, None, :]
    cov2 = (M @ M.transpose(0, 2, 1))[:, :2, :2] / (s * s)   # orthographic: keep x, y; in output px
    px = g["x"].astype(np.float64) / s + w / 2
    py = g["y"].astype(np.float64) / s + h / 2
    rgb = np.stack([g["f_dc_0"], g["f_dc_1"], g["f_dc_2"]], 1) * C0 + 0.5
    alpha = 1 / (1 + np.exp(-g["opacity"].astype(np.float64)))

    for i in np.argsort(-g["z"], kind="stable"):     # far to near
        a, b, d = cov2[i, 0, 0], cov2[i, 0, 1], cov2[i, 1, 1]
        det = a * d - b * b
        if det <= 0:
            continue
        ia, ib, id_ = d / det, -b / det, a / det
        ext = 3.0 * math.sqrt(max(a, d))
        x0, x1 = max(0, int(math.floor(px[i] - ext))), min(w, int(math.ceil(px[i] + ext)) + 1)
        y0, y1 = max(0, int(math.floor(py[i] - ext))), min(h, int(math.ceil(py[i] + ext)) + 1)
        if x0 >= x1 or y0 >= y1:
            continue
        dx = (np.arange(x0, x1) + 0.5 - px[i])[None, :]
        dy = (np.arange(y0, y1) + 0.5 - py[i])[:, None]
        m = ia * dx * dx + 2 * ib * dx * dy + id_ * dy * dy
        k = np.exp(-0.5 * m)
        k[m > 9.0] = 0.0
        al = np.clip(alpha[i] * k, 0, 1)[..., None].astype(np.float32)
        patch = img[y0:y1, x0:x1]
        patch *= 1 - al
        patch += al * rgb[i]
    return img
