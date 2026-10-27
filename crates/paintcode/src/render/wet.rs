use std::collections::HashMap;
use std::simd::prelude::*;

use rayon::prelude::*;

use super::{Canvas, grain};
use crate::math::gauss::{self, CUTOFF_Q};

/// Lanes: pixels laid at once.
const L: usize = 8;
type F = Simd<f32, L>;

/// One dab: an anisotropic 2D Gaussian with an opacity, a profile hardness, the paper grain it
/// meets, and the colour the brush held when it was laid. A coat is a list of these.
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
    /// Every pixel of the 3-sigma square, as the numpy engine of 0.4 visited them. Kept as the
    /// reference that the fast path is tested against.
    Square,
    /// Only the rows and columns the q <= 9 ellipse can touch. Outside the ellipse the kernel is
    /// exactly zero and a zero dab leaves a pixel bit-for-bit unchanged, so the result is the
    /// same, bit for bit.
    Ellipse,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    y: usize,
    x0: usize,
    len: usize,
    off: usize,
}

/// A dab evaluated on the pixel grid: its kernel values, row by row, in the room it was given.
#[derive(Debug, Clone, Default)]
pub struct Footprint<'a> {
    spans: Vec<Span>,
    k: &'a [f32],
    alpha: f32,
    /// wsum <= 1e-8: nothing reaches the canvas, and the dab is neither composited nor recorded.
    pub skip: bool,
    y_min: usize,
    y_max: usize,
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
    planes: [&'a [f32]; 3],
    wet: &'a [f32],
    ox: usize,
    oy: usize,
    w: usize,
}

/// The canvas while it is painted: float RGB that receives dabs, a wetness map, and the paper
/// grain, at the size the dabs are laid (supersampled). Dried, it is the [`Canvas`].
pub struct WetCanvas {
    pub width: usize,
    pub height: usize,
    pub seed: i64,
    /// Row-major red, green and blue in 0..1, a plane each of `height * width`.
    pub planes: [Vec<f32>; 3],
    /// How much paint is still wet, `height * width`.
    pub wet: Vec<f32>,
    /// Every dab laid so far, in order and in this canvas's pixels, when recording is on.
    pub dabs: Option<Vec<Dab>>,
    grain: HashMap<u64, Vec<f32>>,
    /// Room for the kernels of the dabs being laid, kept between petals (see [`WetCanvas::rooms`]).
    pub scratch: Vec<f32>,
}

/// A copy of the paint and the paper; the scratch buffer starts empty again.
impl Clone for WetCanvas {
    fn clone(&self) -> Self {
        WetCanvas {
            width: self.width,
            height: self.height,
            seed: self.seed,
            planes: self.planes.clone(),
            wet: self.wet.clone(),
            dabs: self.dabs.clone(),
            grain: self.grain.clone(),
            scratch: Vec::new(),
        }
    }
}

impl std::fmt::Debug for WetCanvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WetCanvas")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("seed", &self.seed)
            .field("dabs", &self.dabs.as_ref().map(Vec::len))
            .finish_non_exhaustive()
    }
}

/// Whether rayon has one thread to give: then nothing is handed to it, since splitting work for
/// one thread only costs.
pub fn alone() -> bool {
    rayon::current_num_threads() == 1
}

/// The most rows a band of compositing takes.
const BAND: usize = 16;

/// Lay one row of a footprint, "over": eight pixels at a time, then the rest one by one, with
/// the same arithmetic in both, so the lanes change no bit.
#[inline]
fn lay(alpha: f32, col: [f32; 3], ks: &[f32], rgb: [&mut [f32]; 3], wet: &mut [f32]) {
    let n = ks.len() / L * L;
    let (one, zero, al) = (F::splat(1.0), F::splat(0.0), F::splat(alpha));
    let [r, g, b] = rgb;
    for i in (0..n).step_by(L) {
        let a = (al * F::from_slice(&ks[i..])).simd_clamp(zero, one);
        let keep = one - a;
        for (p, c) in [(&mut *r, col[0]), (&mut *g, col[1]), (&mut *b, col[2])] {
            (F::from_slice(&p[i..]) * keep + a * F::splat(c)).copy_to_slice(&mut p[i..]);
        }
        let w = F::from_slice(&wet[i..]);
        (w + a * (one - w)).copy_to_slice(&mut wet[i..]);
    }
    for i in n..ks.len() {
        let a = (alpha * ks[i]).clamp(0.0, 1.0);
        let keep = 1.0f32 - a;
        r[i] = r[i] * keep + a * col[0];
        g[i] = g[i] * keep + a * col[1];
        b[i] = b[i] * keep + a * col[2];
        wet[i] += a * (1.0f32 - wet[i]);
    }
}

impl WetCanvas {
    /// A blank canvas. Fails, rather than aborting, when it is larger than [`MAX_PIXELS`] or the
    /// memory cannot be had.
    pub fn new(
        width: usize,
        height: usize,
        background: [f32; 3],
        seed: i64,
    ) -> Result<WetCanvas, String> {
        let pixels = width
            .checked_mul(height)
            .filter(|&n| n <= MAX_PIXELS)
            .ok_or_else(|| {
                format!(
                    "a {width}x{height} canvas is too large: the limit is {MAX_PIXELS} pixels \
                     (16384 x 16384), counted after scale and supersample"
                )
            })?;
        let plane = |v: f32| -> Result<Vec<f32>, String> {
            let mut p = buffer(pixels, "canvas")?;
            p.fill(v);
            Ok(p)
        };
        let planes = [
            plane(background[0])?,
            plane(background[1])?,
            plane(background[2])?,
        ];
        Ok(WetCanvas {
            width,
            height,
            seed,
            planes,
            wet: buffer(pixels, "wetness map")?,
            dabs: None,
            grain: HashMap::new(),
            scratch: Vec::new(),
        })
    }

    pub fn live(&self) -> Source<'_> {
        Source {
            planes: [&self.planes[0], &self.planes[1], &self.planes[2]],
            wet: &self.wet,
            ox: 0,
            oy: 0,
            w: self.width,
        }
    }

    /// Copy of a rectangle of the canvas (clamped), owned. Read it with [`WetCanvas::view`].
    pub fn snapshot(
        &self,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
    ) -> ([Vec<f32>; 3], Vec<f32>, [usize; 3]) {
        let (x1, y1) = (x1.min(self.width), y1.min(self.height));
        let (x0, y0) = (x0.min(x1), y0.min(y1));
        let w = x1 - x0;
        let cut = |p: &[f32]| -> Vec<f32> {
            (y0..y1)
                .flat_map(|y| &p[y * self.width + x0..y * self.width + x1])
                .copied()
                .collect()
        };
        let planes = [
            cut(&self.planes[0]),
            cut(&self.planes[1]),
            cut(&self.planes[2]),
        ];
        (planes, cut(&self.wet), [x0, y0, w])
    }

    pub fn view<'a>(planes: &'a [Vec<f32>; 3], wet: &'a [f32], origin: [usize; 3]) -> Source<'a> {
        Source {
            planes: [&planes[0], &planes[1], &planes[2]],
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

    /// How many kernel values a dab's footprint may write: its clipped 3-sigma square.
    pub fn room(&self, d: &Dab) -> usize {
        self.bounds(d)
            .map_or(0, |(x0, x1, y0, y1)| (x1 - x0) * (y1 - y0))
    }

    /// A room for each dab, cut from one buffer that grows to fit and is kept between calls, so
    /// evaluating footprints in parallel allocates nothing and needs no lock.
    fn rooms<'a, 'd>(
        &self,
        dabs: impl Iterator<Item = &'d Dab> + Clone,
        scratch: &'a mut Vec<f32>,
    ) -> Vec<&'a mut [f32]> {
        let total: usize = dabs.clone().map(|d| self.room(d)).sum();
        if scratch.len() < total {
            scratch.resize(total, 0.0);
        }
        let mut rest = &mut scratch[..total];
        dabs.map(|d| {
            let (room, tail) = std::mem::take(&mut rest).split_at_mut(self.room(d));
            rest = tail;
            room
        })
        .collect()
    }

    /// Evaluate dabs in order, with their kernels in rooms cut from `scratch` (see
    /// [`WetCanvas::footprint`]), in parallel dab by dab. Cutting the dabs into runs of equal
    /// area, four for each thread, with small petals kept on one thread, was slower on ten cores:
    /// between petals every thread waits, so a petal left to one thread idles the rest, and
    /// rayon's own splitting lets the threads that finish early take dabs from the slow ones.
    pub fn footprints<'a, 'd>(
        &self,
        dabs: impl Iterator<Item = &'d Dab> + Clone,
        spans: Spans,
        src: Option<Source<'_>>,
        scratch: &'a mut Vec<f32>,
    ) -> Vec<(Footprint<'a>, Option<Pickup>)> {
        let dabs: Vec<&Dab> = dabs.collect();
        let rooms = self.rooms(dabs.iter().copied(), scratch);
        let eval = |(d, room)| self.footprint(d, spans, src, room);
        if alone() {
            return dabs.into_iter().zip(rooms).map(eval).collect();
        }
        dabs.into_par_iter().zip(rooms).map(eval).collect()
    }

    /// Evaluate one dab: its kernel, and if `pickup` the wet paint under it read from `src`. The
    /// kernel is written into `room`, which must hold [`WetCanvas::room`] values.
    ///
    /// The first half of laying one dab: float32 arithmetic per pixel, with the geometry above it
    /// in float64 (the split the numpy engine of 0.4 had, kept so the pixels stay the same).
    pub fn footprint<'a>(
        &self,
        d: &Dab,
        spans: Spans,
        src: Option<Source<'_>>,
        room: &'a mut [f32],
    ) -> (Footprint<'a>, Option<Pickup>) {
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
        // u = (dx c + dy s) / sx and v = (-dx s + dy c) / sy, with the divisions done once
        let (ux, vx) = (c / sx32, -s / sy32);
        let lanes = F::from_array(std::array::from_fn(|j| j as f32));
        let n = gauss::hardness_exponent(d.hardness) as f32;
        let profile = gauss::Profile::new(n);
        let paper = (d.grain > 0.0).then(|| self.grain_field(d.grain_scale));
        // (1.0 - grain) + grain * g, with the float64 brush values rounded to float32 first
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

        let (mut wsum, mut kwsum) = (0f64, 0f64);
        let mut used = 0;
        let mut under = [0f64; 3];
        for py in ry0..ry1 {
            let dy = (py as f32 + 0.5) - y32;
            let (u0, v0) = (dy * s / sx32, dy * c / sy32);
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
            let len = cx1 - cx0;
            fp.spans.push(Span {
                y: py,
                x0: cx0,
                len,
                off: used,
            });
            let row = &mut room[used..used + len];
            used += len;
            // the kernel, eight pixels at a time; the last few are a whole eight too, so a
            // pixel's value does not depend on where its row starts
            let paper_row = paper.map(|g| &g[py * self.width + cx0..py * self.width + cx1]);
            for (j, slot) in row.chunks_mut(L).enumerate() {
                let xs = F::splat((cx0 + j * L) as f32) + lanes;
                let dx = (xs + F::splat(0.5)) - F::splat(x32);
                let u = dx * F::splat(ux) + F::splat(u0);
                let v = dx * F::splat(vx) + F::splat(v0);
                let q = u * u + v * v;
                let inside = q.simd_le(F::splat(CUTOFF_Q));
                let mut k = inside.select(profile.lanes(q), F::splat(0.0));
                if let Some(g) = paper_row {
                    k *= F::splat(g_keep) + F::splat(g_amt) * F::load_or_default(&g[j * L..]);
                }
                if slot.len() == L {
                    k.copy_to_slice(slot);
                } else {
                    slot.copy_from_slice(&k.as_array()[..slot.len()]);
                }
            }
            // the sums, pixel by pixel in order
            for (&k, px) in row.iter().zip(cx0..cx1) {
                wsum += k as f64;
                if let Some(src) = src {
                    let i = (py - src.oy) * src.w + (px - src.ox);
                    let kw = k * src.wet[i];
                    kwsum += kw as f64;
                    for ch in 0..3 {
                        under[ch] += (src.planes[ch][i] * kw) as f64;
                    }
                }
            }
        }
        if wsum <= 1e-8 {
            return (fp, None);
        }
        let room: &'a [f32] = room;
        fp.k = &room[..used];
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

    /// Lay footprints down in order, "over" compositing, each in its colour. The rows the
    /// footprints reach are split into bands that run in parallel, about four for each thread,
    /// so a small petal keeps every thread busy too; within a pixel the order and the arithmetic
    /// are those of a one-dab-at-a-time engine, so the result does not depend on the number of
    /// threads.
    pub fn composite(&mut self, prints: &[(&Footprint<'_>, [f32; 3])]) {
        let w = self.width;
        let laid = prints.iter().filter(|(fp, _)| !fp.skip);
        let (top, bottom) = laid.fold((usize::MAX, 0), |(t, b), (fp, _)| {
            (t.min(fp.y_min), b.max(fp.y_max))
        });
        if top >= bottom {
            return;
        }
        let rows = top * w..bottom * w;
        let [red, green, blue] = &mut self.planes;
        let (r, g, b, wet) = (
            &mut red[rows.clone()],
            &mut green[rows.clone()],
            &mut blue[rows.clone()],
            &mut self.wet[rows],
        );
        let lay_band = |b0: usize, [r, g, b, wet]: [&mut [f32]; 4]| {
            let b1 = b0 + wet.len() / w;
            for &(fp, col) in prints {
                if fp.skip || fp.y_max <= b0 || fp.y_min >= b1 {
                    continue;
                }
                // spans are in row order: the band's are one run of them
                let first = fp.spans.partition_point(|sp| sp.y < b0);
                let last = first + fp.spans[first..].partition_point(|sp| sp.y < b1);
                for sp in &fp.spans[first..last] {
                    let row = (sp.y - b0) * w + sp.x0..(sp.y - b0) * w + sp.x0 + sp.len;
                    let ks = &fp.k[sp.off..sp.off + sp.len];
                    let (r, g, b) = (
                        &mut r[row.clone()],
                        &mut g[row.clone()],
                        &mut b[row.clone()],
                    );
                    lay(fp.alpha, col, ks, [r, g, b], &mut wet[row]);
                }
            }
        };
        // one thread lays every row as one band, with nothing handed to rayon
        if alone() {
            return lay_band(top, [r, g, b, wet]);
        }
        let band = (bottom - top)
            .div_ceil(4 * rayon::current_num_threads())
            .clamp(1, BAND);
        r.par_chunks_mut(band * w)
            .zip(g.par_chunks_mut(band * w))
            .zip(b.par_chunks_mut(band * w))
            .zip(wet.par_chunks_mut(band * w))
            .enumerate()
            .for_each(|(i, (((r, g), b), wet))| lay_band(top + i * band, [r, g, b, wet]));
    }

    /// Gaussian-weighted colour around (x, y): loads a smudge brush. None when the window of
    /// radius `1.5 r` around the point holds no pixel of the canvas.
    pub fn sample(&self, x: f64, y: f64, r: f64) -> Option<[f32; 3]> {
        let e = ((1.5 * r).ceil() as i64).max(1);
        let (xi, yi) = (x.trunc() as i64, y.trunc() as i64);
        let (x0, x1) = ((xi - e).max(0), (xi + e + 1).min(self.width as i64));
        let (y0, y1) = ((yi - e).max(0), (yi + e + 1).min(self.height as i64));
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let var = (r / 2.0).powi(2).max(0.25);
        let (mut acc, mut ksum) = ([0f64; 3], 0f64);
        for py in y0..y1 {
            let dy = py as f64 + 0.5 - y;
            for px in x0..x1 {
                let dx = px as f64 + 0.5 - x;
                let k = (-0.5 * (dx * dx + dy * dy) / var).exp();
                ksum += k;
                let i = py as usize * self.width + px as usize;
                for ch in 0..3 {
                    acc[ch] += self.planes[ch][i] as f64 * k;
                }
            }
        }
        Some(acc.map(|v| (v / ksum) as f32))
    }

    /// Box-filter down by an integer factor (exact area averaging).
    pub fn downsample(&mut self, n: usize) {
        if n <= 1 {
            return;
        }
        let (w, h) = (self.width / n, self.height / n);
        let inv = 1.0 / (n * n) as f64;
        let mut planes = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
        let mut wet = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let (mut acc, mut wacc) = ([0f64; 3], 0f64);
                for yy in y * n..(y + 1) * n {
                    for xx in x * n..(x + 1) * n {
                        let i = yy * self.width + xx;
                        for ch in 0..3 {
                            acc[ch] += self.planes[ch][i] as f64;
                        }
                        wacc += self.wet[i] as f64;
                    }
                }
                for ch in 0..3 {
                    planes[ch][y * w + x] = (acc[ch] * inv) as f32;
                }
                wet[y * w + x] = (wacc * inv) as f32;
            }
        }
        self.planes = planes;
        self.wet = wet;
        self.width = w;
        self.height = h;
        self.grain.clear();
    }

    /// Every pixel's red, green and blue, row by row: `height * width * 3` values.
    pub fn rgb(&self) -> Vec<f32> {
        let [r, g, b] = &self.planes;
        r.iter()
            .zip(g)
            .zip(b)
            .flat_map(|((&r, &g), &b)| [r, g, b])
            .collect()
    }

    /// The colour of the pixel at (x, y).
    pub fn pixel(&self, x: usize, y: usize) -> [f32; 3] {
        let i = y * self.width + x;
        [self.planes[0][i], self.planes[1][i], self.planes[2][i]]
    }

    /// Paint the pixel at (x, y) one colour.
    pub fn set_pixel(&mut self, x: usize, y: usize, rgb: [f32; 3]) {
        let i = y * self.width + x;
        for (plane, v) in self.planes.iter_mut().zip(rgb) {
            plane[i] = v;
        }
    }

    /// The pixels you see: every value clamped and rounded to 8 bits, no flower on them yet.
    pub fn dry(&self) -> Canvas {
        let pixels = self
            .rgb()
            .iter()
            .map(|&v| (v.clamp(0.0, 1.0) * 255.0f32 + 0.5f32) as u8)
            .collect();
        Canvas {
            width: self.width,
            height: self.height,
            pixels,
            flower: None,
        }
    }
}
