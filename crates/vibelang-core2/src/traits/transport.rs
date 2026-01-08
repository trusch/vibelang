//! Transport control trait.
//!
//! The [`Transport`] trait provides control over playback timing:
//! tempo, time signature, position, and play/stop.

use crate::types::{Beat, TimeSignature};
use crate::Result;
use async_trait::async_trait;

/// Transport control for playback timing.
///
/// Provides control over the musical clock: tempo, time signature,
/// current position, and play/stop state.
///
/// All methods are async for WASM compatibility.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Get the current tempo in BPM.
    async fn tempo(&self) -> f64;

    /// Set the tempo in BPM.
    async fn set_tempo(&self, bpm: f64) -> Result<()>;

    /// Get the current time signature.
    async fn time_signature(&self) -> TimeSignature;

    /// Set the time signature.
    async fn set_time_signature(&self, sig: TimeSignature) -> Result<()>;

    /// Get the current beat position.
    async fn current_beat(&self) -> Beat;

    /// Seek to a specific beat position.
    async fn seek(&self, beat: Beat) -> Result<()>;

    /// Check if playback is running.
    async fn is_playing(&self) -> bool;

    /// Start playback.
    async fn start(&self) -> Result<()>;

    /// Stop playback.
    async fn stop(&self) -> Result<()>;
}
