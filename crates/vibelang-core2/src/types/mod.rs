//! Core types for vibelang-core2.
//!
//! This module contains type-safe wrappers and fundamental types:
//!
//! - [`ids`]: Newtype wrappers for IDs (NodeId, VoiceId, etc.)
//! - [`time`]: Beat positions and durations with fixed-point precision
//! - [`params`]: Parameter maps for synth control

pub mod ids;
pub mod params;
pub mod time;

pub use ids::*;
pub use params::*;
pub use time::*;
