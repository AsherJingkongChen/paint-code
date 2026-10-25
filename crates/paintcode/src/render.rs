//! The renderer: dabs in, pixels out. It knows nothing about strokes or brushes.
//!
//! A [`Dab`] is one anisotropic 2D Gaussian with an opacity and a profile hardness. Painting a
//! batch of dabs runs in two phases, both parallel:
//!
//! 1. [`Canvas::footprint`] evaluates each dab's kernel on its own, and the colour of the wet
//!    paint under it, reading a fixed state of the canvas;
//! 2. [`Canvas::composite`] lays the footprints down in order, one band of rows per thread.
//!
//! Every pixel sees the same arithmetic in the same order as a one-dab-at-a-time engine (the
//! `Spans::Square` reference path), so splitting the work does not change a single bit.

mod coat;
mod grain;
mod image;
mod wet;

pub use coat::{Coat, coat_to_canvas};
pub use image::RgbImage;
pub use wet::{Canvas, Dab, Footprint, MAX_PIXELS, Pickup, Source, Spans};
