//! Paper tooth: value noise smoothed by Pillow's bicubic resize, then stretched to 0..1.
//!
//! The resize is a port of Pillow's `ImagingResample` for 32-bit float images (separable
//! convolution, horizontal pass first, float64 accumulation, float32 between passes), so the
//! paper matches `Canvas.grain_field` in `python/paintcode/canvas.py` to float rounding.

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

/// Pillow's `precompute_coeffs` for a box spanning the whole input.
fn coeffs(in_size: usize, out_size: usize) -> (usize, Vec<(usize, usize)>, Vec<f64>) {
    let scale = in_size as f64 / out_size as f64;
    let filterscale = scale.max(1.0);
    let support = SUPPORT * filterscale;
    let ksize = support.ceil() as usize * 2 + 1;
    let mut bounds = Vec::with_capacity(out_size);
    let mut kk = vec![0f64; out_size * ksize];
    for xx in 0..out_size {
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
pub fn resize_bicubic(src: &[f32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<f32> {
    let (kh, bh, kkh) = coeffs(sw, dw);
    let (kv, bv, kkv) = coeffs(sh, dh);
    let first = bv[0].0;
    let last = bv[dh - 1].0 + bv[dh - 1].1;
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
        let up = resize_bicubic(&noise, gw, gh, uw, uh);
        let amp32 = amp as f32;
        for y in 0..height {
            for x in 0..width {
                acc[y * width + x] += amp32 * up[y * uw + x];
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
