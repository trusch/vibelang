//! Error types for term-keys

use thiserror::Error;

/// Result type alias for term-keys operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in term-keys
#[derive(Debug, Error)]
pub enum Error {
    /// Configuration file error
    #[error("Configuration error: {0}")]
    Config(String),

    /// MIDI backend error
    #[error("MIDI error: {0}")]
    Midi(String),

    /// JACK connection error
    #[error("JACK error: {0}")]
    Jack(#[from] jack::Error),

    /// Terminal/TUI error
    #[error("Terminal error: {0}")]
    Terminal(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// TOML parsing error
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// TOML serialization error
    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}
