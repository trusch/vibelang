//! Backend trait for synthesis engines.
//!
//! The [`Backend`] trait abstracts over different synthesis engines,
//! allowing the runtime to work with SuperCollider, WebAudio, or mock backends.
//!
//! # Implementing a Backend
//!
//! ```ignore
//! use vibelang_core2::{Backend, AddAction, BufferInfo, NodeId, BufferId, ParamMap};
//! use async_trait::async_trait;
//! use std::path::Path;
//!
//! struct MockBackend;
//!
//! #[async_trait]
//! impl Backend for MockBackend {
//!     type Error = std::io::Error;
//!
//!     async fn load_synthdef(&self, name: &str, data: &[u8]) -> Result<(), Self::Error> {
//!         Ok(())
//!     }
//!     // ... implement other methods
//! }
//! ```

use crate::types::{BufferId, NodeId, ParamMap};
use async_trait::async_trait;
use std::path::Path;
use std::time::Instant;

/// Where to place a new node relative to a target node.
///
/// These correspond to SuperCollider's add actions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AddAction {
    /// Add to the head of the target group.
    Head = 0,
    /// Add to the tail of the target group.
    #[default]
    Tail = 1,
    /// Add immediately before the target node.
    Before = 2,
    /// Add immediately after the target node.
    After = 3,
    /// Replace the target node.
    Replace = 4,
}

/// Information about a loaded audio buffer.
#[derive(Clone, Debug)]
pub struct BufferInfo {
    /// Number of sample frames.
    pub frames: u32,
    /// Number of channels.
    pub channels: u16,
    /// Sample rate in Hz.
    pub sample_rate: f64,
}

impl BufferInfo {
    /// Calculate the duration in seconds.
    #[inline]
    pub fn duration_secs(&self) -> f64 {
        self.frames as f64 / self.sample_rate
    }
}

/// Core synthesis backend abstraction.
///
/// This trait defines the minimal interface needed to control a synthesis engine.
/// All methods are async to support both native (tokio) and WASM
/// (wasm-bindgen-futures) runtimes uniformly.
///
/// # Implementations
///
/// - `ScsynthBackend` (native): Communicates with SuperCollider via OSC
/// - `WebAudioBackend` (WASM): Uses the Web Audio API
/// - `MockBackend` (testing): No-op implementation for tests
///
/// # Thread Safety
///
/// Backends must be `Send + Sync` to allow sharing across async tasks.
#[async_trait]
pub trait Backend: Send + Sync + 'static {
    /// Error type for backend operations.
    type Error: std::error::Error + Send + Sync + 'static;

    // =========================================================================
    // SynthDef Management
    // =========================================================================

    /// Load a synthdef from compiled bytes.
    ///
    /// # Arguments
    ///
    /// * `name` - Name to register the synthdef under
    /// * `data` - Compiled synthdef bytes (e.g., from SuperCollider)
    async fn load_synthdef(&self, name: &str, data: &[u8]) -> Result<(), Self::Error>;

    // =========================================================================
    // Node Lifecycle
    // =========================================================================

    /// Create a new synth node.
    ///
    /// # Arguments
    ///
    /// * `def` - Name of the synthdef to instantiate
    /// * `node` - ID to assign to the new synth
    /// * `target` - Target node for placement
    /// * `action` - Where to place relative to target
    /// * `params` - Initial parameter values
    async fn create_synth(
        &self,
        def: &str,
        node: NodeId,
        target: NodeId,
        action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error>;

    /// Create a new group node.
    ///
    /// Groups contain synths and other groups.
    ///
    /// # Arguments
    ///
    /// * `node` - ID to assign to the new group
    /// * `target` - Target node for placement
    /// * `action` - Where to place relative to target
    async fn create_group(
        &self,
        node: NodeId,
        target: NodeId,
        action: AddAction,
    ) -> Result<(), Self::Error>;

    /// Free (destroy) a node.
    ///
    /// Works for both synths and groups.
    async fn free_node(&self, node: NodeId) -> Result<(), Self::Error>;

    /// Pause or resume a node.
    ///
    /// Paused nodes don't process audio but retain their state.
    async fn run_node(&self, node: NodeId, running: bool) -> Result<(), Self::Error>;

    // =========================================================================
    // Parameter Control
    // =========================================================================

    /// Set a parameter on a node.
    ///
    /// # Arguments
    ///
    /// * `node` - Target node ID
    /// * `param` - Parameter name
    /// * `value` - New value
    async fn set_param(&self, node: NodeId, param: &str, value: f32) -> Result<(), Self::Error>;

    /// Set multiple parameters on a node.
    ///
    /// More efficient than multiple `set_param` calls.
    async fn set_params(&self, node: NodeId, params: &ParamMap) -> Result<(), Self::Error> {
        for (param, value) in params {
            self.set_param(node, param, *value).await?;
        }
        Ok(())
    }

    // =========================================================================
    // Buffer Management
    // =========================================================================

    /// Load an audio file into a buffer.
    ///
    /// # Arguments
    ///
    /// * `id` - Buffer ID to allocate
    /// * `path` - Path to the audio file
    ///
    /// # Returns
    ///
    /// Information about the loaded buffer (frames, channels, sample rate).
    async fn load_buffer(&self, id: BufferId, path: &Path) -> Result<BufferInfo, Self::Error>;

    /// Allocate an empty buffer for recording.
    ///
    /// # Arguments
    ///
    /// * `id` - Buffer ID to allocate
    /// * `frames` - Number of sample frames
    /// * `channels` - Number of channels (1 or 2)
    ///
    /// # Returns
    ///
    /// Information about the allocated buffer.
    async fn alloc_buffer(
        &self,
        id: BufferId,
        frames: u32,
        channels: u16,
    ) -> Result<BufferInfo, Self::Error>;

    /// Write a buffer to an audio file.
    ///
    /// # Arguments
    ///
    /// * `id` - Buffer ID to write
    /// * `path` - Path to the output file (WAV format)
    async fn write_buffer(&self, id: BufferId, path: &Path) -> Result<(), Self::Error>;

    /// Free a buffer.
    async fn free_buffer(&self, id: BufferId) -> Result<(), Self::Error>;

    // =========================================================================
    // Timing
    // =========================================================================

    /// Get the current time instant.
    ///
    /// Used for scheduling and fade interpolation.
    fn current_time(&self) -> Instant;

    // =========================================================================
    // Optional Methods (with default implementations)
    // =========================================================================

    /// Check if the backend is ready to process audio.
    fn is_ready(&self) -> bool {
        true
    }

    /// Get the audio sample rate.
    fn sample_rate(&self) -> f64 {
        44100.0
    }

    /// Get the audio block size.
    fn block_size(&self) -> u32 {
        64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_action_default() {
        assert_eq!(AddAction::default(), AddAction::Tail);
    }

    #[test]
    fn test_add_action_values() {
        assert_eq!(AddAction::Head as i32, 0);
        assert_eq!(AddAction::Tail as i32, 1);
        assert_eq!(AddAction::Before as i32, 2);
        assert_eq!(AddAction::After as i32, 3);
        assert_eq!(AddAction::Replace as i32, 4);
    }

    #[test]
    fn test_add_action_equality() {
        assert_eq!(AddAction::Head, AddAction::Head);
        assert_ne!(AddAction::Head, AddAction::Tail);
    }

    #[test]
    fn test_buffer_info_duration() {
        let info = BufferInfo {
            frames: 44100,
            channels: 2,
            sample_rate: 44100.0,
        };
        assert_eq!(info.duration_secs(), 1.0);
    }

    #[test]
    fn test_buffer_info_duration_stereo() {
        let info = BufferInfo {
            frames: 88200,
            channels: 2,
            sample_rate: 44100.0,
        };
        assert_eq!(info.duration_secs(), 2.0);
    }

    #[test]
    fn test_buffer_info_48khz() {
        let info = BufferInfo {
            frames: 48000,
            channels: 1,
            sample_rate: 48000.0,
        };
        assert_eq!(info.duration_secs(), 1.0);
    }

    #[test]
    fn test_buffer_info_clone() {
        let info = BufferInfo {
            frames: 44100,
            channels: 2,
            sample_rate: 44100.0,
        };
        let cloned = info.clone();
        assert_eq!(cloned.frames, 44100);
        assert_eq!(cloned.channels, 2);
    }
}
