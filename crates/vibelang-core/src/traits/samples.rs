//! Samples trait for audio file management.
//!
//! Samples are audio files loaded into buffers for playback.

use crate::types::{BufferId, SampleId};
use crate::Result;
use async_trait::async_trait;
use std::path::PathBuf;

/// Information about a loaded sample.
#[derive(Clone, Debug)]
pub struct SampleInfo {
    /// Sample ID.
    pub id: SampleId,

    /// Buffer ID (for backend operations).
    pub buffer_id: BufferId,

    /// Original file path.
    pub path: PathBuf,

    /// Duration in seconds.
    pub duration_secs: f64,

    /// Sample rate in Hz.
    pub sample_rate: f64,

    /// Number of channels.
    pub channels: u16,

    /// Detected BPM (if analyzed).
    pub detected_bpm: Option<f64>,

    /// File modification time of the source file for the load that filled
    /// this buffer (copied from [`SampleConfig::mtime`]). Reload diffing
    /// compares it against the incoming script config's mtime to detect
    /// an overwritten file at an unchanged path.
    pub source_mtime: Option<std::time::SystemTime>,
}

/// Configuration for loading a sample.
#[derive(Clone, Debug, PartialEq)]
pub struct SampleConfig {
    /// Path to the audio file.
    pub path: PathBuf,

    /// File modification time captured at script-eval time (see
    /// [`SampleConfig::refresh_mtime`]). Part of the buffer identity for
    /// reload diffing: a changed mtime at an unchanged path means the file
    /// was overwritten and the buffer must be reloaded. mtime is used
    /// instead of a content hash because it costs one `stat` per sample
    /// per reload instead of reading the whole file; the tradeoff is that
    /// rewrites which preserve mtime (e.g. `rsync -t`, sub-granularity
    /// writes) go undetected. `None` when the file could not be stat'ed
    /// (or on targets without a filesystem).
    pub mtime: Option<std::time::SystemTime>,

    /// Enable time-stretching (warp mode).
    pub warp: bool,

    /// Target BPM for warping.
    pub target_bpm: Option<f64>,

    // === Envelope parameters ===
    /// Envelope attack time in seconds.
    pub attack: f64,

    /// Envelope sustain level (0.0 - 1.0).
    pub sustain: f64,

    /// Envelope release time in seconds.
    pub release: f64,

    // === Playback parameters ===
    /// Playback amplitude (0.0 - 1.0+).
    pub amp: f64,

    /// Playback rate multiplier (1.0 = normal speed).
    pub rate: f64,

    /// Loop mode enabled.
    pub loop_mode: bool,

    /// Start offset in seconds.
    pub offset: f64,

    /// Playback length in seconds (None = full sample).
    pub length: Option<f64>,

    // === Warp mode parameters ===
    /// Playback speed for warp mode (1.0 = normal, 0.5 = half speed).
    pub speed: f64,

    /// Pitch shift multiplier for warp mode (1.0 = original, 2.0 = octave up).
    pub pitch: f64,

    /// Granular window size in seconds for warp mode.
    pub window_size: f64,

    /// Number of overlapping grains for warp mode.
    pub overlaps: f64,

    /// Trigger mode: "gate" (default) = release on note-off, "one_shot" = ignore note-off.
    pub trigger_mode: String,
}

impl Default for SampleConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            mtime: None,
            warp: false,
            target_bpm: None,
            attack: 0.001,
            sustain: 1.0,
            release: 0.01,
            amp: 1.0,
            rate: 1.0,
            loop_mode: false,
            offset: 0.0,
            length: None,
            speed: 1.0,
            pitch: 1.0,
            window_size: 0.1,
            overlaps: 8.0,
            trigger_mode: "gate".to_string(),
        }
    }
}

impl SampleConfig {
    /// Create a new sample configuration.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ..Default::default()
        }
    }

    /// Capture the source file's current modification time into
    /// [`SampleConfig::mtime`].
    ///
    /// Called at script-eval time so the reload diff can detect an
    /// overwritten file at an unchanged path. A failed `stat` (missing
    /// file, permissions) leaves `mtime = None` — the load itself will
    /// surface the real error.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn refresh_mtime(&mut self) {
        self.mtime = std::fs::metadata(&self.path)
            .and_then(|meta| meta.modified())
            .ok();
    }

    /// No-op on WASM: there is no filesystem to stat.
    #[cfg(target_arch = "wasm32")]
    pub fn refresh_mtime(&mut self) {}

    /// Enable warp mode.
    pub fn with_warp(mut self) -> Self {
        self.warp = true;
        self
    }

    /// Set target BPM for warping.
    pub fn with_target_bpm(mut self, bpm: f64) -> Self {
        self.warp = true;
        self.target_bpm = Some(bpm);
        self
    }
}

/// Sample management for audio files.
///
/// Samples are loaded into memory for playback by voices.
/// They can optionally be time-stretched to match the tempo.
///
/// All methods are async for WASM compatibility.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait Samples: Send + Sync {
    /// Load a sample from a file.
    ///
    /// # Returns
    ///
    /// Information about the loaded sample.
    async fn load(&self, id: SampleId, config: SampleConfig) -> Result<SampleInfo>;

    /// Unload a sample.
    async fn unload(&self, id: SampleId) -> Result<()>;

    /// Get information about a loaded sample.
    async fn info(&self, id: SampleId) -> Option<SampleInfo>;
}

/// Sample management for audio files (WASM version).
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait Samples {
    /// Load a sample from a file.
    async fn load(&self, id: SampleId, config: SampleConfig) -> Result<SampleInfo>;

    /// Unload a sample.
    async fn unload(&self, id: SampleId) -> Result<()>;

    /// Get information about a loaded sample.
    async fn info(&self, id: SampleId) -> Option<SampleInfo>;
}
