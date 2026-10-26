//! What the engine promises, one behaviour per test (ported from the numpy engine's
//! tests/test_engine.py when that engine retired).

use paintcode::flower::{Brush, Color, brush};
use paintcode::petal::{Options, paint_petal, samples};
use paintcode::render::{Coat, Dab, Spans, WetCanvas};
use paintcode::{Flower, RenderOpts, flower_to_coat, lay_coat, paint_flower};
use serde_json::{Value, json};

fn flower(v: Value) -> Flower {
    Flower::from_value(&v).unwrap()
}

fn small(seed: i64) -> Value {
    json!({"width": 160, "height": 80, "seed": seed, "petals": [{"brush": "filbert", "color": "#c8782a",
                        "points": [[10, 40, 0.2], [60, 20, 1.0], [110, 60, 0.7], [150, 40, 0.1]]}]})
}

fn paint(v: Value, opts: RenderOpts) -> WetCanvas {
    paint_flower(&flower(v), &opts).unwrap()
}

fn coat(v: Value, opts: RenderOpts) -> Coat {
    flower_to_coat(&flower(v), &opts).unwrap()
}

fn rgb(hex: &str) -> [f32; 3] {
    paintcode::flower::parse_color(&json!(hex)).unwrap()
}

fn row(points: &[[f64; 2]]) -> Vec<[f64; 4]> {
    points.iter().map(|p| [p[0], p[1], 1.0, 0.0]).collect()
}

fn petal(c: &mut WetCanvas, b: &Brush, color: Color, pts: &[[f64; 4]]) -> paintcode::Painted {
    paint_petal(c, 0, b, &color, pts, true, Options::default())
}

fn blank(w: usize, h: usize, bg: &str) -> WetCanvas {
    WetCanvas::new(w, h, rgb(bg), 0).unwrap()
}

#[test]
fn same_flower_same_pixels() {
    let o = RenderOpts::default();
    assert!(paint(small(3), o).rgb == paint(small(3), o).rgb);
}

#[test]
fn seed_changes_jitter() {
    let o = RenderOpts::default();
    assert!(paint(small(3), o).rgb != paint(small(4), o).rgb);
}

#[test]
fn duplicate_points_do_not_change_the_petal() {
    let b = brush("ink").unwrap();
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
    petal(&mut c1, &b, Color::Rgb([0.0; 3]), &pts);
    petal(&mut c2, &b, Color::Rgb([0.0; 3]), &dup);
    let gap = c1
        .rgb
        .iter()
        .zip(&c2.rgb)
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
    assert!(
        s.iter()
            .all(|s| (s.direction - std::f64::consts::FRAC_PI_4).abs() < 1e-6)
    );
}

#[test]
fn pressure_grows_radius_and_flow() {
    let b = brush("round").unwrap();
    let ps: Vec<f64> = (0..=10).map(|i| i as f64 / 10.0).collect();
    let r: Vec<f64> = ps.iter().map(|&p| b.radius(p)).collect();
    let f: Vec<f64> = ps.iter().map(|&p| b.flow_at(p)).collect();
    assert!(r.windows(2).all(|w| w[0] < w[1]));
    assert!(f.windows(2).all(|w| w[0] < w[1]));
    assert!((r[0] - b.size * b.min_size).abs() < 1e-12);
}

#[test]
fn a_single_point_is_one_dab() {
    let v = json!({"width": 40, "height": 40, "petals": [{"brush": "round", "color": "#f00", "points": [[20, 20, 0.8]]}]});
    assert_eq!(coat(v, RenderOpts::default()).dabs.len(), 1);
}

#[test]
fn pickup_does_not_depend_on_dab_spacing() {
    let end_colour = |spacing: f64| {
        let mut c = blank(300, 40, "#ffffff");
        // wet black paint already on the right half
        for y in 0..40 {
            for x in 150..300 {
                c.rgb[(y * 300 + x) * 3..(y * 300 + x) * 3 + 3].fill(0.0);
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
        petal(
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
        rgb: [1.0; 3],
    };
    let (fp, _) = c.footprint(&d, Spans::Ellipse, None);
    c.composite(&[(&fp, d.rgb)]);
    // one sigma from the centre (pixel centres at x + 0.5) the value is exp(-0.5)
    let v = c.rgb[(31 * 64 + 32 + 6 - 1) * 3] as f64;
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
    let carried = petal(
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
            c.rgb[(y * 100 + x) * 3..(y * 100 + x) * 3 + 3].copy_from_slice(&[1.0, 0.0, 0.0]);
            c.wet[y * 100 + x] = 1.0;
        }
    }
    petal(
        &mut c,
        &brush("blender").unwrap(),
        Color::Smudge,
        &row(&[[20., 20.], [80., 20.]]),
    );
    let px = &c.rgb[(20 * 100 + 60) * 3..(20 * 100 + 60) * 3 + 3];
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
        petal(
            &mut c,
            &b,
            Color::Rgb([1.0; 3]),
            &row(&[[20., 30.], [180., 30.]]),
        );
        let col: Vec<f32> = (0..30).map(|y| c.rgb[(y * 200 + 100) * 3]).collect();
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
            petal(
                &mut c,
                &b,
                Color::Rgb([1.0; 3]),
                &row(&[[20., 40.], [180., 40.]]),
            );
            (0..80)
                .filter(|&y| c.rgb[(y * 200 + 100) * 3] > 0.5)
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
            supersample: 2,
            ..RenderOpts::default()
        },
    );
    assert_eq!((a.width, a.height), (b.width, b.height));
    let mean = a
        .rgb
        .iter()
        .zip(&b.rgb)
        .map(|(x, y)| (x - y).abs() as f64)
        .sum::<f64>()
        / a.rgb.len() as f64;
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
        let v = json!({"width": 160, "height": 80, "seed": 9, "petals": [first, second.clone()]});
        let dabs = coat(v, RenderOpts::default()).dabs;
        let tail: Vec<_> = dabs
            .into_iter()
            .filter(|d| (d.y - 60.0).abs() <= 15.0)
            .collect();
        tails.push(tail);
    }
    assert!(!tails[0].is_empty() && tails[0] == tails[1]);
}

/// Wet paint, a smudge, paper grain and a hard edge: everything a dab can carry.
fn busy() -> Value {
    json!({"width": 90, "height": 60, "background": "#f4ecd8", "seed": 11, "petals": [
        {"brush": "filbert", "color": "#c8782a", "points": [[5, 30, 0.3], [45, 10], [85, 40, 0.6]]},
        {"brush": "flat", "hardness": 0.8, "grain": 0.6, "color": "#2050a0", "points": [[10, 50], [80, 20]]},
        {"brush": "blender", "color": null, "points": [[20, 20], [70, 45]]},
        {"brush": "ink", "color": "#101010", "points": [[45, 5, 0.2], [45, 55, 1]]}]})
}

#[test]
fn a_coat_lays_the_same_canvas_again() {
    for (scale, supersample) in [(1.0, 1), (1.0, 2), (1.5, 3)] {
        let opts = RenderOpts {
            scale,
            supersample,
            ..RenderOpts::default()
        };
        let c = paint(busy(), opts);
        let back = lay_coat(&coat(busy(), opts)).unwrap();
        assert_eq!((back.width, back.height), (c.width, c.height));
        assert!(
            back.rgb == c.rgb,
            "scale {scale}, supersample {supersample}"
        );
    }
}

#[test]
fn a_coat_survives_its_ply() {
    let opts = RenderOpts {
        supersample: 2,
        ..RenderOpts::default()
    };
    let c = coat(busy(), opts);
    let back = paintcode::decode_coat(&paintcode::encode_coat(&c)).unwrap();
    assert_eq!(back, c);
    assert!(lay_coat(&back).unwrap().rgb == paint(busy(), opts).rgb);
}

#[test]
fn a_flower_rides_on_its_canvas() {
    let canvas = paint(busy(), RenderOpts::default()).dry();
    let on = paintcode::flower_on_canvas(&flower(busy()), canvas.clone());
    let png = paintcode::encode_canvas(&on);
    assert_eq!(paintcode::decode_flower(&png).unwrap(), flower(busy()));
    assert_eq!(paintcode::decode_canvas(&png).unwrap(), on);
    assert_eq!(paintcode::flower_off_canvas(&on).unwrap(), flower(busy()));
    let e = paintcode::flower_off_canvas(&canvas).unwrap_err().0;
    assert_eq!(e, "no flower on this canvas");
}

// ------------------------------------------------------------ mistakes are errors, not crashes

fn error(v: Value, opts: RenderOpts) -> String {
    match Flower::from_value(&v) {
        Err(e) => e.0,
        Ok(s) => paint_flower(&s, &opts).unwrap_err().0,
    }
}

#[test]
fn a_canvas_too_large_is_an_error_not_an_abort() {
    let e = error(
        json!({"width": 100000, "height": 100000, "petals": []}),
        RenderOpts::default(),
    );
    assert!(e.contains("too large"), "{e}");
    let e = error(
        json!({"width": 9000, "height": 9000, "petals": []}),
        RenderOpts {
            supersample: 4,
            ..RenderOpts::default()
        },
    );
    assert!(e.contains("too large"), "{e}");
}

#[test]
fn a_point_far_off_the_canvas_is_an_error() {
    let v = json!({"width": 100, "height": 100, "petals": [{"brush": "ink", "points": [[10, 10], [1e8, 10]]}]});
    let e = error(v, RenderOpts::default());
    assert!(
        e.starts_with("petal 0:") && e.contains("too far outside"),
        "{e}"
    );
    // a petal that enters from just off the canvas is fine
    let ok = json!({"width": 100, "height": 100, "petals": [{"brush": "ink", "points": [[-300, 10], [50, 50]]}]});
    assert!(Flower::from_value(&ok).is_ok());
}

#[test]
fn canvas_numbers_must_be_whole() {
    for (k, v) in [
        ("width", json!(200.7)),
        ("height", json!(0)),
        ("seed", json!(1.5)),
    ] {
        let mut s = json!({"width": 100, "height": 100, "petals": []});
        s[k] = v;
        let e = error(s, RenderOpts::default());
        assert!(e.starts_with(k), "{e}");
    }
    assert!(Flower::from_value(&json!({"width": 100.0, "height": 50})).is_ok());
    let e = paint_flower(
        &flower(json!({"petals": []})),
        &RenderOpts {
            supersample: 0,
            ..RenderOpts::default()
        },
    );
    assert!(e.unwrap_err().0.contains("supersample"));
}

#[test]
fn the_old_names_are_explained() {
    let e = |v: Value| Flower::from_value(&v).unwrap_err().0;
    assert!(e(json!({"canvas": {"width": 10}})).contains("top of the flower"));
    assert!(e(json!({"strokes": []})).contains("petals"));
    assert!(e(json!({"colour": "#fff"})).contains("unknown key"));
    let old = json!({"brushes": {"soft": {"preset": "filbert"}}});
    assert!(e(old).contains("\"preset\" is called \"brush\" now"));
}

#[test]
fn far_points_are_measured_against_the_flower() {
    let v = json!({"width": 10, "height": 10, "petals": [{"brush": "round", "points": [[20, 20], [300, 200]]}]});
    let e = paint_flower(&flower(v), &RenderOpts::default())
        .unwrap_err()
        .0;
    assert!(e.contains("too far outside the 10x10 canvas"), "{e}");
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
    let v = json!({"width": 50, "height": 50, "petals": [{"brush": "airbrush", "size": 1e9, "points": [[25, 25], [30, 30]]}]});
    let c = paint(v, RenderOpts::default());
    assert_eq!(c.rgb.len(), 50 * 50 * 3);
}

#[test]
fn a_seed_may_use_all_64_bits() {
    let at = |seed: Value| {
        let mut v = small(0);
        v["seed"] = seed;
        paint(v, RenderOpts::default()).rgb
    };
    // 2^63 and -2^63 are the same 64 bits, so the same jitter
    assert!(at(json!(9223372036854775808u64)) == at(json!(i64::MIN)));
}

#[test]
fn a_huge_grain_scale_paints_without_a_huge_paper() {
    let v = json!({"width": 50, "height": 50, "petals": [{"brush": "pencil", "grain_scale": 1e7, "points": [[5, 5], [40, 40]]}]});
    let c = paint(v, RenderOpts::default());
    assert_eq!(c.rgb.len(), 50 * 50 * 3);
}

/// A petal that jitters every way it can, at a height of its own so no petal picks up another.
fn jittery(y: f64, extra: Value) -> Value {
    let mut p = json!({"brush": "round", "color": "#336699", "size": 6, "size_jitter": 0.4,
                       "angle_jitter": 20, "pos_jitter": 0.3, "flow_jitter": 0.3,
                       "points": [[10, y], [150, y]]});
    p.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    p
}

/// Where the last petal's dabs went and how they were shaped, from a flower of `petals`.
fn last_petal(petals: Vec<Value>) -> Vec<[f64; 6]> {
    let all = |ps: Vec<Value>| -> Vec<[f64; 6]> {
        coat(
            json!({"width": 160, "height": 80, "seed": 3, "petals": ps}),
            RenderOpts::default(),
        )
        .dabs
        .iter()
        .map(|d| [d.x, d.y, d.sx, d.sy, d.theta, d.alpha])
        .collect()
    };
    let before = all(petals[..petals.len() - 1].to_vec()).len();
    all(petals)[before..].to_vec()
}

#[test]
fn an_id_keeps_its_jitter_when_other_petals_come_and_go() {
    let named = jittery(60.0, json!({"id": "tail"}));
    let alone = last_petal(vec![named.clone()]);
    assert_eq!(
        last_petal(vec![jittery(10.0, json!({})), named.clone()]),
        alone
    );
    assert_eq!(
        last_petal(vec![
            jittery(10.0, json!({})),
            jittery(30.0, json!({})),
            named
        ]),
        alone
    );
    // without the id, its index keys it, and the index moved
    let plain = jittery(60.0, json!({}));
    assert_ne!(
        last_petal(vec![jittery(10.0, json!({})), plain.clone()]),
        last_petal(vec![plain])
    );
}

#[test]
fn a_group_keeps_its_jitter_unless_its_own_group_changes() {
    let sky = |y: f64| jittery(y, json!({"group": "sky"}));
    let stem = jittery(40.0, json!({"group": "stem"}));
    let whole = last_petal(vec![sky(10.0), stem.clone(), sky(70.0)]);
    // another group's petal taken out: the same
    assert_eq!(last_petal(vec![sky(10.0), sky(70.0)]), whole);
    // a petal with an id does not count among the group's places
    let named = jittery(25.0, json!({"group": "sky", "id": "moon"}));
    assert_eq!(last_petal(vec![sky(10.0), named, sky(70.0)]), whole);
    // a petal of its own group taken out: it is first in the group now
    assert_ne!(last_petal(vec![stem, sky(70.0)]), whole);
}

#[test]
fn ids_and_groups_are_names() {
    let bad = |extra: Value| {
        let f = json!({"petals": [jittery(10.0, json!({"id": "a"})), jittery(20.0, extra)]});
        Flower::from_value(&f).unwrap_err().0
    };
    assert_eq!(
        bad(json!({"id": "a"})),
        "petal 1: id \"a\" is petal 0's already"
    );
    assert!(bad(json!({"id": 3})).contains("id must be a non-empty string"));
    assert!(bad(json!({"group": ""})).contains("group must be a non-empty string"));
}
