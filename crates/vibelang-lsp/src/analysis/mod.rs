//! Code analysis for VibeLang files.
//!
//! This module handles parsing and analyzing .vibe files to extract:
//! - Syntax errors (via Rhai compilation)
//! - Import statements
//! - Function calls (for synthdef validation)
//! - Symbols for completion
//! - Variable usage analysis
//! - Melody and pattern linting

use std::collections::HashSet;
use std::path::PathBuf;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

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
    /// Synthdef references (calls to .synth("name") or .on("name")).
    pub synthdef_refs: Vec<SynthdefRef>,
    /// Effect references (calls to .synth("name") on fx).
    pub effect_refs: Vec<EffectRef>,
    /// Variable definitions (let name = ...).
    pub variable_defs: Vec<VariableDef>,
    /// Local synthdef definitions (define_synthdef("name")).
    pub local_synthdefs: HashSet<String>,
    /// Local effect definitions (define_fx("name")).
    pub local_effects: HashSet<String>,
    /// Voice definitions (voice("name")).
    pub voice_defs: HashSet<String>,
    /// Pattern definitions (pattern("name")).
    pub pattern_defs: HashSet<String>,
    /// Melody definitions (melody("name")).
    pub melody_defs: HashSet<String>,
    /// Group definitions (define_group("name", ...)).
    pub group_defs: HashSet<String>,
    /// Sequence definitions (sequence("name")).
    pub sequence_defs: HashSet<String>,
    /// Sample file references.
    pub sample_refs: Vec<SampleRef>,
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

/// A reference to a sample file.
#[derive(Debug, Clone)]
pub struct SampleRef {
    /// The sample path as written.
    pub path: String,
    /// The range of the path in the document.
    pub range: Range,
    /// Whether the sample is an SFZ instrument.
    pub is_sfz: bool,
}

/// Analyze a VibeLang document.
pub fn analyze_document(
    content: &str,
    file_path: Option<&PathBuf>,
    import_paths: &[PathBuf],
) -> AnalysisResult {
    let mut result = AnalysisResult::default();

    // Run Rhai compilation to check syntax
    let engine = rhai::Engine::new();
    match engine.compile(content) {
        Ok(_) => {}
        Err(e) => {
            result.syntax_errors.push(parse_error_to_diagnostic(&e));
        }
    }

    // Parse imports, synthdef refs, effect refs, variable definitions
    parse_imports(content, file_path, import_paths, &mut result);
    parse_synthdef_refs(content, &mut result);
    parse_effect_refs(content, &mut result);
    parse_variable_defs(content, &mut result);
    parse_local_synthdefs(content, &mut result);
    parse_local_effects(content, &mut result);
    parse_entity_definitions(content, &mut result);
    parse_sample_refs(content, &mut result);

    // Run linting passes
    lint_melodies(content, &mut result.lint_diagnostics);
    lint_patterns(content, &mut result.lint_diagnostics);
    lint_voice_references(content, &result.voice_defs, &mut result.lint_diagnostics);
    lint_sample_files(file_path, &result.sample_refs, &mut result.lint_diagnostics);

    result
}

/// Convert a Rhai parse error to an LSP diagnostic.
fn parse_error_to_diagnostic(error: &rhai::ParseError) -> Diagnostic {
    let pos = error.position();
    let line = pos.line().map(|l| l as u32 - 1).unwrap_or(0);
    let col = pos.position().map(|c| c as u32 - 1).unwrap_or(0);

    Diagnostic {
        range: Range {
            start: Position {
                line,
                character: col,
            },
            end: Position {
                line,
                character: col + 10, // Approximate error length
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String("syntax-error".to_string())),
        source: Some("vibelang".to_string()),
        message: error.to_string(),
        ..Default::default()
    }
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

                    // If import cannot be resolved, add a diagnostic
                    if resolved.is_none() {
                        result.semantic_diagnostics.push(Diagnostic {
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
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(NumberOrString::String("unresolved-import".to_string())),
                            source: Some("vibelang".to_string()),
                            message: format!("Cannot resolve import '{}'", import_path),
                            ..Default::default()
                        });
                    }

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

/// Parse synthdef references from .synth("name") and .on("name") calls.
fn parse_synthdef_refs(content: &str, result: &mut AnalysisResult) {
    // Match .synth("name") calls
    let synth_pattern = regex::Regex::new(r#"\.synth\s*\(\s*["']([^"']+)["']\s*\)"#).ok();

    if let Some(re) = synth_pattern {
        for (line_num, line) in content.lines().enumerate() {
            // Skip if this is an fx() line
            if line.contains("fx(") || line.contains("define_fx(") {
                continue;
            }
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
                    result
                        .local_synthdefs
                        .insert(name_match.as_str().to_string());
                }
            }
        }
    }
}

/// Parse local effect definitions from define_fx("name") calls.
fn parse_local_effects(content: &str, result: &mut AnalysisResult) {
    let fx_pattern = regex::Regex::new(r#"define_fx\s*\(\s*["']([^"']+)["']"#).ok();

    if let Some(re) = fx_pattern {
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    result.local_effects.insert(name_match.as_str().to_string());
                }
            }
        }
    }
}

/// Parse entity definitions (voice, pattern, melody, group, sequence).
fn parse_entity_definitions(content: &str, result: &mut AnalysisResult) {
    // Voice definitions
    let voice_pattern = regex::Regex::new(r#"voice\s*\(\s*["']([^"']+)["']\s*\)"#).ok();
    if let Some(re) = voice_pattern {
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    result.voice_defs.insert(name_match.as_str().to_string());
                }
            }
        }
    }

    // Pattern definitions
    let pattern_pattern = regex::Regex::new(r#"pattern\s*\(\s*["']([^"']+)["']\s*\)"#).ok();
    if let Some(re) = pattern_pattern {
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    result.pattern_defs.insert(name_match.as_str().to_string());
                }
            }
        }
    }

    // Melody definitions
    let melody_pattern = regex::Regex::new(r#"melody\s*\(\s*["']([^"']+)["']\s*\)"#).ok();
    if let Some(re) = melody_pattern {
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    result.melody_defs.insert(name_match.as_str().to_string());
                }
            }
        }
    }

    // Group definitions
    let group_pattern = regex::Regex::new(r#"define_group\s*\(\s*["']([^"']+)["']"#).ok();
    if let Some(re) = group_pattern {
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    result.group_defs.insert(name_match.as_str().to_string());
                }
            }
        }
    }

    // Sequence definitions
    let sequence_pattern = regex::Regex::new(r#"sequence\s*\(\s*["']([^"']+)["']\s*\)"#).ok();
    if let Some(re) = sequence_pattern {
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                if let Some(name_match) = cap.get(1) {
                    result.sequence_defs.insert(name_match.as_str().to_string());
                }
            }
        }
    }
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
    /// Inside function call - for signature help.
    FunctionCall {
        function_name: String,
        param_index: usize,
    },
    /// Inside DSP body (define_synthdef/define_fx .body closure) - suggest UGens.
    DspBody,
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

    // Check for DSP body context (inside define_synthdef/define_fx .body closure)
    if is_inside_dsp_body(content, line, character) {
        return CompletionContext::DspBody;
    }

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
    if prefix.contains("record(") || prefix.contains("record (") {
        return Some("Record".to_string());
    }
    if prefix.contains("define_synthdef(") {
        return Some("SynthdefBuilder".to_string());
    }
    if prefix.contains("define_fx(") {
        return Some("FxBuilder".to_string());
    }
    if prefix.contains("envelope(") {
        return Some("Envelope".to_string());
    }
    None
}

/// Check if the cursor is inside a DSP body (define_synthdef/define_fx .body closure).
fn is_inside_dsp_body(content: &str, line: usize, _character: usize) -> bool {
    // Look backwards from the current position to find unclosed .body() closures
    let lines: Vec<&str> = content.lines().take(line + 1).collect();

    let mut in_body = false;
    let mut brace_depth = 0;
    let mut body_brace_depth = 0;

    for line_content in &lines {
        // Check for .body( which starts a DSP context
        if line_content.contains(".body(") {
            // Check if it's preceded by define_synthdef or define_fx
            let text_before: String = lines
                .iter()
                .take(lines.iter().position(|l| l == line_content).unwrap_or(0) + 1)
                .copied()
                .collect::<Vec<_>>()
                .join(" ");

            if text_before.contains("define_synthdef") || text_before.contains("define_fx") {
                in_body = true;
                body_brace_depth = brace_depth;
            }
        }

        // Track brace depth
        for c in line_content.chars() {
            match c {
                '{' => {
                    brace_depth += 1;
                }
                '}' => {
                    if brace_depth > 0 {
                        brace_depth -= 1;
                    }
                    // Check if we're closing the body's brace
                    if in_body && brace_depth < body_brace_depth {
                        in_body = false;
                    }
                }
                _ => {}
            }
        }
    }

    in_body
}

// =============================================================================
// Linting Functions
// =============================================================================

/// Valid chord types.
/// Valid pattern tokens.
const VALID_PATTERN_TOKENS: [char; 15] = [
    'x', 'X', '.', '-', '|', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
];

/// Lint melody calls in the source.
fn lint_melodies(content: &str, diagnostics: &mut Vec<Diagnostic>) {
    let notes_pattern = regex::Regex::new(r#"\.notes\s*\(\s*"([^"]*)"\s*\)"#).ok();
    let notes_backtick = regex::Regex::new(r#"\.notes\s*\(\s*`([^`]*)`\s*\)"#).ok();

    for (line_num, line) in content.lines().enumerate() {
        // Check double-quoted notes
        if let Some(ref re) = notes_pattern {
            for cap in re.captures_iter(line) {
                if let Some(content_match) = cap.get(1) {
                    let notes_content = content_match.as_str();
                    let content_start = content_match.start() as u32;
                    lint_melody_content(notes_content, line_num as u32, content_start, diagnostics);
                }
            }
        }

        // Check backtick notes
        if let Some(ref re) = notes_backtick {
            for cap in re.captures_iter(line) {
                if let Some(content_match) = cap.get(1) {
                    let notes_content = content_match.as_str();
                    let content_start = content_match.start() as u32;
                    lint_melody_content(notes_content, line_num as u32, content_start, diagnostics);
                }
            }
        }
    }
}

/// Lint the content of a .notes() call.
///
/// Tokenizes the melody string the same way the runtime does — character by
/// character — so that concatenated shorthand like `1.1.` is valid.
fn lint_melody_content(
    content: &str,
    line: u32,
    start_col: u32,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut token_count = 0u32;
    let mut chars = content.chars().enumerate().peekable();

    while let Some((idx, c)) = chars.next() {
        match c {
            // Whitespace and bar lines are separators
            ' ' | '\t' | '\n' | '\r' | '|' => {}

            // Rest markers
            '.' | '_' | '~' => {
                token_count += 1;
            }

            // Tie/hold
            '-' => {
                token_count += 1;
            }

            // Chord bracket notation: [C4 E4 G4] with optional [params]
            '[' => {
                token_count += 1;
                // Skip to closing ']'
                let mut found_close = false;
                for (_, ch) in chars.by_ref() {
                    if ch == ']' {
                        found_close = true;
                        break;
                    }
                }
                if !found_close {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line,
                                character: start_col + idx as u32,
                            },
                            end: Position {
                                line,
                                character: start_col + content.len() as u32,
                            },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("invalid-melody-token".to_string())),
                        source: Some("vibelang".to_string()),
                        message: "Unterminated chord bracket '['.".to_string(),
                        ..Default::default()
                    });
                    return;
                }
                // Optional per-note params [key=value]
                if chars.peek().map(|(_, ch)| *ch) == Some('[') {
                    for (_, ch) in chars.by_ref() {
                        if ch == ']' {
                            break;
                        }
                    }
                }
            }

            // Scale degrees 1-7 with optional octave modifiers ('/,) and [params]
            '1'..='7' => {
                token_count += 1;
                // Consume octave modifiers
                while chars
                    .peek()
                    .map(|(_, ch)| *ch == '\'' || *ch == ',')
                    .unwrap_or(false)
                {
                    chars.next();
                }
                // Optional per-note params
                if chars.peek().map(|(_, ch)| *ch) == Some('[') {
                    for (_, ch) in chars.by_ref() {
                        if ch == ']' {
                            break;
                        }
                    }
                }
            }

            // Accidentals before scale degrees: #4, b7
            '#' | 'b'
                if chars
                    .peek()
                    .map(|(_, ch)| ('1'..='7').contains(ch))
                    .unwrap_or(false) =>
            {
                // Check that next char is actually a digit (already peeked)
                if let Some((_, _degree)) = chars.next() {
                    token_count += 1;
                    // Consume octave modifiers
                    while chars
                        .peek()
                        .map(|(_, ch)| *ch == '\'' || *ch == ',')
                        .unwrap_or(false)
                    {
                        chars.next();
                    }
                    // Optional per-note params
                    if chars.peek().map(|(_, ch)| *ch) == Some('[') {
                        for (_, ch) in chars.by_ref() {
                            if ch == ']' {
                                break;
                            }
                        }
                    }
                }
            }

            // Absolute note names A-G
            'A'..='G' | 'a'..='g' => {
                token_count += 1;
                // Consume accidentals and octave digits
                while chars
                    .peek()
                    .map(|(_, ch)| {
                        matches!(ch, '#' | '♯' | '♭' | '0'..='9')
                            || (*ch == 'b' && !('a'..='g').contains(ch))
                    })
                    .unwrap_or(false)
                {
                    // Special case: 'b' is ambiguous (note B vs flat). If we just parsed
                    // a note letter and see 'b', treat as accidental only if followed by a digit.
                    if chars.peek().map(|(_, ch)| *ch) == Some('b') {
                        // Look ahead: is there a digit after 'b'? If so, it's a flat accidental.
                        // Otherwise break — it might be the note B.
                        // We can't easily look 2 ahead with peekable, so consume 'b' as accidental
                        // since we're already inside a note parse.
                        chars.next();
                        continue;
                    }
                    chars.next();
                }
                // Optional chord suffix :maj7, :m7, etc.
                if chars.peek().map(|(_, ch)| *ch) == Some(':') {
                    chars.next(); // consume ':'
                    while chars
                        .peek()
                        .map(|(_, ch)| ch.is_alphanumeric())
                        .unwrap_or(false)
                    {
                        chars.next();
                    }
                }
                // Optional per-note params
                if chars.peek().map(|(_, ch)| *ch) == Some('[') {
                    for (_, ch) in chars.by_ref() {
                        if ch == ']' {
                            break;
                        }
                    }
                }
            }

            // Anything else is invalid
            _ => {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position { line, character: start_col + idx as u32 },
                        end: Position { line, character: start_col + idx as u32 + 1 },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("invalid-melody-token".to_string())),
                    source: Some("vibelang".to_string()),
                    message: format!(
                        "Invalid melody character '{}'. Expected a note (C4, F#3), chord ([C4 E4 G4]), \
                         scale degree (1-7), tie (-), or rest (., _, ~).",
                        c
                    ),
                    ..Default::default()
                });
            }
        }
    }

    // Warn if token count is not a multiple of 4
    if token_count > 0 && token_count % 4 != 0 {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position { line, character: start_col },
                end: Position { line, character: start_col + content.len() as u32 },
            },
            severity: Some(DiagnosticSeverity::HINT),
            code: Some(NumberOrString::String("melody-length".to_string())),
            source: Some("vibelang".to_string()),
            message: format!(
                "Melody has {} tokens. Consider using a multiple of 4 for standard time signatures.",
                token_count
            ),
            ..Default::default()
        });
    }
}

/// Lint pattern calls in the source.
fn lint_patterns(content: &str, diagnostics: &mut Vec<Diagnostic>) {
    let step_pattern = regex::Regex::new(r#"\.step\s*\(\s*"([^"]*)"\s*\)"#).ok();

    if let Some(re) = step_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(content_match) = cap.get(1) {
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

    // Check for invalid tokens
    for (idx, c) in content.chars().enumerate() {
        if c.is_whitespace() || c == '|' {
            continue;
        }

        if !VALID_PATTERN_TOKENS.contains(&c) {
            let char_col = start_col + idx as u32;
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line, character: char_col },
                    end: Position { line, character: char_col + 1 },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("invalid-pattern-token".to_string())),
                source: Some("vibelang".to_string()),
                message: format!(
                    "Invalid pattern token '{}'. Valid: x/X (hit), . (rest), - (sustain), | (bar), 0-9 (velocity).",
                    c
                ),
                ..Default::default()
            });
        }
    }

    // Warn if not a multiple of 4
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
            code: Some(NumberOrString::String("pattern-length".to_string())),
            source: Some("vibelang".to_string()),
            message: format!(
                "Pattern has {} steps (expected multiple of 4 for standard time signatures).",
                steps.len()
            ),
            ..Default::default()
        });
    }
}

/// Lint .on("voice_name") calls to check if the voice exists.
fn lint_voice_references(
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
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("unknown-voice".to_string())),
                            source: Some("vibelang".to_string()),
                            message: format!(
                                "Unknown voice '{}'. Make sure it's defined before use.",
                                voice_name
                            ),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
}

/// Parse sample and SFZ file references.
fn parse_sample_refs(content: &str, result: &mut AnalysisResult) {
    // Match sample("name", "path") - extract the path (second argument)
    let sample_pattern = regex::Regex::new(r#"sample\s*\(\s*"[^"]*"\s*,\s*"([^"]*)""#).ok();
    if let Some(re) = sample_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(path_match) = cap.get(1) {
                    let full_match = cap.get(0).unwrap();
                    let path_offset = path_match.start() - full_match.start();
                    let line_start = line.find(full_match.as_str()).unwrap_or(0);
                    let start_col = line_start + path_offset;

                    result.sample_refs.push(SampleRef {
                        path: path_match.as_str().to_string(),
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: start_col as u32,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: (start_col + path_match.len()) as u32,
                            },
                        },
                        is_sfz: false,
                    });
                }
            }
        }
    }

    // Match load_sfz("name", "path") - extract the path (second argument)
    let sfz_pattern = regex::Regex::new(r#"load_sfz\s*\(\s*"[^"]*"\s*,\s*"([^"]*)""#).ok();
    if let Some(re) = sfz_pattern {
        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(path_match) = cap.get(1) {
                    let full_match = cap.get(0).unwrap();
                    let path_offset = path_match.start() - full_match.start();
                    let line_start = line.find(full_match.as_str()).unwrap_or(0);
                    let start_col = line_start + path_offset;

                    result.sample_refs.push(SampleRef {
                        path: path_match.as_str().to_string(),
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: start_col as u32,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: (start_col + path_match.len()) as u32,
                            },
                        },
                        is_sfz: true,
                    });
                }
            }
        }
    }
}

/// Lint sample file references to check if files exist.
fn lint_sample_files(
    file_path: Option<&PathBuf>,
    sample_refs: &[SampleRef],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let base_dir = match file_path {
        Some(path) => path.parent().map(|p| p.to_path_buf()),
        None => None,
    };

    for sample in sample_refs {
        // Skip if no base directory (can't resolve relative paths)
        let resolved_path = match &base_dir {
            Some(base) => base.join(&sample.path),
            None => PathBuf::from(&sample.path),
        };

        // Check if file exists
        if !resolved_path.exists() {
            let file_type = if sample.is_sfz {
                "SFZ instrument"
            } else {
                "sample"
            };
            diagnostics.push(Diagnostic {
                range: sample.range,
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String("missing-sample".to_string())),
                source: Some("vibelang".to_string()),
                message: format!(
                    "{} file not found: '{}'. Check the path is correct.",
                    file_type, sample.path
                ),
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: lint a melody string and return only ERROR diagnostics.
    fn melody_errors(content: &str) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        lint_melody_content(content, 0, 0, &mut diags);
        diags
            .into_iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .collect()
    }

    #[test]
    fn test_absolute_notes() {
        assert!(melody_errors("C4 D#5 Bb3 A4").is_empty());
        assert!(melody_errors("C D E F").is_empty());
    }

    #[test]
    fn test_scale_degrees() {
        assert!(melody_errors("1 2 3 4").is_empty());
        assert!(melody_errors("1 3 5 7").is_empty());
    }

    #[test]
    fn test_scale_degrees_with_octave() {
        assert!(melody_errors("1' 2' 3' 4'").is_empty());
        assert!(melody_errors("1, 2, 3, 4,").is_empty());
        assert!(melody_errors("1'' 2,, 3' 4").is_empty());
    }

    #[test]
    fn test_accidental_degrees() {
        assert!(melody_errors("#4 b7 #1 b3").is_empty());
    }

    #[test]
    fn test_rests_and_ties() {
        assert!(melody_errors(". . . .").is_empty());
        assert!(melody_errors("_ _ _ _").is_empty());
        assert!(melody_errors("~ ~ ~ ~").is_empty());
        assert!(melody_errors("- - - -").is_empty());
        assert!(melody_errors("C4 - - .").is_empty());
    }

    #[test]
    fn test_concatenated_shorthand() {
        // This is the key case: "1.1." = degree 1, rest, degree 1, rest
        assert!(melody_errors("1.1. 1.1. 1.1. 1.1.").is_empty());
        assert!(melody_errors("1.1. 1.3. 5.3. 1.1.").is_empty());
        assert!(melody_errors("1-1- 3-3- 5-5- 7-7-").is_empty());
    }

    #[test]
    fn test_chord_brackets() {
        assert!(melody_errors("[C4 E4 G4] . . .").is_empty());
        assert!(melody_errors("[C4 E4 G4][vel=80] . . .").is_empty());
    }

    #[test]
    fn test_chord_suffix() {
        assert!(melody_errors("C4:maj7 D4:m7 E4:dim A4:aug").is_empty());
    }

    #[test]
    fn test_per_note_params() {
        assert!(melody_errors("C4[velocity=100] D4 E4 F4").is_empty());
        assert!(melody_errors("1[cutoff=2000] 3[pan=-0.5] 5 7").is_empty());
    }

    #[test]
    fn test_bars() {
        assert!(melody_errors("C4 D4 E4 F4 | G4 A4 B4 C5").is_empty());
        assert!(melody_errors("1.1. 1.1. 1.1. 1.1. | 1.1. 1.1. 1.1. 1.1.").is_empty());
    }

    #[test]
    fn test_invalid_characters() {
        assert!(!melody_errors("X Y Z W").is_empty());
        assert!(!melody_errors("0 8 9 0").is_empty());
    }

    #[test]
    fn test_unterminated_bracket() {
        assert!(!melody_errors("[C4 E4").is_empty());
    }
}
