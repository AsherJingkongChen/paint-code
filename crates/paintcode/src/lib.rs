//! paintcode: a flower goes in, a canvas comes out.
//!
//! A flower is what you write: a size, a background and petals (strokes of any shape). Painting
//! turns each petal into dabs; all the dabs together are the coat. The coat over the background
//! is the canvas, the pixels you see.
//!
//! One library, one module per part of the engine. Dependencies point one way, from the leaf up,
//! and `tests/layering.rs` fails if a module reaches back down the list:
//!
//! | module   | part   | may use               | does                                            |
//! |----------|--------|-----------------------|-------------------------------------------------|
//! | `math`   | leaf   | nothing               | counter-based RNG, dab profile and geometry     |
//! | `flower` | bones  | math                  | flower, petal and brush types, presets, colours |
//! | `render` | render | math                  | dabs to pixels, paper grain, downsampling       |
//! | `coat`   | flesh  | math, flower, render  | petals to dabs, carrying paint                  |
//! | `io`     | files  | render                | encoders and decoders, in pairs                 |
//!
//! This file only conducts: [`render`] walks the petals of a [`Flower`] over a [`Canvas`].

pub mod coat;
pub mod flower;
pub mod io;
pub mod math;
pub mod render;

pub use coat::{Options, Painted};
pub use flower::{Error, Flower, Result};
pub use io::{image_to_png, laid_to_ply, petal_to_scene, png_to_image, scene_to_petal};
pub use render::{Canvas, Laid, RgbImage, Spans};

#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    /// Render larger: coordinates and brush sizes scale together.
    pub scale: f64,
    /// Paint at N times the size and box-filter down; crisp hard edges at 2 or 3.
    pub supersample: i64,
    /// Keep every laid dab on the canvas (`Canvas::laid`), for a PLY export.
    pub record: bool,
    pub petal: Options,
}

impl Default for RenderOpts {
    fn default() -> Self {
        RenderOpts {
            scale: 1.0,
            supersample: 1,
            record: false,
            petal: Options::default(),
        }
    }
}

/// Paint a flower on a blank canvas.
pub fn render(flower: &Flower, opts: &RenderOpts) -> Result<Canvas> {
    if !(opts.scale.is_finite() && opts.scale > 0.0) {
        return Err(Error(format!(
            "scale must be a positive number, got {}",
            opts.scale
        )));
    }
    let ss = opts.supersample;
    if ss < 1 {
        return Err(Error(format!("supersample must be at least 1, got {ss}")));
    }
    let side = |v: usize| (v as f64 * opts.scale).round_ties_even() as usize;
    let (w, h) = (side(flower.width), side(flower.height));
    if w == 0 || h == 0 {
        return Err(Error(format!(
            "a {}x{} flower at scale {} has no pixels",
            flower.width, flower.height, opts.scale
        )));
    }
    let (w, h) = (w.saturating_mul(ss as usize), h.saturating_mul(ss as usize));
    let mut canvas = Canvas::new(w, h, flower.background, flower.seed).map_err(Error)?;
    let scale = opts.scale * ss as f64;
    if opts.record {
        canvas.laid = Some(Vec::new());
    }
    let (fw, fh) = (flower.width, flower.height);
    for (i, p) in flower.petals.iter().enumerate() {
        if let Some([x, y]) = p.far_point(fw, fh) {
            return Err(Error(format!(
                "petal {i}: point [{x}, {y}] is too far outside the {fw}x{fh} canvas \
                 (more than {} px)",
                flower::POINT_MARGIN * fw.max(fh) as f64
            )));
        }
    }
    for p in &flower.petals {
        let brush = p.brush.scaled(scale);
        let pts: Vec<[f64; 4]> = p
            .points
            .iter()
            .map(|q| [q[0] * scale, q[1] * scale, q[2], q[3]])
            .collect();
        coat::paint_petal(&mut canvas, &brush, &p.color, &pts, p.smooth, opts.petal);
    }
    canvas.downsample(ss as usize);
    Ok(canvas)
}
