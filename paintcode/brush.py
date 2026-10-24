"""Brush model: how one control point (x, y, pressure, tilt) becomes one dab."""

from __future__ import annotations

from dataclasses import dataclass, fields, replace
import math


@dataclass(frozen=True)
class Brush:
    # --- footprint ---
    size: float = 8.0            # radius in px at full pressure (the dab is a Gaussian with sigma = size / 2)
    aspect: float = 1.0          # minor / major axis ratio; 1 is round, 0.3 is a flat or filbert
    angle: float | None = None   # None: major axis turns to lie across the stroke; a number: fixed nib angle in degrees
    hardness: float = 0.0        # 0 is a pure Gaussian; toward 1 the top flattens and the edge sharpens

    # --- deposition ---
    flow: float = 0.6            # opacity of one dab at full pressure
    spacing: float = 0.15        # dab distance as a fraction of the footprint; a quality knob, flow is normalised to it

    # --- pressure response ---
    min_size: float = 0.25       # radius fraction left at zero pressure
    size_gamma: float = 1.0      # radius = size * lerp(min_size, 1, p ** size_gamma)
    min_flow: float = 0.15
    flow_gamma: float = 1.0
    tilt_stretch: float = 0.8    # tilt 1 stretches the major axis by (1 + tilt_stretch) and thins the paint

    # --- the hand is not a machine ---
    size_jitter: float = 0.0     # relative, uniform +-
    angle_jitter: float = 0.0    # degrees, uniform +-
    pos_jitter: float = 0.0      # fraction of radius, uniform +-
    flow_jitter: float = 0.0     # relative, uniform +-

    # --- wet media ---
    load: float = 1.0            # paint on the brush when the stroke starts
    half_life: float | None = None   # px of travel until the load halves; None never runs dry
    min_load: float = 0.15
    pickup: float = 0.0          # fraction of the canvas colour mixed into the carried paint per radius of travel

    # --- surface ---
    grain: float = 0.0           # 0..1 how much paper texture modulates each dab
    grain_scale: float = 3.0     # px size of the paper tooth

    def with_(self, **kw) -> "Brush":
        known = {f.name for f in fields(self)}
        unknown = set(kw) - known
        if unknown:
            raise ValueError(f"unknown brush field(s): {sorted(unknown)}")
        return replace(self, **kw)

    def radius(self, p: float) -> float:
        p = min(1.0, max(0.0, p))
        k = self.min_size + (1.0 - self.min_size) * p ** self.size_gamma
        return self.size * k

    def flow_at(self, p: float) -> float:
        p = min(1.0, max(0.0, p))
        return self.flow * (self.min_flow + (1.0 - self.min_flow) * p ** self.flow_gamma)

    def footprint_along(self, r: float) -> float:
        """Half-extent of the dab measured along the direction of travel."""
        if self.angle is None:          # major axis lies across the stroke, so the minor one runs along it
            return r * self.aspect
        return r * max(self.aspect, 0.35)   # fixed nib: direction varies, stay conservative

    def step(self, p: float) -> float:
        return max(0.35, self.spacing * 2.0 * self.footprint_along(self.radius(p)))


PRESETS: dict[str, Brush] = {
    "ink": Brush(size=3.0, hardness=0.6, flow=0.9, min_size=0.15, spacing=0.12),
    "pencil": Brush(size=1.6, hardness=0.7, flow=0.35, min_size=0.5, min_flow=0.05,
                    grain=0.75, grain_scale=1.6, spacing=0.2),
    "nib": Brush(size=7.0, aspect=0.18, angle=40.0, hardness=0.8, flow=0.95, min_size=0.5, spacing=0.1),
    "round": Brush(size=8.0, hardness=0.3, flow=0.7),
    "filbert": Brush(size=9.0, aspect=0.45, hardness=0.35, flow=0.8, pickup=0.03,
                     half_life=320.0, grain=0.25, size_jitter=0.06, angle_jitter=3.0),
    "flat": Brush(size=12.0, aspect=0.22, hardness=0.6, flow=0.85, pickup=0.05,
                  half_life=260.0, grain=0.35, size_jitter=0.04),
    "airbrush": Brush(size=30.0, hardness=0.0, flow=0.06, min_size=0.6, spacing=0.08),
    "dry": Brush(size=10.0, aspect=0.6, hardness=0.5, flow=0.8, half_life=90.0, min_load=0.05,
                 grain=0.8, grain_scale=2.5, pickup=0.02),
    "blender": Brush(size=10.0, hardness=0.0, flow=0.35, pickup=0.6),
}


def get(name_or_spec) -> Brush:
    """Brush from a preset name, a dict ({"preset": ..., **overrides}) or a Brush."""
    if isinstance(name_or_spec, Brush):
        return name_or_spec
    if isinstance(name_or_spec, str):
        if name_or_spec not in PRESETS:
            raise ValueError(f"unknown preset {name_or_spec!r}; have {sorted(PRESETS)}")
        return PRESETS[name_or_spec]
    spec = dict(name_or_spec)
    base = get(spec.pop("preset")) if "preset" in spec else Brush()
    return base.with_(**spec)
