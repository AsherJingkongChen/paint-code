//! The leaf of the engine. Every other module may use this one; it uses nothing.
//!
//! - [`rng`]: counter-based random numbers, pinned by tests as part of the format
//! - [`gauss`]: the dab profile and the geometry of an anisotropic 2D Gaussian

pub mod gauss;
pub mod rng;
