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
//! use vibelang_core::{SampleId, SampleConfig};
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
use crate::compat::{Instant, RwLock};
use crate::state::State;
use crate::traits::{SampleConfig, SampleInfo, Samples};
use crate::types::{BufferId, SampleId};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::compat::Duration;

/// Grace period before a displaced/unloaded sample buffer is freed on the
/// backend (in milliseconds).
///
/// Freeing a buffer while a live synth node still reads it (`/b_free` under
/// a playing `PlayBuf`) cuts the note to silence; worse, reusing the buffer
/// ID for a new `/b_allocRead` while old nodes read it plays garbage. The
/// grace period keeps the old buffer — and its ID, which stays out of the
/// allocator pool until the free executes — alive long enough for typical
/// one-shot tails and note releases to finish. Same deferred-free idiom as
/// `EffectsHandler` (its 50ms covers a mix fade; sample notes read buffers
/// for longer, hence the larger value). Notes that outlive the grace go
/// silent rather than glitch. A permanent fix would refcount reading nodes.
pub(crate) const BUFFER_FREE_GRACE_PERIOD_MS: u64 = 500;

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
    /// Buffers pending backend free after the grace period (see
    /// [`BUFFER_FREE_GRACE_PERIOD_MS`]). Drained by [`Self::tick`]; the
    /// buffer IDs are returned to the allocator pool only when the free
    /// actually executes, so an in-grace ID can never be re-allocated
    /// under a still-reading node.
    pending_frees: Arc<RwLock<Vec<(BufferId, Instant)>>>,
}

impl<B: Backend> SamplesHandler<B> {
    /// Create a new samples handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self {
            backend,
            state,
            pending_frees: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Schedule a buffer for backend free after the grace period.
    async fn defer_free(&self, buffer_id: BufferId) {
        self.pending_frees
            .write()
            .await
            .push((buffer_id, Instant::now()));
        tracing::debug!(
            "Buffer {}: scheduled for free after {}ms grace period",
            buffer_id.0,
            BUFFER_FREE_GRACE_PERIOD_MS
        );
    }

    /// Process pending buffer frees whose grace period elapsed.
    ///
    /// Called by the runtime's tick loop.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn tick(&self) {
        let now = Instant::now();
        let grace_period = Duration::from_millis(BUFFER_FREE_GRACE_PERIOD_MS);

        let buffers_to_free: Vec<BufferId> = {
            let mut pending = self.pending_frees.write().await;
            if pending.is_empty() {
                return;
            }
            let mut to_free = Vec::new();
            let mut remaining = Vec::new();
            for (buffer_id, requested_at) in pending.drain(..) {
                if now.duration_since(requested_at) >= grace_period {
                    to_free.push(buffer_id);
                } else {
                    remaining.push((buffer_id, requested_at));
                }
            }
            *pending = remaining;
            to_free
        };

        for buffer_id in buffers_to_free {
            tracing::debug!(
                "Buffer grace period elapsed, freeing buffer {}",
                buffer_id.0
            );
            if let Err(e) = self.backend.free_buffer(buffer_id).await {
                tracing::warn!("free_buffer({}) failed: {}", buffer_id.0, e);
            }
            self.state.write().await.free_buffer_id(buffer_id);
        }
    }

    /// Process pending buffer frees (WASM version - immediate free).
    #[cfg(target_arch = "wasm32")]
    pub async fn tick(&self) {
        let buffers_to_free: Vec<BufferId> = {
            let mut pending = self.pending_frees.write().await;
            pending.drain(..).map(|(buffer_id, _)| buffer_id).collect()
        };
        for buffer_id in buffers_to_free {
            let _ = self.backend.free_buffer(buffer_id).await;
            self.state.write().await.free_buffer_id(buffer_id);
        }
    }

    /// Load a sample's buffer without publishing it to state.
    ///
    /// The state lock is held only for the synchronous buffer-ID
    /// allocation, NOT across the backend round-trip. Holding it across
    /// the await would serialize concurrent loads (making a surrounding
    /// `join_all` a no-op) and stall every other state reader — including
    /// the runtime tick task — for the duration of the file load.
    ///
    /// Pair with [`Self::commit`] to make the sample visible, or return
    /// the buffer via `State::free_buffer_id` + `Backend::free_buffer` if
    /// the staged sample is discarded.
    pub async fn stage_load(&self, id: SampleId, config: SampleConfig) -> Result<SampleInfo> {
        // Allocate buffer ID (short lock, dropped before the await below)
        let buffer_id = self.state.write().await.alloc_buffer_id()?;

        // Load buffer via backend — no state lock held here.
        let buffer_info = match self.backend.load_buffer(buffer_id, &config.path).await {
            Ok(info) => info,
            Err(e) => {
                // Nothing references the ID yet — return it to the pool.
                self.state.write().await.free_buffer_id(buffer_id);
                return Err(Error::SampleLoadFailed {
                    path: config.path,
                    reason: e.to_string(),
                });
            }
        };

        Ok(SampleInfo {
            id,
            buffer_id,
            path: config.path,
            duration_secs: buffer_info.duration_secs(),
            sample_rate: buffer_info.sample_rate,
            channels: buffer_info.channels,
            detected_bpm: None, // TODO: Implement BPM detection
            source_mtime: config.mtime,
        })
    }

    /// Publish a staged sample into state (synchronous mutation only).
    ///
    /// If the insert displaces an existing entry for the same sample ID
    /// (in-place content reload), the OLD buffer is freed only after the
    /// grace period: new notes resolve `bufnum` through `State::samples`
    /// and immediately pick up the new buffer, while notes already playing
    /// keep reading the old one until the deferred free.
    pub async fn commit(&self, info: SampleInfo) {
        tracing::debug!(
            "Loaded sample {} from {:?} (buffer_id={}, duration={:.2}s)",
            info.id.0,
            info.path,
            info.buffer_id.0,
            info.duration_secs
        );
        let displaced = {
            let mut state = self.state.write().await;
            state.samples.insert(info.id, info.clone())
        };
        if let Some(old) = displaced {
            if old.buffer_id != info.buffer_id {
                self.defer_free(old.buffer_id).await;
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Samples for SamplesHandler<B> {
    async fn load(&self, id: SampleId, config: SampleConfig) -> Result<SampleInfo> {
        let info = self.stage_load(id, config).await?;
        self.commit(info.clone()).await;
        Ok(info)
    }

    async fn unload(&self, id: SampleId) -> Result<()> {
        // Remove the mapping now (no new note may resolve this sample),
        // but defer the backend free and the ID-pool return past the
        // grace period — live nodes may still be reading the buffer.
        let buffer_id = {
            let mut state = self.state.write().await;
            let info = state.samples.remove(&id).ok_or(Error::SampleNotFound(id))?;
            info.buffer_id
        };
        self.defer_free(buffer_id).await;

        tracing::debug!("Unloaded sample {} (buffer_id={})", id.0, buffer_id.0);

        Ok(())
    }

    async fn info(&self, id: SampleId) -> Option<SampleInfo> {
        self.state.read().await.samples.get(&id).cloned()
    }
}
