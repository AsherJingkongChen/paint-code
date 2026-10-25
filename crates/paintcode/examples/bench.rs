//! cargo run --release --example bench -- SCENE.json [--square] [--threads N] [--scale S] [--out canvas.f32]
//!
//! Renders a scene and prints the wall time. --out writes the float canvas (h, w, 3 little-endian
//! f32) for comparing against the Python engine.
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
        opts.stroke.spans = paintcode::Spans::Square;
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
    let scene = paintcode::Scene::from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
    let t = Instant::now();
    let canvas = paintcode::render(&scene, &opts).unwrap();
    let dt = t.elapsed();
    println!(
        "{path}: {}x{}  {:.0} ms  ({} threads, {:?})",
        canvas.width,
        canvas.height,
        dt.as_secs_f64() * 1e3,
        rayon::current_num_threads(),
        opts.stroke.spans
    );
    if let Some(out) = flag("--out") {
        let bytes: Vec<u8> = canvas.img.iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(out, bytes).unwrap();
    }
}
