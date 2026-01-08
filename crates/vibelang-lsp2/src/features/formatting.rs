//! Document formatting provider for VibeLang.
//!
//! Provides basic code formatting:
//! - Consistent indentation
//! - Proper spacing
//! - Method chain alignment

use tower_lsp::lsp_types::{Position, Range, TextEdit};

/// Format an entire document.
pub fn format_document(content: &str, tab_size: u32, insert_spaces: bool) -> Vec<TextEdit> {
    let formatted = format_content(content, tab_size, insert_spaces);

    if formatted == content {
        return vec![];
    }

    // Return a single edit that replaces the entire document
    let line_count = content.lines().count();
    let last_line_len = content.lines().last().map(|l| l.len()).unwrap_or(0);

    vec![TextEdit {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: line_count as u32,
                character: last_line_len as u32,
            },
        },
        new_text: formatted,
    }]
}

/// Format the content.
fn format_content(content: &str, tab_size: u32, insert_spaces: bool) -> String {
    let indent_str = if insert_spaces {
        " ".repeat(tab_size as usize)
    } else {
        "\t".to_string()
    };

    let mut result = String::new();
    let mut indent_level: i32 = 0;
    let mut in_multiline_string = false;
    let mut prev_line_ends_with_open = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Handle multiline strings (backticks)
        let backtick_count = line.chars().filter(|c| *c == '`').count();
        if backtick_count % 2 == 1 {
            in_multiline_string = !in_multiline_string;
        }

        // Don't format inside multiline strings
        if in_multiline_string {
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // Skip empty lines but preserve them
        if trimmed.is_empty() {
            result.push('\n');
            prev_line_ends_with_open = false;
            continue;
        }

        // Adjust indent for closing braces/parens at start of line
        let starts_with_close = trimmed.starts_with('}')
            || trimmed.starts_with(')')
            || trimmed.starts_with(']')
            || trimmed.starts_with("});")
            || trimmed.starts_with(");");

        if starts_with_close && indent_level > 0 {
            indent_level -= 1;
        }

        // Apply indentation
        let current_indent = indent_str.repeat(indent_level as usize);

        // Format the line content
        let formatted_line = format_line(trimmed);

        result.push_str(&current_indent);
        result.push_str(&formatted_line);
        result.push('\n');

        // Adjust indent for next line based on this line
        let opens = count_unmatched_opens(trimmed);
        let closes = count_unmatched_closes(trimmed);

        indent_level += opens - closes;
        if indent_level < 0 {
            indent_level = 0;
        }

        // Track if line ends with open brace/paren for method chains
        prev_line_ends_with_open = trimmed.ends_with('{')
            || trimmed.ends_with('(')
            || trimmed.ends_with("||")
            || trimmed.ends_with(',');
    }

    // Remove trailing newline if original didn't have one
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

/// Format a single line.
fn format_line(line: &str) -> String {
    let mut result = String::new();
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    let mut string_char = '"';

    while let Some(c) = chars.next() {
        // Track string literals
        if (c == '"' || c == '\'' || c == '`') && !in_string {
            in_string = true;
            string_char = c;
            result.push(c);
            continue;
        }

        if c == string_char && in_string {
            in_string = false;
            result.push(c);
            continue;
        }

        // Don't format inside strings
        if in_string {
            result.push(c);
            continue;
        }

        match c {
            // Ensure space after commas
            ',' => {
                result.push(',');
                if chars.peek().is_some_and(|&next| next != ' ' && next != '\n') {
                    result.push(' ');
                }
            }

            // Ensure space around operators
            '=' => {
                // Check for ==, !=, <=, >=
                if chars.peek() == Some(&'=') {
                    // Don't add space before ==
                    if !result.ends_with(' ') && !result.is_empty() {
                        result.push(' ');
                    }
                    result.push('=');
                    result.push(chars.next().unwrap());
                    if chars.peek().is_some_and(|&next| next != ' ') {
                        result.push(' ');
                    }
                } else {
                    // Single = (assignment)
                    if !result.ends_with(' ') && !result.is_empty() {
                        result.push(' ');
                    }
                    result.push('=');
                    if chars.peek().is_some_and(|&next| next != ' ' && next != '=') {
                        result.push(' ');
                    }
                }
            }

            // Colon for method chains - no extra spacing
            ':' => {
                result.push(':');
            }

            // Semicolons - no space before
            ';' => {
                // Remove trailing space before semicolon
                while result.ends_with(' ') {
                    result.pop();
                }
                result.push(';');
            }

            // Dots for method chains - no spacing
            '.' => {
                // Remove trailing space before dot
                while result.ends_with(' ') {
                    result.pop();
                }
                result.push('.');
            }

            // Opening parens/braces
            '(' | '{' | '[' => {
                result.push(c);
            }

            // Closing parens/braces
            ')' | '}' | ']' => {
                result.push(c);
            }

            // Pipe for closures
            '|' => {
                result.push('|');
            }

            // Arithmetic operators
            '+' | '-' | '*' | '/' | '%' => {
                // Check for compound assignment (+=, -=, etc.)
                if chars.peek() == Some(&'=') {
                    if !result.ends_with(' ') && !result.is_empty() {
                        result.push(' ');
                    }
                    result.push(c);
                    result.push(chars.next().unwrap());
                    if chars.peek().is_some_and(|&next| next != ' ') {
                        result.push(' ');
                    }
                } else {
                    // Regular operator - check context
                    // Don't add spaces for negative numbers or ranges
                    let prev_is_operator = result.ends_with(&['(', ',', '[', '=', '<', '>'][..]);
                    if c == '-' && prev_is_operator {
                        result.push(c);
                    } else if c == '.' && chars.peek() == Some(&'.') {
                        // Range operator ..
                        result.push(c);
                    } else {
                        if !result.ends_with(' ') && !result.is_empty() && !result.ends_with('(') {
                            result.push(' ');
                        }
                        result.push(c);
                        if chars.peek().is_some_and(|&next| next != ' ' && next != '=' && !(c == '-' && next.is_ascii_digit())) {
                            result.push(' ');
                        }
                    }
                }
            }

            // Default: just copy the character
            _ => {
                result.push(c);
            }
        }
    }

    result
}

/// Count unmatched opening braces/parens on a line.
fn count_unmatched_opens(line: &str) -> i32 {
    let mut count = 0;
    let mut in_string = false;
    let mut string_char = '"';

    for c in line.chars() {
        if (c == '"' || c == '\'' || c == '`') && !in_string {
            in_string = true;
            string_char = c;
            continue;
        }
        if c == string_char && in_string {
            in_string = false;
            continue;
        }
        if in_string {
            continue;
        }

        match c {
            '{' | '(' | '[' => count += 1,
            '}' | ')' | ']' => count -= 1,
            _ => {}
        }
    }

    count.max(0)
}

/// Count unmatched closing braces/parens on a line.
fn count_unmatched_closes(line: &str) -> i32 {
    let mut count = 0;
    let mut in_string = false;
    let mut string_char = '"';

    for c in line.chars() {
        if (c == '"' || c == '\'' || c == '`') && !in_string {
            in_string = true;
            string_char = c;
            continue;
        }
        if c == string_char && in_string {
            in_string = false;
            continue;
        }
        if in_string {
            continue;
        }

        match c {
            '}' | ')' | ']' => count += 1,
            '{' | '(' | '[' => count -= 1,
            _ => {}
        }
    }

    count.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_basic() {
        let input = "let x=1+2;";
        let formatted = format_content(input, 4, true);
        assert!(formatted.contains("let x = 1 + 2;"));
    }

    #[test]
    fn test_format_indentation() {
        let input = r#"define_group("test", || {
let x = 1;
});"#;
        let formatted = format_content(input, 4, true);
        // The inner line should be indented
        assert!(formatted.lines().nth(1).unwrap().starts_with("    "));
    }

    #[test]
    fn test_format_method_chain() {
        let input = "voice(\"kick\").synth(\"kick_808\").gain(db(-6));";
        let formatted = format_content(input, 4, true);
        // Method chains should not have spaces around dots
        assert!(formatted.contains(".synth("));
        assert!(formatted.contains(".gain("));
    }

    #[test]
    fn test_format_preserves_strings() {
        let input = r#"let s = "hello   world";"#;
        let formatted = format_content(input, 4, true);
        // String content should be preserved
        assert!(formatted.contains("\"hello   world\""));
    }

    #[test]
    fn test_count_unmatched() {
        assert_eq!(count_unmatched_opens("{ { }"), 1);
        assert_eq!(count_unmatched_opens("{ }"), 0);
        assert_eq!(count_unmatched_closes("} }"), 2);
    }
}
