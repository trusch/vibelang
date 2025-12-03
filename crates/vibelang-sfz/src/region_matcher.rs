//! Region matching logic for SFZ instruments.

use crate::parser::TriggerMode;
use crate::types::*;
use std::collections::HashMap;

/// Find all regions that match the given criteria.
///
/// This function handles:
/// - Key range matching
/// - Velocity range matching
/// - Trigger mode filtering
/// - Round-robin sample selection
///
/// # Arguments
///
/// * `instrument` - The SFZ instrument to search
/// * `note` - MIDI note number (0-127)
/// * `velocity` - MIDI velocity (0-127)
/// * `trigger` - Trigger mode to match (Attack, Release, etc.)
/// * `round_robin_state` - Mutable round-robin state for cycling samples
///
/// # Returns
///
/// A vector of references to matching regions.
pub fn find_matching_regions<'a>(
    instrument: &'a SfzInstrument,
    note: u8,
    velocity: u8,
    trigger: TriggerMode,
    round_robin_state: &mut RoundRobinState,
) -> Vec<&'a SfzRegion> {
    // First, collect all candidate regions (matching key, velocity, trigger)
    // and group them by their round-robin sequence key
    let mut rr_groups: HashMap<String, Vec<&'a SfzRegion>> = HashMap::new();
    let mut non_rr_regions: Vec<&'a SfzRegion> = Vec::new();

    for region in &instrument.regions {
        // Check basic criteria
        if note < region.key_range.0 || note > region.key_range.1 {
            continue;
        }
        if velocity < region.vel_range.0 || velocity > region.vel_range.1 {
            continue;
        }
        if region.trigger != trigger {
            continue;
        }

        // Check if this region has round-robin (seq_length indicates round-robin)
        // Note: seq_position defaults to 1 if not specified in SFZ
        if region.seq_length.is_some() {
            let rr_key = format!("{}_{}", note, region.group.unwrap_or(0));
            rr_groups.entry(rr_key).or_default().push(region);
        } else {
            non_rr_regions.push(region);
        }
    }

    // Now select one region from each round-robin group
    let mut result = non_rr_regions;

    for (rr_key, regions) in rr_groups {
        if regions.is_empty() {
            continue;
        }

        // Get the sequence length from the first region (they should all have the same)
        let seq_len = regions[0].seq_length.unwrap_or(1);

        // Get the current round-robin position (advances the counter ONCE per group)
        let current_pos = round_robin_state.next_position(&rr_key, seq_len);

        // Find the region that matches this position
        // Note: seq_position defaults to 1 if not specified in SFZ file
        for region in regions {
            let region_pos = region.seq_position.unwrap_or(1);
            if region_pos == current_pos {
                result.push(region);
                break; // Only one region per round-robin group
            }
        }
    }

    result
}

/// Convert MIDI note number to frequency in Hz.
///
/// Uses the standard A4 = 440 Hz tuning.
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}

/// Convert frequency in Hz to MIDI note number (rounded).
pub fn freq_to_midi(freq: f32) -> u8 {
    let midi_float = 69.0 + 12.0 * (freq / 440.0).log2();
    midi_float.round().clamp(0.0, 127.0) as u8
}

/// Convert velocity (0-127) to amplitude (0.0-1.0).
pub fn velocity_to_amp(velocity: u8) -> f32 {
    velocity as f32 / 127.0
}

/// Convert dB to linear amplitude.
pub fn db_to_amp(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Convert SFZ pan (-100 to 100) to SuperCollider pan (-1.0 to 1.0).
pub fn sfz_pan_to_sc(pan: f32) -> f32 {
    pan / 100.0
}

/// Calculate the playback rate for a sample.
///
/// Given the target note and the sample's pitch keycenter,
/// calculates the rate needed to pitch-shift the sample.
///
/// # Arguments
///
/// * `target_note` - The MIDI note to play
/// * `pitch_keycenter` - The MIDI note the sample was recorded at
/// * `tune_cents` - Optional fine tuning in cents
/// * `transpose` - Optional transposition in semitones
///
/// # Returns
///
/// The playback rate multiplier (1.0 = original pitch).
pub fn calculate_playback_rate(
    target_note: u8,
    pitch_keycenter: Option<u8>,
    tune_cents: Option<f32>,
    transpose: Option<i32>,
) -> f32 {
    // Default pitch_keycenter to the target note (no pitch shift)
    let root_note = pitch_keycenter.unwrap_or(target_note);

    // Calculate base rate from note difference
    let note_diff = target_note as f32 - root_note as f32;
    let semitone_rate = 2.0_f32.powf(note_diff / 12.0);

    // Apply transposition
    let transpose_rate = if let Some(t) = transpose {
        2.0_f32.powf(t as f32 / 12.0)
    } else {
        1.0
    };

    // Apply fine tuning
    let tune_rate = if let Some(cents) = tune_cents {
        2.0_f32.powf(cents / 1200.0)
    } else {
        1.0
    };

    semitone_rate * transpose_rate * tune_rate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_to_freq() {
        // A4 = 440 Hz
        assert!((midi_to_freq(69) - 440.0).abs() < 0.01);

        // C4 = ~261.63 Hz
        assert!((midi_to_freq(60) - 261.63).abs() < 0.01);

        // A5 = 880 Hz (one octave up from A4)
        assert!((midi_to_freq(81) - 880.0).abs() < 0.01);
    }

    #[test]
    fn test_freq_to_midi() {
        assert_eq!(freq_to_midi(440.0), 69);
        assert_eq!(freq_to_midi(880.0), 81);
        assert_eq!(freq_to_midi(261.63), 60);
    }

    #[test]
    fn test_velocity_to_amp() {
        assert!((velocity_to_amp(0) - 0.0).abs() < 0.01);
        assert!((velocity_to_amp(127) - 1.0).abs() < 0.01);
        assert!((velocity_to_amp(64) - 0.504).abs() < 0.01);
    }

    #[test]
    fn test_db_to_amp() {
        // 0 dB = 1.0
        assert!((db_to_amp(0.0) - 1.0).abs() < 0.01);

        // -6 dB ~ 0.5
        assert!((db_to_amp(-6.0) - 0.5).abs() < 0.01);

        // -12 dB ~ 0.25
        assert!((db_to_amp(-12.0) - 0.25).abs() < 0.01);
    }

    #[test]
    fn test_sfz_pan_to_sc() {
        assert!((sfz_pan_to_sc(0.0) - 0.0).abs() < 0.01);
        assert!((sfz_pan_to_sc(-100.0) - (-1.0)).abs() < 0.01);
        assert!((sfz_pan_to_sc(100.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_playback_rate() {
        // Same note = 1.0
        assert!((calculate_playback_rate(60, Some(60), None, None) - 1.0).abs() < 0.001);

        // One octave up = 2.0
        assert!((calculate_playback_rate(72, Some(60), None, None) - 2.0).abs() < 0.001);

        // One octave down = 0.5
        assert!((calculate_playback_rate(48, Some(60), None, None) - 0.5).abs() < 0.001);

        // With transpose
        assert!(
            (calculate_playback_rate(60, Some(60), None, Some(12)) - 2.0).abs() < 0.001
        );
    }
}
