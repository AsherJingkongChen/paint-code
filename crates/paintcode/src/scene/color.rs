use serde_json::Value;

use super::{err, Result};

/// `[r, g, b]` in 0..255, or `"#rrggbb"` / `"#rgb"`, to float32 0..1 (as `canvas.parse_color`).
pub fn parse_color(c: &Value) -> Result<[f32; 3]> {
    let raw: [f32; 3] = match c {
        Value::String(s) => {
            let s = s.trim_start_matches('#');
            let s: String = if s.len() == 3 {
                s.chars().flat_map(|ch| [ch, ch]).collect()
            } else {
                s.to_string()
            };
            if s.len() != 6 || !s.is_ascii() {
                return err(format!("bad colour {c}"));
            }
            let mut out = [0f32; 3];
            for (i, o) in out.iter_mut().enumerate() {
                *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16)
                    .map_err(|_| super::Error(format!("bad colour {c}")))?
                    as f32;
            }
            out
        }
        Value::Array(a) if a.len() == 3 => {
            let mut out = [0f32; 3];
            for (o, v) in out.iter_mut().zip(a) {
                *o = v
                    .as_f64()
                    .ok_or_else(|| super::Error(format!("bad colour {c}")))?
                    as f32;
            }
            out
        }
        _ => return err(format!("colour must have 3 channels, got {c}")),
    };
    Ok(raw.map(|v| (v / 255.0f32).clamp(0.0, 1.0)))
}
