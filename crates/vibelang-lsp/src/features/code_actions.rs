//! Code actions provider for VibeLang.
//!
//! Provides quick fixes and refactoring actions.

use std::collections::HashMap;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic, Position, Range, TextEdit, Url,
    WorkspaceEdit,
};

use crate::analysis::AnalysisResult;

/// Get code actions for the given range.
pub fn get_code_actions(
    uri: &Url,
    range: Range,
    diagnostics: &[Diagnostic],
    content: &str,
    analysis: Option<&AnalysisResult>,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    // Quick fixes for diagnostics
    for diagnostic in diagnostics {
        if let Some(fix) = get_quick_fix(uri, diagnostic, content) {
            actions.push(CodeActionOrCommand::CodeAction(fix));
        }
    }

    // Refactoring actions based on selection
    actions.extend(get_refactoring_actions(uri, range, content, analysis));

    // Source actions (organize, format, etc.)
    actions.extend(get_source_actions(uri, content));

    actions
}

/// Get a quick fix for a diagnostic if available.
fn get_quick_fix(uri: &Url, diagnostic: &Diagnostic, content: &str) -> Option<CodeAction> {
    let message = &diagnostic.message;

    // Fix: Missing .apply() on pattern/melody
    if message.contains("may not trigger") || message.contains("needs .apply()") {
        return Some(create_apply_fix(uri, diagnostic, content));
    }

    // Fix: Unused variable
    if message.contains("unused") {
        if let Some(var_name) = extract_variable_name(message) {
            return Some(create_prefix_underscore_fix(uri, diagnostic, &var_name));
        }
    }

    // Fix: Unknown synthdef - suggest similar names
    if message.contains("synthdef not found") || message.contains("unknown synthdef") {
        if let Some(name) = extract_quoted_name(message) {
            return Some(create_synthdef_suggestion_fix(uri, diagnostic, &name));
        }
    }

    // Fix: Missing import
    if message.contains("not defined") || message.contains("undefined") {
        if let Some(name) = extract_quoted_name(message) {
            return Some(create_import_fix(uri, &name, content));
        }
    }

    None
}

/// Create a fix that adds .apply() to a pattern/melody.
fn create_apply_fix(uri: &Url, diagnostic: &Diagnostic, content: &str) -> CodeAction {
    let line_num = diagnostic.range.start.line as usize;
    let line = content.lines().nth(line_num).unwrap_or("");

    // Find the end of the statement (before semicolon or end of line)
    let end_char = line
        .find(';')
        .or_else(|| line.find("//"))
        .unwrap_or(line.trim_end().len());

    let edit_pos = Position {
        line: diagnostic.range.start.line,
        character: end_char as u32,
    };

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: edit_pos,
                end: edit_pos,
            },
            new_text: ".apply()".to_string(),
        }],
    );

    CodeAction {
        title: "Add .apply() to trigger playback".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }
}

/// Create a fix that prefixes a variable with underscore.
fn create_prefix_underscore_fix(uri: &Url, diagnostic: &Diagnostic, var_name: &str) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: diagnostic.range,
            new_text: format!("_{}", var_name),
        }],
    );

    CodeAction {
        title: format!("Rename to _{} to indicate intentionally unused", var_name),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }
}

/// Create a fix suggesting similar synthdef names.
fn create_synthdef_suggestion_fix(uri: &Url, diagnostic: &Diagnostic, name: &str) -> CodeAction {
    let suggestions = get_similar_synthdefs(name);

    if let Some(best) = suggestions.first() {
        let mut changes = HashMap::new();
        changes.insert(
            uri.clone(),
            vec![TextEdit {
                range: diagnostic.range,
                new_text: format!("\"{}\"", best),
            }],
        );

        CodeAction {
            title: format!("Did you mean '{}'?", best),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }),
            command: None,
            is_preferred: Some(true),
            disabled: None,
            data: None,
        }
    } else {
        CodeAction {
            title: "No similar synthdefs found".to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: None,
            command: None,
            is_preferred: Some(false),
            disabled: Some(tower_lsp::lsp_types::CodeActionDisabled {
                reason: "No suggestions available".to_string(),
            }),
            data: None,
        }
    }
}

/// Create an import fix for undefined symbols.
fn create_import_fix(uri: &Url, name: &str, content: &str) -> CodeAction {
    // Find the first non-comment, non-empty line to insert import
    let insert_line = content
        .lines()
        .enumerate()
        .find(|(_, line)| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//")
        })
        .map(|(i, _)| i)
        .unwrap_or(0);

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: Position {
                    line: insert_line as u32,
                    character: 0,
                },
                end: Position {
                    line: insert_line as u32,
                    character: 0,
                },
            },
            new_text: format!("import \"{}\";\n", guess_import_path(name)),
        }],
    );

    CodeAction {
        title: format!("Add import for '{}'", name),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }
}

/// Get refactoring actions based on selection.
fn get_refactoring_actions(
    uri: &Url,
    range: Range,
    content: &str,
    _analysis: Option<&AnalysisResult>,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    // Get selected text
    let selected = get_text_in_range(content, range);

    // Extract to pattern (if selection looks like a pattern notation)
    if looks_like_pattern(&selected) {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Extract to pattern".to_string(),
            kind: Some(CodeActionKind::REFACTOR_EXTRACT),
            diagnostics: None,
            edit: None, // Would need to be computed
            command: None,
            is_preferred: Some(false),
            disabled: None,
            data: Some(serde_json::json!({
                "type": "extract_pattern",
                "uri": uri.to_string(),
                "range": range,
                "text": selected
            })),
        }));
    }

    // Extract to melody (if selection looks like note notation)
    if looks_like_melody(&selected) {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Extract to melody".to_string(),
            kind: Some(CodeActionKind::REFACTOR_EXTRACT),
            diagnostics: None,
            edit: None,
            command: None,
            is_preferred: Some(false),
            disabled: None,
            data: Some(serde_json::json!({
                "type": "extract_melody",
                "uri": uri.to_string(),
                "range": range,
                "text": selected
            })),
        }));
    }

    // Extract to voice (if selection contains voice-like code)
    if selected.contains("voice(") || selected.contains(".play(") {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Extract to group".to_string(),
            kind: Some(CodeActionKind::REFACTOR_EXTRACT),
            diagnostics: None,
            edit: None,
            command: None,
            is_preferred: Some(false),
            disabled: None,
            data: Some(serde_json::json!({
                "type": "extract_group",
                "uri": uri.to_string(),
                "range": range,
                "text": selected
            })),
        }));
    }

    actions
}

/// Get source-level actions (organize imports, etc.).
fn get_source_actions(uri: &Url, content: &str) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    // Organize imports
    if content.contains("import ") {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Organize imports".to_string(),
            kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
            diagnostics: None,
            edit: None, // Would need to be computed
            command: None,
            is_preferred: Some(false),
            disabled: None,
            data: Some(serde_json::json!({
                "type": "organize_imports",
                "uri": uri.to_string()
            })),
        }));
    }

    // Add standard imports
    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Add standard imports".to_string(),
        kind: Some(CodeActionKind::SOURCE),
        diagnostics: None,
        edit: Some(create_standard_imports_edit(uri, content)),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }));

    actions
}

/// Create edit to add standard imports.
fn create_standard_imports_edit(uri: &Url, content: &str) -> WorkspaceEdit {
    let has_stdlib_import = content.contains(r#"import "stdlib""#);

    if has_stdlib_import {
        return WorkspaceEdit {
            changes: Some(HashMap::new()),
            document_changes: None,
            change_annotations: None,
        };
    }

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            new_text: "import \"stdlib\";\n".to_string(),
        }],
    );

    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

// Helper functions

fn extract_variable_name(message: &str) -> Option<String> {
    // Extract variable name from messages like "unused variable 'foo'"
    let re = regex::Regex::new(r"'(\w+)'").ok()?;
    re.captures(message)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_quoted_name(message: &str) -> Option<String> {
    let re = regex::Regex::new(r#"['"]([^'"]+)['"]"#).ok()?;
    re.captures(message)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn get_similar_synthdefs(name: &str) -> Vec<String> {
    // Common synthdefs to suggest
    let known = vec![
        "kick", "snare", "hihat", "clap", "bass", "lead", "pad", "pluck", "sine", "saw", "square",
        "triangle", "noise", "fm", "am", "sampler", "sfz", "granular",
    ];

    // Simple fuzzy matching - find names that share characters
    let name_lower = name.to_lowercase();
    let mut matches: Vec<_> = known
        .iter()
        .filter(|k| {
            let k_lower = k.to_lowercase();
            k_lower.contains(&name_lower)
                || name_lower.contains(&k_lower)
                || levenshtein_distance(&name_lower, &k_lower) <= 2
        })
        .map(|s| s.to_string())
        .collect();

    matches.sort_by_key(|m| levenshtein_distance(&name.to_lowercase(), &m.to_lowercase()));
    matches.truncate(3);
    matches
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    let mut matrix = vec![vec![0usize; b.len() + 1]; a.len() + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        matrix[0][j] = j;
    }

    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a.len()][b.len()]
}

fn guess_import_path(name: &str) -> String {
    // Map common names to likely import paths
    match name.to_lowercase().as_str() {
        n if n.contains("kick") || n.contains("snare") || n.contains("hihat") => {
            "drums".to_string()
        }
        n if n.contains("bass") => "bass".to_string(),
        n if n.contains("lead") || n.contains("synth") => "synths".to_string(),
        n if n.contains("pad") => "pads".to_string(),
        n if n.contains("fx") || n.contains("effect") => "effects".to_string(),
        _ => format!("{}.vibe", name),
    }
}

fn get_text_in_range(content: &str, range: Range) -> String {
    let lines: Vec<&str> = content.lines().collect();

    if range.start.line == range.end.line {
        let line = lines.get(range.start.line as usize).unwrap_or(&"");
        let start = (range.start.character as usize).min(line.len());
        let end = (range.end.character as usize).min(line.len());
        return line[start..end].to_string();
    }

    let mut result = String::new();
    for line_num in range.start.line..=range.end.line {
        if let Some(line) = lines.get(line_num as usize) {
            if line_num == range.start.line {
                let start = (range.start.character as usize).min(line.len());
                result.push_str(&line[start..]);
            } else if line_num == range.end.line {
                let end = (range.end.character as usize).min(line.len());
                result.push_str(&line[..end]);
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }
    }

    result
}

fn looks_like_pattern(text: &str) -> bool {
    // Pattern notation contains x, -, or numbers with spacing
    let pattern_chars = ['x', 'X', '-', '.', '|'];
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.len() < 100
        && pattern_chars.iter().any(|c| trimmed.contains(*c))
        && !trimmed.contains("fn ")
        && !trimmed.contains("let ")
}

fn looks_like_melody(text: &str) -> bool {
    // Melody notation contains note names like c4, d#5, etc.
    let re = regex::Regex::new(r"[a-gA-G][#b]?\d").ok();
    re.map(|r| r.is_match(text)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_pattern() {
        assert!(looks_like_pattern("x-x- x-x-"));
        assert!(looks_like_pattern("xxxx ----"));
        assert!(!looks_like_pattern("let x = 5"));
        assert!(!looks_like_pattern("fn foo() {}"));
    }

    #[test]
    fn test_looks_like_melody() {
        assert!(looks_like_melody("c4 e4 g4"));
        assert!(looks_like_melody("C#5 Db3"));
        assert!(!looks_like_melody("hello world"));
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein_distance("kick", "kick"), 0);
        assert_eq!(levenshtein_distance("kick", "kik"), 1);
        assert_eq!(levenshtein_distance("kick", "pike"), 3); // k→p, c→k, k→e
        assert_eq!(levenshtein_distance("kick", "lick"), 1); // k→l
    }
}
