/// The canvas: the pixels you see, 8-bit RGB, row-major, three bytes per pixel. A flower may be
/// on it, as the JSON `encode_flower` wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    pub flower: Option<String>,
}
