//! State model types for VibeLang.
//!
//! These types represent the complete state of a VibeLang session,
//! including groups, voices, patterns, melodies, effects, and samples.

use crate::events::{BeatEvent, FadeTargetType, Pattern};
use crate::sequences::SequenceDefinition;
use crate::timing::TimeSignature;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Central state snapshot for a VibeLang session.
///
/// This is the single source of truth for all audio-related state.
/// It is owned by the state manager thread and accessed via
/// message passing and read-only snapshots.
#[derive(Clone, Debug)]
pub struct ScriptState {
    /// Monotonically increasing version for change detection.
    pub version: u64,
    /// Current tempo in BPM.
    pub tempo: f64,
    /// Quantization grid in beats.
    pub quantization_beats: f64,
    /// Current time signature.
    pub time_signature: TimeSignature,
    /// Whether the transport is running.
    pub transport_running: bool,
    /// Current beat position.
    pub current_beat: f64,
    /// Registered groups by path.
    pub groups: HashMap<String, GroupState>,
    /// Voice definitions by name.
    pub voices: HashMap<String, VoiceState>,
    /// Pattern definitions by name.
    pub patterns: HashMap<String, PatternState>,
    /// Melody definitions by name.
    pub melodies: HashMap<String, MelodyState>,
    /// Sequence definitions by name.
    pub sequences: HashMap<String, SequenceDefinition>,
    /// Fade definitions by name.
    pub fade_defs: HashMap<String, crate::sequences::FadeDefinition>,
    /// Loaded samples by ID.
    pub samples: HashMap<String, SampleInfo>,
    /// Loaded SFZ instruments by ID (placeholder type).
    pub sfz_instruments: HashMap<String, SfzInstrument>,
    /// Loaded VST instruments by ID.
    pub vst_instruments: HashMap<String, VstInstrumentInfo>,
    /// One-shot scheduled events.
    pub scheduled_events: Vec<ScheduledEvent>,
    /// Scheduled note-off events.
    pub scheduled_note_offs: Vec<ScheduledNoteOff>,
    /// Log of sequence runs.
    pub sequence_runs: Vec<SequenceRunLog>,
    /// Currently playing sequences.
    pub active_sequences: HashMap<String, ActiveSequence>,
    /// Currently playing synth nodes.
    pub active_synths: HashMap<i32, ActiveSynth>,
    /// Pending synth nodes (sent in timed bundle but not yet confirmed to exist on scsynth).
    /// Maps node ID to the Instant when the synth will be live on scsynth.
    /// These should NOT receive n_set messages until that time has passed.
    pub pending_nodes: HashMap<i32, std::time::Instant>,
    /// Active parameter fades.
    pub fades: Vec<ActiveFadeJob>,
    /// Next available synth node ID.
    pub next_synth_node_id: i32,
    /// Next available group node ID.
    pub next_group_node_id: i32,
    /// Next available buffer ID.
    pub next_buffer_id: i32,
    /// Next available audio bus.
    pub next_audio_bus: i32,
    /// Effects by ID.
    pub effects: HashMap<String, EffectState>,
    /// Reload generation counter.
    pub reload_generation: u64,
    /// Global scrub mute flag.
    pub scrub_muted: bool,
}

impl Default for ScriptState {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptState {
    /// Create a new state with default values.
    pub fn new() -> Self {
        Self {
            version: 0,
            tempo: 120.0,
            quantization_beats: 4.0,
            time_signature: TimeSignature::default(),
            transport_running: false,
            current_beat: 0.0,
            groups: HashMap::new(),
            voices: HashMap::new(),
            patterns: HashMap::new(),
            melodies: HashMap::new(),
            sequences: HashMap::new(),
            samples: HashMap::new(),
            sfz_instruments: HashMap::new(),
            vst_instruments: HashMap::new(),
            scheduled_events: Vec::new(),
            scheduled_note_offs: Vec::new(),
            sequence_runs: Vec::new(),
            active_sequences: HashMap::new(),
            active_synths: HashMap::new(),
            pending_nodes: HashMap::new(),
            fades: Vec::new(),
            fade_defs: HashMap::new(),
            next_synth_node_id: 2000,
            next_group_node_id: 1000,
            next_buffer_id: 100,
            next_audio_bus: 16,
            effects: HashMap::new(),
            reload_generation: 0,
            scrub_muted: false,
        }
    }

    /// Increment the version counter.
    pub fn bump_version(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// Allocate a new synth node ID.
    pub fn allocate_synth_node(&mut self) -> i32 {
        let id = self.next_synth_node_id;
        self.next_synth_node_id += 1;
        id
    }

    /// Allocate a new group node ID.
    pub fn allocate_group_node(&mut self) -> i32 {
        let id = self.next_group_node_id;
        self.next_group_node_id += 1;
        id
    }

    /// Allocate a new buffer ID.
    pub fn allocate_buffer_id(&mut self) -> i32 {
        let id = self.next_buffer_id;
        self.next_buffer_id += 1;
        id
    }

    /// Allocate a new audio bus.
    pub fn allocate_audio_bus(&mut self) -> i32 {
        let id = self.next_audio_bus;
        self.next_audio_bus += 1;
        id
    }
}

/// State for a group in the audio hierarchy.
#[derive(Clone, Debug)]
pub struct GroupState {
    /// Short name of the group.
    pub name: String,
    /// Full path (e.g., "main.drums.kick").
    pub path: String,
    /// Parent group path, if any.
    pub parent_path: Option<String>,
    /// SuperCollider node ID.
    pub node_id: Option<i32>,
    /// Allocated audio bus for this group.
    pub audio_bus: Option<i32>,
    /// Node ID of the link synth routing to parent.
    pub link_synth_node_id: Option<i32>,
    /// Group-level parameters.
    pub params: HashMap<String, f32>,
    /// Whether this group is muted.
    pub muted: bool,
    /// Whether this group is soloed.
    pub soloed: bool,
    /// Synth nodes belonging to this group.
    pub synth_node_ids: Vec<i32>,
    /// Reload generation.
    pub generation: u64,
}

impl GroupState {
    /// Create a new group state.
    pub fn new(name: String, path: String, parent_path: Option<String>) -> Self {
        Self {
            name,
            path,
            parent_path,
            node_id: None,
            audio_bus: None,
            link_synth_node_id: None,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            synth_node_ids: Vec::new(),
            generation: 0,
        }
    }
}

/// State for a voice definition.
#[derive(Clone, Debug)]
pub struct VoiceState {
    /// Voice name.
    pub name: String,
    /// SynthDef name to use.
    pub synth_name: Option<String>,
    /// Maximum polyphony.
    pub polyphony: i64,
    /// Gain multiplier.
    pub gain: f64,
    /// Group path this voice belongs to.
    pub group_path: String,
    /// Short group name.
    pub group_name: Option<String>,
    /// Explicit output bus override.
    pub output_bus: Option<i64>,
    /// Whether this voice is muted.
    pub muted: bool,
    /// Whether this voice is soloed.
    pub soloed: bool,
    /// Voice parameters.
    pub params: HashMap<String, f32>,
    /// SFZ instrument ID if using SFZ.
    pub sfz_instrument: Option<String>,
    /// Active notes for SFZ playback.
    pub active_notes: HashMap<u8, Vec<i32>>,
    /// Sustained notes.
    pub sustained_notes: HashSet<u8>,
    /// Round-robin state for SFZ.
    pub round_robin_state: RoundRobinState,
    /// VST instrument ID if using VST.
    pub vst_instrument: Option<String>,
    /// Reload generation.
    pub generation: u64,
    /// Whether this voice is running continuously (for line-in, drones, etc.).
    pub running: bool,
    /// Node ID of the running synth (if running).
    pub running_node_id: Option<i32>,
}

impl VoiceState {
    /// Create a new voice state.
    pub fn new(name: String, group_path: String) -> Self {
        Self {
            name,
            synth_name: None,
            polyphony: 1,
            gain: 1.0,
            group_path,
            group_name: None,
            output_bus: None,
            muted: false,
            soloed: false,
            params: HashMap::new(),
            sfz_instrument: None,
            active_notes: HashMap::new(),
            sustained_notes: HashSet::new(),
            round_robin_state: RoundRobinState::new(),
            vst_instrument: None,
            generation: 0,
            running: false,
            running_node_id: None,
        }
    }
}

/// Round-robin state for SFZ sample selection.
#[derive(Clone, Debug, Default)]
pub struct RoundRobinState {
    counters: HashMap<(u8, u8), usize>, // (note, velocity_layer) -> counter
}

impl RoundRobinState {
    /// Create a new round-robin state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get and increment the counter for a note/velocity combination.
    pub fn get_and_increment(&mut self, note: u8, velocity: u8) -> usize {
        let key = (note, velocity / 32); // Group velocities into layers
        let counter = self.counters.entry(key).or_insert(0);
        let value = *counter;
        *counter += 1;
        value
    }
}

/// Loop playback status.
#[derive(Clone, Debug)]
pub enum LoopStatus {
    /// Not currently playing.
    Stopped,
    /// Queued to start at the given beat.
    Queued { start_beat: f64 },
    /// Currently playing, started at the given beat.
    Playing { start_beat: f64 },
    /// Playing but queued to stop at the given beat.
    QueuedStop { start_beat: f64, stop_beat: f64 },
}

impl LoopStatus {
    /// Check if the loop is currently playing.
    pub fn is_playing(&self) -> bool {
        matches!(
            self,
            LoopStatus::Playing { .. } | LoopStatus::QueuedStop { .. }
        )
    }

    /// Get the start beat if playing or queued.
    pub fn start_beat(&self) -> Option<f64> {
        match self {
            LoopStatus::Stopped => None,
            LoopStatus::Queued { start_beat }
            | LoopStatus::Playing { start_beat }
            | LoopStatus::QueuedStop { start_beat, .. } => Some(*start_beat),
        }
    }
}

/// State for a pattern.
#[derive(Clone, Debug)]
pub struct PatternState {
    /// Pattern name.
    pub name: String,
    /// Group path.
    pub group_path: String,
    /// Voice name this pattern triggers.
    pub voice_name: Option<String>,
    /// The pattern data.
    pub loop_pattern: Option<Pattern>,
    /// Pattern parameters.
    pub params: HashMap<String, f32>,
    /// Current playback status.
    pub status: LoopStatus,
    /// Whether this pattern loops.
    pub is_looping: bool,
    /// Reload generation.
    pub generation: u64,
}

impl PatternState {
    /// Create a new pattern state.
    pub fn new(name: String, group_path: String, voice_name: Option<String>) -> Self {
        Self {
            name,
            group_path,
            voice_name,
            loop_pattern: None,
            params: HashMap::new(),
            status: LoopStatus::Stopped,
            is_looping: true,
            generation: 0,
        }
    }
}

/// State for a melody.
#[derive(Clone, Debug)]
pub struct MelodyState {
    /// Melody name.
    pub name: String,
    /// Group path.
    pub group_path: String,
    /// Voice name this melody triggers.
    pub voice_name: Option<String>,
    /// The pattern data.
    pub loop_pattern: Option<Pattern>,
    /// Melody parameters.
    pub params: HashMap<String, f32>,
    /// Current playback status.
    pub status: LoopStatus,
    /// Whether this melody loops.
    pub is_looping: bool,
    /// Reload generation.
    pub generation: u64,
}

impl MelodyState {
    /// Create a new melody state.
    pub fn new(name: String, group_path: String, voice_name: Option<String>) -> Self {
        Self {
            name,
            group_path,
            voice_name,
            loop_pattern: None,
            params: HashMap::new(),
            status: LoopStatus::Stopped,
            is_looping: true,
            generation: 0,
        }
    }
}

/// Metadata for an active synth node.
#[derive(Clone, Debug)]
pub struct ActiveSynth {
    /// SuperCollider node ID.
    pub node_id: i32,
    /// Group paths this synth belongs to.
    pub group_paths: Vec<String>,
    /// Voice names this synth is triggered by.
    pub voice_names: Vec<String>,
    /// Pattern names this synth is triggered by.
    pub pattern_names: Vec<String>,
    /// Melody names this synth is triggered by.
    pub melody_names: Vec<String>,
}

/// A one-shot scheduled event.
#[derive(Clone, Debug)]
pub struct ScheduledEvent {
    /// Beat when to execute.
    pub beat: f64,
    /// The event to execute.
    pub event: BeatEvent,
}

/// A scheduled note-off event.
#[derive(Clone, Debug)]
pub struct ScheduledNoteOff {
    /// Beat when to send note-off.
    pub beat: f64,
    /// Voice to send note-off to.
    pub voice_name: String,
    /// MIDI note number.
    pub note: u8,
    /// Specific node ID to release (None = all).
    pub node_id: Option<i32>,
}

/// Log entry for a sequence run.
#[derive(Clone, Debug)]
pub struct SequenceRunLog {
    /// Sequence name.
    pub name: String,
    /// Anchor beat.
    pub anchor_beat: f64,
    /// When the sequence started.
    pub started_at: std::time::SystemTime,
}

/// Runtime state for an active sequence.
#[derive(Clone, Debug)]
pub struct ActiveSequence {
    /// Beat when the sequence started.
    pub anchor_beat: f64,
    /// Whether the sequence is paused.
    pub paused: bool,
    /// Clips that have been triggered (for clip_once mode).
    /// Key is clip_id, value is the iteration number when triggered.
    pub triggered_clips: HashMap<String, u64>,
    /// The last loop iteration where we processed clips.
    /// Used to clear triggered_clips when entering a new iteration.
    pub last_iteration: u64,
}

/// An active parameter fade job.
#[derive(Clone, Debug)]
pub struct ActiveFadeJob {
    /// Target type.
    pub target_type: FadeTargetType,
    /// Target name.
    pub target_name: String,
    /// Parameter name.
    pub param_name: String,
    /// Starting value.
    pub start_value: f32,
    /// Target value.
    pub target_value: f32,
    /// When the fade started.
    pub start_time: Instant,
    /// Duration in seconds.
    pub duration_seconds: f64,
    /// Delay before fade starts.
    pub delay_seconds: f64,
    /// Whether the fade has completed.
    pub completed: bool,
    /// Last value sent (for deduplication).
    pub last_value: Option<f32>,
}

/// Information about a loaded sample.
#[derive(Clone, Debug)]
pub struct SampleInfo {
    /// Sample ID.
    pub id: String,
    /// File path.
    pub path: String,
    /// SuperCollider buffer ID.
    pub buffer_id: i32,
    /// Number of channels.
    pub num_channels: i32,
    /// Number of frames.
    pub num_frames: i32,
    /// Sample rate.
    pub sample_rate: f32,
    /// SynthDef name for playback.
    pub synthdef_name: String,
    /// Sample slices.
    pub slices: Vec<SampleSlice>,
}

/// A sample slice.
#[derive(Clone, Debug)]
pub struct SampleSlice {
    /// Slice index.
    pub index: usize,
    /// Start frame.
    pub start_frame: i32,
    /// End frame.
    pub end_frame: i32,
    /// SynthDef name for this slice.
    pub synthdef_name: String,
}

/// State for an effect instance.
#[derive(Clone, Debug)]
pub struct EffectState {
    /// Effect ID.
    pub id: String,
    /// SynthDef name.
    pub synthdef_name: String,
    /// Group path this effect is attached to.
    pub group_path: String,
    /// SuperCollider node ID.
    pub node_id: Option<i32>,
    /// Input bus.
    pub bus_in: i32,
    /// Output bus.
    pub bus_out: i32,
    /// Effect parameters.
    pub params: HashMap<String, f32>,
    /// Reload generation.
    pub generation: u64,
    /// Position in effect chain.
    pub position: usize,
    /// VST plugin key if using VST.
    pub vst_plugin: Option<String>,
}


/// Information about a loaded VST instrument.
#[derive(Clone, Debug)]
pub struct VstInstrumentInfo {
    /// User-assigned ID.
    pub id: String,
    /// Plugin name/path.
    pub plugin_key: String,
    /// Synth node ID when active.
    pub node_id: Option<i32>,
}

// Re-export the full SFZ instrument type from vibelang-sfz
pub use vibelang_sfz::SfzInstrument;

// ============================================================================
// Content Hashing for Reload Diffing
// ============================================================================

use std::hash::{Hash, Hasher};

/// Helper to hash a HashMap<String, f32> in a deterministic order.
fn hash_params<H: Hasher>(params: &HashMap<String, f32>, state: &mut H) {
    let mut keys: Vec<_> = params.keys().collect();
    keys.sort();
    for key in keys {
        key.hash(state);
        params[key].to_bits().hash(state);
    }
}

impl GroupState {
    /// Compute a content hash of this group's configuration.
    /// Excludes ephemeral state like node_id, link_synth_node_id.
    pub fn content_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.name.hash(&mut hasher);
        self.path.hash(&mut hasher);
        self.parent_path.hash(&mut hasher);
        hash_params(&self.params, &mut hasher);
        self.muted.hash(&mut hasher);
        self.soloed.hash(&mut hasher);
        hasher.finish()
    }
}

impl VoiceState {
    /// Compute a content hash of this voice's configuration.
    /// Excludes ephemeral state like active_notes, round_robin_state, running_node_id.
    pub fn content_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.name.hash(&mut hasher);
        self.synth_name.hash(&mut hasher);
        self.polyphony.hash(&mut hasher);
        self.gain.to_bits().hash(&mut hasher);
        self.group_path.hash(&mut hasher);
        self.group_name.hash(&mut hasher);
        self.output_bus.hash(&mut hasher);
        self.muted.hash(&mut hasher);
        self.soloed.hash(&mut hasher);
        hash_params(&self.params, &mut hasher);
        self.sfz_instrument.hash(&mut hasher);
        self.vst_instrument.hash(&mut hasher);
        self.running.hash(&mut hasher);
        hasher.finish()
    }
}

impl PatternState {
    /// Compute a content hash of this pattern's configuration.
    /// Excludes ephemeral state like status.
    pub fn content_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.name.hash(&mut hasher);
        self.group_path.hash(&mut hasher);
        self.voice_name.hash(&mut hasher);
        // Hash the pattern data
        if let Some(ref lp) = self.loop_pattern {
            lp.name.hash(&mut hasher);
            lp.loop_length_beats.to_bits().hash(&mut hasher);
            lp.phase_offset.to_bits().hash(&mut hasher);
            lp.events.len().hash(&mut hasher);
            for event in &lp.events {
                event.beat.to_bits().hash(&mut hasher);
                event.synth_def.hash(&mut hasher);
                for (k, v) in &event.controls {
                    k.hash(&mut hasher);
                    v.to_bits().hash(&mut hasher);
                }
            }
        }
        hash_params(&self.params, &mut hasher);
        self.is_looping.hash(&mut hasher);
        hasher.finish()
    }
}

impl MelodyState {
    /// Compute a content hash of this melody's configuration.
    /// Excludes ephemeral state like status.
    pub fn content_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.name.hash(&mut hasher);
        self.group_path.hash(&mut hasher);
        self.voice_name.hash(&mut hasher);
        // Hash the pattern data
        if let Some(ref lp) = self.loop_pattern {
            lp.name.hash(&mut hasher);
            lp.loop_length_beats.to_bits().hash(&mut hasher);
            lp.phase_offset.to_bits().hash(&mut hasher);
            lp.events.len().hash(&mut hasher);
            for event in &lp.events {
                event.beat.to_bits().hash(&mut hasher);
                event.synth_def.hash(&mut hasher);
                for (k, v) in &event.controls {
                    k.hash(&mut hasher);
                    v.to_bits().hash(&mut hasher);
                }
            }
        }
        hash_params(&self.params, &mut hasher);
        self.is_looping.hash(&mut hasher);
        hasher.finish()
    }
}

impl EffectState {
    /// Compute a content hash of this effect's configuration.
    /// Excludes ephemeral state like node_id, bus routing.
    pub fn content_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.id.hash(&mut hasher);
        self.synthdef_name.hash(&mut hasher);
        self.group_path.hash(&mut hasher);
        hash_params(&self.params, &mut hasher);
        self.position.hash(&mut hasher);
        self.vst_plugin.hash(&mut hasher);
        hasher.finish()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_state_defaults() {
        let state = ScriptState::new();
        assert!((state.tempo - 120.0).abs() < 0.001);
        assert!(!state.transport_running);
        assert_eq!(state.version, 0);
    }

    #[test]
    fn test_allocate_ids() {
        let mut state = ScriptState::new();
        let id1 = state.allocate_synth_node();
        let id2 = state.allocate_synth_node();
        assert_eq!(id2, id1 + 1);
    }

    #[test]
    fn test_loop_status() {
        let stopped = LoopStatus::Stopped;
        assert!(!stopped.is_playing());

        let playing = LoopStatus::Playing { start_beat: 4.0 };
        assert!(playing.is_playing());
        assert_eq!(playing.start_beat(), Some(4.0));
    }

    #[test]
    fn test_group_state() {
        let group = GroupState::new(
            "drums".to_string(),
            "main.drums".to_string(),
            Some("main".to_string()),
        );
        assert_eq!(group.name, "drums");
        assert_eq!(group.path, "main.drums");
        assert!(!group.muted);
    }

    #[test]
    fn test_voice_state() {
        let voice = VoiceState::new("kick".to_string(), "main.drums".to_string());
        assert_eq!(voice.name, "kick");
        assert_eq!(voice.polyphony, 1);
        assert!((voice.gain - 1.0).abs() < 0.001);
    }
}
