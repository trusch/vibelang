//! MIDI recording functionality.
//!
//! This module provides recording of MIDI input events with beat timestamps,
//! allowing users to capture performances and convert them to patterns.
//!
//! # Example
//!
//! ```ignore
//! // Start recording
//! midi_handler.start_recording(device_id).await?;
//!
//! // ... user plays notes ...
//!
//! // Stop recording and get events
//! let recording = midi_handler.stop_recording(device_id).await?;
//!
//! // Convert to pattern with quantization
//! let pattern_config = recording.to_pattern("recorded_pattern", 0.25, voice_id); // Quantize to 16th notes
//! ```

use crate::traits::{PatternConfig, Step};
use crate::types::{Beat, MidiDeviceId, VoiceId};
use std::time::Instant;

/// A recorded MIDI note event with timing information.
#[derive(Clone, Debug)]
pub struct RecordedMidiNote {
    /// MIDI note number (0-127).
    pub note: u8,

    /// MIDI velocity (0-127).
    pub velocity: u8,

    /// MIDI channel (0-15).
    pub channel: u8,

    /// Beat position when the note was triggered.
    pub beat: Beat,

    /// Duration in beats (if note-off was recorded, else None).
    pub duration: Option<Beat>,

    /// Wall-clock instant when the note was recorded.
    pub recorded_at: Instant,
}

impl RecordedMidiNote {
    /// Create a new recorded note (note-on event).
    pub fn new(note: u8, velocity: u8, channel: u8, beat: Beat) -> Self {
        Self {
            note,
            velocity,
            channel,
            beat,
            duration: None,
            recorded_at: Instant::now(),
        }
    }

    /// Set the duration when note-off is received.
    pub fn with_duration(mut self, duration: Beat) -> Self {
        self.duration = Some(duration);
        self
    }
}

/// A recorded MIDI CC event with timing information.
#[derive(Clone, Debug)]
pub struct RecordedMidiCc {
    /// CC number (0-127).
    pub cc: u8,

    /// CC value (0-127).
    pub value: u8,

    /// MIDI channel (0-15).
    pub channel: u8,

    /// Beat position when the CC was received.
    pub beat: Beat,

    /// Wall-clock instant when the CC was recorded.
    pub recorded_at: Instant,
}

impl RecordedMidiCc {
    /// Create a new recorded CC event.
    pub fn new(cc: u8, value: u8, channel: u8, beat: Beat) -> Self {
        Self {
            cc,
            value,
            channel,
            beat,
            recorded_at: Instant::now(),
        }
    }
}

/// A complete MIDI recording session.
#[derive(Clone, Debug)]
pub struct MidiRecording {
    /// Device this recording is from.
    pub device_id: MidiDeviceId,

    /// Recorded note events.
    pub notes: Vec<RecordedMidiNote>,

    /// Recorded CC events.
    pub cc_events: Vec<RecordedMidiCc>,

    /// Beat position when recording started.
    pub start_beat: Beat,

    /// Beat position when recording stopped (None if still recording).
    pub end_beat: Option<Beat>,

    /// Wall-clock instant when recording started.
    pub started_at: Instant,

    /// Whether recording is currently active.
    pub is_recording: bool,

    /// Optional channel filter (None = all channels).
    pub channel_filter: Option<u8>,

    /// Pending note-on events awaiting note-off (note -> index in notes vec).
    pending_notes: std::collections::HashMap<(u8, u8), usize>, // (note, channel) -> index
}

impl Default for MidiRecording {
    fn default() -> Self {
        Self {
            device_id: MidiDeviceId::default(),
            notes: Vec::new(),
            cc_events: Vec::new(),
            start_beat: Beat::ZERO,
            end_beat: None,
            started_at: Instant::now(),
            is_recording: false,
            channel_filter: None,
            pending_notes: std::collections::HashMap::new(),
        }
    }
}

impl MidiRecording {
    /// Create a new recording for the given device.
    pub fn new(device_id: MidiDeviceId, start_beat: Beat) -> Self {
        Self {
            device_id,
            notes: Vec::new(),
            cc_events: Vec::new(),
            start_beat,
            end_beat: None,
            started_at: Instant::now(),
            is_recording: true,
            channel_filter: None,
            pending_notes: std::collections::HashMap::new(),
        }
    }

    /// Create a new recording with a channel filter.
    pub fn with_channel_filter(mut self, channel: u8) -> Self {
        self.channel_filter = Some(channel);
        self
    }

    /// Record a note-on event.
    pub fn record_note_on(&mut self, note: u8, velocity: u8, channel: u8, current_beat: Beat) {
        if !self.is_recording {
            return;
        }

        // Check channel filter
        if let Some(filter) = self.channel_filter {
            if channel != filter {
                return;
            }
        }

        // Calculate beat relative to recording start
        let relative_beat = Beat::from_f64(current_beat.to_f64() - self.start_beat.to_f64());

        let recorded_note = RecordedMidiNote::new(note, velocity, channel, relative_beat);
        let index = self.notes.len();
        self.notes.push(recorded_note);

        // Track this note-on for duration calculation
        self.pending_notes.insert((note, channel), index);
    }

    /// Record a note-off event (updates duration of matching note-on).
    pub fn record_note_off(&mut self, note: u8, channel: u8, current_beat: Beat) {
        if !self.is_recording {
            return;
        }

        // Check channel filter
        if let Some(filter) = self.channel_filter {
            if channel != filter {
                return;
            }
        }

        // Find the pending note-on and set its duration
        if let Some(index) = self.pending_notes.remove(&(note, channel)) {
            if let Some(recorded_note) = self.notes.get_mut(index) {
                let relative_beat =
                    Beat::from_f64(current_beat.to_f64() - self.start_beat.to_f64());
                let duration = Beat::from_f64(relative_beat.to_f64() - recorded_note.beat.to_f64());
                recorded_note.duration = Some(duration);
            }
        }
    }

    /// Record a CC event.
    pub fn record_cc(&mut self, cc: u8, value: u8, channel: u8, current_beat: Beat) {
        if !self.is_recording {
            return;
        }

        // Check channel filter
        if let Some(filter) = self.channel_filter {
            if channel != filter {
                return;
            }
        }

        let relative_beat = Beat::from_f64(current_beat.to_f64() - self.start_beat.to_f64());
        self.cc_events
            .push(RecordedMidiCc::new(cc, value, channel, relative_beat));
    }

    /// Stop recording and finalize.
    pub fn stop(&mut self, end_beat: Beat) {
        self.is_recording = false;
        self.end_beat = Some(end_beat);

        // Close any pending notes with duration to end
        let relative_end = Beat::from_f64(end_beat.to_f64() - self.start_beat.to_f64());
        for (_key, index) in self.pending_notes.drain() {
            if let Some(recorded_note) = self.notes.get_mut(index) {
                if recorded_note.duration.is_none() {
                    let duration =
                        Beat::from_f64(relative_end.to_f64() - recorded_note.beat.to_f64());
                    recorded_note.duration = Some(duration);
                }
            }
        }
    }

    /// Get the total duration of the recording in beats.
    pub fn duration(&self) -> Beat {
        match self.end_beat {
            Some(end) => Beat::from_f64(end.to_f64() - self.start_beat.to_f64()),
            None => Beat::ZERO,
        }
    }

    /// Get the number of recorded notes.
    pub fn note_count(&self) -> usize {
        self.notes.len()
    }

    /// Get the number of recorded CC events.
    pub fn cc_count(&self) -> usize {
        self.cc_events.len()
    }

    /// Quantize a beat value to the nearest grid position.
    fn quantize_beat(beat: f64, grid: f64) -> f64 {
        if grid <= 0.0 {
            beat
        } else {
            (beat / grid).round() * grid
        }
    }

    /// Convert the recording to a PatternConfig.
    ///
    /// # Arguments
    ///
    /// * `name` - Name for the pattern.
    /// * `quantize_beats` - Grid size for quantization (e.g., 0.25 for 16th notes).
    ///   Use 0.0 for no quantization.
    /// * `voice` - The voice ID to assign to the pattern.
    pub fn to_pattern(
        &self,
        name: impl Into<String>,
        quantize_beats: f64,
        voice: VoiceId,
    ) -> PatternConfig {
        let duration = self.duration();
        let length = if quantize_beats > 0.0 {
            // Round up to nearest bar
            let bars = (duration.to_f64() / 4.0).ceil();
            Beat::from_f64(bars * 4.0)
        } else {
            duration
        };

        let mut config = PatternConfig::new(name, voice, length);

        for note in &self.notes {
            let beat_pos = if quantize_beats > 0.0 {
                Self::quantize_beat(note.beat.to_f64(), quantize_beats)
            } else {
                note.beat.to_f64()
            };

            // Create step with note parameters
            let mut step = Step::at(beat_pos)
                .with_param("note", note.note as f32)
                .with_param("velocity", note.velocity as f32 / 127.0);

            // Add duration if available
            if let Some(dur) = note.duration {
                let dur_val = if quantize_beats > 0.0 {
                    Self::quantize_beat(dur.to_f64(), quantize_beats)
                } else {
                    dur.to_f64()
                };
                step = step.with_param("gate", dur_val.max(0.1) as f32);
            }

            config.steps.push(step);
        }

        // Sort steps by beat position
        config
            .steps
            .sort_by(|a, b| a.beat.to_f64().partial_cmp(&b.beat.to_f64()).unwrap_or(std::cmp::Ordering::Equal));

        config
    }

    /// Convert the recording to a melody-style output with pitch information.
    ///
    /// Returns a vector of (beat, note, velocity, duration) tuples.
    pub fn to_note_events(&self, quantize_beats: f64) -> Vec<(f64, u8, u8, f64)> {
        let mut events = Vec::new();

        for note in &self.notes {
            let beat_pos = if quantize_beats > 0.0 {
                Self::quantize_beat(note.beat.to_f64(), quantize_beats)
            } else {
                note.beat.to_f64()
            };

            let duration = note
                .duration
                .map(|d| {
                    if quantize_beats > 0.0 {
                        Self::quantize_beat(d.to_f64(), quantize_beats).max(quantize_beats)
                    } else {
                        d.to_f64()
                    }
                })
                .unwrap_or(0.25);

            events.push((beat_pos, note.note, note.velocity, duration));
        }

        // Sort by beat position
        events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        events
    }
}

/// Information about an active or completed MIDI recording.
#[derive(Clone, Debug)]
pub struct MidiRecordingInfo {
    /// Device ID.
    pub device_id: MidiDeviceId,

    /// Whether currently recording.
    pub is_recording: bool,

    /// Number of notes recorded.
    pub note_count: usize,

    /// Number of CC events recorded.
    pub cc_count: usize,

    /// Duration in beats (0 if still recording).
    pub duration_beats: f64,

    /// Start beat.
    pub start_beat: f64,
}

impl From<&MidiRecording> for MidiRecordingInfo {
    fn from(recording: &MidiRecording) -> Self {
        Self {
            device_id: recording.device_id,
            is_recording: recording.is_recording,
            note_count: recording.note_count(),
            cc_count: recording.cc_count(),
            duration_beats: recording.duration().to_f64(),
            start_beat: recording.start_beat.to_f64(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recording_basic() {
        let device_id = MidiDeviceId::new(0);
        let mut recording = MidiRecording::new(device_id, Beat::ZERO);

        // Record a note
        recording.record_note_on(60, 100, 0, Beat::from_f64(0.0));
        recording.record_note_off(60, 0, Beat::from_f64(0.5));

        assert_eq!(recording.note_count(), 1);
        assert!(recording.notes[0].duration.is_some());
        assert!((recording.notes[0].duration.unwrap().to_f64() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_recording_quantization() {
        let device_id = MidiDeviceId::new(0);
        let mut recording = MidiRecording::new(device_id, Beat::ZERO);

        // Record notes at non-quantized positions
        // With grid=0.25, beat/grid rounds: 0.1/0.25=0.4 -> 0, 0.55/0.25=2.2 -> 2
        recording.record_note_on(60, 100, 0, Beat::from_f64(0.1));
        recording.record_note_off(60, 0, Beat::from_f64(0.35));

        recording.record_note_on(64, 100, 0, Beat::from_f64(0.55));
        recording.record_note_off(64, 0, Beat::from_f64(0.8));

        recording.stop(Beat::from_f64(1.0));

        // Convert to pattern with 1/4 note quantization
        let pattern = recording.to_pattern("test_pattern", 0.25, VoiceId::new(1));

        // First note should be at 0.0 (0.1/0.25=0.4 rounds to 0)
        // Second note should be at 0.5 (0.55/0.25=2.2 rounds to 2, * 0.25 = 0.5)
        assert_eq!(pattern.steps.len(), 2);
        assert!((pattern.steps[0].beat.to_f64() - 0.0).abs() < 0.001);
        assert!((pattern.steps[1].beat.to_f64() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_recording_channel_filter() {
        let device_id = MidiDeviceId::new(0);
        let mut recording = MidiRecording::new(device_id, Beat::ZERO).with_channel_filter(0);

        // Record notes on different channels
        recording.record_note_on(60, 100, 0, Beat::from_f64(0.0)); // Should be recorded
        recording.record_note_on(64, 100, 1, Beat::from_f64(0.5)); // Should be filtered out

        assert_eq!(recording.note_count(), 1);
    }

    #[test]
    fn test_to_note_events() {
        let device_id = MidiDeviceId::new(0);
        let mut recording = MidiRecording::new(device_id, Beat::ZERO);

        recording.record_note_on(60, 100, 0, Beat::from_f64(0.0));
        recording.record_note_off(60, 0, Beat::from_f64(0.5));
        recording.record_note_on(64, 80, 0, Beat::from_f64(1.0));
        recording.record_note_off(64, 0, Beat::from_f64(1.5));

        recording.stop(Beat::from_f64(2.0));

        let events = recording.to_note_events(0.0);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], (0.0, 60, 100, 0.5));
        assert_eq!(events[1], (1.0, 64, 80, 0.5));
    }
}
