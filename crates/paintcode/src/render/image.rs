/// An 8-bit RGB image, row-major, three bytes per pixel. This is what leaves the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}
