//! Voices trait for sound-producing units.
//!
//! Voices are the primary way to produce sound. They wrap a synthdef
//! and can be triggered by patterns, melodies, or direct MIDI input.

#[cfg(feature = "midi")]
use crate::types::MidiDeviceId;
use crate::types::{GroupId, ModulatorId, ParamMap, SfzId, VoiceId};
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Configuration for creating a voice.
#[derive(Clone, Debug, PartialEq)]
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

    /// Round-robin count for cycling through sample variations.
    ///
    /// Set to 0 to disable round-robin (default). When > 0, a `rr` parameter
    /// is passed to the synthdef with values cycling from 0 to round_robin_count-1.
    /// Useful for drum voices with multiple sample variations.
    pub round_robin_count: u32,

    /// Choke group name for exclusive triggering.
    ///
    /// When set, triggering this voice will stop all other currently playing
    /// voices in the same choke group. This is commonly used for hi-hat sounds
    /// where an open hi-hat should be stopped when a closed hi-hat is triggered.
    pub choke_group: Option<String>,

    /// Modulation mappings (param_name -> modulator_id).
    ///
    /// When set, the specified parameters will be modulated by control buses
    /// written to by the corresponding modulators (LFOs, envelopes, etc.).
    pub modulations: HashMap<String, ModulatorId>,

    /// MIDI output device (if routing to external MIDI).
    #[cfg(feature = "midi")]
    pub midi_output: Option<MidiDeviceId>,

    /// MIDI channel for output (0-15).
    #[cfg(feature = "midi")]
    pub midi_channel: u8,

    /// Parameter to MIDI CC mapping.
    ///
    /// Maps parameter names to MIDI CC numbers (0-127).
    /// When set_param is called on a MIDI voice, the value is sent as CC.
    /// Also used for modulator output when the voice has modulations.
    #[cfg(feature = "midi")]
    pub param_cc_map: HashMap<String, u8>,
}

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
            round_robin_count: 0,
            choke_group: None,
            modulations: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_output: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
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

    /// Add a modulation mapping from a parameter to a modulator.
    ///
    /// The specified parameter will read its value from the modulator's
    /// control bus output instead of a static value.
    pub fn with_modulation(mut self, param: impl Into<String>, modulator: ModulatorId) -> Self {
        self.modulations.insert(param.into(), modulator);
        self
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
