//! A coat: every dab of a flower in the order it was laid, and what it takes to lay them again.

use rayon::prelude::*;

use super::{Dab, Footprint, Spans, WetCanvas};

/// All the dabs of a painted flower, in order, with the colour each one carried.
///
/// Dabs are in the pixels of the canvas they were laid on, which is `supersample` times the size
/// of the finished canvas; laying them again and box-filtering down gives the same pixels, bit
/// for bit, as the render that recorded them.
#[derive(Debug, Clone, PartialEq)]
pub struct Coat {
    /// The finished canvas, in pixels.
    pub width: usize,
    pub height: usize,
    pub supersample: usize,
    pub background: [f32; 3],
    /// Keys the paper grain; the jitter is already in the dabs.
    pub seed: i64,
    pub dabs: Vec<Dab>,
}

/// Dabs whose footprints are evaluated at once when a coat is laid.
const BATCH: usize = 4096;

/// Lay every dab on a fresh canvas and box-filter it down: the canvas this coat makes, still wet.
pub fn lay_coat(coat: &Coat) -> Result<WetCanvas, String> {
    let ss = coat.supersample.max(1);
    let mut canvas = WetCanvas::new(
        coat.width.saturating_mul(ss),
        coat.height.saturating_mul(ss),
        coat.background,
        coat.seed,
    )?;
    // the paper is the same whichever dab asks first, so make every grain before the
    // footprints read them in parallel
    for d in &coat.dabs {
        if d.grain > 0.0 {
            canvas.prepare_grain(d.grain_scale);
        }
    }
    for batch in coat.dabs.chunks(BATCH) {
        let cv: &WetCanvas = &canvas;
        let prints: Vec<Footprint> = batch
            .par_iter()
            .map(|d| cv.footprint(d, Spans::Ellipse, None).0)
            .collect();
        let coloured: Vec<(&Footprint, [f32; 3])> =
            prints.iter().zip(batch).map(|(f, d)| (f, d.rgb)).collect();
        canvas.composite(&coloured);
    }
    canvas.downsample(ss);
    Ok(canvas)
}
