//! LooperBuilder — Rhai-facing builder for looper configuration.

use rhai::{CustomType, Dynamic, EvalAltResult, Position, TypeBuilder};
use vibelang_core::reload::LooperConfig;
use vibelang_core::types::MidiDeviceId;

use crate::api::voice::Voice;
use crate::context;

/// Builder for routing a MIDI device into looper mode for a given voice.
///
/// # Example (Rhai)
/// ```rhai
/// let kbd = midi_device("Keystep");
/// let piano = voice("piano").synth("piano").apply();
/// kbd.looper().channel(1).silence(2.0).quantize(0.5).to(piano);
/// ```
#[derive(Debug, Clone, CustomType)]
pub struct LooperBuilder {
    device_id: MidiDeviceId,
    channel: Option<u8>,
    silence_bars: f64,
    quantize_beats: f64,
}

impl LooperBuilder {
    pub fn new(device_id: MidiDeviceId) -> Self {
        Self {
            device_id,
            channel: None,
            silence_bars: 1.0,
            // No quantization by default. The previous 16th-note default
            // collapsed multiple fast hits onto the same step (e.g., snare
            // rolls turned into a single sustained tone) and snapped every
            // recorded note off the user's intended groove. Opt-in via
            // `.quantize(0.25)` for a tight grid.
            quantize_beats: 0.0,
        }
    }

    /// Optional: filter to a specific MIDI channel (1-16).
    pub fn channel(mut self, ch: i64) -> Self {
        self.channel = Some((ch.clamp(1, 16) - 1) as u8);
        self
    }

    /// Override silence threshold in bars before playback starts. Default: 1.0.
    pub fn silence(mut self, bars: f64) -> Self {
        self.silence_bars = bars.max(0.25);
        self
    }

    /// Override quantization grid in beats. Default: 0.25 (16th notes).
    pub fn quantize(mut self, beats: f64) -> Self {
        self.quantize_beats = beats.max(0.0625);
        self
    }

    /// Finalize: route the looper to a voice. Pushes the LooperConfig to script state.
    pub fn to(self, voice: Voice) {
        let voice_id = context::get_or_create_voice_id(&voice.name);
        let config = LooperConfig {
            device_id: self.device_id,
            voice_id,
            channel: self.channel,
            silence_bars: self.silence_bars,
            quantize_beats: self.quantize_beats,
        };
        context::with_state(|state| {
            state.loopers.retain(|l| l.device_id != self.device_id);
            state.loopers.push(config);
            state.midi_inputs.insert(self.device_id);
        });
    }
}
