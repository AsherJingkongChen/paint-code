"""Float RGB canvas that receives anisotropic Gaussian dabs."""

from __future__ import annotations

import math

import numpy as np
from PIL import Image

from . import rng


def parse_color(c) -> np.ndarray:
    """[r, g, b] in 0..255, or "#rrggbb", to float32 0..1."""
    if isinstance(c, str):
        s = c.lstrip("#")
        if len(s) == 3:
            s = "".join(ch * 2 for ch in s)
        if len(s) != 6:
            raise ValueError(f"bad colour {c!r}")
        c = [int(s[i:i + 2], 16) for i in (0, 2, 4)]
    a = np.asarray(c, dtype=np.float32)
    if a.shape != (3,):
        raise ValueError(f"colour must have 3 channels, got {c!r}")
    return np.clip(a / 255.0, 0.0, 1.0)


def hardness_exponent(h: float) -> float:
    """Profile exponent for a hardness in 0..1: 1 at h = 0 (Gaussian), growing without bound
    toward h = 1. Capped at 50, which is already a hard edge at any practical size."""
    h = min(max(float(h), 0.0), 0.98)
    return 1.0 / (1.0 - h)


class Canvas:
    def __init__(self, width: int, height: int, background=(255, 255, 255), seed: int = 0,
                 record_dabs: bool = False):
        self.width, self.height = int(width), int(height)
        self.seed = int(seed)
        self.img = np.empty((self.height, self.width, 3), dtype=np.float32)
        self.img[:] = parse_color(background)
        self.wet = np.zeros((self.height, self.width), dtype=np.float32)   # how much paint is still wet
        self.strokes = 0   # strokes painted so far; a stroke's index keys its jitter
        self.dabs: list[dict] | None = [] if record_dabs else None
        self._grain: dict[float, np.ndarray] = {}

    @classmethod
    def from_image(cls, path, seed: int = 0, record_dabs: bool = False) -> "Canvas":
        im = Image.open(path).convert("RGB")
        c = cls(im.width, im.height, seed=seed, record_dabs=record_dabs)
        c.img[:] = np.asarray(im, dtype=np.float32) / 255.0   # a base image counts as dry paint
        return c

    def sample(self, x: float, y: float, r: float) -> np.ndarray:
        """Gaussian-weighted colour around (x, y); used to load a smudge brush."""
        e = max(1, int(math.ceil(1.5 * r)))
        x0, x1 = max(0, int(x) - e), min(self.width, int(x) + e + 1)
        y0, y1 = max(0, int(y) - e), min(self.height, int(y) + e + 1)
        if x0 >= x1 or y0 >= y1:
            return np.zeros(3, np.float32)
        dx = (np.arange(x0, x1) + 0.5 - x)[None, :]
        dy = (np.arange(y0, y1) + 0.5 - y)[:, None]
        k = np.exp(-0.5 * (dx * dx + dy * dy) / max(0.25, (r / 2) ** 2))
        return (self.img[y0:y1, x0:x1] * k[..., None]).sum((0, 1)) / k.sum()

    def downsample(self, n: int) -> None:
        """Box-filter the canvas down by an integer factor (exact area averaging).

        Recorded dabs are rescaled with it, so they stay in output pixel units."""
        h, w = self.height // n, self.width // n
        self.img = self.img[: h * n, : w * n].reshape(h, n, w, n, 3).mean(axis=(1, 3))
        self.wet = self.wet[: h * n, : w * n].reshape(h, n, w, n).mean(axis=(1, 3))
        self.width, self.height = w, h
        self._grain = {}
        if self.dabs:
            for d in self.dabs:
                for k in ("x", "y", "sx", "sy"):
                    d[k] = d[k] / n

    def to_image(self) -> Image.Image:
        return Image.fromarray((np.clip(self.img, 0, 1) * 255.0 + 0.5).astype(np.uint8), "RGB")

    def save(self, path) -> None:
        self.to_image().save(path)

    # ------------------------------------------------------------------ paper
    def grain_field(self, scale: float) -> np.ndarray:
        """Paper tooth in 0..1, fixed to the canvas so every stroke meets the same texture."""
        key = round(float(scale), 3)
        if key not in self._grain:
            acc = np.zeros((self.height, self.width), dtype=np.float32)
            amp_total = 0.0
            for i, (octave, amp) in enumerate(((1.0, 0.65), (0.5, 0.35))):
                s = max(1.0, key * octave)
                gw, gh = int(math.ceil(self.width / s)) + 2, int(math.ceil(self.height / s)) + 2
                noise = rng.uniform_grid(rng.key(self.seed, rng.GRAIN, int(key * 1000), i), gw * gh)
                g = Image.fromarray(noise.reshape(gh, gw), "F")
                up = g.resize((int(gw * s), int(gh * s)), Image.BICUBIC)
                acc += amp * np.asarray(up, dtype=np.float32)[: self.height, : self.width]
                amp_total += amp
            acc /= amp_total
            lo, hi = np.percentile(acc, (2, 98))
            self._grain[key] = np.clip((acc - lo) / max(hi - lo, 1e-6), 0, 1).astype(np.float32)
        return self._grain[key]

    # ------------------------------------------------------------------- dabs
    def stamp(self, x: float, y: float, sx: float, sy: float, theta: float, color: np.ndarray,
              alpha: float, hardness: float = 0.0, grain: float = 0.0, grain_scale: float = 3.0,
              sample_src=None):
        """Composite one dab; returns the kernel-weighted colour that was under it (or None).

        Returns (colour, wetness): the colour of the wet paint under the dab and how wet that area
        is on average (0..1), or None. sample_src, when given, is (img, wet, ox, oy) covering the
        dab; the pickup is read from it instead of the live canvas.

        The dab is exp(-0.5 * q ** n), n = 1 / (1 - h), with q the squared Mahalanobis distance for
        standard deviations sx (major) and sy (minor) rotated by theta. h = 0 is a 2D Gaussian;
        as h approaches 1 the dab becomes a hard-edged ellipse of radius one sigma.
        """
        # Plain Python floats, so numpy keeps the pixel arithmetic in float32 (a numpy float64
        # scalar here would promote every array below to float64).
        x, y, sx, sy, theta, alpha = float(x), float(y), float(sx), float(sy), float(theta), float(alpha)
        # below half a pixel a Gaussian falls between pixel centres and flickers; 3DGS adds a
        # similar low-pass floor. The clamped sigmas are what gets recorded and exported.
        sx, sy = max(sx, 0.5), max(sy, 0.5)
        ext = 3.0 * max(sx, sy)
        x0, x1 = max(0, int(math.floor(x - ext))), min(self.width, int(math.ceil(x + ext)) + 1)
        y0, y1 = max(0, int(math.floor(y - ext))), min(self.height, int(math.ceil(y + ext)) + 1)
        if x0 >= x1 or y0 >= y1 or alpha <= 0:
            return None

        dx = (np.arange(x0, x1, dtype=np.float32) + 0.5 - x)[None, :]
        dy = (np.arange(y0, y1, dtype=np.float32) + 0.5 - y)[:, None]
        c, s = math.cos(theta), math.sin(theta)
        u = (dx * c + dy * s) / sx
        v = (-dx * s + dy * c) / sy
        q = u * u + v * v
        k = np.exp(-0.5 * q ** hardness_exponent(hardness))
        k[q > 9.0] = 0.0

        if grain > 0:
            g = self.grain_field(grain_scale)[y0:y1, x0:x1]
            k = k * ((1.0 - grain) + grain * g)

        wsum = float(k.sum())
        if wsum <= 1e-8:
            return None
        patch = self.img[y0:y1, x0:x1]
        wpatch = self.wet[y0:y1, x0:x1]
        if sample_src is None:
            src, swet = patch, wpatch
        else:   # a snapshot, so the brush picks up older paint and not its own trail
            arr, warr, ox, oy = sample_src
            src = arr[y0 - oy:y1 - oy, x0 - ox:x1 - ox]
            swet = warr[y0 - oy:y1 - oy, x0 - ox:x1 - ox]
        kw = k * swet
        kwsum = float(kw.sum())
        wetness = kwsum / wsum
        under = (src * kw[..., None]).sum(axis=(0, 1)) / kwsum if kwsum > 1e-8 else None
        a = np.clip(alpha * k, 0.0, 1.0)
        patch *= 1.0 - a[..., None]
        patch += a[..., None] * color
        wpatch += a * (1.0 - wpatch)

        if self.dabs is not None:
            self.dabs.append({
                "x": round(x, 3), "y": round(y, 3), "sx": round(sx, 3), "sy": round(sy, 3),
                "theta": round(theta, 4), "rgb": [round(float(v), 4) for v in color],
                "alpha": round(float(alpha), 4), "hardness": hardness,
            })
        return (under, wetness) if under is not None else None
