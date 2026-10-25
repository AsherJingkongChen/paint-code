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

pub mod io;
pub mod math;
pub mod render;
pub mod scene;
pub mod stroke;

pub use io::{image_to_png, png_to_image};
pub use render::{Canvas, RgbImage, Spans};
pub use scene::{Error, Result, Scene};
pub use stroke::Options;

#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    /// Render larger: coordinates and brush sizes scale together.
    pub scale: f64,
    /// Paint at N times the size and box-filter down; `None` uses `canvas.supersample`.
    pub supersample: Option<i64>,
    pub stroke: Options,
}

impl Default for RenderOpts {
    fn default() -> Self {
        RenderOpts {
            scale: 1.0,
            supersample: None,
            stroke: Options::default(),
        }
    }
}

/// Paint a scene, as `paintcode.scene.render`.
pub fn render(scene: &Scene, opts: &RenderOpts) -> Result<Canvas> {
    let cfg = &scene.canvas;
    let ss = opts.supersample.unwrap_or(cfg.supersample);
    if ss < 1 {
        return Err(Error("supersample must be >= 1".into()));
    }
    let size = |v: f64| (v * opts.scale).round_ties_even() as usize * ss as usize;
    let mut canvas = Canvas::new(size(cfg.width), size(cfg.height), cfg.background, cfg.seed);
    let scale = opts.scale * ss as f64;
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
