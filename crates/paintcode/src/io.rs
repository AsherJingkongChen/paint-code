//! The files. Each block does one conversion and comes with its inverse, so every pair can be
//! checked with a round trip:
//!
//! | out                    | back                   | what                                      |
//! |------------------------|------------------------|-------------------------------------------|
//! | [`image_to_png`]       | [`png_to_image`]       | the painting, 8-bit RGB, lossless         |
//! | [`scene_to_petal`]     | [`petal_to_scene`]     | the project: strokes in a ZIP, never dabs |
//! | [`laid_to_ply`]        | [`ply_to_splats`]      | every laid dab as a flat 3D Gaussian      |

use std::io::{Cursor, Read, Write};

use serde_json::Value;

use crate::render::{Laid, RgbImage, MAX_PIXELS};

pub type Result<T> = std::result::Result<T, String>;

// --------------------------------------------------------------------------------------- PNG

/// Encode pixels as a PNG (8-bit RGB, lossless).
pub fn image_to_png(image: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, image.width as u32, image.height as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("PNG header");
        w.write_image_data(&image.data).expect("PNG data");
    }
    out
}

/// Decode a PNG to 8-bit RGB. Any PNG works: palette and low bit depths are expanded, grey is
/// copied to all three channels and alpha is dropped without compositing (as Pillow's
/// `convert("RGB")`). 16-bit samples keep their high byte, so a 16-bit image keeps its tones;
/// Pillow instead clips 16-bit grey at 255, which turns most of such an image white.
///
/// The size in the header is checked against [`MAX_PIXELS`] before anything is allocated.
pub fn png_to_image(png_bytes: &[u8]) -> Result<RgbImage> {
    let mut dec = png::Decoder::new(Cursor::new(png_bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| format!("not a PNG: {e}"))?;
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
    Ok(RgbImage {
        width: info.width as usize,
        height: info.height as usize,
        data,
    })
}

/// A smaller copy that fits in `max` x `max`, by averaging the source pixels under each output
/// pixel. Images that already fit come back unchanged. For previews, not for paintings.
pub fn thumbnail(image: &RgbImage, max: usize) -> RgbImage {
    let (w, h) = (image.width, image.height);
    if w <= max && h <= max || w == 0 || h == 0 {
        return image.clone();
    }
    let f = (max as f64 / w as f64).min(max as f64 / h as f64);
    let (tw, th) = (
        ((w as f64 * f).round() as usize).max(1),
        ((h as f64 * f).round() as usize).max(1),
    );
    let mut data = Vec::with_capacity(tw * th * 3);
    for ty in 0..th {
        let (y0, y1) = (ty * h / th, ((ty + 1) * h / th).max(ty * h / th + 1));
        for tx in 0..tw {
            let (x0, x1) = (tx * w / tw, ((tx + 1) * w / tw).max(tx * w / tw + 1));
            let mut acc = [0u32; 3];
            for y in y0..y1 {
                for x in x0..x1 {
                    for (ch, a) in acc.iter_mut().enumerate() {
                        *a += image.data[(y * w + x) * 3 + ch] as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            data.extend(acc.map(|a| ((a + n / 2) / n) as u8));
        }
    }
    RgbImage {
        width: tw,
        height: th,
        data,
    }
}

// ------------------------------------------------------------------------------------ .petal

/// The project format: a plain ZIP (Deflate) holding `meta.json`, `scene.json` (the scene
/// exactly as written, keys in their order) and optionally `preview.png`.
pub const PETAL_FORMAT: &str = "petal";
pub const PETAL_VERSION: i64 = 1;

/// Write a `.petal`. `preview` is shrunk to fit 256 px. The bytes depend only on the inputs (no
/// timestamps), so the same scene always makes the same file.
pub fn scene_to_petal(scene: &Value, preview: Option<&RgbImage>) -> Result<Vec<u8>> {
    use zip::write::SimpleFileOptions;
    let meta = serde_json::json!({
        "format": PETAL_FORMAT,
        "version": PETAL_VERSION,
        "engine": concat!("paintcode ", env!("CARGO_PKG_VERSION")),
    });
    let meta_text = serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?;
    let scene_text = serde_json::to_vec(scene).map_err(|e| e.to_string())?;

    let deflate = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9));
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let e = |e: zip::result::ZipError| e.to_string();
    z.start_file("meta.json", deflate).map_err(e)?;
    z.write_all(&meta_text).map_err(|e| e.to_string())?;
    z.start_file("scene.json", deflate).map_err(e)?;
    z.write_all(&scene_text).map_err(|e| e.to_string())?;
    if let Some(img) = preview {
        z.start_file("preview.png", stored).map_err(e)?;
        z.write_all(&image_to_png(&thumbnail(img, 256)))
            .map_err(|e| e.to_string())?;
    }
    Ok(z.finish().map_err(e)?.into_inner())
}

/// Read the scene back out of a `.petal`.
pub fn petal_to_scene(petal: &[u8]) -> Result<Value> {
    let mut z = zip::ZipArchive::new(Cursor::new(petal))
        .map_err(|e| format!("not a .petal file (not a ZIP: {e})"))?;
    let mut read = |name: &str| -> Result<Option<Vec<u8>>> {
        match z.by_name(name) {
            Ok(mut f) => {
                let mut v = Vec::new();
                f.read_to_end(&mut v).map_err(|e| format!("{name}: {e}"))?;
                Ok(Some(v))
            }
            Err(zip::result::ZipError::FileNotFound) => Ok(None),
            Err(e) => Err(format!("{name}: {e}")),
        }
    };
    if let Some(meta) = read("meta.json")? {
        let meta: Value = serde_json::from_slice(&meta).map_err(|e| format!("meta.json: {e}"))?;
        // a whole number, or a number or numeric string read the way Python's int() reads it
        let version = match meta.get("version") {
            None => Some(0),
            Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
            Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
            Some(_) => None,
        };
        let known = meta.get("format").and_then(Value::as_str) == Some(PETAL_FORMAT);
        if !known || !version.is_some_and(|v| v <= PETAL_VERSION) {
            return Err(format!("unsupported project format {meta}"));
        }
    }
    let scene = read("scene.json")?.ok_or("not a .petal file (no scene.json)")?;
    serde_json::from_slice(&scene).map_err(|e| format!("scene.json: {e}"))
}

// --------------------------------------------------------------------------------------- PLY

/// The 3DGS layout, SH degree 0, in the order the original 3DGS code writes it.
pub const PLY_FIELDS: [&str; 17] = [
    "x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1",
    "scale_2", "rot_0", "rot_1", "rot_2", "rot_3",
];
/// Zeroth-order spherical harmonic.
const C0: f64 = 0.28209479177387814;
/// Total depth the paint order is spread over, in scene units.
const DEPTH_SPAN: f64 = 0.05;
/// Sigma of the flat axis, in scene units.
const THIN: f64 = 1e-5;

/// Every laid dab as one flat 3D Gaussian, binary little endian:
///
/// - position: the image plane, x right and y down (the COLMAP convention 3DGS uses), the longer
///   side spanning [-1, 1]; z is paint order, the first dab farthest and the last nearest to a
///   camera on the -z side
/// - scale: log sigma for the dab's two axes, and a vanishing third
/// - rotation: quaternion (w, x, y, z) for the dab angle about z
/// - colour: the SH DC term, (rgb - 0.5) / C0; opacity: the logit of the dab's alpha
///
/// The 3DGS kernel is a plain Gaussian, so hardness and paper grain do not survive the trip.
/// `background` goes into a header comment, so a viewer can put the paper behind the splats.
pub fn laid_to_ply(
    laid: &[Laid],
    width: usize,
    height: usize,
    background: Option<[f32; 3]>,
) -> Vec<u8> {
    let n = laid.len();
    let s = 2.0 / width.max(height) as f64; // px -> scene units
    let mut header = vec![
        "ply".to_string(),
        "format binary_little_endian 1.0".into(),
        "comment made with paintcode: one flat Gaussian per brush dab".into(),
        format!("comment canvas {width} {height}"),
    ];
    if let Some(bg) = background {
        header.push(format!(
            "comment background {:.6} {:.6} {:.6}",
            bg[0] as f64, bg[1] as f64, bg[2] as f64
        ));
    }
    header.push(format!("element vertex {n}"));
    header.extend(PLY_FIELDS.iter().map(|f| format!("property float {f}")));
    header.push("end_header\n".into());
    let mut out = header.join("\n").into_bytes();
    out.reserve(n * PLY_FIELDS.len() * 4);
    for (i, l) in laid.iter().enumerate() {
        let d = &l.dab;
        let alpha = d.alpha.clamp(1e-4, 1.0 - 1e-4);
        let half = d.theta / 2.0;
        let rgb = l.rgb.map(|c| (c as f64 - 0.5) / C0);
        let row: [f64; 17] = [
            (d.x - width as f64 / 2.0) * s,
            (d.y - height as f64 / 2.0) * s,
            DEPTH_SPAN * (1.0 - i as f64 / (n.max(2) - 1) as f64),
            0.0,
            0.0,
            0.0,
            rgb[0],
            rgb[1],
            rgb[2],
            (alpha / (1.0 - alpha)).ln(),
            (d.sx * s).ln(),
            (d.sy * s).ln(),
            THIN.ln(),
            half.cos(),
            0.0,
            0.0,
            half.sin(),
        ];
        for v in row {
            out.extend_from_slice(&(v as f32).to_le_bytes());
        }
    }
    out
}

/// A PLY read back: each property's column, and the header comments.
#[derive(Debug, Clone, PartialEq)]
pub struct Splats {
    pub names: Vec<String>,
    /// `columns[j][i]` is property `names[j]` of vertex `i`.
    pub columns: Vec<Vec<f32>>,
    pub comments: Vec<String>,
}

impl Splats {
    pub fn column(&self, name: &str) -> Option<&[f32]> {
        let j = self.names.iter().position(|n| n == name)?;
        Some(&self.columns[j])
    }
}

/// Read a binary little-endian PLY of float properties, as [`laid_to_ply`] writes.
pub fn ply_to_splats(ply: &[u8]) -> Result<Splats> {
    let mut pos = 0;
    let mut line = || -> Result<String> {
        let end = ply[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("PLY header has no end_header")?;
        let s = std::str::from_utf8(&ply[pos..pos + end])
            .map_err(|_| "PLY header is not ASCII")?
            .trim()
            .to_string();
        pos += end + 1;
        Ok(s)
    };
    if line()? != "ply" {
        return Err("not a PLY file".into());
    }
    let (mut names, mut comments, mut count) = (Vec::new(), Vec::new(), 0usize);
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
            match p.split_whitespace().collect::<Vec<_>>()[..] {
                ["float", name] => names.push(name.to_string()),
                _ => return Err(format!("only float properties are supported, got {l:?}")),
            }
        } else if l != "format binary_little_endian 1.0" {
            return Err(format!("unsupported PLY line {l:?}"));
        }
    }
    let body = &ply[pos..];
    let need = count * names.len() * 4;
    if body.len() < need {
        return Err(format!("PLY is cut short: {} of {need} bytes", body.len()));
    }
    let mut columns = vec![Vec::with_capacity(count); names.len()];
    for (k, b) in body[..need].chunks_exact(4).enumerate() {
        columns[k % names.len()].push(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    }
    Ok(Splats {
        names,
        columns,
        comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Dab;

    #[test]
    fn png_round_trip() {
        let data: Vec<u8> = (0..7 * 5 * 3).map(|i| (i * 37 % 256) as u8).collect();
        let img = RgbImage {
            width: 7,
            height: 5,
            data,
        };
        assert_eq!(png_to_image(&image_to_png(&img)).unwrap(), img);
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
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
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
        let e = png_to_image(&png_header(100_000, 100_000)).unwrap_err();
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
        let img = png_to_image(&out).unwrap();
        assert_eq!(img.data, [0x12, 0x12, 0x12, 0xff, 0xff, 0xff]);
        // RGBA 8-bit: alpha is dropped, not composited
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 1, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[10, 20, 30, 0]).unwrap();
        }
        assert_eq!(png_to_image(&out).unwrap().data, [10, 20, 30]);
    }

    #[test]
    fn petal_round_trip_keeps_the_scene_as_written() {
        // keys out of alphabetical order, floats that need every digit, a null and non-ASCII text
        let text = r#"{"strokes":[{"points":[[1,2.5],[485.09081788179355,0.1]],"color":null,"brush":"ink"}],"canvas":{"width":3},"note":"菊"}"#;
        let scene: Value = serde_json::from_str(text).unwrap();
        let petal = scene_to_petal(&scene, None).unwrap();
        let back = petal_to_scene(&petal).unwrap();
        assert_eq!(back, scene);
        assert_eq!(serde_json::to_string(&back).unwrap(), text);
        assert_eq!(
            scene_to_petal(&scene, None).unwrap(),
            petal,
            "not deterministic"
        );
    }

    #[test]
    fn petal_refuses_what_is_not_a_petal() {
        assert!(petal_to_scene(b"plain text").is_err());
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file("meta.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(br#"{"format": "petal", "version": 99}"#)
            .unwrap();
        z.start_file("scene.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"{}").unwrap();
        let newer = z.finish().unwrap().into_inner();
        assert!(petal_to_scene(&newer).unwrap_err().contains("unsupported"));

        let with_version = |v: &str| {
            let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("meta.json", o).unwrap();
            z.write_all(format!(r#"{{"format": "petal", "version": {v}}}"#).as_bytes())
                .unwrap();
            z.start_file("scene.json", o).unwrap();
            z.write_all(b"{}").unwrap();
            petal_to_scene(&z.finish().unwrap().into_inner())
        };
        for bad in [r#""2""#, "2.5", r#""abc""#, "true", "null"] {
            assert!(with_version(bad).is_err(), "version {bad} was accepted");
        }
        for good in ["1", "1.0", r#""1""#, "0"] {
            assert!(with_version(good).is_ok(), "version {good} was refused");
        }
    }

    #[test]
    fn ply_round_trip() {
        let dab = Dab {
            x: 30.0,
            y: 10.0,
            sx: 4.0,
            sy: 1.5,
            theta: 0.7,
            alpha: 0.6,
            hardness: 0.0,
            grain: 0.0,
            grain_scale: 3.0,
        };
        let laid = [
            Laid {
                dab,
                rgb: [1.0, 0.5, 0.0],
            },
            Laid {
                dab: Dab { x: 50.0, ..dab },
                rgb: [0.2, 0.2, 0.2],
            },
        ];
        let s = ply_to_splats(&laid_to_ply(&laid, 80, 40, Some([1.0, 0.0, 0.5]))).unwrap();
        assert_eq!(s.names, PLY_FIELDS);
        assert_eq!(s.column("x").unwrap(), [-0.25, 0.25]);
        assert_eq!(s.column("z").unwrap(), [0.05, 0.0]);
        assert!(s
            .comments
            .contains(&"background 1.000000 0.000000 0.500000".to_string()));
    }

    #[test]
    fn thumbnail_fits_and_keeps_the_aspect() {
        let img = RgbImage {
            width: 1000,
            height: 500,
            data: vec![200; 1000 * 500 * 3],
        };
        let t = thumbnail(&img, 256);
        assert_eq!((t.width, t.height), (256, 128));
        assert!(t.data.iter().all(|&v| v == 200));
    }
}
