//! Source location tracking for entities.
//!
//! Tracks where entities (voices, patterns, etc.) are defined in source files.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Source location in a script file.
///
/// Used to track where entities are defined for:
/// - Click-to-navigate in editors (Emacs, VS Code)
/// - Error messages with file:line info
/// - Debugging and hot-reload context
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceLocation {
    /// Path to the source file (absolute or relative to project root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    /// Line number (1-indexed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Column number (1-indexed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

impl SourceLocation {
    /// Create a new source location.
    pub fn new(file: Option<PathBuf>, line: Option<u32>, column: Option<u32>) -> Self {
        Self { file, line, column }
    }

    /// Create a source location with file and line.
    pub fn at(file: impl Into<PathBuf>, line: u32) -> Self {
        Self {
            file: Some(file.into()),
            line: Some(line),
            column: None,
        }
    }

    /// Create a source location with file, line, and column.
    pub fn at_column(file: impl Into<PathBuf>, line: u32, column: u32) -> Self {
        Self {
            file: Some(file.into()),
            line: Some(line),
            column: Some(column),
        }
    }

    /// Returns true if this location has any information.
    pub fn is_known(&self) -> bool {
        self.file.is_some() || self.line.is_some()
    }

    /// Format as "file:line:column" or "file:line" or just the components available.
    pub fn display(&self) -> String {
        match (&self.file, self.line, self.column) {
            (Some(f), Some(l), Some(c)) => format!("{}:{}:{}", f.display(), l, c),
            (Some(f), Some(l), None) => format!("{}:{}", f.display(), l),
            (Some(f), None, _) => f.display().to_string(),
            (None, Some(l), Some(c)) => format!("line {}:{}", l, c),
            (None, Some(l), None) => format!("line {}", l),
            (None, None, _) => "<unknown>".to_string(),
        }
    }
}

impl std::fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_location_display() {
        let loc = SourceLocation::at("main.vibe", 42);
        assert_eq!(loc.display(), "main.vibe:42");

        let loc = SourceLocation::at_column("main.vibe", 42, 5);
        assert_eq!(loc.display(), "main.vibe:42:5");

        let loc = SourceLocation::default();
        assert_eq!(loc.display(), "<unknown>");
    }
}
