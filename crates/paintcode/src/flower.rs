//! The bones: a flower, the JSON a painter writes, as plain data.
//!
//! A [`Flower`] is a size, a background, a seed, named brushes and an ordered list of petals
//! (strokes of any shape). Nothing here paints; it only parses and validates.

mod brush;
mod color;
mod parse;

pub use brush::{Brush, PRESET_NAMES, preset};
pub use color::parse_color;
pub use parse::{Color, Flower, POINT_MARGIN, Petal};

/// A message for a person or a model reading a bad flower.
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
