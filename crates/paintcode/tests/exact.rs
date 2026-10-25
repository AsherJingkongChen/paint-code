//! The fast path must paint the same bits as the reference path: whole 3-sigma squares, `powf`
//! on every pixel, one thread. Any speed-up that changes a pixel fails here.

use paintcode::{RenderOpts, Scene, Spans, render};

fn scene(name: &str) -> Scene {
    let path = format!("{}/../../examples/{name}", env!("CARGO_MANIFEST_DIR"));
    Scene::from_json(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn reference(s: &Scene) -> Vec<f32> {
    let mut o = RenderOpts::default();
    o.stroke.spans = Spans::Square;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    pool.install(|| render(s, &o).unwrap().img)
}

#[test]
fn fast_path_is_bit_identical_on_the_brush_sheet() {
    let s = scene("brush_sheet.json");
    let fast = render(&s, &RenderOpts::default()).unwrap().img;
    assert!(fast == reference(&s), "the fast path changed pixels");
}

#[test]
fn chunking_and_snapshots_do_not_change_pixels() {
    // a tiny budget forces every stroke into many chunks, and pickup strokes onto a snapshot
    let s = scene("brush_sheet.json");
    let mut o = RenderOpts::default();
    o.stroke.budget = 2_000;
    assert!(render(&s, &o).unwrap().img == reference(&s));
}

#[test]
fn errors_name_the_stroke() {
    let bad = r##"{"canvas": {"width": 10, "height": 10}, "strokes": [{"brush": "ink", "points": [[1, 1]]}, {"brush": "ink", "sise": 3, "points": [[1, 1]]}]}"##;
    let e = Scene::from_json(bad).unwrap_err();
    assert!(
        e.0.starts_with("stroke 1:") && e.0.contains("sise"),
        "{}",
        e.0
    );
}

#[test]
fn png_round_trip_of_a_render() {
    let s = scene("brush_sheet.json");
    let img = render(&s, &RenderOpts::default()).unwrap().to_rgb8();
    assert_eq!(
        paintcode::png_to_image(&paintcode::image_to_png(&img)).unwrap(),
        img
    );
}
