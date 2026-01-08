//! Request and response models for the HTTP API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Transport
// =============================================================================

#[derive(Debug, Serialize)]
pub struct TransportState {
    pub bpm: f64,
    pub time_signature: TimeSignature,
    pub running: bool,
    pub current_beat: f64,
    /// Server timestamp when this state was captured (milliseconds since Unix epoch).
    /// Clients can use this to compensate for network latency.
    pub server_time_ms: u64,
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
    pub id: String,
    pub parent_id: Option<String>,
    pub node_id: i32,
    pub audio_bus: i32,
    pub link_synth_node_id: Option<i32>,
    pub muted: bool,
    pub soloed: bool,
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
    pub id: String,
    pub synthdef: Option<String>,
    pub group_id: String,
    pub polyphony: u8,
    pub gain: f32,
    pub muted: bool,
    pub params: HashMap<String, f32>,
    pub sfz_instrument: Option<String>,
    pub active_notes: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct VoiceUpdate {
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
// Patterns
// =============================================================================

#[derive(Debug, Serialize)]
pub struct Pattern {
    pub id: String,
    pub voice_id: String,
    pub loop_beats: f64,
    pub steps: Vec<PatternStep>,
    pub playing: bool,
    pub loop_position: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatternStep {
    pub beat: f64,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Deserialize)]
pub struct PatternUpdate {
    /// Update steps (note: requires script reload, not supported via API)
    pub steps: Option<Vec<PatternStep>>,
    /// Update loop length (note: requires script reload, not supported via API)
    pub loop_beats: Option<f64>,
    /// Set params on all steps (e.g., {"amp": 0.5} sets amp on every step)
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
    pub id: String,
    pub voice_id: String,
    pub loop_beats: f64,
    pub notes: Vec<MelodyNote>,
    pub playing: bool,
    pub loop_position: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MelodyNote {
    pub beat: f64,
    pub note: u8,
    pub velocity: f32,
    pub gate: f64,
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Deserialize)]
pub struct MelodyUpdate {
    pub notes: Option<Vec<MelodyNote>>,
    pub loop_beats: Option<f64>,
}

// =============================================================================
// Sequences
// =============================================================================

#[derive(Debug, Serialize)]
pub struct Sequence {
    pub id: String,
    pub loop_beats: f64,
    pub clips: Vec<SequenceClip>,
    pub playing: bool,
    pub paused: bool,
    pub looping: bool,
    pub position: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SequenceClip {
    #[serde(rename = "type")]
    pub clip_type: String,
    pub target_id: String,
    pub start_beat: f64,
    pub end_beat: f64,
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
    pub group_id: String,
    pub synthdef: String,
    pub node_id: i32,
    pub params: HashMap<String, f32>,
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
    pub num_frames: i64,
    pub sample_rate: f32,
}

// =============================================================================
// SynthDefs
// =============================================================================

#[derive(Debug, Serialize)]
pub struct SynthDefInfo {
    pub name: String,
}

// =============================================================================
// Fades
// =============================================================================

#[derive(Debug, Serialize)]
pub struct ActiveFade {
    pub target_type: String,
    pub target_id: String,
    pub param: String,
    pub from: f32,
    pub to: f32,
    pub duration_beats: f64,
    pub curve: String,
    pub current_value: f32,
}

// =============================================================================
// Live State
// =============================================================================

#[derive(Debug, Serialize)]
pub struct LiveState {
    pub transport: TransportState,
    pub groups: Vec<Group>,
    pub voices: Vec<Voice>,
    pub patterns: Vec<Pattern>,
    pub melodies: Vec<Melody>,
    pub sequences: Vec<Sequence>,
    pub effects: Vec<Effect>,
    pub active_fades: Vec<ActiveFade>,
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

    pub fn internal(message: &str) -> Self {
        Self::new("internal_error", message)
    }
}
