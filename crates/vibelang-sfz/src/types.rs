//! SFZ type definitions.

use crate::parser::{LoopMode, TriggerMode};
use std::collections::HashMap;
use std::path::PathBuf;

/// Complete SFZ instrument definition loaded into VibeLang.
#[derive(Clone, Debug)]
pub struct SfzInstrument {
    /// Instrument name.
    pub name: String,
    /// Path to the source SFZ file.
    pub source_file: PathBuf,
    /// All regions in the instrument.
    pub regions: Vec<SfzRegion>,
    /// Global opcodes that apply to all regions.
    pub global_opcodes: HashMap<String, String>,
    /// Control opcodes.
    pub control_opcodes: HashMap<String, String>,
}

impl SfzInstrument {
    /// Get the number of regions in this instrument.
    pub fn num_regions(&self) -> usize {
        self.regions.len()
    }

    /// Get a human-readable info string.
    pub fn info(&self) -> String {
        format!(
            "SFZ Instrument '{}': {} regions from {}",
            self.name,
            self.regions.len(),
            self.source_file.display()
        )
    }
}

/// A single SFZ region with all its parameters.
#[derive(Clone, Debug)]
pub struct SfzRegion {
    /// Buffer ID for the loaded sample in SuperCollider.
    pub buffer_id: i32,
    /// Number of channels in the sample (1 = mono, 2 = stereo).
    pub num_channels: u32,
    /// Path to the sample file.
    pub sample_path: PathBuf,
    /// MIDI key range (lokey, hikey).
    pub key_range: (u8, u8),
    /// Velocity range (lovel, hivel).
    pub vel_range: (u8, u8),
    /// Trigger mode.
    pub trigger: TriggerMode,
    /// Loop mode.
    pub loop_mode: LoopMode,
    /// Loop start point in samples (if looping).
    pub loop_start: Option<u32>,
    /// Loop end point in samples (if looping).
    pub loop_end: Option<u32>,
    /// All SFZ opcodes for this region.
    pub opcodes: SfzRegionOpcodes,
    /// Voice group number (for polyphony management).
    pub group: Option<i64>,
    /// Group to turn off when this region triggers.
    pub off_by: Option<i64>,
    /// Round-robin sequence position.
    pub seq_position: Option<i64>,
    /// Round-robin sequence length.
    pub seq_length: Option<i64>,
    /// Buffer frame count (for duration calculation).
    pub buffer_frames: u32,
    /// Sample rate of the buffer (for duration calculation).
    pub sample_rate: f32,
}

impl SfzRegion {
    /// Get the duration of this region's sample in seconds.
    pub fn duration_seconds(&self) -> f32 {
        self.buffer_frames as f32 / self.sample_rate
    }
}

/// Extracted and parsed SFZ opcodes for easy access.
#[derive(Clone, Debug, Default)]
pub struct SfzRegionOpcodes {
    // Sound source & playback
    /// Sample offset in frames.
    pub offset: Option<u32>,
    /// Random offset range.
    pub offset_random: Option<u32>,
    /// MIDI note of the original sample.
    pub pitch_keycenter: Option<u8>,
    /// Pitch tracking amount (100 = normal).
    pub pitch_keytrack: Option<f32>,
    /// Fine tuning in cents.
    pub tune: Option<f32>,
    /// Transposition in semitones.
    pub transpose: Option<i32>,

    // Amplitude envelope
    /// Attack time in seconds.
    pub ampeg_attack: Option<f32>,
    /// Hold time in seconds.
    pub ampeg_hold: Option<f32>,
    /// Decay time in seconds.
    pub ampeg_decay: Option<f32>,
    /// Sustain level (0-100).
    pub ampeg_sustain: Option<f32>,
    /// Release time in seconds.
    pub ampeg_release: Option<f32>,
    /// Velocity to attack modulation.
    pub ampeg_vel2attack: Option<f32>,
    /// Velocity to decay modulation.
    pub ampeg_vel2decay: Option<f32>,
    /// Velocity to sustain modulation.
    pub ampeg_vel2sustain: Option<f32>,
    /// Velocity to release modulation.
    pub ampeg_vel2release: Option<f32>,

    // Filter
    /// Filter cutoff frequency in Hz.
    pub cutoff: Option<f32>,
    /// Filter resonance in dB.
    pub resonance: Option<f32>,
    /// Filter type.
    pub fil_type: Option<FilterType>,
    /// Filter key tracking in cents.
    pub fil_keytrack: Option<f32>,
    /// Filter key tracking center note.
    pub fil_keycenter: Option<u8>,
    /// Filter velocity tracking in cents per velocity.
    pub fil_veltrack: Option<f32>,

    // Filter envelope
    /// Filter envelope attack time.
    pub fileg_attack: Option<f32>,
    /// Filter envelope hold time.
    pub fileg_hold: Option<f32>,
    /// Filter envelope decay time.
    pub fileg_decay: Option<f32>,
    /// Filter envelope sustain (0-100).
    pub fileg_sustain: Option<f32>,
    /// Filter envelope release time.
    pub fileg_release: Option<f32>,
    /// Filter envelope depth in cents.
    pub fileg_depth: Option<f32>,

    // Pitch envelope
    /// Pitch envelope attack time.
    pub pitcheg_attack: Option<f32>,
    /// Pitch envelope hold time.
    pub pitcheg_hold: Option<f32>,
    /// Pitch envelope decay time.
    pub pitcheg_decay: Option<f32>,
    /// Pitch envelope sustain (0-100).
    pub pitcheg_sustain: Option<f32>,
    /// Pitch envelope release time.
    pub pitcheg_release: Option<f32>,
    /// Pitch envelope depth in cents.
    pub pitcheg_depth: Option<f32>,

    // Performance
    /// Volume in dB.
    pub volume: Option<f32>,
    /// Amplitude (0-100).
    pub amplitude: Option<f32>,
    /// Pan (-100 to 100).
    pub pan: Option<f32>,
    /// Width (0-100).
    pub width: Option<f32>,
    /// Position (0-100).
    pub position: Option<f32>,

    // LFO - Amplitude
    /// Amplitude LFO frequency in Hz.
    pub amplfo_freq: Option<f32>,
    /// Amplitude LFO depth (0-100).
    pub amplfo_depth: Option<f32>,

    // LFO - Filter
    /// Filter LFO frequency in Hz.
    pub fillfo_freq: Option<f32>,
    /// Filter LFO depth in cents.
    pub fillfo_depth: Option<f32>,

    // LFO - Pitch
    /// Pitch LFO frequency in Hz.
    pub pitchlfo_freq: Option<f32>,
    /// Pitch LFO depth in cents.
    pub pitchlfo_depth: Option<f32>,
}

/// Filter types supported by SFZ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterType {
    /// Low-pass 1-pole (6dB/oct).
    Lpf1p,
    /// Low-pass 2-pole (12dB/oct).
    Lpf2p,
    /// Low-pass 4-pole (24dB/oct).
    Lpf4p,
    /// High-pass 1-pole (6dB/oct).
    Hpf1p,
    /// High-pass 2-pole (12dB/oct).
    Hpf2p,
    /// High-pass 4-pole (24dB/oct).
    Hpf4p,
    /// Band-pass 2-pole.
    Bpf2p,
    /// Band-reject 2-pole.
    Brf2p,
}

impl FilterType {
    /// Parse a filter type from a string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "lpf_1p" | "lpf1p" => Some(Self::Lpf1p),
            "lpf_2p" | "lpf2p" => Some(Self::Lpf2p),
            "lpf_4p" | "lpf4p" => Some(Self::Lpf4p),
            "hpf_1p" | "hpf1p" => Some(Self::Hpf1p),
            "hpf_2p" | "hpf2p" => Some(Self::Hpf2p),
            "hpf_4p" | "hpf4p" => Some(Self::Hpf4p),
            "bpf_2p" | "bpf2p" => Some(Self::Bpf2p),
            "brf_2p" | "brf2p" => Some(Self::Brf2p),
            _ => None,
        }
    }
}

/// Round-robin state for a voice.
#[derive(Clone, Debug, Default)]
pub struct RoundRobinState {
    /// Current round-robin positions for each sequence (by group/key).
    pub positions: HashMap<String, i64>,
}

impl RoundRobinState {
    /// Create a new round-robin state.
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
        }
    }

    /// Get the next position for a given key and advance the counter.
    pub fn next_position(&mut self, key: &str, length: i64) -> i64 {
        let pos = self.positions.entry(key.to_string()).or_insert(1);
        let current = *pos;
        *pos = if current >= length { 1 } else { current + 1 };
        current
    }

    /// Reset all round-robin counters.
    pub fn reset(&mut self) {
        self.positions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_robin_state() {
        let mut rr = RoundRobinState::new();

        // First call returns 1
        assert_eq!(rr.next_position("test", 4), 1);
        assert_eq!(rr.next_position("test", 4), 2);
        assert_eq!(rr.next_position("test", 4), 3);
        assert_eq!(rr.next_position("test", 4), 4);
        // Wraps around
        assert_eq!(rr.next_position("test", 4), 1);
    }

    #[test]
    fn test_filter_type_from_str() {
        assert_eq!(FilterType::from_str("lpf_2p"), Some(FilterType::Lpf2p));
        assert_eq!(FilterType::from_str("hpf2p"), Some(FilterType::Hpf2p));
        assert_eq!(FilterType::from_str("unknown"), None);
    }
}
