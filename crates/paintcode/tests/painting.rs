//! A Painting paints the same canvas, bit for bit, as painting the flower from the start, however
//! the flower changed since the last paint. Whether it reused anything is the bench's to show.

use std::path::Path;

use paintcode::{Flower, Painting, RenderOpts, WetCanvas, paint_flower};

fn flower(file: &str) -> Flower {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(file);
    Flower::from_json(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Every wet, smudging, grainy and pickup brush, in eight petals.
fn features() -> Flower {
    flower("tests/golden/features.flower.json")
}

fn first(f: &Flower, n: usize) -> Flower {
    let mut f = f.clone();
    f.petals.truncate(n);
    f
}

fn opts(supersample: i64) -> RenderOpts {
    RenderOpts {
        supersample,
        ..RenderOpts::default()
    }
}

#[track_caller]
fn same(painting: &mut Painting, f: &Flower, o: &RenderOpts) {
    let got: WetCanvas = painting.paint_flower(f, o).unwrap();
    let want = paint_flower(f, o).unwrap();
    assert_eq!((got.width, got.height), (want.width, want.height));
    assert!(got.planes == want.planes, "the colour differs");
    assert!(got.wet == want.wet, "the wetness differs");
}

#[test]
fn appending_petals_one_at_a_time() {
    let f = features();
    for ss in [1, 3] {
        let mut p = Painting::new();
        for n in 0..=f.petals.len() {
            same(&mut p, &first(&f, n), &opts(ss));
        }
    }
}

#[test]
fn taking_petals_back_and_putting_them_again() {
    let f = features();
    let mut p = Painting::new();
    for n in [8, 7, 8, 5, 8, 3, 6, 6, 0, 8] {
        same(&mut p, &first(&f, n), &opts(2));
    }
}

#[test]
fn changing_an_early_petal() {
    let f = features();
    let mut p = Painting::new();
    same(&mut p, &f, &opts(1));
    let mut g = f.clone();
    g.petals[1].points[0][0] += 3.0;
    same(&mut p, &g, &opts(1));
    same(&mut p, &f, &opts(1));
}

#[test]
fn another_seed_background_scale_or_supersample() {
    let f = features();
    let mut p = Painting::new();
    same(&mut p, &f, &opts(1));
    let mut g = f.clone();
    g.seed += 1;
    same(&mut p, &g, &opts(1));
    let mut g = f.clone();
    g.background = [0.9, 0.1, 0.1];
    same(&mut p, &g, &opts(1));
    same(&mut p, &f, &opts(2));
    let scaled = RenderOpts {
        scale: 1.5,
        ..opts(1)
    };
    same(&mut p, &f, &scaled);
    same(&mut p, &f, &opts(1));
}

#[test]
fn a_bad_flower_leaves_the_painting_as_it_was() {
    let f = features();
    let mut p = Painting::new();
    same(&mut p, &f, &opts(1));
    let mut g = f.clone();
    g.petals[7].points[0] = [1e9, 0.0, 1.0, 0.0];
    assert!(p.paint_flower(&g, &opts(1)).is_err());
    assert!(p.paint_flower(&f, &opts(0)).is_err());
    same(&mut p, &f, &opts(1));
}
