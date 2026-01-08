//! Find references provider for VibeLang.
//!
//! Finds all references to a symbol (variable, voice, pattern, etc.) in a document.

use tower_lsp::lsp_types::{Location, Position, Range, Url};

/// Find all references to a symbol in the document.
pub fn find_references(
    content: &str,
    symbol: &str,
    uri: &Url,
    include_declaration: bool,
) -> Vec<Location> {
    let mut references = Vec::new();

    // Skip empty symbols
    if symbol.is_empty() {
        return references;
    }

    // Find all occurrences of the symbol
    for (line_num, line) in content.lines().enumerate() {
        let mut search_start = 0;

        while let Some(col) = line[search_start..].find(symbol) {
            let absolute_col = search_start + col;

            // Check if this is a whole word match (not part of a larger identifier)
            let before_ok = absolute_col == 0
                || !is_identifier_char(line.chars().nth(absolute_col - 1).unwrap_or(' '));
            let after_ok = absolute_col + symbol.len() >= line.len()
                || !is_identifier_char(
                    line.chars().nth(absolute_col + symbol.len()).unwrap_or(' '),
                );

            if before_ok && after_ok {
                // Check if this is a declaration (let symbol = ...)
                let is_declaration = is_declaration_site(line, absolute_col, symbol);

                if include_declaration || !is_declaration {
                    references.push(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: absolute_col as u32,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: (absolute_col + symbol.len()) as u32,
                            },
                        },
                    });
                }
            }

            search_start = absolute_col + 1;
        }
    }

    references
}

/// Check if a character is a valid identifier character.
fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Check if this occurrence is a declaration site (let x = ...).
fn is_declaration_site(line: &str, col: usize, symbol: &str) -> bool {
    let before = &line[..col].trim_end();

    // Check for "let symbol" pattern
    if before.ends_with("let") {
        return true;
    }

    // Check for "let symbol" with whitespace
    if let Some(let_pos) = before.rfind("let ") {
        let between = &before[let_pos + 4..];
        if between.chars().all(|c| c.is_whitespace()) {
            return true;
        }
    }

    // Check for closure parameter (|symbol| or |a, symbol|)
    if let Some(pipe_pos) = before.rfind('|') {
        let after_pipe = &before[pipe_pos + 1..];
        // Check if symbol appears in parameter list
        let params: Vec<&str> = after_pipe.split(',').map(|s| s.trim()).collect();
        if params.iter().any(|p| *p == symbol || p.starts_with(symbol)) {
            return true;
        }
    }

    // Check for for loop variable (for symbol in ...)
    if before.ends_with("for") || before.contains("for ") {
        if let Some(for_pos) = before.rfind("for ") {
            let between = &before[for_pos + 4..];
            if between.trim() == symbol || between.trim().is_empty() {
                return true;
            }
        }
    }

    false
}

/// Find references to a named entity (voice, pattern, melody, group, etc.).
/// These are referenced by string literals like voice("kick") or .on("kick").
pub fn find_entity_references(
    content: &str,
    entity_name: &str,
    uri: &Url,
    include_declaration: bool,
) -> Vec<Location> {
    let mut references = Vec::new();

    // Patterns to find entity references
    let patterns = [
        format!(r#"voice("{}")"#, entity_name),
        format!(r#"voice('{}')"#, entity_name),
        format!(r#"pattern("{}")"#, entity_name),
        format!(r#"pattern('{}')"#, entity_name),
        format!(r#"melody("{}")"#, entity_name),
        format!(r#"melody('{}')"#, entity_name),
        format!(r#"sequence("{}")"#, entity_name),
        format!(r#"sequence('{}')"#, entity_name),
        format!(r#"group("{}")"#, entity_name),
        format!(r#"group('{}')"#, entity_name),
        format!(r#"define_group("{}")"#, entity_name),
        format!(r#"define_group('{}')"#, entity_name),
        format!(r#"define_synthdef("{}")"#, entity_name),
        format!(r#"define_synthdef('{}')"#, entity_name),
        format!(r#"define_fx("{}")"#, entity_name),
        format!(r#"define_fx('{}')"#, entity_name),
        format!(r#".on("{}")"#, entity_name),
        format!(r#".on('{}')"#, entity_name),
        format!(r#".on_voice("{}")"#, entity_name),
        format!(r#".on_voice('{}')"#, entity_name),
        format!(r#".on_group("{}")"#, entity_name),
        format!(r#".on_group('{}')"#, entity_name),
        format!(r#".synth("{}")"#, entity_name),
        format!(r#".synth('{}')"#, entity_name),
        format!(r#".from_group("{}")"#, entity_name),
        format!(r#".from_group('{}')"#, entity_name),
    ];

    // Declaration patterns (these define the entity)
    let declaration_patterns = [
        format!(r#"voice("{}")"#, entity_name),
        format!(r#"pattern("{}")"#, entity_name),
        format!(r#"melody("{}")"#, entity_name),
        format!(r#"sequence("{}")"#, entity_name),
        format!(r#"define_group("{}")"#, entity_name),
        format!(r#"define_synthdef("{}")"#, entity_name),
        format!(r#"define_fx("{}")"#, entity_name),
    ];

    for (line_num, line) in content.lines().enumerate() {
        for pattern in &patterns {
            if let Some(col) = line.find(pattern.as_str()) {
                // Find the position of the entity name within the pattern
                let name_offset = pattern.find(entity_name).unwrap_or(0);
                let name_col = col + name_offset;

                // Check if this is a declaration
                let is_declaration = declaration_patterns
                    .iter()
                    .any(|dp| line.contains(dp.as_str()));

                if include_declaration || !is_declaration {
                    references.push(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: name_col as u32,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: (name_col + entity_name.len()) as u32,
                            },
                        },
                    });
                }
            }
        }
    }

    // Deduplicate (same location might be found multiple times)
    references.sort_by(|a, b| {
        a.range
            .start
            .line
            .cmp(&b.range.start.line)
            .then(a.range.start.character.cmp(&b.range.start.character))
    });
    references.dedup_by(|a, b| {
        a.range.start.line == b.range.start.line
            && a.range.start.character == b.range.start.character
    });

    references
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_variable_references() {
        let content = r#"
let my_kick = voice("kick_drum");
pattern("p").on(my_kick).start();
melody("m").on(my_kick).start();
"#;
        let uri = Url::parse("file:///test.vibe").unwrap();
        let refs = find_references(content, "my_kick", &uri, true);
        assert_eq!(refs.len(), 3);
    }

    #[test]
    fn test_find_entity_references() {
        let content = r#"
let v = voice("kick").synth("kick_808");
pattern("beat").on("kick").start();
"#;
        let uri = Url::parse("file:///test.vibe").unwrap();
        let refs = find_entity_references(content, "kick", &uri, true);
        assert!(refs.len() >= 2);
    }

    #[test]
    fn test_is_declaration_site() {
        assert!(is_declaration_site("let kick = voice", 4, "kick"));
        assert!(is_declaration_site("  let  kick = voice", 7, "kick"));
        assert!(!is_declaration_site("pattern.on(kick)", 11, "kick"));
    }
}
