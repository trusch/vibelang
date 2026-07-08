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
}
