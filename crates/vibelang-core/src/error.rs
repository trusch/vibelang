//! Error types for vibelang-core.
//!
//! This module defines the unified error type used throughout the crate.

use crate::types::{
    EffectId, GroupId, MelodyId, PatternId, RecordingId, SampleId, SequenceId, SfzId, VoiceId,
};
use std::fmt::Write;
use std::path::PathBuf;
use thiserror::Error;

/// A synthdef rejected by the backend during load/preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthDefRejection {
    pub name: String,
    pub reason: String,
    pub missing_ugen: Option<String>,
}

/// Unified error type for vibelang-core operations.
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

    /// Backend rejected a synthdef during load.
    #[error("synthdef rejected: {name}: {reason}")]
    SynthDefRejected {
        name: String,
        reason: String,
        missing_ugen: Option<String>,
    },

    /// Backend rejected one or more synthdefs during startup preflight.
    #[error("{}", format_synthdef_preflight_failed(rejected))]
    SynthDefPreflightFailed { rejected: Vec<SynthDefRejection> },

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

    // =========================================================================
    // Validation Errors
    // =========================================================================
    /// Invalid parameter value.
    #[error("invalid parameter value for {param}: {reason}")]
    InvalidParam { param: String, reason: String },

    /// Invalid configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// An ID allocator ran out of IDs (node IDs, buffer IDs, or bus IDs).
    ///
    /// Surfaced instead of panicking so a single failed operation degrades
    /// gracefully while the runtime keeps running.
    #[error("{0} exhausted")]
    IdsExhausted(&'static str),

    // =========================================================================
    // Channel Errors
    // =========================================================================
    /// Message channel closed.
    #[error("message channel closed")]
    ChannelClosed,

    /// Message channel is at capacity.
    #[error("message channel full")]
    ChannelFull,

    /// A result-bearing backend barrier did not complete before its deadline.
    #[error("backend synchronization timed out")]
    SyncTimeout,

    /// A result-bearing backend barrier completed without delivering its acknowledgement.
    #[error("backend synchronization acknowledgement was lost")]
    AcknowledgementLost,

    /// The canonical mutation ledger rejected an internal operation.
    #[error("mutation ledger error: {0}")]
    MutationLedger(String),

    /// Mutation ingress is fenced after an uncertain or partial effect.
    #[error("runtime fenced by partial mutation {0}")]
    RuntimeFenced(String),

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

    /// Create a backend error from a string message.
    pub fn backend_msg(msg: impl Into<String>) -> Self {
        Error::Backend(msg.into())
    }

    /// Create an invalid parameter error.
    pub fn invalid_param(param: impl Into<String>, reason: impl Into<String>) -> Self {
        Error::InvalidParam {
            param: param.into(),
            reason: reason.into(),
        }
    }

    /// Create a synthdef-load error from a backend error while preserving
    /// structured rejection details for native scsynth.
    pub fn synthdef_load(name: &str, err: &(dyn std::error::Error + 'static)) -> Self {
        #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
        {
            if let Some(crate::backends::ScsynthError::SynthDefRejected {
                name,
                reason,
                missing_ugen,
            }) = err.downcast_ref::<crate::backends::ScsynthError>()
            {
                return Error::SynthDefRejected {
                    name: name.clone(),
                    reason: reason.clone(),
                    missing_ugen: missing_ugen.clone(),
                };
            }
        }

        Error::Backend(format!("failed to load synthdef '{}': {}", name, err))
    }

    pub fn as_synthdef_rejection(&self) -> Option<SynthDefRejection> {
        match self {
            Error::SynthDefRejected {
                name,
                reason,
                missing_ugen,
            } => Some(SynthDefRejection {
                name: name.clone(),
                reason: reason.clone(),
                missing_ugen: missing_ugen.clone(),
            }),
            _ => None,
        }
    }
}

fn format_synthdef_preflight_failed(rejected: &[SynthDefRejection]) -> String {
    let mut out = format!(
        "failed to load {} SynthDef{} into scsynth.\n\nThe SuperCollider server rejected these definitions:",
        rejected.len(),
        if rejected.len() == 1 { "" } else { "s" }
    );

    for rejection in rejected {
        let _ = write!(out, "\n\n  - {}", rejection.name);
        if let Some(missing_ugen) = &rejection.missing_ugen {
            let _ = write!(out, "\n    missing UGen: {}", missing_ugen);
        }
        let _ = write!(out, "\n    scsynth: {}", rejection.reason);
    }

    out.push_str(
        "\n\nInstall the missing SuperCollider UGen plugins or remove voices/effects that depend on these SynthDefs. Startup stopped before creating synths.",
    );
    out
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
