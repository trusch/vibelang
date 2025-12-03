//! Global API functions.
//!
//! These functions control global runtime state like tempo, transport, and quantization.

use crate::state::StateMessage;
use rhai::Engine;

use super::require_handle;

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

    // Transport
    engine.register_fn("get_current_beat", get_current_beat);
    engine.register_fn("get_current_bar", get_current_bar);
    engine.register_fn("nudge_transport", nudge_transport);
    engine.register_fn("jump_to_start", jump_to_start);

    // Latency - TODO: Add SetLatency message
    // engine.register_fn("set_latency_ms", set_latency_ms);
}

/// Set the tempo in BPM.
pub fn set_tempo(bpm: f64) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::SetBpm { bpm });
}

/// Set the tempo in BPM (integer overload).
pub fn set_tempo_int(bpm: i64) {
    set_tempo(bpm as f64);
}

/// Get the current tempo.
pub fn get_tempo() -> f64 {
    let handle = require_handle();
    handle.with_state(|state| state.tempo)
}

/// Set the time signature.
pub fn set_time_signature(numerator: i64, denominator: i64) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::SetTimeSignature {
        numerator: numerator as u32,
        denominator: denominator as u32,
    });
}

/// Set the quantization in beats.
pub fn set_quantization(beats: f64) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::SetQuantization { beats });
}

/// Get the current beat position.
pub fn get_current_beat() -> f64 {
    let handle = require_handle();
    handle.with_state(|state| state.current_beat)
}

/// Get the current bar number (1-indexed).
pub fn get_current_bar() -> i64 {
    let handle = require_handle();
    handle.with_state(|state| {
        let beats_per_bar = state.time_signature.beats_per_bar();
        (state.current_beat / beats_per_bar).floor() as i64 + 1
    })
}

/// Nudge the transport by a number of beats.
pub fn nudge_transport(beats: f64) {
    let handle = require_handle();
    let current = handle.with_state(|state| state.current_beat);
    let new_beat = (current + beats).max(0.0);
    let _ = handle.send(StateMessage::SeekTransport { beat: new_beat });
}

/// Jump to the start of the transport (beat 0).
pub fn jump_to_start() {
    let handle = require_handle();
    let _ = handle.send(StateMessage::SeekTransport { beat: 0.0 });
}
