//! The fast path must paint the same bits as the reference path: whole 3-sigma squares, one
//! thread. Any speed-up that changes a pixel fails here.

use paintcode::{Flower, RenderOpts, Spans, paint_flower};

fn flower(name: &str) -> Flower {
    let path = format!("{}/../../examples/{name}", env!("CARGO_MANIFEST_DIR"));
    Flower::from_json(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn reference(s: &Flower) -> Vec<f32> {
    let mut o = RenderOpts::default();
    o.petal.spans = Spans::Square;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    pool.install(|| paint_flower(s, &o).unwrap().rgb)
}

#[test]
fn fast_path_is_bit_identical_on_the_brush_sheet() {
    let s = flower("brush_sheet.flower.json");
    let fast = paint_flower(&s, &RenderOpts::default()).unwrap().rgb;
    assert!(fast == reference(&s), "the fast path changed pixels");
}

#[test]
fn chunking_and_snapshots_do_not_change_pixels() {
    // a tiny budget forces every petal into many chunks, and pickup petals onto a snapshot
    let s = flower("brush_sheet.flower.json");
    let mut o = RenderOpts::default();
    o.petal.budget = 2_000;
    assert!(paint_flower(&s, &o).unwrap().rgb == reference(&s));
}

#[test]
fn errors_name_the_petal() {
    let bad = r##"{"width": 10, "height": 10, "petals": [{"brush": "ink", "points": [[1, 1]]}, {"brush": "ink", "sise": 3, "points": [[1, 1]]}]}"##;
    let e = Flower::from_json(bad).unwrap_err();
    assert!(
        e.0.starts_with("petal 1:") && e.0.contains("sise"),
        "{}",
        e.0
    );
}

#[test]
fn png_round_trip_of_a_render() {
    let s = flower("brush_sheet.flower.json");
    let img = paint_flower(&s, &RenderOpts::default()).unwrap().dry();
    assert_eq!(
        paintcode::decode_canvas(&paintcode::encode_canvas(&img)).unwrap(),
        img
    );
}
