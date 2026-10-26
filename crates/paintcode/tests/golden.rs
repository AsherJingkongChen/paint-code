//! Snapshots of what the core paints, in tests/golden. Every case writes its files again and git
//! shows what moved; CI fails when a run changes them (`git diff --exit-code -- tests/golden`),
//! so a change that moves pixels commits its snapshots with it. examples/diff.rs is the diff
//! driver that lets git say how far an image moved (see the README).
//!
//! Each case writes the 8-bit PNG, the float canvas quantised to 16 bits for the small cases and
//! the coat where the case keeps it, and checks that its coat, written as a PLY and read back,
//! lays the same canvas again bit for bit.
//!
//! Up to 0.6 these files were what the numpy engine of 0.4 painted (tests/golden/make.py, whose
//! last version is at 02f2aea); 0.7 painted them anew.

use std::io::Write;
use std::path::{Path, PathBuf};

use paintcode::{
    Flower, RenderOpts, decode_coat, encode_canvas, encode_coat, flower_to_coat, lay_coat,
    paint_flower,
};
use serde_json::Value;

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

    // the coat, through its file and back: the same canvas, every bit
    let coat = flower_to_coat(&flower, &opts).unwrap();
    let ply = encode_coat(&coat);
    let again = lay_coat(&decode_coat(&ply).unwrap()).unwrap();
    assert!(
        again.rgb == canvas.rgb,
        "{name}: the coat lays another canvas"
    );

    std::fs::write(golden(&format!("{name}.png")), encode_canvas(&canvas.dry())).unwrap();
    if let Some(f) = case.get("float").and_then(Value::as_str) {
        let q: Vec<u8> = canvas
            .rgb
            .iter()
            .flat_map(|v| ((v.clamp(0.0, 1.0) * 65535.0).round() as u16).to_le_bytes())
            .collect();
        let mut gz = flate2::GzBuilder::new()
            .mtime(0)
            .write(Vec::new(), flate2::Compression::best());
        gz.write_all(&q).unwrap();
        std::fs::write(golden(f), gz.finish().unwrap()).unwrap();
    }
    if let Some(f) = case.get("coat").and_then(Value::as_str) {
        std::fs::write(golden(f), &ply).unwrap();
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
fn soft() {
    check("soft");
}

#[test]
fn soft_supersampled() {
    // the 3DGS fields are in the pixels of the finished canvas, not the supersampled one, as
    // they were at 0.4
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
