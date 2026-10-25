use crate::math::gauss;
use serde_json::{Map, Value};

use super::{err, Error, Result};

/// How one control point (x, y, pressure, tilt) becomes one dab. The fields are the README's
/// "Brush fields" table.
#[derive(Debug, Clone, PartialEq)]
pub struct Brush {
    pub size: f64,
    pub aspect: f64,
    pub angle: Option<f64>,
    pub hardness: f64,
    pub flow: f64,
    pub spacing: f64,
    pub min_size: f64,
    pub size_gamma: f64,
    pub min_flow: f64,
    pub flow_gamma: f64,
    pub tilt_stretch: f64,
    pub size_jitter: f64,
    pub angle_jitter: f64,
    pub pos_jitter: f64,
    pub flow_jitter: f64,
    pub load: f64,
    pub half_life: Option<f64>,
    pub min_load: f64,
    pub pickup: f64,
    pub grain: f64,
    pub grain_scale: f64,
}

impl Default for Brush {
    fn default() -> Self {
        Brush {
            size: 8.0,
            aspect: 1.0,
            angle: None,
            hardness: 0.0,
            flow: 0.6,
            spacing: 0.15,
            min_size: 0.25,
            size_gamma: 1.0,
            min_flow: 0.15,
            flow_gamma: 1.0,
            tilt_stretch: 0.8,
            size_jitter: 0.0,
            angle_jitter: 0.0,
            pos_jitter: 0.0,
            flow_jitter: 0.0,
            load: 1.0,
            half_life: None,
            min_load: 0.15,
            pickup: 0.0,
            grain: 0.0,
            grain_scale: 3.0,
        }
    }
}

pub const PRESET_NAMES: [&str; 9] = [
    "ink", "pencil", "nib", "round", "filbert", "flat", "airbrush", "dry", "blender",
];

pub fn preset(name: &str) -> Option<Brush> {
    let d = Brush::default();
    Some(match name {
        "ink" => Brush {
            size: 2.0,
            hardness: 0.6,
            flow: 0.9,
            min_size: 0.15,
            spacing: 0.12,
            ..d
        },
        "pencil" => Brush {
            size: 1.0,
            hardness: 0.7,
            flow: 0.35,
            min_size: 0.5,
            min_flow: 0.05,
            grain: 0.75,
            grain_scale: 1.6,
            spacing: 0.2,
            ..d
        },
        "nib" => Brush {
            size: 4.0,
            aspect: 0.18,
            angle: Some(40.0),
            hardness: 0.8,
            flow: 0.95,
            min_size: 0.5,
            spacing: 0.1,
            ..d
        },
        "round" => Brush {
            size: 5.3,
            hardness: 0.6,
            flow: 0.7,
            ..d
        },
        "filbert" => Brush {
            size: 5.2,
            aspect: 0.45,
            hardness: 0.8,
            flow: 0.8,
            pickup: 0.03,
            half_life: Some(320.0),
            grain: 0.25,
            size_jitter: 0.06,
            angle_jitter: 3.0,
            ..d
        },
        "flat" => Brush {
            size: 6.7,
            aspect: 0.22,
            hardness: 0.85,
            flow: 0.85,
            pickup: 0.05,
            half_life: Some(260.0),
            grain: 0.35,
            size_jitter: 0.04,
            ..d
        },
        "airbrush" => Brush {
            size: 30.0,
            hardness: 0.0,
            flow: 0.06,
            min_size: 0.6,
            spacing: 0.08,
            ..d
        },
        "dry" => Brush {
            size: 7.1,
            aspect: 0.6,
            hardness: 0.5,
            flow: 0.8,
            half_life: Some(90.0),
            min_load: 0.05,
            grain: 0.8,
            grain_scale: 2.5,
            pickup: 0.02,
            ..d
        },
        "blender" => Brush {
            size: 10.0,
            hardness: 0.0,
            flow: 0.35,
            pickup: 0.6,
            ..d
        },
        _ => return None,
    })
}

impl Brush {
    /// Brush from a preset name, or an object `{"preset": ..., field: value, ...}`.
    pub fn from_spec(spec: &Value) -> Result<Brush> {
        match spec {
            Value::String(name) => preset(name).ok_or_else(|| {
                Error(format!(
                    "unknown preset {name:?}; have {:?}",
                    sorted_presets()
                ))
            }),
            Value::Object(m) => {
                let mut fields = m.clone();
                let base = match fields.remove("preset") {
                    Some(p) => Brush::from_spec(&p)?,
                    None => Brush::default(),
                };
                base.with_fields(&fields)
            }
            other => err(format!(
                "a brush is a preset name or an object, got {other}"
            )),
        }
    }

    /// A copy with some fields replaced; unknown names are an error.
    pub fn with_fields(&self, fields: &Map<String, Value>) -> Result<Brush> {
        let unknown: Vec<&str> = fields
            .keys()
            .map(|k| k.as_str())
            .filter(|k| !FIELDS.contains(k))
            .collect();
        if !unknown.is_empty() {
            let mut u = unknown.clone();
            u.sort();
            return err(format!("unknown brush field(s): {u:?}"));
        }
        let mut b = self.clone();
        for (k, v) in fields {
            let num = |v: &Value| {
                v.as_f64()
                    .ok_or_else(|| Error(format!("brush field {k} must be a number, got {v}")))
            };
            let opt = |v: &Value| {
                if v.is_null() {
                    Ok(None)
                } else {
                    num(v).map(Some)
                }
            };
            match k.as_str() {
                "size" => b.size = num(v)?,
                "aspect" => b.aspect = num(v)?,
                "angle" => b.angle = opt(v)?,
                "hardness" => b.hardness = num(v)?,
                "flow" => b.flow = num(v)?,
                "spacing" => b.spacing = num(v)?,
                "min_size" => b.min_size = num(v)?,
                "size_gamma" => b.size_gamma = num(v)?,
                "min_flow" => b.min_flow = num(v)?,
                "flow_gamma" => b.flow_gamma = num(v)?,
                "tilt_stretch" => b.tilt_stretch = num(v)?,
                "size_jitter" => b.size_jitter = num(v)?,
                "angle_jitter" => b.angle_jitter = num(v)?,
                "pos_jitter" => b.pos_jitter = num(v)?,
                "flow_jitter" => b.flow_jitter = num(v)?,
                "load" => b.load = num(v)?,
                "half_life" => b.half_life = opt(v)?,
                "min_load" => b.min_load = num(v)?,
                "pickup" => b.pickup = num(v)?,
                "grain" => b.grain = num(v)?,
                "grain_scale" => b.grain_scale = num(v)?,
                _ => unreachable!(),
            }
        }
        Ok(b)
    }

    /// Sizes and distances scaled for a larger render.
    pub fn scaled(&self, scale: f64) -> Brush {
        if scale == 1.0 {
            return self.clone();
        }
        let mut b = self.clone();
        b.size *= scale;
        b.grain_scale *= scale;
        if let Some(h) = b.half_life.filter(|&h| h != 0.0) {
            b.half_life = Some(h * scale);
        }
        b
    }

    pub fn radius(&self, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0);
        self.size * (self.min_size + (1.0 - self.min_size) * p.powf(self.size_gamma))
    }

    pub fn flow_at(&self, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0);
        self.flow * (self.min_flow + (1.0 - self.min_flow) * p.powf(self.flow_gamma))
    }

    pub fn sigma_scale(&self) -> f64 {
        gauss::sigma_scale(self.hardness)
    }

    /// Half-extent of the dab measured along the direction of travel.
    pub fn footprint_along(&self, r: f64) -> f64 {
        match self.angle {
            None => r * self.aspect,
            Some(_) => r * self.aspect.max(0.35),
        }
    }

    pub fn step(&self, p: f64) -> f64 {
        (self.spacing * 2.0 * self.footprint_along(self.radius(p))).max(0.35)
    }
}

const FIELDS: [&str; 21] = [
    "size",
    "aspect",
    "angle",
    "hardness",
    "flow",
    "spacing",
    "min_size",
    "size_gamma",
    "min_flow",
    "flow_gamma",
    "tilt_stretch",
    "size_jitter",
    "angle_jitter",
    "pos_jitter",
    "flow_jitter",
    "load",
    "half_life",
    "min_load",
    "pickup",
    "grain",
    "grain_scale",
];

fn sorted_presets() -> Vec<&'static str> {
    let mut v = PRESET_NAMES.to_vec();
    v.sort();
    v
}
