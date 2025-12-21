//! Helper functions for Rhai scripts.
//!
//! Utility functions for common operations like dB conversion, note parsing, etc.

use rhai::{Array, Dynamic, Engine};

/// Register helper functions with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // dB conversion
    engine.register_fn("db", db);
    engine.register_fn("db", db_int);

    // Note parsing
    engine.register_fn("note", note);

    // Time helpers
    engine.register_fn("bars", bars);
    engine.register_fn("bars", bars_int);

    // Range operators for mixed types (needed for patterns like `0..bars(8)`)
    engine.register_fn("..", make_range_if);
    engine.register_fn("..", make_range_ff);
    engine.register_fn("..", make_range_fi);

    // Sleep (useful for scripts that need to wait)
    engine.register_fn("sleep", sleep);
    engine.register_fn("sleep_secs", sleep_secs);

    // Exit
    engine.register_fn("exit", exit);
    engine.register_fn("exit_with_code", exit_with_code);

    // Array utilities
    // zip(a, b) or a.zip(b) - both work because Rhai converts method calls to function calls
    engine.register_fn("zip", array_zip);
}

/// Convert decibels to linear amplitude.
///
/// # Example
/// ```rhai
/// let amp = db(-6.0);  // Returns ~0.5
/// ```
pub fn db(decibels: f64) -> f64 {
    10.0_f64.powf(decibels / 20.0)
}

/// Convert decibels to linear amplitude (integer overload).
pub fn db_int(decibels: i64) -> f64 {
    db(decibels as f64)
}

/// Parse a note name to MIDI note number.
///
/// # Example
/// ```rhai
/// let midi = note("C4");  // Returns 60
/// let midi = note("A#3"); // Returns 58
/// ```
pub fn note(name: &str) -> i64 {
    parse_note_name(name).unwrap_or(60) as i64
}

/// Convert bars to beats using current time signature.
///
/// # Example
/// ```rhai
/// let beats = bars(2.0);  // Returns 8.0 in 4/4 time
/// ```
pub fn bars(num_bars: f64) -> f64 {
    let handle = super::require_handle();
    let beats_per_bar = handle.with_state(|state| state.time_signature.beats_per_bar());
    num_bars * beats_per_bar
}

/// Convert bars to beats (integer overload).
pub fn bars_int(num_bars: i64) -> f64 {
    bars(num_bars as f64)
}

/// Create a range from int to float - needed for patterns like `0..bars(8)`
pub fn make_range_if(start: i64, end: f64) -> std::ops::Range<i64> {
    (start)..(end as i64)
}

/// Create a range from float to float
pub fn make_range_ff(start: f64, end: f64) -> std::ops::Range<i64> {
    (start as i64)..(end as i64)
}

/// Create a range from float to int
pub fn make_range_fi(start: f64, end: i64) -> std::ops::Range<i64> {
    (start as i64)..end
}

/// Sleep for a number of milliseconds.
pub fn sleep(ms: i64) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
}

/// Sleep for a number of seconds.
pub fn sleep_secs(secs: f64) {
    std::thread::sleep(std::time::Duration::from_secs_f64(secs));
}

/// Exit the script.
pub fn exit() {
    std::process::exit(0);
}

/// Exit with a specific code.
pub fn exit_with_code(code: i64) {
    std::process::exit(code as i32);
}

/// Zip two arrays together into an array of pairs.
///
/// # Example
/// ```rhai
/// let a = [1, 2, 3];
/// let b = [4, 5, 6];
/// let zipped = zip(a, b);  // [[1, 4], [2, 5], [3, 6]]
/// ```
pub fn array_zip(arr1: Array, arr2: Array) -> Array {
    arr1.into_iter()
        .zip(arr2)
        .map(|(a, b)| Dynamic::from(vec![a, b]))
        .collect()
}

/// Parse a note name to MIDI note number.
pub fn parse_note_name(name: &str) -> Option<u8> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let mut chars = name.chars().peekable();

    // Parse note letter (C, D, E, F, G, A, B)
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    // Parse accidental (# or b)
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

    // Parse octave
    let octave_str: String = chars.collect();
    let octave: i8 = octave_str.parse().unwrap_or(4);

    // Calculate MIDI note (C4 = 60)
    let midi = (octave + 1) as i16 * 12 + base as i16 + accidental as i16;

    if (0..=127).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
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
        let num_str = spec
            .trim_end_matches("bars")
            .trim_end_matches("bar")
            .trim();
        if let Ok(num_bars) = num_str.parse::<f64>() {
            // Assume 4/4 time for simplicity
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
                // Interpret as fraction of a bar (4 beats)
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

    #[test]
    fn test_parse_note_name() {
        assert_eq!(parse_note_name("C4"), Some(60));
        assert_eq!(parse_note_name("A4"), Some(69));
        assert_eq!(parse_note_name("C#4"), Some(61));
        assert_eq!(parse_note_name("Db4"), Some(61));
        assert_eq!(parse_note_name("C5"), Some(72));
        assert_eq!(parse_note_name("C3"), Some(48));
    }

    #[test]
    fn test_db() {
        assert!((db(0.0) - 1.0).abs() < 0.001);
        assert!((db(-6.0) - 0.501).abs() < 0.01);
        assert!((db(-20.0) - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_parse_time_spec() {
        let tempo = 120.0;
        assert!((parse_time_spec("4b", tempo) - 4.0).abs() < 0.001);
        assert!((parse_time_spec("1bar", tempo) - 4.0).abs() < 0.001);
        assert!((parse_time_spec("500ms", tempo) - 1.0).abs() < 0.001);
        assert!((parse_time_spec("1/4", tempo) - 1.0).abs() < 0.001);
    }
}
