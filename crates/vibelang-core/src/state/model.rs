//! State model types for VibeLang.
//!
//! These types represent the complete state of a VibeLang session,
//! including groups, voices, patterns, melodies, effects, and samples.

use crate::api::context::SourceLocation;
use crate::events::{BeatEvent, FadeTargetType, Pattern};
use crate::midi::{MidiBackend, MidiDeviceInfo, MidiRouting};
use crate::sequences::SequenceDefinition;
use crate::timing::TimeSignature;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

// ============================================================================
// MIDI State Structures
// ============================================================================

/// Central MIDI configuration stored in state.
///
/// This contains all MIDI routing configuration and device connections,
/// making MIDI a first-class citizen in the state management system.
#[derive(Clone, Debug, Default)]
pub struct MidiConfiguration {
    /// Connected MIDI devices: device_id -> device state
    pub devices: HashMap<u32, MidiDeviceState>,
    /// MIDI routing configuration
    pub routing: MidiRouting,
    /// MIDI callbacks: callback_id -> callback metadata
    pub callbacks: HashMap<u64, MidiCallbackInfo>,
    /// Whether MIDI monitoring is enabled
    pub monitor_enabled: bool,
}

impl MidiConfiguration {
    /// Create a new empty MIDI configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all routing (but keep devices connected).
    pub fn clear_routing(&mut self) {
        self.routing.clear();
        self.callbacks.clear();
    }
}

/// State for a connected MIDI device.
#[derive(Clone, Debug)]
pub struct MidiDeviceState {
    /// Unique device ID (allocated by state)
    pub id: u32,
    /// Device info (name, port, backend)
    pub info: MidiDeviceInfo,
    /// Backend type (ALSA or JACK)
    pub backend: MidiBackend,
    /// Reload generation when this device was last seen
    pub generation: u64,
}

impl MidiDeviceState {
    /// Create a new MIDI device state.
    pub fn new(id: u32, info: MidiDeviceInfo, backend: MidiBackend) -> Self {
        Self {
            id,
            info,
            backend,
            generation: 0,
        }
    }
}

/// Metadata for a registered MIDI callback.
#[derive(Clone, Debug)]
pub struct MidiCallbackInfo {
    /// Unique callback ID
    pub id: u64,
    /// Type of callback
    pub callback_type: MidiCallbackType,
    /// Reload generation when this callback was registered
    pub generation: u64,
}

/// Type of MIDI callback.
#[derive(Clone, Debug)]
pub enum MidiCallbackType {
    /// Note callback (channel, note, on_note_on, on_note_off)
    Note {
        channel: Option<u8>,
        note: u8,
        on_note_on: bool,
        on_note_off: bool,
    },
    /// CC callback (channel, cc_number, threshold, above_threshold)
    Cc {
        channel: Option<u8>,
        cc_number: u8,
        threshold: Option<u8>,
        above_threshold: bool,
    },
}

// ============================================================================
// MIDI Recording State
// ============================================================================

/// A single recorded MIDI note with timing information.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedMidiNote {
    /// Absolute beat position (quantized).
    pub beat: f64,
    /// MIDI note number (0-127).
    pub note: u8,
    /// Velocity (0-127).
    pub velocity: u8,
    /// Duration in beats (calculated from note-off).
    pub duration: f64,
    /// Original unquantized beat (for display/debugging).
    pub raw_beat: f64,
    /// MIDI channel (1-16).
    pub channel: u8,
    /// Voice name this note was routed to.
    pub voice_name: String,
}

/// Configuration and state for MIDI recording.
///
/// Records incoming MIDI notes with quantization for later export
/// as pattern or melody syntax.
#[derive(Debug, Clone)]
pub struct MidiRecordingState {
    /// Ring buffer of recorded notes (oldest first).
    pub notes: VecDeque<RecordedMidiNote>,

    /// Quantization grid: valid positions per bar (4, 8, 16, 32, 64).
    pub quantization: u8,

    /// Maximum history in bars.
    pub max_history_bars: u16,

    /// Recording enabled flag.
    pub recording_enabled: bool,

    /// Pending note-ons awaiting note-off: (channel, note, voice_name) -> (quantized_beat, velocity, raw_beat).
    pub pending_notes: HashMap<(u8, u8, String), (f64, u8, f64)>,

    /// The beat position of the oldest note in history.
    pub oldest_beat: f64,
}

impl Default for MidiRecordingState {
    fn default() -> Self {
        Self {
            notes: VecDeque::with_capacity(8192), // ~32 notes/bar * 256 bars
            quantization: 16,                      // 16th notes default
            max_history_bars: 256,
            recording_enabled: true,
            pending_notes: HashMap::new(),
            oldest_beat: 0.0,
        }
    }
}

impl MidiRecordingState {
    /// Create a new MIDI recording state with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Quantize a beat position to the current grid.
    pub fn quantize_beat(&self, beat: f64, beats_per_bar: f64) -> f64 {
        let grid_size = beats_per_bar / self.quantization as f64;
        (beat / grid_size).round() * grid_size
    }

    /// Add a completed note to the history.
    pub fn add_note(&mut self, note: RecordedMidiNote, current_beat: f64, beats_per_bar: f64) {
        self.notes.push_back(note);
        self.prune_old_notes(current_beat, beats_per_bar);
    }

    /// Remove notes older than max_history_bars.
    fn prune_old_notes(&mut self, current_beat: f64, beats_per_bar: f64) {
        let max_history_beats = self.max_history_bars as f64 * beats_per_bar;
        let cutoff_beat = current_beat - max_history_beats;

        while let Some(front) = self.notes.front() {
            if front.beat < cutoff_beat {
                self.notes.pop_front();
            } else {
                break;
            }
        }

        self.oldest_beat = self.notes.front().map(|n| n.beat).unwrap_or(current_beat);
    }

    /// Get notes within a beat range (inclusive start, exclusive end).
    pub fn get_notes_in_range(&self, start_beat: f64, end_beat: f64) -> Vec<&RecordedMidiNote> {
        self.notes
            .iter()
            .filter(|n| n.beat >= start_beat && n.beat < end_beat)
            .collect()
    }

    /// Find the last N bars that contain actual notes (skip empty trailing bars).
    ///
    /// Returns the beat range (start_beat, end_beat) of the bars containing notes,
    /// or None if there are no recorded notes.
    pub fn find_active_bars(
        &self,
        num_bars: u8,
        _current_beat: f64,
        beats_per_bar: f64,
    ) -> Option<(f64, f64)> {
        if self.notes.is_empty() {
            return None;
        }

        // Find the last note's bar
        let last_note_beat = self.notes.back()?.beat;
        let last_note_bar = (last_note_beat / beats_per_bar).floor();

        // Calculate range: from (last_note_bar - num_bars + 1) to (last_note_bar + 1)
        let start_bar = (last_note_bar - num_bars as f64 + 1.0).max(0.0);
        let end_bar = last_note_bar + 1.0;

        let start_beat = start_bar * beats_per_bar;
        let end_beat = end_bar * beats_per_bar;

        Some((start_beat, end_beat))
    }

    /// Clear all recorded notes.
    pub fn clear(&mut self) {
        self.notes.clear();
        self.pending_notes.clear();
        self.oldest_beat = 0.0;
    }

    /// Get all unique voice names that have recorded notes.
    pub fn get_voices(&self) -> Vec<String> {
        let mut voices: Vec<String> = self
            .notes
            .iter()
            .map(|n| n.voice_name.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        voices.sort();
        voices
    }

    /// Get notes within a beat range filtered by voice.
    pub fn get_notes_for_voice(
        &self,
        start_beat: f64,
        end_beat: f64,
        voice_name: &str,
    ) -> Vec<&RecordedMidiNote> {
        self.notes
            .iter()
            .filter(|n| n.beat >= start_beat && n.beat < end_beat && n.voice_name == voice_name)
            .collect()
    }

    /// Export notes as pattern syntax (for drums - single pitch).
    ///
    /// Returns a string like `"x...x...|x.x.x..."` suitable for `.step()`.
    ///
    /// # Arguments
    /// * `start_beat` - Start of the range
    /// * `end_beat` - End of the range
    /// * `beats_per_bar` - Beats per bar (e.g., 4.0 for 4/4 time)
    /// * `voice_name` - Voice to filter by
    pub fn export_as_pattern(
        &self,
        start_beat: f64,
        end_beat: f64,
        beats_per_bar: f64,
        voice_name: &str,
    ) -> String {
        let total_beats = end_beat - start_beat;
        let steps_per_beat = self.quantization as f64 / beats_per_bar;
        let total_steps = (total_beats * steps_per_beat).round() as usize;
        let steps_per_bar = self.quantization as usize;

        let mut pattern = vec!['.'; total_steps];

        for note in self.get_notes_for_voice(start_beat, end_beat, voice_name) {
            let relative_beat = note.beat - start_beat;
            let step_index = (relative_beat * steps_per_beat).round() as usize;

            if step_index < total_steps {
                // Map velocity to character
                pattern[step_index] = velocity_to_step_char(note.velocity);
            }
        }

        // Insert bar separators
        format_pattern_with_bars(&pattern, steps_per_bar)
    }

    /// Export notes as melody syntax.
    ///
    /// Returns a string like `"C4 - E4 - | G4 - - -"` suitable for `.notes()`.
    ///
    /// # Arguments
    /// * `start_beat` - Start of the range
    /// * `end_beat` - End of the range
    /// * `beats_per_bar` - Beats per bar (e.g., 4.0 for 4/4 time)
    /// * `voice_name` - Voice to filter by
    pub fn export_as_melody(
        &self,
        start_beat: f64,
        end_beat: f64,
        beats_per_bar: f64,
        voice_name: &str,
    ) -> String {
        let total_beats = end_beat - start_beat;
        let steps_per_beat = self.quantization as f64 / beats_per_bar;
        let total_steps = (total_beats * steps_per_beat).round() as usize;
        let steps_per_bar = self.quantization as usize;

        // Initialize with rests: None = rest, Some((note, is_start))
        let mut melody: Vec<Option<(u8, bool)>> = vec![None; total_steps];

        // Sort notes by beat for consistent output
        let mut notes: Vec<_> = self.get_notes_for_voice(start_beat, end_beat, voice_name);
        notes.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap());

        for note in notes {
            let relative_beat = note.beat - start_beat;
            let start_step = (relative_beat * steps_per_beat).round() as usize;
            let duration_steps = (note.duration * steps_per_beat).round().max(1.0) as usize;

            if start_step < total_steps {
                // First step is the note name
                melody[start_step] = Some((note.note, true));

                // Following steps are ties (if duration > 1 step)
                for i in 1..duration_steps {
                    let tie_step = start_step + i;
                    if tie_step < total_steps && melody[tie_step].is_none() {
                        melody[tie_step] = Some((note.note, false));
                    }
                }
            }
        }

        // Convert to string tokens
        let tokens: Vec<String> = melody
            .iter()
            .map(|slot| match slot {
                None => ".".to_string(),
                Some((note, true)) => midi_note_to_name(*note),
                Some((_, false)) => "-".to_string(),
            })
            .collect();

        // Insert bar separators
        format_melody_with_bars(&tokens, steps_per_bar)
    }

    /// Export full pattern definitions for selected voices.
    ///
    /// Returns lines like: `pattern("rec_kick").on("kick").step("x...x...");`
    pub fn export_full_patterns(
        &self,
        start_beat: f64,
        end_beat: f64,
        beats_per_bar: f64,
        voices: &[String],
    ) -> String {
        let mut counter = 1;
        voices
            .iter()
            .filter_map(|voice| {
                let pattern = self.export_as_pattern(start_beat, end_beat, beats_per_bar, voice);
                // Skip if all dots/spaces/bars (empty pattern)
                if pattern.chars().all(|c| c == '.' || c == '|' || c == ' ') {
                    return None;
                }
                let name = format!("rec_{}_{}", voice, counter);
                counter += 1;
                Some(format!(
                    "pattern(\"{}\").on(\"{}\").step(\"{}\");",
                    name, voice, pattern
                ))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Export full melody definitions for selected voices.
    ///
    /// Returns lines like: `melody("rec_lead_1").on("lead").notes("C4 E4 G4");`
    pub fn export_full_melodies(
        &self,
        start_beat: f64,
        end_beat: f64,
        beats_per_bar: f64,
        voices: &[String],
    ) -> String {
        let mut counter = 1;
        voices
            .iter()
            .filter_map(|voice| {
                let melody = self.export_as_melody(start_beat, end_beat, beats_per_bar, voice);
                // Skip if all dots/spaces/bars (empty melody)
                if melody.chars().all(|c| c == '.' || c == '|' || c == ' ') {
                    return None;
                }
                let name = format!("rec_{}_{}", voice, counter);
                counter += 1;
                Some(format!(
                    "melody(\"{}\").on(\"{}\").notes(\"{}\");",
                    name, voice, melody
                ))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Map MIDI velocity (0-127) to step character.
fn velocity_to_step_char(velocity: u8) -> char {
    match velocity {
        0..=12 => '1',
        13..=25 => '2',
        26..=38 => '3',
        39..=51 => '4',
        52..=64 => '5',
        65..=76 => '6',
        77..=89 => '7',
        90..=101 => '8',
        102..=114 => '9',
        115..=127 => 'x',
        _ => 'x',
    }
}

/// Convert MIDI note number to note name (e.g., 60 -> "C4").
fn midi_note_to_name(note: u8) -> String {
    const NOTES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let octave = (note / 12) as i8 - 1;
    let note_name = NOTES[(note % 12) as usize];
    format!("{}{}", note_name, octave)
}

/// Determine the sub-group size for formatting within a bar.
/// Returns a group size that divides the bar into 2-4 readable chunks.
fn get_subgroup_size(steps_per_bar: usize) -> usize {
    match steps_per_bar {
        1..=4 => steps_per_bar, // No sub-grouping for small bars
        5..=8 => 4,             // Group by 4 (2 groups for 8)
        9..=16 => 4,            // Group by 4 (4 groups for 16)
        17..=32 => 8,           // Group by 8 (4 groups for 32)
        _ => 8,                 // Default to 8 for larger values
    }
}

/// Format pattern with bar separators and sub-groupings.
fn format_pattern_with_bars(pattern: &[char], steps_per_bar: usize) -> String {
    let subgroup_size = get_subgroup_size(steps_per_bar);

    pattern
        .chunks(steps_per_bar)
        .map(|bar| {
            // Format each bar with sub-groupings (spaces between groups)
            bar.chunks(subgroup_size)
                .map(|group| group.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Format a group of melody tokens compactly (no spaces within groups).
fn format_melody_group(tokens: &[String]) -> String {
    tokens.concat()
}

/// Format melody tokens with bar separators and sub-groupings.
fn format_melody_with_bars(tokens: &[String], steps_per_bar: usize) -> String {
    let subgroup_size = get_subgroup_size(steps_per_bar);

    tokens
        .chunks(steps_per_bar)
        .map(|bar| {
            // Format each bar with sub-groupings
            bar.chunks(subgroup_size)
                .map(|group| format_melody_group(group))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

// ============================================================================
// Script State
// ============================================================================

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
    /// Loaded synthdefs by name (bytes stored for score capture).
    pub synthdefs: HashMap<String, Vec<u8>>,
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
    /// MIDI configuration (devices, routing, callbacks).
    pub midi_config: MidiConfiguration,
    /// Next available MIDI device ID.
    pub next_midi_device_id: u32,
    /// Next available MIDI callback ID.
    pub next_midi_callback_id: u64,
    /// MIDI recording state for pattern/melody export.
    pub midi_recording: MidiRecordingState,
    /// Audio meter levels by group path.
    pub meter_levels: HashMap<String, MeterLevel>,
}

// ============================================================================
// Audio Metering
// ============================================================================

/// Audio meter level for a group (stereo).
#[derive(Clone, Debug, Default)]
pub struct MeterLevel {
    /// Peak level for left channel (0.0 to 1.0+, can exceed 1.0 for clipping).
    pub peak_left: f32,
    /// Peak level for right channel (0.0 to 1.0+).
    pub peak_right: f32,
    /// RMS level for left channel (0.0 to 1.0+).
    pub rms_left: f32,
    /// RMS level for right channel (0.0 to 1.0+).
    pub rms_right: f32,
    /// Time of last update.
    pub last_update: Option<Instant>,
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
            synthdefs: HashMap::new(),
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
            midi_config: MidiConfiguration::new(),
            next_midi_device_id: 1,
            next_midi_callback_id: 1,
            midi_recording: MidiRecordingState::new(),
            meter_levels: HashMap::new(),
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

    /// Allocate a new MIDI device ID.
    pub fn allocate_midi_device_id(&mut self) -> u32 {
        let id = self.next_midi_device_id;
        self.next_midi_device_id += 1;
        id
    }

    /// Allocate a new MIDI callback ID.
    pub fn allocate_midi_callback_id(&mut self) -> u64 {
        let id = self.next_midi_callback_id;
        self.next_midi_callback_id += 1;
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
    /// Allocated audio bus for this group (always set, never optional).
    pub audio_bus: i32,
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
    /// Source location where this group was defined.
    pub source_location: SourceLocation,
}

impl GroupState {
    /// Create a new group state.
    ///
    /// # Arguments
    /// * `name` - Short name of the group
    /// * `path` - Full path (e.g., "main/drums")
    /// * `parent_path` - Parent group path, if any (None for root "main" group)
    /// * `audio_bus` - Allocated audio bus for this group (required)
    pub fn new(name: String, path: String, parent_path: Option<String>, audio_bus: i32) -> Self {
        Self {
            name,
            path,
            parent_path,
            node_id: None,
            audio_bus,
            link_synth_node_id: None,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            synth_node_ids: Vec::new(),
            generation: 0,
            source_location: SourceLocation::default(),
        }
    }

    /// Create a new group state with source location.
    pub fn with_source_location(mut self, source_location: SourceLocation) -> Self {
        self.source_location = source_location;
        self
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
    /// Source location where this voice was defined.
    pub source_location: SourceLocation,
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
            source_location: SourceLocation::default(),
        }
    }

    /// Create a new voice state with source location.
    pub fn with_source_location(mut self, source_location: SourceLocation) -> Self {
        self.source_location = source_location;
        self
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
    /// Source location where this pattern was defined.
    pub source_location: SourceLocation,
    /// Original step pattern string (e.g., "x..x..x.|x.x.x.x.") for visual editing.
    pub step_pattern: Option<String>,
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
            source_location: SourceLocation::default(),
            step_pattern: None,
        }
    }

    /// Create a new pattern state with source location.
    pub fn with_source_location(mut self, source_location: SourceLocation) -> Self {
        self.source_location = source_location;
        self
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
    /// Source location where this melody was defined.
    pub source_location: SourceLocation,
    /// Original notes pattern string for visual editing.
    pub notes_pattern: Option<String>,
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
            source_location: SourceLocation::default(),
            notes_pattern: None,
        }
    }

    /// Create a new melody state with source location.
    pub fn with_source_location(mut self, source_location: SourceLocation) -> Self {
        self.source_location = source_location;
        self
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
    /// Whether this sequence has completed (for play_once mode).
    pub completed: bool,
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
    /// Source location where this effect was defined.
    pub source_location: SourceLocation,
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
            16,  // audio_bus
        );
        assert_eq!(group.name, "drums");
        assert_eq!(group.path, "main.drums");
        assert_eq!(group.audio_bus, 16);
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
