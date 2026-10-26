//! The files. Each encoder comes with its decoder, so every pair can be checked with a round trip.
//! Decoders look at the bytes, never at a file name.
//!
//! `encode_x` turns an `x` into bytes and `decode_x` turns the bytes back:
//!
//! | out               | back              | file           | what                                  |
//! |-------------------|-------------------|----------------|---------------------------------------|
//! | [`encode_flower`] | [`decode_flower`] | `.flower.json` | a flower, one petal per line          |
//! | [`encode_canvas`] | [`decode_canvas`] | `.png`         | a canvas, 8-bit RGB, maybe its flower |
//! | [`encode_coat`]   | [`decode_coat`]   | `.coat.ply`    | a coat: every dab, as 3D Gaussians    |
//!
//! [`decode_flower`] also reads the flower on a `.flower.png`.

use std::io::Cursor;

use serde_json::Value;

use crate::flower::Flower;
use crate::render::{Canvas, Coat, Dab, MAX_PIXELS};

pub type Result<T> = std::result::Result<T, String>;

// --------------------------------------------------------------------------------------- PNG

/// The iTXt keyword a flower sits under on a canvas.
pub const FLOWER_KEYWORD: &str = "paintcode-flower";
/// The longest flower a PNG may carry, uncompressed.
const FLOWER_LIMIT: usize = 64 << 20;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(PNG_SIGNATURE)
}

/// A canvas as a PNG (8-bit RGB, lossless), with its flower if one is on it. The flower goes in a
/// compressed iTXt chunk before the pixels, where a reader finds it first.
pub fn encode_canvas(canvas: &Canvas) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, canvas.width as u32, canvas.height as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("PNG header");
        if let Some(text) = &canvas.flower {
            let mut chunk = png::text_metadata::ITXtChunk::new(FLOWER_KEYWORD, text.as_str());
            chunk.compressed = true;
            w.write_text_chunk(&chunk).expect("PNG text");
        }
        w.write_image_data(&canvas.pixels).expect("PNG data");
    }
    out
}

fn read_flower_chunk(info: &png::Info) -> Result<Option<String>> {
    let Some(chunk) = info.utf8_text.iter().find(|c| c.keyword == FLOWER_KEYWORD) else {
        return Ok(None);
    };
    let mut chunk = chunk.clone();
    chunk
        .decompress_text_with_limit(FLOWER_LIMIT)
        .map_err(|e| format!("the flower on this PNG is unreadable: {e}"))?;
    chunk
        .get_text()
        .map(Some)
        .map_err(|e| format!("the flower on this PNG is unreadable: {e}"))
}

fn open_png(bytes: &[u8]) -> Result<png::Reader<Cursor<&[u8]>>> {
    let mut dec = png::Decoder::new(Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    dec.read_info().map_err(|e| format!("not a PNG: {e}"))
}

/// A canvas from a PNG, with the flower on it if there is one. Any PNG works: palette and
/// low bit depths are expanded, grey is copied to all three channels and alpha is dropped without
/// compositing (as Pillow's `convert("RGB")`). 16-bit samples keep their high byte, so a 16-bit
/// image keeps its tones; Pillow instead clips 16-bit grey at 255, which turns most of such an
/// image white.
///
/// The size in the header is checked against [`MAX_PIXELS`] before anything is allocated.
pub fn decode_canvas(bytes: &[u8]) -> Result<Canvas> {
    let mut reader = open_png(bytes)?;
    let (w, h) = (reader.info().width as usize, reader.info().height as usize);
    if w.saturating_mul(h) > MAX_PIXELS {
        return Err(format!(
            "a {w}x{h} PNG is too large: the limit is {MAX_PIXELS} pixels (16384 x 16384)"
        ));
    }
    let size = reader.output_buffer_size().ok_or("PNG too large")?;
    let mut buf = Vec::new();
    buf.try_reserve_exact(size)
        .map_err(|_| format!("not enough memory for a {w}x{h} PNG"))?;
    buf.resize(size, 0);
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("bad PNG: {e}"))?;
    buf.truncate(info.buffer_size());
    // a flower may also sit after the pixels, if another program wrote the file again
    let mut flower = read_flower_chunk(reader.info())?;
    if flower.is_none() && reader.finish().is_ok() {
        flower = read_flower_chunk(reader.info())?;
    }
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("PNG palette was not expanded".into()),
    };
    let data = if channels == 3 {
        buf
    } else {
        buf.chunks_exact(channels)
            .flat_map(|px| {
                if channels <= 2 {
                    [px[0]; 3]
                } else {
                    [px[0], px[1], px[2]]
                }
            })
            .collect()
    };
    Ok(Canvas {
        width: info.width as usize,
        height: info.height as usize,
        pixels: data,
        flower,
    })
}

// ------------------------------------------------------------------------------------ flower

/// A flower as JSON: compact, but one petal per line, so a diff shows which petals changed. Keys
/// stay in the order they were written.
pub fn encode_flower(flower: &Flower) -> Vec<u8> {
    let Some(root) = flower.source.as_object() else {
        return format!("{}\n", flower.source).into_bytes();
    };
    let mut out = String::from("{");
    for (i, (k, v)) in root.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&Value::String(k.clone()).to_string());
        out.push(':');
        match v.as_array().filter(|a| k == "petals" && !a.is_empty()) {
            Some(petals) => {
                out.push_str("[\n");
                let lines: Vec<String> = petals.iter().map(Value::to_string).collect();
                out.push_str(&lines.join(",\n"));
                out.push_str("\n]");
            }
            None => out.push_str(&v.to_string()),
        }
    }
    out.push_str("}\n");
    out.into_bytes()
}

/// A flower, from a `.flower.json` or from the canvas it is on. A PNG must carry a flower: a
/// canvas without one is an error, never a guess.
pub fn decode_flower(bytes: &[u8]) -> Result<Flower> {
    let text = if is_png(bytes) {
        let mut reader = open_png(bytes)?;
        match read_flower_chunk(reader.info())? {
            Some(text) => text,
            None => {
                reader.finish().map_err(|e| format!("bad PNG: {e}"))?;
                read_flower_chunk(reader.info())?.ok_or("no flower on this PNG")?
            }
        }
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| "a flower is UTF-8 JSON")?
    };
    Flower::from_json(&text).map_err(|e| e.0)
}

// --------------------------------------------------------------------------------------- PLY

/// The 3DGS layout, SH degree 0, in the order the original 3DGS code writes it.
pub(crate) const PLY_FIELDS: [&str; 17] = [
    "x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1",
    "scale_2", "rot_0", "rot_1", "rot_2", "rot_3",
];
/// The dab exactly as it was laid, after the 3DGS fields: doubles, then the colour as floats.
const EXACT_FIELDS: [&str; 12] = [
    "pc_x",
    "pc_y",
    "pc_sx",
    "pc_sy",
    "pc_theta",
    "pc_alpha",
    "pc_hardness",
    "pc_grain",
    "pc_grain_scale",
    "pc_r",
    "pc_g",
    "pc_b",
];
const EXACT_DOUBLES: usize = 9;
/// Zeroth-order spherical harmonic.
const C0: f64 = 0.28209479177387814;
/// Total depth the paint order is spread over, in world units.
const DEPTH_SPAN: f64 = 0.05;
/// Sigma of the flat axis, in world units.
const THIN: f64 = 1e-5;
/// The header comment that carries what a coat needs besides its dabs.
const COAT_COMMENT: &str = "paintcode ";

/// A coat as a PLY, binary little endian. Every dab is one flat 3D Gaussian any 3DGS viewer can
/// show:
///
/// - position: the image plane, x right and y down (the COLMAP convention 3DGS uses), the longer
///   side spanning [-1, 1]; z is paint order, the first dab farthest and the last nearest to a
///   camera on the -z side
/// - scale: log sigma for the dab's two axes, and a vanishing third
/// - rotation: quaternion (w, x, y, z) for the dab angle about z
/// - colour: the SH DC term, (rgb - 0.5) / C0; opacity: the logit of the dab's alpha
///
/// The 3DGS kernel is a plain Gaussian, so hardness and paper grain are not in those fields; the
/// `pc_*` fields after them keep every dab exactly, and the `paintcode` comment keeps the size,
/// supersample, seed and background, so [`decode_coat`] gives the same coat back.
pub fn encode_coat(coat: &Coat) -> Vec<u8> {
    let (width, height, ss) = (coat.width, coat.height, coat.supersample.max(1) as f64);
    let n = coat.dabs.len();
    let s = 2.0 / width.max(height) as f64; // output px -> world units
    let bg = coat.background;
    let meta = serde_json::json!({
        "width": width,
        "height": height,
        "supersample": coat.supersample,
        "seed": coat.seed,
        "background": bg,
    });
    let mut header = vec![
        "ply".to_string(),
        "format binary_little_endian 1.0".into(),
        "comment made with paintcode: one flat Gaussian per brush dab".into(),
        format!("comment canvas {width} {height}"),
        format!(
            "comment background {:.6} {:.6} {:.6}",
            bg[0] as f64, bg[1] as f64, bg[2] as f64
        ),
        format!("comment {COAT_COMMENT}{meta}"),
        format!("element vertex {n}"),
    ];
    header.extend(PLY_FIELDS.iter().map(|f| format!("property float {f}")));
    for (i, f) in EXACT_FIELDS.iter().enumerate() {
        let ty = if i < EXACT_DOUBLES { "double" } else { "float" };
        header.push(format!("property {ty} {f}"));
    }
    header.push("end_header\n".into());
    let mut out = header.join("\n").into_bytes();
    out.reserve(n * (PLY_FIELDS.len() * 4 + EXACT_DOUBLES * 8 + 12));
    for (i, d) in coat.dabs.iter().enumerate() {
        // the 3DGS fields are in the pixels of the finished canvas
        let (x, y, sx, sy) = (d.x / ss, d.y / ss, d.sx / ss, d.sy / ss);
        let alpha = d.alpha.clamp(1e-4, 1.0 - 1e-4);
        let half = d.theta / 2.0;
        let rgb = d.rgb.map(|c| (c as f64 - 0.5) / C0);
        let row: [f64; 17] = [
            (x - width as f64 / 2.0) * s,
            (y - height as f64 / 2.0) * s,
            DEPTH_SPAN * (1.0 - i as f64 / (n.max(2) - 1) as f64),
            0.0,
            0.0,
            0.0,
            rgb[0],
            rgb[1],
            rgb[2],
            (alpha / (1.0 - alpha)).ln(),
            (sx * s).ln(),
            (sy * s).ln(),
            THIN.ln(),
            half.cos(),
            0.0,
            0.0,
            half.sin(),
        ];
        for v in row {
            out.extend_from_slice(&(v as f32).to_le_bytes());
        }
        let exact = [
            d.x,
            d.y,
            d.sx,
            d.sy,
            d.theta,
            d.alpha,
            d.hardness,
            d.grain,
            d.grain_scale,
        ];
        for v in exact {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for c in d.rgb {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out
}

/// A PLY read back: each property's column, and the header comments.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ply {
    pub(crate) names: Vec<String>,
    /// `columns[j][i]` is property `names[j]` of vertex `i`.
    pub(crate) columns: Vec<Vec<f64>>,
    pub(crate) comments: Vec<String>,
}

impl Ply {
    pub(crate) fn column(&self, name: &str) -> Option<&[f64]> {
        let j = self.names.iter().position(|n| n == name)?;
        Some(&self.columns[j])
    }
}

/// Read a binary little-endian PLY of float and double vertex properties, as [`encode_coat`]
/// writes and as 3DGS files are. There is no `encode_ply`: a PLY is written from a coat.
pub(crate) fn decode_ply(ply: &[u8]) -> Result<Ply> {
    let mut pos = 0;
    let mut line = || -> Result<String> {
        let end = ply[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("PLY header has no end_header")?;
        let s = std::str::from_utf8(&ply[pos..pos + end])
            .map_err(|_| "PLY header is not UTF-8")?
            .trim()
            .to_string();
        pos += end + 1;
        Ok(s)
    };
    if line().ok().as_deref() != Some("ply") {
        return Err("not a PLY file".into());
    }
    let (mut names, mut sizes, mut comments) = (Vec::new(), Vec::new(), Vec::new());
    let mut count = 0usize;
    loop {
        let l = line()?;
        if l == "end_header" {
            break;
        } else if let Some(c) = l.strip_prefix("comment ") {
            comments.push(c.to_string());
        } else if let Some(n) = l.strip_prefix("element vertex ") {
            count = n
                .trim()
                .parse()
                .map_err(|_| format!("bad PLY line {l:?}"))?;
        } else if let Some(p) = l.strip_prefix("property ") {
            let size = match p.split_whitespace().collect::<Vec<_>>()[..] {
                ["float" | "float32", name] => (4, name),
                ["double" | "float64", name] => (8, name),
                _ => return Err(format!("only float and double properties, got {l:?}")),
            };
            sizes.push(size.0);
            names.push(size.1.to_string());
        } else if l != "format binary_little_endian 1.0" {
            return Err(format!("unsupported PLY line {l:?}"));
        }
    }
    let stride: usize = sizes.iter().sum();
    let body = &ply[pos..];
    let need = count
        .checked_mul(stride)
        .ok_or("PLY vertex count is too large")?;
    if body.len() < need {
        return Err(format!("PLY is cut short: {} of {need} bytes", body.len()));
    }
    let mut columns = vec![Vec::with_capacity(count); names.len()];
    for row in body[..need].chunks_exact(stride.max(1)) {
        let mut at = 0;
        for (col, &size) in columns.iter_mut().zip(&sizes) {
            let b = &row[at..at + size];
            col.push(if size == 4 {
                f32::from_le_bytes(b.try_into().unwrap()) as f64
            } else {
                f64::from_le_bytes(b.try_into().unwrap())
            });
            at += size;
        }
    }
    Ok(Ply {
        names,
        columns,
        comments,
    })
}

/// A coat back from the PLY [`encode_coat`] wrote. A 3DGS file from elsewhere has no `pc_*` fields
/// and is refused: its Gaussians are not dabs this engine laid.
pub fn decode_coat(ply: &[u8]) -> Result<Coat> {
    let s = decode_ply(ply)?;
    let meta = s
        .comments
        .iter()
        .find_map(|c| c.strip_prefix(COAT_COMMENT))
        .ok_or("this PLY is not a coat: it has no paintcode comment")?;
    let meta: Value =
        serde_json::from_str(meta).map_err(|e| format!("bad paintcode comment: {e}"))?;
    let size = |k: &str| -> Result<usize> {
        meta[k]
            .as_u64()
            .filter(|&v| v >= 1)
            .map(|v| v as usize)
            .ok_or_else(|| format!("bad {k} in the paintcode comment"))
    };
    let background: [f32; 3] = meta["background"]
        .as_array()
        .filter(|a| a.len() == 3)
        .and_then(|a| {
            let v: Option<Vec<f32>> = a.iter().map(|c| c.as_f64().map(|f| f as f32)).collect();
            v?.try_into().ok()
        })
        .ok_or("bad background in the paintcode comment")?;
    let seed = meta["seed"]
        .as_i64()
        .ok_or("bad seed in the paintcode comment")?;
    let cols: Vec<&[f64]> = EXACT_FIELDS
        .iter()
        .map(|f| {
            s.column(f)
                .ok_or_else(|| format!("this PLY is not a coat: it has no {f} field"))
        })
        .collect::<Result<_>>()?;
    let n = cols[0].len();
    let dabs = (0..n)
        .map(|i| Dab {
            x: cols[0][i],
            y: cols[1][i],
            sx: cols[2][i],
            sy: cols[3][i],
            theta: cols[4][i],
            alpha: cols[5][i],
            hardness: cols[6][i],
            grain: cols[7][i],
            grain_scale: cols[8][i],
            rgb: [cols[9][i] as f32, cols[10][i] as f32, cols[11][i] as f32],
        })
        .collect();
    Ok(Coat {
        width: size("width")?,
        height: size("height")?,
        supersample: size("supersample")?,
        background,
        seed,
        dabs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> Canvas {
        let data: Vec<u8> = (0..7 * 5 * 3).map(|i| (i * 37 % 256) as u8).collect();
        Canvas {
            width: 7,
            height: 5,
            pixels: data,
            flower: None,
        }
    }

    #[test]
    fn png_round_trip() {
        let c = canvas();
        assert_eq!(decode_canvas(&encode_canvas(&c)).unwrap(), c);
    }

    #[test]
    fn a_flower_rides_on_the_png() {
        let text = r#"{"petals":[{"points":[[1,2.5]],"color":null}],"seed":-3}"#;
        let flower = Flower::from_json(text).unwrap();
        let on = Canvas {
            flower: Some(text.to_string()),
            ..canvas()
        };
        let png = encode_canvas(&on);
        assert_eq!(decode_canvas(&png).unwrap(), on);
        assert_eq!(decode_flower(&png).unwrap(), flower);
        assert_eq!(decode_flower(text.as_bytes()).unwrap(), flower);
        let bare = encode_canvas(&canvas());
        assert_eq!(decode_flower(&bare).unwrap_err(), "no flower on this PNG");
        // the flower is compressed: a thousand petals cost far less than their text
        let long = format!("[{}]", vec![r#"{"points":[[1,2.5]]}"#; 1000].join(","));
        let png = encode_canvas(&Canvas {
            flower: Some(long.clone()),
            ..canvas()
        });
        assert!(
            png.len() < bare.len() + long.len() / 10,
            "{} bytes",
            png.len()
        );
        assert_eq!(decode_canvas(&png).unwrap().flower.unwrap(), long);
        assert!(decode_flower(b"{nope").unwrap_err().contains("not JSON"));
    }

    #[test]
    fn a_flower_file_has_one_petal_per_line_and_reads_back() {
        let text = r##"{"width":3,"petals":[{"points":[[1,2.5]]},{"color":"#e8d9b0","points":[[485.09081788179355,0.1]]}],"seed":1}"##;
        let flower = Flower::from_json(text).unwrap();
        let out = encode_flower(&flower);
        assert_eq!(
            String::from_utf8(out.clone()).unwrap(),
            "{\"width\":3,\"petals\":[\n{\"points\":[[1,2.5]]},\n{\"color\":\"#e8d9b0\",\"points\":[[485.09081788179355,0.1]]}\n],\"seed\":1}\n"
        );
        assert_eq!(decode_flower(&out).unwrap(), flower);
    }

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let chunk = |t: &[u8], d: &[u8]| {
            let mut c = (d.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(t);
            c.extend_from_slice(d);
            let mut crc = crc32(t);
            crc = crc32_more(crc, d);
            c.extend_from_slice(&crc.to_be_bytes());
            c
        };
        let mut ihdr = w.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend(chunk(b"IHDR", &ihdr));
        png.extend(chunk(b"IDAT", &[0x78, 0x9c, 0x03, 0, 0, 0, 0, 1])); // an empty zlib stream
        png.extend(chunk(b"IEND", b""));
        png
    }

    fn crc32(d: &[u8]) -> u32 {
        crc32_more(0, d)
    }

    fn crc32_more(crc: u32, d: &[u8]) -> u32 {
        let mut c = !crc;
        for &b in d {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }

    #[test]
    fn a_huge_png_header_is_an_error_before_any_allocation() {
        let e = decode_canvas(&png_header(100_000, 100_000)).unwrap_err();
        assert!(e.contains("100000x100000 PNG is too large"), "{e}");
    }

    #[test]
    fn grey_and_sixteen_bit_pngs_become_rgb() {
        // 2x1 grey 16-bit: 0x1234 and 0xff00 keep their high bytes
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 2, 1);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Sixteen);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[0x12, 0x34, 0xff, 0x00]).unwrap();
        }
        let c = decode_canvas(&out).unwrap();
        assert_eq!(c.pixels, [0x12, 0x12, 0x12, 0xff, 0xff, 0xff]);
        // RGBA 8-bit: alpha is dropped, not composited
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 1, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[10, 20, 30, 0]).unwrap();
        }
        assert_eq!(decode_canvas(&out).unwrap().pixels, [10, 20, 30]);
    }

    fn coat() -> Coat {
        let dab = Dab {
            x: 60.0,
            y: 20.0,
            sx: 8.0,
            sy: 3.0,
            theta: 0.7,
            alpha: 0.6,
            hardness: 0.3,
            grain: 0.25,
            grain_scale: 3.0,
            rgb: [1.0, 0.5, 0.0],
        };
        Coat {
            width: 80,
            height: 40,
            supersample: 2,
            background: [1.0, 0.0, 0.1],
            seed: -7,
            dabs: vec![
                dab,
                Dab {
                    x: 100.0,
                    rgb: [0.2, 0.2, 0.2],
                    ..dab
                },
            ],
        }
    }

    #[test]
    fn ply_round_trip_is_lossless() {
        let c = coat();
        let ply = encode_coat(&c);
        assert_eq!(decode_coat(&ply).unwrap(), c);
        let s = decode_ply(&ply).unwrap();
        assert_eq!(&s.names[..17], PLY_FIELDS);
        // the 3DGS fields are in the pixels of the finished canvas: x = 60 / 2 and 100 / 2
        assert_eq!(s.column("x").unwrap(), [-0.25, 0.25]);
        assert_eq!(s.column("z").unwrap(), [0.05f32 as f64, 0.0]);
        assert!(
            s.comments
                .contains(&"background 1.000000 0.000000 0.100000".to_string())
        );
    }

    #[test]
    fn a_ply_that_is_not_a_coat_is_refused() {
        let ply = encode_coat(&coat());
        let text = String::from_utf8_lossy(&ply);
        let cut = text.find("comment paintcode").unwrap();
        let mut other = ply[..cut].to_vec();
        other.extend_from_slice(&ply[cut + text[cut..].find('\n').unwrap() + 1..]);
        assert!(decode_coat(&other).unwrap_err().contains("not a coat"));
        assert!(decode_coat(b"plain text").is_err());
        let png = encode_canvas(&canvas());
        assert_eq!(decode_coat(&png).unwrap_err(), "not a PLY file");
    }
}
