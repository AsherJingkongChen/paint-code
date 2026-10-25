//! The flesh: a petal's control points become a row of dabs, and the brush carries paint. All
//! the dabs of a flower together are its coat.
//!
//! A petal is painted in three steps:
//!
//! 1. walk the path and lay out every dab (position, size, angle, opacity), in order;
//! 2. evaluate every dab's footprint in parallel, with the wet paint under it read from the
//!    canvas as it was when the petal began;
//! 3. carry the colour along the dabs (pickup mixes into the brush), then composite.

mod paint;
mod path;

pub use paint::{Options, Painted, paint_petal};
pub use path::{Sample, samples};
