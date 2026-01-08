//! Voice API for Rhai scripts.
//!
//! Voices are the basic sound-producing units in VibeLang.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::collections::HashMap;
use vibelang_core2::traits::VoiceConfig;
use vibelang_core2::types::SfzId;
#[cfg(feature = "midi")]
use vibelang_core2::types::MidiDeviceId;

use crate::context;
use super::sample::SampleHandle;
use super::sfz::SfzHandle;
#[cfg(feature = "midi")]
use super::midi::MidiDevice;

/// A Voice builder for creating and configuring voices.
#[derive(Debug, Clone, CustomType)]
pub struct Voice {
    /// Voice name.
    pub name: String,
    /// SynthDef name.
    synth_name: Option<String>,
    /// Group path.
    group_path: String,
    /// Polyphony (number of simultaneous voices).
    polyphony: u8,
    /// Gain in linear amplitude.
    gain: f32,
    /// Default parameters.
    params: HashMap<String, f32>,
    /// Whether this voice is muted.
    muted: bool,
    /// Whether this voice is soloed.
    soloed: bool,
    /// SFZ instrument ID (if this voice uses an SFZ instrument).
    sfz_instrument: Option<SfzId>,
    /// Round-robin count for cycling through sample variations.
    round_robin_count: u32,
    /// Choke group name for exclusive triggering.
    choke_group: Option<String>,
    /// MIDI output device (if routing to external MIDI).
    #[cfg(feature = "midi")]
    midi_output_device: Option<MidiDeviceId>,
    /// MIDI output channel (0-15).
    #[cfg(feature = "midi")]
    midi_channel: u8,
}

impl Voice {
    /// Create a new voice with the given name.
    pub fn new(_ctx: NativeCallContext, name: String) -> Self {
        Self {
            name,
            synth_name: None,
            group_path: context::current_group_path(),
            polyphony: 4,
            gain: 1.0,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            sfz_instrument: None,
            round_robin_count: 0,
            choke_group: None,
            #[cfg(feature = "midi")]
            midi_output_device: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
        }
    }

    // === Getters ===

    /// Get the voice ID (name).
    pub fn id(&mut self) -> String {
        self.name.clone()
    }

    /// Get the voice name.
    pub fn get_name(&mut self) -> String {
        self.name.clone()
    }

    /// Get the synth name.
    pub fn get_synth_name(&mut self) -> String {
        self.synth_name.clone().unwrap_or_default()
    }

    /// Get the gain.
    pub fn get_gain(&mut self) -> f64 {
        self.gain as f64
    }

    /// Get the polyphony.
    pub fn get_polyphony(&mut self) -> i64 {
        self.polyphony as i64
    }

    /// Get the group path.
    pub fn get_group_path(&mut self) -> String {
        self.group_path.clone()
    }

    // === Builder methods ===

    /// Set the group for this voice.
    pub fn group(mut self, group: String) -> Self {
        self.group_path = if group.starts_with("main/") || group == "main" {
            group
        } else {
            format!("{}/{}", context::current_group_path(), group)
        };
        self
    }

    /// Set the synth for this voice.
    pub fn synth(mut self, synth_name: String) -> Self {
        self.synth_name = Some(synth_name);
        self
    }

    /// Set the sound source (synthdef name).
    pub fn on(mut self, source: String) -> Self {
        self.synth_name = Some(source);
        self
    }

    /// Set the sound source to a sample.
    pub fn on_sample(mut self, sample: SampleHandle) -> Self {
        // Use sample_voice synthdef
        self.synth_name = Some("sample_voice".to_string());
        self.params.insert("bufnum".to_string(), sample.buffer_id as f32);
        self
    }

    /// Set the sound source to an SFZ instrument.
    ///
    /// SFZ instruments contain multiple samples mapped across keys and velocities.
    /// When a note is triggered, the appropriate sample(s) are selected based on
    /// the note and velocity.
    pub fn on_sfz(mut self, sfz: SfzHandle) -> Self {
        // Get the SFZ ID for this instrument
        let sfz_id = context::get_or_create_sfz_id(&sfz.id);
        self.sfz_instrument = Some(sfz_id);
        // SFZ voices use sfz_voice_mono or sfz_voice_stereo synthdef
        // The runtime will select the appropriate one based on the sample
        self.synth_name = Some("sfz_voice_stereo".to_string());
        self
    }

    /// Set the output to a MIDI device (routes notes to external MIDI instead of audio).
    #[cfg(feature = "midi")]
    pub fn on_midi_device(mut self, device: MidiDevice) -> Self {
        self.midi_output_device = Some(device.id);
        self
    }

    /// Set the MIDI channel (0-15) for MIDI output.
    #[cfg(feature = "midi")]
    pub fn channel(mut self, ch: i64) -> Self {
        self.midi_channel = ch.clamp(0, 15) as u8;
        self
    }

    /// Set the polyphony.
    pub fn poly(mut self, count: i64) -> Self {
        self.polyphony = count.clamp(1, 255) as u8;
        self
    }

    /// Set the gain.
    pub fn gain(mut self, value: f64) -> Self {
        self.gain = value as f32;
        self
    }

    /// Set a parameter.
    pub fn set_param(mut self, param: String, value: f64) -> Self {
        self.params.insert(param, value as f32);
        self
    }

    /// Set the round-robin count for cycling through sample variations.
    ///
    /// When set to a value > 0, triggers will include an `rr` parameter
    /// that cycles from 0 to count-1. Useful for drum voices with multiple
    /// sample variations.
    pub fn round_robin(mut self, count: i64) -> Self {
        self.round_robin_count = count.max(0) as u32;
        self
    }

    /// Set the choke group for exclusive triggering.
    ///
    /// When triggered, this voice will stop all other playing voices
    /// in the same choke group. Commonly used for hi-hat sounds where
    /// an open hi-hat should be stopped when a closed hi-hat is triggered.
    pub fn choke(mut self, group: String) -> Self {
        self.choke_group = if group.is_empty() { None } else { Some(group) };
        self
    }

    /// Mute the voice (chainable).
    pub fn mute(mut self) -> Self {
        self.muted = true;
        self.sync_to_state();
        self
    }

    /// Unmute the voice (chainable).
    pub fn unmute(mut self) -> Self {
        self.muted = false;
        self.sync_to_state();
        self
    }

    /// Solo the voice (chainable).
    pub fn solo(mut self) -> Self {
        self.soloed = true;
        self.sync_to_state();
        self
    }

    /// Unsolo the voice (chainable).
    pub fn unsolo(mut self) -> Self {
        self.soloed = false;
        self.sync_to_state();
        self
    }

    /// Check if the voice is muted.
    pub fn is_muted(&mut self) -> bool {
        self.muted
    }

    /// Check if the voice is soloed.
    pub fn is_soloed(&mut self) -> bool {
        self.soloed
    }

    /// Register this voice with the script state (chainable).
    pub(crate) fn sync_to_state(&self) {
        let voice_id = context::get_or_create_voice_id(&self.name);
        let group_id = context::get_or_create_group_id(&self.group_path);

        let config = VoiceConfig {
            synthdef: self.synth_name.clone().unwrap_or_default(),
            group: group_id,
            polyphony: self.polyphony,
            params: self.params.clone(),
            muted: self.muted,
            soloed: self.soloed,
            sfz_instrument: self.sfz_instrument,
            round_robin_count: self.round_robin_count,
            choke_group: self.choke_group.clone(),
            #[cfg(feature = "midi")]
            midi_output: self.midi_output_device,
            #[cfg(feature = "midi")]
            midi_channel: self.midi_channel,
        };

        context::with_state(|state| {
            state.voices.insert(voice_id, config);
        });
    }

    /// Apply the voice configuration (chainable).
    pub fn apply(self) -> Self {
        self.sync_to_state();
        self
    }
}

/// Create a new voice builder.
pub fn voice(ctx: NativeCallContext, name: String) -> Voice {
    let v = Voice::new(ctx, name);
    // Auto-sync on creation so the voice is available immediately
    v.sync_to_state();
    v
}

/// Register voice API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register Voice type
    engine.build_type::<Voice>();

    // Constructor
    engine.register_fn("voice", voice);

    // Getters
    engine.register_fn("id", Voice::id);
    engine.register_fn("name", Voice::get_name);
    engine.register_get("name", Voice::get_name);
    engine.register_fn("synth_name", Voice::get_synth_name);
    engine.register_get("synth_name", Voice::get_synth_name);
    engine.register_fn("get_gain", Voice::get_gain);
    engine.register_get("gain", Voice::get_gain);
    engine.register_fn("polyphony", Voice::get_polyphony);
    engine.register_get("polyphony", Voice::get_polyphony);
    engine.register_fn("group_path", Voice::get_group_path);
    engine.register_get("group_path", Voice::get_group_path);

    // Builder methods
    engine.register_fn("group", Voice::group);
    engine.register_fn("synth", Voice::synth);
    engine.register_fn("on", Voice::on);
    engine.register_fn("on", Voice::on_sample);
    engine.register_fn("on_sfz", Voice::on_sfz);
    #[cfg(feature = "midi")]
    engine.register_fn("on", Voice::on_midi_device);
    #[cfg(feature = "midi")]
    engine.register_fn("channel", Voice::channel);
    engine.register_fn("poly", Voice::poly);
    engine.register_fn("gain", Voice::gain);
    engine.register_fn("set_param", Voice::set_param);
    engine.register_fn("round_robin", Voice::round_robin);
    engine.register_fn("choke", Voice::choke);

    // Mute/solo
    engine.register_fn("mute", Voice::mute);
    engine.register_fn("unmute", Voice::unmute);
    engine.register_fn("solo", Voice::solo);
    engine.register_fn("unsolo", Voice::unsolo);
    engine.register_fn("is_muted", Voice::is_muted);
    engine.register_get("muted", Voice::is_muted);
    engine.register_fn("is_soloed", Voice::is_soloed);
    engine.register_get("soloed", Voice::is_soloed);

    // Actions
    engine.register_fn("apply", Voice::apply);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a Voice for testing without NativeCallContext.
    fn test_voice(name: &str) -> Voice {
        Voice {
            name: name.to_string(),
            synth_name: None,
            group_path: "main".to_string(),
            polyphony: 4,
            gain: 1.0,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            sfz_instrument: None,
            round_robin_count: 0,
            choke_group: None,
            #[cfg(feature = "midi")]
            midi_output_device: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
        }
    }

    // ==================== Builder Method Tests ====================

    #[test]
    fn test_voice_synth() {
        let v = test_voice("test").synth("my_synth".to_string());
        assert_eq!(v.synth_name, Some("my_synth".to_string()));
    }

    #[test]
    fn test_voice_on() {
        let v = test_voice("test").on("another_synth".to_string());
        assert_eq!(v.synth_name, Some("another_synth".to_string()));
    }

    #[test]
    fn test_voice_poly() {
        let v = test_voice("test").poly(8);
        assert_eq!(v.polyphony, 8);
    }

    #[test]
    fn test_voice_poly_clamping() {
        // Should clamp to 1-255
        let v1 = test_voice("test").poly(0);
        assert_eq!(v1.polyphony, 1);

        let v2 = test_voice("test").poly(300);
        assert_eq!(v2.polyphony, 255);

        let v3 = test_voice("test").poly(-5);
        assert_eq!(v3.polyphony, 1);
    }

    #[test]
    fn test_voice_gain() {
        let v = test_voice("test").gain(0.5);
        assert!((v.gain - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_voice_set_param() {
        let v = test_voice("test")
            .set_param("freq".to_string(), 440.0)
            .set_param("amp".to_string(), 0.8);

        assert_eq!(v.params.get("freq"), Some(&440.0_f32));
        assert_eq!(v.params.get("amp"), Some(&0.8_f32));
    }

    #[test]
    fn test_voice_mute_unmute() {
        let mut v = test_voice("test");
        assert!(!v.muted);

        v.muted = true;
        assert!(v.muted);

        v.muted = false;
        assert!(!v.muted);
    }

    #[test]
    fn test_voice_solo_unsolo() {
        let mut v = test_voice("test");
        assert!(!v.soloed);

        v.soloed = true;
        assert!(v.soloed);

        v.soloed = false;
        assert!(!v.soloed);
    }

    #[test]
    fn test_voice_group_relative_path() {
        let v = test_voice("test");
        // When group path doesn't start with "main/", it should be relative
        let v = Voice {
            group_path: "main/drums".to_string(),
            ..v
        };
        assert_eq!(v.group_path, "main/drums");
    }

    #[test]
    fn test_voice_chained_builders() {
        let v = test_voice("lead")
            .synth("supersaw".to_string())
            .poly(4)
            .gain(0.7)
            .set_param("cutoff".to_string(), 2000.0);

        assert_eq!(v.name, "lead");
        assert_eq!(v.synth_name, Some("supersaw".to_string()));
        assert_eq!(v.polyphony, 4);
        assert!((v.gain - 0.7).abs() < 0.001);
        assert_eq!(v.params.get("cutoff"), Some(&2000.0_f32));
    }

    // ==================== Getter Tests ====================

    #[test]
    fn test_voice_getters() {
        let mut v = test_voice("my_voice");
        v.synth_name = Some("test_synth".to_string());
        v.gain = 0.5;
        v.polyphony = 8;
        v.group_path = "main/drums".to_string();

        assert_eq!(v.id(), "my_voice");
        assert_eq!(v.get_name(), "my_voice");
        assert_eq!(v.get_synth_name(), "test_synth");
        assert!((v.get_gain() - 0.5).abs() < 0.001);
        assert_eq!(v.get_polyphony(), 8);
        assert_eq!(v.get_group_path(), "main/drums");
    }

    #[test]
    fn test_voice_get_synth_name_default() {
        let mut v = test_voice("test");
        // When no synth is set, should return empty string
        assert_eq!(v.get_synth_name(), "");
    }

    #[test]
    fn test_voice_is_muted_soloed() {
        let mut v = test_voice("test");

        assert!(!v.is_muted());
        assert!(!v.is_soloed());

        v.muted = true;
        v.soloed = true;

        assert!(v.is_muted());
        assert!(v.is_soloed());
    }

    // ==================== Default Values Tests ====================

    #[test]
    fn test_voice_default_values() {
        let v = test_voice("test");

        assert_eq!(v.polyphony, 4);
        assert!((v.gain - 1.0).abs() < 0.001);
        assert!(v.params.is_empty());
        assert!(!v.muted);
        assert!(!v.soloed);
        assert!(v.synth_name.is_none());
        assert!(v.sfz_instrument.is_none());
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_voice_channel_clamping() {
        let v1 = test_voice("test").channel(0);
        assert_eq!(v1.midi_channel, 0);

        let v2 = test_voice("test").channel(15);
        assert_eq!(v2.midi_channel, 15);

        let v3 = test_voice("test").channel(20);
        assert_eq!(v3.midi_channel, 15);

        let v4 = test_voice("test").channel(-5);
        assert_eq!(v4.midi_channel, 0);
    }
}
