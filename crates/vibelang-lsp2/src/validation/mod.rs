//! Validation engine for VibeLang LSP.
//!
//! This module provides script validation using vibelang-rhai.
//! Unlike the old LSP, it doesn't need a mock backend - the ScriptEngine
//! collects state without requiring audio output.
//!
//! # Architecture
//!
//! The validation engine:
//! 1. Parses scripts using Rhai's parser to get syntax errors
//! 2. Optionally executes scripts to get runtime errors
//! 3. Provides detailed diagnostics with line/column information
//!
//! # Example
//!
//! ```ignore
//! use vibelang_lsp2::validation::ValidationEngine;
//!
//! let engine = ValidationEngine::new();
//! let result = engine.validate("set_tempo(120);");
//! ```

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use vibelang_rhai::ScriptEngine;

/// Result of script validation.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// Syntax errors from parsing.
    pub syntax_errors: Vec<Diagnostic>,
    /// Runtime errors from execution.
    pub runtime_errors: Vec<Diagnostic>,
    /// Warnings from validation.
    pub warnings: Vec<Diagnostic>,
    /// Information messages.
    pub info: Vec<Diagnostic>,
    /// Whether the script is valid (no syntax or runtime errors).
    pub is_valid: bool,
}

impl ValidationResult {
    /// Get all diagnostics combined.
    pub fn all_diagnostics(&self) -> Vec<Diagnostic> {
        let mut all = Vec::new();
        all.extend(self.syntax_errors.clone());
        all.extend(self.runtime_errors.clone());
        all.extend(self.warnings.clone());
        all.extend(self.info.clone());
        all
    }

    /// Get only errors (syntax + runtime).
    pub fn errors(&self) -> Vec<Diagnostic> {
        let mut errors = Vec::new();
        errors.extend(self.syntax_errors.clone());
        errors.extend(self.runtime_errors.clone());
        errors
    }
}

/// Validation engine for VibeLang scripts.
///
/// Uses vibelang-rhai's ScriptEngine to parse and validate scripts.
pub struct ValidationEngine {
    /// Inner script engine (wrapped in Mutex for interior mutability).
    engine: Mutex<ScriptEngine>,
    /// Import paths for module resolution.
    import_paths: Vec<PathBuf>,
}

impl Default for ValidationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationEngine {
    /// Create a new validation engine.
    pub fn new() -> Self {
        Self {
            engine: Mutex::new(ScriptEngine::new()),
            import_paths: Vec::new(),
        }
    }

    /// Add an import path for module resolution.
    pub fn add_import_path(&mut self, path: impl Into<PathBuf>) {
        self.import_paths.push(path.into());
    }

    /// Validate a script from a string.
    ///
    /// This performs both parsing and execution to catch all errors.
    pub fn validate(&self, script: &str) -> ValidationResult {
        let mut result = ValidationResult::default();

        // Try to lock the engine
        let mut engine = match self.engine.lock() {
            Ok(e) => e,
            Err(_) => {
                result.runtime_errors.push(create_diagnostic(
                    0,
                    0,
                    "Internal error: validation engine lock failed",
                    DiagnosticSeverity::ERROR,
                ));
                return result;
            }
        };

        // First, try to compile to catch syntax errors
        match engine.engine().compile(script) {
            Ok(_ast) => {
                // Script parsed successfully, now try to execute
                match engine.execute(script) {
                    Ok(state) => {
                        // Execution succeeded - add warnings for potential issues
                        result.warnings.extend(check_state_warnings(&state));
                        result.is_valid = true;
                    }
                    Err(err) => {
                        // Runtime error
                        result
                            .runtime_errors
                            .push(rhai_error_to_diagnostic(&err.to_string()));
                    }
                }
            }
            Err(err) => {
                // Syntax error
                result
                    .syntax_errors
                    .push(parse_error_to_diagnostic(&err.to_string()));
            }
        }

        result
    }

    /// Validate a script file.
    pub fn validate_file(&self, path: impl AsRef<Path>) -> ValidationResult {
        let path = path.as_ref();

        // Read the file
        let script = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                let mut result = ValidationResult::default();
                result.syntax_errors.push(create_diagnostic(
                    0,
                    0,
                    &format!("Failed to read file: {}", err),
                    DiagnosticSeverity::ERROR,
                ));
                return result;
            }
        };

        // Validate with file context
        self.validate_with_context(&script, Some(path.to_path_buf()))
    }

    /// Validate a script with file context.
    pub fn validate_with_context(&self, script: &str, file: Option<PathBuf>) -> ValidationResult {
        let mut result = ValidationResult::default();

        // Create a fresh engine for file-based validation to handle imports
        let mut engine = ScriptEngine::new();

        // Add import paths
        for path in &self.import_paths {
            engine.add_import_path(path.clone());
        }

        // Add file's parent directory as import path
        if let Some(ref file_path) = file {
            if let Some(parent) = file_path.parent() {
                engine.add_import_path(parent.to_path_buf());
            }
        }

        // First, try to compile
        match engine.compile(script) {
            Ok(_ast) => {
                // Script parsed successfully, now try to execute
                match engine.execute(script) {
                    Ok(state) => {
                        // Execution succeeded
                        result.warnings.extend(check_state_warnings(&state));
                        result.is_valid = true;
                    }
                    Err(err) => {
                        result
                            .runtime_errors
                            .push(rhai_error_to_diagnostic(&err.to_string()));
                    }
                }
            }
            Err(err) => {
                result
                    .syntax_errors
                    .push(parse_error_to_diagnostic(&err.to_string()));
            }
        }

        result
    }

    /// Quickly check syntax only (faster, no execution).
    pub fn check_syntax(&self, script: &str) -> ValidationResult {
        let mut result = ValidationResult::default();

        let engine = match self.engine.lock() {
            Ok(e) => e,
            Err(_) => return result,
        };

        match engine.engine().compile(script) {
            Ok(_) => {
                result.is_valid = true;
            }
            Err(err) => {
                result
                    .syntax_errors
                    .push(parse_error_to_diagnostic(&err.to_string()));
            }
        }

        result
    }
}

/// Convert a Rhai parse error string to a diagnostic.
fn parse_error_to_diagnostic(error: &str) -> Diagnostic {
    // Parse error format: "... (line X, position Y)"
    let (line, col) = parse_position_from_error(error);

    create_diagnostic(line, col, error, DiagnosticSeverity::ERROR)
}

/// Convert a Rhai runtime error string to a diagnostic.
fn rhai_error_to_diagnostic(error: &str) -> Diagnostic {
    let (line, col) = parse_position_from_error(error);

    create_diagnostic(line, col, error, DiagnosticSeverity::ERROR)
}

/// Parse line and column from a Rhai error message.
fn parse_position_from_error(error: &str) -> (u32, u32) {
    // Rhai errors often contain "(line X, position Y)" or "at line X"
    let line_re = regex::Regex::new(r"line\s+(\d+)").ok();
    let pos_re = regex::Regex::new(r"position\s+(\d+)").ok();

    let line = line_re
        .and_then(|r| r.captures(error))
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .map(|l| l.saturating_sub(1)) // Convert to 0-based
        .unwrap_or(0);

    let col = pos_re
        .and_then(|r| r.captures(error))
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .map(|c| c.saturating_sub(1)) // Convert to 0-based
        .unwrap_or(0);

    (line, col)
}

/// Create a diagnostic at a specific position.
fn create_diagnostic(
    line: u32,
    col: u32,
    message: &str,
    severity: DiagnosticSeverity,
) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line,
                character: col,
            },
            end: Position {
                line,
                character: col + 1,
            },
        },
        severity: Some(severity),
        code: None,
        code_description: None,
        source: Some("vibelang".to_string()),
        message: message.to_string(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Check the script state for potential warnings.
fn check_state_warnings(state: &vibelang_core2::reload::ScriptState) -> Vec<Diagnostic> {
    let mut warnings = Vec::new();

    // Warning: Very high or low tempo
    if state.tempo < 20.0 {
        warnings.push(create_diagnostic(
            0,
            0,
            &format!(
                "Very low tempo ({} BPM) may cause timing issues",
                state.tempo
            ),
            DiagnosticSeverity::WARNING,
        ));
    } else if state.tempo > 300.0 {
        warnings.push(create_diagnostic(
            0,
            0,
            &format!(
                "Very high tempo ({} BPM) may cause timing issues",
                state.tempo
            ),
            DiagnosticSeverity::WARNING,
        ));
    }

    // Warning: No voices defined
    if state.voices.is_empty() && !state.patterns.is_empty() {
        warnings.push(create_diagnostic(
            0,
            0,
            "Patterns defined but no voices - patterns need voices to play",
            DiagnosticSeverity::WARNING,
        ));
    }

    // Warning: Voices without patterns or melodies
    if !state.voices.is_empty()
        && state.patterns.is_empty()
        && state.melodies.is_empty()
        && state.sequences.is_empty()
    {
        warnings.push(create_diagnostic(
            0,
            0,
            "Voices defined but no patterns or melodies - nothing will play",
            DiagnosticSeverity::HINT,
        ));
    }

    // Warning: Patterns/melodies without apply
    // Note: This check would need more context from analysis

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_simple_script() {
        let engine = ValidationEngine::new();
        let result = engine.validate("set_tempo(120);");
        assert!(result.is_valid, "Simple script should be valid");
        assert!(result.syntax_errors.is_empty());
        assert!(result.runtime_errors.is_empty());
    }

    #[test]
    fn test_validate_syntax_error() {
        let engine = ValidationEngine::new();
        let result = engine.validate("set_tempo("); // Missing closing paren
        assert!(!result.is_valid);
        assert!(!result.syntax_errors.is_empty());
    }

    #[test]
    fn test_validate_runtime_error() {
        let engine = ValidationEngine::new();
        let result = engine.validate("undefined_function();");
        assert!(!result.is_valid);
        // This should be a runtime error (undefined function)
        assert!(
            !result.syntax_errors.is_empty() || !result.runtime_errors.is_empty(),
            "Should have some error"
        );
    }

    #[test]
    fn test_validate_complex_script() {
        let engine = ValidationEngine::new();
        let result = engine.validate(
            r#"
            set_tempo(128);
            set_time_signature(4, 4);

            define_group("Drums", || {
                let kick = voice("kick").synth("kick_909");
                pattern("kick_main").on(kick).step("x... x...").apply();
            });
        "#,
        );
        assert!(result.is_valid, "Complex script should be valid");
    }

    #[test]
    fn test_check_syntax_only() {
        let engine = ValidationEngine::new();

        // Valid syntax
        let result = engine.check_syntax("let x = 5;");
        assert!(result.is_valid);

        // Invalid syntax
        let result = engine.check_syntax("let x = ;");
        assert!(!result.is_valid);
    }

    #[test]
    fn test_parse_position_from_error() {
        let (line, col) = parse_position_from_error("Error at line 5, position 10");
        assert_eq!(line, 4); // 0-based
        assert_eq!(col, 9); // 0-based
    }

    #[test]
    fn test_tempo_warnings() {
        let engine = ValidationEngine::new();

        // Very low tempo
        let result = engine.validate("set_tempo(10);");
        assert!(result.is_valid);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.message.contains("low tempo")),
            "Should warn about low tempo"
        );

        // Very high tempo
        let result = engine.validate("set_tempo(500);");
        assert!(result.is_valid);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.message.contains("high tempo")),
            "Should warn about high tempo"
        );
    }
}
