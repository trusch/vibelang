//! Script state types for live reloading.
//!
//! This module defines the state extracted from a `.vibe` script,
//! without runtime-specific IDs (node IDs, buffer IDs, etc.).

use crate::traits::{MelodyConfig, ModulatorConfig, PatternConfig, SampleConfig, SequenceConfig, SfzConfig, VoiceConfig};
#[cfg(not(target_arch = "wasm32"))]
use crate::traits::RecordingConfig;
#[cfg(feature = "midi")]
use crate::traits::FadeTarget;
use crate::types::{
    EffectId, GroupId, MelodyId, ModulatorId, ParamMap, PatternId, SampleId, SequenceId, SfzId, TimeSignature, VoiceId,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::types::RecordingId;
#[cfg(feature = "midi")]
use crate::types::MidiDeviceId;
use std::collections::{HashMap, HashSet};

/// Configuration for a group (from script, no runtime IDs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupConfig {
    /// Group name.
    pub name: String,

    /// Parent group (None for root level).
    pub parent: Option<GroupId>,

    /// Initial parameter values.
    pub params: ParamMap,

    /// Effects in this group.
    pub effects: Vec<EffectId>,

    /// Whether this group is muted.
    pub muted: bool,

    /// Whether this group is soloed.
    pub soloed: bool,
}

/// Configuration for an effect (from script, no runtime IDs).
#[derive(Clone, Debug, PartialEq)]
pub struct EffectConfig {
    /// Group this effect belongs to.
    pub group: GroupId,

    /// SynthDef name.
    pub synthdef: String,

    /// Initial parameter values.
    pub params: ParamMap,
}

/// Configuration for routing a MIDI keyboard to a voice.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiKeyboardRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Voice to trigger.
    pub voice: VoiceId,
}

/// Configuration for routing a MIDI CC to a parameter.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiCcRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// CC number (0-127).
    pub cc: u8,

    /// Target to control.
    pub target: FadeTarget,

    /// Parameter name.
    pub param: String,

    /// Min value when CC is 0.
    pub min_value: f32,

    /// Max value when CC is 127.
    pub max_value: f32,
}

/// Configuration for a MIDI event callback.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiCallbackConfig {
    /// Unique callback ID for matching with FnPtr storage.
    pub callback_id: u64,

    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// Callback type: "note", "cc", "cc:N" (where N is CC number), "clock", "all".
    pub callback_type: String,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,
}

/// A MIDI message to be sent to an output device.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub enum MidiOutputMessage {
    /// Note on message.
    NoteOn {
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
    },
    /// Note off message.
    NoteOff {
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
    },
    /// Control change message.
    ControlChange {
        device_id: MidiDeviceId,
        channel: u8,
        cc: u8,
        value: u8,
    },
    /// Program change message.
    ProgramChange {
        device_id: MidiDeviceId,
        channel: u8,
        program: u8,
    },
}

/// Advanced MIDI keyboard route with range, transpose, and velocity curves.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancedMidiKeyboardRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Minimum note (inclusive).
    pub note_min: u8,

    /// Maximum note (inclusive).
    pub note_max: u8,

    /// Transpose in semitones.
    pub transpose: i8,

    /// Velocity curve name.
    pub velocity_curve: String,

    /// Voice to trigger.
    pub voice: VoiceId,
}

/// Advanced MIDI note route (for drums/pads).
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancedMidiNoteRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// Source note number.
    pub source_note: u8,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Choke group name (notes in same group stop each other).
    pub choke_group: Option<String>,

    /// Velocity parameter name (if velocity mapping is enabled).
    pub velocity_param: Option<String>,

    /// Velocity minimum value.
    pub velocity_min: Option<f32>,

    /// Velocity maximum value.
    pub velocity_max: Option<f32>,

    /// Voice to trigger.
    pub voice: VoiceId,
}

/// Advanced MIDI CC route with curves.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancedMidiCcRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// CC number (0-127).
    pub cc: u8,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Curve type name.
    pub curve: String,

    /// Voice to control.
    pub voice: VoiceId,

    /// Parameter name.
    pub param: String,

    /// Min value when CC is 0.
    pub min: f32,

    /// Max value when CC is 127.
    pub max: f32,
}

/// State extracted from a `.vibe` script.
///
/// This represents the desired state without runtime-specific IDs.
/// It's used for diffing against the current runtime state to determine
/// what needs to be created, deleted, or updated.
#[derive(Clone, Debug, Default)]
pub struct ScriptState {
    /// Tempo in BPM.
    pub tempo: f64,

    /// Time signature.
    pub time_sig: TimeSignature,

    /// Quantization in beats (0 = no quantization).
    pub quantization: f64,

    /// Groups defined in the script.
    pub groups: HashMap<GroupId, GroupConfig>,

    /// Voices defined in the script.
    pub voices: HashMap<VoiceId, VoiceConfig>,

    /// Patterns defined in the script.
    pub patterns: HashMap<PatternId, PatternConfig>,

    /// Melodies defined in the script.
    pub melodies: HashMap<MelodyId, MelodyConfig>,

    /// Sequences defined in the script.
    pub sequences: HashMap<SequenceId, SequenceConfig>,

    /// Effects defined in the script.
    pub effects: HashMap<EffectId, EffectConfig>,

    /// Samples to load.
    pub samples: HashMap<SampleId, SampleConfig>,

    /// SFZ instruments to load.
    pub sfz_instruments: HashMap<SfzId, SfzConfig>,

    /// Modulators defined in the script.
    pub modulators: HashMap<ModulatorId, ModulatorConfig>,

    /// Recordings to start (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub recordings: HashMap<RecordingId, RecordingConfig>,

    /// Patterns that should be playing.
    pub playing_patterns: HashSet<PatternId>,

    /// Melodies that should be playing.
    pub playing_melodies: HashSet<MelodyId>,

    /// Sequences that should be playing.
    pub playing_sequences: HashSet<SequenceId>,

    /// MIDI keyboard routes.
    #[cfg(feature = "midi")]
    pub midi_keyboard_routes: Vec<MidiKeyboardRoute>,

    /// MIDI CC routes.
    #[cfg(feature = "midi")]
    pub midi_cc_routes: Vec<MidiCcRoute>,

    /// Advanced MIDI keyboard routes with range, transpose, and velocity curves.
    #[cfg(feature = "midi")]
    pub advanced_keyboard_routes: Vec<AdvancedMidiKeyboardRoute>,

    /// Advanced MIDI note routes for drums/pads.
    #[cfg(feature = "midi")]
    pub advanced_note_routes: Vec<AdvancedMidiNoteRoute>,

    /// Advanced MIDI CC routes with curves.
    #[cfg(feature = "midi")]
    pub advanced_cc_routes: Vec<AdvancedMidiCcRoute>,

    /// MIDI devices to open for input.
    #[cfg(feature = "midi")]
    pub midi_inputs: HashSet<MidiDeviceId>,

    /// MIDI devices to open for output.
    #[cfg(feature = "midi")]
    pub midi_outputs: HashSet<MidiDeviceId>,

    /// Pending MIDI output messages to send immediately.
    #[cfg(feature = "midi")]
    pub midi_output_messages: Vec<MidiOutputMessage>,

    /// MIDI event callbacks to register.
    #[cfg(feature = "midi")]
    pub midi_callbacks: Vec<MidiCallbackConfig>,

    /// MIDI recording requests.
    #[cfg(feature = "midi")]
    pub midi_recording_requests: Vec<MidiRecordingRequest>,

    /// MIDI clock output configurations.
    #[cfg(feature = "midi")]
    pub midi_clock_outputs: Vec<MidiClockOutputRequest>,
}

/// Configuration for a MIDI recording request.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiRecordingRequest {
    /// Device ID to record from.
    pub device_id: MidiDeviceId,

    /// Optional channel filter.
    pub channel: Option<u8>,

    /// Start (true) or stop (false) recording.
    pub start: bool,
}

/// Configuration for MIDI clock output.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiClockOutputRequest {
    /// Device ID to send clock to.
    pub device_id: MidiDeviceId,

    /// Enable (true) or disable (false) clock output.
    pub enabled: bool,
}

impl ScriptState {
    /// Create a new empty script state with default tempo.
    pub fn new() -> Self {
        Self {
            tempo: 120.0,
            time_sig: TimeSignature::default(),
            ..Default::default()
        }
    }

    /// Builder method to set tempo.
    pub fn with_tempo(mut self, bpm: f64) -> Self {
        self.tempo = bpm;
        self
    }

    /// Builder method to set time signature.
    pub fn with_time_signature(mut self, time_sig: TimeSignature) -> Self {
        self.time_sig = time_sig;
        self
    }

    /// Add a group.
    pub fn add_group(&mut self, id: GroupId, config: GroupConfig) {
        self.groups.insert(id, config);
    }

    /// Add a voice.
    pub fn add_voice(&mut self, id: VoiceId, config: VoiceConfig) {
        self.voices.insert(id, config);
    }

    /// Add a pattern.
    pub fn add_pattern(&mut self, id: PatternId, config: PatternConfig) {
        self.patterns.insert(id, config);
    }

    /// Add a melody.
    pub fn add_melody(&mut self, id: MelodyId, config: MelodyConfig) {
        self.melodies.insert(id, config);
    }

    /// Add a sequence.
    pub fn add_sequence(&mut self, id: SequenceId, config: SequenceConfig) {
        self.sequences.insert(id, config);
    }

    /// Add an effect.
    pub fn add_effect(&mut self, id: EffectId, config: EffectConfig) {
        self.effects.insert(id, config);
    }

    /// Add a sample.
    pub fn add_sample(&mut self, id: SampleId, config: SampleConfig) {
        self.samples.insert(id, config);
    }

    /// Add an SFZ instrument.
    pub fn add_sfz(&mut self, id: SfzId, config: SfzConfig) {
        self.sfz_instruments.insert(id, config);
    }

    /// Add a modulator.
    pub fn add_modulator(&mut self, id: ModulatorId, config: ModulatorConfig) {
        self.modulators.insert(id, config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_state_new() {
        let state = ScriptState::new();
        assert_eq!(state.tempo, 120.0);
        assert!(state.groups.is_empty());
        assert!(state.voices.is_empty());
    }

    #[test]
    fn test_script_state_builder() {
        let state = ScriptState::new()
            .with_tempo(140.0)
            .with_time_signature(TimeSignature::new(3, 4));

        assert_eq!(state.tempo, 140.0);
        assert_eq!(state.time_sig.numerator, 3);
    }

    #[test]
    fn test_add_group() {
        let mut state = ScriptState::new();
        state.add_group(
            GroupId::new(1),
            GroupConfig {
                name: "test".to_string(),
                parent: None,
                params: ParamMap::new(),
                effects: Vec::new(),
                muted: false,
                soloed: false,
            },
        );

        assert!(state.groups.contains_key(&GroupId::new(1)));
    }

    #[test]
    fn test_add_voice() {
        let mut state = ScriptState::new();
        state.add_voice(
            VoiceId::new(1),
            VoiceConfig {
                name: "test".to_string(),
                synthdef: "sine".to_string(),
                group: GroupId::new(1),
                polyphony: 8,
                params: ParamMap::new(),
                muted: false,
                soloed: false,
                sfz_instrument: None,
                choke_group: None,
                round_robin_count: 0,
                modulations: std::collections::HashMap::new(),
                #[cfg(feature = "midi")]
                midi_output: None,
                #[cfg(feature = "midi")]
                midi_channel: 0,
                #[cfg(feature = "midi")]
                param_cc_map: std::collections::HashMap::new(),
            },
        );

        assert!(state.voices.contains_key(&VoiceId::new(1)));
    }

    #[test]
    fn test_group_config_default() {
        let config = GroupConfig::default();
        assert!(config.name.is_empty());
        assert!(config.parent.is_none());
        assert!(config.params.is_empty());
        assert!(config.effects.is_empty());
    }
}
