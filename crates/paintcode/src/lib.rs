//! paintcode: a scene goes in, pixels come out.
//!
//! One library, one module per part of the engine. Dependencies point one way, from the leaf up,
//! and `tests/layering.rs` fails if a module reaches back down the list:
//!
//! | module   | part   | may use               | does                                            |
//! |----------|--------|-----------------------|-------------------------------------------------|
//! | `math`   | leaf   | nothing               | counter-based RNG, dab profile and geometry     |
//! | `scene`  | bones  | math                  | scene, brush and stroke types, presets, colours |
//! | `render` | render | math                  | dabs to pixels, paper grain, downsampling       |
//! | `stroke` | flesh  | math, scene, render   | control points to dabs, carrying paint          |
//! | `io`     | files  | render                | encoders and decoders, in pairs                 |
//!
//! This file only conducts: [`render`] walks the strokes of a [`Scene`] over a [`Canvas`].

pub mod cli;
pub mod io;
pub mod math;
pub mod render;
pub mod scene;
pub mod stroke;

pub use io::{image_to_png, laid_to_ply, petal_to_scene, png_to_image, scene_to_petal};
pub use render::{Canvas, Laid, RgbImage, Spans};
pub use scene::{Error, Result, Scene};
pub use stroke::{Options, Painted};

#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    /// Render larger: coordinates and brush sizes scale together.
    pub scale: f64,
    /// Paint at N times the size and box-filter down; `None` uses `canvas.supersample`.
    pub supersample: Option<i64>,
    /// Keep every laid dab on the canvas (`Canvas::laid`), for a PLY export.
    pub record: bool,
    pub stroke: Options,
}

impl Default for RenderOpts {
    fn default() -> Self {
        RenderOpts {
            scale: 1.0,
            supersample: None,
            record: false,
            stroke: Options::default(),
        }
    }
}

/// Paint a scene on a blank canvas.
pub fn render(scene: &Scene, opts: &RenderOpts) -> Result<Canvas> {
    render_over(scene, None, opts)
}

/// Paint a scene, on top of `base` when there is one.
///
/// A base image is the canvas: its size replaces `canvas.width` and `canvas.height`, it replaces
/// the background, and it counts as dry paint. It is painted at its own size, so `scale` and
/// `supersample` must be 1 (the scene's own `canvas.supersample` is not used).
pub fn render_over(scene: &Scene, base: Option<&RgbImage>, opts: &RenderOpts) -> Result<Canvas> {
    let cfg = &scene.canvas;
    if !(opts.scale.is_finite() && opts.scale > 0.0) {
        return Err(Error(format!(
            "scale must be a positive number, got {}",
            opts.scale
        )));
    }
    let (mut canvas, ss, scale) = match base {
        Some(img) => {
            if opts.scale != 1.0 || opts.supersample.is_some_and(|s| s != 1) {
                return Err(Error(
                    "a base image is painted at its own size: scale and supersample must be 1"
                        .into(),
                ));
            }
            (Canvas::from_rgb8(img, cfg.seed).map_err(Error)?, 1, 1.0)
        }
        None => {
            let ss = opts.supersample.unwrap_or(cfg.supersample);
            if ss < 1 {
                return Err(Error(format!("supersample must be >= 1, got {ss}")));
            }
            let side = |v: usize| (v as f64 * opts.scale).round_ties_even() as usize;
            let (w, h) = (side(cfg.width), side(cfg.height));
            if w == 0 || h == 0 {
                return Err(Error(format!(
                    "a {}x{} canvas at scale {} has no pixels",
                    cfg.width, cfg.height, opts.scale
                )));
            }
            let ss_u = ss as usize;
            let (w, h) = (w.saturating_mul(ss_u), h.saturating_mul(ss_u));
            let c = Canvas::new(w, h, cfg.background, cfg.seed).map_err(Error)?;
            (c, ss, opts.scale * ss as f64)
        }
    };
    if opts.record {
        canvas.laid = Some(Vec::new());
    }
    for s in &scene.strokes {
        let brush = s.brush.scaled(scale);
        let pts: Vec<[f64; 4]> = s
            .points
            .iter()
            .map(|p| [p[0] * scale, p[1] * scale, p[2], p[3]])
            .collect();
        stroke::paint_stroke(&mut canvas, &brush, &s.color, &pts, s.smooth, opts.stroke);
    }
    canvas.downsample(ss as usize);
    Ok(canvas)
}

/// Scene JSON in, PNG bytes out.
pub fn render_png(scene_json: &str, opts: &RenderOpts) -> Result<Vec<u8>> {
    let scene = Scene::from_json(scene_json)?;
    Ok(image_to_png(&render(&scene, opts)?.to_rgb8()))
}
