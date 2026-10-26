//! The 3DGS half of a coat. The engine never reads those 17 fields back (decode_coat reads the
//! exact `pc_*` fields), so nothing in it would break if they were wrong; only a splat viewer
//! would show it. This test is that viewer: it reads them with io's own PLY reader, which is why
//! it lives inside the crate. tests/golden.rs keeps the bytes from drifting.

use std::path::{Path, PathBuf};

use crate::io::{self, decode_ply};
use crate::{Flower, RenderOpts, flower_to_coat, paint_flower};

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden")
        .join(name)
}

/// A deliberately plain 3DGS-style rasteriser that reads only the PLY: builds each Gaussian's 3D
/// covariance from scale and rotation, projects it orthographically, sorts far to near, and
/// composites exp(-0.5 d^T S^-1 d) * sigmoid(opacity), cut off at three sigma.
fn render_splats(ply: &[u8], background: [f32; 3]) -> (usize, usize, Vec<f32>) {
    let s = decode_ply(ply).unwrap();
    let canvas = s
        .comments
        .iter()
        .find_map(|c| c.strip_prefix("canvas "))
        .unwrap();
    let wh: Vec<usize> = canvas
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    let (w, h) = (wh[0], wh[1]);
    let unit = 2.0 / w.max(h) as f64;
    let col = |n: &str| s.column(n).unwrap();
    let n = col("x").len();
    let mut img: Vec<f32> = (0..w * h).flat_map(|_| background).collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| col("z")[b].total_cmp(&col("z")[a]));
    for i in order {
        let q = [
            col("rot_0")[i],
            col("rot_1")[i],
            col("rot_2")[i],
            col("rot_3")[i],
        ]
        .map(|v| v);
        let norm = q.iter().map(|v| v * v).sum::<f64>().sqrt();
        let [qw, qx, qy, qz] = q.map(|v| v / norm);
        let r = [
            [
                1. - 2. * (qy * qy + qz * qz),
                2. * (qx * qy - qw * qz),
                2. * (qx * qz + qw * qy),
            ],
            [
                2. * (qx * qy + qw * qz),
                1. - 2. * (qx * qx + qz * qz),
                2. * (qy * qz - qw * qx),
            ],
        ];
        let sc = [col("scale_0")[i], col("scale_1")[i], col("scale_2")[i]].map(|v| v.exp());
        let m = |a: usize, b: usize| {
            (0..3)
                .map(|k| r[a][k] * sc[k] * r[b][k] * sc[k])
                .sum::<f64>()
                / (unit * unit)
        };
        let (a, b, d) = (m(0, 0), m(0, 1), m(1, 1));
        let det = a * d - b * b;
        if det <= 0.0 {
            continue;
        }
        let (ia, ib, id) = (d / det, -b / det, a / det);
        let (px, py) = (
            col("x")[i] / unit + w as f64 / 2.0,
            col("y")[i] / unit + h as f64 / 2.0,
        );
        let rgb = [col("f_dc_0")[i], col("f_dc_1")[i], col("f_dc_2")[i]]
            .map(|v| v * 0.28209479177387814 + 0.5);
        let alpha = 1.0 / (1.0 + (-col("opacity")[i]).exp());
        let ext = 3.0 * a.max(d).sqrt();
        let x0 = ((px - ext).floor() as i64).max(0) as usize;
        let x1 = ((px + ext).ceil() as i64 + 1).clamp(0, w as i64) as usize;
        let y0 = ((py - ext).floor() as i64).max(0) as usize;
        let y1 = ((py + ext).ceil() as i64 + 1).clamp(0, h as i64) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let (dx, dy) = (x as f64 + 0.5 - px, y as f64 + 0.5 - py);
                let mm = ia * dx * dx + 2.0 * ib * dx * dy + id * dy * dy;
                if mm > 9.0 {
                    continue;
                }
                let al = (alpha * (-0.5 * mm).exp()).clamp(0.0, 1.0) as f32;
                for ch in 0..3 {
                    let p = &mut img[(y * w + x) * 3 + ch];
                    *p = *p * (1.0 - al) + al * rgb[ch] as f32;
                }
            }
        }
    }
    (w, h, img)
}

#[test]
fn a_soft_painting_round_trips_through_the_ply() {
    let text = std::fs::read_to_string(golden("soft.flower.json")).unwrap();
    let flower = Flower::from_json(&text).unwrap();
    let c = paint_flower(&flower, &RenderOpts::default()).unwrap();
    let bg = flower.background;
    let ply = io::encode_coat(&flower_to_coat(&flower, &RenderOpts::default()).unwrap());
    let (w, h, back) = render_splats(&ply, bg);
    assert_eq!((w, h), (c.width, c.height));
    let gap = back
        .iter()
        .zip(&c.rgb())
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    assert!(gap < 2.0 / 255.0, "{gap}");
}
