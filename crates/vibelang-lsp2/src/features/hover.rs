//! Hover provider for VibeLang.
//!
//! Provides documentation on hover for:
//! - API functions
//! - UGen functions
//! - Synthdef names
//! - Effect names
//! - Note names

use std::collections::HashMap;

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::data::{format_ugen_hover, get_api_function_doc, SynthdefInfo};

/// Get hover information for a word.
pub fn get_hover(
    word: &str,
    content: &str,
    position: Position,
    synthdef_info: &HashMap<String, SynthdefInfo>,
    effect_info: &HashMap<String, SynthdefInfo>,
) -> Option<Hover> {
    // Check if it's an API function
    if let Some(doc) = get_api_function_doc(word) {
        let params_doc = if !doc.parameters.is_empty() {
            let params: Vec<String> = doc
                .parameters
                .iter()
                .map(|(name, typ, desc)| format!("- `{}` ({}) - {}", name, typ, desc))
                .collect();
            format!("\n\n**Parameters:**\n{}", params.join("\n"))
        } else {
            String::new()
        };

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "### `{}`\n\n{}\n\n**Signature:** `{}`{}\n\n**Example:**\n```rhai\n{}\n```",
                    doc.name, doc.description, doc.signature, params_doc, doc.example
                ),
            }),
            range: None,
        });
    }

    // Check if it's a UGen function
    if let Some(content) = format_ugen_hover(word) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: None,
        });
    }

    // Check if it's a synthdef
    if let Some(info) = synthdef_info.get(word) {
        return Some(Hover {
            contents: HoverContents::Markup(format_synthdef_hover(info)),
            range: None,
        });
    }

    // Check if it's an effect
    if let Some(info) = effect_info.get(word) {
        return Some(Hover {
            contents: HoverContents::Markup(format_effect_hover(info)),
            range: None,
        });
    }

    // Check if it's a note name
    if let Some(content) = get_note_hover(word) {
        return Some(Hover {
            contents: HoverContents::Markup(content),
            range: None,
        });
    }

    // Check for context-specific hover (e.g., inside string)
    if let Some(context_hover) = get_context_hover(content, position) {
        return Some(Hover {
            contents: HoverContents::Markup(context_hover),
            range: None,
        });
    }

    None
}

/// Format synthdef hover information.
fn format_synthdef_hover(info: &SynthdefInfo) -> MarkupContent {
    let mut content = format!("### Synthdef: `{}`\n\n", info.name);

    if let Some(ref desc) = info.description {
        content.push_str(desc);
        content.push_str("\n\n");
    }

    if let Some(ref category) = info.category {
        content.push_str(&format!("**Category:** {}\n\n", category));
    }

    if !info.parameters.is_empty() {
        content.push_str("**Parameters:**\n\n");
        for param in &info.parameters {
            content.push_str(&format!("- `{}` (default: {})", param.name, param.default));
            if let Some(ref desc) = param.description {
                content.push_str(&format!(" - {}", desc));
            }
            content.push('\n');
        }
    }

    MarkupContent {
        kind: MarkupKind::Markdown,
        value: content,
    }
}

/// Format effect hover information.
fn format_effect_hover(info: &SynthdefInfo) -> MarkupContent {
    let mut content = format!("### Effect: `{}`\n\n", info.name);

    if let Some(ref desc) = info.description {
        content.push_str(desc);
        content.push_str("\n\n");
    }

    if let Some(ref category) = info.category {
        content.push_str(&format!("**Category:** {}\n\n", category));
    }

    if !info.parameters.is_empty() {
        content.push_str("**Parameters:**\n\n");
        for param in &info.parameters {
            content.push_str(&format!("- `{}` (default: {})", param.name, param.default));
            if let Some(ref desc) = param.description {
                content.push_str(&format!(" - {}", desc));
            }
            content.push('\n');
        }
    }

    MarkupContent {
        kind: MarkupKind::Markdown,
        value: content,
    }
}

/// Get hover for note names (C4, F#3, etc.).
fn get_note_hover(note: &str) -> Option<MarkupContent> {
    // Parse note name
    let note_upper = note.to_uppercase();
    let chars: Vec<char> = note_upper.chars().collect();

    if chars.is_empty() {
        return None;
    }

    // Must start with A-G
    let base_note = chars[0];
    if !('A'..='G').contains(&base_note) {
        return None;
    }

    let mut idx = 1;
    let mut accidental = 0i32;

    // Check for accidental
    if idx < chars.len() {
        match chars[idx] {
            '#' | '♯' => {
                accidental = 1;
                idx += 1;
            }
            'B' if idx + 1 < chars.len() && !chars[idx + 1].is_ascii_digit() => {
                accidental = -1;
                idx += 1;
            }
            _ => {}
        }
    }

    // Parse octave
    if idx >= chars.len() {
        return None;
    }

    let octave_str: String = chars[idx..].iter().collect();
    let octave: i32 = octave_str.parse().ok()?;

    // Calculate MIDI note number
    let base_semitone = match base_note {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    let midi_note = (octave + 1) * 12 + base_semitone + accidental;
    let frequency = 440.0 * 2.0_f64.powf((midi_note as f64 - 69.0) / 12.0);

    Some(MarkupContent {
        kind: MarkupKind::Markdown,
        value: format!(
            "### Note: `{}`\n\n- **MIDI Note:** {}\n- **Frequency:** {:.2} Hz\n- **Octave:** {}",
            note, midi_note, frequency, octave
        ),
    })
}

/// Get context-specific hover (e.g., for values inside strings).
fn get_context_hover(content: &str, position: Position) -> Option<MarkupContent> {
    let line_content = content.lines().nth(position.line as usize)?;
    let char_idx = position.character as usize;

    // Check if we're inside a db() call
    if let Some(db_start) = line_content[..char_idx.min(line_content.len())].rfind("db(") {
        let after_db = &line_content[db_start + 3..];
        if let Some(paren_end) = after_db.find(')') {
            if char_idx <= db_start + 3 + paren_end {
                // Extract the value
                let value_str = &after_db[..paren_end];
                if let Ok(db_value) = value_str.trim().parse::<f64>() {
                    let linear = 10.0_f64.powf(db_value / 20.0);
                    return Some(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!(
                            "### dB Conversion\n\n- **dB:** {:.1} dB\n- **Linear:** {:.4}\n- **Percentage:** {:.1}%",
                            db_value, linear, linear * 100.0
                        ),
                    });
                }
            }
        }
    }

    // Check if we're inside a bars() call
    if let Some(bars_start) = line_content[..char_idx.min(line_content.len())].rfind("bars(") {
        let after_bars = &line_content[bars_start + 5..];
        if let Some(paren_end) = after_bars.find(')') {
            if char_idx <= bars_start + 5 + paren_end {
                let value_str = &after_bars[..paren_end];
                if let Ok(bars_value) = value_str.trim().parse::<f64>() {
                    let beats = bars_value * 4.0; // Assuming 4/4
                    return Some(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!(
                            "### Bar Conversion\n\n- **Bars:** {}\n- **Beats (4/4):** {}\n- **Sixteenths:** {}",
                            bars_value, beats, beats * 4.0
                        ),
                    });
                }
            }
        }
    }

    None
}

/// Get hover for chord types.
pub fn get_chord_hover(chord_type: &str) -> Option<MarkupContent> {
    let chord_info = match chord_type {
        "maj" | "major" => ("Major", "1 - 3 - 5", "C E G"),
        "min" | "minor" | "m" => ("Minor", "1 - b3 - 5", "C Eb G"),
        "7" | "dom7" => ("Dominant 7th", "1 - 3 - 5 - b7", "C E G Bb"),
        "maj7" | "major7" => ("Major 7th", "1 - 3 - 5 - 7", "C E G B"),
        "m7" | "min7" | "minor7" => ("Minor 7th", "1 - b3 - 5 - b7", "C Eb G Bb"),
        "dim" => ("Diminished", "1 - b3 - b5", "C Eb Gb"),
        "dim7" => ("Diminished 7th", "1 - b3 - b5 - bb7", "C Eb Gb Bbb"),
        "aug" => ("Augmented", "1 - 3 - #5", "C E G#"),
        "sus2" => ("Suspended 2nd", "1 - 2 - 5", "C D G"),
        "sus4" => ("Suspended 4th", "1 - 4 - 5", "C F G"),
        "add9" => ("Add 9", "1 - 3 - 5 - 9", "C E G D"),
        "9" => ("Dominant 9th", "1 - 3 - 5 - b7 - 9", "C E G Bb D"),
        "m9" | "min9" => ("Minor 9th", "1 - b3 - 5 - b7 - 9", "C Eb G Bb D"),
        "maj9" => ("Major 9th", "1 - 3 - 5 - 7 - 9", "C E G B D"),
        "6" => ("Major 6th", "1 - 3 - 5 - 6", "C E G A"),
        "m6" | "min6" => ("Minor 6th", "1 - b3 - 5 - 6", "C Eb G A"),
        "5" | "power" => ("Power Chord", "1 - 5", "C G"),
        _ => return None,
    };

    Some(MarkupContent {
        kind: MarkupKind::Markdown,
        value: format!(
            "### {} Chord\n\n- **Formula:** {}\n- **Example (C):** {}",
            chord_info.0, chord_info.1, chord_info.2
        ),
    })
}
