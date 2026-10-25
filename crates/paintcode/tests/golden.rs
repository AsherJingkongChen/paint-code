//! The core against the numpy engine's frozen output (tests/golden, made by
//! tests/golden/make.py at paintcode 0.4, the last version with that engine).
//!
//! Each case checks the 8-bit PNG within 1/255 and, for the small cases, the float canvas
//! within 1e-5 plus half a 16-bit step. The PLY case checks every splat field.

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

    // the PNG, within one step of 8-bit rounding
    let want = png_to_image(&std::fs::read(golden(&format!("{name}.png"))).unwrap()).unwrap();
    let got = canvas.to_rgb8();
    let worst = want
        .data
        .iter()
        .zip(&got.data)
        .map(|(a, b)| (*a as i16 - *b as i16).abs())
        .max()
        .unwrap();
    assert!(worst <= 1, "{name}: PNG differs by {worst}/255");

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

    // the PLY: same header, every field close (the numpy engine rounded dabs to 3-4 decimals
    // before writing them, the core does not)
    if let Some(f) = case.get("ply").and_then(Value::as_str) {
        let want = ply_to_splats(&std::fs::read(golden(f)).unwrap()).unwrap();
        let laid = canvas.laid.as_deref().unwrap();
        let bg = Some([
            0x20 as f32 / 255.0,
            0x30 as f32 / 255.0,
            0x40 as f32 / 255.0,
        ]);
        let got = ply_to_splats(&laid_to_ply(laid, w, h, bg)).unwrap();
        assert_eq!(got.names, want.names);
        assert_eq!(got.comments, want.comments);
        assert_eq!(
            laid.len() as u64,
            case["splats"].as_u64().unwrap(),
            "{name}: splat count"
        );
        for field in PLY_FIELDS {
            let (a, b) = (got.column(field).unwrap(), want.column(field).unwrap());
            let worst = a
                .iter()
                .zip(b)
                .map(|(x, y)| (x - y).abs())
                .fold(0f32, f32::max);
            assert!(worst < 2e-3, "{name}: PLY field {field} differs by {worst}");
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
