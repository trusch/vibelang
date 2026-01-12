//! Rename symbol provider for VibeLang.
//!
//! Supports renaming variables, voices, patterns, melodies, groups, etc.

use std::collections::HashMap;

use tower_lsp::lsp_types::{Position, PrepareRenameResponse, Range, TextEdit, Url, WorkspaceEdit};

use super::references::{find_entity_references, find_references};

/// Prepare rename - validate that the symbol can be renamed.
pub fn prepare_rename(content: &str, position: Position) -> Option<PrepareRenameResponse> {
    let word = get_word_at_position(content, position)?;

    // Check if this is a valid renameable symbol
    if word.is_empty() {
        return None;
    }

    // Don't allow renaming keywords
    if is_keyword(&word) {
        return None;
    }

    // Find the range of the word
    let line = content.lines().nth(position.line as usize)?;
    let char_pos = position.character as usize;

    let start = line[..char_pos]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = line[char_pos..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| char_pos + i)
        .unwrap_or(line.len());

    Some(PrepareRenameResponse::Range(Range {
        start: Position {
            line: position.line,
            character: start as u32,
        },
        end: Position {
            line: position.line,
            character: end as u32,
        },
    }))
}

/// Rename a symbol throughout the document.
pub fn rename_symbol(
    content: &str,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let old_name = get_word_at_position(content, position)?;

    if old_name.is_empty() || new_name.is_empty() {
        return None;
    }

    // Validate new name
    if !is_valid_identifier(new_name) {
        return None;
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    let mut edits = Vec::new();

    // Determine if this is a variable or an entity name
    let line = content.lines().nth(position.line as usize)?;
    let is_in_string = is_position_in_string(line, position.character as usize);

    if is_in_string {
        // This is an entity name (inside quotes)
        let refs = find_entity_references(content, &old_name, uri, true);
        for location in refs {
            edits.push(TextEdit {
                range: location.range,
                new_text: new_name.to_string(),
            });
        }
    } else {
        // This is a variable name
        let refs = find_references(content, &old_name, uri, true);
        for location in refs {
            edits.push(TextEdit {
                range: location.range,
                new_text: new_name.to_string(),
            });
        }
    }

    if edits.is_empty() {
        return None;
    }

    // Sort edits from bottom to top to avoid position shifts during apply
    edits.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then(b.range.start.character.cmp(&a.range.start.character))
    });

    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

/// Get the word at a position.
fn get_word_at_position(content: &str, position: Position) -> Option<String> {
    let line = content.lines().nth(position.line as usize)?;
    let char_pos = position.character as usize;

    if char_pos > line.len() {
        return None;
    }

    // Check if we're inside a string (for entity names)
    if is_position_in_string(line, char_pos) {
        // Extract the string content
        return extract_string_at_position(line, char_pos);
    }

    // Find word boundaries for identifiers
    let start = line[..char_pos]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = line[char_pos..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| char_pos + i)
        .unwrap_or(line.len());

    if start >= end {
        return None;
    }

    Some(line[start..end].to_string())
}

/// Check if position is inside a string literal.
fn is_position_in_string(line: &str, char_pos: usize) -> bool {
    let mut in_string = false;
    let mut escape_next = false;
    let mut quote_char = '"';

    for (i, c) in line.char_indices() {
        if i >= char_pos {
            return in_string;
        }

        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => escape_next = true,
            '"' | '\'' => {
                if !in_string {
                    in_string = true;
                    quote_char = c;
                } else if c == quote_char {
                    in_string = false;
                }
            }
            _ => {}
        }
    }

    in_string
}

/// Extract the string content at a position.
fn extract_string_at_position(line: &str, char_pos: usize) -> Option<String> {
    // Find the opening quote
    let before = &line[..char_pos];
    let quote_start = before.rfind(['"', '\''])?;
    let quote_char = before.chars().nth(quote_start)?;

    // Find the closing quote
    let after = &line[quote_start + 1..];
    let quote_end = after.find(quote_char)?;

    Some(after[..quote_end].to_string())
}

/// Check if a string is a valid identifier.
fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let first = s.chars().next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }

    s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Check if a string is a keyword.
fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "let"
            | "if"
            | "else"
            | "for"
            | "while"
            | "loop"
            | "fn"
            | "return"
            | "true"
            | "false"
            | "in"
            | "break"
            | "continue"
            | "import"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_word_at_position() {
        let content = "let kick = voice(\"kick\");";
        let pos = Position {
            line: 0,
            character: 5,
        };
        assert_eq!(get_word_at_position(content, pos), Some("kick".to_string()));
    }

    #[test]
    fn test_get_entity_name_at_position() {
        let content = "let kick = voice(\"kick_drum\");";
        let pos = Position {
            line: 0,
            character: 20,
        };
        assert_eq!(
            get_word_at_position(content, pos),
            Some("kick_drum".to_string())
        );
    }

    #[test]
    fn test_is_position_in_string() {
        let line = r#"voice("kick")"#;
        assert!(!is_position_in_string(line, 0));
        assert!(is_position_in_string(line, 8));
        assert!(!is_position_in_string(line, 12));
    }

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("kick"));
        assert!(is_valid_identifier("kick_drum"));
        assert!(is_valid_identifier("_private"));
        assert!(!is_valid_identifier("123abc"));
        assert!(!is_valid_identifier(""));
    }

    #[test]
    fn test_prepare_rename() {
        let content = "let kick = voice(\"kick\");";
        let pos = Position {
            line: 0,
            character: 5,
        };
        let result = prepare_rename(content, pos);
        assert!(result.is_some());
    }
}
