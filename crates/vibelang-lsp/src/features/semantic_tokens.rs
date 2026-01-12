//! Semantic tokens provider for VibeLang.
//!
//! Provides semantic highlighting for better syntax coloring.

use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensLegend,
};

use crate::analysis::AnalysisResult;

/// Semantic token types used by VibeLang.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE,   // 0: groups
    SemanticTokenType::TYPE,        // 1: synthdefs
    SemanticTokenType::CLASS,       // 2: effects
    SemanticTokenType::FUNCTION,    // 3: voices, functions
    SemanticTokenType::METHOD,      // 4: methods like .apply(), .play()
    SemanticTokenType::PROPERTY,    // 5: parameters
    SemanticTokenType::VARIABLE,    // 6: variables
    SemanticTokenType::STRING,      // 7: strings
    SemanticTokenType::NUMBER,      // 8: numbers
    SemanticTokenType::KEYWORD,     // 9: keywords
    SemanticTokenType::OPERATOR,    // 10: operators
    SemanticTokenType::COMMENT,     // 11: comments
    SemanticTokenType::MACRO,       // 12: patterns/melodies
    SemanticTokenType::PARAMETER,   // 13: function parameters
    SemanticTokenType::ENUM_MEMBER, // 14: note names
];

/// Semantic token modifiers.
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,     // 0
    SemanticTokenModifier::DEFINITION,      // 1
    SemanticTokenModifier::READONLY,        // 2
    SemanticTokenModifier::STATIC,          // 3
    SemanticTokenModifier::DEPRECATED,      // 4
    SemanticTokenModifier::DEFAULT_LIBRARY, // 5
];

/// Get the semantic tokens legend for registration.
pub fn get_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

/// Generate semantic tokens for a document.
pub fn get_semantic_tokens(content: &str, analysis: Option<&AnalysisResult>) -> SemanticTokens {
    let mut builder = SemanticTokenBuilder::new();

    for (line_num, line) in content.lines().enumerate() {
        tokenize_line(&mut builder, line_num as u32, line, analysis);
    }

    SemanticTokens {
        result_id: None,
        data: builder.build(),
    }
}

/// Builder for semantic tokens using delta encoding.
struct SemanticTokenBuilder {
    tokens: Vec<SemanticToken>,
    prev_line: u32,
    prev_char: u32,
}

impl SemanticTokenBuilder {
    fn new() -> Self {
        Self {
            tokens: Vec::new(),
            prev_line: 0,
            prev_char: 0,
        }
    }

    fn push(&mut self, line: u32, start_char: u32, length: u32, token_type: u32, modifiers: u32) {
        let delta_line = line - self.prev_line;
        let delta_start = if delta_line == 0 {
            start_char - self.prev_char
        } else {
            start_char
        };

        self.tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: modifiers,
        });

        self.prev_line = line;
        self.prev_char = start_char;
    }

    fn build(self) -> Vec<SemanticToken> {
        self.tokens
    }
}

/// Token type indices
const TT_NAMESPACE: u32 = 0;
const TT_TYPE: u32 = 1;
const TT_CLASS: u32 = 2;
const TT_FUNCTION: u32 = 3;
const TT_METHOD: u32 = 4;
const TT_VARIABLE: u32 = 6;
const TT_STRING: u32 = 7;
const TT_NUMBER: u32 = 8;
const TT_KEYWORD: u32 = 9;
const TT_COMMENT: u32 = 11;
const TT_MACRO: u32 = 12;
const TT_ENUM_MEMBER: u32 = 14;

/// Modifier bit flags
const MOD_DECLARATION: u32 = 1 << 0;
const MOD_DEFINITION: u32 = 1 << 1;
const MOD_DEFAULT_LIBRARY: u32 = 1 << 5;

/// Tokenize a single line.
fn tokenize_line(
    builder: &mut SemanticTokenBuilder,
    line_num: u32,
    line: &str,
    analysis: Option<&AnalysisResult>,
) {
    let mut pos = 0;
    let chars: Vec<char> = line.chars().collect();

    while pos < chars.len() {
        // Skip whitespace
        if chars[pos].is_whitespace() {
            pos += 1;
            continue;
        }

        // Comments
        if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '/' {
            let length = (chars.len() - pos) as u32;
            builder.push(line_num, pos as u32, length, TT_COMMENT, 0);
            break;
        }

        // Strings
        if chars[pos] == '"' {
            if let Some(end) = find_string_end(&chars, pos) {
                let length = (end - pos + 1) as u32;
                let content: String = chars[pos + 1..end].iter().collect();

                // Check if it's a note pattern or melody
                let token_type = if looks_like_notes(&content) {
                    TT_ENUM_MEMBER
                } else if looks_like_pattern(&content) {
                    TT_MACRO
                } else {
                    TT_STRING
                };

                builder.push(line_num, pos as u32, length, token_type, 0);
                pos = end + 1;
                continue;
            }
        }

        // Numbers
        if chars[pos].is_ascii_digit()
            || (chars[pos] == '-' && pos + 1 < chars.len() && chars[pos + 1].is_ascii_digit())
        {
            let start = pos;
            if chars[pos] == '-' {
                pos += 1;
            }
            while pos < chars.len() && (chars[pos].is_ascii_digit() || chars[pos] == '.') {
                pos += 1;
            }
            let length = (pos - start) as u32;
            builder.push(line_num, start as u32, length, TT_NUMBER, 0);
            continue;
        }

        // Identifiers and keywords
        if chars[pos].is_alphabetic() || chars[pos] == '_' {
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                pos += 1;
            }
            let word: String = chars[start..pos].iter().collect();
            let length = (pos - start) as u32;

            let (token_type, modifiers) = classify_identifier(&word, line, start, analysis);
            builder.push(line_num, start as u32, length, token_type, modifiers);
            continue;
        }

        // Method calls (after a dot)
        if chars[pos] == '.' {
            pos += 1;
            if pos < chars.len() && (chars[pos].is_alphabetic() || chars[pos] == '_') {
                let start = pos;
                while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                    pos += 1;
                }
                let length = (pos - start) as u32;
                builder.push(line_num, start as u32, length, TT_METHOD, 0);
                continue;
            }
            continue;
        }

        // Skip other characters
        pos += 1;
    }
}

/// Find the end of a string literal (handling escapes).
fn find_string_end(chars: &[char], start: usize) -> Option<usize> {
    let mut pos = start + 1;
    while pos < chars.len() {
        if chars[pos] == '\\' && pos + 1 < chars.len() {
            pos += 2;
        } else if chars[pos] == '"' {
            return Some(pos);
        } else {
            pos += 1;
        }
    }
    None
}

/// Classify an identifier.
fn classify_identifier(
    word: &str,
    line: &str,
    _pos: usize,
    analysis: Option<&AnalysisResult>,
) -> (u32, u32) {
    // Keywords
    let keywords = [
        "let", "fn", "if", "else", "for", "while", "loop", "break", "continue", "return", "true",
        "false", "import", "export", "const", "mut",
    ];
    if keywords.contains(&word) {
        return (TT_KEYWORD, 0);
    }

    // Built-in functions (from stdlib)
    let builtins = [
        "voice",
        "pattern",
        "melody",
        "sequence",
        "group",
        "define_group",
        "set_tempo",
        "set_time_signature",
        "set_quantization",
        "load_sample",
        "load_sfz",
        "define_synthdef",
        "define_fx",
        "at_bar",
        "every",
        "after",
        "fade_in",
        "fade_out",
        "midi_out",
        "midi_in",
        "record",
        "export_audio",
    ];
    if builtins.contains(&word) {
        return (TT_FUNCTION, MOD_DEFAULT_LIBRARY);
    }

    // Check if it's a definition
    if line.contains(&format!("let {} =", word)) || line.contains(&format!("let {}=", word)) {
        return (TT_VARIABLE, MOD_DECLARATION);
    }

    // Check against analysis results
    if let Some(analysis) = analysis {
        if analysis.voice_defs.contains(word) {
            return (TT_FUNCTION, MOD_DEFINITION);
        }
        if analysis.pattern_defs.contains(word) {
            return (TT_MACRO, MOD_DEFINITION);
        }
        if analysis.melody_defs.contains(word) {
            return (TT_MACRO, MOD_DEFINITION);
        }
        if analysis.group_defs.contains(word) {
            return (TT_NAMESPACE, MOD_DEFINITION);
        }
        if analysis.local_synthdefs.contains(word) {
            return (TT_TYPE, MOD_DEFINITION);
        }
        if analysis.local_effects.contains(word) {
            return (TT_CLASS, MOD_DEFINITION);
        }
    }

    // Default to variable
    (TT_VARIABLE, 0)
}

/// Check if string content looks like note names.
fn looks_like_notes(content: &str) -> bool {
    let note_re = regex::Regex::new(r"^[a-gA-G][#b]?\d").ok();
    note_re.map(|r| r.is_match(content.trim())).unwrap_or(false)
}

/// Check if string content looks like a pattern.
fn looks_like_pattern(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Pattern chars: x, -, ., |, space, numbers
    trimmed.chars().all(|c| {
        c == 'x'
            || c == 'X'
            || c == '-'
            || c == '.'
            || c == '|'
            || c == ' '
            || c.is_ascii_digit()
            || c == 'o'
            || c == 'O'
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_legend() {
        let legend = get_legend();
        assert!(!legend.token_types.is_empty());
        assert!(!legend.token_modifiers.is_empty());
    }

    #[test]
    fn test_looks_like_notes() {
        assert!(looks_like_notes("c4"));
        assert!(looks_like_notes("C#5"));
        assert!(looks_like_notes("Db3 Eb4"));
        assert!(!looks_like_notes("hello"));
        assert!(!looks_like_notes("x-x-"));
    }

    #[test]
    fn test_looks_like_pattern() {
        assert!(looks_like_pattern("x-x- x-x-"));
        assert!(looks_like_pattern("xxxx ----"));
        assert!(looks_like_pattern("x...x..."));
        assert!(!looks_like_pattern("c4 e4 g4"));
        assert!(!looks_like_pattern("hello"));
    }

    #[test]
    fn test_semantic_tokens_simple() {
        let content = r#"let x = 42;"#;
        let tokens = get_semantic_tokens(content, None);
        // Should have tokens for: let (keyword), x (variable), 42 (number)
        assert!(!tokens.data.is_empty());
    }

    #[test]
    fn test_find_string_end() {
        let chars: Vec<char> = r#""hello""#.chars().collect();
        assert_eq!(find_string_end(&chars, 0), Some(6));

        let chars: Vec<char> = r#""hello\"world""#.chars().collect();
        assert_eq!(find_string_end(&chars, 0), Some(13));
    }
}
