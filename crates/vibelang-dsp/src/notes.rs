//! Canonical note-name to MIDI-number conversion.
//!
//! This is the single implementation shared across the workspace
//! (vibelang-core re-exports it as `vibelang_core::midi::parse_note_name`;
//! vibelang-rhai, vibelang-http, vibelang-lsp and vibelang-wasm use it
//! directly). Do not add per-crate copies — extend this module instead.
//!
//! Accepted grammar: `letter [accidentals] [octave]`
//! - Letter: `C D E F G A B`, case-insensitive.
//! - Accidentals: any number of `#`/`♯` (sharp, +1 semitone) and
//!   `b`/`♭` (flat, -1 semitone). Only lowercase `b` is a flat; an
//!   uppercase `B` after the letter is treated as octave text.
//! - Octave: optional signed integer, parsed as `i8`. When absent or
//!   unparseable it defaults to octave 4 (so `"C"` == `"C4"` == 60).
//! - Middle-C convention: C4 = 60, C-1 = 0, A4 = 69.

use std::ops::Range;

/// A stable, source-located error from the strict note parser.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error(
    "{code}: note parse error at bytes {start}..{end}: expected {expected}; offending token {token:?}",
    start = .span.start,
    end = .span.end
)]
pub struct NoteParseError {
    pub code: &'static str,
    pub span: Range<usize>,
    pub expected: &'static str,
    pub token: String,
}

impl NoteParseError {
    fn new(
        code: &'static str,
        span: Range<usize>,
        expected: &'static str,
        token: impl Into<String>,
    ) -> Self {
        Self {
            code,
            span,
            expected,
            token: token.into(),
        }
    }
}

fn trimmed_input(name: &str) -> (&str, usize) {
    let trimmed_start = name.trim_start();
    let start = name.len() - trimmed_start.len();
    (trimmed_start.trim_end(), start)
}

/// Strictly parse a note name to its raw, unclamped MIDI value.
///
/// The parser consumes the complete trimmed input. A missing octave keeps the
/// documented octave-4 default.
pub fn parse_note_name_raw_strict(name: &str) -> Result<i32, NoteParseError> {
    let (name, offset) = trimmed_input(name);
    if name.is_empty() {
        return Err(NoteParseError::new(
            "dsp.note.empty",
            offset..offset,
            "note letter A-G",
            "",
        ));
    }

    let mut chars = name.char_indices().peekable();
    let Some((_, letter)) = chars.next() else {
        return Err(NoteParseError::new(
            "dsp.note.empty",
            offset..offset,
            "note letter A-G",
            "",
        ));
    };
    let base: i32 = match letter.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => {
            return Err(NoteParseError::new(
                "dsp.note.letter",
                offset..offset + letter.len_utf8(),
                "note letter A-G",
                letter.to_string(),
            ))
        }
    };

    let mut accidental: i32 = 0;
    let mut octave_start = letter.len_utf8();
    while let Some(&(index, accidental_char)) = chars.peek() {
        match accidental_char {
            '#' | '♯' => {
                accidental += 1;
                octave_start = index + accidental_char.len_utf8();
                chars.next();
            }
            'b' | '♭' => {
                accidental -= 1;
                octave_start = index + accidental_char.len_utf8();
                chars.next();
            }
            _ => {
                octave_start = index;
                break;
            }
        }
    }

    let octave_text = &name[octave_start..];
    let octave = if octave_text.is_empty() {
        4
    } else {
        let digits = match octave_text
            .strip_prefix('-')
            .or_else(|| octave_text.strip_prefix('+'))
        {
            Some(digits) => digits,
            None => octave_text,
        };
        if digits.is_empty() {
            return Err(NoteParseError::new(
                "dsp.note.octave",
                offset + octave_start..offset + name.len(),
                "signed octave digits",
                octave_text,
            ));
        }
        if let Some((invalid_index, invalid)) = digits
            .char_indices()
            .find(|(_, character)| !character.is_ascii_digit())
        {
            let sign_len = octave_text.len() - digits.len();
            let start = offset + octave_start + sign_len + invalid_index;
            return Err(NoteParseError::new(
                "dsp.note.trailing",
                start..start + invalid.len_utf8(),
                "end of note after signed octave digits",
                invalid.to_string(),
            ));
        }
        octave_text.parse::<i8>().map_err(|_| {
            NoteParseError::new(
                "dsp.note.octave_range",
                offset + octave_start..offset + name.len(),
                "signed octave in i8 range",
                octave_text,
            )
        })?
    };

    Ok((i32::from(octave) + 1) * 12 + base + accidental)
}

/// Strictly parse a note name into the MIDI range `0..=127`.
pub fn parse_note_name_strict(name: &str) -> Result<u8, NoteParseError> {
    let value = parse_note_name_raw_strict(name)?;
    if !(0..=127).contains(&value) {
        let (trimmed, offset) = trimmed_input(name);
        return Err(NoteParseError::new(
            "dsp.note.range",
            offset..offset + trimmed.len(),
            "MIDI note in 0..=127",
            trimmed,
        ));
    }
    Ok(value as u8)
}

/// Parse a note name to its raw, unclamped MIDI value.
///
/// Returns the computed value without any range check (e.g. `"B9"` ->
/// `Some(131)`), or `None` for an empty string or invalid note letter.
/// Most callers want [`parse_note_name`]; this variant exists for callers
/// that clamp (HTTP API) or display out-of-range values (LSP hints).
pub fn parse_note_name_raw(name: &str) -> Option<i32> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let mut chars = name.chars().peekable();

    // Note letter (C, D, E, F, G, A, B), case-insensitive.
    let base: i32 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    // Accidentals (#/♯ sharp, b/♭ flat, stackable).
    let mut accidental: i32 = 0;
    while let Some(&c) = chars.peek() {
        match c {
            '#' | '♯' => {
                accidental += 1;
                chars.next();
            }
            'b' | '♭' => {
                accidental -= 1;
                chars.next();
            }
            _ => break,
        }
    }

    // Octave: parsed as i8, defaulting to 4 when absent or unparseable
    // (historical behaviour of the vibelang-core implementation).
    let octave_str: String = chars.collect();
    let octave: i8 = octave_str.parse().unwrap_or(4);

    Some((octave as i32 + 1) * 12 + base + accidental)
}

/// Parse a note name like `"C4"`, `"F#3"` or `"Bb4"` to a MIDI note number.
///
/// Returns `None` for invalid input or results outside `0..=127`.
pub fn parse_note_name(name: &str) -> Option<u8> {
    parse_note_name_raw(name)
        .filter(|v| (0..=127).contains(v))
        .map(|v| v as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_notes() {
        assert_eq!(parse_note_name("C4"), Some(60));
        assert_eq!(parse_note_name("D4"), Some(62));
        assert_eq!(parse_note_name("E4"), Some(64));
        assert_eq!(parse_note_name("F4"), Some(65));
        assert_eq!(parse_note_name("G4"), Some(67));
        assert_eq!(parse_note_name("A4"), Some(69));
        assert_eq!(parse_note_name("B4"), Some(71));
    }

    #[test]
    fn accidentals() {
        assert_eq!(parse_note_name("C#4"), Some(61));
        assert_eq!(parse_note_name("F#4"), Some(66));
        assert_eq!(parse_note_name("Bb4"), Some(70));
        assert_eq!(parse_note_name("Db4"), Some(61));
        assert_eq!(parse_note_name("Eb4"), Some(63));
        // Stacked accidentals
        assert_eq!(parse_note_name("C##4"), Some(62));
        assert_eq!(parse_note_name("Cbb4"), Some(58));
        // Unicode symbols
        assert_eq!(parse_note_name("C\u{266f}4"), Some(61)); // C♯4
        assert_eq!(parse_note_name("D\u{266d}4"), Some(61)); // D♭4
                                                             // Accidentals crossing octave boundaries
        assert_eq!(parse_note_name("Cb4"), Some(59)); // == B3
        assert_eq!(parse_note_name("B#4"), Some(72)); // == C5
    }

    #[test]
    fn octaves_and_ranges() {
        assert_eq!(parse_note_name("C0"), Some(12));
        assert_eq!(parse_note_name("C-1"), Some(0));
        assert_eq!(parse_note_name("G9"), Some(127));
        assert_eq!(parse_note_name("G#9"), None); // 128, out of range
        assert_eq!(parse_note_name("C-2"), None); // -12, out of range
    }

    #[test]
    fn case_insensitive_letter_and_default_octave() {
        assert_eq!(parse_note_name("c4"), Some(60));
        assert_eq!(parse_note_name("bb3"), Some(58));
        assert_eq!(parse_note_name("C"), Some(60)); // defaults to octave 4
        assert_eq!(parse_note_name("a"), Some(69));
        assert_eq!(parse_note_name(" C4 "), Some(60)); // trimmed
    }

    #[test]
    fn invalid_input() {
        assert_eq!(parse_note_name(""), None);
        assert_eq!(parse_note_name("   "), None);
        assert_eq!(parse_note_name("H4"), None);
        assert_eq!(parse_note_name("4"), None);
        assert_eq!(parse_note_name("#4"), None);
    }

    #[test]
    fn garbage_octave_defaults_to_4() {
        // Historical leniency: unparseable octave text falls back to 4.
        assert_eq!(parse_note_name("C4x"), Some(60));
        assert_eq!(parse_note_name("Cx"), Some(60));
        // Octave overflowing i8 also falls back to 4.
        assert_eq!(parse_note_name("C300"), Some(60));
        // Uppercase 'B' is not a flat; it makes the octave unparseable.
        assert_eq!(parse_note_name("CB4"), Some(60));
    }

    #[test]
    fn raw_is_unclamped() {
        assert_eq!(parse_note_name_raw("B9"), Some(131));
        assert_eq!(parse_note_name_raw("G#9"), Some(128));
        assert_eq!(parse_note_name_raw("C-2"), Some(-12));
        assert_eq!(parse_note_name_raw("C4"), Some(60));
        assert_eq!(parse_note_name_raw(""), None);
        assert_eq!(parse_note_name_raw("H4"), None);
    }

    #[test]
    fn strict_parser_consumes_full_input_and_reports_spans() {
        assert_eq!(parse_note_name_strict(" C4 "), Ok(60));
        assert_eq!(parse_note_name_strict("C"), Ok(60));
        assert_eq!(parse_note_name_strict("C##4"), Ok(62));

        let trailing = parse_note_name_strict("C4x").unwrap_err();
        assert_eq!(trailing.code, "dsp.note.trailing");
        assert_eq!(trailing.span, 2..3);
        assert_eq!(trailing.token, "x");

        let missing_octave = parse_note_name_strict("Cx").unwrap_err();
        assert_eq!(missing_octave.code, "dsp.note.trailing");
        assert_eq!(missing_octave.span, 1..2);
        assert_eq!(
            missing_octave.expected,
            "end of note after signed octave digits"
        );
        assert_eq!(missing_octave.token, "x");

        let overflow = parse_note_name_strict("C300").unwrap_err();
        assert_eq!(overflow.code, "dsp.note.octave_range");
        assert_eq!(overflow.span, 1..4);

        let uppercase_flat = parse_note_name_strict("CB4").unwrap_err();
        assert_eq!(uppercase_flat.code, "dsp.note.trailing");
        assert_eq!(uppercase_flat.span, 1..2);
        assert_eq!(uppercase_flat.token, "B");

        let range = parse_note_name_strict("G#9").unwrap_err();
        assert_eq!(range.code, "dsp.note.range");
        assert_eq!(range.span, 0..3);
    }
}
