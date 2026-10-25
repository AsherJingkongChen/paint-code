//! The leaf of the engine. Every other crate may depend on this one; it depends on nothing.
//!
//! - [`rng`]: counter-based random numbers, bit-identical to `paintcode/rng.py`
//! - [`gauss`]: the dab profile and the geometry of an anisotropic 2D Gaussian

pub mod gauss;
pub mod rng;
