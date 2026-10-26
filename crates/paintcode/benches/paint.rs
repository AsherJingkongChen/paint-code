//! cargo bench -p paintcode [-- FILTER]
//!
//! paint: the example flowers on one thread and on every core. Throughput is in dabs, so flowers
//! of different sizes compare, and criterion compares each run with the one before it.
//!
//! again: kiku painted again by a Painting that holds it without its last petal (append), or
//! with one petal more (take back), against painting it from the start.
use std::hint::black_box;
use std::path::Path;
use std::time::{Duration, Instant};

use criterion::{
    BatchSize, BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group, criterion_main,
};
use paintcode::{Flower, Painting, RenderOpts, flower_to_coat, paint_flower};

const CASES: [(&str, &str, i64); 3] = [
    ("kiku", "examples/kiku.flower.json", 1),
    ("kiku_ss2", "examples/kiku.flower.json", 2),
    ("brush_sheet", "examples/brush_sheet.flower.json", 1),
];

/// Samples per case; a paint takes up to seconds, so each sample is one paint.
const SAMPLES: usize = 10;

fn load(file: &str) -> Flower {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(file);
    Flower::from_json(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn threads() -> Vec<usize> {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut threads = vec![1, cores];
    threads.dedup();
    threads
}

fn pool(n: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .build()
        .unwrap()
}

fn paint(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint");
    group.sampling_mode(SamplingMode::Flat).sample_size(SAMPLES);
    for (name, file, supersample) in CASES {
        let flower = load(file);
        let opts = RenderOpts {
            supersample,
            ..RenderOpts::default()
        };
        let dabs = flower_to_coat(&flower, &opts).unwrap().dabs.len();
        group.throughput(Throughput::Elements(dabs as u64));
        for n in threads() {
            let pool = pool(n);
            let once =
                || pool.install(|| black_box(paint_flower(black_box(&flower), &opts).unwrap()));
            // room for every sample and a little more, measured on this machine
            let t = Instant::now();
            once();
            group.measurement_time(
                t.elapsed()
                    .mul_f64(SAMPLES as f64 * 1.2)
                    .max(Duration::from_secs(1)),
            );
            group.bench_function(BenchmarkId::new(name, n), |b| b.iter(once));
        }
    }
    group.finish();
}

fn again(c: &mut Criterion) {
    let mut group = c.benchmark_group("again");
    group.sample_size(SAMPLES);
    let kiku = load("examples/kiku.flower.json");
    let mut less = kiku.clone();
    less.petals.pop();
    let mut more = kiku.clone();
    more.petals.push(kiku.petals[0].clone());
    for supersample in [1, 2] {
        let opts = RenderOpts {
            supersample,
            ..RenderOpts::default()
        };
        let held = |f: &Flower| {
            let mut p = Painting::new();
            p.paint_flower(f, &opts).unwrap();
            p
        };
        for (name, from) in [("append", held(&less)), ("take_back", held(&more))] {
            let name = format!("{name}_ss{supersample}");
            for n in threads() {
                let pool = pool(n);
                group.bench_function(BenchmarkId::new(&name, n), |b| {
                    b.iter_batched(
                        || from.clone(),
                        |mut p| pool.install(|| black_box(p.paint_flower(&kiku, &opts).unwrap())),
                        BatchSize::PerIteration,
                    )
                });
            }
        }
    }
    group.finish();
}

criterion_group!(benches, paint, again);
criterion_main!(benches);
