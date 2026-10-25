//! Paper tooth: value noise smoothed by Pillow's bicubic resize, then stretched to 0..1.
//!
//! The resize is a port of Pillow's `ImagingResample` for 32-bit float images (separable
//! convolution, horizontal pass first, float64 accumulation, float32 between passes), so the
//! paper is the one the numpy engine of 0.4 made with Pillow, to float rounding.

use crate::math::rng;
use rayon::prelude::*;

const SUPPORT: f64 = 2.0;

fn bicubic(x: f64) -> f64 {
    const A: f64 = -0.5;
    let x = x.abs();
    if x < 1.0 {
        ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0
    } else if x < 2.0 {
        (((x - 5.0) * x + 8.0) * x - 4.0) * A
    } else {
        0.0
    }
}

/// Pillow's `precompute_coeffs` for a box spanning the whole input, for the first `count` of the
/// `out_size` outputs (each output's weights depend only on its own index and the two sizes).
fn coeffs(in_size: usize, out_size: usize, count: usize) -> (usize, Vec<(usize, usize)>, Vec<f64>) {
    let scale = in_size as f64 / out_size as f64;
    let filterscale = scale.max(1.0);
    let support = SUPPORT * filterscale;
    let ksize = support.ceil() as usize * 2 + 1;
    let count = count.min(out_size);
    let mut bounds = Vec::with_capacity(count);
    let mut kk = vec![0f64; count * ksize];
    for xx in 0..count {
        let center = (xx as f64 + 0.5) * scale;
        let ss = 1.0 / filterscale;
        let xmin = ((center - support + 0.5) as i64).max(0) as usize; // C cast truncates toward 0
        let xmax = ((center + support + 0.5) as i64).min(in_size as i64) as usize - xmin;
        let k = &mut kk[xx * ksize..(xx + 1) * ksize];
        let mut ww = 0.0;
        for x in 0..xmax {
            let w = bicubic((x as f64 + xmin as f64 - center + 0.5) * ss);
            k[x] = w;
            ww += w;
        }
        if ww != 0.0 {
            for v in k.iter_mut().take(xmax) {
                *v /= ww;
            }
        }
        bounds.push((xmin, xmax));
    }
    (ksize, bounds, kk)
}

/// Bicubic resize of a float32 image, as `Image.fromarray(a, "F").resize((dw, dh), BICUBIC)`.
/// The engine only needs corners of it ([`resize_bicubic_crop`]); this is the whole, for tests.
#[cfg(test)]
pub fn resize_bicubic(src: &[f32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<f32> {
    resize_bicubic_crop(src, sw, sh, dw, dh, dw, dh)
}

/// The top-left `cw` x `ch` corner of [`resize_bicubic`] to `dw` x `dh`, bit for bit, without
/// computing the rest: the work and the memory follow the corner, not the full size.
pub fn resize_bicubic_crop(
    src: &[f32],
    sw: usize,
    sh: usize,
    dw: usize,
    dh: usize,
    cw: usize,
    ch: usize,
) -> Vec<f32> {
    let (kh, bh, kkh) = coeffs(sw, dw, cw);
    let (kv, bv, kkv) = coeffs(sh, dh, ch);
    let (dw, dh) = (bh.len(), bv.len());
    if dw == 0 || dh == 0 {
        return Vec::new();
    }
    let first = bv[0].0;
    let last = bv.iter().map(|&(lo, n)| lo + n).max().unwrap_or(first);
    // horizontal pass over the rows the vertical pass will read
    let rows = last - first;
    // every output pixel is its own sum, so rows can go to different threads
    let mut tmp = vec![0f32; dw * rows];
    tmp.par_chunks_mut(dw)
        .enumerate()
        .for_each(|(yy, out_row)| {
            let row = &src[(yy + first) * sw..(yy + first + 1) * sw];
            for (xx, o) in out_row.iter_mut().enumerate() {
                let (xmin, xmax) = bh[xx];
                let k = &kkh[xx * kh..];
                let mut ss = 0f64;
                for x in 0..xmax {
                    ss += row[x + xmin] as f64 * k[x];
                }
                *o = ss as f32;
            }
        });
    let mut out = vec![0f32; dw * dh];
    out.par_chunks_mut(dw)
        .enumerate()
        .for_each(|(yy, out_row)| {
            let (ymin, ymax) = bv[yy];
            let ymin = ymin - first;
            let k = &kkv[yy * kv..];
            for (xx, o) in out_row.iter_mut().enumerate() {
                let mut ss = 0f64;
                for y in 0..ymax {
                    ss += tmp[(y + ymin) * dw + xx] as f64 * k[y];
                }
                *o = ss as f32;
            }
        });
    out
}

/// numpy's default ("linear") percentile of a float32 array, returned as float64.
fn percentile(sorted_pick: &mut [f32], q: f64) -> f64 {
    let n = sorted_pick.len();
    let idx = q / 100.0 * (n - 1) as f64;
    let lo = idx.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    let gamma = idx - lo as f64;
    let (_, a, rest) = sorted_pick.select_nth_unstable_by(lo, |x, y| x.total_cmp(y));
    let a = *a;
    let b = if hi == lo {
        a
    } else {
        *rest.iter().min_by(|x, y| x.total_cmp(y)).unwrap()
    };
    let diff = (b - a) as f64; // subtract(b, a) happens in float32
    if gamma >= 0.5 {
        b as f64 - diff * (1.0 - gamma)
    } else {
        a as f64 + diff * gamma
    }
}

/// `round(x, 3)` closely enough for brush scales.
pub fn round3(x: f64) -> f64 {
    (x * 1000.0).round_ties_even() / 1000.0
}

/// The paper field for one grain scale: `height * width` values in 0..1 (normalised in float64,
/// stored as float32).
pub fn field(width: usize, height: usize, seed: i64, scale: f64) -> Vec<f32> {
    let key = round3(scale);
    let mut acc = vec![0f32; width * height];
    for (i, (octave, amp)) in [(1.0f64, 0.65f64), (0.5, 0.35)].into_iter().enumerate() {
        let s = (key * octave).max(1.0);
        let gw = (width as f64 / s).ceil() as usize + 2;
        let gh = (height as f64 / s).ceil() as usize + 2;
        let prefix = rng::key(&[seed, rng::GRAIN, (key * 1000.0) as i64, i as i64]);
        let noise: Vec<f32> = (0..(gw * gh) as u64)
            .map(|j| rng::grid_value(prefix, j))
            .collect();
        let (uw, uh) = ((gw as f64 * s) as usize, (gh as f64 * s) as usize);
        // the canvas only ever reads the top-left corner of the upsampled noise, so only that
        // corner is computed: a grain_scale far larger than the canvas costs no more memory
        let (cw, ch) = (width.min(uw), height.min(uh));
        let up = resize_bicubic_crop(&noise, gw, gh, uw, uh, cw, ch);
        let amp32 = amp as f32;
        for y in 0..ch {
            for x in 0..cw {
                acc[y * width + x] += amp32 * up[y * cw + x];
            }
        }
    }
    // acc /= 0.65 + 0.35 is a division by exactly 1.0
    let mut tmp = acc.clone();
    let lo = percentile(&mut tmp, 2.0);
    let hi = percentile(&mut tmp, 98.0);
    let span = (hi - lo).max(1e-6);
    acc.iter()
        .map(|&v| ((v as f64 - lo) / span).clamp(0.0, 1.0) as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corner_of_the_resize_is_the_resize() {
        for &(sw, sh, dw, dh, cw, ch) in &[
            (5, 4, 37, 29, 20, 11),
            (9, 9, 30, 30, 30, 30),
            (3, 7, 50, 8, 1, 8),
        ] {
            let src: Vec<f32> = (0..sw * sh)
                .map(|i| ((i * 7919) % 101) as f32 / 101.0)
                .collect();
            let full = resize_bicubic(&src, sw, sh, dw, dh);
            let crop = resize_bicubic_crop(&src, sw, sh, dw, dh, cw, ch);
            for y in 0..ch {
                for x in 0..cw {
                    assert_eq!(
                        crop[y * cw + x].to_bits(),
                        full[y * dw + x].to_bits(),
                        "{x} {y}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_huge_grain_scale_needs_no_huge_paper() {
        let f = field(40, 30, 1, 1e7);
        assert_eq!(f.len(), 40 * 30);
        assert!(f.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
