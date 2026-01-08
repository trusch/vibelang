//! Inlay hints provider for VibeLang.
//!
//! Provides inline hints for parameter names, types, and timing information.

use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintLabelPart, Position,
};

use crate::data::ApiFunctionDoc;

/// Get inlay hints for a document.
pub fn get_inlay_hints(content: &str, api_docs: &[ApiFunctionDoc]) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    // Extract tempo from the document (default 120 BPM)
    let tempo = extract_tempo(content).unwrap_or(120.0);

    for (line_num, line) in content.lines().enumerate() {
        hints.extend(get_line_hints(line_num as u32, line, api_docs, tempo));
    }

    hints
}

/// Extract tempo (BPM) from set_tempo() calls in the document.
fn extract_tempo(content: &str) -> Option<f64> {
    let tempo_re = regex::Regex::new(r"set_tempo\s*\(\s*(\d+(?:\.\d+)?)\s*\)").ok()?;

    // Find the last set_tempo call (most recent)
    let mut last_tempo = None;
    for cap in tempo_re.captures_iter(content) {
        if let Some(bpm_match) = cap.get(1) {
            if let Ok(bpm) = bpm_match.as_str().parse::<f64>() {
                last_tempo = Some(bpm);
            }
        }
    }

    last_tempo
}

/// Get inlay hints for a single line.
fn get_line_hints(line_num: u32, line: &str, api_docs: &[ApiFunctionDoc], tempo: f64) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    // Parameter name hints for function calls
    hints.extend(get_parameter_hints(line_num, line, api_docs));

    // Timing hints for pattern notation (tempo-aware)
    hints.extend(get_timing_hints(line_num, line, tempo));

    // Note frequency hints
    hints.extend(get_note_hints(line_num, line));

    // Duration hints for note values (tempo-aware)
    hints.extend(get_duration_hints(line_num, line, tempo));

    hints
}

/// Get parameter name hints for function calls.
fn get_parameter_hints(line_num: u32, line: &str, api_docs: &[ApiFunctionDoc]) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    // Common VibeLang functions with their parameter names
    let param_info: Vec<(&str, Vec<&str>)> = vec![
        ("voice", vec!["name", "synthdef"]),
        ("pattern", vec!["name", "notation"]),
        ("melody", vec!["name", "notation"]),
        ("sequence", vec!["name", "patterns"]),
        ("define_group", vec!["name", "body"]),
        ("set_tempo", vec!["bpm"]),
        ("set_time_signature", vec!["numerator", "denominator"]),
        ("load_sample", vec!["name", "path"]),
        ("load_sfz", vec!["name", "path"]),
        ("define_synthdef", vec!["name", "body"]),
        ("define_fx", vec!["name", "body"]),
        ("at_bar", vec!["bar", "action"]),
        ("every", vec!["beats", "action"]),
        ("after", vec!["beats", "action"]),
        ("fade_in", vec!["duration"]),
        ("fade_out", vec!["duration"]),
        ("midi_out", vec!["device", "channel"]),
        ("midi_in", vec!["device"]),
    ];

    // Also add from API docs (parameters is (name, type, desc) tuple)
    let mut all_params: Vec<(&str, Vec<&str>)> = param_info;
    for doc in api_docs {
        let params: Vec<&str> = doc.parameters.iter().map(|(n, _t, _d)| *n).collect();
        if !params.is_empty() {
            all_params.push((doc.name, params));
        }
    }

    for (func_name, params) in all_params {
        // Find function calls
        if let Some(call_start) = line.find(&format!("{}(", func_name)) {
            let args_start = call_start + func_name.len() + 1;
            if args_start < line.len() {
                let args_str = &line[args_start..];

                // Parse arguments
                let arg_positions = parse_argument_positions(args_str);

                for (i, (arg_start, _arg_end)) in arg_positions.iter().enumerate() {
                    if i < params.len() && !params[i].is_empty() {
                        // Don't show hint for closure/block arguments
                        let arg_char = args_str.chars().nth(*arg_start).unwrap_or(' ');
                        if arg_char == '|' || arg_char == '{' {
                            continue;
                        }

                        hints.push(InlayHint {
                            position: Position {
                                line: line_num,
                                character: (args_start + arg_start) as u32,
                            },
                            label: InlayHintLabel::String(format!("{}:", params[i])),
                            kind: Some(InlayHintKind::PARAMETER),
                            text_edits: None,
                            tooltip: None,
                            padding_left: Some(false),
                            padding_right: Some(true),
                            data: None,
                        });
                    }
                }
            }
        }
    }

    hints
}

/// Parse argument positions in a function call.
fn parse_argument_positions(args_str: &str) -> Vec<(usize, usize)> {
    let mut positions = Vec::new();
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut arg_start = 0;
    let mut escape_next = false;

    // Skip leading whitespace
    let mut start_found = false;

    for (i, c) in args_str.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            _ if in_string => {}
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => {
                if start_found {
                    positions.push((arg_start, i));
                }
                break;
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if start_found {
                    positions.push((arg_start, i));
                }
                arg_start = i + 1;
                start_found = false;
            }
            c if !c.is_whitespace() && !start_found => {
                arg_start = i;
                start_found = true;
            }
            _ => {}
        }
    }

    positions
}

/// Get timing hints for pattern notation (tempo-aware).
fn get_timing_hints(line_num: u32, line: &str, tempo: f64) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    // Find pattern definitions
    let pattern_re = regex::Regex::new(r#"pattern\s*\(\s*"[^"]+"\s*,\s*"([^"]+)""#).ok();

    if let Some(re) = pattern_re {
        if let Some(caps) = re.captures(line) {
            if let Some(notation) = caps.get(1) {
                // Count steps in the pattern
                let pattern_str = notation.as_str();
                let steps = count_pattern_steps(pattern_str);

                if steps > 0 {
                    // Calculate timing based on tempo (assuming 16th note resolution)
                    let step_duration_ms = (60000.0 / tempo) / 4.0; // 16th note = quarter / 4
                    let total_duration_ms = step_duration_ms * steps as f64;
                    let bars_16th = steps as f64 / 16.0;
                    let bars_8th = steps as f64 / 8.0;

                    let end_pos = notation.end();
                    hints.push(InlayHint {
                        position: Position {
                            line: line_num,
                            character: (end_pos + 1) as u32,
                        },
                        label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                            value: format!(" {} steps ({})", steps, format_duration_ms(total_duration_ms)),
                            tooltip: Some(tower_lsp::lsp_types::MarkupContent {
                                kind: tower_lsp::lsp_types::MarkupKind::Markdown,
                                value: format!(
                                    "**Pattern timing at {} BPM:**\n\n\
                                    - **{}** steps\n\
                                    - Step duration: **{:.0}ms** (16th note)\n\
                                    - Total duration: **{}**\n\n\
                                    At 16th note resolution: {:.2} bars\n\
                                    At 8th note resolution: {:.2} bars",
                                    tempo as u32,
                                    steps,
                                    step_duration_ms,
                                    format_duration_ms(total_duration_ms),
                                    bars_16th,
                                    bars_8th
                                ),
                            }.into()),
                            location: None,
                            command: None,
                        }]),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: Some(true),
                        padding_right: Some(false),
                        data: None,
                    });
                }
            }
        }
    }

    hints
}

/// Format a duration in milliseconds to a human-readable string.
fn format_duration_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.2}s", ms / 1000.0)
    } else {
        format!("{:.0}ms", ms)
    }
}

/// Count steps in a pattern.
fn count_pattern_steps(pattern: &str) -> usize {
    pattern
        .chars()
        .filter(|c| *c != ' ' && *c != '|')
        .count()
}

/// Get note frequency hints.
fn get_note_hints(line_num: u32, line: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    // Find note names in melody definitions
    let note_re = regex::Regex::new(r#"melody\s*\(\s*"[^"]+"\s*,\s*"([^"]+)""#).ok();

    if let Some(re) = note_re {
        if let Some(caps) = re.captures(line) {
            if let Some(notes_match) = caps.get(1) {
                let notes_str = notes_match.as_str();
                let start_offset = notes_match.start();

                // Find individual notes
                let individual_note_re = regex::Regex::new(r"([a-gA-G][#b]?)(\d)").ok();
                if let Some(note_re) = individual_note_re {
                    for cap in note_re.captures_iter(notes_str) {
                        if let (Some(note_match), Some(octave_match)) = (cap.get(1), cap.get(2)) {
                            let note = note_match.as_str().to_lowercase();
                            let octave: i32 = octave_match.as_str().parse().unwrap_or(4);

                            if let Some(freq) = note_to_frequency(&note, octave) {
                                let hint_pos = start_offset + cap.get(0).unwrap().end();
                                hints.push(InlayHint {
                                    position: Position {
                                        line: line_num,
                                        character: hint_pos as u32,
                                    },
                                    label: InlayHintLabel::String(format!(" {:.1}Hz", freq)),
                                    kind: Some(InlayHintKind::TYPE),
                                    text_edits: None,
                                    tooltip: Some(tower_lsp::lsp_types::InlayHintTooltip::String(
                                        format!("Frequency: {:.2} Hz\nMIDI note: {}", freq, note_to_midi(&note, octave))
                                    )),
                                    padding_left: Some(false),
                                    padding_right: Some(true),
                                    data: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    hints
}

/// Convert note name to frequency.
fn note_to_frequency(note: &str, octave: i32) -> Option<f64> {
    let semitone = match note {
        "c" => 0,
        "c#" | "db" => 1,
        "d" => 2,
        "d#" | "eb" => 3,
        "e" => 4,
        "f" => 5,
        "f#" | "gb" => 6,
        "g" => 7,
        "g#" | "ab" => 8,
        "a" => 9,
        "a#" | "bb" => 10,
        "b" => 11,
        _ => return None,
    };

    // A4 = 440 Hz, MIDI note 69
    let midi_note = (octave + 1) * 12 + semitone;
    let freq = 440.0 * 2.0_f64.powf((midi_note as f64 - 69.0) / 12.0);
    Some(freq)
}

/// Convert note name to MIDI number.
fn note_to_midi(note: &str, octave: i32) -> i32 {
    let semitone = match note {
        "c" => 0,
        "c#" | "db" => 1,
        "d" => 2,
        "d#" | "eb" => 3,
        "e" => 4,
        "f" => 5,
        "f#" | "gb" => 6,
        "g" => 7,
        "g#" | "ab" => 8,
        "a" => 9,
        "a#" | "bb" => 10,
        "b" => 11,
        _ => 0,
    };
    (octave + 1) * 12 + semitone
}

/// Get duration hints for note values (tempo-aware).
fn get_duration_hints(line_num: u32, line: &str, tempo: f64) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    // Look for duration specifications like 1/4, 1/8, etc.
    let duration_re = regex::Regex::new(r"\b(\d+)/(\d+)\b").ok();

    if let Some(re) = duration_re {
        for cap in re.captures_iter(line) {
            if let (Some(num), Some(denom)) = (cap.get(1), cap.get(2)) {
                let numerator: u32 = num.as_str().parse().unwrap_or(1);
                let denominator: u32 = denom.as_str().parse().unwrap_or(4);

                // Only show hints for musical fractions
                if denominator.is_power_of_two() && denominator >= 2 && denominator <= 64 {
                    let duration_name = get_duration_name(numerator, denominator);

                    // Calculate duration in milliseconds
                    // Quarter note = 60000 / tempo ms
                    // Duration = quarter * (4 / denominator) * numerator
                    let quarter_ms = 60000.0 / tempo;
                    let duration_ms = quarter_ms * (4.0 / denominator as f64) * numerator as f64;

                    if let Some(name) = duration_name {
                        hints.push(InlayHint {
                            position: Position {
                                line: line_num,
                                character: cap.get(0).unwrap().end() as u32,
                            },
                            label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                                value: format!(" ({}, {})", name, format_duration_ms(duration_ms)),
                                tooltip: Some(tower_lsp::lsp_types::MarkupContent {
                                    kind: tower_lsp::lsp_types::MarkupKind::Markdown,
                                    value: format!(
                                        "**{}/{}** = **{}** at {} BPM\n\n\
                                        Duration: **{}**",
                                        numerator, denominator, name, tempo as u32,
                                        format_duration_ms(duration_ms)
                                    ),
                                }.into()),
                                location: None,
                                command: None,
                            }]),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: None,
                            padding_left: Some(false),
                            padding_right: Some(true),
                            data: None,
                        });
                    } else {
                        // Show just the timing for non-standard fractions
                        hints.push(InlayHint {
                            position: Position {
                                line: line_num,
                                character: cap.get(0).unwrap().end() as u32,
                            },
                            label: InlayHintLabel::String(format!(" ({})", format_duration_ms(duration_ms))),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: Some(tower_lsp::lsp_types::InlayHintTooltip::String(
                                format!("{}/{} = {} at {} BPM", numerator, denominator, format_duration_ms(duration_ms), tempo as u32)
                            )),
                            padding_left: Some(false),
                            padding_right: Some(true),
                            data: None,
                        });
                    }
                }
            }
        }
    }

    hints
}

/// Get the musical name for a duration.
fn get_duration_name(numerator: u32, denominator: u32) -> Option<String> {
    if numerator == 1 {
        match denominator {
            1 => Some("whole".to_string()),
            2 => Some("half".to_string()),
            4 => Some("quarter".to_string()),
            8 => Some("eighth".to_string()),
            16 => Some("sixteenth".to_string()),
            32 => Some("32nd".to_string()),
            64 => Some("64th".to_string()),
            _ => None,
        }
    } else if numerator == 3 {
        match denominator {
            4 => Some("dotted half".to_string()),
            8 => Some("dotted quarter".to_string()),
            16 => Some("dotted eighth".to_string()),
            32 => Some("dotted sixteenth".to_string()),
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_to_frequency() {
        // A4 = 440 Hz
        let freq = note_to_frequency("a", 4).unwrap();
        assert!((freq - 440.0).abs() < 0.01);

        // C4 = 261.63 Hz (middle C)
        let freq = note_to_frequency("c", 4).unwrap();
        assert!((freq - 261.63).abs() < 0.1);
    }

    #[test]
    fn test_note_to_midi() {
        assert_eq!(note_to_midi("c", 4), 60); // Middle C
        assert_eq!(note_to_midi("a", 4), 69); // A4
        assert_eq!(note_to_midi("c", 5), 72); // C5
    }

    #[test]
    fn test_count_pattern_steps() {
        assert_eq!(count_pattern_steps("x-x-"), 4);
        assert_eq!(count_pattern_steps("x-x- x-x-"), 8);
        assert_eq!(count_pattern_steps("xxxx|----"), 8);
    }

    #[test]
    fn test_parse_argument_positions() {
        let args = r#""name", synthdef"#;
        let positions = parse_argument_positions(args);
        assert_eq!(positions.len(), 2);
    }

    #[test]
    fn test_duration_names() {
        assert_eq!(get_duration_name(1, 4), Some("quarter".to_string()));
        assert_eq!(get_duration_name(1, 8), Some("eighth".to_string()));
        assert_eq!(get_duration_name(3, 8), Some("dotted quarter".to_string()));
    }
}
