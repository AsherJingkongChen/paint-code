//! The files. Each block does one conversion and comes with its inverse, so every pair can be
//! checked with a round trip. This window only needs the first one.

use paintcode_render::RgbImage;

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

/// Decode an 8-bit RGB PNG back to pixels.
pub fn png_to_image(png_bytes: &[u8]) -> Result<RgbImage, String> {
    let dec = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.color_type != png::ColorType::Rgb || info.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "expected 8-bit RGB, got {:?} {:?}",
            info.color_type, info.bit_depth
        ));
    }
    buf.truncate(info.buffer_size());
    Ok(RgbImage {
        width: info.width as usize,
        height: info.height as usize,
        data: buf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
