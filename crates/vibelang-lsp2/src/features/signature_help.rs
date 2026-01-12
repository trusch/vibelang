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
    // Find the function name and parameter index
    let (func_name, param_index) = find_function_context(line, character as usize)?;

    // First, try API docs
    if let Some(func_doc) = api_docs.iter().find(|f| f.name == func_name) {
        // Build parameter information (parameters is (name, type, description) tuple)
        let parameters: Vec<ParameterInformation> = func_doc
            .parameters
            .iter()
            .map(|(name, _param_type, desc)| ParameterInformation {
                label: ParameterLabel::Simple(name.to_string()),
                documentation: Some(tower_lsp::lsp_types::Documentation::String(
                    desc.to_string(),
                )),
            })
            .collect();

        // Build signature label
        let param_labels: Vec<String> = func_doc
            .parameters
            .iter()
            .map(|(name, _param_type, _desc)| name.to_string())
            .collect();
        let label = format!("{}({})", func_name, param_labels.join(", "));

        let signature = SignatureInformation {
            label,
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                func_doc.description.to_string(),
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

    // Then, try UGen functions (e.g., sin_osc_ar, lpf_ar, etc.)
    if let Some(ugen_help) = get_ugen_signature_help(&func_name, param_index) {
        return Some(ugen_help);
    }

    None
}

/// Find the function being called and which parameter the cursor is in.
fn find_function_context(line: &str, cursor: usize) -> Option<(String, usize)> {
    let before_cursor = &line[..cursor.min(line.len())];

    // Find the most recent unclosed parenthesis
    let mut paren_depth = 0;
    let mut func_end = None;

    for (i, c) in before_cursor.char_indices().rev() {
        match c {
            ')' => paren_depth += 1,
            '(' => {
                if paren_depth == 0 {
                    func_end = Some(i);
                    break;
                }
                paren_depth -= 1;
            }
            _ => {}
        }
    }

    let func_end = func_end?;

    // Extract function name (word before the parenthesis)
    let before_paren = &before_cursor[..func_end];
    let func_start = before_paren
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let func_name = before_paren[func_start..].to_string();

    if func_name.is_empty() {
        return None;
    }

    // Count commas to determine parameter index
    let inside_parens = &before_cursor[func_end + 1..];
    let param_index = count_parameters(inside_parens);

    Some((func_name, param_index))
}

/// Count how many parameters we've passed (by counting commas at depth 0).
fn count_parameters(text: &str) -> usize {
    let mut count = 0;
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for c in text.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            _ if in_string => {}
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                count += 1;
            }
            _ => {}
        }
    }

    count
}

/// Get signatures for common VibeLang functions.
pub fn get_builtin_signatures() -> Vec<(String, SignatureInformation)> {
    let signatures = vec![
        // Voice creation
        (
            "voice",
            "voice(name, synthdef)",
            "Create a new voice with the given name and synthdef",
            vec![
                ("name", "The name of the voice"),
                ("synthdef", "The synthdef to use"),
            ],
        ),
        // Pattern creation
        (
            "pattern",
            "pattern(name, notation)",
            "Create a pattern from notation string",
            vec![
                ("name", "The name of the pattern"),
                ("notation", "Pattern notation (e.g., \"x-x- x-x-\")"),
            ],
        ),
        // Melody creation
        (
            "melody",
            "melody(name, notation)",
            "Create a melody from notation string",
            vec![
                ("name", "The name of the melody"),
                ("notation", "Melody notation (e.g., \"c4 e4 g4\")"),
            ],
        ),
        // Group creation
        (
            "define_group",
            "define_group(name, closure)",
            "Define a group of voices and patterns",
            vec![
                ("name", "The group name"),
                ("closure", "A closure defining group contents"),
            ],
        ),
        // Transport
        (
            "set_tempo",
            "set_tempo(bpm)",
            "Set the tempo in beats per minute",
            vec![("bpm", "Beats per minute (e.g., 120.0)")],
        ),
        (
            "set_time_signature",
            "set_time_signature(numerator, denominator)",
            "Set the time signature",
            vec![
                ("numerator", "Beats per bar"),
                ("denominator", "Note value for one beat"),
            ],
        ),
        // Sample loading
        (
            "load_sample",
            "load_sample(name, path)",
            "Load an audio sample from file",
            vec![
                ("name", "Sample name for reference"),
                ("path", "Path to the audio file"),
            ],
        ),
        // SFZ loading
        (
            "load_sfz",
            "load_sfz(name, path)",
            "Load an SFZ instrument",
            vec![
                ("name", "Instrument name"),
                ("path", "Path to the .sfz file"),
            ],
        ),
        // Synthdef building
        (
            "define_synthdef",
            "define_synthdef(name, closure)",
            "Define a custom synthesizer",
            vec![
                ("name", "Synthdef name"),
                ("closure", "A closure defining the synth graph"),
            ],
        ),
        // Effect building
        (
            "define_fx",
            "define_fx(name, closure)",
            "Define a custom effect",
            vec![
                ("name", "Effect name"),
                ("closure", "A closure defining the effect graph"),
            ],
        ),
        // Sequence
        (
            "sequence",
            "sequence(name, patterns)",
            "Create a sequence from an array of patterns",
            vec![
                ("name", "Sequence name"),
                ("patterns", "Array of pattern references"),
            ],
        ),
        // Arrangement
        (
            "at_bar",
            "at_bar(bar, closure)",
            "Schedule actions to run at a specific bar",
            vec![
                ("bar", "Bar number (1-based)"),
                ("closure", "Actions to perform"),
            ],
        ),
        // MIDI
        (
            "midi_out",
            "midi_out(device, channel)",
            "Create a MIDI output voice",
            vec![
                ("device", "MIDI device name or index"),
                ("channel", "MIDI channel (0-15)"),
            ],
        ),
        (
            "midi_in",
            "midi_in(device)",
            "Enable MIDI input from a device",
            vec![("device", "MIDI device name or index")],
        ),
    ];

    signatures
        .into_iter()
        .map(|(name, label, doc, params)| {
            let parameters: Vec<ParameterInformation> = params
                .into_iter()
                .map(|(pname, pdoc)| ParameterInformation {
                    label: ParameterLabel::Simple(pname.to_string()),
                    documentation: Some(tower_lsp::lsp_types::Documentation::String(
                        pdoc.to_string(),
                    )),
                })
                .collect();

            (
                name.to_string(),
                SignatureInformation {
                    label: label.to_string(),
                    documentation: Some(tower_lsp::lsp_types::Documentation::String(
                        doc.to_string(),
                    )),
                    parameters: Some(parameters),
                    active_parameter: None,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_function_context() {
        let line = r#"voice("kick", kick_synthdef)"#;
        let result = find_function_context(line, 8);
        assert_eq!(result, Some(("voice".to_string(), 0)));

        let result = find_function_context(line, 20);
        assert_eq!(result, Some(("voice".to_string(), 1)));
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
        // At position after "kick",
        let result = find_function_context(line, 14);
        assert_eq!(result, Some(("voice".to_string(), 1)));
    }
}
