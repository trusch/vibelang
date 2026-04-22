//! Melodies trait for pitched sequences.
//!
//! Melodies are like patterns but include pitch and duration information.

use crate::types::{Beat, MelodyId, VoiceId};
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// A single note event in a melody.
#[derive(Clone, Debug)]
pub struct NoteEvent {
    /// Beat position within the melody.
    pub beat: Beat,

    /// MIDI note number (0-127).
    pub note: u8,

    /// Velocity (0.0-1.0).
    pub velocity: f32,

    /// Duration in beats.
    pub duration: Beat,

    /// Per-note voice parameters (e.g., cutoff, pan, resonance).
    ///
    /// These are merged into the synth params at note-on time,
    /// overriding the voice's default params for this specific note only.
    /// Special params like "velocity" and "gate" are handled separately
    /// via the `velocity` and `duration` fields above.
    pub params: HashMap<String, f32>,
}

/// Tolerance for floating-point comparisons in melody data.
/// This prevents false "updates" during reload due to float precision issues.
const FLOAT_TOLERANCE: f32 = 1e-6;

impl PartialEq for NoteEvent {
    fn eq(&self, other: &Self) -> bool {
        self.beat == other.beat
            && self.note == other.note
            && (self.velocity - other.velocity).abs() < FLOAT_TOLERANCE
            && self.duration == other.duration
            && self.params == other.params
    }
}

impl Eq for NoteEvent {}

impl NoteEvent {
    /// Create a new note event.
    pub fn new(beat: f64, note: u8, velocity: f32, duration: f64) -> Self {
        Self {
            beat: Beat::from_f64(beat),
            note,
            velocity,
            duration: Beat::from_f64(duration),
            params: HashMap::new(),
        }
    }

    /// Create a new note event with per-note parameters.
    pub fn new_with_params(
        beat: f64,
        note: u8,
        velocity: f32,
        duration: f64,
        params: HashMap<String, f32>,
    ) -> Self {
        Self {
            beat: Beat::from_f64(beat),
            note,
            velocity,
            duration: Beat::from_f64(duration),
            params,
        }
    }

    /// Create a quarter note (1 beat duration).
    pub fn quarter(beat: f64, note: u8, velocity: f32) -> Self {
        Self::new(beat, note, velocity, 1.0)
    }

    /// Create an eighth note (0.5 beat duration).
    pub fn eighth(beat: f64, note: u8, velocity: f32) -> Self {
        Self::new(beat, note, velocity, 0.5)
    }

    /// Create a sixteenth note (0.25 beat duration).
    pub fn sixteenth(beat: f64, note: u8, velocity: f32) -> Self {
        Self::new(beat, note, velocity, 0.25)
    }
}

/// Configuration for creating a melody.
#[derive(Clone, Debug)]
pub struct MelodyConfig {
    /// Melody name (for display in TUI/API).
    pub name: String,

    /// Voice to trigger (None if melody is just for MIDI output).
    pub voice: Option<VoiceId>,

    /// Note events in the melody.
    pub notes: Vec<NoteEvent>,

    /// Length of the melody in beats.
    pub length: Beat,

    /// Swing amount (0.0 = none, 1.0 = full).
    pub swing: f32,
}

impl PartialEq for MelodyConfig {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.voice == other.voice
            && self.notes == other.notes
            && self.length == other.length
            && (self.swing - other.swing).abs() < FLOAT_TOLERANCE
    }
}

impl Eq for MelodyConfig {}

impl MelodyConfig {
    /// Create a new melody configuration.
    pub fn new(name: impl Into<String>, voice: VoiceId, length: Beat) -> Self {
        Self {
            name: name.into(),
            voice: Some(voice),
            notes: Vec::new(),
            length,
            swing: 0.0,
        }
    }

    /// Create a melody configuration without a voice.
    pub fn without_voice(name: impl Into<String>, length: Beat) -> Self {
        Self {
            name: name.into(),
            voice: None,
            notes: Vec::new(),
            length,
            swing: 0.0,
        }
    }

    /// Create a melody with length in beats as f64.
    pub fn with_length(name: impl Into<String>, voice: VoiceId, length: f64) -> Self {
        Self::new(name, voice, Beat::from_f64(length))
    }

    /// Add a note to the melody.
    pub fn with_note(mut self, note: NoteEvent) -> Self {
        self.notes.push(note);
        self
    }

    /// Set the swing amount.
    pub fn with_swing(mut self, swing: f32) -> Self {
        self.swing = swing;
        self
    }
}

/// The content of a melody that can be swapped during hot reload.
///
/// This struct is separate from playback state (playing, loop_position) to enable
/// seamless hot reload. The content can be atomically swapped while the melody
/// continues playing, allowing changes to take effect at musical boundaries
/// without any audio disruption.
#[derive(Clone, Debug, PartialEq)]
pub struct MelodyContent {
    /// Melody name (for display in TUI/API).
    pub name: String,

    /// Voice to trigger (None if melody is just for MIDI output).
    pub voice: Option<VoiceId>,

    /// Note events in the melody.
    pub notes: Vec<NoteEvent>,

    /// Length of the melody in beats.
    pub length: Beat,

    /// Swing amount (0.0 = none, 1.0 = full).
    pub swing: f32,
}

impl MelodyContent {
    /// Create new melody content from a config.
    pub fn from_config(config: &MelodyConfig) -> Self {
        Self {
            name: config.name.clone(),
            voice: config.voice,
            notes: config.notes.clone(),
            length: config.length,
            swing: config.swing,
        }
    }

    /// Create an Arc-wrapped content from config.
    pub fn arc_from_config(config: &MelodyConfig) -> Arc<Self> {
        Arc::new(Self::from_config(config))
    }
}

impl From<&MelodyConfig> for MelodyContent {
    fn from(config: &MelodyConfig) -> Self {
        Self::from_config(config)
    }
}

impl From<MelodyConfig> for MelodyContent {
    fn from(config: MelodyConfig) -> Self {
        Self {
            name: config.name,
            voice: config.voice,
            notes: config.notes,
            length: config.length,
            swing: config.swing,
        }
    }
}

/// Melody management for pitched sequences.
///
/// Melodies loop continuously, sending note-on and note-off events
/// to their voice based on the note events.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait Melodies: Send + Sync {
    /// Create a new melody.
    async fn create(&self, id: MelodyId, config: MelodyConfig) -> Result<()>;

    /// Delete a melody.
    async fn delete(&self, id: MelodyId) -> Result<()>;

    /// Start playing a melody.
    async fn start(&self, id: MelodyId) -> Result<()>;

    /// Stop playing a melody.
    async fn stop(&self, id: MelodyId) -> Result<()>;
}

/// Melody management for pitched sequences (WASM version).
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait Melodies {
    /// Create a new melody.
    async fn create(&self, id: MelodyId, config: MelodyConfig) -> Result<()>;

    /// Delete a melody.
    async fn delete(&self, id: MelodyId) -> Result<()>;

    /// Start playing a melody.
    async fn start(&self, id: MelodyId) -> Result<()>;

    /// Stop playing a melody.
    async fn stop(&self, id: MelodyId) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // NoteEvent Constructor Tests
    // =========================================================================

    #[test]
    fn test_note_event_new_has_empty_params() {
        let event = NoteEvent::new(0.0, 60, 0.8, 1.0);
        assert!(event.params.is_empty(), "new() should have empty params");
        assert_eq!(event.note, 60);
        assert_eq!(event.velocity, 0.8);
    }

    #[test]
    fn test_note_event_new_with_params() {
        let mut params = HashMap::new();
        params.insert("cutoff".to_string(), 2000.0_f32);
        params.insert("pan".to_string(), -0.5_f32);

        let event = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params);
        assert_eq!(event.params.len(), 2);
        assert_eq!(*event.params.get("cutoff").unwrap(), 2000.0);
        assert_eq!(*event.params.get("pan").unwrap(), -0.5);
    }

    #[test]
    fn test_note_event_quarter_has_empty_params() {
        let event = NoteEvent::quarter(0.0, 60, 1.0);
        assert!(event.params.is_empty());
        assert_eq!(event.duration, Beat::from_f64(1.0));
    }

    #[test]
    fn test_note_event_eighth_has_empty_params() {
        let event = NoteEvent::eighth(0.0, 60, 1.0);
        assert!(event.params.is_empty());
        assert_eq!(event.duration, Beat::from_f64(0.5));
    }

    #[test]
    fn test_note_event_sixteenth_has_empty_params() {
        let event = NoteEvent::sixteenth(0.0, 60, 1.0);
        assert!(event.params.is_empty());
        assert_eq!(event.duration, Beat::from_f64(0.25));
    }

    // =========================================================================
    // NoteEvent Equality Tests
    // =========================================================================

    #[test]
    fn test_note_event_eq_no_params() {
        let a = NoteEvent::new(0.0, 60, 0.8, 1.0);
        let b = NoteEvent::new(0.0, 60, 0.8, 1.0);
        assert_eq!(a, b, "Identical notes without params should be equal");
    }

    #[test]
    fn test_note_event_eq_with_same_params() {
        let mut params = HashMap::new();
        params.insert("cutoff".to_string(), 2000.0_f32);

        let a = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params.clone());
        let b = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params);
        assert_eq!(a, b, "Notes with same params should be equal");
    }

    #[test]
    fn test_note_event_neq_different_params() {
        let mut params_a = HashMap::new();
        params_a.insert("cutoff".to_string(), 2000.0_f32);

        let mut params_b = HashMap::new();
        params_b.insert("cutoff".to_string(), 3000.0_f32);

        let a = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params_a);
        let b = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params_b);
        assert_ne!(
            a, b,
            "Notes with different param values should not be equal"
        );
    }

    #[test]
    fn test_note_event_neq_extra_params() {
        let mut params_a = HashMap::new();
        params_a.insert("cutoff".to_string(), 2000.0_f32);

        let a = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params_a);
        let b = NoteEvent::new(0.0, 60, 0.8, 1.0); // no params
        assert_ne!(
            a, b,
            "Note with params should not equal note without params"
        );
    }

    #[test]
    fn test_note_event_neq_different_param_keys() {
        let mut params_a = HashMap::new();
        params_a.insert("cutoff".to_string(), 2000.0_f32);

        let mut params_b = HashMap::new();
        params_b.insert("resonance".to_string(), 2000.0_f32);

        let a = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params_a);
        let b = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params_b);
        assert_ne!(a, b, "Notes with different param keys should not be equal");
    }

    #[test]
    fn test_note_event_eq_multiple_params_order_independent() {
        let mut params_a = HashMap::new();
        params_a.insert("cutoff".to_string(), 2000.0_f32);
        params_a.insert("pan".to_string(), -0.5_f32);

        let mut params_b = HashMap::new();
        params_b.insert("pan".to_string(), -0.5_f32);
        params_b.insert("cutoff".to_string(), 2000.0_f32);

        let a = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params_a);
        let b = NoteEvent::new_with_params(0.0, 60, 0.8, 1.0, params_b);
        assert_eq!(a, b, "Param insertion order should not affect equality");
    }

    // =========================================================================
    // MelodyConfig with NoteEvent params Tests
    // =========================================================================

    #[test]
    fn test_melody_config_eq_with_params() {
        let mut params = HashMap::new();
        params.insert("cutoff".to_string(), 2000.0_f32);

        let voice = VoiceId::new(1);
        let config_a = MelodyConfig::with_length("test", voice, 4.0).with_note(
            NoteEvent::new_with_params(0.0, 60, 1.0, 1.0, params.clone()),
        );
        let config_b = MelodyConfig::with_length("test", voice, 4.0)
            .with_note(NoteEvent::new_with_params(0.0, 60, 1.0, 1.0, params));

        assert_eq!(
            config_a, config_b,
            "MelodyConfigs with same params should be equal"
        );
    }

    #[test]
    fn test_melody_config_neq_with_different_params() {
        let mut params_a = HashMap::new();
        params_a.insert("cutoff".to_string(), 2000.0_f32);

        let mut params_b = HashMap::new();
        params_b.insert("cutoff".to_string(), 3000.0_f32);

        let voice = VoiceId::new(1);
        let config_a = MelodyConfig::with_length("test", voice, 4.0)
            .with_note(NoteEvent::new_with_params(0.0, 60, 1.0, 1.0, params_a));
        let config_b = MelodyConfig::with_length("test", voice, 4.0)
            .with_note(NoteEvent::new_with_params(0.0, 60, 1.0, 1.0, params_b));

        assert_ne!(
            config_a, config_b,
            "MelodyConfigs with different params should not be equal (triggers reload)"
        );
    }

    #[test]
    fn test_melody_content_preserves_params() {
        let mut params = HashMap::new();
        params.insert("cutoff".to_string(), 2000.0_f32);
        params.insert("resonance".to_string(), 0.8_f32);

        let voice = VoiceId::new(1);
        let config = MelodyConfig::with_length("test", voice, 4.0)
            .with_note(NoteEvent::new_with_params(0.0, 60, 1.0, 1.0, params));

        let content = MelodyContent::from_config(&config);
        assert_eq!(content.notes.len(), 1);
        assert_eq!(content.notes[0].params.len(), 2);
        assert_eq!(*content.notes[0].params.get("cutoff").unwrap(), 2000.0);
        assert_eq!(*content.notes[0].params.get("resonance").unwrap(), 0.8);
    }
}
