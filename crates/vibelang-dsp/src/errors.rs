//! Error types for the vibelang-dsp crate.

use thiserror::Error;

/// Errors that can occur during SynthDef construction and encoding.
#[derive(Error, Debug)]
pub enum SynthDefError {
    /// A parameter was referenced by name but doesn't exist.
    #[error("Unknown parameter: {0}")]
    UnknownParam(String),

    /// A UGen was called with the wrong number of arguments.
    #[error("Wrong arity for UGen {ugen}: expected {expected}, got {got}")]
    WrongArity {
        ugen: String,
        expected: usize,
        got: usize,
    },

    /// A UGen argument has the wrong type.
    #[error("Wrong type for UGen {ugen}, argument {arg}: expected {expected}")]
    WrongType {
        ugen: String,
        arg: String,
        expected: String,
    },

    /// Attempted to use graph builder functions without an active builder.
    #[error("No active graph builder in scope")]
    NoActiveBuilder,

    /// A synthdef body closure did not return a valid NodeRef.
    #[error("Body closure did not return a NodeRef")]
    InvalidBodyReturn,

    /// Error during Rhai script evaluation.
    #[error("Rhai evaluation error: {0}")]
    RhaiError(String),

    /// OSC communication error.
    #[error("OSC error: {0}")]
    OscError(String),

    /// File I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Error during binary encoding.
    #[error("Encoding error: {0}")]
    EncodingError(String),

    /// Graph validation failed.
    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// Result type alias using SynthDefError.
pub type Result<T> = std::result::Result<T, SynthDefError>;
