//! Voices trait for sound-producing units.
//!
//! Voices are the primary way to produce sound. They wrap a synthdef
//! and can be triggered by patterns, melodies, or direct MIDI input.

#[cfg(feature = "midi")]
use crate::types::MidiDeviceId;
use crate::types::{GroupId, ParamMap, SampleId, SfzId, VoiceId};
use crate::Result;
use async_trait::async_trait;
#[cfg(feature = "midi")]
use std::collections::HashMap;

/// Tolerance for floating-point comparisons in voice data.
/// This prevents false "updates" during reload due to float precision issues.
const FLOAT_TOLERANCE: f32 = 1e-6;

/// Compare two ParamMaps with tolerance for f32 values.
fn params_equal(a: &ParamMap, b: &ParamMap) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (key, &value) in a {
        match b.get(key) {
            Some(&other_value) if (value - other_value).abs() < FLOAT_TOLERANCE => {}
            _ => return false,
        }
    }
    true
}

/// Configuration for creating a voice.
#[derive(Clone, Debug)]
pub struct VoiceConfig {
    /// Voice name (for display in TUI/API).
    pub name: String,

    /// Name of the synthdef to use.
    pub synthdef: String,

    /// Group this voice belongs to.
    pub group: GroupId,

    /// Maximum polyphony (number of simultaneous notes).
    pub polyphony: u8,

    /// Default parameter values.
    pub params: ParamMap,

    /// Whether this voice is muted.
    pub muted: bool,

    /// Whether this voice is soloed.
    pub soloed: bool,

    /// SFZ instrument ID (if this voice uses an SFZ instrument).
    ///
    /// When set, the voice will use the SFZ instrument for sample selection
    /// based on note and velocity, rather than a single synthdef.
    pub sfz_instrument: Option<SfzId>,

    /// Sample ID (if this voice uses a sample).
    ///
    /// When set, the voice will use sample parameters (offset, length, envelope)
    /// from the SampleConfig, converted to synth params at trigger time.
    pub sample_id: Option<SampleId>,

    /// Round-robin count for cycling through sample variations.
    ///
    /// Set to 0 to disable round-robin (default). When > 0, a `rr` parameter
    /// is passed to the synthdef with values cycling from 0 to round_robin_count-1.
    /// Useful for drum voices with multiple sample variations.
    pub round_robin_count: u32,

    /// Trigger mode: "gate" (default) = release on note-off, "one_shot" = ignore note-off.
    pub trigger_mode: String,

    /// Choke group name for exclusive triggering.
    ///
    /// When set, triggering this voice will stop all other currently playing
    /// voices in the same choke group. This is commonly used for hi-hat sounds
    /// where an open hi-hat should be stopped when a closed hi-hat is triggered.
    pub choke_group: Option<String>,

    /// Explicit "modulation source only" flag.
    ///
    /// When true the runtime suppresses the count-based default audio mix
    /// for this voice unconditionally — even when an explicit
    /// `RouteDest::{Group,Main}` user route is present and even when the
    /// voice has no outgoing param routes. The author's intent is "this
    /// voice is wired to other voices' params; do not also dump it into
    /// the surrounding group bus." Explicit user routes still apply, so
    /// `voice("lfo").modulator_only(); voice("lfo").to(group("rec"))` will
    /// mix into `rec` only and skip the implicit default.
    ///
    /// Set via the Rhai `modulator_only()` chain method on
    /// [`crate::api::voice::Voice`](https://docs.rs/) and translated into
    /// [`crate::state::VoiceRole::ModulatorOnly`] during reload diffing.
    pub modulator_only: bool,

    /// MIDI output device (if routing to external MIDI).
    #[cfg(feature = "midi")]
    pub midi_output: Option<MidiDeviceId>,

    /// MIDI channel for output (0-15).
    #[cfg(feature = "midi")]
    pub midi_channel: u8,

    /// Legato (portamento) mode for `poly(1)` MIDI-output voices.
    ///
    /// When `false` (default) a note-steal on a mono MIDI-output voice sends
    /// `NoteOff(old)` before `NoteOn(new)` so the destination synth retriggers
    /// its envelope. When `true` the `NoteOff(old)` is skipped, so glide /
    /// portamento synths slur between overlapping notes. No effect unless
    /// `polyphony == 1` and `midi_output` is set.
    pub mono_legato: bool,

    /// Parameter to MIDI CC mapping.
    ///
    /// Maps parameter names to MIDI CC numbers (0-127).
    /// When set_param is called on a MIDI voice, the value is sent as CC.
    /// Also used for modulator output when the voice has modulations.
    #[cfg(feature = "midi")]
    pub param_cc_map: HashMap<String, u8>,
}

impl PartialEq for VoiceConfig {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.synthdef == other.synthdef
            && self.group == other.group
            && self.polyphony == other.polyphony
            && params_equal(&self.params, &other.params)
            && self.muted == other.muted
            && self.soloed == other.soloed
            && self.sfz_instrument == other.sfz_instrument
            && self.sample_id == other.sample_id
            && self.round_robin_count == other.round_robin_count
            && self.trigger_mode == other.trigger_mode
            && self.choke_group == other.choke_group
            && self.modulator_only == other.modulator_only
            && self.mono_legato == other.mono_legato
            && {
                #[cfg(feature = "midi")]
                {
                    self.midi_output == other.midi_output
                        && self.midi_channel == other.midi_channel
                        && self.param_cc_map == other.param_cc_map
                }
                #[cfg(not(feature = "midi"))]
                true
            }
    }
}

impl Eq for VoiceConfig {}

impl VoiceConfig {
    /// Create a new voice configuration.
    pub fn new(name: impl Into<String>, synthdef: impl Into<String>, group: GroupId) -> Self {
        Self {
            name: name.into(),
            synthdef: synthdef.into(),
            group,
            polyphony: 8,
            params: ParamMap::new(),
            muted: false,
            soloed: false,
            sfz_instrument: None,
            sample_id: None,
            round_robin_count: 0,
            trigger_mode: "gate".to_string(),
            choke_group: None,
            modulator_only: false,
            #[cfg(feature = "midi")]
            midi_output: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
            mono_legato: false,
            #[cfg(feature = "midi")]
            param_cc_map: HashMap::new(),
        }
    }

    /// Set the SFZ instrument for this voice.
    pub fn with_sfz_instrument(mut self, sfz_id: SfzId) -> Self {
        self.sfz_instrument = Some(sfz_id);
        self
    }

    /// Set MIDI output device.
    #[cfg(feature = "midi")]
    pub fn with_midi_output(mut self, device: MidiDeviceId, channel: u8) -> Self {
        self.midi_output = Some(device);
        self.midi_channel = channel.min(15);
        self
    }

    /// Map a parameter name to a MIDI CC number.
    ///
    /// When set_param is called on a MIDI voice, the value is sent as CC.
    /// Also used for modulator output when the voice has modulations.
    ///
    /// # Arguments
    ///
    /// * `param` - Parameter name (e.g., "cutoff", "resonance")
    /// * `cc` - MIDI CC number (0-127)
    #[cfg(feature = "midi")]
    pub fn with_param_cc(mut self, param: impl Into<String>, cc: u8) -> Self {
        self.param_cc_map.insert(param.into(), cc.min(127));
        self
    }

    /// Set the polyphony.
    pub fn with_polyphony(mut self, polyphony: u8) -> Self {
        self.polyphony = polyphony;
        self
    }

    /// Set legato (portamento) mode for `poly(1)` MIDI-output voices.
    ///
    /// See [`Self::mono_legato`].
    pub fn with_mono_legato(mut self, legato: bool) -> Self {
        self.mono_legato = legato;
        self
    }

    /// Set the round-robin count for sample variations.
    ///
    /// When set to a value > 0, triggers will include an `rr` parameter
    /// that cycles from 0 to count-1. This is useful for drum voices
    /// with multiple sample variations.
    pub fn with_round_robin(mut self, count: u32) -> Self {
        self.round_robin_count = count;
        self
    }

    /// Set the choke group for exclusive triggering.
    ///
    /// When triggered, this voice will stop all other playing voices
    /// in the same choke group. Commonly used for hi-hat sounds where
    /// an open hi-hat should be stopped when a closed hi-hat is triggered.
    pub fn with_choke_group(mut self, group: impl Into<String>) -> Self {
        self.choke_group = Some(group.into());
        self
    }

    /// Add a default parameter.
    pub fn with_param(mut self, name: impl Into<String>, value: f32) -> Self {
        self.params.insert(name.into(), value);
        self
    }

    /// Whether this configuration can produce sound (synthdef, SFZ, or MIDI output).
    ///
    /// Used by script layers for get-or-create semantics: a bare `voice("name")` must not
    /// overwrite a fully configured entry in [`ScriptState`](crate::reload::ScriptState).
    pub fn has_sound_source(&self) -> bool {
        #[cfg(feature = "midi")]
        let has_midi = self.midi_output.is_some();
        #[cfg(not(feature = "midi"))]
        let has_midi = false;
        !self.synthdef.is_empty() || self.sfz_instrument.is_some() || has_midi
    }
}

/// Voice management for sound production.
///
/// Voices wrap synthdefs and handle polyphony, providing a high-level
/// interface for triggering sounds from patterns and melodies.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait Voices: Send + Sync {
    /// Create a new voice.
    async fn create(&self, id: VoiceId, config: VoiceConfig) -> Result<()>;

    /// Delete a voice.
    async fn delete(&self, id: VoiceId) -> Result<()>;

    /// Delete a voice gracefully by setting gate=0 on active synths.
    ///
    /// Instead of immediately freeing synth nodes, this sets gate=0 on all
    /// active nodes to trigger their release envelopes. The synths will free
    /// themselves via doneAction=2 when the envelope completes.
    ///
    /// This is used during hot-reload to avoid abrupt audio cuts when
    /// voice configurations change.
    async fn graceful_delete(&self, id: VoiceId) -> Result<()>;

    /// Trigger a voice with parameters.
    ///
    /// Creates a new synth instance with the given parameters.
    async fn trigger(&self, id: VoiceId, params: &ParamMap) -> Result<()>;

    /// Trigger a voice with parameters, optionally scheduled at a future time.
    ///
    /// Used by the pattern lookahead scheduler: `at` is the precise
    /// monotonic deadline the event should sound at, giving sample-accurate
    /// timing on backends with real scheduling (scsynth OSC bundles).
    ///
    /// The default implementation ignores the timestamp and triggers
    /// immediately.
    async fn trigger_at(
        &self,
        id: VoiceId,
        params: &ParamMap,
        at: Option<crate::compat::Instant>,
    ) -> Result<()> {
        let _ = at;
        self.trigger(id, params).await
    }

    /// Stop all playing notes on a voice.
    async fn stop(&self, id: VoiceId) -> Result<()>;

    /// Send a note-on to a voice.
    ///
    /// # Arguments
    ///
    /// * `id` - Voice ID
    /// * `note` - MIDI note number (0-127)
    /// * `velocity` - Velocity (0.0-1.0)
    async fn note_on(&self, id: VoiceId, note: u8, velocity: f32) -> Result<()>;

    /// Send a note-off to a voice.
    async fn note_off(&self, id: VoiceId, note: u8) -> Result<()>;

    /// Mute or unmute a voice.
    ///
    /// When muted, the voice will not produce sound when triggered.
    async fn mute(&self, id: VoiceId, muted: bool) -> Result<()>;

    /// Set a parameter on all active synths of a voice.
    ///
    /// This updates the parameter on already-playing synths and stores
    /// the new value as the default for future triggers.
    async fn set_param(&self, id: VoiceId, param: &str, value: f32) -> Result<()>;
}

/// Voice management for sound production (WASM version).
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait Voices {
    /// Create a new voice.
    async fn create(&self, id: VoiceId, config: VoiceConfig) -> Result<()>;

    /// Delete a voice.
    async fn delete(&self, id: VoiceId) -> Result<()>;

    /// Delete a voice gracefully by setting gate=0 on active synths.
    async fn graceful_delete(&self, id: VoiceId) -> Result<()>;

    /// Trigger a voice with parameters.
    async fn trigger(&self, id: VoiceId, params: &ParamMap) -> Result<()>;

    /// Trigger a voice with parameters, optionally scheduled at a future time.
    ///
    /// Default ignores the timestamp; see the native trait variant.
    async fn trigger_at(
        &self,
        id: VoiceId,
        params: &ParamMap,
        at: Option<crate::compat::Instant>,
    ) -> Result<()> {
        let _ = at;
        self.trigger(id, params).await
    }

    /// Stop all playing notes on a voice.
    async fn stop(&self, id: VoiceId) -> Result<()>;

    /// Send a note-on to a voice.
    async fn note_on(&self, id: VoiceId, note: u8, velocity: f32) -> Result<()>;

    /// Send a note-off to a voice.
    async fn note_off(&self, id: VoiceId, note: u8) -> Result<()>;

    /// Mute or unmute a voice.
    async fn mute(&self, id: VoiceId, muted: bool) -> Result<()>;

    /// Set a parameter on all active synths of a voice.
    async fn set_param(&self, id: VoiceId, param: &str, value: f32) -> Result<()>;
}
