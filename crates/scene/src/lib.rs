//! The bones: what a painting is made of, as plain data.
//!
//! A [`Scene`] is the JSON scene of `paintcode/scene.py`: canvas settings, named brushes and an
//! ordered list of strokes. Nothing here paints; it only parses and validates.

mod brush;
mod color;
mod scene;

pub use brush::{preset, Brush, PRESET_NAMES};
pub use color::parse_color;
pub use scene::{CanvasCfg, Color, Scene, Stroke};

/// A message for a person or a model reading a bad scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error(msg.into()))
}
