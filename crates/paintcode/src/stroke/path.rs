//! Control points to an evenly spaced row of samples along the arc length.

use crate::scene::Brush;

/// One place a dab goes: position, pressure, tilt, direction of travel, distance travelled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub x: f64,
    pub y: f64,
    pub p: f64,
    pub tilt: f64,
    pub direction: f64,
    pub travelled: f64,
}

type Row = [f64; 4];

/// Length of a 2D offset in plain IEEE arithmetic (no `hypot`, whose rounding varies).
#[inline]
fn dist(dx: f64, dy: f64) -> f64 {
    (dx * dx + dy * dy).sqrt()
}

/// Clamp pressure and tilt, and drop consecutive duplicates (a zero-length segment has no
/// direction); a dropped point lends the kept one its larger pressure and tilt.
fn normalise(points: &[Row]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::with_capacity(points.len());
    for &[x, y, p, t] in points {
        let row = [x, y, p.clamp(0.0, 1.0), t.clamp(0.0, 1.0)];
        match out.last_mut() {
            Some(last) if dist(row[0] - last[0], row[1] - last[1]) <= 1e-6 => {
                last[2] = last[2].max(row[2]);
                last[3] = last[3].max(row[3]);
            }
            _ => out.push(row),
        }
    }
    out
}

fn lerp(a: &Row, b: &Row, f: f64) -> Row {
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
        a[3] + (b[3] - a[3]) * f,
    ]
}

/// Dense polyline (about 1 px steps) through the points: centripetal Catmull-Rom when smooth.
fn dense(a: &[Row], smooth: bool) -> Vec<Row> {
    let mut out = vec![a[0]];
    if a.len() < 3 || !smooth {
        for i in 1..a.len() {
            let seg = dist(a[i][0] - a[i - 1][0], a[i][1] - a[i - 1][1]);
            let n = (seg.ceil() as usize).max(1);
            for j in 1..=n {
                out.push(lerp(&a[i - 1], &a[i], j as f64 / n as f64));
            }
        }
        return out;
    }
    let first = [
        2.0 * a[0][0] - a[1][0],
        2.0 * a[0][1] - a[1][1],
        2.0 * a[0][2] - a[1][2],
        2.0 * a[0][3] - a[1][3],
    ];
    let m = a.len();
    let last = [
        2.0 * a[m - 1][0] - a[m - 2][0],
        2.0 * a[m - 1][1] - a[m - 2][1],
        2.0 * a[m - 1][2] - a[m - 2][2],
        2.0 * a[m - 1][3] - a[m - 2][3],
    ];
    let mut pts = Vec::with_capacity(m + 2);
    pts.push(first);
    pts.extend_from_slice(a);
    pts.push(last);
    let knot =
        |ti: f64, pa: &Row, pb: &Row| ti + dist(pb[0] - pa[0], pb[1] - pa[1]).max(1e-6).powf(0.5);
    for i in 1..pts.len() - 2 {
        let (p0, p1, p2, p3) = (&pts[i - 1], &pts[i], &pts[i + 1], &pts[i + 2]);
        let t0 = 0.0;
        let t1 = knot(t0, p0, p1);
        let t2 = knot(t1, p1, p2);
        let t3 = knot(t2, p2, p3);
        let seg = dist(p2[0] - p1[0], p2[1] - p1[1]);
        let n = (seg.ceil() as usize).max(1);
        for j in 1..=n {
            let t = t1 + (t2 - t1) * (j as f64 / n as f64);
            let mut xy = [0f64; 2];
            for d in 0..2 {
                let a1 = (t1 - t) / (t1 - t0) * p0[d] + (t - t0) / (t1 - t0) * p1[d];
                let a2 = (t2 - t) / (t2 - t1) * p1[d] + (t - t1) / (t2 - t1) * p2[d];
                let a3 = (t3 - t) / (t3 - t2) * p2[d] + (t - t2) / (t3 - t2) * p3[d];
                let b1 = (t2 - t) / (t2 - t0) * a1 + (t - t0) / (t2 - t0) * a2;
                let b2 = (t3 - t) / (t3 - t1) * a2 + (t - t1) / (t3 - t1) * a3;
                xy[d] = (t2 - t) / (t2 - t1) * b1 + (t - t1) / (t2 - t1) * b2;
            }
            let f = j as f64 / n as f64;
            out.push([
                xy[0],
                xy[1],
                p1[2] + (p2[2] - p1[2]) * f,
                p1[3] + (p2[3] - p1[3]) * f,
            ]);
        }
    }
    out
}

/// Samples at the brush's own spacing.
pub fn samples(points: &[Row], brush: &Brush, smooth: bool) -> Vec<Sample> {
    let a = normalise(points);
    let single = |r: &Row| {
        vec![Sample {
            x: r[0],
            y: r[1],
            p: r[2],
            tilt: r[3],
            direction: 0.0,
            travelled: 0.0,
        }]
    };
    if a.len() == 1 {
        return single(&a[0]);
    }
    let d = dense(&a, smooth);
    let mut segs: Vec<(Row, Row, f64)> = Vec::with_capacity(d.len());
    for w in d.windows(2) {
        let len = dist(w[1][0] - w[0][0], w[1][1] - w[0][1]);
        if len > 1e-9 {
            segs.push((w[0], w[1], len));
        }
    }
    if segs.is_empty() {
        return single(&a[0]);
    }
    let mut out = Vec::new();
    let (mut i, mut along, mut travelled) = (0usize, 0f64, 0f64);
    while i < segs.len() {
        let (s, e, len) = segs[i];
        let cur = lerp(&s, &e, along / len);
        let direction = (e[1] - s[1]).atan2(e[0] - s[0]);
        out.push(Sample {
            x: cur[0],
            y: cur[1],
            p: cur[2],
            tilt: cur[3],
            direction,
            travelled,
        });
        let step = brush.step(cur[2]);
        travelled += step;
        along += step;
        while i < segs.len() && along >= segs[i].2 {
            along -= segs[i].2;
            i += 1;
        }
    }
    out
}
