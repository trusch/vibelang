//! Utility functions for the LSP server.
//!
//! This module provides helper functions for text manipulation,
//! position conversion, and other common operations.

use tower_lsp::lsp_types::{Position, Range};

/// Convert a byte offset to an LSP position.
pub fn offset_to_position(content: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut col = 0;
    let mut current_offset = 0;

    for c in content.chars() {
        if current_offset >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        current_offset += c.len_utf8();
    }

    Position {
        line,
        character: col,
    }
}

/// Convert an LSP position to a byte offset.
pub fn position_to_offset(content: &str, position: Position) -> usize {
    let mut offset = 0;
    let mut current_line = 0;
    let mut current_col = 0;

    for c in content.chars() {
        if current_line == position.line && current_col == position.character {
            break;
        }
        if c == '\n' {
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
        offset += c.len_utf8();
    }

    offset
}

/// Get the line at the given position.
pub fn get_line_at_position(content: &str, line: u32) -> Option<&str> {
    content.lines().nth(line as usize)
}

/// Get the word at the given position.
pub fn get_word_at_position(content: &str, position: Position) -> Option<String> {
    let line = get_line_at_position(content, position.line)?;
    let col = position.character as usize;

    if col > line.len() {
        return None;
    }

    // Find word boundaries
    let chars: Vec<char> = line.chars().collect();
    let mut start = col;
    let mut end = col;

    // Scan backwards
    while start > 0 && is_word_char(chars.get(start - 1).copied()?) {
        start -= 1;
    }

    // Scan forwards
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

/// Get the range of a word at the given position.
pub fn get_word_range_at_position(content: &str, position: Position) -> Option<Range> {
    let line = get_line_at_position(content, position.line)?;
    let col = position.character as usize;

    if col > line.len() {
        return None;
    }

    let chars: Vec<char> = line.chars().collect();
    let mut start = col;
    let mut end = col;

    while start > 0 && is_word_char(chars.get(start - 1).copied()?) {
        start -= 1;
    }

    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    Some(Range {
        start: Position {
            line: position.line,
            character: start as u32,
        },
        end: Position {
            line: position.line,
            character: end as u32,
        },
    })
}

/// Extract the string at a given range.
pub fn get_text_at_range(content: &str, range: Range) -> String {
    let lines: Vec<&str> = content.lines().collect();

    if range.start.line == range.end.line {
        if let Some(line) = lines.get(range.start.line as usize) {
            let start = range.start.character as usize;
            let end = range.end.character as usize;
            if start <= line.len() && end <= line.len() {
                return line[start..end].to_string();
            }
        }
        return String::new();
    }

    let mut result = String::new();
    for line_num in range.start.line..=range.end.line {
        if let Some(line) = lines.get(line_num as usize) {
            if line_num == range.start.line {
                let start = range.start.character as usize;
                if start < line.len() {
                    result.push_str(&line[start..]);
                }
            } else if line_num == range.end.line {
                let end = range.end.character as usize;
                if end <= line.len() {
                    result.push_str(&line[..end]);
                }
            } else {
                result.push_str(line);
            }
            if line_num < range.end.line {
                result.push('\n');
            }
        }
    }

    result
}

/// Clean up a message for display.
pub fn clean_error_message(msg: &str) -> String {
    // Remove Rhai-specific prefix
    let msg = msg
        .strip_prefix("Runtime error: ")
        .or_else(|| msg.strip_prefix("Script error: "))
        .or_else(|| msg.strip_prefix("Parse error: "))
        .unwrap_or(msg);

    // Truncate long messages
    if msg.len() > 200 {
        format!("{}...", &msg[..197])
    } else {
        msg.to_string()
    }
}

/// Check if a position is inside a string literal.
pub fn is_inside_string(line: &str, col: usize) -> bool {
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in line.char_indices() {
        if i >= col {
            break;
        }
        if escape_next {
            escape_next = false;
            continue;
        }
        match c {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            _ => {}
        }
    }

    in_string
}

/// Check if a position is inside a comment.
pub fn is_inside_comment(line: &str, col: usize) -> bool {
    let before_cursor = &line[..col.min(line.len())];

    // Check for line comment
    if let Some(comment_pos) = before_cursor.find("//") {
        // Make sure it's not inside a string
        let before_comment = &before_cursor[..comment_pos];
        let quote_count = before_comment.matches('"').count();
        // If even number of quotes, the // is not in a string
        return quote_count % 2 == 0;
    }

    false
}

/// Calculate Levenshtein distance between two strings.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

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

/// Find similar strings from a list (for "did you mean" suggestions).
pub fn find_similar<'a>(target: &str, candidates: &'a [&str], max_distance: usize) -> Vec<&'a str> {
    let target_lower = target.to_lowercase();
    let mut matches: Vec<_> = candidates
        .iter()
        .filter_map(|&c| {
            let dist = levenshtein_distance(&target_lower, &c.to_lowercase());
            if dist <= max_distance {
                Some((c, dist))
            } else {
                None
            }
        })
        .collect();

    matches.sort_by_key(|(_, dist)| *dist);
    matches.into_iter().map(|(s, _)| s).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_to_position() {
        let content = "line1\nline2\nline3";
        assert_eq!(
            offset_to_position(content, 0),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            offset_to_position(content, 6),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            offset_to_position(content, 8),
            Position {
                line: 1,
                character: 2
            }
        );
    }

    #[test]
    fn test_position_to_offset() {
        let content = "line1\nline2\nline3";
        assert_eq!(
            position_to_offset(
                content,
                Position {
                    line: 0,
                    character: 0
                }
            ),
            0
        );
        assert_eq!(
            position_to_offset(
                content,
                Position {
                    line: 1,
                    character: 0
                }
            ),
            6
        );
    }

    #[test]
    fn test_get_word_at_position() {
        let content = "let foo_bar = 42;";
        let word = get_word_at_position(
            content,
            Position {
                line: 0,
                character: 6,
            },
        );
        assert_eq!(word, Some("foo_bar".to_string()));
    }

    #[test]
    fn test_is_inside_string() {
        assert!(!is_inside_string("hello", 3));
        assert!(is_inside_string("\"hello\"", 3));
        assert!(!is_inside_string("\"hello\"", 7));
        assert!(is_inside_string("\"hello\\\"world\"", 8));
    }

    #[test]
    fn test_is_inside_comment() {
        assert!(!is_inside_comment("let x = 5;", 5));
        assert!(is_inside_comment("let x = 5; // comment", 15));
        assert!(!is_inside_comment("let x = \"// not a comment\"", 15));
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
    }

    #[test]
    fn test_find_similar() {
        let candidates = vec!["kick", "snare", "hihat", "clap"];
        let similar = find_similar("kik", &candidates, 2);
        assert!(similar.contains(&"kick"));
    }
}
