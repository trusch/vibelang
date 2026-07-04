//! Validation utilities for vibelang-core.
//!
//! This module provides validation functions for configuration types,
//! ensuring values are within valid ranges before they're used in the runtime.
//!
//! # Validation Philosophy
//!
//! We validate at creation time rather than failing silently later:
//! - Users get immediate, clear error messages
//! - Invalid state never enters the runtime
//! - Debugging is easier since errors point to the source

use crate::error::{Error, Result};
#[cfg(not(target_arch = "wasm32"))]
use crate::traits::RecordingConfig;
use crate::traits::{
    Clip, FadeConfig, MelodyConfig, NoteEvent, PatternConfig, SampleConfig, SequenceConfig,
    VoiceConfig,
};
use crate::types::Beat;

/// Validation constants
pub mod limits {
    /// Minimum allowed BPM.
    pub const MIN_BPM: f64 = 1.0;
    /// Maximum allowed BPM.
    pub const MAX_BPM: f64 = 999.0;

    /// Minimum pattern/melody length in beats.
    pub const MIN_LENGTH_BEATS: f64 = 0.0625; // 1/16th beat
    /// Maximum pattern/melody length in beats.
    pub const MAX_LENGTH_BEATS: f64 = 65536.0; // ~273 bars at 240 BPM

    /// Maximum polyphony per voice.
    pub const MAX_POLYPHONY: u8 = 128;
    /// Minimum polyphony per voice.
    pub const MIN_POLYPHONY: u8 = 1;

    /// Maximum MIDI note number.
    pub const MAX_MIDI_NOTE: u8 = 127;
    /// Maximum MIDI channel (0-indexed).
    pub const MAX_MIDI_CHANNEL: u8 = 15;
    /// Maximum MIDI CC number.
    pub const MAX_MIDI_CC: u8 = 127;

    /// Maximum swing amount.
    pub const MAX_SWING: f32 = 1.0;
    /// Minimum swing amount.
    pub const MIN_SWING: f32 = 0.0;

    /// Minimum velocity.
    pub const MIN_VELOCITY: f32 = 0.0;
    /// Maximum velocity.
    pub const MAX_VELOCITY: f32 = 1.0;

    /// Maximum recording duration in seconds (1 hour).
    pub const MAX_RECORDING_SECONDS: f64 = 3600.0;
    /// Maximum recording duration in beats.
    pub const MAX_RECORDING_BEATS: f64 = 65536.0;

    /// Maximum number of audio channels for recording.
    pub const MAX_CHANNELS: u8 = 32;
}

/// Trait for validating configuration types.
pub trait Validate {
    /// Validate the configuration, returning an error if invalid.
    fn validate(&self) -> Result<()>;
}

impl Validate for VoiceConfig {
    fn validate(&self) -> Result<()> {
        if !self.has_sound_source() {
            return Err(Error::invalid_param(
                "sound_source",
                format!(
                    "voice '{}' must have a sound source: use .synth(\"name\"), .on(\"synth_or_sample\"), .on(sample), .on(sfz_instrument), or .on(midi_device)",
                    self.name
                ),
            ));
        }

        // Validate polyphony
        if self.polyphony < limits::MIN_POLYPHONY {
            return Err(Error::invalid_param(
                "polyphony",
                format!(
                    "polyphony must be at least {}, got {}",
                    limits::MIN_POLYPHONY,
                    self.polyphony
                ),
            ));
        }
        if self.polyphony > limits::MAX_POLYPHONY {
            return Err(Error::invalid_param(
                "polyphony",
                format!(
                    "polyphony must be at most {}, got {}",
                    limits::MAX_POLYPHONY,
                    self.polyphony
                ),
            ));
        }

        // Validate MIDI channel (feature-gated)
        #[cfg(feature = "midi")]
        if self.midi_channel > limits::MAX_MIDI_CHANNEL {
            return Err(Error::invalid_param(
                "midi_channel",
                format!(
                    "MIDI channel must be 0-{}, got {}",
                    limits::MAX_MIDI_CHANNEL,
                    self.midi_channel
                ),
            ));
        }

        Ok(())
    }
}

impl Validate for PatternConfig {
    fn validate(&self) -> Result<()> {
        // Validate length
        let length_beats = self.length.to_f64();
        if length_beats < limits::MIN_LENGTH_BEATS {
            return Err(Error::invalid_param(
                "length",
                format!(
                    "pattern length must be at least {} beats, got {}",
                    limits::MIN_LENGTH_BEATS,
                    length_beats
                ),
            ));
        }
        if length_beats > limits::MAX_LENGTH_BEATS {
            return Err(Error::invalid_param(
                "length",
                format!(
                    "pattern length must be at most {} beats, got {}",
                    limits::MAX_LENGTH_BEATS,
                    length_beats
                ),
            ));
        }

        // Validate swing
        if self.swing < limits::MIN_SWING || self.swing > limits::MAX_SWING {
            return Err(Error::invalid_param(
                "swing",
                format!(
                    "swing must be between {} and {}, got {}",
                    limits::MIN_SWING,
                    limits::MAX_SWING,
                    self.swing
                ),
            ));
        }

        // Validate steps
        for (i, step) in self.steps.iter().enumerate() {
            let step_beat = step.beat.to_f64();
            if step_beat < 0.0 {
                return Err(Error::invalid_param(
                    "step",
                    format!("step {} beat position cannot be negative: {}", i, step_beat),
                ));
            }
            if step_beat >= length_beats {
                return Err(Error::invalid_param(
                    "step",
                    format!(
                        "step {} beat position {} exceeds pattern length {}",
                        i, step_beat, length_beats
                    ),
                ));
            }
        }

        Ok(())
    }
}

impl Validate for MelodyConfig {
    fn validate(&self) -> Result<()> {
        // Validate length
        let length_beats = self.length.to_f64();
        if length_beats < limits::MIN_LENGTH_BEATS {
            return Err(Error::invalid_param(
                "length",
                format!(
                    "melody length must be at least {} beats, got {}",
                    limits::MIN_LENGTH_BEATS,
                    length_beats
                ),
            ));
        }
        if length_beats > limits::MAX_LENGTH_BEATS {
            return Err(Error::invalid_param(
                "length",
                format!(
                    "melody length must be at most {} beats, got {}",
                    limits::MAX_LENGTH_BEATS,
                    length_beats
                ),
            ));
        }

        // Validate swing
        if self.swing < limits::MIN_SWING || self.swing > limits::MAX_SWING {
            return Err(Error::invalid_param(
                "swing",
                format!(
                    "swing must be between {} and {}, got {}",
                    limits::MIN_SWING,
                    limits::MAX_SWING,
                    self.swing
                ),
            ));
        }

        // Validate notes
        for (i, note) in self.notes.iter().enumerate() {
            validate_note_event(note, i, length_beats)?;
        }

        Ok(())
    }
}

/// Validate a single note event.
fn validate_note_event(note: &NoteEvent, index: usize, melody_length: f64) -> Result<()> {
    // Validate note number
    if note.note > limits::MAX_MIDI_NOTE {
        return Err(Error::invalid_param(
            "note",
            format!(
                "note {} has invalid MIDI note number {}, max is {}",
                index,
                note.note,
                limits::MAX_MIDI_NOTE
            ),
        ));
    }

    // Validate velocity
    if note.velocity < limits::MIN_VELOCITY || note.velocity > limits::MAX_VELOCITY {
        return Err(Error::invalid_param(
            "velocity",
            format!(
                "note {} velocity {} must be between {} and {}",
                index,
                note.velocity,
                limits::MIN_VELOCITY,
                limits::MAX_VELOCITY
            ),
        ));
    }

    // Validate beat position
    let beat_pos = note.beat.to_f64();
    if beat_pos < 0.0 {
        return Err(Error::invalid_param(
            "beat",
            format!(
                "note {} beat position cannot be negative: {}",
                index, beat_pos
            ),
        ));
    }
    if beat_pos >= melody_length {
        return Err(Error::invalid_param(
            "beat",
            format!(
                "note {} beat position {} exceeds melody length {}",
                index, beat_pos, melody_length
            ),
        ));
    }

    // Validate duration
    let duration = note.duration.to_f64();
    if duration <= 0.0 {
        return Err(Error::invalid_param(
            "duration",
            format!("note {} duration must be positive, got {}", index, duration),
        ));
    }

    Ok(())
}

impl Validate for FadeConfig {
    fn validate(&self) -> Result<()> {
        // Validate duration
        let duration_beats = self.duration.to_beats();
        if duration_beats <= 0.0 {
            return Err(Error::invalid_param(
                "duration",
                format!("fade duration must be positive, got {}", duration_beats),
            ));
        }
        if duration_beats > limits::MAX_LENGTH_BEATS {
            return Err(Error::invalid_param(
                "duration",
                format!(
                    "fade duration must be at most {} beats, got {}",
                    limits::MAX_LENGTH_BEATS,
                    duration_beats
                ),
            ));
        }

        // Validate parameter name is not empty
        if self.param.is_empty() {
            return Err(Error::invalid_param(
                "param",
                "fade parameter name cannot be empty",
            ));
        }

        Ok(())
    }
}

impl Validate for SequenceConfig {
    fn validate(&self) -> Result<()> {
        // Validate length
        let length_beats = self.length.to_f64();
        if length_beats < limits::MIN_LENGTH_BEATS {
            return Err(Error::invalid_param(
                "length",
                format!(
                    "sequence length must be at least {} beats, got {}",
                    limits::MIN_LENGTH_BEATS,
                    length_beats
                ),
            ));
        }
        if length_beats > limits::MAX_LENGTH_BEATS {
            return Err(Error::invalid_param(
                "length",
                format!(
                    "sequence length must be at most {} beats, got {}",
                    limits::MAX_LENGTH_BEATS,
                    length_beats
                ),
            ));
        }

        // Validate clips
        for (i, clip) in self.clips.iter().enumerate() {
            validate_clip(clip, i, length_beats)?;
        }

        Ok(())
    }
}

/// Validate a single clip in a sequence.
fn validate_clip(clip: &Clip, index: usize, sequence_length: f64) -> Result<()> {
    match clip {
        Clip::Pattern { start, end, .. } | Clip::Melody { start, end, .. } => {
            let start_beats = start.to_f64();
            let end_beats = end.to_f64();

            if start_beats < 0.0 {
                return Err(Error::invalid_param(
                    "clip",
                    format!(
                        "clip {} start position cannot be negative: {}",
                        index, start_beats
                    ),
                ));
            }
            if end_beats <= start_beats {
                return Err(Error::invalid_param(
                    "clip",
                    format!(
                        "clip {} end position {} must be greater than start {}",
                        index, end_beats, start_beats
                    ),
                ));
            }
            if end_beats > sequence_length {
                return Err(Error::invalid_param(
                    "clip",
                    format!(
                        "clip {} end position {} exceeds sequence length {}",
                        index, end_beats, sequence_length
                    ),
                ));
            }
        }
        Clip::Fade { start, .. } | Clip::Sequence { start, .. } => {
            let start_beats = start.to_f64();
            if start_beats < 0.0 {
                return Err(Error::invalid_param(
                    "clip",
                    format!(
                        "clip {} start position cannot be negative: {}",
                        index, start_beats
                    ),
                ));
            }
            if start_beats >= sequence_length {
                return Err(Error::invalid_param(
                    "clip",
                    format!(
                        "clip {} start position {} must be less than sequence length {}",
                        index, start_beats, sequence_length
                    ),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
impl Validate for RecordingConfig {
    fn validate(&self) -> Result<()> {
        // Validate duration - either beats or seconds must be set
        match (self.length_beats, self.length_seconds) {
            (Some(beats), _) => {
                if beats <= 0.0 {
                    return Err(Error::invalid_param(
                        "length_beats",
                        "recording length must be positive",
                    ));
                }
                if beats > limits::MAX_RECORDING_BEATS {
                    return Err(Error::invalid_param(
                        "length_beats",
                        format!(
                            "recording length must be at most {} beats",
                            limits::MAX_RECORDING_BEATS
                        ),
                    ));
                }
            }
            (None, Some(secs)) => {
                if secs <= 0.0 {
                    return Err(Error::invalid_param(
                        "length_seconds",
                        "recording length must be positive",
                    ));
                }
                if secs > limits::MAX_RECORDING_SECONDS {
                    return Err(Error::invalid_param(
                        "length_seconds",
                        format!(
                            "recording length must be at most {} seconds ({} hours)",
                            limits::MAX_RECORDING_SECONDS,
                            limits::MAX_RECORDING_SECONDS / 3600.0
                        ),
                    ));
                }
            }
            (None, None) => {
                return Err(Error::invalid_param(
                    "length",
                    "recording must have either length_beats or length_seconds set",
                ));
            }
        }

        // Validate channels
        if self.num_channels == 0 {
            return Err(Error::invalid_param(
                "num_channels",
                "recording must have at least 1 channel",
            ));
        }
        if self.num_channels > limits::MAX_CHANNELS {
            return Err(Error::invalid_param(
                "num_channels",
                format!(
                    "recording can have at most {} channels, got {}",
                    limits::MAX_CHANNELS,
                    self.num_channels
                ),
            ));
        }

        // Validate count-in
        if self.count_in_beats < 0.0 {
            return Err(Error::invalid_param(
                "count_in_beats",
                "count-in beats cannot be negative",
            ));
        }

        Ok(())
    }
}

impl Validate for SampleConfig {
    fn validate(&self) -> Result<()> {
        // Validate path exists (basic check - actual file check happens at load time)
        if self.path.as_os_str().is_empty() {
            return Err(Error::invalid_param("path", "sample path cannot be empty"));
        }

        // Validate rate
        if self.rate <= 0.0 {
            return Err(Error::invalid_param(
                "rate",
                format!("sample rate multiplier must be positive, got {}", self.rate),
            ));
        }
        if self.rate > 100.0 {
            return Err(Error::invalid_param(
                "rate",
                format!(
                    "sample rate multiplier {} is unreasonably high (max 100x)",
                    self.rate
                ),
            ));
        }

        // Validate offset
        if self.offset < 0.0 {
            return Err(Error::invalid_param(
                "offset",
                format!("sample offset cannot be negative, got {}", self.offset),
            ));
        }

        // Validate amp
        if self.amp < 0.0 {
            return Err(Error::invalid_param(
                "amp",
                format!("sample amplitude cannot be negative, got {}", self.amp),
            ));
        }

        // Validate attack/sustain/release
        if self.attack < 0.0 {
            return Err(Error::invalid_param(
                "attack",
                format!("attack time cannot be negative, got {}", self.attack),
            ));
        }
        if self.sustain < 0.0 || self.sustain > 1.0 {
            return Err(Error::invalid_param(
                "sustain",
                format!("sustain must be between 0.0 and 1.0, got {}", self.sustain),
            ));
        }
        if self.release < 0.0 {
            return Err(Error::invalid_param(
                "release",
                format!("release time cannot be negative, got {}", self.release),
            ));
        }

        // Validate warp parameters
        if self.speed <= 0.0 {
            return Err(Error::invalid_param(
                "speed",
                format!("warp speed must be positive, got {}", self.speed),
            ));
        }
        if self.pitch <= 0.0 {
            return Err(Error::invalid_param(
                "pitch",
                format!("pitch multiplier must be positive, got {}", self.pitch),
            ));
        }

        Ok(())
    }
}

/// Validate BPM value.
pub fn validate_bpm(bpm: f64) -> Result<()> {
    if bpm < limits::MIN_BPM {
        return Err(Error::invalid_param(
            "bpm",
            format!("BPM must be at least {}, got {}", limits::MIN_BPM, bpm),
        ));
    }
    if bpm > limits::MAX_BPM {
        return Err(Error::invalid_param(
            "bpm",
            format!("BPM must be at most {}, got {}", limits::MAX_BPM, bpm),
        ));
    }
    Ok(())
}

/// Validate MIDI note number.
pub fn validate_midi_note(note: u8) -> Result<()> {
    if note > limits::MAX_MIDI_NOTE {
        return Err(Error::invalid_param(
            "note",
            format!(
                "MIDI note must be 0-{}, got {}",
                limits::MAX_MIDI_NOTE,
                note
            ),
        ));
    }
    Ok(())
}

/// Validate MIDI CC number.
pub fn validate_midi_cc(cc: u8) -> Result<()> {
    if cc > limits::MAX_MIDI_CC {
        return Err(Error::invalid_param(
            "cc",
            format!("MIDI CC must be 0-{}, got {}", limits::MAX_MIDI_CC, cc),
        ));
    }
    Ok(())
}

/// Validate MIDI channel.
pub fn validate_midi_channel(channel: u8) -> Result<()> {
    if channel > limits::MAX_MIDI_CHANNEL {
        return Err(Error::invalid_param(
            "channel",
            format!(
                "MIDI channel must be 0-{}, got {}",
                limits::MAX_MIDI_CHANNEL,
                channel
            ),
        ));
    }
    Ok(())
}

/// Validate velocity value.
pub fn validate_velocity(velocity: f32) -> Result<()> {
    if !(limits::MIN_VELOCITY..=limits::MAX_VELOCITY).contains(&velocity) {
        return Err(Error::invalid_param(
            "velocity",
            format!(
                "velocity must be between {} and {}, got {}",
                limits::MIN_VELOCITY,
                limits::MAX_VELOCITY,
                velocity
            ),
        ));
    }
    Ok(())
}

/// Validate a beat length is positive.
pub fn validate_positive_beat(beat: Beat, name: &str) -> Result<()> {
    let beats = beat.to_f64();
    if beats <= 0.0 {
        return Err(Error::invalid_param(
            name,
            format!("{} must be positive, got {}", name, beats),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GroupId;

    #[test]
    fn test_voice_config_validation() {
        // Valid config
        let config = VoiceConfig::new("test_voice", "sine", GroupId::new(0));
        assert!(config.has_sound_source());
        assert!(config.validate().is_ok());

        // Empty synthdef
        let mut config = VoiceConfig::new("test_voice", "sine", GroupId::new(0));
        config.synthdef = String::new();
        assert!(!config.has_sound_source());
        assert!(config.validate().is_err());

        // Invalid polyphony
        let config = VoiceConfig::new("test_voice", "sine", GroupId::new(0)).with_polyphony(0);
        assert!(config.validate().is_err());

        let config = VoiceConfig::new("test_voice", "sine", GroupId::new(0)).with_polyphony(255);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_pattern_config_validation() {
        use crate::traits::Step;

        // Valid config
        let config = PatternConfig::with_length("test_pattern", crate::types::VoiceId::new(0), 4.0)
            .with_step(Step::at(0.0))
            .with_step(Step::at(1.0));
        assert!(config.validate().is_ok());

        // Zero length
        let config = PatternConfig::with_length("test_pattern", crate::types::VoiceId::new(0), 0.0);
        assert!(config.validate().is_err());

        // Step outside pattern
        let config = PatternConfig::with_length("test_pattern", crate::types::VoiceId::new(0), 4.0)
            .with_step(Step::at(5.0));
        assert!(config.validate().is_err());

        // Invalid swing
        let config = PatternConfig::with_length("test_pattern", crate::types::VoiceId::new(0), 4.0)
            .with_swing(1.5);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_melody_config_validation() {
        // Valid config
        let config = MelodyConfig::with_length("test_melody", crate::types::VoiceId::new(0), 4.0)
            .with_note(NoteEvent::quarter(0.0, 60, 0.8));
        assert!(config.validate().is_ok());

        // Invalid note number
        let config = MelodyConfig::with_length("test_melody", crate::types::VoiceId::new(0), 4.0)
            .with_note(NoteEvent::quarter(0.0, 200, 0.8));
        assert!(config.validate().is_err());

        // Invalid velocity
        let config = MelodyConfig::with_length("test_melody", crate::types::VoiceId::new(0), 4.0)
            .with_note(NoteEvent::quarter(0.0, 60, 1.5));
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_bpm_validation() {
        assert!(validate_bpm(120.0).is_ok());
        assert!(validate_bpm(1.0).is_ok());
        assert!(validate_bpm(999.0).is_ok());
        assert!(validate_bpm(0.5).is_err());
        assert!(validate_bpm(1000.0).is_err());
    }

    #[test]
    fn test_midi_validation() {
        assert!(validate_midi_note(0).is_ok());
        assert!(validate_midi_note(127).is_ok());
        assert!(validate_midi_note(128).is_err());

        assert!(validate_midi_cc(0).is_ok());
        assert!(validate_midi_cc(127).is_ok());
        assert!(validate_midi_cc(128).is_err());

        assert!(validate_midi_channel(0).is_ok());
        assert!(validate_midi_channel(15).is_ok());
        assert!(validate_midi_channel(16).is_err());
    }

    #[test]
    fn test_sample_config_validation() {
        use std::path::PathBuf;

        // Valid config
        let config = SampleConfig::new(PathBuf::from("/path/to/sample.wav"));
        assert!(config.validate().is_ok());

        // Empty path
        let config = SampleConfig::new(PathBuf::new());
        assert!(config.validate().is_err());

        // Invalid rate
        let mut config = SampleConfig::new(PathBuf::from("/path/to/sample.wav"));
        config.rate = 0.0;
        assert!(config.validate().is_err());

        config.rate = -1.0;
        assert!(config.validate().is_err());

        config.rate = 200.0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_recording_config_validation() {
        // Valid config with beats
        let config = RecordingConfig::new(GroupId::new(0)).with_beats(16.0);
        assert!(config.validate().is_ok());

        // Valid config with seconds
        let config = RecordingConfig::new(GroupId::new(0)).with_seconds(10.0);
        assert!(config.validate().is_ok());

        // No duration set
        let mut config = RecordingConfig::new(GroupId::new(0));
        config.length_beats = None;
        config.length_seconds = None;
        assert!(config.validate().is_err());

        // Negative count-in
        let mut config = RecordingConfig::new(GroupId::new(0)).with_beats(16.0);
        config.count_in_beats = -1.0;
        assert!(config.validate().is_err());
    }
}
