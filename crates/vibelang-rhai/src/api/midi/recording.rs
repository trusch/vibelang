//! MIDI recording handle for Rhai scripts.

use rhai::{Array, CustomType, Dynamic, TypeBuilder};
use vibelang_core::types::MidiDeviceId;

/// Handle to a MIDI recording for use in Rhai scripts.
#[derive(Debug, Clone, CustomType)]
pub struct MidiRecordingHandle {
    /// Device ID.
    pub device_id: MidiDeviceId,

    /// Number of notes recorded.
    pub note_count: i64,

    /// Number of CC events recorded.
    pub cc_count: i64,

    /// Duration in beats.
    pub duration_beats: f64,

    /// The actual note events: (beat, note, velocity, duration).
    pub notes: Vec<(f64, i64, i64, f64)>,
}

impl MidiRecordingHandle {
    /// Get the number of notes.
    pub fn get_note_count(&mut self) -> i64 {
        self.note_count
    }

    /// Get the number of CC events.
    pub fn get_cc_count(&mut self) -> i64 {
        self.cc_count
    }

    /// Get the duration in beats.
    pub fn get_duration(&mut self) -> f64 {
        self.duration_beats
    }

    /// Get the note events as an array.
    pub fn get_notes(&mut self) -> Array {
        use rhai::Map;
        self.notes
            .iter()
            .map(|(beat, note, vel, dur)| {
                let mut map = Map::new();
                map.insert("beat".into(), Dynamic::from(*beat));
                map.insert("note".into(), Dynamic::from(*note));
                map.insert("velocity".into(), Dynamic::from(*vel));
                map.insert("duration".into(), Dynamic::from(*dur));
                Dynamic::from(map)
            })
            .collect()
    }

    /// Convert to a pattern string (quantized).
    ///
    /// # Arguments
    /// * `quantize` - Grid size in beats (e.g., 0.25 for 16th notes)
    ///
    /// Returns a pattern string like "x..x..x." where digits represent velocity levels.
    pub fn to_pattern_string(&self, quantize: f64) -> String {
        if self.notes.is_empty() {
            return ".".to_string();
        }

        // Use 16th notes by default
        let grid = if quantize > 0.0 { quantize } else { 0.25 };

        // Calculate total steps (round up to nearest bar)
        let duration = self.duration_beats.max(4.0);
        let bars = (duration / 4.0).ceil();
        let num_steps = (bars * 4.0 / grid) as usize;

        // Create step array
        let mut steps: Vec<u8> = vec![0; num_steps];

        // Place notes at their quantized positions
        for (beat, _note, vel, _dur) in &self.notes {
            let step_idx = ((beat / grid).round() as usize).min(num_steps.saturating_sub(1));
            // Convert velocity (0-127) to level (1-9)
            let level = ((*vel as f32 / 127.0) * 9.0).round() as u8;
            // Take maximum velocity if multiple notes on same step
            steps[step_idx] = steps[step_idx].max(level.max(1));
        }

        // Convert to pattern string
        steps
            .iter()
            .map(|&v| {
                if v == 0 {
                    '.'
                } else {
                    char::from_digit(v as u32, 10).unwrap_or('x')
                }
            })
            .collect()
    }
}
