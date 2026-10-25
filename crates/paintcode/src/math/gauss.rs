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
