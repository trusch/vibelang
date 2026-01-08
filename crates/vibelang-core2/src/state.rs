//! Runtime state management.
//!
//! The [`State`] struct holds all runtime state, including:
//!
//! - Transport state (tempo, position, playing)
//! - Loaded resources (synthdefs, samples)
//! - Entities (groups, voices, patterns, melodies, sequences, effects)
//! - Active playback state (running synths, active fades)

use crate::traits::{
    FadeConfig, MelodyConfig, PatternConfig, RecordingInfo, SampleInfo, SequenceConfig,
    VoiceConfig,
};
use crate::types::{
    Beat, BufferId, BusId, EffectId, GroupId, MelodyId, NodeId, ParamMap, PatternId, RecordingId,
    SampleId, SequenceId, SfzId, TimeSignature, VoiceId,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

/// Internal state for a group.
#[derive(Clone, Debug)]
pub struct GroupState {
    /// Group ID.
    pub id: GroupId,

    /// Parent group (None for root).
    pub parent: Option<GroupId>,

    /// SuperCollider node ID for this group.
    pub node_id: NodeId,

    /// Audio bus ID for this group.
    ///
    /// Voices in this group output to this bus.
    /// The link synth reads from this bus and writes to the parent's bus (or bus 0 for main output).
    pub audio_bus: BusId,

    /// Node ID of the link synth that routes audio from this group's bus to the parent.
    ///
    /// Created by `FinalizeGroups` after all groups are set up.
    /// Uses the `system_link_audio` synthdef.
    pub link_synth_node_id: Option<NodeId>,

    /// Whether the group is muted.
    pub muted: bool,

    /// Whether the group is soloed.
    pub soloed: bool,

    /// Current parameter values (applied to link synth).
    pub params: ParamMap,
}

/// Internal state for a voice.
#[derive(Clone, Debug)]
pub struct VoiceState {
    /// Voice ID.
    pub id: VoiceId,

    /// Configuration.
    pub config: VoiceConfig,

    /// Active synth node IDs (for polyphony).
    pub active_nodes: Vec<NodeId>,

    /// Note -> node ID mapping.
    pub note_nodes: HashMap<u8, NodeId>,

    /// Round-robin counter for cycling through sample variations.
    ///
    /// Incremented on each trigger, wraps around based on the configured
    /// round-robin count. Useful for drum patterns with multiple sample
    /// variations (e.g., snare_1, snare_2, snare_3).
    pub round_robin_position: u32,
}

/// Internal state for a pattern.
#[derive(Clone, Debug)]
pub struct PatternState {
    /// Pattern ID.
    pub id: PatternId,

    /// Configuration.
    pub config: PatternConfig,

    /// Whether the pattern is playing.
    pub playing: bool,

    /// Current loop position.
    pub loop_position: Beat,
}

/// Internal state for a melody.
#[derive(Clone, Debug)]
pub struct MelodyState {
    /// Melody ID.
    pub id: MelodyId,

    /// Configuration.
    pub config: MelodyConfig,

    /// Whether the melody is playing.
    pub playing: bool,

    /// Current loop position.
    pub loop_position: Beat,
}

/// Internal state for a sequence.
#[derive(Clone, Debug)]
pub struct SequenceState {
    /// Sequence ID.
    pub id: SequenceId,

    /// Configuration.
    pub config: SequenceConfig,

    /// Whether the sequence is playing.
    pub playing: bool,

    /// Whether the sequence is paused.
    pub paused: bool,

    /// Whether to loop.
    pub looping: bool,

    /// Current position.
    pub position: Beat,
}

/// Internal state for an effect.
#[derive(Clone, Debug)]
pub struct EffectState {
    /// Effect ID.
    pub id: EffectId,

    /// Group this effect belongs to.
    pub group: GroupId,

    /// SynthDef name.
    pub synthdef: String,

    /// SuperCollider node ID.
    pub node_id: NodeId,

    /// Audio bus this effect processes (reads from and writes to).
    pub audio_bus: BusId,

    /// Current parameter values.
    pub params: ParamMap,
}

/// Internal state for a loaded SFZ instrument.
#[derive(Clone, Debug)]
pub struct SfzInstrumentState {
    /// SFZ instrument ID.
    pub id: SfzId,

    /// Path to the SFZ file.
    pub path: PathBuf,

    /// The loaded SFZ instrument data (regions, opcodes, etc.).
    /// Contains all sample mappings and playback parameters.
    pub regions: Vec<SfzRegionState>,

    /// Round-robin state for cycling through alternating samples.
    /// Key: (note, velocity_layer), Value: current position.
    pub round_robin_state: HashMap<(u8, u8), u32>,
}

/// State for a single SFZ region (sample mapping).
#[derive(Clone, Debug)]
pub struct SfzRegionState {
    /// Buffer ID for the sample.
    pub buffer_id: BufferId,

    /// Number of channels in the sample.
    pub num_channels: u8,

    /// Key range (low, high).
    pub key_range: (u8, u8),

    /// Velocity range (low, high).
    pub vel_range: (u8, u8),

    /// Pitch keycenter (the note the sample was recorded at).
    pub pitch_keycenter: u8,

    /// Sequence position for round-robin (0 = no round-robin).
    pub seq_position: u32,

    /// Sequence length for round-robin (0 = no round-robin).
    pub seq_length: u32,

    /// Amplitude envelope: attack time in seconds.
    pub ampeg_attack: f32,

    /// Amplitude envelope: decay time in seconds.
    pub ampeg_decay: f32,

    /// Amplitude envelope: sustain level (0.0-1.0).
    pub ampeg_sustain: f32,

    /// Amplitude envelope: release time in seconds.
    pub ampeg_release: f32,

    /// Volume offset in dB.
    pub volume: f32,

    /// Pan position (-1.0 to 1.0).
    pub pan: f32,

    /// Transpose in semitones.
    pub transpose: i8,

    /// Fine tune in cents.
    pub tune: f32,

    /// Loop mode.
    pub loop_enabled: bool,

    /// Loop start position in frames.
    pub loop_start: u32,

    /// Loop end position in frames.
    pub loop_end: u32,

    /// Sample start offset in frames.
    pub offset: u32,

    /// Filter cutoff frequency (if filter is enabled).
    pub cutoff: Option<f32>,

    /// Filter resonance (if filter is enabled).
    pub resonance: Option<f32>,
}

impl Default for SfzRegionState {
    fn default() -> Self {
        Self {
            buffer_id: BufferId::new(0),
            num_channels: 2,
            key_range: (0, 127),
            vel_range: (0, 127),
            pitch_keycenter: 60,
            seq_position: 0,
            seq_length: 0,
            ampeg_attack: 0.001,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.01,
            volume: 0.0,
            pan: 0.0,
            transpose: 0,
            tune: 0.0,
            loop_enabled: false,
            loop_start: 0,
            loop_end: 0,
            offset: 0,
            cutoff: None,
            resonance: None,
        }
    }
}

/// An active fade operation.
#[derive(Clone, Debug)]
pub struct ActiveFade {
    /// Fade configuration.
    pub config: FadeConfig,

    /// When the fade started.
    pub start_time: Instant,

    /// Starting value.
    pub start_value: f32,
}

impl ActiveFade {
    /// Calculate the current interpolated value.
    pub fn current_value(&self, now: Instant, tempo: f64) -> f32 {
        let elapsed_secs = now.duration_since(self.start_time).as_secs_f64();
        let duration_secs = self.config.duration.to_beats() * 60.0 / tempo;

        if elapsed_secs >= duration_secs {
            return self.config.to;
        }

        let t = (elapsed_secs / duration_secs) as f32;
        let t = t.clamp(0.0, 1.0);

        // Apply curve
        let t = match self.config.curve {
            crate::traits::FadeCurve::Linear => t,
            crate::traits::FadeCurve::Exponential => t * t,
            crate::traits::FadeCurve::Sine => {
                (1.0 - (t * std::f32::consts::PI).cos()) / 2.0
            }
        };

        self.start_value + (self.config.to - self.start_value) * t
    }

    /// Check if the fade is complete.
    pub fn is_complete(&self, now: Instant, tempo: f64) -> bool {
        let elapsed_secs = now.duration_since(self.start_time).as_secs_f64();
        let duration_secs = self.config.duration.to_beats() * 60.0 / tempo;
        elapsed_secs >= duration_secs
    }
}

/// Complete runtime state.
///
/// This struct holds all state needed for audio playback. It is wrapped
/// in an `Arc<RwLock<State>>` and shared between handlers.
#[derive(Clone, Debug)]
pub struct State {
    // =========================================================================
    // Transport
    // =========================================================================
    /// Tempo in BPM.
    pub tempo: f64,

    /// Time signature.
    pub time_sig: TimeSignature,

    /// Current beat position.
    pub current_beat: Beat,

    /// Whether playback is running.
    pub playing: bool,

    // =========================================================================
    // Resources
    // =========================================================================
    /// Loaded synthdef names.
    pub synthdefs: HashSet<String>,

    /// Loaded samples.
    pub samples: HashMap<SampleId, SampleInfo>,

    /// Loaded SFZ instruments.
    pub sfz_instruments: HashMap<SfzId, SfzInstrumentState>,

    // =========================================================================
    // Entities
    // =========================================================================
    /// Groups.
    pub groups: HashMap<GroupId, GroupState>,

    /// Voices.
    pub voices: HashMap<VoiceId, VoiceState>,

    /// Patterns.
    pub patterns: HashMap<PatternId, PatternState>,

    /// Melodies.
    pub melodies: HashMap<MelodyId, MelodyState>,

    /// Sequences.
    pub sequences: HashMap<SequenceId, SequenceState>,

    /// Effects.
    pub effects: HashMap<EffectId, EffectState>,

    // =========================================================================
    // Active Playback
    // =========================================================================
    /// Active fade operations.
    pub active_fades: Vec<ActiveFade>,

    /// Active audio recordings.
    pub recordings: HashMap<RecordingId, RecordingInfo>,

    // =========================================================================
    // ID Allocation
    // =========================================================================
    /// Next node ID to allocate.
    pub next_node_id: u32,

    /// Next buffer ID to allocate.
    pub next_buffer_id: u32,

    /// Next audio bus ID to allocate.
    ///
    /// Starts at 16 because buses 0-15 are typically reserved for hardware I/O.
    /// Bus 0 is the main stereo output.
    pub next_bus_id: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tempo: 120.0,
            time_sig: TimeSignature::default(),
            current_beat: Beat::ZERO,
            playing: false,
            synthdefs: HashSet::new(),
            samples: HashMap::new(),
            sfz_instruments: HashMap::new(),
            groups: HashMap::new(),
            voices: HashMap::new(),
            patterns: HashMap::new(),
            melodies: HashMap::new(),
            sequences: HashMap::new(),
            effects: HashMap::new(),
            active_fades: Vec::new(),
            recordings: HashMap::new(),
            next_node_id: 1000, // Reserve low IDs for system nodes
            next_buffer_id: 0,
            next_bus_id: 16, // Reserve buses 0-15 for hardware I/O
        }
    }
}

impl State {
    /// Create a new empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a new node ID.
    pub fn alloc_node_id(&mut self) -> NodeId {
        let id = NodeId::new(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    /// Allocate a new buffer ID.
    pub fn alloc_buffer_id(&mut self) -> crate::types::BufferId {
        let id = crate::types::BufferId::new(self.next_buffer_id);
        self.next_buffer_id += 1;
        id
    }

    /// Allocate a new audio bus ID.
    ///
    /// Each group gets its own bus for audio routing.
    pub fn alloc_bus_id(&mut self) -> BusId {
        let id = BusId::new(self.next_bus_id);
        self.next_bus_id += 1;
        id
    }

    /// Convert beats to seconds at current tempo.
    pub fn beats_to_secs(&self, beats: f64) -> f64 {
        beats * 60.0 / self.tempo
    }

    /// Convert seconds to beats at current tempo.
    pub fn secs_to_beats(&self, secs: f64) -> f64 {
        secs * self.tempo / 60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_default() {
        let state = State::default();
        assert_eq!(state.tempo, 120.0);
        assert!(!state.playing);
        assert_eq!(state.current_beat, Beat::ZERO);
    }

    #[test]
    fn test_alloc_node_id() {
        let mut state = State::new();
        let id1 = state.alloc_node_id();
        let id2 = state.alloc_node_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_beats_to_secs() {
        let state = State::new(); // 120 BPM
        assert!((state.beats_to_secs(2.0) - 1.0).abs() < 0.001);
    }
}
