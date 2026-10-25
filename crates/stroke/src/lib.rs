//! The flesh: a stroke's control points become a row of dabs, and the brush carries paint.
//!
//! Ports `paintcode/stroke.py`. A stroke is painted in three steps:
//!
//! 1. walk the path and lay out every dab (position, size, angle, opacity), in order;
//! 2. evaluate every dab's footprint in parallel, with the wet paint under it read from the
//!    canvas as it was when the stroke began (as the Python engine's snapshot);
//! 3. carry the colour along the dabs (pickup mixes into the brush), then composite.

mod path;
mod stroke;

pub use path::{samples, Sample};
pub use stroke::{layout, paint_stroke, Options};
