//! Melodies trait for pitched sequences.
//!
//! Melodies are like patterns but include pitch and duration information.

use crate::types::{Beat, MelodyId, VoiceId};
use crate::Result;
use async_trait::async_trait;

/// A single note event in a melody.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteEvent {
    /// Beat position within the melody.
    pub beat: Beat,

    /// MIDI note number (0-127).
    pub note: u8,

    /// Velocity (0.0-1.0).
    pub velocity: f32,

    /// Duration in beats.
    pub duration: Beat,
}

impl NoteEvent {
    /// Create a new note event.
    pub fn new(beat: f64, note: u8, velocity: f32, duration: f64) -> Self {
        Self {
            beat: Beat::from_f64(beat),
            note,
            velocity,
            duration: Beat::from_f64(duration),
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
#[derive(Clone, Debug, PartialEq)]
pub struct MelodyConfig {
    /// Voice to trigger (None if melody is just for MIDI output).
    pub voice: Option<VoiceId>,

    /// Note events in the melody.
    pub notes: Vec<NoteEvent>,

    /// Length of the melody in beats.
    pub length: Beat,

    /// Swing amount (0.0 = none, 1.0 = full).
    pub swing: f32,
}

impl MelodyConfig {
    /// Create a new melody configuration.
    pub fn new(voice: VoiceId, length: Beat) -> Self {
        Self {
            voice: Some(voice),
            notes: Vec::new(),
            length,
            swing: 0.0,
        }
    }

    /// Create a melody configuration without a voice.
    pub fn without_voice(length: Beat) -> Self {
        Self {
            voice: None,
            notes: Vec::new(),
            length,
            swing: 0.0,
        }
    }

    /// Create a melody with length in beats as f64.
    pub fn with_length(voice: VoiceId, length: f64) -> Self {
        Self::new(voice, Beat::from_f64(length))
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

/// Melody management for pitched sequences.
///
/// Melodies loop continuously, sending note-on and note-off events
/// to their voice based on the note events.
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
