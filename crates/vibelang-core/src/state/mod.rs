//! State management for VibeLang.
//!
//! This module provides the central state model and message passing system.
//! All state mutations flow through [`StateMessage`], ensuring a single source
//! of truth that can be safely shared between threads.
//!
//! # Architecture
//!
//! - [`ScriptState`] - The complete state snapshot
//! - [`StateMessage`] - All possible state mutations
//! - [`StateManager`] - Thread-safe state access

mod manager;
mod messages;
mod model;

pub use manager::StateManager;
pub use messages::StateMessage;
pub use model::{
    ActiveFadeJob, ActiveSequence, ActiveSynth, EffectState, GroupState, LoopStatus, MelodyState,
    MeterLevel, MidiCallbackInfo, MidiCallbackType, MidiConfiguration, MidiDeviceState,
    MidiRecordingState, PatternState, RecordedMidiNote, SampleInfo, SampleSlice, ScheduledEvent,
    ScheduledNoteOff, ScriptState, SequenceRunLog, VoiceState, VstInstrumentInfo,
};

// Re-export scheduler types that are closely tied to state
pub use crate::scheduler::{LoopKind, LoopSnapshot};
