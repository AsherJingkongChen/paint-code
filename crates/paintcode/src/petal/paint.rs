use crate::flower::{Brush, Color};
use crate::math::rng;
use crate::render::{Canvas, Dab, Footprint, Pickup, Spans};
use rayon::prelude::*;

use super::path::{Sample, samples};

/// Knobs that change how fast a petal is painted, never what it looks like.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub spans: Spans,
    /// Upper bound on kernel values held at once (f32 each); longer petals go in chunks.
    pub budget: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            spans: Spans::Ellipse,
            budget: 1 << 24,
        }
    }
}

/// Where each dab goes and what carries over between dabs, before any pixel is touched.
struct Plan {
    dab: Dab,
    /// Exponent of the pickup mix for this dab: brush.step(p) / max(r, 1e-3).
    mix_exp: f64,
}

fn plan(seed: i64, brush: &Brush, sid: i64, path: &[Sample]) -> Vec<Plan> {
    let jitter = |n: i64, slot: i64, amount: f64| {
        -amount + 2.0 * amount * rng::uniform(&[seed, rng::JITTER, sid, n, slot])
    };
    let k = brush.sigma_scale();
    let mut load = brush.load;
    let mut last_t = 0.0;
    let mut out = Vec::new();
    for (n, s) in path.iter().enumerate() {
        let n = n as i64;
        let (mut x, mut y) = (s.x, s.y);
        if let Some(hl) = brush.half_life.filter(|&h| h != 0.0) {
            load *= 0.5f64.powf((s.travelled - last_t) / hl);
            load = load.max(brush.min_load);
        }
        last_t = s.travelled;

        let mut r = brush.radius(s.p);
        if brush.size_jitter != 0.0 {
            r *= 1.0 + jitter(n, rng::SIZE, brush.size_jitter);
        }
        let mut flow = brush.flow_at(s.p) * load;
        if brush.flow_jitter != 0.0 {
            flow *= 1.0 + jitter(n, rng::FLOW, brush.flow_jitter);
        }
        flow *= 1.0 - 0.35 * s.tilt;
        // flow is the film laid at the default spacing; other spacings build up the same
        let reference = (0.3 * brush.footprint_along(brush.radius(s.p))).max(0.35);
        flow = 1.0 - (1.0 - flow.min(0.999)).powf(brush.step(s.p) / reference);

        let mut theta = match brush.angle {
            None => s.direction + std::f64::consts::FRAC_PI_2,
            Some(a) => a.to_radians(),
        };
        if brush.angle_jitter != 0.0 {
            theta += jitter(n, rng::ANGLE, brush.angle_jitter).to_radians();
        }
        if brush.pos_jitter != 0.0 {
            x += jitter(n, rng::POS_X, 1.0) * brush.pos_jitter * r;
            y += jitter(n, rng::POS_Y, 1.0) * brush.pos_jitter * r;
        }
        let dab = Dab {
            x,
            y,
            sx: k * r * (1.0 + brush.tilt_stretch * s.tilt),
            sy: k * r * brush.aspect,
            theta,
            alpha: flow,
            hardness: brush.hardness,
            grain: brush.grain,
            grain_scale: brush.grain_scale,
            // the colour is known only when the paint is carried along, below
            rgb: [0.0; 3],
        };
        out.push(Plan {
            dab,
            mix_exp: brush.step(s.p) / r.max(1e-3),
        });
    }
    out
}

/// What one petal left behind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Painted {
    /// Dabs laid out along the path (some may fall off the canvas).
    pub dabs: usize,
    /// The colour on the brush after its last dab, pickup included.
    pub carried: [f32; 3],
}

/// Lay one petal onto the canvas.
pub fn paint_petal(
    canvas: &mut Canvas,
    brush: &Brush,
    color: &Color,
    points: &[[f64; 4]],
    smooth: bool,
    opts: Options,
) -> Painted {
    let sid = canvas.petals;
    canvas.petals += 1;
    let path = samples(points, brush, smooth);
    let plans = plan(canvas.seed, brush, sid, &path);
    if plans.is_empty() {
        let carried = match color {
            Color::Rgb(c) => *c,
            Color::Smudge => [0.0; 3],
        };
        return Painted { dabs: 0, carried };
    }
    if brush.grain > 0.0 {
        canvas.prepare_grain(brush.grain_scale);
    }
    let mut carried: [f32; 3] = match color {
        Color::Rgb(c) => *c,
        // a smudge loads the brush with what is under the first sample, before any jitter
        Color::Smudge => canvas.sample(path[0].x, path[0].y, brush.radius(path[0].p)),
    };
    let pickup = brush.pickup > 0.0;

    // chunks of dabs whose kernels fit the budget
    let area = |p: &Plan| {
        canvas
            .bounds(&p.dab)
            .map_or(0, |(x0, x1, y0, y1)| (x1 - x0) * (y1 - y0))
    };
    let mut chunks: Vec<std::ops::Range<usize>> = Vec::new();
    let (mut start, mut used) = (0usize, 0usize);
    for (i, p) in plans.iter().enumerate() {
        let a = area(p);
        if i > start && used + a > opts.budget {
            chunks.push(start..i);
            start = i;
            used = 0;
        }
        used += a;
    }
    chunks.push(start..plans.len());

    // Pickup reads the canvas as it was when the petal began. With one chunk nothing has been
    // composited yet, so the live canvas is that state; with more, keep a snapshot of it.
    let snapshot = (pickup && chunks.len() > 1).then(|| {
        let b = plans.iter().filter_map(|p| canvas.bounds(&p.dab));
        let (x0, x1, y0, y1) = b.fold((usize::MAX, 0, usize::MAX, 0), |a, (x0, x1, y0, y1)| {
            (a.0.min(x0), a.1.max(x1), a.2.min(y0), a.3.max(y1))
        });
        canvas.snapshot(x0, y0, x1, y1)
    });

    for range in chunks {
        let src = match &snapshot {
            Some((img, wet, origin)) => Some(Canvas::view(img, wet, *origin)),
            None if pickup => Some(canvas.live()),
            None => None,
        };
        let cv: &Canvas = canvas;
        let evaluated: Vec<(Footprint, Option<Pickup>)> = plans[range.clone()]
            .par_iter()
            .map(|p| cv.footprint(&p.dab, opts.spans, src))
            .collect();
        // carry the colour along: each dab is laid in the colour the brush holds before its own pickup
        let mut colours = Vec::with_capacity(evaluated.len());
        for (plan, (_, under)) in plans[range.clone()].iter().zip(&evaluated) {
            colours.push(carried);
            if let (true, Some(u)) = (pickup, under) {
                let mix = 1.0 - (1.0 - brush.pickup * u.wetness).powf(plan.mix_exp);
                let (keep, take) = ((1.0 - mix) as f32, mix as f32);
                for ch in 0..3 {
                    carried[ch] = carried[ch] * keep + (u.colour[ch] as f32) * take;
                }
            }
        }
        let prints: Vec<(&Footprint, [f32; 3])> =
            evaluated.iter().map(|(f, _)| f).zip(colours).collect();
        canvas.composite(&prints);
        if let Some(laid) = canvas.laid.as_mut() {
            // only the dabs that reached the canvas, as they were laid
            for (plan, (fp, rgb)) in plans[range].iter().zip(&prints) {
                if !fp.skip {
                    let mut dab = plan.dab;
                    dab.sx = dab.sx.max(0.5);
                    dab.sy = dab.sy.max(0.5);
                    dab.rgb = *rgb;
                    laid.push(dab);
                }
            }
        }
    }
    Painted {
        dabs: plans.len(),
        carried,
    }
}
