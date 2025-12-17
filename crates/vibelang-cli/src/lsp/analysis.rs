//! Code analysis for VibeLang files.
//!
//! This module handles parsing and analyzing .vibe files to extract:
//! - Syntax errors
//! - Import statements
//! - Function calls (for synthdef validation)
//! - Symbols for completion
//! - Variable usage analysis
//! - Melody and pattern linting

use std::collections::HashSet;
use std::path::PathBuf;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range};

/// Result of analyzing a document.
#[derive(Clone, Debug, Default)]
pub struct AnalysisResult {
    /// Syntax errors from Rhai compilation.
    pub syntax_errors: Vec<Diagnostic>,
    /// Semantic diagnostics (unknown synthdefs, etc.).
    pub semantic_diagnostics: Vec<Diagnostic>,
    /// Linting diagnostics (melody/pattern issues, etc.).
    pub lint_diagnostics: Vec<Diagnostic>,
    /// Import statements found in the file.
    pub imports: Vec<ImportInfo>,
    /// Synthdef references (calls to .synth("name")).
    pub synthdef_refs: Vec<SynthdefRef>,
    /// Effect references (calls to .synth("name") on fx).
    pub effect_refs: Vec<EffectRef>,
    /// Variable definitions (let name = ...).
    pub variable_defs: Vec<VariableDef>,
}

/// Information about an import statement.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The import path as written.
    pub path: String,
    /// The range in the document.
    pub range: Range,
    /// The resolved absolute path (if it exists).
    pub resolved_path: Option<PathBuf>,
}

/// Reference to a synthdef.
#[derive(Debug, Clone)]
pub struct SynthdefRef {
    /// The synthdef name.
    pub name: String,
    /// The range of the name in the document.
    pub range: Range,
}

/// Reference to an effect.
#[derive(Debug, Clone)]
pub struct EffectRef {
    /// The effect name.
    pub name: String,
    /// The range of the name in the document.
    pub range: Range,
}

/// A variable definition (let name = ...).
#[derive(Debug, Clone)]
pub struct VariableDef {
    /// The variable name.
    pub name: String,
    /// The range of the variable name in the document.
    pub range: Range,
    /// The full range including the let keyword and value.
    pub full_range: Range,
}

/// Analyze a VibeLang document.
pub fn analyze_document(
    content: &str,
    file_path: Option<&PathBuf>,
    import_paths: &[PathBuf],
) -> AnalysisResult {
    let mut result = AnalysisResult::default();

    // Run full validation using the VibeLang core validation engine
    // This executes the script with a no-op backend to catch all errors
    let validation = vibelang_core::validation::validate_script(
        content,
        file_path.map(|p| p.as_path()),
        import_paths,
    );

    // Convert parse errors to diagnostics
    for err in &validation.parse_errors {
        result.syntax_errors.push(validation_error_to_diagnostic(err));
    }

    // Convert runtime errors to diagnostics
    for err in &validation.runtime_errors {
        result.syntax_errors.push(validation_error_to_diagnostic(err));
    }

    // Convert undefined synthdef errors to semantic diagnostics
    for undef in &validation.undefined_synthdefs {
        let line = undef.line.saturating_sub(1);
        let col = undef.column.saturating_sub(1);
        result.semantic_diagnostics.push(Diagnostic {
            range: Range {
                start: Position { line, character: col },
                end: Position { line, character: col + undef.name.len() as u32 },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("undefined-synthdef".to_string())),
            code_description: None,
            source: Some("vibelang".to_string()),
            message: format!(
                "Undefined synthdef '{}' used by voice '{}'",
                undef.name, undef.voice_name
            ),
            related_information: None,
            tags: None,
            data: None,
        });
    }

    // Parse imports, synthdef refs, effect refs, and variable definitions from the source
    // (for completion/hover support - the validation already caught errors)
    parse_imports(content, file_path, import_paths, &mut result);
    parse_synthdef_refs(content, &mut result);
    parse_effect_refs(content, &mut result);
    parse_variable_defs(content, &mut result);

    // Run linting passes
    lint_melodies(content, &mut result.lint_diagnostics);
    lint_patterns(content, &mut result.lint_diagnostics);
    lint_variables(content, &mut result.lint_diagnostics);
    lint_voice_references(content, &validation.defined_voices, &mut result.lint_diagnostics);

    result
}

/// Convert a validation error to an LSP diagnostic.
fn validation_error_to_diagnostic(error: &vibelang_core::validation::ValidationError) -> Diagnostic {
    let line = error.line.unwrap_or(1).saturating_sub(1);
    let col = error.column.unwrap_or(1).saturating_sub(1);

    // Try to estimate error length from the message
    // Common patterns: "Unknown identifier 'foo'", "Function not found: ..", etc.
    let error_len = estimate_error_length(&error.message, col);

    Diagnostic {
        range: Range {
            start: Position { line, character: col },
            end: Position { line, character: col + error_len },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("vibelang".to_string()),
        message: error.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Estimate the length of the erroneous token from error message.
fn estimate_error_length(message: &str, _col: u32) -> u32 {
    // Try to extract quoted identifiers from error messages
    // Pattern: 'identifier' or "string"
    if let Some(start) = message.find('\'') {
        if let Some(end) = message[start + 1..].find('\'') {
            let len = end as u32;
            if len > 0 && len < 50 {
                return len;
            }
        }
    }

    // Try double quotes
    if let Some(start) = message.find('"') {
        if let Some(end) = message[start + 1..].find('"') {
            let len = end as u32;
            if len > 0 && len < 50 {
                return len;
            }
        }
    }

    // For "Function not found: .." errors, highlight the operator
    if message.contains("Function not found: ..") {
        return 2; // ".." is 2 characters
    }

    // Default to a reasonable length for visibility
    10
}

/// Convert a Rhai error to an LSP diagnostic.
#[allow(dead_code)]
fn rhai_error_to_diagnostic(error: &rhai::ParseError) -> Option<Diagnostic> {
    let position = error.position();
    let (line, col) = if position.is_none() {
        (0, 0)
    } else {
        (
            position.line().unwrap_or(1).saturating_sub(1) as u32,
            position.position().unwrap_or(1).saturating_sub(1) as u32,
        )
    };

    Some(Diagnostic {
        range: Range {
            start: Position { line, character: col },
            end: Position { line, character: col + 1 },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("vibelang".to_string()),
        message: error.to_string(),
        related_information: None,
        tags: None,
        data: None,
    })
}

/// Parse import statements from the source.
fn parse_imports(
    content: &str,
    file_path: Option<&PathBuf>,
    import_paths: &[PathBuf],
    result: &mut AnalysisResult,
) {
    // Match: import "path/to/file.vibe";
    // Also handles: import "path" as alias;
    let import_pattern = regex::Regex::new(r#"import\s+"([^"]+)"(?:\s+as\s+\w+)?;"#).ok();

    if let Some(re) = import_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(path_match) = cap.get(1) {
                    let import_path = path_match.as_str().to_string();
                    let start_col = path_match.start() as u32;
                    let end_col = path_match.end() as u32;

                    // Try to resolve the import
                    let resolved = resolve_import(&import_path, file_path, import_paths);

                    result.imports.push(ImportInfo {
                        path: import_path,
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: start_col,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: end_col,
                            },
                        },
                        resolved_path: resolved,
                    });
                }
            }
        }
    }
}

/// Try to resolve an import path to an absolute path.
fn resolve_import(
    import_path: &str,
    file_path: Option<&PathBuf>,
    import_paths: &[PathBuf],
) -> Option<PathBuf> {
    // Add .vibe extension if not present
    let path_with_ext = if import_path.ends_with(".vibe") {
        import_path.to_string()
    } else {
        format!("{}.vibe", import_path)
    };

    // Try relative to current file first
    if let Some(base) = file_path.and_then(|p| p.parent()) {
        let candidate = base.join(&path_with_ext);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // Try import paths
    for import_dir in import_paths {
        let candidate = import_dir.join(&path_with_ext);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Parse synthdef references from .synth("name") calls.
fn parse_synthdef_refs(content: &str, result: &mut AnalysisResult) {
    // Match: .synth("name") or .synth('name')
    let synth_pattern = regex::Regex::new(r#"\.synth\s*\(\s*["']([^"']+)["']\s*\)"#).ok();

    if let Some(re) = synth_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    result.synthdef_refs.push(SynthdefRef {
                        name: name_match.as_str().to_string(),
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: name_match.start() as u32,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: name_match.end() as u32,
                            },
                        },
                    });
                }
            }
        }
    }
}

/// Parse effect references from fx().synth("name") calls.
fn parse_effect_refs(content: &str, result: &mut AnalysisResult) {
    // Match fx("...").synth("name") pattern
    // This is a simplified pattern - we look for .synth() that comes after fx()
    let fx_synth_pattern =
        regex::Regex::new(r#"fx\s*\([^)]*\)[^.]*\.synth\s*\(\s*["']([^"']+)["']\s*\)"#).ok();

    if let Some(re) = fx_synth_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    result.effect_refs.push(EffectRef {
                        name: name_match.as_str().to_string(),
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: name_match.start() as u32,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: name_match.end() as u32,
                            },
                        },
                    });
                }
            }
        }
    }
}

/// Parse variable definitions from let statements.
fn parse_variable_defs(content: &str, result: &mut AnalysisResult) {
    // Match: let name = ... (captures the variable name and position)
    // This handles various forms: let foo = ..., let foo_bar = ..., etc.
    let let_pattern = regex::Regex::new(r"\blet\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*=").ok();

    if let Some(re) = let_pattern {
        for (line_num, line) in content.lines().enumerate() {
            // Skip comments
            let line_without_comments = if let Some(idx) = line.find("//") {
                &line[..idx]
            } else {
                line
            };

            for cap in re.captures_iter(line_without_comments) {
                if let (Some(full_match), Some(name_match)) = (cap.get(0), cap.get(1)) {
                    let var_name = name_match.as_str().to_string();
                    let name_start = name_match.start() as u32;
                    let name_end = name_match.end() as u32;
                    let full_start = full_match.start() as u32;

                    // Try to find the end of the statement (semicolon or end of line)
                    let statement_end = line_without_comments[full_match.end()..]
                        .find(';')
                        .map(|idx| (full_match.end() + idx + 1) as u32)
                        .unwrap_or(line_without_comments.len() as u32);

                    result.variable_defs.push(VariableDef {
                        name: var_name,
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: name_start,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: name_end,
                            },
                        },
                        full_range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: full_start,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: statement_end,
                            },
                        },
                    });
                }
            }
        }
    }
}

/// Get the word at a position in the content.
pub fn get_word_at_position(content: &str, line: usize, character: usize) -> Option<String> {
    let line_content = content.lines().nth(line)?;

    // Find word boundaries
    let chars: Vec<char> = line_content.chars().collect();
    if character >= chars.len() {
        return None;
    }

    let mut start = character;
    let mut end = character;

    // Expand left
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }

    // Expand right
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    Some(chars[start..end].iter().collect())
}

/// Check if a character is part of a word.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Get the context at a position (what kind of completion is expected).
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionContext {
    /// Top-level context - suggest functions like voice(), pattern(), etc.
    TopLevel,
    /// Inside .synth() - suggest synthdef names.
    SynthdefName,
    /// Inside fx().synth() - suggest effect names.
    EffectName,
    /// Inside import "" - suggest import paths.
    ImportPath,
    /// Inside .param() - suggest parameter names.
    ParamName { synthdef: Option<String> },
    /// Inside .notes() or .step() - suggest note/pattern syntax.
    NotePattern,
    /// Inside method chain - suggest chainable methods.
    MethodChain { object_type: Option<String> },
    /// Unknown context.
    Unknown,
}

/// Determine the completion context at a position.
pub fn get_completion_context(content: &str, line: usize, character: usize) -> CompletionContext {
    let line_content = match content.lines().nth(line) {
        Some(l) => l,
        None => return CompletionContext::Unknown,
    };

    let prefix = &line_content[..character.min(line_content.len())];

    // Check for .synth(" context
    if prefix.contains(".synth(") && prefix.rfind(".synth(").map(|i| !prefix[i..].contains(")")).unwrap_or(false) {
        // Check if this is after fx()
        if prefix.contains("fx(") || prefix.contains("fx (") {
            return CompletionContext::EffectName;
        }
        return CompletionContext::SynthdefName;
    }

    // Check for import " context
    if prefix.contains("import ") && prefix.contains("\"") {
        let last_quote = prefix.rfind('"');
        let import_pos = prefix.rfind("import ");
        if let (Some(q), Some(i)) = (last_quote, import_pos) {
            if q > i && !prefix[q..].contains(";") {
                return CompletionContext::ImportPath;
            }
        }
    }

    // Check for .param(" context
    if prefix.contains(".param(") && prefix.rfind(".param(").map(|i| !prefix[i..].contains(")")).unwrap_or(false) {
        // Try to find the synthdef name from the chain
        let synthdef = extract_synthdef_from_chain(content, line);
        return CompletionContext::ParamName { synthdef };
    }

    // Check for .notes(" or .step(" context
    if (prefix.contains(".notes(") && prefix.rfind(".notes(").map(|i| !prefix[i..].contains(")")).unwrap_or(false))
        || (prefix.contains(".step(") && prefix.rfind(".step(").map(|i| !prefix[i..].contains(")")).unwrap_or(false))
    {
        return CompletionContext::NotePattern;
    }

    // Check for method chain (anything after .)
    if prefix.trim_end().ends_with('.') || (prefix.contains('.') && !prefix.ends_with(')') && !prefix.ends_with(';')) {
        let object_type = detect_object_type(prefix);
        return CompletionContext::MethodChain { object_type };
    }

    // Default to top level
    CompletionContext::TopLevel
}

/// Try to extract the synthdef name from a method chain.
fn extract_synthdef_from_chain(content: &str, line: usize) -> Option<String> {
    // Look backwards from current line for .synth("name")
    for line_idx in (0..=line).rev() {
        if let Some(line_content) = content.lines().nth(line_idx) {
            if let Some(synth_re) = regex::Regex::new(r#"\.synth\s*\(\s*["']([^"']+)["']\s*\)"#).ok() {
                if let Some(cap) = synth_re.captures(line_content) {
                    return cap.get(1).map(|m| m.as_str().to_string());
                }
            }
        }
    }
    None
}

/// Detect the object type from the prefix for method completion.
fn detect_object_type(prefix: &str) -> Option<String> {
    // Simple heuristic based on builder pattern
    if prefix.contains("voice(") || prefix.contains("voice (") {
        return Some("Voice".to_string());
    }
    if prefix.contains("pattern(") || prefix.contains("pattern (") {
        return Some("Pattern".to_string());
    }
    if prefix.contains("melody(") || prefix.contains("melody (") {
        return Some("Melody".to_string());
    }
    if prefix.contains("sequence(") || prefix.contains("sequence (") {
        return Some("Sequence".to_string());
    }
    if prefix.contains("fx(") || prefix.contains("fx (") {
        return Some("Fx".to_string());
    }
    if prefix.contains("group(") || prefix.contains("group (") || prefix.contains("define_group(") {
        return Some("Group".to_string());
    }
    if prefix.contains("fade(") || prefix.contains("fade (") {
        return Some("Fade".to_string());
    }
    if prefix.contains("sample(") || prefix.contains("sample (") {
        return Some("Sample".to_string());
    }
    None
}

// =============================================================================
// Melody and Pattern Linting
// =============================================================================

/// Valid note names (without octave).
const VALID_NOTES: [&str; 21] = [
    "C", "C#", "Cb", "D", "D#", "Db", "E", "E#", "Eb", "F", "F#", "Fb",
    "G", "G#", "Gb", "A", "A#", "Ab", "B", "B#", "Bb",
];

/// Valid chord types.
const VALID_CHORDS: [&str; 24] = [
    "maj", "min", "dim", "aug", "sus2", "sus4",
    "7", "maj7", "min7", "dim7", "aug7", "m7b5", "mM7",
    "9", "maj9", "min9", "add9",
    "11", "13",
    "6", "m6",
    "5", // power chord
    "dom7", "mmaj7",
];

/// Valid pattern tokens.
const VALID_PATTERN_TOKENS: [char; 14] = [
    'x', 'X', '.', '-', '|', '0', '1', '2', '3', '4', '5', '6', '7', '8',
];

/// Lint melody calls in the source.
pub fn lint_melodies(content: &str, diagnostics: &mut Vec<Diagnostic>) {
    // Match: .notes("...") or melody().notes("...")
    let notes_pattern = regex::Regex::new(r#"\.notes\s*\(\s*"([^"]*)"\s*\)"#).ok();

    if let Some(re) = notes_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let (Some(_full_match), Some(content_match)) = (cap.get(0), cap.get(1)) {
                    let notes_content = content_match.as_str();
                    let content_start = content_match.start() as u32;

                    // Parse and lint the melody content
                    lint_melody_content(
                        notes_content,
                        line_num as u32,
                        content_start,
                        diagnostics,
                    );
                }
            }
        }
    }
}

/// Lint the content of a .notes() call.
fn lint_melody_content(
    content: &str,
    line: u32,
    start_col: u32,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Split by whitespace and bars to get tokens
    let tokens: Vec<&str> = content
        .split(|c: char| c.is_whitespace() || c == '|')
        .filter(|s| !s.is_empty())
        .collect();

    // Count actual note/rest tokens (not just whitespace separators)
    let note_tokens: Vec<&str> = tokens
        .iter()
        .filter(|t| !t.is_empty() && **t != "|")
        .copied()
        .collect();

    // Check if token count is a multiple of 4 (common time signature)
    if !note_tokens.is_empty() && note_tokens.len() % 4 != 0 {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position { line, character: start_col },
                end: Position { line, character: start_col + content.len() as u32 },
            },
            severity: Some(DiagnosticSeverity::HINT),
            code: Some(NumberOrString::String("melody-token-count".to_string())),
            code_description: None,
            source: Some("vibelang".to_string()),
            message: format!(
                "Melody has {} tokens. Consider using a multiple of 4 for standard time signatures.",
                note_tokens.len()
            ),
            related_information: None,
            tags: None,
            data: None,
        });
    }

    // Validate each token
    let mut char_offset = 0;
    for token in content.split_whitespace() {
        // Find position of this token in the content
        if let Some(pos) = content[char_offset..].find(token) {
            let token_start = start_col + char_offset as u32 + pos as u32;
            char_offset += pos + token.len();

            // Skip bar separators
            if token == "|" {
                continue;
            }

            // Skip rest markers
            if token == "-" || token == "~" || token == "_" || token == "." {
                continue;
            }

            // Check if it's a valid note or chord
            if !is_valid_melody_token(token) {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position { line, character: token_start },
                        end: Position { line, character: token_start + token.len() as u32 },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("invalid-melody-token".to_string())),
                    code_description: None,
                    source: Some("vibelang".to_string()),
                    message: format!(
                        "Invalid melody token '{}'. Expected a note (e.g., C4, F#3), chord (e.g., C4:maj7), or rest (-, ~, .).",
                        token
                    ),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            }
        }
    }
}

/// Check if a token is a valid melody token (note, chord, or rest).
fn is_valid_melody_token(token: &str) -> bool {
    // Handle rest markers
    if token == "-" || token == "~" || token == "_" || token == "." {
        return true;
    }

    // Handle bar separators
    if token == "|" {
        return true;
    }

    // Check for chord notation: Note:ChordType (e.g., C4:maj7)
    if let Some(colon_pos) = token.find(':') {
        let note_part = &token[..colon_pos];
        let chord_part = &token[colon_pos + 1..];
        return is_valid_note(note_part) && is_valid_chord(chord_part);
    }

    // Check for simple note
    is_valid_note(token)
}

/// Check if a string is a valid note (e.g., C4, F#3, Bb5).
fn is_valid_note(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Parse note name and octave
    let chars: Vec<char> = s.chars().collect();
    let first = chars[0].to_ascii_uppercase();

    // First character must be a note letter
    if !['C', 'D', 'E', 'F', 'G', 'A', 'B'].contains(&first) {
        return false;
    }

    let mut idx = 1;

    // Check for accidental (# or b)
    if idx < chars.len() && (chars[idx] == '#' || chars[idx] == 'b') {
        idx += 1;
    }

    // Rest should be octave number (0-9)
    if idx >= chars.len() {
        // No octave is OK for scale-relative notes
        return true;
    }

    // Check octave digits
    for c in &chars[idx..] {
        if !c.is_ascii_digit() {
            return false;
        }
    }

    true
}

/// Check if a string is a valid chord type.
fn is_valid_chord(s: &str) -> bool {
    VALID_CHORDS.iter().any(|&chord| s == chord)
}

/// Lint pattern calls in the source.
pub fn lint_patterns(content: &str, diagnostics: &mut Vec<Diagnostic>) {
    // Match: .step("...") or pattern().step("...")
    let step_pattern = regex::Regex::new(r#"\.step\s*\(\s*"([^"]*)"\s*\)"#).ok();

    if let Some(re) = step_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let (Some(_full_match), Some(content_match)) = (cap.get(0), cap.get(1)) {
                    let pattern_content = content_match.as_str();
                    let content_start = content_match.start() as u32;

                    // Parse and lint the pattern content
                    lint_pattern_content(
                        pattern_content,
                        line_num as u32,
                        content_start,
                        diagnostics,
                    );
                }
            }
        }
    }
}

/// Lint the content of a .step() call.
fn lint_pattern_content(
    content: &str,
    line: u32,
    start_col: u32,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Count actual steps (exclude whitespace and bar separators)
    let steps: Vec<char> = content
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '|')
        .collect();

    // Check if step count is a multiple of 4
    if !steps.is_empty() && steps.len() % 4 != 0 {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position { line, character: start_col },
                end: Position { line, character: start_col + content.len() as u32 },
            },
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String("pattern-step-count".to_string())),
            code_description: None,
            source: Some("vibelang".to_string()),
            message: format!(
                "Pattern has {} steps (expected multiple of 4 for standard time signatures).",
                steps.len()
            ),
            related_information: None,
            tags: None,
            data: None,
        });
    }

    // Check for invalid tokens
    for (idx, c) in content.chars().enumerate() {
        // Skip whitespace and bar separators
        if c.is_whitespace() || c == '|' {
            continue;
        }

        if !VALID_PATTERN_TOKENS.contains(&c) && c != '9' {
            let char_col = start_col + idx as u32;
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line, character: char_col },
                    end: Position { line, character: char_col + 1 },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("invalid-pattern-token".to_string())),
                code_description: None,
                source: Some("vibelang".to_string()),
                message: format!(
                    "Invalid pattern token '{}'. Valid tokens: x/X (hit), . (rest), - (sustain), | (bar), 0-9 (velocity).",
                    c
                ),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }
}

/// Track variable usage and report potential typos or undefined variables.
pub fn lint_variables(content: &str, diagnostics: &mut Vec<Diagnostic>) {
    // This is a simplified variable tracker
    // It collects assignments and usages, then reports potential issues

    let mut defined_vars: HashSet<String> = HashSet::new();
    let mut var_usages: Vec<(String, u32, u32)> = Vec::new(); // (name, line, col)

    // Common VibeLang built-in functions/variables
    let builtins: HashSet<&str> = [
        // Functions
        "voice", "pattern", "melody", "sequence", "group", "define_group",
        "fx", "fade", "sample", "bpm", "transport", "print", "debug",
        "import", "true", "false", "nil", "if", "else", "while", "for",
        "loop", "break", "continue", "return", "fn", "let", "const",
        // Common methods
        "synth", "param", "notes", "step", "start", "stop", "volume",
        "pan", "send", "to_group", "length", "rate", "duration",
        // Stdlib functions
        "scale", "chord", "random", "rand", "floor", "ceil", "abs",
        "min", "max", "sin", "cos", "tan", "sqrt", "pow", "log",
    ].into_iter().collect();

    // Regex for variable assignments: let foo = ... or foo = ...
    let assign_pattern = regex::Regex::new(r"\b(let\s+)?(\w+)\s*=").ok();
    // Regex for variable usages (identifiers not followed by '(' or preceded by '.')
    let usage_pattern = regex::Regex::new(r"(?<![.\w])([a-zA-Z_]\w*)(?!\s*\()").ok();

    if let Some(assign_re) = &assign_pattern {
        for (_line_num, line) in content.lines().enumerate() {
            for cap in assign_re.captures_iter(line) {
                if let Some(var_match) = cap.get(2) {
                    let var_name = var_match.as_str().to_string();
                    // Skip if it looks like a method call
                    if !var_name.starts_with('.') {
                        defined_vars.insert(var_name);
                    }
                }
            }
        }
    }

    if let Some(usage_re) = &usage_pattern {
        for (line_num, line) in content.lines().enumerate() {
            // Skip comments
            let line_without_comments = if let Some(idx) = line.find("//") {
                &line[..idx]
            } else {
                line
            };

            for cap in usage_re.captures_iter(line_without_comments) {
                if let Some(var_match) = cap.get(1) {
                    let var_name = var_match.as_str().to_string();
                    let col = var_match.start() as u32;

                    // Skip if it's a builtin, a number, or already defined
                    if !builtins.contains(var_name.as_str())
                        && !defined_vars.contains(&var_name)
                        && !var_name.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true)
                    {
                        var_usages.push((var_name, line_num as u32, col));
                    }
                }
            }
        }
    }

    // Report undefined variable usages
    // Note: This is a simple heuristic and may have false positives
    // We use HINT severity to be less intrusive
    for (var_name, line, col) in var_usages {
        // Try to find a similar variable (possible typo)
        let similar = find_similar_variable(&var_name, &defined_vars, &builtins);

        let message = if let Some(suggestion) = similar {
            format!(
                "Unknown identifier '{}'. Did you mean '{}'?",
                var_name, suggestion
            )
        } else {
            format!(
                "Unknown identifier '{}'. It may be defined elsewhere or could be a typo.",
                var_name
            )
        };

        diagnostics.push(Diagnostic {
            range: Range {
                start: Position { line, character: col },
                end: Position { line, character: col + var_name.len() as u32 },
            },
            severity: Some(DiagnosticSeverity::HINT),
            code: Some(NumberOrString::String("unknown-identifier".to_string())),
            code_description: None,
            source: Some("vibelang".to_string()),
            message,
            related_information: None,
            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
            data: None,
        });
    }
}

/// Find a similar variable name (for typo suggestions).
fn find_similar_variable(
    name: &str,
    defined: &HashSet<String>,
    builtins: &HashSet<&str>,
) -> Option<String> {
    let name_lower = name.to_lowercase();

    // Check defined variables first
    for var in defined {
        if levenshtein_distance(&name_lower, &var.to_lowercase()) <= 2 {
            return Some(var.clone());
        }
    }

    // Check builtins
    for &builtin in builtins {
        if levenshtein_distance(&name_lower, &builtin.to_lowercase()) <= 2 {
            return Some(builtin.to_string());
        }
    }

    None
}

/// Calculate Levenshtein distance between two strings.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 { return n; }
    if n == 0 { return m; }

    let mut dp = vec![vec![0; n + 1]; m + 1];

    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[m][n]
}

// =============================================================================
// Voice Reference Linting
// =============================================================================

/// Lint .on("voice_name") calls to check if the voice exists.
pub fn lint_voice_references(
    content: &str,
    defined_voices: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Match: .on("voice_name") - string argument to .on()
    // This catches pattern().on("name") where name should be a defined voice
    let on_pattern = regex::Regex::new(r#"\.on\s*\(\s*"([^"]+)"\s*\)"#).ok();

    if let Some(re) = on_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    let voice_name = name_match.as_str();
                    let name_start = name_match.start() as u32;
                    let name_end = name_match.end() as u32;

                    // Check if this voice name is defined
                    if !defined_voices.contains(voice_name) {
                        // Find similar voice name for suggestion
                        let suggestion = defined_voices
                            .iter()
                            .find(|v| levenshtein_distance(&voice_name.to_lowercase(), &v.to_lowercase()) <= 2)
                            .cloned();

                        let message = if let Some(similar) = suggestion {
                            format!(
                                "Unknown voice '{}'. Did you mean '{}'?",
                                voice_name, similar
                            )
                        } else if defined_voices.is_empty() {
                            format!(
                                "Unknown voice '{}'. No voices are defined in this file.",
                                voice_name
                            )
                        } else {
                            format!(
                                "Unknown voice '{}'. Defined voices: {}",
                                voice_name,
                                defined_voices.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                            )
                        };

                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position {
                                    line: line_num as u32,
                                    character: name_start,
                                },
                                end: Position {
                                    line: line_num as u32,
                                    character: name_end,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(NumberOrString::String("unknown-voice".to_string())),
                            code_description: None,
                            source: Some("vibelang".to_string()),
                            message,
                            related_information: None,
                            tags: None,
                            data: None,
                        });
                    }
                }
            }
        }
    }
}
