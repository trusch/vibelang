//! Error types for vibelang-core2.
//!
//! This module defines the unified error type used throughout the crate.

use crate::types::{EffectId, GroupId, MelodyId, ModulatorId, PatternId, RecordingId, SampleId, SequenceId, SfzId, VoiceId};
use std::path::PathBuf;
use thiserror::Error;

/// Unified error type for vibelang-core2 operations.
#[derive(Error, Debug)]
pub enum Error {
    // =========================================================================
    // Backend Errors
    // =========================================================================
    /// Backend operation failed.
    #[error("backend error: {0}")]
    Backend(String),

    /// Backend is not ready.
    #[error("backend not ready")]
    BackendNotReady,

    // =========================================================================
    // Resource Errors
    // =========================================================================
    /// SynthDef not found.
    #[error("synthdef not found: {0}")]
    SynthDefNotFound(String),

    /// Sample not found.
    #[error("sample not found: {0}")]
    SampleNotFound(SampleId),

    /// Failed to load sample file.
    #[error("failed to load sample from {path}: {reason}")]
    SampleLoadFailed { path: PathBuf, reason: String },

    /// SFZ instrument not found.
    #[error("SFZ instrument not found: {0}")]
    SfzNotFound(SfzId),

    /// Failed to load SFZ instrument.
    #[error("failed to load SFZ from {path}: {reason}")]
    SfzLoadFailed { path: PathBuf, reason: String },

    /// Recording not found.
    #[error("recording not found: {0}")]
    RecordingNotFound(RecordingId),

    /// Recording already exists.
    #[error("recording already exists: {0}")]
    RecordingAlreadyExists(RecordingId),

    // =========================================================================
    // Entity Not Found Errors
    // =========================================================================
    /// Group not found.
    #[error("group not found: {0}")]
    GroupNotFound(GroupId),

    /// Voice not found.
    #[error("voice not found: {0}")]
    VoiceNotFound(VoiceId),

    /// Pattern not found.
    #[error("pattern not found: {0}")]
    PatternNotFound(PatternId),

    /// Melody not found.
    #[error("melody not found: {0}")]
    MelodyNotFound(MelodyId),

    /// Sequence not found.
    #[error("sequence not found: {0}")]
    SequenceNotFound(SequenceId),

    /// Effect not found.
    #[error("effect not found: {0}")]
    EffectNotFound(EffectId),

    /// Modulator not found.
    #[error("modulator not found: {0}")]
    ModulatorNotFound(ModulatorId),

    // =========================================================================
    // Entity Already Exists Errors
    // =========================================================================
    /// Group already exists.
    #[error("group already exists: {0}")]
    GroupExists(GroupId),

    /// Voice already exists.
    #[error("voice already exists: {0}")]
    VoiceExists(VoiceId),

    /// Pattern already exists.
    #[error("pattern already exists: {0}")]
    PatternExists(PatternId),

    /// Melody already exists.
    #[error("melody already exists: {0}")]
    MelodyExists(MelodyId),

    /// Sequence already exists.
    #[error("sequence already exists: {0}")]
    SequenceExists(SequenceId),

    /// Effect already exists.
    #[error("effect already exists: {0}")]
    EffectExists(EffectId),

    /// Modulator already exists.
    #[error("modulator already exists: {0}")]
    ModulatorExists(ModulatorId),

    // =========================================================================
    // Validation Errors
    // =========================================================================
    /// Invalid parameter value.
    #[error("invalid parameter value for {param}: {reason}")]
    InvalidParam { param: String, reason: String },

    /// Invalid configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    // =========================================================================
    // Channel Errors
    // =========================================================================
    /// Message channel closed.
    #[error("message channel closed")]
    ChannelClosed,

    // =========================================================================
    // MIDI Errors (feature-gated)
    // =========================================================================
    #[cfg(feature = "midi")]
    /// MIDI device not found.
    #[error("MIDI device not found: {0}")]
    MidiDeviceNotFound(crate::types::ids::MidiDeviceId),

    #[cfg(feature = "midi")]
    /// MIDI operation failed.
    #[error("MIDI error: {0}")]
    MidiError(String),
}

/// Result type alias using [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Create a backend error from any error type.
    pub fn backend(err: impl std::error::Error) -> Self {
        Error::Backend(err.to_string())
    }

    /// Create an invalid parameter error.
    pub fn invalid_param(param: impl Into<String>, reason: impl Into<String>) -> Self {
        Error::InvalidParam {
            param: param.into(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::VoiceNotFound(VoiceId::new(42));
        assert_eq!(err.to_string(), "voice not found: 42");
    }

    #[test]
    fn test_backend_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = Error::backend(io_err);
        assert!(err.to_string().contains("file not found"));
    }
}
