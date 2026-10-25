use std::collections::HashMap;

use serde_json::{Map, Value};

use super::{err, parse_color, Brush, Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasCfg {
    pub width: f64,
    pub height: f64,
    pub background: [f32; 3],
    pub seed: i64,
    pub supersample: i64,
}

/// What a stroke paints with.
#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    Rgb([f32; 3]),
    /// `null`: load the brush with whatever is under its first touch (a smudge).
    Smudge,
}

/// One stroke, with its brush already resolved and its overrides applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub brush: Brush,
    pub color: Color,
    /// Rows of (x, y, pressure, tilt); missing pressure is 1, missing tilt is 0.
    pub points: Vec<[f64; 4]>,
    pub smooth: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub canvas: CanvasCfg,
    pub strokes: Vec<Stroke>,
}

const STROKE_KEYS: [&str; 4] = ["brush", "color", "points", "smooth"];

impl Scene {
    pub fn from_json(text: &str) -> Result<Scene> {
        let v: Value =
            serde_json::from_str(text).map_err(|e| Error(format!("scene is not JSON: {e}")))?;
        Scene::from_value(&v)
    }

    pub fn from_value(v: &Value) -> Result<Scene> {
        let empty = Map::new();
        let root = v
            .as_object()
            .ok_or_else(|| Error("a scene is a JSON object".into()))?;
        let cfg = root
            .get("canvas")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let num = |k: &str, d: f64| -> Result<f64> {
            match cfg.get(k) {
                None => Ok(d),
                Some(v) => v
                    .as_f64()
                    .ok_or_else(|| Error(format!("canvas.{k} must be a number, got {v}"))),
            }
        };
        let canvas = CanvasCfg {
            width: num("width", 800.0)?,
            height: num("height", 600.0)?,
            background: match cfg.get("background") {
                None => [1.0, 1.0, 1.0],
                Some(c) => parse_color(c)?,
            },
            seed: num("seed", 0.0)?.trunc() as i64,
            supersample: num("supersample", 1.0)?.trunc() as i64,
        };

        let mut named: HashMap<&str, Brush> = HashMap::new();
        if let Some(bs) = root.get("brushes").and_then(Value::as_object) {
            for (name, spec) in bs {
                named.insert(
                    name,
                    Brush::from_spec(spec).map_err(|e| Error(format!("brush {name}: {e}")))?,
                );
            }
        }

        let mut strokes = Vec::new();
        for (idx, s) in root
            .get("strokes")
            .and_then(Value::as_array)
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            strokes.push(parse_stroke(s, &named).map_err(|e| Error(format!("stroke {idx}: {e}")))?);
        }
        Ok(Scene { canvas, strokes })
    }
}

fn parse_stroke(s: &Value, named: &HashMap<&str, Brush>) -> Result<Stroke> {
    let s = s
        .as_object()
        .ok_or_else(|| Error("a stroke is a JSON object".into()))?;
    let round = Value::String("round".into());
    let r = s.get("brush").unwrap_or(&round);
    let mut brush = match r {
        Value::String(name) if named.contains_key(name.as_str()) => named[name.as_str()].clone(),
        other => Brush::from_spec(other)?,
    };
    let overrides: Map<String, Value> = s
        .iter()
        .filter(|(k, _)| !STROKE_KEYS.contains(&k.as_str()))
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
        .ok_or_else(|| Error("stroke has no points".into()))?;
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
        return err("stroke has no points");
    }
    let smooth = match s.get("smooth") {
        None => true,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| Error(format!("smooth must be true or false, got {v}")))?,
    };
    Ok(Stroke {
        brush,
        color,
        points,
        smooth,
    })
}
