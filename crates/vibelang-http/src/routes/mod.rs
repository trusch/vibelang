//! Route handlers for the HTTP API.

pub mod effects;
pub mod eval;
pub mod modulators;
pub mod fades;
pub mod groups;
pub mod live;
pub mod melodies;
#[cfg(feature = "midi")]
pub mod midi;
pub mod patterns;
#[cfg(not(target_arch = "wasm32"))]
pub mod recordings;
pub mod samples;
pub mod sequences;
pub mod synthdefs;
pub mod transport;
pub mod voices;
