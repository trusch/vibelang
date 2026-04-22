//! Helper functions for Rhai scripts.
//!
//! Utility functions for common operations like dB conversion, note parsing, etc.

use rhai::{Array, Dynamic, Engine, EvalAltResult, Position};
use std::sync::atomic::{AtomicU64, Ordering};
use vibelang_core::midi::parse_note_name;

use crate::context;

// Simple RNG state (xorshift64)
static RNG_STATE: AtomicU64 = AtomicU64::new(0);

/// Register helper functions with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // dB conversion
    engine.register_fn("db", db);
    engine.register_fn("db", db_int);

    // Note parsing
    engine.register_fn("note", note);

    // Chord parsing
    engine.register_fn("chord", chord);
    engine.register_fn("chord", chord_with_octave);

    // Scale helpers
    engine.register_fn("scale", scale);
    engine.register_fn("scale", scale_with_octave);
    engine.register_fn("scale_degree", scale_degree);

    // Time helpers
    engine.register_fn("bars", bars);
    engine.register_fn("bars", bars_int);

    // Range operators for mixed types (needed for patterns like `0..bars(8)`)
    engine.register_fn("..", make_range_if);
    engine.register_fn("..", make_range_ff);
    engine.register_fn("..", make_range_fi);

    // Array utilities
    engine.register_fn("zip", array_zip);
    engine.register_fn("shuffle", array_shuffle);
    engine.register_fn("rotate", array_rotate);
    engine.register_fn("reverse", array_reverse);
    engine.register_fn("flatten", array_flatten);
    engine.register_fn("repeat", array_repeat);
    engine.register_fn("take", array_take);
    engine.register_fn("skip", array_skip);

    // Random functions
    engine.register_fn("random", random);
    engine.register_fn("random_range", random_range_ff);
    engine.register_fn("random_range", random_range_ii);
    engine.register_fn("random_int", random_int);
    engine.register_fn("random_choice", random_choice);
    engine.register_fn("random_seed", random_seed);

    // Math utilities
    engine.register_fn("clamp", clamp_fff);
    engine.register_fn("clamp", clamp_iii);
    engine.register_fn("lerp", lerp);
    engine.register_fn("map_range", map_range);
    engine.register_fn("smoothstep", smoothstep);
    engine.register_fn("wrap", wrap);
    engine.register_fn("quantize", quantize);
    engine.register_fn("mtof", midi_to_freq);
    engine.register_fn("ftom", freq_to_midi);

    // Time utilities (native only)
    #[cfg(not(target_arch = "wasm32"))]
    {
        engine.register_fn("timestamp", timestamp);
        engine.register_fn("timestamp_ms", timestamp_ms);
    }

    // Type conversion utilities
    engine.register_fn("to_int", to_int);
    engine.register_fn("to_float", to_float_from_int);
    engine.register_fn("to_string", to_string_int);
    engine.register_fn("to_string", to_string_float);
}

/// Convert decibels to linear amplitude.
pub fn db(decibels: f64) -> f64 {
    10.0_f64.powf(decibels / 20.0)
}

/// Convert decibels to linear amplitude (integer overload).
pub fn db_int(decibels: i64) -> f64 {
    db(decibels as f64)
}

/// Parse a note name to MIDI note number.
pub fn note(name: &str) -> Result<i64, Box<EvalAltResult>> {
    parse_note_name(name).map(|n| n as i64).ok_or_else(|| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!(
                "Note parse error: invalid note name '{}' \
                 — expected format like C4, D#3, Bb5",
                name
            )
            .into(),
            Position::NONE,
        ))
    })
}

/// Parse a chord name to an array of MIDI note numbers.
///
/// Supports common chord types:
/// - Major: "C", "Cmaj", "CM"
/// - Minor: "Cm", "Cmin", "C-"
/// - Seventh: "C7", "Cmaj7", "Cm7", "Cdim7"
/// - Suspended: "Csus2", "Csus4"
/// - Augmented: "Caug", "C+"
/// - Diminished: "Cdim", "C°"
/// - Add chords: "Cadd9", "Cadd11"
/// - Extended: "C9", "C11", "C13"
pub fn chord(name: &str) -> Array {
    chord_with_octave(name, 4)
}

/// Parse a chord name to an array of MIDI note numbers with specified octave.
pub fn chord_with_octave(name: &str, octave: i64) -> Array {
    parse_chord(name, octave as i8)
        .into_iter()
        .map(|n| Dynamic::from(n as i64))
        .collect()
}

/// Parse chord name to MIDI notes.
pub fn parse_chord(name: &str, default_octave: i8) -> Vec<u8> {
    let name = name.trim();
    if name.is_empty() {
        return vec![];
    }

    // Parse root note
    let mut chars = name.chars().peekable();
    let root_letter = match chars.next() {
        Some(c) => c.to_ascii_uppercase(),
        None => return vec![],
    };

    let root_base = match root_letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return vec![],
    };

    // Parse accidental
    let mut accidental = 0i8;
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

    // Root MIDI note
    let root = (default_octave + 1) as i16 * 12 + root_base as i16 + accidental as i16;
    if !(0..=127).contains(&root) {
        return vec![];
    }
    let root = root as u8;

    // Parse chord quality
    let quality: String = chars.collect();
    let quality_lower = quality.to_lowercase();

    // Determine intervals based on chord quality
    let intervals = match quality_lower.as_str() {
        // Major chords
        "" | "maj" | "major" | "m" if quality == "M" => vec![0, 4, 7],

        // Minor chords
        "m" | "min" | "minor" | "-" => vec![0, 3, 7],

        // Seventh chords
        "7" => vec![0, 4, 7, 10], // Dominant 7th
        "maj7" | "m7" if quality.starts_with("maj") || quality == "M7" => vec![0, 4, 7, 11], // Major 7th
        "m7" | "min7" | "-7" => vec![0, 3, 7, 10], // Minor 7th
        "dim7" | "°7" => vec![0, 3, 6, 9],         // Diminished 7th
        "m7b5" | "ø7" | "ø" => vec![0, 3, 6, 10],  // Half-diminished

        // Suspended
        "sus2" => vec![0, 2, 7],
        "sus4" | "sus" => vec![0, 5, 7],
        "7sus4" => vec![0, 5, 7, 10],

        // Augmented
        "aug" | "+" => vec![0, 4, 8],
        "aug7" | "+7" => vec![0, 4, 8, 10],

        // Diminished
        "dim" | "°" => vec![0, 3, 6],

        // Add chords
        "add9" => vec![0, 4, 7, 14],
        "add11" => vec![0, 4, 7, 17],
        "add13" => vec![0, 4, 7, 21],

        // Extended chords
        "9" => vec![0, 4, 7, 10, 14],
        "maj9" | "m9" if quality.starts_with("maj") => vec![0, 4, 7, 11, 14],
        "m9" | "min9" => vec![0, 3, 7, 10, 14],
        "11" => vec![0, 4, 7, 10, 14, 17],
        "13" => vec![0, 4, 7, 10, 14, 21],

        // Sixth chords
        "6" => vec![0, 4, 7, 9],
        "m6" | "min6" => vec![0, 3, 7, 9],

        // Power chord
        "5" => vec![0, 7],

        // Default to major if unknown
        _ => vec![0, 4, 7],
    };

    // Build chord notes
    intervals
        .into_iter()
        .map(|i| {
            let note = root as i16 + i;
            note.clamp(0, 127) as u8
        })
        .collect()
}

/// Get scale notes for a given root and scale type.
///
/// Scale types: "major", "minor", "dorian", "phrygian", "lydian",
/// "mixolydian", "locrian", "harmonic_minor", "melodic_minor",
/// "pentatonic", "minor_pentatonic", "blues", "chromatic", "whole_tone"
pub fn scale(root: &str, scale_type: &str) -> Array {
    scale_with_octave(root, scale_type, 4)
}

/// Get scale notes with specified octave.
pub fn scale_with_octave(root: &str, scale_type: &str, octave: i64) -> Array {
    let root_note = match parse_note_name(&format!("{}{}", root, octave)) {
        Some(n) => n,
        None => return Array::new(),
    };

    let intervals = get_scale_intervals(scale_type);

    intervals
        .into_iter()
        .map(|i| Dynamic::from((root_note + i) as i64))
        .collect()
}

/// Get a specific scale degree (1-indexed).
///
/// Returns the MIDI note number for the given scale degree.
pub fn scale_degree(root: &str, scale_type: &str, degree: i64) -> i64 {
    let root_note = match parse_note_name(&format!("{}4", root)) {
        Some(n) => n,
        None => return 60,
    };

    let intervals = get_scale_intervals(scale_type);
    if intervals.is_empty() {
        return root_note as i64;
    }

    // Handle degrees outside the base octave
    let degree = degree - 1; // Convert to 0-indexed
    let octave_offset = degree / intervals.len() as i64;
    let index = (degree % intervals.len() as i64) as usize;

    let interval = intervals.get(index).copied().unwrap_or(0);
    (root_note as i64 + interval as i64 + octave_offset * 12).clamp(0, 127)
}

/// Get scale intervals for a given scale type.
fn get_scale_intervals(scale_type: &str) -> Vec<u8> {
    match scale_type.to_lowercase().as_str() {
        "major" | "ionian" => vec![0, 2, 4, 5, 7, 9, 11],
        "minor" | "natural_minor" | "aeolian" => vec![0, 2, 3, 5, 7, 8, 10],
        "harmonic_minor" => vec![0, 2, 3, 5, 7, 8, 11],
        "melodic_minor" => vec![0, 2, 3, 5, 7, 9, 11],
        "dorian" => vec![0, 2, 3, 5, 7, 9, 10],
        "phrygian" => vec![0, 1, 3, 5, 7, 8, 10],
        "lydian" => vec![0, 2, 4, 6, 7, 9, 11],
        "mixolydian" => vec![0, 2, 4, 5, 7, 9, 10],
        "locrian" => vec![0, 1, 3, 5, 6, 8, 10],
        "pentatonic" | "major_pentatonic" => vec![0, 2, 4, 7, 9],
        "minor_pentatonic" => vec![0, 3, 5, 7, 10],
        "blues" => vec![0, 3, 5, 6, 7, 10],
        "chromatic" => vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        "whole_tone" => vec![0, 2, 4, 6, 8, 10],
        _ => vec![0, 2, 4, 5, 7, 9, 11], // Default to major
    }
}

/// Convert bars to beats using current time signature.
pub fn bars(num_bars: f64) -> f64 {
    let beats = context::beats_per_bar();
    num_bars * beats
}

/// Convert bars to beats (integer overload).
pub fn bars_int(num_bars: i64) -> f64 {
    bars(num_bars as f64)
}

/// Create a range from int to float.
pub fn make_range_if(start: i64, end: f64) -> std::ops::Range<i64> {
    start..(end as i64)
}

/// Create a range from float to float.
pub fn make_range_ff(start: f64, end: f64) -> std::ops::Range<i64> {
    (start as i64)..(end as i64)
}

/// Create a range from float to int.
pub fn make_range_fi(start: f64, end: i64) -> std::ops::Range<i64> {
    (start as i64)..end
}

/// Zip two arrays together into an array of pairs.
pub fn array_zip(arr1: Array, arr2: Array) -> Array {
    arr1.into_iter()
        .zip(arr2)
        .map(|(a, b)| Dynamic::from(vec![a, b]))
        .collect()
}

/// Shuffle an array randomly.
pub fn array_shuffle(mut arr: Array) -> Array {
    let len = arr.len();
    for i in (1..len).rev() {
        let j = (next_random() as usize) % (i + 1);
        arr.swap(i, j);
    }
    arr
}

/// Rotate array elements by n positions (positive = right, negative = left).
pub fn array_rotate(arr: Array, n: i64) -> Array {
    if arr.is_empty() {
        return arr;
    }
    let len = arr.len() as i64;
    let n = ((n % len) + len) % len; // Normalize to positive
    let n = n as usize;
    let mut result = Array::new();
    result.extend(arr[len as usize - n..].iter().cloned());
    result.extend(arr[..len as usize - n].iter().cloned());
    result
}

/// Reverse an array.
pub fn array_reverse(arr: Array) -> Array {
    arr.into_iter().rev().collect()
}

/// Flatten nested arrays into a single array.
pub fn array_flatten(arr: Array) -> Array {
    let mut result = Array::new();
    for item in arr {
        if item.is_array() {
            let nested: Array = item.cast();
            result.extend(array_flatten(nested));
        } else {
            result.push(item);
        }
    }
    result
}

/// Repeat an array n times.
pub fn array_repeat(arr: Array, n: i64) -> Array {
    let mut result = Array::new();
    for _ in 0..n.max(0) {
        result.extend(arr.clone());
    }
    result
}

/// Take first n elements from array.
pub fn array_take(arr: Array, n: i64) -> Array {
    arr.into_iter().take(n.max(0) as usize).collect()
}

/// Skip first n elements from array.
pub fn array_skip(arr: Array, n: i64) -> Array {
    arr.into_iter().skip(n.max(0) as usize).collect()
}

// ============================================================================
// Random Number Generation
// ============================================================================

/// Initialize RNG state if needed, using a time-based seed.
fn init_rng_if_needed() {
    if RNG_STATE.load(Ordering::Relaxed) == 0 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(12345678);
            RNG_STATE.store(seed, Ordering::Relaxed);
        }
        #[cfg(target_arch = "wasm32")]
        {
            // In WASM, use a simple constant seed (can be changed via random_seed)
            RNG_STATE.store(88172645463325252, Ordering::Relaxed);
        }
    }
}

/// Simple xorshift64 PRNG.
fn next_random() -> u64 {
    init_rng_if_needed();
    let mut x = RNG_STATE.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    RNG_STATE.store(x, Ordering::Relaxed);
    x
}

/// Generate a random float between 0.0 and 1.0.
pub fn random() -> f64 {
    (next_random() as f64) / (u64::MAX as f64)
}

/// Generate a random float in range [min, max).
pub fn random_range_ff(min: f64, max: f64) -> f64 {
    min + random() * (max - min)
}

/// Generate a random integer in range [min, max].
pub fn random_range_ii(min: i64, max: i64) -> i64 {
    if min >= max {
        return min;
    }
    min + ((next_random() as i64).abs() % (max - min + 1))
}

/// Generate a random integer in range [0, max).
pub fn random_int(max: i64) -> i64 {
    if max <= 0 {
        return 0;
    }
    (next_random() as i64).abs() % max
}

/// Choose a random element from an array.
pub fn random_choice(arr: Array) -> Dynamic {
    if arr.is_empty() {
        return Dynamic::UNIT;
    }
    let idx = (next_random() as usize) % arr.len();
    arr[idx].clone()
}

/// Seed the random number generator.
pub fn random_seed(seed: i64) {
    RNG_STATE.store(seed as u64, Ordering::Relaxed);
}

// ============================================================================
// Math Utilities
// ============================================================================

/// Clamp a float value between min and max.
pub fn clamp_fff(value: f64, min: f64, max: f64) -> f64 {
    value.clamp(min, max)
}

/// Clamp an integer value between min and max.
pub fn clamp_iii(value: i64, min: i64, max: i64) -> i64 {
    value.clamp(min, max)
}

/// Linear interpolation between a and b.
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Map a value from one range to another.
pub fn map_range(value: f64, in_min: f64, in_max: f64, out_min: f64, out_max: f64) -> f64 {
    let normalized = (value - in_min) / (in_max - in_min);
    out_min + normalized * (out_max - out_min)
}

/// Smoothstep interpolation (eases in and out).
pub fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Wrap a value within a range [0, max).
pub fn wrap(value: f64, max: f64) -> f64 {
    if max <= 0.0 {
        return 0.0;
    }
    ((value % max) + max) % max
}

/// Quantize a value to the nearest multiple of step.
pub fn quantize(value: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return value;
    }
    (value / step).round() * step
}

/// Convert MIDI note number to frequency in Hz.
pub fn midi_to_freq(note: f64) -> f64 {
    440.0 * 2.0_f64.powf((note - 69.0) / 12.0)
}

/// Convert frequency in Hz to MIDI note number.
pub fn freq_to_midi(freq: f64) -> f64 {
    if freq <= 0.0 {
        return 0.0;
    }
    69.0 + 12.0 * (freq / 440.0).log2()
}

// ============================================================================
// Time Utilities (native only)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
/// Get current Unix timestamp in seconds.
pub fn timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(not(target_arch = "wasm32"))]
/// Get current Unix timestamp in milliseconds.
pub fn timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ============================================================================
// Type Conversion Utilities
// ============================================================================

/// Convert float to integer.
pub fn to_int(value: f64) -> i64 {
    value as i64
}

/// Convert integer to float.
pub fn to_float_from_int(value: i64) -> f64 {
    value as f64
}

/// Convert integer to string.
pub fn to_string_int(value: i64) -> String {
    value.to_string()
}

/// Convert float to string.
pub fn to_string_float(value: f64) -> String {
    value.to_string()
}

/// Parse a time specification string (e.g., "2b", "1/4", "500ms") to beats.
pub fn parse_time_spec(spec: &str, tempo: f64) -> f64 {
    let spec = spec.trim().to_lowercase();

    // Beats: "4b", "2.5b"
    if spec.ends_with('b') {
        if let Ok(beats) = spec[..spec.len() - 1].parse::<f64>() {
            return beats;
        }
    }

    // Bars: "2bars", "1bar"
    if spec.ends_with("bars") || spec.ends_with("bar") {
        let num_str = spec.trim_end_matches("bars").trim_end_matches("bar").trim();
        if let Ok(num_bars) = num_str.parse::<f64>() {
            return num_bars * 4.0;
        }
    }

    // Milliseconds: "500ms"
    if spec.ends_with("ms") {
        if let Ok(ms) = spec[..spec.len() - 2].parse::<f64>() {
            let beats_per_second = tempo / 60.0;
            return ms / 1000.0 * beats_per_second;
        }
    }

    // Seconds: "2s", "1.5s"
    if spec.ends_with('s') && !spec.ends_with("ms") {
        if let Ok(secs) = spec[..spec.len() - 1].parse::<f64>() {
            let beats_per_second = tempo / 60.0;
            return secs * beats_per_second;
        }
    }

    // Fraction: "1/4", "1/8"
    if spec.contains('/') {
        let parts: Vec<&str> = spec.split('/').collect();
        if parts.len() == 2 {
            if let (Ok(num), Ok(denom)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                return 4.0 * num / denom;
            }
        }
    }

    // Default: try to parse as beats
    spec.parse::<f64>().unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Note Parsing Tests ====================

    #[test]
    fn test_parse_note_name_basic() {
        // Standard notes at octave 4 (middle C octave)
        assert_eq!(parse_note_name("C4"), Some(60));
        assert_eq!(parse_note_name("D4"), Some(62));
        assert_eq!(parse_note_name("E4"), Some(64));
        assert_eq!(parse_note_name("F4"), Some(65));
        assert_eq!(parse_note_name("G4"), Some(67));
        assert_eq!(parse_note_name("A4"), Some(69));
        assert_eq!(parse_note_name("B4"), Some(71));
    }

    #[test]
    fn test_parse_note_name_accidentals() {
        // Sharps
        assert_eq!(parse_note_name("C#4"), Some(61));
        assert_eq!(parse_note_name("F#4"), Some(66));
        assert_eq!(parse_note_name("G#4"), Some(68));

        // Flats
        assert_eq!(parse_note_name("Db4"), Some(61));
        assert_eq!(parse_note_name("Eb4"), Some(63));
        assert_eq!(parse_note_name("Bb4"), Some(70));

        // Double accidentals
        assert_eq!(parse_note_name("C##4"), Some(62));
        assert_eq!(parse_note_name("Dbb4"), Some(60));

        // Unicode accidentals
        assert_eq!(parse_note_name("C♯4"), Some(61));
        assert_eq!(parse_note_name("D♭4"), Some(61));
    }

    #[test]
    fn test_parse_note_name_octaves() {
        assert_eq!(parse_note_name("C0"), Some(12));
        assert_eq!(parse_note_name("C1"), Some(24));
        assert_eq!(parse_note_name("C2"), Some(36));
        assert_eq!(parse_note_name("C3"), Some(48));
        assert_eq!(parse_note_name("C4"), Some(60));
        assert_eq!(parse_note_name("C5"), Some(72));
        assert_eq!(parse_note_name("C6"), Some(84));
        assert_eq!(parse_note_name("C7"), Some(96));
        assert_eq!(parse_note_name("C8"), Some(108));
        assert_eq!(parse_note_name("G9"), Some(127)); // Highest valid note
    }

    #[test]
    fn test_parse_note_name_edge_cases() {
        // Empty string
        assert_eq!(parse_note_name(""), None);

        // Whitespace handling
        assert_eq!(parse_note_name("  C4  "), Some(60));

        // Lowercase
        assert_eq!(parse_note_name("c4"), Some(60));
        assert_eq!(parse_note_name("a4"), Some(69));

        // Invalid notes
        assert_eq!(parse_note_name("X4"), None);
        assert_eq!(parse_note_name("H4"), None);

        // Default octave (4)
        assert_eq!(parse_note_name("C"), Some(60));

        // Out of MIDI range (returns None)
        assert_eq!(parse_note_name("C-2"), None); // Too low
    }

    // ==================== Chord Parsing Tests ====================

    #[test]
    fn test_parse_chord_major() {
        // C major: C E G (0, 4, 7 semitones from root)
        let chord = parse_chord("C", 4);
        assert_eq!(chord, vec![60, 64, 67]);

        // G major
        let chord = parse_chord("G", 4);
        assert_eq!(chord, vec![67, 71, 74]);

        // Alternative spellings
        assert_eq!(parse_chord("Cmaj", 4), vec![60, 64, 67]);
        assert_eq!(parse_chord("Cmajor", 4), vec![60, 64, 67]);
    }

    #[test]
    fn test_parse_chord_minor() {
        // C minor: C Eb G (0, 3, 7 semitones from root)
        let chord = parse_chord("Cm", 4);
        assert_eq!(chord, vec![60, 63, 67]);

        // Alternative spellings
        assert_eq!(parse_chord("Cmin", 4), vec![60, 63, 67]);
        assert_eq!(parse_chord("Cminor", 4), vec![60, 63, 67]);
        assert_eq!(parse_chord("C-", 4), vec![60, 63, 67]);

        // A minor
        let chord = parse_chord("Am", 4);
        assert_eq!(chord, vec![69, 72, 76]);
    }

    #[test]
    fn test_parse_chord_seventh() {
        // C dominant 7th: C E G Bb (0, 4, 7, 10)
        let chord = parse_chord("C7", 4);
        assert_eq!(chord, vec![60, 64, 67, 70]);

        // C major 7th: C E G B (0, 4, 7, 11)
        let chord = parse_chord("Cmaj7", 4);
        assert_eq!(chord, vec![60, 64, 67, 71]);

        // C minor 7th: C Eb G Bb (0, 3, 7, 10)
        let chord = parse_chord("Cm7", 4);
        assert_eq!(chord, vec![60, 63, 67, 70]);

        // C diminished 7th: C Eb Gb Bbb (0, 3, 6, 9)
        let chord = parse_chord("Cdim7", 4);
        assert_eq!(chord, vec![60, 63, 66, 69]);

        // Half-diminished (m7b5)
        let chord = parse_chord("Cm7b5", 4);
        assert_eq!(chord, vec![60, 63, 66, 70]);
    }

    #[test]
    fn test_parse_chord_suspended() {
        // Csus2: C D G (0, 2, 7)
        let chord = parse_chord("Csus2", 4);
        assert_eq!(chord, vec![60, 62, 67]);

        // Csus4: C F G (0, 5, 7)
        let chord = parse_chord("Csus4", 4);
        assert_eq!(chord, vec![60, 65, 67]);

        // Csus (defaults to sus4)
        let chord = parse_chord("Csus", 4);
        assert_eq!(chord, vec![60, 65, 67]);

        // 7sus4: C F G Bb (0, 5, 7, 10)
        let chord = parse_chord("C7sus4", 4);
        assert_eq!(chord, vec![60, 65, 67, 70]);
    }

    #[test]
    fn test_parse_chord_augmented_diminished() {
        // C augmented: C E G# (0, 4, 8)
        let chord = parse_chord("Caug", 4);
        assert_eq!(chord, vec![60, 64, 68]);

        // Alternative spelling
        assert_eq!(parse_chord("C+", 4), vec![60, 64, 68]);

        // C diminished: C Eb Gb (0, 3, 6)
        let chord = parse_chord("Cdim", 4);
        assert_eq!(chord, vec![60, 63, 66]);
    }

    #[test]
    fn test_parse_chord_extended() {
        // C9: C E G Bb D (0, 4, 7, 10, 14)
        let chord = parse_chord("C9", 4);
        assert_eq!(chord, vec![60, 64, 67, 70, 74]);

        // Cadd9: C E G D (0, 4, 7, 14)
        let chord = parse_chord("Cadd9", 4);
        assert_eq!(chord, vec![60, 64, 67, 74]);

        // C6: C E G A (0, 4, 7, 9)
        let chord = parse_chord("C6", 4);
        assert_eq!(chord, vec![60, 64, 67, 69]);

        // Cm6: C Eb G A (0, 3, 7, 9)
        let chord = parse_chord("Cm6", 4);
        assert_eq!(chord, vec![60, 63, 67, 69]);

        // Power chord C5: C G (0, 7)
        let chord = parse_chord("C5", 4);
        assert_eq!(chord, vec![60, 67]);
    }

    #[test]
    fn test_parse_chord_accidentals() {
        // F# major
        let chord = parse_chord("F#", 4);
        assert_eq!(chord, vec![66, 70, 73]);

        // Bb minor
        let chord = parse_chord("Bbm", 4);
        assert_eq!(chord, vec![70, 73, 77]);

        // Eb major 7th
        let chord = parse_chord("Ebmaj7", 4);
        assert_eq!(chord, vec![63, 67, 70, 74]);
    }

    #[test]
    fn test_parse_chord_octaves() {
        // C major at octave 3
        let chord = parse_chord("C", 3);
        assert_eq!(chord, vec![48, 52, 55]);

        // C major at octave 5
        let chord = parse_chord("C", 5);
        assert_eq!(chord, vec![72, 76, 79]);

        // C major at octave 2
        let chord = parse_chord("C", 2);
        assert_eq!(chord, vec![36, 40, 43]);
    }

    #[test]
    fn test_parse_chord_edge_cases() {
        // Empty string
        assert_eq!(parse_chord("", 4), vec![]);

        // Whitespace
        assert_eq!(parse_chord("  C  ", 4), vec![60, 64, 67]);

        // Lowercase
        assert_eq!(parse_chord("c", 4), vec![60, 64, 67]);

        // Invalid root note
        assert_eq!(parse_chord("X", 4), vec![]);

        // Unknown quality defaults to major
        assert_eq!(parse_chord("Cxyz", 4), vec![60, 64, 67]);
    }

    #[test]
    fn test_parse_chord_midi_clamping() {
        // High octave chord - notes should be clamped to 127
        let chord = parse_chord("G", 9);
        // G9 = 127, B9 would be 131 -> clamped to 127, D10 would be 134 -> clamped to 127
        assert!(chord.iter().all(|&n| n <= 127));
    }

    // ==================== Scale Tests ====================

    #[test]
    fn test_get_scale_intervals_major() {
        let intervals = get_scale_intervals("major");
        assert_eq!(intervals, vec![0, 2, 4, 5, 7, 9, 11]);

        // Alternative name
        assert_eq!(get_scale_intervals("ionian"), vec![0, 2, 4, 5, 7, 9, 11]);
    }

    #[test]
    fn test_get_scale_intervals_minor() {
        let intervals = get_scale_intervals("minor");
        assert_eq!(intervals, vec![0, 2, 3, 5, 7, 8, 10]);

        // Alternative names
        assert_eq!(
            get_scale_intervals("natural_minor"),
            vec![0, 2, 3, 5, 7, 8, 10]
        );
        assert_eq!(get_scale_intervals("aeolian"), vec![0, 2, 3, 5, 7, 8, 10]);
    }

    #[test]
    fn test_get_scale_intervals_modes() {
        // All 7 church modes
        assert_eq!(get_scale_intervals("dorian"), vec![0, 2, 3, 5, 7, 9, 10]);
        assert_eq!(get_scale_intervals("phrygian"), vec![0, 1, 3, 5, 7, 8, 10]);
        assert_eq!(get_scale_intervals("lydian"), vec![0, 2, 4, 6, 7, 9, 11]);
        assert_eq!(
            get_scale_intervals("mixolydian"),
            vec![0, 2, 4, 5, 7, 9, 10]
        );
        assert_eq!(get_scale_intervals("locrian"), vec![0, 1, 3, 5, 6, 8, 10]);
    }

    #[test]
    fn test_get_scale_intervals_harmonic_melodic() {
        // Harmonic minor: raised 7th
        assert_eq!(
            get_scale_intervals("harmonic_minor"),
            vec![0, 2, 3, 5, 7, 8, 11]
        );

        // Melodic minor: raised 6th and 7th
        assert_eq!(
            get_scale_intervals("melodic_minor"),
            vec![0, 2, 3, 5, 7, 9, 11]
        );
    }

    #[test]
    fn test_get_scale_intervals_pentatonic() {
        // Major pentatonic: 1 2 3 5 6
        assert_eq!(get_scale_intervals("pentatonic"), vec![0, 2, 4, 7, 9]);
        assert_eq!(get_scale_intervals("major_pentatonic"), vec![0, 2, 4, 7, 9]);

        // Minor pentatonic: 1 b3 4 5 b7
        assert_eq!(
            get_scale_intervals("minor_pentatonic"),
            vec![0, 3, 5, 7, 10]
        );
    }

    #[test]
    fn test_get_scale_intervals_blues() {
        // Blues scale: 1 b3 4 b5 5 b7
        assert_eq!(get_scale_intervals("blues"), vec![0, 3, 5, 6, 7, 10]);
    }

    #[test]
    fn test_get_scale_intervals_chromatic() {
        // All 12 semitones
        assert_eq!(
            get_scale_intervals("chromatic"),
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
        );
    }

    #[test]
    fn test_get_scale_intervals_whole_tone() {
        // 6 whole tones
        assert_eq!(get_scale_intervals("whole_tone"), vec![0, 2, 4, 6, 8, 10]);
    }

    #[test]
    fn test_get_scale_intervals_unknown() {
        // Unknown scale defaults to major
        assert_eq!(
            get_scale_intervals("unknown_scale"),
            vec![0, 2, 4, 5, 7, 9, 11]
        );
    }

    #[test]
    fn test_get_scale_intervals_case_insensitive() {
        assert_eq!(get_scale_intervals("MAJOR"), vec![0, 2, 4, 5, 7, 9, 11]);
        assert_eq!(get_scale_intervals("Minor"), vec![0, 2, 3, 5, 7, 8, 10]);
        assert_eq!(get_scale_intervals("DoRiAn"), vec![0, 2, 3, 5, 7, 9, 10]);
    }

    #[test]
    fn test_scale_degree_basic() {
        // C major scale degrees (C=60, D=62, E=64, F=65, G=67, A=69, B=71)
        assert_eq!(scale_degree("C", "major", 1), 60); // C
        assert_eq!(scale_degree("C", "major", 2), 62); // D
        assert_eq!(scale_degree("C", "major", 3), 64); // E
        assert_eq!(scale_degree("C", "major", 4), 65); // F
        assert_eq!(scale_degree("C", "major", 5), 67); // G
        assert_eq!(scale_degree("C", "major", 6), 69); // A
        assert_eq!(scale_degree("C", "major", 7), 71); // B
    }

    #[test]
    fn test_scale_degree_octave_wrapping() {
        // Degree 8 should be root + octave
        assert_eq!(scale_degree("C", "major", 8), 72); // C5

        // Degree 9 should be 2nd + octave
        assert_eq!(scale_degree("C", "major", 9), 74); // D5

        // Degree 15 should be root + 2 octaves
        assert_eq!(scale_degree("C", "major", 15), 84); // C6
    }

    #[test]
    fn test_scale_degree_minor() {
        // A minor scale degrees (A=69, B=71, C=72, D=74, E=76, F=77, G=79)
        assert_eq!(scale_degree("A", "minor", 1), 69); // A
        assert_eq!(scale_degree("A", "minor", 3), 72); // C (minor 3rd)
        assert_eq!(scale_degree("A", "minor", 5), 76); // E
    }

    #[test]
    fn test_scale_degree_pentatonic() {
        // C pentatonic: C D E G A (5 notes)
        assert_eq!(scale_degree("C", "pentatonic", 1), 60); // C
        assert_eq!(scale_degree("C", "pentatonic", 2), 62); // D
        assert_eq!(scale_degree("C", "pentatonic", 3), 64); // E
        assert_eq!(scale_degree("C", "pentatonic", 4), 67); // G
        assert_eq!(scale_degree("C", "pentatonic", 5), 69); // A
        assert_eq!(scale_degree("C", "pentatonic", 6), 72); // C (next octave)
    }

    #[test]
    fn test_scale_degree_clamping() {
        // Very high degree should be clamped to 127
        let degree = scale_degree("C", "major", 100);
        assert!(degree <= 127);
    }

    // ==================== dB Conversion Tests ====================

    #[test]
    fn test_db_conversion() {
        // 0 dB = unity gain (1.0)
        assert!((db(0.0) - 1.0).abs() < 0.0001);

        // -6 dB ≈ 0.5 (half amplitude)
        assert!((db(-6.0) - 0.501).abs() < 0.01);

        // -12 dB ≈ 0.25
        assert!((db(-12.0) - 0.251).abs() < 0.01);

        // -20 dB = 0.1
        assert!((db(-20.0) - 0.1).abs() < 0.0001);

        // -40 dB = 0.01
        assert!((db(-40.0) - 0.01).abs() < 0.0001);

        // -60 dB = 0.001
        assert!((db(-60.0) - 0.001).abs() < 0.0001);

        // +6 dB ≈ 2.0 (double amplitude)
        assert!((db(6.0) - 1.995).abs() < 0.01);

        // +12 dB ≈ 4.0
        assert!((db(12.0) - 3.981).abs() < 0.01);
    }

    #[test]
    fn test_db_int() {
        assert!((db_int(0) - 1.0).abs() < 0.0001);
        assert!((db_int(-6) - 0.501).abs() < 0.01);
        assert!((db_int(-20) - 0.1).abs() < 0.0001);
    }

    // ==================== Time Specification Tests ====================

    #[test]
    fn test_parse_time_spec_beats() {
        let tempo = 120.0;
        assert!((parse_time_spec("1b", tempo) - 1.0).abs() < 0.001);
        assert!((parse_time_spec("4b", tempo) - 4.0).abs() < 0.001);
        assert!((parse_time_spec("0.5b", tempo) - 0.5).abs() < 0.001);
        assert!((parse_time_spec("2.5b", tempo) - 2.5).abs() < 0.001);
    }

    #[test]
    fn test_parse_time_spec_bars() {
        let tempo = 120.0;
        assert!((parse_time_spec("1bar", tempo) - 4.0).abs() < 0.001);
        assert!((parse_time_spec("2bars", tempo) - 8.0).abs() < 0.001);
        assert!((parse_time_spec("0.5bar", tempo) - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_time_spec_milliseconds() {
        // At 120 BPM, 500ms = 1 beat
        let tempo = 120.0;
        assert!((parse_time_spec("500ms", tempo) - 1.0).abs() < 0.001);
        assert!((parse_time_spec("1000ms", tempo) - 2.0).abs() < 0.001);
        assert!((parse_time_spec("250ms", tempo) - 0.5).abs() < 0.001);

        // At 60 BPM, 1000ms = 1 beat
        let tempo = 60.0;
        assert!((parse_time_spec("1000ms", tempo) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_time_spec_seconds() {
        // At 120 BPM, 1s = 2 beats
        let tempo = 120.0;
        assert!((parse_time_spec("1s", tempo) - 2.0).abs() < 0.001);
        assert!((parse_time_spec("0.5s", tempo) - 1.0).abs() < 0.001);
        assert!((parse_time_spec("2s", tempo) - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_time_spec_fractions() {
        let tempo = 120.0;
        // 1/4 note = 1 beat
        assert!((parse_time_spec("1/4", tempo) - 1.0).abs() < 0.001);
        // 1/8 note = 0.5 beats
        assert!((parse_time_spec("1/8", tempo) - 0.5).abs() < 0.001);
        // 1/16 note = 0.25 beats
        assert!((parse_time_spec("1/16", tempo) - 0.25).abs() < 0.001);
        // 1/2 note = 2 beats
        assert!((parse_time_spec("1/2", tempo) - 2.0).abs() < 0.001);
        // 1/1 note (whole note) = 4 beats
        assert!((parse_time_spec("1/1", tempo) - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_time_spec_raw_number() {
        let tempo = 120.0;
        // Raw number treated as beats
        assert!((parse_time_spec("4", tempo) - 4.0).abs() < 0.001);
        assert!((parse_time_spec("2.5", tempo) - 2.5).abs() < 0.001);
    }

    #[test]
    fn test_parse_time_spec_invalid() {
        let tempo = 120.0;
        // Invalid input defaults to 1.0
        assert!((parse_time_spec("invalid", tempo) - 1.0).abs() < 0.001);
    }

    // ==================== Integration / Consistency Tests ====================

    #[test]
    fn test_chord_and_scale_consistency() {
        // The notes in a C major chord should be scale degrees 1, 3, 5
        let c_major_chord = parse_chord("C", 4);
        assert_eq!(c_major_chord[0], scale_degree("C", "major", 1) as u8); // Root
        assert_eq!(c_major_chord[1], scale_degree("C", "major", 3) as u8); // 3rd
        assert_eq!(c_major_chord[2], scale_degree("C", "major", 5) as u8); // 5th
    }

    #[test]
    fn test_minor_chord_and_scale_consistency() {
        // The notes in a C minor chord should use minor scale degrees
        let c_minor_chord = parse_chord("Cm", 4);
        // C minor: C(60) Eb(63) G(67)
        // C minor scale degree 1 = 60, degree 3 = 63 (flat 3rd), degree 5 = 67
        assert_eq!(c_minor_chord[0], 60); // C
        assert_eq!(c_minor_chord[1], 63); // Eb (minor 3rd)
        assert_eq!(c_minor_chord[2], 67); // G (perfect 5th)
    }

    #[test]
    fn test_all_natural_notes_roundtrip() {
        // Verify all natural notes parse correctly at all standard octaves
        let notes = ["C", "D", "E", "F", "G", "A", "B"];
        for octave in 0..=8 {
            for note in &notes {
                let note_str = format!("{}{}", note, octave);
                let result = parse_note_name(&note_str);
                assert!(result.is_some(), "Failed to parse note: {}", note_str);
                let midi = result.unwrap();
                assert!(
                    midi <= 127,
                    "MIDI note {} out of range for {}",
                    midi,
                    note_str
                );
            }
        }
    }

    #[test]
    fn test_enharmonic_equivalents() {
        // C# = Db
        assert_eq!(parse_note_name("C#4"), parse_note_name("Db4"));
        // D# = Eb
        assert_eq!(parse_note_name("D#4"), parse_note_name("Eb4"));
        // F# = Gb
        assert_eq!(parse_note_name("F#4"), parse_note_name("Gb4"));
        // G# = Ab
        assert_eq!(parse_note_name("G#4"), parse_note_name("Ab4"));
        // A# = Bb
        assert_eq!(parse_note_name("A#4"), parse_note_name("Bb4"));
    }

    #[test]
    fn test_scale_note_count() {
        // Verify each scale type has the correct number of notes
        assert_eq!(get_scale_intervals("major").len(), 7);
        assert_eq!(get_scale_intervals("minor").len(), 7);
        assert_eq!(get_scale_intervals("dorian").len(), 7);
        assert_eq!(get_scale_intervals("pentatonic").len(), 5);
        assert_eq!(get_scale_intervals("blues").len(), 6);
        assert_eq!(get_scale_intervals("chromatic").len(), 12);
        assert_eq!(get_scale_intervals("whole_tone").len(), 6);
    }

    #[test]
    fn test_scale_intervals_ascending() {
        // All scale intervals should be strictly ascending
        let scales = [
            "major",
            "minor",
            "dorian",
            "phrygian",
            "lydian",
            "mixolydian",
            "locrian",
            "harmonic_minor",
            "melodic_minor",
            "pentatonic",
            "blues",
            "chromatic",
            "whole_tone",
        ];

        for scale_name in &scales {
            let intervals = get_scale_intervals(scale_name);
            for i in 1..intervals.len() {
                assert!(
                    intervals[i] > intervals[i - 1],
                    "Scale {} intervals not ascending at position {}",
                    scale_name,
                    i
                );
            }
        }
    }

    #[test]
    fn test_scale_intervals_within_octave() {
        // All scale intervals should be within one octave (0-11)
        let scales = [
            "major",
            "minor",
            "dorian",
            "phrygian",
            "lydian",
            "mixolydian",
            "locrian",
            "harmonic_minor",
            "melodic_minor",
            "pentatonic",
            "blues",
            "chromatic",
            "whole_tone",
        ];

        for scale_name in &scales {
            let intervals = get_scale_intervals(scale_name);
            for &interval in &intervals {
                assert!(
                    interval <= 11,
                    "Scale {} has interval {} > 11",
                    scale_name,
                    interval
                );
            }
            // First interval should always be 0 (root)
            assert_eq!(
                intervals[0], 0,
                "Scale {} should start with interval 0",
                scale_name
            );
        }
    }
}
