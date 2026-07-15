//! Signature help provider for VibeLang.
//!
//! Provides parameter hints when calling functions (API and UGen).

use tower_lsp::lsp_types::{
    ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation,
};

use crate::data::{get_ugen_signature_help, ApiFunctionDoc};

/// Get signature help for a function call at the cursor position.
pub fn get_signature_help(
    line: &str,
    character: u32,
    api_docs: &[ApiFunctionDoc],
) -> Option<SignatureHelp> {
    let (func_name, param_index) = find_function_context(line, character as usize)?;

    if let Some(func_doc) = api_docs.iter().find(|function| function.name == func_name) {
        let parameters = func_doc
            .parameters
            .iter()
            .map(|(name, _param_type, description)| ParameterInformation {
                label: ParameterLabel::Simple(name.clone()),
                documentation: Some(tower_lsp::lsp_types::Documentation::String(
                    description.clone(),
                )),
            })
            .collect();
        let signature = SignatureInformation {
            label: func_doc.signature.clone(),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                func_doc.description.clone(),
            )),
            parameters: Some(parameters),
            active_parameter: Some(param_index as u32),
        };

        return Some(SignatureHelp {
            signatures: vec![signature],
            active_signature: Some(0),
            active_parameter: Some(param_index as u32),
        });
    }

    get_ugen_signature_help(&func_name, param_index)
}

/// Find the function being called and which parameter the cursor is in.
fn find_function_context(line: &str, cursor: usize) -> Option<(String, usize)> {
    let before_cursor = &line[..cursor.min(line.len())];
    let mut paren_depth = 0;
    let mut func_end = None;

    for (index, character) in before_cursor.char_indices().rev() {
        match character {
            ')' => paren_depth += 1,
            '(' => {
                if paren_depth == 0 {
                    func_end = Some(index);
                    break;
                }
                paren_depth -= 1;
            }
            _ => {}
        }
    }

    let func_end = func_end?;
    let before_paren = &before_cursor[..func_end];
    let func_start = before_paren
        .rfind(|character: char| !character.is_alphanumeric() && character != '_')
        .map(|index| index + 1)
        .unwrap_or(0);
    let func_name = before_paren[func_start..].to_string();
    if func_name.is_empty() {
        return None;
    }

    let inside_parens = &before_cursor[func_end + 1..];
    Some((func_name, count_parameters(inside_parens)))
}

/// Count how many parameters we've passed (by counting commas at depth 0).
fn count_parameters(text: &str) -> usize {
    let mut count = 0;
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for character in text.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match character {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            _ if in_string => {}
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => count += 1,
            _ => {}
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_function_context() {
        let line = r#"voice("kick", kick_synthdef)"#;
        assert_eq!(
            find_function_context(line, 8),
            Some(("voice".to_string(), 0))
        );
        assert_eq!(
            find_function_context(line, 20),
            Some(("voice".to_string(), 1))
        );
    }

    #[test]
    fn test_count_parameters() {
        assert_eq!(count_parameters(""), 0);
        assert_eq!(count_parameters("a"), 0);
        assert_eq!(count_parameters("a, b"), 1);
        assert_eq!(count_parameters("a, b, c"), 2);
        assert_eq!(count_parameters(r#""a, b", c"#), 1);
    }

    #[test]
    fn test_nested_parens() {
        let line = r#"voice("kick", fn(x, y))"#;
        assert_eq!(
            find_function_context(line, 14),
            Some(("voice".to_string(), 1))
        );
    }
}
