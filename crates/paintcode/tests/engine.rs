//! What the engine promises, one behaviour per test (ported from the numpy engine's
//! tests/test_engine.py when that engine retired).

use paintcode::render::{Canvas, Dab, Spans};
use paintcode::scene::{preset, Brush, Color};
use paintcode::stroke::{paint_stroke, samples, Options};
use paintcode::{render, render_over, RenderOpts, RgbImage, Scene};
use serde_json::{json, Value};

fn scene(v: Value) -> Scene {
    Scene::from_value(&v).unwrap()
}

fn small(seed: i64) -> Value {
    json!({"canvas": {"width": 160, "height": 80, "seed": seed},
           "strokes": [{"brush": "filbert", "color": "#c8782a",
                        "points": [[10, 40, 0.2], [60, 20, 1.0], [110, 60, 0.7], [150, 40, 0.1]]}]})
}

fn paint(v: Value, opts: RenderOpts) -> Canvas {
    render(&scene(v), &opts).unwrap()
}

fn rgb(hex: &str) -> [f32; 3] {
    paintcode::scene::parse_color(&json!(hex)).unwrap()
}

fn row(points: &[[f64; 2]]) -> Vec<[f64; 4]> {
    points.iter().map(|p| [p[0], p[1], 1.0, 0.0]).collect()
}

fn stroke(c: &mut Canvas, b: &Brush, color: Color, pts: &[[f64; 4]]) -> paintcode::Painted {
    paint_stroke(c, b, &color, pts, true, Options::default())
}

fn blank(w: usize, h: usize, bg: &str) -> Canvas {
    Canvas::new(w, h, rgb(bg), 0).unwrap()
}

#[test]
fn same_scene_same_pixels() {
    let o = RenderOpts::default();
    assert!(paint(small(3), o).img == paint(small(3), o).img);
}

#[test]
fn seed_changes_jitter() {
    let o = RenderOpts::default();
    assert!(paint(small(3), o).img != paint(small(4), o).img);
}

#[test]
fn duplicate_points_do_not_change_the_stroke() {
    let b = preset("ink").unwrap();
    let pts = row(&[[10., 40.], [60., 40.], [110., 40.], [150., 40.]]);
    let dup = row(&[
        [10., 40.],
        [60., 40.],
        [60., 40.],
        [110., 40.],
        [110., 40.],
        [110., 40.],
        [150., 40.],
    ]);
    let (mut c1, mut c2) = (blank(160, 80, "#ffffff"), blank(160, 80, "#ffffff"));
    stroke(&mut c1, &b, Color::Rgb([0.0; 3]), &pts);
    stroke(&mut c2, &b, Color::Rgb([0.0; 3]), &dup);
    let gap = c1
        .img
        .iter()
        .zip(&c2.img)
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    assert!(gap < 1e-6, "{gap}");
}

#[test]
fn joints_keep_the_stroke_direction() {
    // the paint-mcp bug: a repeated joint point got direction atan2(0, 0) = 0
    let b = Brush {
        size: 6.0,
        aspect: 0.4,
        ..Brush::default()
    };
    let pts = row(&[[10., 10.], [30., 30.], [30., 30.], [50., 50.], [70., 70.]]);
    let s = samples(&pts, &b, false);
    assert!(!s.is_empty());
    assert!(s
        .iter()
        .all(|s| (s.direction - std::f64::consts::FRAC_PI_4).abs() < 1e-6));
}

#[test]
fn pressure_grows_radius_and_flow() {
    let b = preset("round").unwrap();
    let ps: Vec<f64> = (0..=10).map(|i| i as f64 / 10.0).collect();
    let r: Vec<f64> = ps.iter().map(|&p| b.radius(p)).collect();
    let f: Vec<f64> = ps.iter().map(|&p| b.flow_at(p)).collect();
    assert!(r.windows(2).all(|w| w[0] < w[1]));
    assert!(f.windows(2).all(|w| w[0] < w[1]));
    assert!((r[0] - b.size * b.min_size).abs() < 1e-12);
}

#[test]
fn a_single_point_is_one_dab() {
    let v = json!({"canvas": {"width": 40, "height": 40},
                   "strokes": [{"brush": "round", "color": "#f00", "points": [[20, 20, 0.8]]}]});
    let c = paint(
        v,
        RenderOpts {
            record: true,
            ..RenderOpts::default()
        },
    );
    assert_eq!(c.laid.unwrap().len(), 1);
}

#[test]
fn pickup_does_not_depend_on_dab_spacing() {
    let end_colour = |spacing: f64| {
        let mut c = blank(300, 40, "#ffffff");
        // wet black paint already on the right half
        for y in 0..40 {
            for x in 150..300 {
                c.img[(y * 300 + x) * 3..(y * 300 + x) * 3 + 3].fill(0.0);
                c.wet[y * 300 + x] = 1.0;
            }
        }
        let b = Brush {
            size: 8.0,
            flow: 1.0,
            hardness: 0.8,
            pickup: 0.05,
            spacing,
            ..Brush::default()
        };
        stroke(
            &mut c,
            &b,
            Color::Rgb(rgb("#ff0000")),
            &row(&[[10., 20.], [290., 20.]]),
        )
        .carried
    };
    let (a, b) = (end_colour(0.08), end_colour(0.3));
    assert!(
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 0.06),
        "{a:?} {b:?}"
    );
}

#[test]
fn scale_renders_larger() {
    let c = paint(
        small(3),
        RenderOpts {
            scale: 2.0,
            ..RenderOpts::default()
        },
    );
    assert_eq!((c.width, c.height), (320, 160));
}

#[test]
fn a_dab_is_a_gaussian() {
    let mut c = blank(64, 64, "#000000");
    let d = Dab {
        x: 32.0,
        y: 32.0,
        sx: 6.0,
        sy: 6.0,
        theta: 0.0,
        alpha: 1.0,
        hardness: 0.0,
        grain: 0.0,
        grain_scale: 3.0,
    };
    let (fp, _) = c.footprint(&d, Spans::Ellipse, None);
    c.composite(&[(&fp, [1.0; 3])]);
    // one sigma from the centre (pixel centres at x + 0.5) the value is exp(-0.5)
    let v = c.img[(31 * 64 + 32 + 6 - 1) * 3] as f64;
    let want = (-0.5 * ((5.5f64 / 6.0).powi(2) + (0.5f64 / 6.0).powi(2))).exp();
    assert!((v - want).abs() / want < 1e-4, "{v} {want}");
}

#[test]
fn a_dry_background_is_not_picked_up() {
    let mut c = blank(300, 40, "#f4efe6");
    let b = Brush {
        size: 8.0,
        pickup: 0.5,
        ..Brush::default()
    };
    let carried = stroke(
        &mut c,
        &b,
        Color::Rgb(rgb("#2b2d42")),
        &row(&[[10., 20.], [290., 20.]]),
    )
    .carried;
    let want = rgb("#2b2d42");
    assert!(carried.iter().zip(&want).all(|(a, b)| (a - b).abs() < 1e-6));
}

#[test]
fn a_smudge_starts_with_the_colour_under_it() {
    let mut c = blank(100, 40, "#ffffff");
    for y in 0..40 {
        for x in 0..50 {
            c.img[(y * 100 + x) * 3..(y * 100 + x) * 3 + 3].copy_from_slice(&[1.0, 0.0, 0.0]);
            c.wet[y * 100 + x] = 1.0;
        }
    }
    stroke(
        &mut c,
        &preset("blender").unwrap(),
        Color::Smudge,
        &row(&[[20., 20.], [80., 20.]]),
    );
    let px = &c.img[(20 * 100 + 60) * 3..(20 * 100 + 60) * 3 + 3];
    assert!(
        px[0] > 0.9 && px[1] < 0.9,
        "red dragged into the white: {px:?}"
    );
}

#[test]
fn hardness_sharpens_the_edge() {
    let edge = |h: f64| {
        let mut c = blank(200, 60, "#000000");
        let b = Brush {
            size: 18.0,
            aspect: 0.5,
            hardness: h,
            ..Brush::default()
        };
        stroke(
            &mut c,
            &b,
            Color::Rgb([1.0; 3]),
            &row(&[[20., 30.], [180., 30.]]),
        );
        let col: Vec<f32> = (0..30).map(|y| c.img[(y * 200 + 100) * 3]).collect();
        let first = |t: f32| col.iter().position(|&v| v > t).unwrap() as i64;
        first(0.9) - first(0.1)
    };
    let (a, b, c) = (edge(0.0), edge(0.6), edge(0.9));
    assert!(a > b && b > c, "{a} {b} {c}");
}

#[test]
fn the_same_size_looks_the_same_at_any_hardness() {
    let widths: Vec<usize> = [0.0, 0.5, 0.8, 0.95]
        .iter()
        .map(|&h| {
            let mut c = blank(200, 80, "#000000");
            let b = Brush {
                size: 16.0,
                hardness: h,
                ..Brush::default()
            };
            stroke(
                &mut c,
                &b,
                Color::Rgb([1.0; 3]),
                &row(&[[20., 40.], [180., 40.]]),
            );
            (0..80)
                .filter(|&y| c.img[(y * 200 + 100) * 3] > 0.5)
                .count()
        })
        .collect();
    let (lo, hi) = (*widths.iter().min().unwrap(), *widths.iter().max().unwrap());
    assert!((hi - lo) as f64 <= 0.2 * hi as f64, "{widths:?}");
}

#[test]
fn supersampling_keeps_the_same_painting() {
    let a = paint(small(3), RenderOpts::default());
    let b = paint(
        small(3),
        RenderOpts {
            supersample: Some(2),
            ..RenderOpts::default()
        },
    );
    assert_eq!((a.width, a.height), (b.width, b.height));
    let mean = a
        .img
        .iter()
        .zip(&b.img)
        .map(|(x, y)| (x - y).abs() as f64)
        .sum::<f64>()
        / a.img.len() as f64;
    assert!(mean < 0.01, "{mean}");
}

#[test]
fn editing_a_stroke_leaves_the_others_jitter_alone() {
    let second = json!({"brush": "flat", "color": "#40a0e0", "points": [[20, 60], [140, 60]]});
    let short = json!({"brush": "filbert", "color": "#c8782a", "points": [[10, 20], [80, 20]]});
    let long =
        json!({"brush": "filbert", "color": "#c8782a", "points": [[10, 20], [80, 20], [150, 30]]});
    let mut tails = Vec::new();
    for first in [short, long] {
        let v = json!({"canvas": {"width": 160, "height": 80, "seed": 9}, "strokes": [first, second.clone()]});
        let laid = paint(
            v,
            RenderOpts {
                record: true,
                ..RenderOpts::default()
            },
        )
        .laid
        .unwrap();
        let tail: Vec<_> = laid
            .iter()
            .filter(|l| (l.dab.y - 60.0).abs() <= 15.0)
            .map(|l| l.dab)
            .collect();
        tails.push(tail);
    }
    assert!(!tails[0].is_empty() && tails[0] == tails[1]);
}

#[test]
fn a_petal_holds_the_scene_and_renders_the_same() {
    let v = small(3);
    let c = paint(v.clone(), RenderOpts::default());
    let petal = paintcode::scene_to_petal(&v, Some(&c.to_rgb8())).unwrap();
    let back = paintcode::petal_to_scene(&petal).unwrap();
    assert_eq!(back, v);
    assert!(paint(back, RenderOpts::default()).img == c.img);
}

/// A deliberately plain 3DGS-style rasteriser that reads only the PLY: builds each Gaussian's 3D
/// covariance from scale and rotation, projects it orthographically, sorts far to near, and
/// composites exp(-0.5 d^T S^-1 d) * sigmoid(opacity), cut off at three sigma.
fn render_splats(ply: &[u8], background: [f32; 3]) -> (usize, usize, Vec<f32>) {
    let s = paintcode::io::ply_to_splats(ply).unwrap();
    let canvas = s
        .comments
        .iter()
        .find_map(|c| c.strip_prefix("canvas "))
        .unwrap();
    let wh: Vec<usize> = canvas
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    let (w, h) = (wh[0], wh[1]);
    let unit = 2.0 / w.max(h) as f64;
    let col = |n: &str| s.column(n).unwrap();
    let n = col("x").len();
    let mut img: Vec<f32> = (0..w * h).flat_map(|_| background).collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| col("z")[b].total_cmp(&col("z")[a]));
    for i in order {
        let q = [
            col("rot_0")[i],
            col("rot_1")[i],
            col("rot_2")[i],
            col("rot_3")[i],
        ]
        .map(|v| v as f64);
        let norm = q.iter().map(|v| v * v).sum::<f64>().sqrt();
        let [qw, qx, qy, qz] = q.map(|v| v / norm);
        let r = [
            [
                1. - 2. * (qy * qy + qz * qz),
                2. * (qx * qy - qw * qz),
                2. * (qx * qz + qw * qy),
            ],
            [
                2. * (qx * qy + qw * qz),
                1. - 2. * (qx * qx + qz * qz),
                2. * (qy * qz - qw * qx),
            ],
        ];
        let sc =
            [col("scale_0")[i], col("scale_1")[i], col("scale_2")[i]].map(|v| (v as f64).exp());
        let m = |a: usize, b: usize| {
            (0..3)
                .map(|k| r[a][k] * sc[k] * r[b][k] * sc[k])
                .sum::<f64>()
                / (unit * unit)
        };
        let (a, b, d) = (m(0, 0), m(0, 1), m(1, 1));
        let det = a * d - b * b;
        if det <= 0.0 {
            continue;
        }
        let (ia, ib, id) = (d / det, -b / det, a / det);
        let (px, py) = (
            col("x")[i] as f64 / unit + w as f64 / 2.0,
            col("y")[i] as f64 / unit + h as f64 / 2.0,
        );
        let rgb = [col("f_dc_0")[i], col("f_dc_1")[i], col("f_dc_2")[i]]
            .map(|v| v as f64 * 0.28209479177387814 + 0.5);
        let alpha = 1.0 / (1.0 + (-(col("opacity")[i] as f64)).exp());
        let ext = 3.0 * a.max(d).sqrt();
        let x0 = ((px - ext).floor() as i64).max(0) as usize;
        let x1 = ((px + ext).ceil() as i64 + 1).clamp(0, w as i64) as usize;
        let y0 = ((py - ext).floor() as i64).max(0) as usize;
        let y1 = ((py + ext).ceil() as i64 + 1).clamp(0, h as i64) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let (dx, dy) = (x as f64 + 0.5 - px, y as f64 + 0.5 - py);
                let mm = ia * dx * dx + 2.0 * ib * dx * dy + id * dy * dy;
                if mm > 9.0 {
                    continue;
                }
                let al = (alpha * (-0.5 * mm).exp()).clamp(0.0, 1.0) as f32;
                for ch in 0..3 {
                    let p = &mut img[(y * w + x) * 3 + ch];
                    *p = *p * (1.0 - al) + al * rgb[ch] as f32;
                }
            }
        }
    }
    (w, h, img)
}

#[test]
fn a_soft_painting_round_trips_through_the_ply() {
    let v = json!({"canvas": {"width": 120, "height": 90, "background": "#203040", "seed": 5},
        "strokes": [
            {"brush": "round", "hardness": 0, "color": "#e0a040", "points": [[10, 20, 0.2], [60, 70, 1], [110, 30, 0.4]]},
            {"brush": "filbert", "hardness": 0, "grain": 0, "color": "#40a0e0", "points": [[10, 80], [110, 10]]},
            {"brush": "ink", "hardness": 0, "color": "#ffffff", "points": [[20, 45, 0.3], [100, 50, 1]]},
            {"brush": "blender", "color": null, "points": [[30, 30], [90, 60]]}]});
    let c = paint(
        v,
        RenderOpts {
            record: true,
            ..RenderOpts::default()
        },
    );
    let bg = rgb("#203040");
    let ply = paintcode::laid_to_ply(c.laid.as_deref().unwrap(), c.width, c.height, Some(bg));
    let (w, h, back) = render_splats(&ply, bg);
    assert_eq!((w, h), (c.width, c.height));
    let gap = back
        .iter()
        .zip(&c.img)
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    assert!(gap < 2.0 / 255.0, "{gap}");
}

// ------------------------------------------------------------ mistakes are errors, not crashes

fn error(v: Value, opts: RenderOpts) -> String {
    match Scene::from_value(&v) {
        Err(e) => e.0,
        Ok(s) => render(&s, &opts).unwrap_err().0,
    }
}

#[test]
fn a_canvas_too_large_is_an_error_not_an_abort() {
    let e = error(
        json!({"canvas": {"width": 100000, "height": 100000}, "strokes": []}),
        RenderOpts::default(),
    );
    assert!(e.contains("too large"), "{e}");
    let e = error(
        json!({"canvas": {"width": 9000, "height": 9000}, "strokes": []}),
        RenderOpts {
            supersample: Some(4),
            ..RenderOpts::default()
        },
    );
    assert!(e.contains("too large"), "{e}");
}

#[test]
fn a_point_far_off_the_canvas_is_an_error() {
    let v = json!({"canvas": {"width": 100, "height": 100},
                   "strokes": [{"brush": "ink", "points": [[10, 10], [1e8, 10]]}]});
    let e = error(v, RenderOpts::default());
    assert!(
        e.starts_with("stroke 0:") && e.contains("too far outside"),
        "{e}"
    );
    // a stroke that enters from just off the canvas is fine
    let ok = json!({"canvas": {"width": 100, "height": 100},
                    "strokes": [{"brush": "ink", "points": [[-300, 10], [50, 50]]}]});
    assert!(Scene::from_value(&ok).is_ok());
}

#[test]
fn canvas_numbers_must_be_whole() {
    for (k, v) in [
        ("width", json!(200.7)),
        ("height", json!(0)),
        ("seed", json!(1.5)),
        ("supersample", json!(0)),
    ] {
        let mut s = json!({"canvas": {"width": 100, "height": 100}, "strokes": []});
        s["canvas"][k] = v;
        let e = error(s, RenderOpts::default());
        assert!(e.contains(&format!("canvas.{k}")), "{e}");
    }
    assert!(Scene::from_value(&json!({"canvas": {"width": 100.0, "height": 50}})).is_ok());
}

#[test]
fn a_base_image_is_painted_at_its_own_size() {
    let base = RgbImage {
        width: 30,
        height: 20,
        data: vec![128; 30 * 20 * 3],
    };
    let s = scene(
        json!({"canvas": {"width": 100, "height": 100, "supersample": 2},
                         "strokes": [{"brush": "round", "points": [[5, 5], [25, 15]]}]}),
    );
    let c = render_over(&s, Some(&base), &RenderOpts::default()).unwrap();
    assert_eq!((c.width, c.height), (30, 20));
    let e = render_over(
        &s,
        Some(&base),
        &RenderOpts {
            scale: 2.0,
            ..RenderOpts::default()
        },
    )
    .unwrap_err();
    assert!(e.0.contains("base image"), "{}", e.0);
}

#[test]
fn a_bad_scale_is_an_error() {
    for scale in [0.0, -1.0, f64::NAN, 1e-6] {
        let e = error(
            small(3),
            RenderOpts {
                scale,
                ..RenderOpts::default()
            },
        );
        assert!(e.contains("scale"), "{scale}: {e}");
    }
}

#[test]
fn a_brush_larger_than_the_canvas_still_paints() {
    let v = json!({"canvas": {"width": 50, "height": 50},
                   "strokes": [{"brush": "airbrush", "size": 1e9, "points": [[25, 25], [30, 30]]}]});
    let c = paint(v, RenderOpts::default());
    assert_eq!(c.img.len(), 50 * 50 * 3);
}
