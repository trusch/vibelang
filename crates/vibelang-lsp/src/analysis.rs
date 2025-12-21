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
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range,
};

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
    /// Local synthdef definitions (define_synthdef("name")).
    pub local_synthdefs: HashSet<String>,
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
}

/// Analyze a VibeLang document.
pub fn analyze_document(
    content: &str,
    file_path: Option<&PathBuf>,
    import_paths: &[PathBuf],
) -> AnalysisResult {
    let mut result = AnalysisResult::default();

    // Run full validation using the VibeLang core validation engine
    let validation = vibelang_core::validation::validate_script(
        content,
        file_path.map(|p| p.as_path()),
        import_paths,
    );

    // Convert parse errors to diagnostics
    for err in &validation.parse_errors {
        result
            .syntax_errors
            .push(validation_error_to_diagnostic(err));
    }

    // Convert runtime errors to diagnostics
    for err in &validation.runtime_errors {
        result
            .syntax_errors
            .push(validation_error_to_diagnostic(err));
    }

    // Convert undefined synthdef errors to semantic diagnostics
    for undef in &validation.undefined_synthdefs {
        let line = undef.line.saturating_sub(1);
        let col = undef.column.saturating_sub(1);
        result.semantic_diagnostics.push(Diagnostic {
            range: Range {
                start: Position { line, character: col },
                end: Position {
                    line,
                    character: col + undef.name.len() as u32,
                },
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

    // Parse imports, synthdef refs, effect refs, variable definitions, and local synthdef definitions
    parse_imports(content, file_path, import_paths, &mut result);
    parse_synthdef_refs(content, &mut result);
    parse_effect_refs(content, &mut result);
    parse_variable_defs(content, &mut result);
    parse_local_synthdefs(content, &mut result);

    // Run linting passes
    lint_melodies(content, &mut result.lint_diagnostics);
    lint_patterns(content, &mut result.lint_diagnostics);
    lint_variables(content, &mut result.lint_diagnostics);

    // Combine runtime-collected voice names with regex-parsed voice definitions
    // This ensures voices defined without .apply() or .run() are still recognized
    let mut all_voices = validation.defined_voices.clone();
    all_voices.extend(parse_voice_definitions(content));
    lint_voice_references(content, &all_voices, &mut result.lint_diagnostics);

    result
}

/// Convert a validation error to an LSP diagnostic.
fn validation_error_to_diagnostic(
    error: &vibelang_core::validation::ValidationError,
) -> Diagnostic {
    let line = error.line.unwrap_or(1).saturating_sub(1);
    let col = error.column.unwrap_or(1).saturating_sub(1);
    let error_len = estimate_error_length(&error.message, col);

    Diagnostic {
        range: Range {
            start: Position { line, character: col },
            end: Position {
                line,
                character: col + error_len,
            },
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

    // Default to a reasonable length for visibility
    10
}

/// Parse import statements from the source.
fn parse_imports(
    content: &str,
    file_path: Option<&PathBuf>,
    import_paths: &[PathBuf],
    result: &mut AnalysisResult,
) {
    let import_pattern = regex::Regex::new(r#"import\s+"([^"]+)"(?:\s+as\s+\w+)?;"#).ok();

    if let Some(re) = import_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(path_match) = cap.get(1) {
                    let import_path = path_match.as_str().to_string();
                    let start_col = path_match.start() as u32;
                    let end_col = path_match.end() as u32;

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

/// Parse local synthdef definitions from define_synthdef("name") calls.
fn parse_local_synthdefs(content: &str, result: &mut AnalysisResult) {
    let synthdef_pattern = regex::Regex::new(r#"define_synthdef\s*\(\s*["']([^"']+)["']"#).ok();

    if let Some(re) = synthdef_pattern {
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    result.local_synthdefs.insert(name_match.as_str().to_string());
                }
            }
        }
    }
}

/// Parse voice definitions from voice("name") calls.
/// This is used to supplement the runtime-collected voice names since voices
/// may be defined without calling .apply() or .run().
fn parse_voice_definitions(content: &str) -> HashSet<String> {
    let mut voices = HashSet::new();
    let voice_pattern = regex::Regex::new(r#"voice\s*\(\s*["']([^"']+)["']\s*\)"#).ok();

    if let Some(re) = voice_pattern {
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    voices.insert(name_match.as_str().to_string());
                }
            }
        }
    }

    voices
}

/// Parse variable definitions from let statements.
fn parse_variable_defs(content: &str, result: &mut AnalysisResult) {
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
                if let Some(name_match) = cap.get(1) {
                    let var_name = name_match.as_str().to_string();
                    let name_start = name_match.start() as u32;
                    let name_end = name_match.end() as u32;

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
                    });
                }
            }
        }
    }
}

/// Get the word at a position in the content.
pub fn get_word_at_position(content: &str, line: usize, character: usize) -> Option<String> {
    let line_content = content.lines().nth(line)?;
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

/// Completion context types.
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
    if prefix.contains(".synth(")
        && prefix
            .rfind(".synth(")
            .map(|i| !prefix[i..].contains(")"))
            .unwrap_or(false)
    {
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
    if prefix.contains(".param(")
        && prefix
            .rfind(".param(")
            .map(|i| !prefix[i..].contains(")"))
            .unwrap_or(false)
    {
        let synthdef = extract_synthdef_from_chain(content, line);
        return CompletionContext::ParamName { synthdef };
    }

    // Check for .notes(" or .step(" context
    if (prefix.contains(".notes(")
        && prefix
            .rfind(".notes(")
            .map(|i| !prefix[i..].contains(")"))
            .unwrap_or(false))
        || (prefix.contains(".step(")
            && prefix
                .rfind(".step(")
                .map(|i| !prefix[i..].contains(")"))
                .unwrap_or(false))
    {
        return CompletionContext::NotePattern;
    }

    // Check for method chain (anything after .)
    if prefix.trim_end().ends_with('.')
        || (prefix.contains('.') && !prefix.ends_with(')') && !prefix.ends_with(';'))
    {
        let object_type = detect_object_type(prefix);
        return CompletionContext::MethodChain { object_type };
    }

    // Default to top level
    CompletionContext::TopLevel
}

/// Try to extract the synthdef name from a method chain.
fn extract_synthdef_from_chain(content: &str, line: usize) -> Option<String> {
    let synth_re = regex::Regex::new(r#"\.synth\s*\(\s*["']([^"']+)["']\s*\)"#).ok()?;
    for line_idx in (0..=line).rev() {
        if let Some(line_content) = content.lines().nth(line_idx) {
            if let Some(cap) = synth_re.captures(line_content) {
                return cap.get(1).map(|m| m.as_str().to_string());
            }
        }
    }
    None
}

/// Detect the object type from the prefix for method completion.
fn detect_object_type(prefix: &str) -> Option<String> {
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

/// Valid chord types (includes aliases like m7 for min7).
const VALID_CHORDS: [&str; 32] = [
    "maj", "major", "min", "minor", "dim", "aug", "sus2", "sus4", "7", "maj7", "major7", "min7",
    "m7", "dim7", "aug7", "m7b5", "half-dim", "mM7", "mmaj7", "minmaj7", "9", "maj9", "min9",
    "add9", "11", "13", "6", "m6", "min6", "5", "power", "dom7",
];

/// Valid pattern tokens.
const VALID_PATTERN_TOKENS: [char; 14] = [
    'x', 'X', '.', '-', '|', '0', '1', '2', '3', '4', '5', '6', '7', '8',
];

/// Lint melody calls in the source.
pub fn lint_melodies(content: &str, diagnostics: &mut Vec<Diagnostic>) {
    let notes_pattern = regex::Regex::new(r#"\.notes\s*\(\s*"([^"]*)"\s*\)"#).ok();
    let lines: Vec<&str> = content.lines().collect();

    if let Some(re) = notes_pattern {
        for (line_num, line) in lines.iter().enumerate() {
            for cap in re.captures_iter(line) {
                if let (Some(_full_match), Some(content_match)) = (cap.get(0), cap.get(1)) {
                    let notes_content = content_match.as_str();
                    let content_start = content_match.start() as u32;

                    // Check if notes content contains scale degrees (numbers 1-7)
                    let uses_scale_degrees = notes_content
                        .split(|c: char| c.is_whitespace() || c == '|')
                        .filter(|s| !s.is_empty())
                        .any(|token| {
                            // Check if token starts with a digit or is a scale degree
                            let base = if let Some(colon) = token.find(':') {
                                &token[..colon]
                            } else {
                                token
                            };
                            is_valid_scale_degree(base)
                        });

                    // If scale degrees are used, check for .scale() and .root() in the method chain
                    // Look back up to 10 lines to find the start of the melody() statement
                    if uses_scale_degrees {
                        let context = get_melody_chain_context(&lines, line_num);
                        let has_scale = context.contains(".scale(");
                        let has_root = context.contains(".root(");

                        if !has_scale || !has_root {
                            let missing = match (has_scale, has_root) {
                                (false, false) => ".scale() and .root()",
                                (false, true) => ".scale()",
                                (true, false) => ".root()",
                                _ => "",
                            };

                            diagnostics.push(Diagnostic {
                                range: Range {
                                    start: Position {
                                        line: line_num as u32,
                                        character: content_start,
                                    },
                                    end: Position {
                                        line: line_num as u32,
                                        character: content_start + notes_content.len() as u32,
                                    },
                                },
                                severity: Some(DiagnosticSeverity::WARNING),
                                code: Some(NumberOrString::String("missing-scale-root".to_string())),
                                code_description: None,
                                source: Some("vibelang".to_string()),
                                message: format!(
                                    "Scale degree numbers (1-7) are used but {} is missing. Add {}.scale(\"major\").root(\"C4\") to specify the tonality.",
                                    missing, missing
                                ),
                                related_information: None,
                                tags: None,
                                data: None,
                            });
                        }
                    }

                    lint_melody_content(notes_content, line_num as u32, content_start, diagnostics);
                }
            }
        }
    }
}

/// Get the context of a melody method chain by looking back for melody() and forward until semicolon.
/// Returns only the non-commented parts of the code.
fn get_melody_chain_context(lines: &[&str], current_line: usize) -> String {
    let mut context = String::new();
    let max_lookback = 15;

    // Look back to find the start of the melody chain (melody() call)
    let start_line = current_line.saturating_sub(max_lookback);

    let mut found_melody_start = false;
    for i in (start_line..=current_line).rev() {
        let line = lines[i];
        // Strip comments before checking for melody() and adding to context
        let line_without_comments = strip_line_comment(line);
        if line_without_comments.contains("melody(") {
            found_melody_start = true;
        }
        // If we find a semicolon before melody(), stop looking
        if line_without_comments.contains(';') && !found_melody_start && i != current_line {
            break;
        }
        context = format!("{} {}", line_without_comments, context);
        if found_melody_start {
            break;
        }
    }

    // Look forward for the rest of the chain
    let end_line = lines.len().min(current_line + max_lookback);
    for line in lines.iter().take(end_line).skip(current_line + 1) {
        let line_without_comments = strip_line_comment(line);
        context.push_str(line_without_comments);
        context.push(' ');
        if line_without_comments.contains(';') {
            break;
        }
    }

    context
}

/// Strip single-line comment from a line (handles // comments).
/// Preserves content inside strings to avoid stripping // inside string literals.
fn strip_line_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut string_char = '"';
    let mut prev_char = '\0';

    for (i, c) in line.char_indices() {
        if !in_string {
            if c == '"' || c == '\'' {
                in_string = true;
                string_char = c;
            } else if c == '/' && prev_char == '/' {
                // Found // outside of string, return everything before it
                return &line[..i.saturating_sub(1)];
            }
        } else if c == string_char && prev_char != '\\' {
            in_string = false;
        }
        prev_char = c;
    }

    line
}

/// Lint the content of a .notes() call using character-based parsing.
/// This matches the real melody parser's behavior where whitespace is optional.
fn lint_melody_content(
    content: &str,
    line: u32,
    start_col: u32,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Use character-based tokenization like the real melody parser
    let tokens = tokenize_melody_content(content);

    // Check if token count is a multiple of 4
    if !tokens.is_empty() && tokens.len() % 4 != 0 {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position {
                    line,
                    character: start_col,
                },
                end: Position {
                    line,
                    character: start_col + content.len() as u32,
                },
            },
            severity: Some(DiagnosticSeverity::HINT),
            code: Some(NumberOrString::String("melody-token-count".to_string())),
            code_description: None,
            source: Some("vibelang".to_string()),
            message: format!(
                "Melody has {} tokens. Consider using a multiple of 4 for standard time signatures.",
                tokens.len()
            ),
            related_information: None,
            tags: None,
            data: None,
        });
    }

    // Report any invalid tokens found during parsing
    for (token_str, offset, is_valid) in &tokens {
        if !is_valid {
            let token_start = start_col + *offset as u32;
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line,
                        character: token_start,
                    },
                    end: Position {
                        line,
                        character: token_start + token_str.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("invalid-melody-token".to_string())),
                code_description: None,
                source: Some("vibelang".to_string()),
                message: format!(
                    "Invalid melody token '{}'. Expected a note (e.g., C4, F#3), chord (e.g., C4:maj7), scale degree (1-7), or rest (-, ~, .).",
                    token_str
                ),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }
}

/// Tokenize melody content using character-based parsing (like the real melody parser).
/// Returns a list of (token_string, offset, is_valid) tuples.
fn tokenize_melody_content(content: &str) -> Vec<(String, usize, bool)> {
    let mut tokens = Vec::new();
    let mut chars = content.char_indices().peekable();

    while let Some((idx, c)) = chars.next() {
        match c {
            // Whitespace and bar separators are ignored (not counted as tokens)
            ' ' | '\t' | '\n' | '\r' | '|' => {}

            // Tie/continuation markers - valid single-character tokens
            '-' | '~' => {
                tokens.push((c.to_string(), idx, true));
            }

            // Rest markers - valid single-character tokens
            '.' | '_' => {
                tokens.push((c.to_string(), idx, true));
            }

            // Scale degree (1-7), optionally with chord quality
            '1'..='7' => {
                let mut token = String::new();
                token.push(c);

                // Check for chord quality suffix (e.g., ":maj7")
                if chars.peek().map(|(_, c)| *c) == Some(':') {
                    token.push(chars.next().unwrap().1); // consume ':'
                    // Collect chord quality (letters, numbers)
                    while let Some(&(_, next)) = chars.peek() {
                        match next {
                            'a'..='z' | 'A'..='Z' | '0'..='9' => {
                                token.push(chars.next().unwrap().1);
                            }
                            _ => break,
                        }
                    }
                }

                // Validate: scale degree with optional valid chord
                let is_valid = if let Some(colon_pos) = token.find(':') {
                    let degree_part = &token[..colon_pos];
                    let chord_part = &token[colon_pos + 1..];
                    is_valid_scale_degree(degree_part) && is_valid_chord(chord_part)
                } else {
                    true // Plain scale degree 1-7 is always valid
                };

                tokens.push((token, idx, is_valid));
            }

            // Start of a note name (A-G)
            'A'..='G' | 'a'..='g' => {
                let mut token = String::new();
                token.push(c.to_ascii_uppercase());

                // Collect accidentals and octave digits
                while let Some(&(_, next)) = chars.peek() {
                    match next {
                        '#' | 'b' | '♯' | '♭' => {
                            token.push(chars.next().unwrap().1);
                        }
                        '0'..='9' => {
                            token.push(chars.next().unwrap().1);
                        }
                        _ => break,
                    }
                }

                // Check for chord quality suffix (e.g., ":maj7")
                if chars.peek().map(|(_, c)| *c) == Some(':') {
                    token.push(chars.next().unwrap().1); // consume ':'
                    // Collect chord quality
                    while let Some(&(_, next)) = chars.peek() {
                        match next {
                            'a'..='z' | 'A'..='Z' | '0'..='9' => {
                                token.push(chars.next().unwrap().1);
                            }
                            _ => break,
                        }
                    }
                }

                // Validate the note/chord
                let is_valid = is_valid_melody_token(&token);
                tokens.push((token, idx, is_valid));
            }

            // Unknown character - invalid token
            _ => {
                tokens.push((c.to_string(), idx, false));
            }
        }
    }

    tokens
}

/// Check if a token is a valid melody token.
fn is_valid_melody_token(token: &str) -> bool {
    if token == "-" || token == "~" || token == "_" || token == "." || token == "|" {
        return true;
    }

    // Check for chord notation (note:chord or number:chord)
    if let Some(colon_pos) = token.find(':') {
        let note_part = &token[..colon_pos];
        let chord_part = &token[colon_pos + 1..];
        return (is_valid_note(note_part) || is_valid_scale_degree(note_part)) && is_valid_chord(chord_part);
    }

    // Accept both standard notes (C4, D#5) and scale degrees (1, 2, 3, etc.)
    is_valid_note(token) || is_valid_scale_degree(token)
}

/// Check if a string is a valid scale degree (1-7, optionally with # or b prefix/suffix).
fn is_valid_scale_degree(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Pure numbers 1-7 are valid scale degrees
    if let Ok(n) = s.parse::<i32>() {
        return (1..=7).contains(&n);
    }

    // Also allow accidentals like #4 or b7
    let chars: Vec<char> = s.chars().collect();

    // Check for prefix accidental: #1, b3, etc.
    if chars.len() == 2 && (chars[0] == '#' || chars[0] == 'b') {
        if let Some(d) = chars[1].to_digit(10) {
            return (1..=7).contains(&d);
        }
    }

    false
}

/// Check if a string is a valid note.
fn is_valid_note(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let chars: Vec<char> = s.chars().collect();
    let first = chars[0].to_ascii_uppercase();

    if !['C', 'D', 'E', 'F', 'G', 'A', 'B'].contains(&first) {
        return false;
    }

    let mut idx = 1;

    if idx < chars.len() && (chars[idx] == '#' || chars[idx] == 'b') {
        idx += 1;
    }

    if idx >= chars.len() {
        return true;
    }

    for c in &chars[idx..] {
        if !c.is_ascii_digit() {
            return false;
        }
    }

    true
}

/// Check if a string is a valid chord type.
fn is_valid_chord(s: &str) -> bool {
    VALID_CHORDS.contains(&s)
}

/// Lint pattern calls in the source.
pub fn lint_patterns(content: &str, diagnostics: &mut Vec<Diagnostic>) {
    let step_pattern = regex::Regex::new(r#"\.step\s*\(\s*"([^"]*)"\s*\)"#).ok();

    if let Some(re) = step_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let (Some(_full_match), Some(content_match)) = (cap.get(0), cap.get(1)) {
                    let pattern_content = content_match.as_str();
                    let content_start = content_match.start() as u32;

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
    let steps: Vec<char> = content
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '|')
        .collect();

    // Check if step count is a multiple of 4
    if !steps.is_empty() && steps.len() % 4 != 0 {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position {
                    line,
                    character: start_col,
                },
                end: Position {
                    line,
                    character: start_col + content.len() as u32,
                },
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
        if c.is_whitespace() || c == '|' {
            continue;
        }

        if !VALID_PATTERN_TOKENS.contains(&c) && c != '9' {
            let char_col = start_col + idx as u32;
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line,
                        character: char_col,
                    },
                    end: Position {
                        line,
                        character: char_col + 1,
                    },
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

/// Track variable usage and report potential typos.
pub fn lint_variables(content: &str, diagnostics: &mut Vec<Diagnostic>) {
    let mut defined_vars: HashSet<String> = HashSet::new();
    let mut var_usages: Vec<(String, u32, u32)> = Vec::new();

    // Comprehensive list of builtins including Rhai stdlib, VibeLang API, and DSP functions
    let builtins: HashSet<&str> = [
        // Rhai control flow and keywords
        "if", "else", "while", "for", "loop", "break", "continue", "return", "fn", "let", "const",
        "true", "false", "nil", "in", "this", "switch", "case", "default", "throw", "try", "catch",
        "import", "export", "as", "private",
        // Rhai stdlib
        "print", "debug", "type_of", "is_def_var", "is_def_fn", "len", "is_empty", "contains",
        "push", "pop", "shift", "insert", "remove", "reverse", "sort", "drain", "retain",
        "splice", "truncate", "clear", "dedup", "map", "filter", "reduce", "fold", "all", "any",
        "some", "find", "find_map", "index_of", "split", "chop", "extract", "zip", "sum",
        "keys", "values", "get", "set", "to_string", "to_int", "to_float", "to_bool", "parse_int",
        "parse_float", "to_upper", "to_lower", "trim", "starts_with", "ends_with", "replace",
        "sub_string", "crop", "index_of", "split", "join", "chars", "bytes",
        // Math
        "abs", "sign", "floor", "ceil", "round", "trunc", "frac", "sqrt", "exp", "ln", "log",
        "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
        "min", "max", "clamp", "pow", "hypot", "atan2", "random", "rand",
        // VibeLang core API
        "voice", "pattern", "melody", "sequence", "group", "define_group", "fx", "fade", "sample",
        "define_synthdef", "define_fx", "load_sfz", "load_sample", "load_vst_instrument", "load_vst_effect",
        "set_tempo", "get_tempo", "set_quantization", "set_time_signature", "get_current_beat", "get_current_bar",
        "db", "bars", "note", "sleep", "sleep_secs", "exit", "exit_with_code",
        "all_group_names", "all_voice_names", "all_pattern_names", "all_melody_names", "all_effect_names",
        "get_voice", "get_pattern", "get_melody", "get_effect", "active_synth_count", "jump_to_start",
        "record", "stop_recording", "nudge_transport", "fade_group_gain", "fade_param",
        "define_macro", "trigger_macro", "define_send", "melody_gen", "detect_bpm", "set_group_gain",
        "automation", "scene", "scene_morph", "midi_device", "midi_map", "midi_devices",
        // DSP functions
        "envelope", "env_perc", "env_adsr", "env_asr", "env_triangle", "Env",
        "mix", "dup", "channels", "channel", "sound_in", "sound_in_channel", "detune_spread",
        "db_to_amp", "amp_to_db", "tanh", "distort", "softclip", "clip", "wrap", "fold",
        "squared", "cubed", "modulo", "round_to", "lerp",
        // UGen oscillators
        "sin_osc_ar", "sin_osc_kr", "saw_ar", "saw_kr", "pulse_ar", "pulse_kr", "tri_ar", "tri_kr",
        "square_ar", "square_kr", "white_noise_ar", "pink_noise_ar", "brown_noise_ar",
        "lf_saw_ar", "lf_saw_kr", "lf_pulse_ar", "lf_pulse_kr", "lf_tri_ar", "lf_tri_kr",
        "lf_noise0_ar", "lf_noise0_kr", "lf_noise1_ar", "lf_noise1_kr", "lf_noise2_ar", "lf_noise2_kr",
        "sync_saw_ar", "var_saw_ar", "blip_ar", "formant_ar",
        // UGen filters
        "lpf_ar", "hpf_ar", "bpf_ar", "brf_ar", "rlpf_ar", "rhpf_ar", "moog_ff_ar",
        "one_pole_ar", "two_pole_ar", "lag_ar", "lag_kr", "lag2_ar", "lag2_kr", "lag3_ar", "lag3_kr",
        "resonz_ar", "ringz_ar", "formlet_ar", "comb_c_ar", "comb_l_ar", "comb_n_ar",
        "allpass_c_ar", "allpass_l_ar", "allpass_n_ar", "free_verb_ar", "g_verb_ar",
        // UGen envelopes and control
        "env_gen_ar", "env_gen_kr", "line_ar", "line_kr", "x_line_ar", "x_line_kr",
        "linen_ar", "linen_kr", "decay_ar", "decay_kr", "decay2_ar", "decay2_kr",
        // UGen math/conversion
        "dc_ar", "dc_kr", "kr", "ar", "a2k", "k2a", "t2a", "t2k",
        // Builder method names (common)
        "synth", "param", "body", "on", "step", "notes", "start", "stop", "apply", "run",
        "gain", "poly", "mute", "unmute", "solo", "trigger", "note_on", "note_off",
        "scale", "root", "gate", "transpose", "len", "swing", "quantize", "lane", "values",
        "euclid", "clip", "clip_once", "clip_loops", "loop_bars", "loop_beats",
        "from", "to", "over", "over_bars", "on_group", "on_voice", "on_effect", "on_pattern", "on_melody",
        "attack", "decay", "sustain", "release", "adsr", "perc", "asr", "triangle",
        "cleanup_on_finish", "build", "time_scale", "level_scale",
    ]
    .into_iter()
    .collect();

    let assign_pattern = regex::Regex::new(r"\b(let\s+)?(\w+)\s*=").ok();
    let closure_param_pattern = regex::Regex::new(r"\|([^|]*)\|").ok();
    // Use a simple word boundary pattern and filter programmatically
    // (Rust regex doesn't support look-around assertions)
    let usage_pattern = regex::Regex::new(r"\b([a-zA-Z_]\w*)\b").ok();

    // Parse variable definitions from let statements
    if let Some(assign_re) = &assign_pattern {
        for line in content.lines() {
            for cap in assign_re.captures_iter(line) {
                if let Some(var_match) = cap.get(2) {
                    let var_name = var_match.as_str().to_string();
                    if !var_name.starts_with('.') {
                        defined_vars.insert(var_name);
                    }
                }
            }
        }
    }

    // Parse closure parameters (|param1, param2|)
    if let Some(closure_re) = &closure_param_pattern {
        for line in content.lines() {
            for cap in closure_re.captures_iter(line) {
                if let Some(params_match) = cap.get(1) {
                    let params_str = params_match.as_str();
                    // Split by comma and parse each parameter
                    for param in params_str.split(',') {
                        let param = param.trim();
                        // Handle patterns like |a, b| or |pair|
                        if !param.is_empty() && param.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false) {
                            // Extract just the identifier part (in case of type annotations etc.)
                            let ident: String = param.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                            if !ident.is_empty() {
                                defined_vars.insert(ident);
                            }
                        }
                    }
                }
            }
        }
    }

    // Parse for loop variables - simple form: for x in ...
    let for_simple_pattern = regex::Regex::new(r"\bfor\s+([a-zA-Z_]\w*)\s+in\b").ok();
    if let Some(for_re) = &for_simple_pattern {
        for line in content.lines() {
            for cap in for_re.captures_iter(line) {
                if let Some(var_match) = cap.get(1) {
                    defined_vars.insert(var_match.as_str().to_string());
                }
            }
        }
    }

    // Parse for loop variables - tuple form: for (a, b) in ...
    let for_tuple_pattern =
        regex::Regex::new(r"\bfor\s+\(\s*([a-zA-Z_]\w*)\s*,\s*([a-zA-Z_]\w*)\s*\)\s+in\b").ok();
    if let Some(for_re) = &for_tuple_pattern {
        for line in content.lines() {
            for cap in for_re.captures_iter(line) {
                if let Some(var_match) = cap.get(1) {
                    defined_vars.insert(var_match.as_str().to_string());
                }
                if let Some(var_match) = cap.get(2) {
                    defined_vars.insert(var_match.as_str().to_string());
                }
            }
        }
    }

    // Pre-process entire content to strip all string literals (including multiline backtick strings)
    let stripped_lines = strip_all_string_literals(content);

    if let Some(usage_re) = &usage_pattern {
        for (line_num, _line) in content.lines().enumerate() {
            // Use pre-stripped line to avoid matching identifiers inside strings
            // (including multiline backtick strings)
            let line_without_strings = stripped_lines
                .get(line_num)
                .map(|s| {
                    // Also strip comments from the pre-stripped line
                    if let Some(idx) = s.find("//") {
                        s[..idx].to_string()
                    } else {
                        s.clone()
                    }
                })
                .unwrap_or_default();

            for cap in usage_re.captures_iter(&line_without_strings) {
                if let Some(var_match) = cap.get(1) {
                    let var_name = var_match.as_str().to_string();
                    let start = var_match.start();
                    let end = var_match.end();
                    let col = start as u32;

                    // Skip if preceded by '.' (method call like foo.bar)
                    if start > 0 {
                        let prev_char = line_without_strings.chars().nth(start - 1);
                        if prev_char == Some('.') {
                            continue;
                        }
                    }

                    // Skip if followed by '(' (function call like bar())
                    if end < line_without_strings.len() {
                        let rest = &line_without_strings[end..];
                        if rest.trim_start().starts_with('(') {
                            continue;
                        }
                    }

                    if !builtins.contains(var_name.as_str())
                        && !defined_vars.contains(&var_name)
                        && !var_name
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_digit())
                            .unwrap_or(true)
                    {
                        var_usages.push((var_name, line_num as u32, col));
                    }
                }
            }
        }
    }

    for (var_name, line, col) in var_usages {
        let similar = find_similar_variable(&var_name, &defined_vars, &builtins);

        let message = if let Some(suggestion) = similar {
            format!("Unknown identifier '{}'. Did you mean '{}'?", var_name, suggestion)
        } else {
            format!(
                "Unknown identifier '{}'. It may be defined elsewhere or could be a typo.",
                var_name
            )
        };

        diagnostics.push(Diagnostic {
            range: Range {
                start: Position { line, character: col },
                end: Position {
                    line,
                    character: col + var_name.len() as u32,
                },
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

/// Strip string literals from content, replacing them with spaces to preserve positions.
/// Handles double-quoted, single-quoted, and backtick multiline strings, including escaped quotes.
/// Returns a vector of stripped lines.
fn strip_all_string_literals(content: &str) -> Vec<String> {
    let mut result = String::with_capacity(content.len());
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    let mut string_char = '"';

    while i < chars.len() {
        let c = chars[i];

        if in_string {
            // Inside a string - replace with space to preserve positions (but keep newlines)
            if c == '\n' {
                result.push('\n');
            } else {
                result.push(' ');
            }

            // Check for escape sequence (not applicable to backtick strings)
            if string_char != '`' && c == '\\' && i + 1 < chars.len() {
                // Skip the next character (escaped)
                i += 1;
                if chars[i] == '\n' {
                    result.push('\n');
                } else {
                    result.push(' ');
                }
            } else if c == string_char {
                // End of string
                in_string = false;
            }
        } else if c == '"' || c == '\'' || c == '`' {
            // Starting a string
            in_string = true;
            string_char = c;
            result.push(' '); // Replace opening quote with space
        } else {
            result.push(c);
        }

        i += 1;
    }

    result.lines().map(|s| s.to_string()).collect()
}

/// Find a similar variable name (for typo suggestions).
fn find_similar_variable(
    name: &str,
    defined: &HashSet<String>,
    builtins: &HashSet<&str>,
) -> Option<String> {
    let name_lower = name.to_lowercase();

    for var in defined {
        if levenshtein_distance(&name_lower, &var.to_lowercase()) <= 2 {
            return Some(var.clone());
        }
    }

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

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut dp = vec![vec![0; n + 1]; m + 1];

    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[m][n]
}

/// Lint .on("voice_name") calls to check if the voice exists.
pub fn lint_voice_references(
    content: &str,
    defined_voices: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let on_pattern = regex::Regex::new(r#"\.on\s*\(\s*"([^"]+)"\s*\)"#).ok();

    if let Some(re) = on_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    let voice_name = name_match.as_str();
                    let name_start = name_match.start() as u32;
                    let name_end = name_match.end() as u32;

                    if !defined_voices.contains(voice_name) {
                        let suggestion = defined_voices
                            .iter()
                            .find(|v| {
                                levenshtein_distance(
                                    &voice_name.to_lowercase(),
                                    &v.to_lowercase(),
                                ) <= 2
                            })
                            .cloned();

                        let message = if let Some(similar) = suggestion {
                            format!("Unknown voice '{}'. Did you mean '{}'?", voice_name, similar)
                        } else if defined_voices.is_empty() {
                            format!(
                                "Unknown voice '{}'. No voices are defined in this file.",
                                voice_name
                            )
                        } else {
                            format!(
                                "Unknown voice '{}'. Defined voices: {}",
                                voice_name,
                                defined_voices
                                    .iter()
                                    .take(5)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(", ")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_multiline_backtick_strings() {
        let content = r#"let x = `
            D3:m7 - - -
            A3:m7 - - -
        `;
let y = 5;"#;

        let stripped = strip_all_string_literals(content);

        // Line 0: "let x = `" -> "let x =  " (backtick replaced with space)
        assert!(stripped[0].contains("let x"));
        // Lines 1-3 should be all spaces (inside the multiline string)
        assert!(
            stripped[1].chars().all(|c| c == ' '),
            "Line 1 should be all spaces: {:?}",
            stripped[1]
        );
        assert!(
            stripped[2].chars().all(|c| c == ' '),
            "Line 2 should be all spaces: {:?}",
            stripped[2]
        );
        // Line 4: "let y = 5;" should be preserved
        assert!(
            stripped[4].contains("let y"),
            "Line 4 should contain 'let y': {:?}",
            stripped[4]
        );
    }

    #[test]
    fn test_no_false_positives_in_multiline_strings() {
        // Test that identifiers inside multiline strings are NOT reported as unknown
        let content = r#"melody("chords").on(rhodes)
    .notes(`
        D3:m7 - - - | - - - - |
        A3:m7 - - - | - - - - |
    `)
    .start();"#;

        let mut diagnostics = Vec::new();
        lint_variables(content, &mut diagnostics);

        // Should not report D3, A3, m7 as unknown identifiers
        for diag in &diagnostics {
            let msg = &diag.message;
            assert!(
                !msg.contains("D3") && !msg.contains("A3") && !msg.contains("m7"),
                "Should not report identifiers inside multiline strings: {}",
                msg
            );
        }
    }

    #[test]
    fn test_strip_regular_strings() {
        let content = r#"let x = "hello world";
let y = 'single quote';"#;

        let stripped = strip_all_string_literals(content);

        // String content should be replaced with spaces
        assert!(!stripped[0].contains("hello"));
        assert!(!stripped[1].contains("single"));
        // Variable names should be preserved
        assert!(stripped[0].contains("let x"));
        assert!(stripped[1].contains("let y"));
    }
}
