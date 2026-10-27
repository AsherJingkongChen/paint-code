//! The dab profile `exp(-0.5 q^n)` and the geometry of an anisotropic 2D Gaussian.

/// Profile exponent for a hardness in 0..1: `1 / (1 - h)`, with h clamped to [0, 0.98].
#[inline]
pub fn hardness_exponent(h: f64) -> f64 {
    1.0 / (1.0 - h.clamp(0.0, 0.98))
}

/// sigma / radius: the profile reaches e^-2 at `sigma * 2^(1/n)`, so this puts that edge at the
/// radius for every hardness.
#[inline]
pub fn sigma_scale(hardness: f64) -> f64 {
    2f64.powf(-1.0 / hardness_exponent(hardness))
}

/// Dabs are cut off where the squared Mahalanobis distance exceeds this.
pub const CUTOFF_Q: f32 = 9.0;

/// Lanes: values of `q` evaluated at once.
pub const L: usize = 8;
pub type F = std::simd::Simd<f32, L>;

/// The profile `exp(-0.5 q^n)` for one exponent, eight values of `q` at a time, through
/// polynomial `log2` and `exp2`: no table to gather from and no `powf`, so every lane runs in
/// the vector unit. Every lane is computed on its own, so a value does not depend on its lane.
#[derive(Debug, Clone, Copy)]
pub struct Profile {
    n: f32,
}

impl Profile {
    pub fn new(n: f32) -> Profile {
        Profile { n }
    }

    /// The profile at eight values of `q >= 0`.
    #[inline]
    pub fn lanes(&self, q: F) -> F {
        // exp(-0.5 q^n) = 2^(-0.5 log2(e) q^n), and q^n = 2^(n log2 q)
        let qn = if self.n == 1.0 {
            q
        } else {
            exp2(F::splat(self.n) * log2(q))
        };
        exp2(F::splat(-0.5 * std::f32::consts::LOG2_E) * qn)
    }

    /// The profile at one value of `q >= 0`: the same bits as a lane of [`Profile::lanes`].
    pub fn at(&self, q: f32) -> f32 {
        self.lanes(F::splat(q))[0]
    }
}

/// `2^x`, to a few float32 rounding steps. Below `-126.5` it is 0, and it never overflows: the
/// argument is clamped to 127.
#[inline]
fn exp2(x: F) -> F {
    use std::simd::prelude::*;
    let x = x.simd_clamp(F::splat(-127.0), F::splat(127.0));
    // round to the nearest integer by adding and taking away 1.5 * 2^23, where a float32 has
    // no fraction bits left: two adds in any vector unit, where round() may not be one
    const MAGIC: f32 = 12_582_912.0;
    let i = (x + F::splat(MAGIC)) - F::splat(MAGIC);
    // 2^f = e^(f ln 2) for |f| <= 1/2, its Taylor series to the 7th power
    let t = (x - i) * F::splat(std::f32::consts::LN_2);
    let mut p = F::splat(1.0 / 5040.0);
    for c in [
        1.0 / 720.0,
        1.0 / 120.0,
        1.0 / 24.0,
        1.0 / 6.0,
        0.5,
        1.0,
        1.0,
    ] {
        p = p * t + F::splat(c);
    }
    // 2^i from its exponent bits; i = -127 gives the bits of 0.0
    let bits = ((i.cast::<i32>() + Simd::splat(127)) << Simd::splat(23)).cast::<u32>();
    p * F::from_bits(bits)
}

/// `log2 x` for `x >= 0`, to a few float32 rounding steps; 0 is taken as the smallest normal
/// number, so `log2 0` is -126.
#[inline]
fn log2(x: F) -> F {
    use std::simd::prelude::*;
    let bits = x.simd_max(F::splat(f32::MIN_POSITIVE)).to_bits();
    let mut e = ((bits >> Simd::splat(23)).cast::<i32>() - Simd::splat(127)).cast::<f32>();
    let mut m = F::from_bits((bits & Simd::splat(0x007f_ffff)) | Simd::splat(0x3f80_0000));
    // m in [1, 2), moved to [sqrt 1/2, sqrt 2) so the series below is short
    let high = m.simd_gt(F::splat(std::f32::consts::SQRT_2));
    m = high.select(m * F::splat(0.5), m);
    e = high.select(e + F::splat(1.0), e);
    // log2 m = 2 / ln 2 * atanh t, t = (m - 1) / (m + 1), |t| < 0.172, to the 9th power
    let t = (m - F::splat(1.0)) / (m + F::splat(1.0));
    let t2 = t * t;
    let mut p = F::splat(1.0 / 9.0);
    for c in [1.0 / 7.0, 1.0 / 5.0, 1.0 / 3.0, 1.0] {
        p = p * t2 + F::splat(c);
    }
    e + F::splat(2.0 / std::f32::consts::LN_2) * t * p
}

/// Coefficients of `q(dx, dy) = a dx^2 + b dx dy + c dy^2` for standard deviations `sx` (major)
/// and `sy` (minor) rotated by `theta`.
#[inline]
pub fn quadratic(sx: f64, sy: f64, theta: f64) -> (f64, f64, f64) {
    let (s, c) = theta.sin_cos();
    let (ix, iy) = (1.0 / (sx * sx), 1.0 / (sy * sy));
    (
        c * c * ix + s * s * iy,
        2.0 * c * s * (ix - iy),
        s * s * ix + c * c * iy,
    )
}

/// Half extents (along x and along y) of the ellipse `q <= q_max`.
#[inline]
pub fn half_extents(sx: f64, sy: f64, theta: f64, q_max: f64) -> (f64, f64) {
    let (s, c) = theta.sin_cos();
    let r = q_max.sqrt();
    (r * (sx * c).hypot(sy * s), r * (sx * s).hypot(sy * c))
}

/// The dx interval where `q <= q_max` on the row at offset `dy`, or None if the row misses.
#[inline]
pub fn row_interval(quad: (f64, f64, f64), dy: f64, q_max: f64) -> Option<(f64, f64)> {
    let (a, b, c) = quad;
    let (bb, cc) = (b * dy, c * dy * dy - q_max);
    let disc = bb * bb - 4.0 * a * cc;
    if disc < 0.0 {
        return None;
    }
    let r = disc.sqrt();
    Some(((-bb - r) / (2.0 * a), (-bb + r) / (2.0 * a)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_is_the_formula() {
        let mut worst = vec![];
        for h in [0.0, 0.2, 0.4, 0.6, 0.8, 0.85, 0.9, 0.95, 0.98] {
            let n = hardness_exponent(h) as f32;
            let p = Profile::new(n);
            let mut gap = 0f64;
            for i in 0..=90_000 {
                let q = i as f32 * 1e-4;
                let want = (-0.5 * (q as f64).powf(n as f64)).exp();
                gap = gap.max((p.at(q) as f64 - want).abs());
            }
            worst.push((h, gap));
        }
        // 6e-8 for a Gaussian to 1.0e-7 for the hardest: float32 rounding, the table was 1.1e-4
        for (h, gap) in &worst {
            assert!(*gap < 2e-7, "{worst:?} at hardness {h}");
        }
    }

    #[test]
    fn a_value_does_not_depend_on_its_lane() {
        let p = Profile::new(hardness_exponent(0.8) as f32);
        let qs = F::from_array([0.0, 0.3, 1.0, 1.7, 4.0, 8.2, 9.0, 12.0]);
        let lanes = p.lanes(qs);
        for (q, v) in qs.to_array().iter().zip(lanes.to_array()) {
            assert_eq!(p.at(*q).to_bits(), v.to_bits());
        }
    }

    #[test]
    fn interval_matches_brute_force() {
        for &(sx, sy, th) in &[
            (6.0, 2.0, 0.3),
            (3.0, 3.0, 0.0),
            (9.0, 0.7, 2.2),
            (1.0, 4.0, -1.0),
        ] {
            let quad = quadratic(sx, sy, th);
            let (ex, ey) = half_extents(sx, sy, th, 9.0);
            let mut dy = -ey;
            while dy <= ey {
                let (lo, hi) = row_interval(quad, dy, 9.0).unwrap_or((1.0, -1.0));
                let mut dx = -ex - 1.0;
                while dx <= ex + 1.0 {
                    let (s, c) = th.sin_cos();
                    let (u, v) = ((dx * c + dy * s) / sx, (-dx * s + dy * c) / sy);
                    let q = u * u + v * v;
                    if q < 9.0 - 1e-9 {
                        assert!(
                            dx >= lo - 1e-9 && dx <= hi + 1e-9,
                            "{sx} {sy} {th} {dx} {dy}"
                        );
                    }
                    if q > 9.0 + 1e-9 {
                        assert!(dx < lo || dx > hi);
                    }
                    dx += 0.173;
                }
                dy += 0.131;
            }
        }
    }
}
