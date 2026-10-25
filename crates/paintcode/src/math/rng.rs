//! Counter-based random numbers: every draw is a hash of where it is used.
//!
//! A draw is a pure function of a key such as (seed, stream, stroke, dab, slot), so editing one
//! stroke cannot shift the jitter of another, and draws can happen in any order or in parallel.
//! The mixer is the SplitMix64 finaliser; a key is folded in one part at a time,
//! `h = mix(h ^ part)` from `h = 0`, with every part taken modulo 2^64.

pub const JITTER: i64 = 1;
pub const GRAIN: i64 = 2;

pub const SIZE: i64 = 0;
pub const FLOW: i64 = 1;
pub const ANGLE: i64 = 2;
pub const POS_X: i64 = 3;
pub const POS_Y: i64 = 4;

#[inline]
pub fn mix(z: u64) -> u64 {
    let z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline]
pub fn key(parts: &[i64]) -> u64 {
    parts.iter().fold(0u64, |h, &p| mix(h ^ p as u64))
}

/// A float64 in [0, 1) from the top 53 bits of the key.
#[inline]
pub fn uniform(parts: &[i64]) -> f64 {
    (key(parts) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// Value `i` of a noise grid: a float32 in [0, 1) from the top 24 bits of `mix(prefix ^ i)`.
#[inline]
pub fn grid_value(prefix: u64, i: u64) -> f32 {
    (mix(prefix ^ i) >> 40) as f32 * (1.0 / (1u32 << 24) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These numbers are part of the format: a scene renders the same in every version only
    /// while they hold (the numpy engine of 0.4 pinned the same values).
    #[test]
    fn spec_is_pinned() {
        assert_eq!(key(&[24, 1, 0, 0, 0]), 0x45C2_76ED_BC63_8DF7);
        assert_eq!(key(&[-1, 2, 3]), 0x4814_1E37_02D4_765A);
        assert_eq!(uniform(&[24, 1, 0, 0, 0]), 0.27249854377840965);
        let prefix = key(&[7, GRAIN, 3000, 0]);
        assert_eq!(prefix, 0x7311_5726_4855_CD28);
        let grid: Vec<f32> = (0..4).map(|i| grid_value(prefix, i)).collect();
        assert_eq!(
            grid,
            [
                0.47280991077423096,
                0.3959234356880188,
                0.8277716636657715,
                0.01953345537185669
            ]
        );
    }
}
