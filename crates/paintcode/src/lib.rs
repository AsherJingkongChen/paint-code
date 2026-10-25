//! paint-code: a scene goes in, pixels come out.
//!
//! This crate only conducts. The parts it calls, from the leaf up:
//!
//! | crate              | part   | does                                             |
//! |--------------------|--------|--------------------------------------------------|
//! | `paintcode-math`   | leaf   | counter-based RNG, dab profile and geometry      |
//! | `paintcode-scene`  | bones  | scene, brush and stroke types, presets, colours  |
//! | `paintcode-render` | render | dabs to pixels, paper grain, downsampling        |
//! | `paintcode-stroke` | flesh  | control points to dabs, carrying paint           |
//! | `paintcode-io`     | files  | encoders and decoders                            |

pub use paintcode_io::{image_to_png, png_to_image};
pub use paintcode_render::{Canvas, RgbImage, Spans};
pub use paintcode_scene::{Error, Result, Scene};
pub use paintcode_stroke::Options;

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
        paintcode_stroke::paint_stroke(&mut canvas, &brush, &s.color, &pts, s.smooth, opts.stroke);
    }
    canvas.downsample(ss as usize);
    Ok(canvas)
}

/// Scene JSON in, PNG bytes out.
pub fn render_png(scene_json: &str, opts: &RenderOpts) -> Result<Vec<u8>> {
    let scene = Scene::from_json(scene_json)?;
    Ok(image_to_png(&render(&scene, opts)?.to_rgb8()))
}
