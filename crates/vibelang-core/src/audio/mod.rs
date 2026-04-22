//! Audio connection management for VibeLang.
//!
//! This module provides a unified interface for managing audio port connections
//! across different audio systems (PipeWire, JACK). It handles:
//!
//! - Auto-detection of available audio backends (pw-link, jack_connect)
//! - Listing available ports from all backends
//! - Fuzzy/partial matching of port names for better UX
//! - Connecting and disconnecting ports
//!
//! # Port Matching
//!
//! The module supports flexible port name matching:
//!
//! - Exact match: `"Vibelang:out_1"`
//! - Glob patterns: `"UMC404*:playback_FL"` or `"*Headphones*"`
//! - Substring match: `"UMC404"` matches any port containing that string
//! - Case-insensitive matching
//!
//! When a pattern matches multiple ports, the operation fails with an
//! `AmbiguousMatch` error listing all matches, allowing the user to refine.

mod backend;
mod connector;
mod matcher;

pub use backend::{AudioBackend, BackendCapabilities};
pub use connector::AudioConnector;
pub use matcher::PortMatcher;

use std::fmt;

/// Error types for audio connection operations.
#[derive(Debug)]
pub enum AudioError {
    /// No audio backend available on the system
    NoBackendAvailable { tried: Vec<String> },

    /// Port not found matching the given pattern
    PortNotFound {
        pattern: String,
        suggestions: Vec<String>,
    },

    /// Pattern matches multiple ports - user needs to be more specific
    AmbiguousMatch {
        pattern: String,
        matches: Vec<String>,
    },

    /// Connection operation failed
    ConnectionFailed {
        source: String,
        destination: String,
        reason: String,
    },

    /// Disconnection operation failed
    DisconnectionFailed {
        source: String,
        destination: String,
        reason: String,
    },

    /// External command failed to execute
    CommandFailed { command: String, reason: String },

    /// Failed to parse port list output
    ParseError { reason: String },
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::NoBackendAvailable { tried } => {
                write!(
                    f,
                    "No audio backend available (tried: {})",
                    tried.join(", ")
                )
            }
            AudioError::PortNotFound {
                pattern,
                suggestions,
            } => {
                if suggestions.is_empty() {
                    write!(f, "Port not found: '{}'", pattern)
                } else {
                    write!(
                        f,
                        "Port not found: '{}'. Did you mean: {}?",
                        pattern,
                        suggestions.join(", ")
                    )
                }
            }
            AudioError::AmbiguousMatch { pattern, matches } => {
                write!(
                    f,
                    "Ambiguous port: '{}' matches {} ports:\n  {}",
                    pattern,
                    matches.len(),
                    matches.join("\n  ")
                )
            }
            AudioError::ConnectionFailed {
                source,
                destination,
                reason,
            } => {
                write!(
                    f,
                    "Connection failed: {} -> {}: {}",
                    source, destination, reason
                )
            }
            AudioError::DisconnectionFailed {
                source,
                destination,
                reason,
            } => {
                write!(
                    f,
                    "Disconnection failed: {} -> {}: {}",
                    source, destination, reason
                )
            }
            AudioError::CommandFailed { command, reason } => {
                write!(f, "Command '{}' failed: {}", command, reason)
            }
            AudioError::ParseError { reason } => {
                write!(f, "Failed to parse port list: {}", reason)
            }
        }
    }
}

impl std::error::Error for AudioError {}

pub type Result<T> = std::result::Result<T, AudioError>;

/// Direction of an audio port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    /// Output port (audio source, can be connected FROM)
    Output,
    /// Input port (audio sink, can be connected TO)
    Input,
}

/// Represents an audio port in the system.
#[derive(Debug, Clone)]
pub struct Port {
    /// Full port name (e.g., "Vibelang:out_1")
    pub name: String,
    /// Client/device name (e.g., "Vibelang")
    pub client: String,
    /// Port name within the client (e.g., "out_1")
    pub port: String,
    /// Port direction
    pub direction: PortDirection,
    /// Ports this is connected to
    pub connections: Vec<String>,
}

impl Port {
    /// Parse a port name into client and port components.
    pub fn parse(name: &str, direction: PortDirection) -> Self {
        let (client, port) = if let Some(idx) = name.find(':') {
            (name[..idx].to_string(), name[idx + 1..].to_string())
        } else {
            (name.to_string(), String::new())
        };

        Self {
            name: name.to_string(),
            client,
            port,
            direction,
            connections: Vec::new(),
        }
    }

    /// Check if this port matches a pattern.
    pub fn matches(&self, pattern: &str) -> bool {
        PortMatcher::matches(&self.name, pattern)
    }
}
