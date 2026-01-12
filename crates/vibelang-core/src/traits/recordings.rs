//! Recording trait and configuration types.
//!
//! Recordings capture audio from a group's bus into a buffer, optionally
//! saving to a file and returning a sample handle.
//!
//! ## Recording Lifecycle
//!
//! 1. **Configuration**: Create a `RecordingConfig` with length, source, and options
//! 2. **Pending**: Recording waits for its start beat (quantized start)
//! 3. **CountingIn**: Optional count-in metronome plays
//! 4. **Recording**: Audio is being captured to buffer
//! 5. **Completed**: Recording finished, buffer is usable as a sample
//! 6. **Cancelled**: Recording was aborted before completion
//!
//! ## Example
//!
//! ```rhai
//! // Record 4 bars from the drums group with 2 bar count-in
//! let take1 = record("take1")
//!     .from_group("drums")
//!     .bars(4)
//!     .count_in(2)
//!     .metronome(true)
//!     .to_file("recordings/drums_take1.wav")
//!     .apply();
//!
//! // The sample handle can be used immediately (audio arrives when recording completes)
//! let drums_voice = voice("playback").on(take1).apply();
//! ```

use crate::error::Result;
use crate::types::{Beat, BufferId, BusId, GroupId, NodeId, RecordingId, SampleId};
use async_trait::async_trait;
use std::path::PathBuf;

/// Status of an audio recording session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingStatus {
    /// Waiting for start beat (quantized start pending).
    Pending,
    /// Count-in metronome is playing.
    CountingIn,
    /// Actively recording audio.
    Recording,
    /// Recording complete, buffer ready for use.
    Completed,
    /// Recording was cancelled.
    Cancelled,
}

impl Default for RecordingStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Configuration for starting a recording session.
#[derive(Clone, Debug)]
pub struct RecordingConfig {
    /// Group to record from.
    pub group: GroupId,

    /// Length in beats (for tempo-synced recordings).
    /// Takes precedence over `length_seconds` if both are set.
    pub length_beats: Option<f64>,

    /// Length in seconds (for fixed-time recordings).
    pub length_seconds: Option<f64>,

    /// Beat when recording should start.
    /// If None, starts at the next quantization boundary.
    pub start_beat: Option<Beat>,

    /// Count-in length in beats (0 = no count-in).
    pub count_in_beats: f64,

    /// Whether to play metronome during count-in/recording.
    pub metronome: bool,

    /// File path to save recording (None = buffer only).
    pub file_path: Option<PathBuf>,

    /// Number of audio channels (1 = mono, 2 = stereo).
    pub num_channels: u8,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            group: GroupId::new(0),
            length_beats: Some(16.0), // Default to 4 bars in 4/4
            length_seconds: None,
            start_beat: None,
            count_in_beats: 0.0,
            metronome: false,
            file_path: None,
            num_channels: 2,
        }
    }
}

impl RecordingConfig {
    /// Create a new recording config for the given group.
    pub fn new(group: GroupId) -> Self {
        Self {
            group,
            ..Default::default()
        }
    }

    /// Set the length in beats.
    pub fn with_beats(mut self, beats: f64) -> Self {
        self.length_beats = Some(beats);
        self.length_seconds = None;
        self
    }

    /// Set the length in bars (using 4 beats per bar).
    pub fn with_bars(mut self, bars: f64) -> Self {
        self.length_beats = Some(bars * 4.0);
        self.length_seconds = None;
        self
    }

    /// Set the length in seconds.
    pub fn with_seconds(mut self, seconds: f64) -> Self {
        self.length_beats = None;
        self.length_seconds = Some(seconds);
        self
    }

    /// Set the start beat.
    pub fn starting_at(mut self, beat: Beat) -> Self {
        self.start_beat = Some(beat);
        self
    }

    /// Set the count-in length in beats.
    pub fn with_count_in(mut self, beats: f64) -> Self {
        self.count_in_beats = beats;
        self
    }

    /// Enable or disable metronome.
    pub fn with_metronome(mut self, enabled: bool) -> Self {
        self.metronome = enabled;
        self
    }

    /// Set the output file path.
    pub fn to_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Set the number of channels.
    pub fn with_channels(mut self, channels: u8) -> Self {
        self.num_channels = channels.clamp(1, 2);
        self
    }
}

/// Information about an active or completed recording.
#[derive(Clone, Debug)]
pub struct RecordingInfo {
    /// Recording ID.
    pub id: RecordingId,

    /// Group being recorded.
    pub group: GroupId,

    /// Buffer ID where audio is captured.
    pub buffer_id: BufferId,

    /// Recording synth node ID (set when recording starts).
    pub node_id: Option<NodeId>,

    /// Audio bus being recorded from.
    pub audio_bus: BusId,

    /// Length in beats (for tempo-synced recordings).
    pub length_beats: Option<f64>,

    /// Length in seconds (for fixed-time recordings).
    pub length_seconds: Option<f64>,

    /// Beat when recording starts.
    pub start_beat: Beat,

    /// Beat when recording ends.
    pub end_beat: Beat,

    /// Count-in length in beats.
    pub count_in_beats: f64,

    /// Whether metronome is enabled.
    pub metronome: bool,

    /// File path to save recording.
    pub file_path: Option<PathBuf>,

    /// Number of audio channels.
    pub num_channels: u8,

    /// Number of frames allocated in buffer.
    pub num_frames: u32,

    /// Sample rate.
    pub sample_rate: f32,

    /// Current recording status.
    pub status: RecordingStatus,

    /// Whether the buffer has been confirmed allocated.
    /// Recording synth won't start until this is true.
    pub buffer_ready: bool,

    /// Sample ID created from this recording (set when completed).
    pub sample_id: Option<SampleId>,
}

impl RecordingInfo {
    /// Create a new recording info with default values.
    pub fn new(id: RecordingId, group: GroupId, buffer_id: BufferId, audio_bus: BusId) -> Self {
        Self {
            id,
            group,
            buffer_id,
            node_id: None,
            audio_bus,
            length_beats: None,
            length_seconds: None,
            start_beat: Beat::ZERO,
            end_beat: Beat::ZERO,
            count_in_beats: 0.0,
            metronome: false,
            file_path: None,
            num_channels: 2,
            num_frames: 0,
            sample_rate: 48000.0,
            status: RecordingStatus::Pending,
            buffer_ready: false,
            sample_id: None,
        }
    }

    /// Calculate the duration in seconds at the given tempo.
    pub fn duration_secs(&self, tempo: f64) -> f64 {
        if let Some(seconds) = self.length_seconds {
            seconds
        } else if let Some(beats) = self.length_beats {
            beats * 60.0 / tempo
        } else {
            0.0
        }
    }
}

/// Trait for managing audio recordings.
#[async_trait]
pub trait Recordings: Send + Sync {
    /// Start a new recording session.
    ///
    /// Returns the buffer ID allocated for the recording. The actual recording
    /// starts when the start beat is reached (or immediately if `start_beat` is in the past).
    async fn start(&self, id: RecordingId, config: RecordingConfig) -> Result<BufferId>;

    /// Stop an active recording early.
    ///
    /// The recorded audio up to this point is preserved and the buffer becomes usable.
    async fn stop(&self, id: RecordingId) -> Result<()>;

    /// Cancel a pending or active recording.
    ///
    /// Unlike stop, cancel discards the recorded audio and frees the buffer.
    async fn cancel(&self, id: RecordingId) -> Result<()>;

    /// Get the status of a recording.
    async fn status(&self, id: RecordingId) -> Result<RecordingStatus>;

    /// Get full info about a recording.
    async fn info(&self, id: RecordingId) -> Result<RecordingInfo>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recording_config_defaults() {
        let config = RecordingConfig::default();
        assert_eq!(config.length_beats, Some(16.0));
        assert_eq!(config.num_channels, 2);
        assert!(!config.metronome);
        assert_eq!(config.count_in_beats, 0.0);
    }

    #[test]
    fn test_recording_config_builder() {
        let config = RecordingConfig::new(GroupId::new(1))
            .with_bars(4.0)
            .with_count_in(8.0)
            .with_metronome(true)
            .to_file("test.wav")
            .with_channels(1);

        assert_eq!(config.length_beats, Some(16.0));
        assert_eq!(config.count_in_beats, 8.0);
        assert!(config.metronome);
        assert_eq!(config.file_path, Some(PathBuf::from("test.wav")));
        assert_eq!(config.num_channels, 1);
    }

    #[test]
    fn test_recording_status_default() {
        let status = RecordingStatus::default();
        assert_eq!(status, RecordingStatus::Pending);
    }

    #[test]
    fn test_recording_info_duration() {
        let mut info = RecordingInfo::new(
            RecordingId::new(1),
            GroupId::new(1),
            BufferId::new(0),
            BusId::new(16),
        );

        // Test beats-based duration at 120 BPM
        info.length_beats = Some(8.0);
        assert!((info.duration_secs(120.0) - 4.0).abs() < 0.001);

        // Test seconds-based duration (ignores tempo)
        info.length_seconds = Some(10.0);
        assert!((info.duration_secs(120.0) - 10.0).abs() < 0.001);
    }
}
