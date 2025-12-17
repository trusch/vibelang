//! Request and response models for the HTTP API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Source Location (for navigation to code)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

// =============================================================================
// Transport
// =============================================================================

#[derive(Debug, Serialize)]
pub struct TransportState {
    pub bpm: f32,
    pub time_signature: TimeSignature,
    pub running: bool,
    pub current_beat: f64,
    pub quantization_beats: f64,
    /// The loop length in beats (from the longest active sequence), or None if no sequences.
    /// UI can use this to calculate loop position for display.
    pub loop_beats: Option<f64>,
    /// The current beat position within the loop (current_beat % loop_beats).
    /// None if no loop is active.
    pub loop_beat: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Debug, Deserialize)]
pub struct TransportUpdate {
    pub bpm: Option<f32>,
    pub time_signature: Option<TimeSignature>,
    pub quantization_beats: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct SeekRequest {
    pub beat: f64,
}

// =============================================================================
// Groups
// =============================================================================

#[derive(Debug, Serialize)]
pub struct Group {
    pub name: String,
    pub path: String,
    pub parent_path: Option<String>,
    pub children: Vec<String>,
    pub node_id: Option<i32>,
    pub audio_bus: i32,
    pub link_synth_node_id: Option<i32>,
    pub muted: bool,
    pub soloed: bool,
    pub params: HashMap<String, f32>,
    pub synth_node_ids: Vec<i32>,
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Deserialize)]
pub struct GroupCreate {
    pub name: String,
    #[serde(default = "default_main")]
    pub parent_path: String,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

fn default_main() -> String {
    "main".to_string()
}

#[derive(Debug, Deserialize)]
pub struct GroupUpdate {
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Deserialize)]
pub struct ParamSet {
    pub value: f32,
    pub fade_beats: Option<f64>,
}

// =============================================================================
// Voices
// =============================================================================

#[derive(Debug, Serialize)]
pub struct Voice {
    pub name: String,
    pub synth_name: String,
    pub polyphony: usize,
    pub gain: f32,
    pub group_path: String,
    pub group_name: String,
    pub output_bus: Option<i32>,
    pub muted: bool,
    pub soloed: bool,
    pub params: HashMap<String, f32>,
    pub sfz_instrument: Option<String>,
    pub vst_instrument: Option<String>,
    pub active_notes: Vec<u8>,
    pub sustained_notes: Vec<u8>,
    pub running: bool,
    pub running_node_id: Option<i32>,
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Deserialize)]
pub struct VoiceCreate {
    pub name: String,
    pub synth_name: Option<String>,
    #[serde(default = "default_polyphony")]
    pub polyphony: usize,
    #[serde(default = "default_gain")]
    pub gain: f32,
    #[serde(default = "default_main")]
    pub group_path: String,
    #[serde(default)]
    pub params: HashMap<String, f32>,
    pub sample: Option<String>,
    pub sfz: Option<String>,
}

fn default_polyphony() -> usize {
    8
}

fn default_gain() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
pub struct VoiceUpdate {
    pub synth_name: Option<String>,
    pub polyphony: Option<usize>,
    pub gain: Option<f32>,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Deserialize)]
pub struct TriggerRequest {
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Deserialize)]
pub struct NoteOnRequest {
    pub note: u8,
    #[serde(default = "default_velocity")]
    pub velocity: u8,
}

fn default_velocity() -> u8 {
    100
}

#[derive(Debug, Deserialize)]
pub struct NoteOffRequest {
    pub note: u8,
}

// =============================================================================
// Patterns
// =============================================================================

#[derive(Debug, Serialize)]
pub struct Pattern {
    pub name: String,
    pub voice_name: String,
    pub group_path: String,
    pub loop_beats: f64,
    pub events: Vec<PatternEvent>,
    pub params: HashMap<String, f32>,
    pub status: LoopStatus,
    pub is_looping: bool,
    pub source_location: Option<SourceLocation>,
    /// Original step pattern string (e.g., "x..x..x.|x.x.x.x.") for visual editing
    pub step_pattern: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatternEvent {
    pub beat: f64,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Serialize)]
pub struct LoopStatus {
    pub state: String,
    pub start_beat: Option<f64>,
    pub stop_beat: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct PatternCreate {
    pub name: String,
    pub voice_name: String,
    pub group_path: Option<String>,
    #[serde(default = "default_loop_beats")]
    pub loop_beats: f64,
    #[serde(default)]
    pub events: Vec<PatternEvent>,
    pub pattern_string: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

fn default_loop_beats() -> f64 {
    4.0
}

#[derive(Debug, Deserialize)]
pub struct PatternUpdate {
    pub events: Option<Vec<PatternEvent>>,
    pub pattern_string: Option<String>,
    pub loop_beats: Option<f64>,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Deserialize)]
pub struct StartRequest {
    pub quantize_beats: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct StopRequest {
    pub quantize_beats: Option<f64>,
}

// =============================================================================
// Melodies
// =============================================================================

#[derive(Debug, Serialize)]
pub struct Melody {
    pub name: String,
    pub voice_name: String,
    pub group_path: String,
    pub loop_beats: f64,
    pub events: Vec<MelodyEvent>,
    pub params: HashMap<String, f32>,
    pub status: LoopStatus,
    pub is_looping: bool,
    pub source_location: Option<SourceLocation>,
    /// Original notes pattern string for visual editing
    pub notes_pattern: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MelodyEvent {
    pub beat: f64,
    pub note: String,
    pub frequency: Option<f32>,
    pub duration: Option<f64>,
    pub velocity: Option<f32>,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Deserialize)]
pub struct MelodyCreate {
    pub name: String,
    pub voice_name: String,
    pub group_path: Option<String>,
    #[serde(default = "default_loop_beats")]
    pub loop_beats: f64,
    #[serde(default)]
    pub events: Vec<MelodyEvent>,
    pub melody_string: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Deserialize)]
pub struct MelodyUpdate {
    pub events: Option<Vec<MelodyEvent>>,
    pub melody_string: Option<String>,
    pub loop_beats: Option<f64>,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

// =============================================================================
// Sequences
// =============================================================================

#[derive(Debug, Serialize)]
pub struct Sequence {
    pub name: String,
    pub loop_beats: f64,
    pub clips: Vec<SequenceClip>,
    pub play_once: bool,
    pub active: bool,
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SequenceClip {
    #[serde(rename = "type")]
    pub clip_type: String,
    pub name: String,
    pub start_beat: f64,
    pub end_beat: f64,
    pub mode: String,
}

#[derive(Debug, Deserialize)]
pub struct SequenceCreate {
    pub name: String,
    #[serde(default = "default_sequence_loop_beats")]
    pub loop_beats: f64,
    #[serde(default)]
    pub clips: Vec<SequenceClip>,
}

fn default_sequence_loop_beats() -> f64 {
    16.0
}

#[derive(Debug, Deserialize)]
pub struct SequenceUpdate {
    pub loop_beats: Option<f64>,
    pub clips: Option<Vec<SequenceClip>>,
}

#[derive(Debug, Deserialize)]
pub struct SequenceStartRequest {
    #[serde(default)]
    pub play_once: bool,
}

// =============================================================================
// Effects
// =============================================================================

#[derive(Debug, Serialize)]
pub struct Effect {
    pub id: String,
    pub synthdef_name: String,
    pub group_path: String,
    pub node_id: Option<i32>,
    pub bus_in: Option<i32>,
    pub bus_out: Option<i32>,
    pub params: HashMap<String, f32>,
    pub position: usize,
    pub vst_plugin: Option<String>,
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Deserialize)]
pub struct EffectCreate {
    pub id: Option<String>,
    pub synthdef_name: String,
    pub group_path: String,
    #[serde(default)]
    pub params: HashMap<String, f32>,
    pub position: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct EffectUpdate {
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

// =============================================================================
// Samples
// =============================================================================

#[derive(Debug, Serialize)]
pub struct Sample {
    pub id: String,
    pub path: String,
    pub buffer_id: i32,
    pub num_channels: i32,
    pub num_frames: i32,
    pub sample_rate: f32,
    pub synthdef_name: String,
    pub slices: Vec<SampleSlice>,
}

#[derive(Debug, Serialize)]
pub struct SampleSlice {
    pub index: usize,
    pub start_frame: i32,
    pub end_frame: i32,
    pub synthdef_name: String,
}

#[derive(Debug, Deserialize)]
pub struct SampleLoad {
    pub id: Option<String>,
    pub path: String,
}

// =============================================================================
// SynthDefs
// =============================================================================

#[derive(Debug, Serialize)]
pub struct SynthDef {
    pub name: String,
    pub params: Vec<SynthDefParam>,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct SynthDefParam {
    pub name: String,
    pub default_value: f32,
    pub min_value: Option<f32>,
    pub max_value: Option<f32>,
}

// =============================================================================
// Fades
// =============================================================================

#[derive(Debug, Serialize)]
pub struct ActiveFade {
    pub id: String,
    pub name: Option<String>,
    pub target_type: String,
    pub target_name: String,
    pub param_name: String,
    pub start_value: f32,
    pub target_value: f32,
    pub current_value: f32,
    pub duration_beats: f64,
    pub start_beat: f64,
    pub progress: f32,
}

#[derive(Debug, Deserialize)]
pub struct FadeCreate {
    pub target_type: String,
    pub target_name: String,
    pub param_name: String,
    pub start_value: Option<f32>,
    pub target_value: f32,
    pub duration_beats: f64,
}

// =============================================================================
// MIDI
// =============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct MidiDeviceInfo {
    pub name: String,
    pub port_index: usize,
    pub backend: String,
}

#[derive(Debug, Serialize)]
pub struct MidiDeviceState {
    pub id: u32,
    pub info: MidiDeviceInfo,
    pub backend: String,
}

#[derive(Debug, Serialize)]
pub struct MidiDevicesResponse {
    pub available: Vec<MidiDeviceInfo>,
    pub connected: Vec<MidiDeviceState>,
}

#[derive(Debug, Deserialize)]
pub struct MidiConnectRequest {
    #[serde(default = "default_alsa")]
    pub backend: String,
}

fn default_alsa() -> String {
    "alsa".to_string()
}

#[derive(Debug, Serialize)]
pub struct MidiRouting {
    pub keyboard_routes: Vec<KeyboardRoute>,
    pub note_routes: Vec<NoteRoute>,
    pub cc_routes: Vec<CcRoute>,
    pub pitch_bend_routes: Vec<CcRoute>,
    pub aftertouch_routes: Vec<CcRoute>,
    pub choke_groups: HashMap<String, Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeyboardRoute {
    pub channel: Option<u8>,
    pub voice_name: String,
    #[serde(default)]
    pub transpose: i32,
    #[serde(default = "default_velocity_curve")]
    pub velocity_curve: String,
    #[serde(default)]
    pub note_range_low: u8,
    #[serde(default = "default_note_high")]
    pub note_range_high: u8,
}

fn default_velocity_curve() -> String {
    "linear".to_string()
}

fn default_note_high() -> u8 {
    127
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NoteRoute {
    pub channel: u8,
    pub note: u8,
    pub voice_name: String,
    pub choke_group: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CcRoute {
    pub channel: u8,
    pub cc_number: u8,
    pub target_type: String,
    pub target_name: String,
    pub param_name: String,
    #[serde(default)]
    pub min_value: f32,
    #[serde(default = "default_gain")]
    pub max_value: f32,
}

#[derive(Debug, Serialize)]
pub struct MidiCallback {
    pub id: u64,
    pub callback_type: String,
    pub channel: Option<u8>,
    pub note: Option<u8>,
    pub cc_number: Option<u8>,
    pub threshold: Option<u8>,
    pub above_threshold: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MidiRecordingState {
    pub recording_enabled: bool,
    pub quantization: u8,
    pub max_history_bars: u16,
    pub note_count: usize,
    pub oldest_beat: f64,
    pub pending_notes: usize,
}

#[derive(Debug, Deserialize)]
pub struct MidiRecordingUpdate {
    pub recording_enabled: Option<bool>,
    pub quantization: Option<u8>,
    pub max_history_bars: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct RecordedMidiNote {
    pub beat: f64,
    pub note: u8,
    pub velocity: u8,
    pub duration: f64,
    pub raw_beat: f64,
    pub channel: u8,
    pub voice_name: String,
}

#[derive(Debug, Deserialize)]
pub struct RecordedNotesQuery {
    pub start_beat: Option<f64>,
    pub end_beat: Option<f64>,
    pub voice: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub start_beat: Option<f64>,
    pub bars: Option<u32>,
    pub voice: Option<String>,
    #[serde(default = "default_melody")]
    pub format: String,
}

fn default_melody() -> String {
    "melody".to_string()
}

#[derive(Debug, Deserialize)]
pub struct MonitorRequest {
    pub enabled: bool,
}

// =============================================================================
// Live State
// =============================================================================

#[derive(Debug, Serialize)]
pub struct LiveState {
    pub transport: TransportState,
    pub active_synths: Vec<ActiveSynth>,
    pub active_sequences: Vec<ActiveSequence>,
    pub active_fades: Vec<ActiveFade>,
    pub active_notes: HashMap<String, Vec<u8>>,
    pub patterns_status: HashMap<String, LoopStatus>,
    pub melodies_status: HashMap<String, LoopStatus>,
}

#[derive(Debug, Serialize)]
pub struct ActiveSynth {
    pub node_id: i32,
    pub synthdef_name: String,
    pub voice_name: Option<String>,
    pub group_path: Option<String>,
    pub created_at_beat: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ActiveSequence {
    pub name: String,
    pub start_beat: f64,
    pub current_position: f64,
    pub loop_beats: f64,
    pub iteration: u32,
    pub play_once: bool,
}

// =============================================================================
// Audio Metering
// =============================================================================

/// Audio meter level for a group (stereo).
#[derive(Debug, Clone, Serialize)]
pub struct MeterLevel {
    /// Peak level for left channel (0.0 to 1.0+, can exceed 1.0 for clipping).
    pub peak_left: f32,
    /// Peak level for right channel (0.0 to 1.0+).
    pub peak_right: f32,
    /// RMS level for left channel (0.0 to 1.0+).
    pub rms_left: f32,
    /// RMS level for right channel (0.0 to 1.0+).
    pub rms_right: f32,
}

// =============================================================================
// Error Response
// =============================================================================

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

impl ErrorResponse {
    pub fn new(error: &str, message: &str) -> Self {
        Self {
            error: error.to_string(),
            message: message.to_string(),
        }
    }

    pub fn not_found(message: &str) -> Self {
        Self::new("not_found", message)
    }

    pub fn bad_request(message: &str) -> Self {
        Self::new("bad_request", message)
    }

    pub fn conflict(message: &str) -> Self {
        Self::new("conflict", message)
    }

    pub fn internal(message: &str) -> Self {
        Self::new("internal_error", message)
    }
}
