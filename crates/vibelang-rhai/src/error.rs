//! Error types for the scripting layer.

use thiserror::Error;

/// Result type for scripting operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during script execution.
#[derive(Error, Debug)]
pub enum Error {
    /// Script execution error.
    #[error("Script error: {0}")]
    Script(String),

    /// Parse error.
    #[error("Parse error: {0}")]
    Parse(String),

    /// File I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Runtime error.
    #[error("Runtime error: {0}")]
    Runtime(String),
}

impl From<Box<rhai::EvalAltResult>> for Error {
    fn from(err: Box<rhai::EvalAltResult>) -> Self {
        Error::Script(err.to_string())
    }
}

impl From<rhai::ParseError> for Error {
    fn from(err: rhai::ParseError) -> Self {
        Error::Parse(err.to_string())
    }
}
