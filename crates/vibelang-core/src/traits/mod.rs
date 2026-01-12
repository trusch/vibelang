//! Feature traits for vibelang-core2.
//!
//! This module defines the public interface for each feature area. Traits are
//! implemented by handlers in the [`handlers`](crate::handlers) module.
//!
//! # Trait Overview
//!
//! | Trait | Description |
//! |-------|-------------|
//! | [`Transport`] | Tempo, time signature, playback control |
//! | [`Groups`] | Audio bus groups for hierarchical routing |
//! | [`Voices`] | Sound-producing synth voices |
//! | [`Patterns`] | Rhythmic step sequences |
//! | [`Melodies`] | Pitched note sequences |
//! | [`Sequences`] | Arrangements of clips |
//! | [`Fades`] | Parameter automation over time |
//! | [`Effects`] | Audio effects processing |
//! | [`Samples`] | Audio file loading |
//! | [`Sfz`] | SFZ instrument loading and playback |
//! | [`Recordings`] | Audio recording to buffers/files |
//! | [`SynthDefs`] | SynthDef management |
//! | [`Midi`] | MIDI device I/O (feature-gated) |
//!
//! # Async Methods
//!
//! All mutating methods are async to support both:
//! - **Native**: Uses tokio async runtime
//! - **WASM**: Uses wasm-bindgen-futures
//!
//! # Configuration Types
//!
//! Many traits include associated configuration types (e.g., [`VoiceConfig`],
//! [`PatternConfig`]) that define the initial state of entities.

mod effects;
mod fades;
mod groups;
mod melodies;
mod modulators;
mod patterns;
mod samples;
mod sequences;
mod sfz;
mod synthdefs;
mod transport;
mod voices;

#[cfg(not(target_arch = "wasm32"))]
mod recordings;

#[cfg(feature = "midi")]
mod midi;

// Re-export traits
pub use effects::Effects;
pub use fades::{FadeConfig, FadeCurve, FadeTarget, Fades};
pub use groups::Groups;
pub use melodies::{Melodies, MelodyConfig, NoteEvent};
pub use modulators::{ModulatorConfig, Modulators};
pub use patterns::{PatternConfig, Patterns, Step};
pub use samples::{SampleConfig, SampleInfo, Samples};
pub use sequences::{Clip, SequenceConfig, Sequences};
pub use sfz::{Sfz, SfzConfig, SfzTriggerInfo, SfzTriggerMode};
pub use synthdefs::SynthDefs;
pub use transport::Transport;
pub use voices::{VoiceConfig, Voices};

#[cfg(not(target_arch = "wasm32"))]
pub use recordings::{RecordingConfig, RecordingInfo, RecordingStatus, Recordings};

#[cfg(feature = "midi")]
pub use midi::{Midi, MidiDeviceInfo};
