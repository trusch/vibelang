//! Global API functions.
//!
//! These functions control global runtime state like tempo and time signature.

use rhai::Engine;
use vibelang_core2::types::TimeSignature;

use crate::context;

/// Register global functions with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Tempo
    engine.register_fn("set_tempo", set_tempo);
    engine.register_fn("set_tempo", set_tempo_int);
    engine.register_fn("get_tempo", get_tempo);

    // Time signature
    engine.register_fn("set_time_signature", set_time_signature);

    // Quantization
    engine.register_fn("set_quantization", set_quantization);
    engine.register_fn("set_quantization", set_quantization_int);
    engine.register_fn("get_quantization", get_quantization);

    // Transport info (from context, not runtime - returns script-time values)
    engine.register_fn("get_current_bar", get_current_bar);
}

/// Set the tempo in BPM.
pub fn set_tempo(bpm: f64) {
    context::with_state(|state| {
        state.tempo = bpm;
    });
}

/// Set the tempo in BPM (integer overload).
pub fn set_tempo_int(bpm: i64) {
    set_tempo(bpm as f64);
}

/// Get the current tempo.
pub fn get_tempo() -> f64 {
    context::get_tempo()
}

/// Set the time signature.
pub fn set_time_signature(numerator: i64, denominator: i64) {
    context::with_state(|state| {
        state.time_sig = TimeSignature {
            numerator: numerator as u8,
            denominator: denominator as u8,
        };
    });
}

/// Set the quantization in beats.
///
/// Quantization affects when patterns and melodies start.
/// Set to 0 for no quantization.
pub fn set_quantization(beats: f64) {
    context::with_state(|state| {
        state.quantization = beats.max(0.0);
    });
}

/// Set the quantization in beats (integer overload).
pub fn set_quantization_int(beats: i64) {
    set_quantization(beats as f64);
}

/// Get the current quantization setting.
pub fn get_quantization() -> f64 {
    context::with_state(|state| state.quantization)
}

/// Get the current bar number (1-indexed).
///
/// Note: During script execution, this returns 1 since the script
/// runs before playback. Use this for calculations relative to bars.
pub fn get_current_bar() -> i64 {
    // During script execution, we're at the start
    // This is mainly useful for bar-based calculations
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Tempo Tests ====================

    #[test]
    fn test_get_current_bar() {
        // During script execution, always returns 1
        assert_eq!(get_current_bar(), 1);
    }

    // ==================== TimeSignature Tests ====================

    #[test]
    fn test_time_signature_struct() {
        let ts = TimeSignature {
            numerator: 4,
            denominator: 4,
        };
        assert_eq!(ts.numerator, 4);
        assert_eq!(ts.denominator, 4);
    }

    #[test]
    fn test_time_signature_other_values() {
        let ts_3_4 = TimeSignature {
            numerator: 3,
            denominator: 4,
        };
        assert_eq!(ts_3_4.numerator, 3);
        assert_eq!(ts_3_4.denominator, 4);

        let ts_6_8 = TimeSignature {
            numerator: 6,
            denominator: 8,
        };
        assert_eq!(ts_6_8.numerator, 6);
        assert_eq!(ts_6_8.denominator, 8);

        let ts_7_8 = TimeSignature {
            numerator: 7,
            denominator: 8,
        };
        assert_eq!(ts_7_8.numerator, 7);
        assert_eq!(ts_7_8.denominator, 8);
    }
}
