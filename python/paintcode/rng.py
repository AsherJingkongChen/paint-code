"""Counter-based random numbers: every draw is a hash of where it is used.

A draw is a pure function of a key such as (seed, stream, stroke, dab, slot). It never depends
on how many draws came before it, so editing one stroke cannot shift the jitter of any other
stroke, and draws can be made in any order or in parallel. The Rust core uses the same integer
arithmetic, so both engines get bit-identical random numbers.

The mixer is the SplitMix64 finaliser. A key is folded in one part at a time:
h = mix(h ^ part), starting from h = 0, with every part taken modulo 2**64.
"""

from __future__ import annotations

import numpy as np

MASK = (1 << 64) - 1

# streams
JITTER = 1
GRAIN = 2

# jitter slots, one per random number a dab can draw
SIZE, FLOW, ANGLE, POS_X, POS_Y = range(5)


def mix(z: int) -> int:
    z = (z + 0x9E3779B97F4A7C15) & MASK
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
    return z ^ (z >> 31)


def key(*parts: int) -> int:
    h = 0
    for p in parts:
        h = mix(h ^ (int(p) & MASK))
    return h


def uniform(*parts: int) -> float:
    """A float64 in [0, 1) from the top 53 bits of the key."""
    return (key(*parts) >> 11) * (1.0 / (1 << 53))


def uniform_grid(prefix: int, n: int) -> np.ndarray:
    """n float32 values in [0, 1): value i is the top 24 bits of mix(prefix ^ i)."""
    z = np.uint64(prefix) ^ np.arange(n, dtype=np.uint64)
    z = z + np.uint64(0x9E3779B97F4A7C15)
    z = (z ^ (z >> np.uint64(30))) * np.uint64(0xBF58476D1CE4E5B9)
    z = (z ^ (z >> np.uint64(27))) * np.uint64(0x94D049BB133111EB)
    z = z ^ (z >> np.uint64(31))
    return (z >> np.uint64(40)).astype(np.float32) * np.float32(1.0 / (1 << 24))
