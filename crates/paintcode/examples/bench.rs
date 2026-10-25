//! cargo run --release --example bench -- FLOWER.json [--square] [--threads N] [--scale S] [--ss N] [--out canvas.f32]
//!
//! Renders a flower and prints the wall time. --out writes the float canvas (h, w, 3 little-endian
//! f32) for comparing renders outside the tests.
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = &args[0];
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .map(|i| args[i + 1].clone())
    };
    let mut opts = paintcode::RenderOpts::default();
    if args.iter().any(|a| a == "--square") {
        opts.petal.spans = paintcode::Spans::Square;
    }
    if let Some(s) = flag("--scale") {
        opts.scale = s.parse().unwrap();
    }
    if let Some(n) = flag("--threads") {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n.parse().unwrap())
            .build_global()
            .unwrap();
    }
    if let Some(s) = flag("--ss") {
        opts.supersample = s.parse().unwrap();
    }
    let flower = paintcode::Flower::from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
    let t = Instant::now();
    let canvas = paintcode::flower_to_canvas(&flower, &opts).unwrap();
    let dt = t.elapsed();
    println!(
        "{path}: {}x{}  {:.0} ms  ({} threads, {:?})",
        canvas.width,
        canvas.height,
        dt.as_secs_f64() * 1e3,
        rayon::current_num_threads(),
        opts.petal.spans
    );
    if let Some(out) = flag("--out") {
        let bytes: Vec<u8> = canvas.img.iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(out, bytes).unwrap();
    }
}
