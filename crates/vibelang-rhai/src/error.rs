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

    /// Language-contract selection or compatibility error.
    #[error("Language contract error: {0}")]
    Language(#[from] crate::version::LanguageSelectionError),

    /// V2 foundation validation error.
    #[error("V2 foundation error: {0}")]
    Foundation(#[from] crate::foundation::FoundationError),
}

impl Error {
    /// Parsing and source reads complete before script registries, deployment
    /// callbacks, or extension operations can run.
    #[must_use]
    pub const fn definitely_no_effect(&self) -> bool {
        matches!(
            self,
            Self::Parse(_) | Self::Io(_) | Self::Language(_) | Self::Foundation(_)
        )
    }
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
