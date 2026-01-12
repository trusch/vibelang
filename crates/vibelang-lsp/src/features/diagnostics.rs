//! Diagnostics utilities for VibeLang.

use std::collections::HashSet;
use tower_lsp::lsp_types::Diagnostic;

/// Deduplicate diagnostics based on range and message.
pub fn deduplicate_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for diag in diagnostics {
        let key = (
            diag.range.start.line,
            diag.range.start.character,
            diag.range.end.line,
            diag.range.end.character,
            diag.message.clone(),
        );

        if seen.insert(key) {
            result.push(diag);
        }
    }

    result
}

/// Sort diagnostics by position.
pub fn sort_diagnostics(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by(|a, b| {
        a.range
            .start
            .line
            .cmp(&b.range.start.line)
            .then_with(|| a.range.start.character.cmp(&b.range.start.character))
    });
    diagnostics
}
