//! Workspace symbols provider for VibeLang.
//!
//! Provides symbol search across all open documents.

use tower_lsp::lsp_types::{Location, Position, Range, SymbolInformation, SymbolKind, Url};

use crate::data::DocumentStore;

/// Get workspace symbols matching a query.
pub fn get_workspace_symbols(query: &str, documents: &DocumentStore) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();
    let query_lower = query.to_lowercase();

    // Search all open documents
    for uri in documents.uris() {
        if let Some(doc) = documents.get(&uri) {
            let content = doc.text();

            // Extract symbols from this document
            let doc_symbols = extract_symbols_from_content(&content, &uri);

            // Filter by query
            for symbol in doc_symbols {
                if query.is_empty() || symbol.name.to_lowercase().contains(&query_lower) {
                    symbols.push(symbol);
                }
            }
        }
    }

    // Sort by relevance (exact matches first, then prefix matches, then contains)
    symbols.sort_by(|a, b| {
        let a_exact = a.name.to_lowercase() == query_lower;
        let b_exact = b.name.to_lowercase() == query_lower;
        let a_prefix = a.name.to_lowercase().starts_with(&query_lower);
        let b_prefix = b.name.to_lowercase().starts_with(&query_lower);

        match (a_exact, b_exact) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => match (a_prefix, b_prefix) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            },
        }
    });

    symbols
}

/// Extract all symbols from document content.
#[allow(deprecated)] // SymbolInformation.deprecated field
fn extract_symbols_from_content(content: &str, uri: &Url) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        // Voice definitions: voice("name")
        if let Some(name) = extract_quoted_name(line, "voice(") {
            symbols.push(SymbolInformation {
                name: name.clone(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: make_range(line_num, line, &format!("voice(\"{name}\"")),
                },
                container_name: Some("Voices".to_string()),
            });
        }

        // Pattern definitions: pattern("name")
        if let Some(name) = extract_quoted_name(line, "pattern(") {
            symbols.push(SymbolInformation {
                name: name.clone(),
                kind: SymbolKind::EVENT,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: make_range(line_num, line, &format!("pattern(\"{name}\"")),
                },
                container_name: Some("Patterns".to_string()),
            });
        }

        // Melody definitions: melody("name")
        if let Some(name) = extract_quoted_name(line, "melody(") {
            symbols.push(SymbolInformation {
                name: name.clone(),
                kind: SymbolKind::EVENT,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: make_range(line_num, line, &format!("melody(\"{name}\"")),
                },
                container_name: Some("Melodies".to_string()),
            });
        }

        // Sequence definitions: sequence("name")
        if let Some(name) = extract_quoted_name(line, "sequence(") {
            symbols.push(SymbolInformation {
                name: name.clone(),
                kind: SymbolKind::EVENT,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: make_range(line_num, line, &format!("sequence(\"{name}\"")),
                },
                container_name: Some("Sequences".to_string()),
            });
        }

        // Group definitions: define_group("name", ...)
        if let Some(name) = extract_quoted_name(line, "define_group(") {
            symbols.push(SymbolInformation {
                name: name.clone(),
                kind: SymbolKind::MODULE,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: make_range(line_num, line, &format!("define_group(\"{name}\"")),
                },
                container_name: Some("Groups".to_string()),
            });
        }

        // Synthdef definitions: define_synthdef("name")
        if let Some(name) = extract_quoted_name(line, "define_synthdef(") {
            symbols.push(SymbolInformation {
                name: name.clone(),
                kind: SymbolKind::CLASS,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: make_range(line_num, line, &format!("define_synthdef(\"{name}\"")),
                },
                container_name: Some("Synthdefs".to_string()),
            });
        }

        // Effect definitions: define_fx("name")
        if let Some(name) = extract_quoted_name(line, "define_fx(") {
            symbols.push(SymbolInformation {
                name: name.clone(),
                kind: SymbolKind::CLASS,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: make_range(line_num, line, &format!("define_fx(\"{name}\"")),
                },
                container_name: Some("Effects".to_string()),
            });
        }

        // Variable definitions: let name = ...
        if let Some(var_name) = extract_variable_name(line) {
            symbols.push(SymbolInformation {
                name: var_name.clone(),
                kind: SymbolKind::VARIABLE,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: make_range(line_num, line, &format!("let {var_name}")),
                },
                container_name: None,
            });
        }
    }

    symbols
}

/// Extract a quoted name from a function call like `voice("name")`.
fn extract_quoted_name(line: &str, prefix: &str) -> Option<String> {
    let start = line.find(prefix)?;
    let after_prefix = &line[start + prefix.len()..];

    // Find opening quote
    let quote_char = after_prefix.chars().next()?;
    if quote_char != '"' && quote_char != '\'' {
        return None;
    }

    // Find closing quote
    let rest = &after_prefix[1..];
    let end = rest.find(quote_char)?;

    Some(rest[..end].to_string())
}

/// Extract variable name from `let name = ...`.
fn extract_variable_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("let ") {
        return None;
    }

    let after_let = &trimmed[4..].trim_start();
    let name_end = after_let
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(after_let.len());

    if name_end == 0 {
        return None;
    }

    Some(after_let[..name_end].to_string())
}

/// Create a range for a symbol.
fn make_range(line_num: usize, line: &str, pattern: &str) -> Range {
    let start_char = line.find(pattern).unwrap_or(0);
    Range {
        start: Position {
            line: line_num as u32,
            character: start_char as u32,
        },
        end: Position {
            line: line_num as u32,
            character: (start_char + pattern.len()) as u32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_quoted_name() {
        assert_eq!(
            extract_quoted_name(r#"voice("kick")"#, "voice("),
            Some("kick".to_string())
        );
        assert_eq!(
            extract_quoted_name(r#"define_synthdef("my_synth")"#, "define_synthdef("),
            Some("my_synth".to_string())
        );
    }

    #[test]
    fn test_extract_variable_name() {
        assert_eq!(
            extract_variable_name("let kick = voice(\"kick\");"),
            Some("kick".to_string())
        );
        assert_eq!(
            extract_variable_name("  let my_var = 42;"),
            Some("my_var".to_string())
        );
        assert_eq!(extract_variable_name("voice(\"kick\");"), None);
    }

    #[test]
    fn test_extract_symbols() {
        let content = r#"
let kick = voice("kick").synth("kick_808");
pattern("beat").on(kick).step("x...x...").start();
define_synthdef("my_synth").param("freq", 440).body(|freq| sin_osc_ar(freq));
"#;
        let uri = Url::parse("file:///test.vibe").unwrap();
        let symbols = extract_symbols_from_content(content, &uri);

        assert!(symbols
            .iter()
            .any(|s| s.name == "kick" && s.kind == SymbolKind::VARIABLE));
        assert!(symbols
            .iter()
            .any(|s| s.name == "kick" && s.kind == SymbolKind::FUNCTION));
        assert!(symbols.iter().any(|s| s.name == "beat"));
        assert!(symbols.iter().any(|s| s.name == "my_synth"));
    }
}
