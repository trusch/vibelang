//! Voice API for Rhai scripts.
//!
//! Voices are the basic sound-producing units in VibeLang.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::collections::HashMap;
use vibelang_core::traits::VoiceConfig;
#[cfg(feature = "midi")]
use vibelang_core::types::MidiDeviceId;
use vibelang_core::types::{ModulatorId, SampleId};
#[cfg(not(target_arch = "wasm32"))]
use vibelang_core::types::SfzId;

#[cfg(feature = "midi")]
use super::midi::MidiDevice;
use super::modulator::Modulator;
use super::sample::SampleHandle;
#[cfg(not(target_arch = "wasm32"))]
use super::sfz::SfzHandle;
use crate::context;

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
    /// SFZ instrument ID (if this voice uses an SFZ instrument) - native only.
    #[cfg(not(target_arch = "wasm32"))]
    sfz_instrument: Option<SfzId>,
    /// Sample ID (if this voice uses a sample).
    sample_id: Option<SampleId>,
    /// Round-robin count for cycling through sample variations.
    round_robin_count: u32,
    /// Choke group name for exclusive triggering.
    choke_group: Option<String>,
    /// Modulation mappings (param_name -> modulator_id).
    modulations: HashMap<String, ModulatorId>,
    /// MIDI output device (if routing to external MIDI).
    #[cfg(feature = "midi")]
    midi_output_device: Option<MidiDeviceId>,
    /// MIDI output channel (0-15).
    #[cfg(feature = "midi")]
    midi_channel: u8,
    /// Parameter to MIDI CC mapping.
    #[cfg(feature = "midi")]
    param_cc_map: HashMap<String, u8>,
}

impl Voice {
    /// Create a new voice with the given name.
    pub fn new(_ctx: NativeCallContext, name: String) -> Self {
        Self {
            name: name,
            synth_name: None,
            group_path: context::current_group_path(),
            polyphony: 4,
            gain: 1.0,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            #[cfg(not(target_arch = "wasm32"))]
            sfz_instrument: None,
            sample_id: None,
            round_robin_count: 0,
            choke_group: None,
            modulations: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_output_device: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
            #[cfg(feature = "midi")]
            param_cc_map: HashMap::new(),
        }
    }

    /// Create an anonymous voice (name resolved at finalization).
    pub fn new_anon(_ctx: NativeCallContext) -> Self {
        Self {
            name: String::new(),
            synth_name: None,
            group_path: context::current_group_path(),
            polyphony: 4,
            gain: 1.0,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            #[cfg(not(target_arch = "wasm32"))]
            sfz_instrument: None,
            sample_id: None,
            round_robin_count: 0,
            choke_group: None,
            modulations: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_output_device: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
            #[cfg(feature = "midi")]
            param_cc_map: HashMap::new(),
        }
    }

    /// Resolve the name for an anonymous voice from its structural identity.
    ///
    /// Uses the synthdef name and group path to generate a stable, human-readable name.
    /// No-op if the voice already has a name.
    pub(crate) fn resolve_name(&mut self) {
        if !self.name.is_empty() {
            return;
        }
        let synth = self.synth_name.as_deref().unwrap_or("voice");
        let base = if self.group_path == "main" {
            format!("_{}", synth)
        } else {
            let group_suffix = self.group_path.strip_prefix("main/").unwrap_or(&self.group_path);
            format!("_{}/{}", group_suffix, synth)
        };
        self.name = context::resolve_auto_name(&base);
    }

    // === Getters ===

    /// Get the voice ID (name).
    pub fn id(&mut self) -> String {
        self.resolve_name();
        self.name.clone()
    }

    /// Get the voice name.
    pub fn get_name(&mut self) -> String {
        self.resolve_name();
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
        self.sync_to_state();
        self
    }

    /// Set the synth for this voice.
    pub fn synth(mut self, synth_name: String) -> Self {
        self.synth_name = Some(synth_name);
        self.sync_to_state();
        self
    }

    /// Set the sound source (synthdef name).
    pub fn on(mut self, source: String) -> Self {
        self.synth_name = Some(source);
        self.sync_to_state();
        self
    }

    /// Set the sound source to a sample.
    ///
    /// This extracts all sample configuration (envelope, playback, warp settings)
    /// and stores them in the voice params for use at trigger time.
    pub fn on_sample(mut self, sample: SampleHandle) -> Self {
        // Get sample ID and config from context
        if let Some(sample_id) = context::get_sample_id(&sample.id) {
            self.sample_id = Some(sample_id);

            context::with_state(|state| {
                if let Some(config) = state.samples.get(&sample_id) {
                    // Choose synthdef based on warp mode
                    self.synth_name = Some(if config.warp {
                        "warp_voice".to_string()
                    } else {
                        "sample_voice".to_string()
                    });

                    // Copy envelope params
                    self.params
                        .insert("attack".to_string(), config.attack as f32);
                    self.params
                        .insert("sustain".to_string(), config.sustain as f32);
                    self.params
                        .insert("release".to_string(), config.release as f32);
                    self.params.insert("amp".to_string(), config.amp as f32);

                    // Playback params
                    self.params.insert("rate".to_string(), config.rate as f32);
                    self.params.insert(
                        "loop".to_string(),
                        if config.loop_mode { 1.0 } else { 0.0 },
                    );

                    // Warp params (if warp mode)
                    if config.warp {
                        self.params
                            .insert("speed".to_string(), config.speed as f32);
                        self.params
                            .insert("pitch".to_string(), config.pitch as f32);
                        self.params
                            .insert("windowSize".to_string(), config.window_size as f32);
                        self.params
                            .insert("overlaps".to_string(), config.overlaps as f32);
                    }

                    // Store offset/length for conversion to frames at trigger time
                    self.params
                        .insert("_offset_secs".to_string(), config.offset as f32);
                    if let Some(len) = config.length {
                        self.params.insert("_length_secs".to_string(), len as f32);
                    }

                    // Store release time for fade-out calculation at trigger time
                    self.params
                        .insert("_release_secs".to_string(), config.release as f32);
                }
            });
        }

        // Always set bufnum
        self.params
            .insert("bufnum".to_string(), sample.buffer_id as f32);
        self.sync_to_state();
        self
    }

    /// Set the sound source to an SFZ instrument (native only).
    ///
    /// SFZ instruments contain multiple samples mapped across keys and velocities.
    /// When a note is triggered, the appropriate sample(s) are selected based on
    /// the note and velocity.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn on_sfz(mut self, sfz: SfzHandle) -> Self {
        // Get the SFZ ID for this instrument
        let sfz_id = context::get_or_create_sfz_id(&sfz.id);
        self.sfz_instrument = Some(sfz_id);
        // SFZ voices use sfz_voice_mono or sfz_voice_stereo synthdef
        // The runtime will select the appropriate one based on the sample
        self.synth_name = Some("sfz_voice_stereo".to_string());
        self.sync_to_state();
        self
    }

    /// Set the output to a MIDI device (routes notes to external MIDI instead of audio).
    ///
    /// The channel is taken from the MidiDevice. Use `midi_device("name").channel(n)`
    /// to configure the channel before passing to this method.
    #[cfg(feature = "midi")]
    pub fn on_midi_device(mut self, device: MidiDevice) -> Self {
        self.midi_output_device = Some(device.id);
        self.midi_channel = device.channel;
        // Mark this device for output opening during reload
        context::with_state(|state| {
            state.midi_outputs.insert(device.id);
        });
        self.sync_to_state();
        self
    }

    /// Set the MIDI channel (0-15) for MIDI output.
    ///
    /// Deprecated: prefer using `midi_device("name").channel(n)` instead.
    /// This method is kept for backward compatibility.
    #[cfg(feature = "midi")]
    pub fn channel(mut self, ch: i64) -> Self {
        self.midi_channel = ch.clamp(0, 15) as u8;
        self.sync_to_state();
        self
    }

    /// Map a parameter name to a MIDI CC number.
    ///
    /// When the parameter changes (via set_param or modulation), the value
    /// is sent as a MIDI CC message to the voice's MIDI output device.
    ///
    /// # Arguments
    ///
    /// * `param` - Parameter name (e.g., "cutoff", "resonance")
    /// * `cc` - MIDI CC number (0-127)
    #[cfg(feature = "midi")]
    pub fn cc_map(mut self, param: String, cc: i64) -> Self {
        self.param_cc_map.insert(param, cc.clamp(0, 127) as u8);
        self.sync_to_state();
        self
    }

    /// Set the polyphony.
    pub fn poly(mut self, count: i64) -> Self {
        self.polyphony = count.clamp(1, 255) as u8;
        self.sync_to_state();
        self
    }

    /// Set the gain.
    pub fn gain(mut self, value: f64) -> Self {
        self.gain = value as f32;
        self.sync_to_state();
        self
    }

    /// Set a parameter.
    pub fn set_param(mut self, param: String, value: f64) -> Self {
        self.params.insert(param, value as f32);
        self.sync_to_state();
        self
    }

    /// Set the round-robin count for cycling through sample variations.
    ///
    /// When set to a value > 0, triggers will include an `rr` parameter
    /// that cycles from 0 to count-1. Useful for drum voices with multiple
    /// sample variations.
    pub fn round_robin(mut self, count: i64) -> Self {
        self.round_robin_count = count.max(0) as u32;
        self.sync_to_state();
        self
    }

    /// Set the choke group for exclusive triggering.
    ///
    /// When triggered, this voice will stop all other playing voices
    /// in the same choke group. Commonly used for hi-hat sounds where
    /// an open hi-hat should be stopped when a closed hi-hat is triggered.
    pub fn choke(mut self, group: String) -> Self {
        self.choke_group = if group.is_empty() { None } else { Some(group) };
        self.sync_to_state();
        self
    }

    /// Connect a modulator to a voice parameter.
    ///
    /// The modulator's control bus output will be used as the value for
    /// the specified parameter. This enables dynamic modulation via LFOs,
    /// envelopes, envelope followers, etc.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let cutoff_lfo = modulator("lfo_sine")
    ///     .set_param("rate", 4.0)
    ///     .set_param("lo", 200)
    ///     .set_param("hi", 2000)
    ///     .apply();
    ///
    /// let bass = voice("bass")
    ///     .synth("moog_bass")
    ///     .modulate("cutoff", cutoff_lfo)
    ///     .apply();
    /// ```
    pub fn modulate(mut self, param: String, modulator: Modulator) -> Self {
        // Get or create the modulator ID from the modulator's name
        let modulator_id = context::get_or_create_modulator_id(&modulator.name);
        self.modulations.insert(param, modulator_id);
        self.sync_to_state();
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
    ///
    /// Skips registration for anonymous voices (empty name) that haven't been resolved yet.
    pub(crate) fn sync_to_state(&self) {
        if self.name.is_empty() {
            return; // Anonymous voice not yet resolved — defer registration
        }
        let voice_id = context::get_or_create_voice_id(&self.name);
        let group_id = context::get_or_create_group_id(&self.group_path);

        // Clone params and add gain as "amp" if not explicitly set
        let mut params = self.params.clone();
        if !params.contains_key("amp") {
            params.insert("amp".to_string(), self.gain);
        }
        tracing::debug!(
            "Voice sync_to_state: name={}, gain={}, amp in params={:?}",
            self.name,
            self.gain,
            params.get("amp")
        );

        let synthdef = self.synth_name.clone().unwrap_or_default();
        if synthdef.is_empty() {
            tracing::warn!(
                "Voice '{}': no synthdef set — use .synth(\"name\") or .on(\"sample\")",
                self.name
            );
        }

        let config = VoiceConfig {
            name: self.name.clone(),
            synthdef,
            group: group_id,
            polyphony: self.polyphony,
            params,
            muted: self.muted,
            soloed: self.soloed,
            #[cfg(not(target_arch = "wasm32"))]
            sfz_instrument: self.sfz_instrument,
            #[cfg(target_arch = "wasm32")]
            sfz_instrument: None,
            sample_id: self.sample_id,
            round_robin_count: self.round_robin_count,
            choke_group: self.choke_group.clone(),
            modulations: self.modulations.clone(),
            #[cfg(feature = "midi")]
            midi_output: self.midi_output_device,
            #[cfg(feature = "midi")]
            midi_channel: self.midi_channel,
            #[cfg(feature = "midi")]
            param_cc_map: self.param_cc_map.clone(),
        };

        context::with_state(|state| {
            state.voices.insert(voice_id, config);
        });
    }

    /// Apply the voice configuration (chainable).
    ///
    /// For anonymous voices, this resolves the name from the structural identity
    /// (synthdef + group path) before registering.
    pub fn apply(mut self) -> Self {
        self.resolve_name();
        self.sync_to_state();
        self
    }

    /// Run the voice continuously (for line-in, drones, etc.).
    ///
    /// This syncs the voice config and marks it for auto-triggering.
    /// The voice will be triggered automatically on startup and after
    /// reloads, producing continuous sound (e.g., for line-in monitors,
    /// drones, or other always-on sounds).
    pub fn run(mut self) -> Self {
        self.resolve_name();
        self.sync_to_state();
        context::mark_voice_for_running(&self.name);
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

/// Create an anonymous voice builder (name resolved from structural identity).
pub fn voice_anon(ctx: NativeCallContext) -> Voice {
    Voice::new_anon(ctx)
}

/// Register voice API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register Voice type
    engine.build_type::<Voice>();

    // Constructor (named and anonymous overloads)
    engine.register_fn("voice", voice);
    engine.register_fn("voice", voice_anon);

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
    #[cfg(not(target_arch = "wasm32"))]
    engine.register_fn("on", Voice::on_sfz);
    #[cfg(not(target_arch = "wasm32"))]
    engine.register_fn("on_sfz", Voice::on_sfz); // Backward compatibility alias
    #[cfg(feature = "midi")]
    engine.register_fn("on", Voice::on_midi_device);
    #[cfg(feature = "midi")]
    engine.register_fn("channel", Voice::channel);
    #[cfg(feature = "midi")]
    engine.register_fn("cc_map", Voice::cc_map);
    engine.register_fn("poly", Voice::poly);
    engine.register_fn("gain", Voice::gain);
    engine.register_fn("set_param", Voice::set_param);
    engine.register_fn("round_robin", Voice::round_robin);
    engine.register_fn("choke", Voice::choke);
    engine.register_fn("modulate", Voice::modulate);

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
    engine.register_fn("run", Voice::run);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context;

    /// Initialize a script context for testing, run the closure, then clean up.
    fn with_test_context<F: FnOnce()>(f: F) {
        context::init_context();
        f();
        context::clear_context();
    }

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
            #[cfg(not(target_arch = "wasm32"))]
            sfz_instrument: None,
            sample_id: None,
            round_robin_count: 0,
            choke_group: None,
            modulations: HashMap::new(),
            param_cc_map: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_output_device: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
        }
    }

    // ==================== Builder Method Tests ====================

    #[test]
    fn test_voice_synth() {
        with_test_context(|| {
            let v = test_voice("test").synth("my_synth".to_string());
            assert_eq!(v.synth_name, Some("my_synth".to_string()));
        });
    }

    #[test]
    fn test_voice_on() {
        with_test_context(|| {
            let v = test_voice("test").on("another_synth".to_string());
            assert_eq!(v.synth_name, Some("another_synth".to_string()));
        });
    }

    #[test]
    fn test_voice_poly() {
        with_test_context(|| {
            let v = test_voice("test").poly(8);
            assert_eq!(v.polyphony, 8);
        });
    }

    #[test]
    fn test_voice_poly_clamping() {
        with_test_context(|| {
            // Should clamp to 1-255
            let v1 = test_voice("test").poly(0);
            assert_eq!(v1.polyphony, 1);

            let v2 = test_voice("test").poly(300);
            assert_eq!(v2.polyphony, 255);

            let v3 = test_voice("test").poly(-5);
            assert_eq!(v3.polyphony, 1);
        });
    }

    #[test]
    fn test_voice_gain() {
        with_test_context(|| {
            let v = test_voice("test").gain(0.5);
            assert!((v.gain - 0.5).abs() < 0.001);
        });
    }

    #[test]
    fn test_voice_set_param() {
        with_test_context(|| {
            let v = test_voice("test")
                .set_param("freq".to_string(), 440.0)
                .set_param("amp".to_string(), 0.8);

            assert_eq!(v.params.get("freq"), Some(&440.0_f32));
            assert_eq!(v.params.get("amp"), Some(&0.8_f32));
        });
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
        with_test_context(|| {
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
        });
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
        #[cfg(not(target_arch = "wasm32"))]
        assert!(v.sfz_instrument.is_none());
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_voice_channel_clamping() {
        with_test_context(|| {
            let v1 = test_voice("test").channel(0);
            assert_eq!(v1.midi_channel, 0);

            let v2 = test_voice("test").channel(15);
            assert_eq!(v2.midi_channel, 15);

            let v3 = test_voice("test").channel(20);
            assert_eq!(v3.midi_channel, 15);

            let v4 = test_voice("test").channel(-5);
            assert_eq!(v4.midi_channel, 0);
        });
    }

    // ==================== Modulation Tests ====================
    // Note: These tests use direct manipulation of the modulations HashMap
    // because the modulate() method requires script context which isn't
    // available in unit tests.

    use vibelang_core::types::ModulatorId;

    /// Helper to test modulation storage directly without context
    fn test_voice_with_modulation(name: &str, param: &str, mod_id: u32) -> Voice {
        let mut v = test_voice(name);
        v.modulations
            .insert(param.to_string(), ModulatorId::new(mod_id));
        v
    }

    #[test]
    fn test_voice_modulate_single_param() {
        let v = test_voice_with_modulation("test", "cutoff", 1);

        assert_eq!(v.modulations.len(), 1);
        assert!(v.modulations.contains_key("cutoff"));
    }

    #[test]
    fn test_voice_modulate_multiple_params() {
        with_test_context(|| {
            let mut v = test_voice("test").synth("my_synth".to_string());
            v.modulations
                .insert("cutoff".to_string(), ModulatorId::new(1));
            v.modulations
                .insert("resonance".to_string(), ModulatorId::new(2));
            v.modulations.insert("pan".to_string(), ModulatorId::new(3));

            assert_eq!(v.modulations.len(), 3);
            assert!(v.modulations.contains_key("cutoff"));
            assert!(v.modulations.contains_key("resonance"));
            assert!(v.modulations.contains_key("pan"));
        });
    }

    #[test]
    fn test_voice_modulate_override() {
        // Test that modulating the same param twice uses the last value
        let mut v = test_voice("test");
        v.modulations
            .insert("cutoff".to_string(), ModulatorId::new(1)); // slow_lfo
        v.modulations
            .insert("cutoff".to_string(), ModulatorId::new(2)); // fast_lfo

        assert_eq!(v.modulations.len(), 1);
        assert_eq!(v.modulations.get("cutoff"), Some(&ModulatorId::new(2)));
    }

    #[test]
    fn test_voice_modulate_preserves_other_settings() {
        with_test_context(|| {
            let mut v = test_voice("test")
                .synth("my_synth".to_string())
                .poly(8)
                .gain(0.5)
                .set_param("freq".to_string(), 440.0);
            v.modulations
                .insert("cutoff".to_string(), ModulatorId::new(1));

            // Verify other settings are preserved
            assert_eq!(v.synth_name, Some("my_synth".to_string()));
            assert_eq!(v.polyphony, 8);
            assert!((v.gain - 0.5).abs() < 0.001);
            assert_eq!(v.params.get("freq"), Some(&440.0_f32));
            assert_eq!(v.modulations.len(), 1);
        });
    }

    #[test]
    fn test_voice_modulate_empty_initially() {
        let v = test_voice("test");
        assert!(v.modulations.is_empty());
    }

    #[test]
    fn test_voice_modulate_with_different_ids() {
        // Test that modulations can store different modulator IDs
        let mut v = test_voice("test");
        v.modulations
            .insert("param1".to_string(), ModulatorId::new(100));
        v.modulations
            .insert("param2".to_string(), ModulatorId::new(200));
        v.modulations
            .insert("param3".to_string(), ModulatorId::new(300));
        v.modulations
            .insert("param4".to_string(), ModulatorId::new(400));

        assert_eq!(v.modulations.len(), 4);
        assert_eq!(v.modulations.get("param1"), Some(&ModulatorId::new(100)));
        assert_eq!(v.modulations.get("param2"), Some(&ModulatorId::new(200)));
        assert_eq!(v.modulations.get("param3"), Some(&ModulatorId::new(300)));
        assert_eq!(v.modulations.get("param4"), Some(&ModulatorId::new(400)));
    }
}
