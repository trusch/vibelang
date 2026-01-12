//! Request and response models for the HTTP API.
//!
//! These models match the TypeScript types expected by the VSCode extension.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Source Location (for navigation to code)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

// =============================================================================
// Transport
// =============================================================================

#[derive(Debug, Serialize)]
pub struct TransportState {
    pub bpm: f64,
    pub time_signature: TimeSignature,
    pub running: bool,
    pub current_beat: f64,
    pub quantization_beats: f64,
    /// Loop length in beats from longest active sequence (null if no sequences).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_beats: Option<f64>,
    /// Current beat position within the loop (current_beat % loop_beats).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_beat: Option<f64>,
    /// Server timestamp when this state was captured (milliseconds since Unix epoch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_time_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Debug, Deserialize)]
pub struct TransportUpdate {
    pub bpm: Option<f64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_path: Option<String>,
    pub children: Vec<String>,
    pub node_id: i32,
    pub audio_bus: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_synth_node_id: Option<i32>,
    pub muted: bool,
    pub soloed: bool,
    pub params: HashMap<String, f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synth_node_ids: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Deserialize)]
pub struct GroupCreate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_path: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, f32>,
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
    pub polyphony: u8,
    pub gain: f32,
    pub group_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_bus: Option<i32>,
    pub muted: bool,
    pub soloed: bool,
    pub params: HashMap<String, f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sfz_instrument: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vst_instrument: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_notes: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sustained_notes: Option<Vec<u8>>,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_node_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Deserialize)]
pub struct VoiceCreate {
    pub name: Option<String>,
    #[serde(alias = "synthdef")]
    pub synth_name: Option<String>,
    pub polyphony: Option<u8>,
    pub gain: Option<f32>,
    #[serde(alias = "group_id")]
    pub group_path: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, f32>,
    pub sample: Option<String>,
    pub sfz: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VoiceUpdate {
    pub synth_name: Option<String>,
    pub polyphony: Option<u8>,
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
    pub velocity: f32,
}

fn default_velocity() -> f32 {
    0.8
}

#[derive(Debug, Deserialize)]
pub struct NoteOffRequest {
    pub note: u8,
}

// =============================================================================
// Loop Status (for patterns and melodies)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopState {
    Stopped,
    Queued,
    Playing,
    QueuedStop,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoopStatus {
    pub state: LoopState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_beat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_beat: Option<f64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, f32>>,
    pub status: LoopStatus,
    pub is_looping: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_location: Option<SourceLocation>,
    /// Original step pattern string (e.g., "x..x..x.|x.x.x.x.") for visual editing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_pattern: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatternEvent {
    pub beat: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, f32>>,
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
pub struct PatternCreate {
    /// Pattern name/ID.
    pub name: String,
    /// Voice name to trigger.
    pub voice_name: String,
    /// Loop length in beats.
    #[serde(default = "default_loop_beats")]
    pub loop_beats: f64,
    /// Pattern events.
    #[serde(default)]
    pub events: Vec<PatternEvent>,
    /// Step pattern string (e.g., "x..x..x.").
    pub pattern_string: Option<String>,
    /// Default parameters for all events.
    #[serde(default)]
    pub params: HashMap<String, f32>,
    /// Swing amount (0.0 to 1.0).
    #[serde(default)]
    pub swing: f32,
}

fn default_loop_beats() -> f64 {
    4.0
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, f32>>,
    pub status: LoopStatus,
    pub is_looping: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_location: Option<SourceLocation>,
    /// Notes pattern strings for visual editing (one per lane).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_patterns: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MelodyEvent {
    pub beat: f64,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, f32>>,
}

#[derive(Debug, Deserialize)]
pub struct MelodyCreate {
    /// Melody name/ID.
    pub name: String,
    /// Voice name to trigger notes on.
    pub voice_name: String,
    /// Loop length in beats.
    #[serde(default = "default_loop_beats")]
    pub loop_beats: f64,
    /// Melody events (notes with beat positions).
    #[serde(default)]
    pub events: Vec<MelodyEvent>,
    /// Melody string notation (e.g., "C4 D4 E4").
    pub melody_string: Option<String>,
    /// Default parameters for all notes.
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Deserialize)]
pub struct MelodyUpdate {
    pub events: Option<Vec<MelodyEvent>>,
    pub melody_string: Option<String>,
    pub lanes: Option<Vec<String>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_once: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SequenceClip {
    #[serde(rename = "type")]
    pub clip_type: String,
    pub name: String,
    pub start_beat: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_beat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_beats: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub once: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SequenceCreate {
    /// Sequence name/ID.
    pub name: String,
    /// Loop length in beats.
    #[serde(default = "default_sequence_length")]
    pub loop_beats: f64,
    /// Clips in the sequence (patterns, melodies, fades, nested sequences).
    #[serde(default)]
    pub clips: Vec<SequenceClip>,
}

fn default_sequence_length() -> f64 {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus_in: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus_out: Option<i32>,
    pub params: HashMap<String, f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vst_plugin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Deserialize)]
pub struct EffectCreate {
    pub id: Option<String>,
    pub synthdef_name: String,
    pub group_path: String,
    #[serde(default)]
    pub params: HashMap<String, f32>,
    pub position: Option<i32>,
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
pub struct SampleSlice {
    pub name: String,
    pub start_frame: i64,
    pub end_frame: i64,
}

#[derive(Debug, Serialize)]
pub struct Sample {
    pub id: String,
    pub path: String,
    pub buffer_id: i32,
    pub num_channels: i32,
    pub num_frames: i64,
    pub sample_rate: f32,
    pub synthdef_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slices: Option<Vec<SampleSlice>>,
}

#[derive(Debug, Deserialize)]
pub struct SampleLoad {
    /// Path to the audio file.
    pub path: String,
    /// Optional sample ID (auto-generated if not provided).
    pub id: Option<String>,
}

// =============================================================================
// SynthDefs
// =============================================================================

#[derive(Debug, Serialize)]
pub struct SynthDefParam {
    pub name: String,
    pub default_value: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SynthDefSource {
    Builtin,
    User,
    Stdlib,
}

#[derive(Debug, Serialize)]
pub struct SynthDefInfo {
    pub name: String,
    pub params: Vec<SynthDefParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SynthDefSource>,
}

// =============================================================================
// Fades
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FadeTargetType {
    Group,
    Voice,
    Effect,
}

#[derive(Debug, Serialize)]
pub struct ActiveFade {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub target_type: FadeTargetType,
    pub target_name: String,
    pub param_name: String,
    pub start_value: f32,
    pub target_value: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_value: Option<f32>,
    pub duration_beats: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_beat: Option<f64>,
    pub progress: f64,
}

#[derive(Debug, Deserialize)]
pub struct FadeCreate {
    pub target_type: FadeTargetType,
    pub target_name: String,
    pub param_name: String,
    pub start_value: Option<f32>,
    pub target_value: f32,
    pub duration_beats: f64,
}

// =============================================================================
// Live State
// =============================================================================

#[derive(Debug, Serialize)]
pub struct ActiveSynth {
    pub node_id: i32,
    pub synthdef_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at_beat: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ActiveSequence {
    pub name: String,
    pub start_beat: f64,
    pub current_position: f64,
    pub loop_beats: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_once: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct LiveState {
    pub transport: TransportState,
    pub active_synths: Vec<ActiveSynth>,
    pub active_sequences: Vec<ActiveSequence>,
    pub active_fades: Vec<ActiveFade>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_notes: Option<HashMap<String, Vec<u8>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patterns_status: Option<HashMap<String, LoopStatus>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub melodies_status: Option<HashMap<String, LoopStatus>>,
}

// =============================================================================
// Metering
// =============================================================================

#[derive(Debug, Serialize)]
pub struct MeterLevel {
    pub peak_left: f32,
    pub peak_right: f32,
    pub rms_left: f32,
    pub rms_right: f32,
}

/// MeterLevels is a map from group path to meter level.
pub type MeterLevels = HashMap<String, MeterLevel>;

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

    pub fn internal(message: &str) -> Self {
        Self::new("internal_error", message)
    }
}
