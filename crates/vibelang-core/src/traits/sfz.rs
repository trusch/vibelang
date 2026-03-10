//! SFZ instrument trait for sample-based instruments.
//!
//! SFZ instruments are collections of samples mapped across keys and velocities,
//! following the SFZ format specification.

use crate::types::SfzId;
use crate::Result;
use async_trait::async_trait;
use std::path::Path;

/// Configuration for loading an SFZ instrument.
#[derive(Clone, Debug, PartialEq)]
pub struct SfzConfig {
    /// Path to the SFZ file.
    pub path: std::path::PathBuf,
}

impl SfzConfig {
    /// Create a new SFZ configuration.
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Default for SfzConfig {
    fn default() -> Self {
        Self {
            path: std::path::PathBuf::new(),
        }
    }
}

/// Information about a trigger from an SFZ region match.
///
/// This is returned when finding matching regions for a note trigger.
#[derive(Clone, Debug)]
pub struct SfzTriggerInfo {
    /// Buffer ID for the sample.
    pub buffer_id: crate::types::BufferId,

    /// Playback rate for correct pitch.
    ///
    /// Calculated from pitch_keycenter and the triggered note.
    pub rate: f32,

    /// Number of channels in the sample.
    pub num_channels: u8,

    /// Amplitude envelope attack time in seconds.
    pub ampeg_attack: f32,

    /// Amplitude envelope decay time in seconds.
    pub ampeg_decay: f32,

    /// Amplitude envelope sustain level (0.0-1.0).
    pub ampeg_sustain: f32,

    /// Amplitude envelope release time in seconds.
    pub ampeg_release: f32,

    /// Volume in linear amplitude.
    pub amp: f32,

    /// Pan position (-1.0 to 1.0).
    pub pan: f32,

    /// Loop enabled.
    pub loop_enabled: bool,

    /// Loop start position in frames.
    pub loop_start: u32,

    /// Loop end position in frames.
    pub loop_end: u32,

    /// Sample start offset in frames.
    pub offset: u32,

    /// Filter cutoff frequency (if filter is enabled).
    pub cutoff: Option<f32>,

    /// Filter resonance (if filter is enabled).
    pub resonance: Option<f32>,
}

impl Default for SfzTriggerInfo {
    fn default() -> Self {
        Self {
            buffer_id: crate::types::BufferId::new(0),
            rate: 1.0,
            num_channels: 2,
            ampeg_attack: 0.001,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.01,
            amp: 1.0,
            pan: 0.0,
            loop_enabled: false,
            loop_start: 0,
            loop_end: 0,
            offset: 0,
            cutoff: None,
            resonance: None,
        }
    }
}

/// Trigger mode for SFZ region matching.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SfzTriggerMode {
    /// Note-on trigger (default).
    #[default]
    Attack,
    /// Note-off trigger (release samples).
    Release,
    /// First note in legato phrase.
    First,
    /// Subsequent notes in legato phrase.
    Legato,
}

/// SFZ instrument management.
///
/// SFZ instruments contain multiple samples mapped across the keyboard
/// and velocity range. When a note is triggered, the appropriate sample(s)
/// are selected based on key, velocity, and other criteria.
///
/// Features:
/// - Multi-sample instruments with key/velocity layers
/// - Round-robin sample alternation
/// - Release triggers for key-off samples
/// - Full opcode support for envelopes, filters, and modulation
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait Sfz: Send + Sync {
    /// Load an SFZ instrument from a file.
    ///
    /// This parses the SFZ file and loads all referenced samples into buffers.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for this instrument
    /// * `path` - Path to the .sfz file
    ///
    /// # Returns
    ///
    /// Ok(()) on success, or an error if loading fails.
    async fn load(&self, id: SfzId, path: &Path) -> Result<()>;

    /// Unload an SFZ instrument.
    ///
    /// This frees all buffers associated with the instrument.
    async fn unload(&self, id: SfzId) -> Result<()>;

    /// Find matching regions for a note trigger.
    ///
    /// This is called when a voice with an SFZ instrument triggers a note.
    /// It finds all matching regions based on note, velocity, and trigger mode,
    /// and returns the information needed to play them.
    ///
    /// # Arguments
    ///
    /// * `id` - SFZ instrument ID
    /// * `note` - MIDI note number (0-127)
    /// * `velocity` - MIDI velocity (0-127)
    /// * `trigger_mode` - Trigger mode (Attack, Release, etc.)
    ///
    /// # Returns
    ///
    /// A vector of trigger info for each matching region.
    /// Usually returns 1 region, but can return multiple for layered samples.
    async fn find_regions(
        &self,
        id: SfzId,
        note: u8,
        velocity: u8,
        trigger_mode: SfzTriggerMode,
    ) -> Result<Vec<SfzTriggerInfo>>;
}

/// SFZ instrument management (WASM version).
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait Sfz {
    /// Load an SFZ instrument from a file.
    async fn load(&self, id: SfzId, path: &Path) -> Result<()>;

    /// Unload an SFZ instrument.
    async fn unload(&self, id: SfzId) -> Result<()>;

    /// Find matching regions for a note trigger.
    async fn find_regions(
        &self,
        id: SfzId,
        note: u8,
        velocity: u8,
        trigger_mode: SfzTriggerMode,
    ) -> Result<Vec<SfzTriggerInfo>>;
}
