//! Samples handler implementation.
//!
//! This handler manages audio sample loading and unloading. Samples are loaded
//! into buffers on the synthesis backend and can be referenced by voices for
//! playback.
//!
//! # Supported Formats
//!
//! The supported formats depend on the backend:
//!
//! - **scsynth (SuperCollider)**: WAV, AIFF, FLAC, and other libsndfile formats
//! - **WebAudio (WASM)**: Formats supported by the browser's AudioContext
//!
//! # Example
//!
//! ```ignore
//! use vibelang_core2::{SampleId, SampleConfig};
//! use std::path::PathBuf;
//!
//! // Load a sample
//! let id = SampleId::new(1);
//! let config = SampleConfig {
//!     path: PathBuf::from("kick.wav"),
//! };
//! handler.load(id, config).await?;
//!
//! // Query sample info
//! if let Some(info) = handler.info(id).await {
//!     println!("Duration: {} seconds", info.duration_secs);
//! }
//!
//! // Unload when done
//! handler.unload(id).await?;
//! ```

use crate::backend::Backend;
use crate::compat::RwLock;
use crate::state::State;
use crate::traits::{SampleConfig, SampleInfo, Samples};
use crate::types::SampleId;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// Handler for sample loading and management.
///
/// Samples are audio files loaded into the backend's buffer system. Once loaded,
/// they can be played back using the `sample_playbuf` synthdef or similar.
///
/// # Buffer Management
///
/// Each sample is assigned a unique [`BufferId`](crate::types::BufferId) when loaded.
/// The handler tracks the mapping from [`SampleId`] to `BufferId` so voices can
/// reference samples by their logical ID.
pub struct SamplesHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

impl<B: Backend> SamplesHandler<B> {
    /// Create a new samples handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Samples for SamplesHandler<B> {
    async fn load(&self, id: SampleId, config: SampleConfig) -> Result<SampleInfo> {
        let mut state = self.state.write().await;

        // Allocate buffer ID
        let buffer_id = state.alloc_buffer_id();

        // Load buffer via backend
        let buffer_info = self
            .backend
            .load_buffer(buffer_id, &config.path)
            .await
            .map_err(|e| Error::SampleLoadFailed {
                path: config.path.clone(),
                reason: e.to_string(),
            })?;

        // Create sample info
        let info = SampleInfo {
            id,
            buffer_id,
            path: config.path,
            duration_secs: buffer_info.duration_secs(),
            sample_rate: buffer_info.sample_rate,
            channels: buffer_info.channels,
            detected_bpm: None, // TODO: Implement BPM detection
        };

        // Store in state
        state.samples.insert(id, info.clone());

        tracing::debug!(
            "Loaded sample {} from {:?} (buffer_id={}, duration={:.2}s)",
            id.0,
            info.path,
            buffer_id.0,
            info.duration_secs
        );

        Ok(info)
    }

    async fn unload(&self, id: SampleId) -> Result<()> {
        // Get the buffer_id before removing from state
        let buffer_id = {
            let mut state = self.state.write().await;
            let info = state.samples.remove(&id).ok_or(Error::SampleNotFound(id))?;
            info.buffer_id
        };

        // Free the buffer in backend
        self.backend
            .free_buffer(buffer_id)
            .await
            .map_err(Error::backend)?;

        tracing::debug!("Unloaded sample {} (buffer_id={})", id.0, buffer_id.0);

        Ok(())
    }

    async fn info(&self, id: SampleId) -> Option<SampleInfo> {
        self.state.read().await.samples.get(&id).cloned()
    }
}
