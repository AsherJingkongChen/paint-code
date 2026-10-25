use std::collections::HashMap;

use crate::math::gauss::{self, CUTOFF_Q};
use rayon::prelude::*;

use super::{grain, RgbImage};

/// One anisotropic 2D Gaussian dab.
///
/// The profile is `flow * exp(-0.5 q^n)` with `n = 1 / (1 - hardness)` and `q` the squared
/// Mahalanobis distance for standard deviations `sx` (major) and `sy` (minor) rotated by `theta`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dab {
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub theta: f64,
    pub alpha: f64,
    pub hardness: f64,
    pub grain: f64,
    pub grain_scale: f64,
}

/// A dab as it was laid on the canvas: its geometry (sigmas already raised to half a pixel) and
/// the colour the brush held. This is what a PLY export writes, one splat per laid dab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Laid {
    pub dab: Dab,
    pub rgb: [f32; 3],
}

/// The largest canvas the renderer allocates, in pixels, counted after scale and supersample.
/// 2^28 pixels is a 16384 x 16384 canvas, 4 GiB of float RGB and wetness.
pub const MAX_PIXELS: usize = 1 << 28;

/// A zeroed buffer, or a message instead of an abort when the memory is not there.
fn buffer(len: usize, what: &str) -> Result<Vec<f32>, String> {
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| format!("not enough memory for the {what} ({len} floats)"))?;
    v.resize(len, 0f32);
    Ok(v)
}

/// Which pixels of a dab's bounding square get evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spans {
    /// The whole 3-sigma square with `powf` on every pixel, exactly as the Python engine does.
    /// Kept as the reference that the fast path is tested against.
    Square,
    /// Only the rows and columns the q <= 9 ellipse can touch, and no `powf` when the exponent
    /// is 1 (a pure Gaussian). Outside the ellipse the kernel is exactly zero and a zero dab
    /// leaves a pixel bit-for-bit unchanged; `powf(q, 1) == q` is checked by the tests. So the
    /// result is the same, bit for bit.
    Ellipse,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    y: usize,
    x0: usize,
    len: usize,
    off: usize,
}

/// A dab evaluated on the pixel grid: its kernel values, row by row.
#[derive(Debug, Clone, Default)]
pub struct Footprint {
    spans: Vec<Span>,
    k: Vec<f32>,
    alpha: f32,
    /// wsum <= 1e-8: the Python engine returns before compositing.
    pub skip: bool,
    y_min: usize,
    y_max: usize,
}

impl Footprint {
    /// Every evaluated pixel as (x, y, kernel value), row by row.
    pub fn pixels(&self) -> impl Iterator<Item = (usize, usize, f32)> + '_ {
        self.spans
            .iter()
            .flat_map(move |sp| (0..sp.len).map(move |i| (sp.x0 + i, sp.y, self.k[sp.off + i])))
    }
}

/// The wet paint under a dab: its kernel-weighted colour and how wet that area is (0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pickup {
    pub colour: [f64; 3],
    pub wetness: f64,
}

/// A fixed view of the canvas to read pickup from: the live canvas, or a snapshot of a rectangle.
#[derive(Clone, Copy)]
pub struct Source<'a> {
    img: &'a [f32],
    wet: &'a [f32],
    ox: usize,
    oy: usize,
    w: usize,
}

/// Float RGB canvas that receives dabs, plus a wetness map.
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub seed: i64,
    /// Row-major RGB in 0..1, `height * width * 3`.
    pub img: Vec<f32>,
    /// How much paint is still wet, `height * width`.
    pub wet: Vec<f32>,
    /// Strokes painted so far; a stroke's index keys its jitter.
    pub strokes: i64,
    /// Every dab laid so far, in order, when recording is on (for the PLY export).
    pub laid: Option<Vec<Laid>>,
    grain: HashMap<u64, Vec<f32>>,
}

impl std::fmt::Debug for Canvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Canvas")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("seed", &self.seed)
            .field("strokes", &self.strokes)
            .field("laid", &self.laid.as_ref().map(Vec::len))
            .finish_non_exhaustive()
    }
}

const BAND: usize = 16;

impl Canvas {
    /// A blank canvas. Fails, rather than aborting, when it is larger than [`MAX_PIXELS`] or the
    /// memory cannot be had.
    pub fn new(
        width: usize,
        height: usize,
        background: [f32; 3],
        seed: i64,
    ) -> Result<Canvas, String> {
        let pixels = width
            .checked_mul(height)
            .filter(|&n| n <= MAX_PIXELS)
            .ok_or_else(|| {
                format!(
                    "a {width}x{height} canvas is too large: the limit is {MAX_PIXELS} pixels \
                     (16384 x 16384), counted after scale and supersample"
                )
            })?;
        let mut img = buffer(pixels * 3, "canvas")?;
        for px in img.chunks_exact_mut(3) {
            px.copy_from_slice(&background);
        }
        Ok(Canvas {
            width,
            height,
            seed,
            img,
            wet: buffer(pixels, "wetness map")?,
            strokes: 0,
            laid: None,
            grain: HashMap::new(),
        })
    }

    /// A canvas that starts as an image. The image counts as dry paint: nothing is wet, so no
    /// brush picks it up, but a smudge still loads from it.
    pub fn from_rgb8(image: &RgbImage, seed: i64) -> Result<Canvas, String> {
        let mut c = Canvas::new(image.width, image.height, [1.0; 3], seed)?;
        for (o, &v) in c.img.iter_mut().zip(&image.data) {
            *o = v as f32 / 255.0f32;
        }
        Ok(c)
    }

    pub fn live(&self) -> Source<'_> {
        Source {
            img: &self.img,
            wet: &self.wet,
            ox: 0,
            oy: 0,
            w: self.width,
        }
    }

    /// Copy of a rectangle of the canvas (clamped), owned. Read it with [`Canvas::view`].
    pub fn snapshot(
        &self,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
    ) -> (Vec<f32>, Vec<f32>, [usize; 3]) {
        let (x1, y1) = (x1.min(self.width), y1.min(self.height));
        let (x0, y0) = (x0.min(x1), y0.min(y1));
        let w = x1 - x0;
        let mut img = Vec::with_capacity(w * (y1 - y0) * 3);
        let mut wet = Vec::with_capacity(w * (y1 - y0));
        for y in y0..y1 {
            img.extend_from_slice(&self.img[(y * self.width + x0) * 3..(y * self.width + x1) * 3]);
            wet.extend_from_slice(&self.wet[y * self.width + x0..y * self.width + x1]);
        }
        (img, wet, [x0, y0, w])
    }

    pub fn view<'a>(img: &'a [f32], wet: &'a [f32], origin: [usize; 3]) -> Source<'a> {
        Source {
            img,
            wet,
            ox: origin[0],
            oy: origin[1],
            w: origin[2],
        }
    }

    /// Make sure the paper for this grain scale exists before footprints read it in parallel.
    pub fn prepare_grain(&mut self, scale: f64) {
        let key = grain::round3(scale).to_bits();
        if !self.grain.contains_key(&key) {
            let field = grain::field(self.width, self.height, self.seed, scale);
            self.grain.insert(key, field);
        }
    }

    fn grain_field(&self, scale: f64) -> &[f32] {
        self.grain
            .get(&grain::round3(scale).to_bits())
            .expect("prepare_grain before footprint")
    }

    /// Pixel bounds of the dab's 3-sigma square, clipped to the canvas: (x0, x1, y0, y1).
    pub fn bounds(&self, d: &Dab) -> Option<(usize, usize, usize, usize)> {
        let (sx, sy) = (d.sx.max(0.5), d.sy.max(0.5));
        let ext = 3.0 * sx.max(sy);
        let x0 = ((d.x - ext).floor() as i64).max(0);
        let x1 = ((d.x + ext).ceil() as i64 + 1).min(self.width as i64);
        let y0 = ((d.y - ext).floor() as i64).max(0);
        let y1 = ((d.y + ext).ceil() as i64 + 1).min(self.height as i64);
        if x0 >= x1 || y0 >= y1 || d.alpha <= 0.0 {
            return None;
        }
        Some((x0 as usize, x1 as usize, y0 as usize, y1 as usize))
    }

    /// Evaluate one dab: its kernel, and if `pickup` the wet paint under it read from `src`.
    ///
    /// This is the first half of `Canvas.stamp` in `python/paintcode/canvas.py`, with the same float32
    /// arithmetic per pixel (the geometry above it is float64, as in Python).
    pub fn footprint(
        &self,
        d: &Dab,
        spans: Spans,
        src: Option<Source<'_>>,
    ) -> (Footprint, Option<Pickup>) {
        let mut fp = Footprint {
            skip: true,
            ..Default::default()
        };
        let Some((x0, x1, y0, y1)) = self.bounds(d) else {
            return (fp, None);
        };
        let (sx, sy) = (d.sx.max(0.5), d.sy.max(0.5));
        let (s64, c64) = (d.theta.sin(), d.theta.cos());
        let (c, s) = (c64 as f32, s64 as f32);
        let (sx32, sy32, x32, y32) = (sx as f32, sy as f32, d.x as f32, d.y as f32);
        let n = gauss::hardness_exponent(d.hardness) as f32;
        let gaussian = spans == Spans::Ellipse && n == 1.0;
        let paper = (d.grain > 0.0).then(|| self.grain_field(d.grain_scale));
        // (1.0 - grain) + grain * g, with the Python floats rounded to float32 as numpy does
        let (g_keep, g_amt) = ((1.0 - d.grain) as f32, d.grain as f32);

        // rows and columns to visit
        let quad = gauss::quadratic(sx, sy, d.theta);
        let q_pad = CUTOFF_Q as f64 * 1.001;
        let (ry0, ry1) = match spans {
            Spans::Square => (y0, y1),
            Spans::Ellipse => {
                let (_, ey) = gauss::half_extents(sx, sy, d.theta, q_pad);
                let lo = ((d.y - ey - 0.5).floor() as i64 - 1).max(y0 as i64) as usize;
                let hi = ((d.y + ey - 0.5).ceil() as i64 + 2)
                    .min(y1 as i64)
                    .max(lo as i64) as usize;
                (lo, hi)
            }
        };

        if spans == Spans::Ellipse {
            let (ex, ey) = gauss::half_extents(sx, sy, d.theta, q_pad);
            // the ellipse's area, but never more than the clipped square: a dab far larger than
            // the canvas must not ask for more memory than the canvas has
            let ellipse = std::f64::consts::PI * (ex + 2.0) * (ey + 2.0);
            fp.k.reserve(ellipse.min(((x1 - x0) * (y1 - y0)) as f64) as usize);
        }
        let (mut wsum, mut kwsum) = (0f64, 0f64);
        let mut under = [0f64; 3];
        for py in ry0..ry1 {
            let dy = (py as f32 + 0.5) - y32;
            let (cx0, cx1) = match spans {
                Spans::Square => (x0, x1),
                Spans::Ellipse => match gauss::row_interval(quad, (py as f64 + 0.5) - d.y, q_pad) {
                    None => continue,
                    Some((lo, hi)) => {
                        let a = ((d.x + lo - 0.5).floor() as i64 - 1).max(x0 as i64);
                        let b = ((d.x + hi - 0.5).ceil() as i64 + 2).min(x1 as i64);
                        if a >= b {
                            continue;
                        }
                        (a as usize, b as usize)
                    }
                },
            };
            let off = fp.k.len();
            fp.spans.push(Span {
                y: py,
                x0: cx0,
                len: cx1 - cx0,
                off,
            });
            for px in cx0..cx1 {
                let dx = (px as f32 + 0.5) - x32;
                let u = (dx * c + dy * s) / sx32;
                let v = (-dx * s + dy * c) / sy32;
                let q = u * u + v * v;
                let qn = if gaussian { q } else { q.powf(n) };
                let k32 = if q > CUTOFF_Q {
                    0.0
                } else {
                    (-0.5f32 * qn).exp()
                };
                let k = match paper {
                    Some(g) => k32 * (g_keep + g_amt * g[py * self.width + px]),
                    None => k32,
                };
                fp.k.push(k);
                wsum += k as f64;
                if let Some(src) = src {
                    let i = (py - src.oy) * src.w + (px - src.ox);
                    let kw = k * src.wet[i];
                    kwsum += kw as f64;
                    for ch in 0..3 {
                        under[ch] += (src.img[i * 3 + ch] * kw) as f64;
                    }
                }
            }
        }
        if wsum <= 1e-8 {
            return (fp, None);
        }
        fp.skip = false;
        fp.alpha = d.alpha as f32;
        fp.y_min = fp.spans.first().map_or(0, |s| s.y);
        fp.y_max = fp.spans.last().map_or(0, |s| s.y + 1);
        let pickup = (src.is_some() && kwsum > 1e-8).then(|| Pickup {
            colour: under.map(|v| v / kwsum),
            wetness: kwsum / wsum,
        });
        (fp, pickup)
    }

    /// Lay footprints down in order, "over" compositing, each in its colour. Rows are split into
    /// bands that run in parallel; within a pixel the order and the arithmetic are those of
    /// `Canvas.stamp`, so the result does not depend on the number of threads.
    pub fn composite(&mut self, prints: &[(&Footprint, [f32; 3])]) {
        let w = self.width;
        self.img
            .par_chunks_mut(BAND * w * 3)
            .zip(self.wet.par_chunks_mut(BAND * w))
            .enumerate()
            .for_each(|(band, (img, wet))| {
                let (b0, b1) = (band * BAND, band * BAND + wet.len() / w);
                for &(fp, col) in prints {
                    if fp.skip || fp.y_max <= b0 || fp.y_min >= b1 {
                        continue;
                    }
                    for sp in fp.spans.iter().filter(|sp| sp.y >= b0 && sp.y < b1) {
                        let row = (sp.y - b0) * w + sp.x0;
                        let ks = &fp.k[sp.off..sp.off + sp.len];
                        let wrow = &mut wet[row..row + sp.len];
                        let prow = &mut img[row * 3..(row + sp.len) * 3];
                        for ((&k, wv), p) in
                            ks.iter().zip(wrow.iter_mut()).zip(prow.chunks_exact_mut(3))
                        {
                            let a = (fp.alpha * k).clamp(0.0, 1.0);
                            let keep = 1.0f32 - a;
                            for ch in 0..3 {
                                p[ch] = p[ch] * keep + a * col[ch];
                            }
                            *wv += a * (1.0f32 - *wv);
                        }
                    }
                }
            });
    }

    /// Gaussian-weighted colour around (x, y), as `Canvas.sample`: loads a smudge brush.
    pub fn sample(&self, x: f64, y: f64, r: f64) -> [f32; 3] {
        let e = ((1.5 * r).ceil() as i64).max(1);
        let (xi, yi) = (x.trunc() as i64, y.trunc() as i64);
        let (x0, x1) = ((xi - e).max(0), (xi + e + 1).min(self.width as i64));
        let (y0, y1) = ((yi - e).max(0), (yi + e + 1).min(self.height as i64));
        if x0 >= x1 || y0 >= y1 {
            return [0.0; 3];
        }
        let var = (r / 2.0).powi(2).max(0.25);
        let (mut acc, mut ksum) = ([0f64; 3], 0f64);
        for py in y0..y1 {
            let dy = py as f64 + 0.5 - y;
            for px in x0..x1 {
                let dx = px as f64 + 0.5 - x;
                let k = (-0.5 * (dx * dx + dy * dy) / var).exp();
                ksum += k;
                let i = (py as usize * self.width + px as usize) * 3;
                for ch in 0..3 {
                    acc[ch] += self.img[i + ch] as f64 * k;
                }
            }
        }
        acc.map(|v| (v / ksum) as f32)
    }

    /// Box-filter down by an integer factor (exact area averaging).
    pub fn downsample(&mut self, n: usize) {
        if n <= 1 {
            return;
        }
        let (w, h) = (self.width / n, self.height / n);
        let inv = 1.0 / (n * n) as f64;
        let mut img = vec![0f32; w * h * 3];
        let mut wet = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let (mut acc, mut wacc) = ([0f64; 3], 0f64);
                for yy in y * n..(y + 1) * n {
                    for xx in x * n..(x + 1) * n {
                        let i = yy * self.width + xx;
                        for ch in 0..3 {
                            acc[ch] += self.img[i * 3 + ch] as f64;
                        }
                        wacc += self.wet[i] as f64;
                    }
                }
                for ch in 0..3 {
                    img[(y * w + x) * 3 + ch] = (acc[ch] * inv) as f32;
                }
                wet[y * w + x] = (wacc * inv) as f32;
            }
        }
        self.img = img;
        self.wet = wet;
        self.width = w;
        self.height = h;
        self.grain.clear();
        // laid dabs stay in output pixel units
        let k = n as f64;
        for l in self.laid.iter_mut().flatten() {
            l.dab.x /= k;
            l.dab.y /= k;
            l.dab.sx /= k;
            l.dab.sy /= k;
        }
    }

    pub fn to_rgb8(&self) -> RgbImage {
        let data = self
            .img
            .iter()
            .map(|&v| (v.clamp(0.0, 1.0) * 255.0f32 + 0.5f32) as u8)
            .collect();
        RgbImage {
            width: self.width,
            height: self.height,
            data,
        }
    }
}
