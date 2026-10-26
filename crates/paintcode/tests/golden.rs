//! The core against the numpy engine's frozen output (tests/golden, made by
//! tests/golden/make.py at paintcode 0.4, the last version with that engine).
//!
//! Each case checks the 8-bit PNG (no value off by more than 1/255, and at most one value in
//! 10,000 off at all: libm rounding can flip a value that sits on a half step, a real change
//! moves many), the float canvas within 1e-5 plus half a 16-bit step for the small cases, and
//! the coat of every case, written as a PLY and read back, which must lay the same canvas again bit
//! for bit. The 3DGS fields of the PLY cases are checked in src/tests.rs, which can read them.

use std::io::Read;
use std::path::{Path, PathBuf};

use paintcode::{
    Flower, RenderOpts, decode_canvas, decode_coat, encode_coat, flower_to_coat, lay_coat,
    paint_flower,
};
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
    let text = std::fs::read_to_string(root().join(case["flower"].as_str().unwrap())).unwrap();
    let flower = Flower::from_json(&text).unwrap();
    let o = &case["options"];
    let opts = RenderOpts {
        scale: o.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
        supersample: o.get("supersample").and_then(Value::as_i64).unwrap_or(1),
        ..RenderOpts::default()
    };
    let canvas = paint_flower(&flower, &opts).unwrap();
    let (w, h) = (
        case["width"].as_u64().unwrap() as usize,
        case["height"].as_u64().unwrap() as usize,
    );
    assert_eq!((canvas.width, canvas.height), (w, h), "{name}: size");

    // the PNG: the same, but for a few values one step off where 8-bit rounding sits on a tie
    let want = decode_canvas(&std::fs::read(golden(&format!("{name}.png"))).unwrap()).unwrap();
    let got = canvas.dry();
    let diffs: Vec<i16> = want
        .pixels
        .iter()
        .zip(&got.pixels)
        .map(|(a, b)| (*a as i16 - *b as i16).abs())
        .filter(|&d| d > 0)
        .collect();
    let worst = diffs.iter().copied().max().unwrap_or(0);
    let allowed = (want.pixels.len() / 10_000).max(8);
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
        for (b, v) in raw.chunks_exact(2).zip(&canvas.rgb) {
            let q = u16::from_le_bytes([b[0], b[1]]) as f32 / 65535.0;
            gap = gap.max((q - v.clamp(0.0, 1.0)).abs());
        }
        assert!(gap <= GATE, "{name}: float canvas differs by {gap:e}");
    }

    // the coat, through its file and back: the same canvas, every bit
    let coat = flower_to_coat(&flower, &opts).unwrap();
    let ply = encode_coat(&coat);
    let again = lay_coat(&decode_coat(&ply).unwrap()).unwrap();
    assert!(
        again.rgb == canvas.rgb,
        "{name}: the coat lays another canvas"
    );
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
fn soft() {
    check("soft");
}

#[test]
fn soft_supersampled() {
    // the 3DGS fields are in the pixels of the finished canvas, not the supersampled one
    check("soft_ss2");
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
