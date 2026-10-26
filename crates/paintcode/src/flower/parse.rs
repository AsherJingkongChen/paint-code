use std::collections::HashMap;

use serde_json::{Map, Value};

use super::{Brush, Error, Result, err, parse_color};

/// What a petal paints with.
#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    Rgb([f32; 3]),
    /// `null`: load the brush with whatever is under its first touch (a smudge).
    Smudge,
}

/// One petal: a stroke of any shape, with its brush already resolved and its overrides applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Petal {
    pub brush: Brush,
    pub color: Color,
    /// Rows of (x, y, pressure, tilt); missing pressure is 1, missing tilt is 0.
    pub points: Vec<[f64; 4]>,
    pub smooth: bool,
}

/// A flower: what a painter writes. A size, a background, a seed for every random number, named
/// brushes, and the petals in the order they are laid down.
#[derive(Debug, Clone, PartialEq)]
pub struct Flower {
    /// The flower as it was written, keys in their order: what a file keeps.
    pub source: Value,
    pub width: usize,
    pub height: usize,
    pub background: [f32; 3],
    pub seed: i64,
    pub petals: Vec<Petal>,
}

/// The keys a flower may have. Anything else is a mistake worth naming.
const FLOWER_KEYS: [&str; 6] = ["width", "height", "background", "seed", "brushes", "petals"];
const PETAL_KEYS: [&str; 4] = ["brush", "color", "points", "smooth"];

impl Flower {
    pub fn from_json(text: &str) -> Result<Flower> {
        let v: Value =
            serde_json::from_str(text).map_err(|e| Error(format!("flower is not JSON: {e}")))?;
        Flower::from_value(&v)
    }

    pub fn from_value(v: &Value) -> Result<Flower> {
        let root = v
            .as_object()
            .ok_or_else(|| Error("a flower is a JSON object".into()))?;
        for k in root.keys() {
            if !FLOWER_KEYS.contains(&k.as_str()) {
                return err(match k.as_str() {
                    "canvas" => "the \"canvas\" section is gone: width, height, background and \
                                 seed sit at the top of the flower, and supersample is an option \
                                 of the render"
                        .to_string(),
                    "strokes" => "\"strokes\" are called \"petals\" now".to_string(),
                    other => format!("unknown key {other:?}; a flower has {FLOWER_KEYS:?}"),
                });
            }
        }
        // whole numbers: a size of 640.7 or a seed of 1.5 is a mistake to report, not a value to
        // round quietly
        let int = |k: &str, d: i64, min: i64| -> Result<i64> {
            let Some(v) = root.get(k) else { return Ok(d) };
            let n = v
                .as_i64()
                // a seed may use all 64 bits; it is hashed as a u64
                .or_else(|| v.as_u64().filter(|_| k == "seed").map(|u| u as i64))
                .or_else(|| {
                    v.as_f64()
                        .filter(|f| f.fract() == 0.0 && f.abs() < 9e15)
                        .map(|f| f as i64)
                })
                .ok_or_else(|| Error(format!("{k} must be a whole number, got {v}")))?;
            if n < min {
                return Err(Error(format!("{k} must be at least {min}, got {v}")));
            }
            Ok(n)
        };
        let width = int("width", 800, 1)? as usize;
        let height = int("height", 600, 1)? as usize;
        let background = match root.get("background") {
            None => [1.0, 1.0, 1.0],
            Some(c) => parse_color(c)?,
        };
        let seed = int("seed", 0, i64::MIN)?;

        let mut named: HashMap<&str, Brush> = HashMap::new();
        if let Some(bs) = root.get("brushes").and_then(Value::as_object) {
            for (name, spec) in bs {
                named.insert(
                    name,
                    Brush::from_spec(spec).map_err(|e| Error(format!("brush {name}: {e}")))?,
                );
            }
        }

        let mut petals = Vec::new();
        for (idx, s) in root
            .get("petals")
            .and_then(Value::as_array)
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            petals.push(parse_petal(s, &named).map_err(|e| Error(format!("petal {idx}: {e}")))?);
        }
        Ok(Flower {
            source: v.clone(),
            width,
            height,
            background,
            seed,
            petals,
        })
    }
}

/// How far outside the canvas a control point may lie, in canvas sizes. Petals may start off the
/// canvas, but the path is walked a pixel at a time, so a point a million pixels away (a typo in
/// a coordinate) would take seconds and gigabytes before anything is painted.
pub const POINT_MARGIN: f64 = 16.0;

impl Petal {
    /// The first control point more than [`POINT_MARGIN`] canvas sizes outside a `width` x
    /// `height` canvas, if there is one.
    pub fn far_point(&self, width: usize, height: usize) -> Option<[f64; 2]> {
        let margin = POINT_MARGIN * width.max(height) as f64;
        let outside = |v: f64, side: usize| v < -margin || v > side as f64 + margin;
        self.points
            .iter()
            .find(|p| outside(p[0], width) || outside(p[1], height))
            .map(|p| [p[0], p[1]])
    }
}

fn parse_petal(s: &Value, named: &HashMap<&str, Brush>) -> Result<Petal> {
    let s = s
        .as_object()
        .ok_or_else(|| Error("a petal is a JSON object".into()))?;
    let round = Value::String("round".into());
    let r = s.get("brush").unwrap_or(&round);
    let mut brush = match r {
        Value::String(name) if named.contains_key(name.as_str()) => named[name.as_str()].clone(),
        other => Brush::from_spec(other)?,
    };
    let overrides: Map<String, Value> = s
        .iter()
        .filter(|(k, _)| !PETAL_KEYS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if !overrides.is_empty() {
        brush = brush.with_fields(&overrides)?;
    }
    let color = match s.get("color") {
        None => Color::Rgb([0.0; 3]),
        Some(Value::Null) => Color::Smudge,
        Some(c) => Color::Rgb(parse_color(c)?),
    };
    let pts = s
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| Error("petal has no points".into()))?;
    let mut points = Vec::with_capacity(pts.len());
    for pt in pts {
        let a = pt
            .as_array()
            .filter(|a| (2..=4).contains(&a.len()))
            .ok_or_else(|| {
                Error(format!(
                    "point must be [x, y], [x, y, p] or [x, y, p, tilt], got {pt}"
                ))
            })?;
        let mut row = [0.0, 0.0, 1.0, 0.0];
        for (o, v) in row.iter_mut().zip(a) {
            *o = v
                .as_f64()
                .ok_or_else(|| Error(format!("point values must be numbers, got {pt}")))?;
        }
        points.push(row);
    }
    if points.is_empty() {
        return err("petal has no points");
    }
    let smooth = match s.get("smooth") {
        None => true,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| Error(format!("smooth must be true or false, got {v}")))?,
    };
    Ok(Petal {
        brush,
        color,
        points,
        smooth,
    })
}
