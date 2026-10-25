//! The core against the numpy engine's frozen output (tests/golden, made by
//! tests/golden/make.py at paintcode 0.4, the last version with that engine).
//!
//! Each case checks the 8-bit PNG (no value off by more than 1/255, and at most one value in
//! 10,000 off at all: libm rounding can flip a value that sits on a half step, a real change
//! moves many), the float canvas within 1e-5 plus half a 16-bit step for the small cases, and
//! every splat field of the PLY cases.

use std::io::Read;
use std::path::{Path, PathBuf};

use paintcode::io::{ply_to_splats, PLY_FIELDS};
use paintcode::{laid_to_ply, png_to_image, render_over, RenderOpts, Scene};
use serde_json::Value;

const GATE: f32 = 1e-5 + 0.5 / 65535.0;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden(name: &str) -> PathBuf {
    root().join("tests/golden").join(name)
}

fn manifest() -> serde_json::Map<String, Value> {
    let text = std::fs::read_to_string(golden("manifest.json")).unwrap();
    serde_json::from_str::<Value>(&text)
        .unwrap()
        .as_object()
        .unwrap()
        .clone()
}

fn max_gap(a: &[f32], b: &[f32], f: impl Fn(f32) -> f32) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (f(*x) - f(*y)).abs())
        .fold(0f32, f32::max)
}

fn check(name: &str) {
    let m = manifest();
    let case = &m[name];
    let scene_text = std::fs::read_to_string(root().join(case["scene"].as_str().unwrap())).unwrap();
    let scene = Scene::from_json(&scene_text).unwrap();
    let o = &case["options"];
    let opts = RenderOpts {
        scale: o.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
        supersample: o.get("supersample").and_then(Value::as_i64),
        record: o.get("ply").is_some(),
        ..RenderOpts::default()
    };
    let base = o
        .get("base")
        .map(|p| png_to_image(&std::fs::read(root().join(p.as_str().unwrap())).unwrap()).unwrap());
    let canvas = render_over(&scene, base.as_ref(), &opts).unwrap();
    let (w, h) = (
        case["width"].as_u64().unwrap() as usize,
        case["height"].as_u64().unwrap() as usize,
    );
    assert_eq!((canvas.width, canvas.height), (w, h), "{name}: size");

    // the PNG: the same, but for a few values one step off where 8-bit rounding sits on a tie
    let want = png_to_image(&std::fs::read(golden(&format!("{name}.png"))).unwrap()).unwrap();
    let got = canvas.to_rgb8();
    let diffs: Vec<i16> = want
        .data
        .iter()
        .zip(&got.data)
        .map(|(a, b)| (*a as i16 - *b as i16).abs())
        .filter(|&d| d > 0)
        .collect();
    let worst = diffs.iter().copied().max().unwrap_or(0);
    let allowed = (want.data.len() / 10_000).max(8);
    assert!(worst <= 1, "{name}: PNG differs by {worst}/255");
    assert!(
        diffs.len() <= allowed,
        "{name}: {} PNG values differ (at most {allowed} may)",
        diffs.len()
    );

    // the float canvas, where it was kept
    if let Some(f) = case.get("float").and_then(Value::as_str) {
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(std::fs::File::open(golden(f)).unwrap())
            .read_to_end(&mut raw)
            .unwrap();
        assert_eq!(raw.len(), w * h * 3 * 2, "{name}: float size");
        let mut gap = 0f32;
        for (b, v) in raw.chunks_exact(2).zip(&canvas.img) {
            let q = u16::from_le_bytes([b[0], b[1]]) as f32 / 65535.0;
            gap = gap.max((q - v.clamp(0.0, 1.0)).abs());
        }
        assert!(gap <= GATE, "{name}: float canvas differs by {gap:e}");
    }

    // the PLY: the same header, and every field as close as the numpy engine's rounding allows
    // (it rounded x, y, sx, sy to 3 decimals and theta, colour, alpha to 4 before writing; the
    // core writes them unrounded)
    if let Some(f) = case.get("ply").and_then(Value::as_str) {
        let want = ply_to_splats(&std::fs::read(golden(f)).unwrap()).unwrap();
        let laid = canvas.laid.as_deref().unwrap();
        let bg = base.is_none().then_some(scene.canvas.background);
        let got = ply_to_splats(&laid_to_ply(laid, w, h, bg)).unwrap();
        assert_eq!(got.names, want.names);
        assert_eq!(got.comments, want.comments);
        assert_eq!(
            laid.len() as u64,
            case["splats"].as_u64().unwrap(),
            "{name}: splat count"
        );
        let unit = 2.0 / w.max(h) as f32; // px -> scene units
        for field in PLY_FIELDS {
            let (a, b) = (got.column(field).unwrap(), want.column(field).unwrap());
            // opacity is a logit, steep near the clamp: compare it as alpha
            let sigmoid = |v: f32| 1.0 / (1.0 + (-v).exp());
            let (tol, gap): (f32, f32) = match field {
                "x" | "y" => (5e-4 * unit * 1.01, max_gap(a, b, |v| v)),
                // ln(sx * unit) with sx rounded to 3 decimals, and sx at least 0.5 px
                "scale_0" | "scale_1" => (1.01e-3, max_gap(a, b, |v| v)),
                "f_dc_0" | "f_dc_1" | "f_dc_2" => (5e-5 / 0.2820948 * 1.01, max_gap(a, b, |v| v)),
                "rot_0" | "rot_1" | "rot_2" | "rot_3" => (2.6e-5, max_gap(a, b, |v| v)),
                "opacity" => (5.1e-5, max_gap(a, b, sigmoid)),
                // z, the normals and the flat axis are computed the same way, unrounded
                _ => (1e-6, max_gap(a, b, |v| v)),
            };
            assert!(
                gap <= tol,
                "{name}: PLY field {field} differs by {gap} (at most {tol})"
            );
        }
    }
}

#[test]
fn features() {
    check("features");
}

#[test]
fn features_scaled() {
    check("features_scale2");
}

#[test]
fn features_supersampled() {
    check("features_ss3");
    check("features_half_ss2");
}

#[test]
fn features_on_a_base_image() {
    check("features_on_base");
}

#[test]
fn ply_export() {
    check("ply_scene");
}

#[test]
fn ply_export_supersampled() {
    // laid dabs come back to output pixel units when the canvas is box-filtered down
    check("ply_scene_ss2");
}

#[test]
fn ply_export_on_a_base_image() {
    check("ply_on_base");
}

#[test]
fn brush_sheet() {
    check("brush_sheet");
}

#[test]
#[ignore = "slow without --release; CI runs it with --include-ignored"]
fn kiku() {
    check("kiku");
}

#[test]
fn the_petal_made_by_the_numpy_engine_still_opens() {
    let petal = std::fs::read(root().join("examples/kiku.petal")).unwrap();
    let scene = paintcode::petal_to_scene(&petal).unwrap();
    let json = std::fs::read_to_string(root().join("examples/kiku.json")).unwrap();
    assert_eq!(
        Scene::from_value(&scene).unwrap(),
        Scene::from_json(&json).unwrap()
    );
}
