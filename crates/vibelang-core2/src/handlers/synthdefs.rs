//! SynthDefs handler implementation.

use crate::backend::Backend;
use crate::state::State;
use crate::synthdefs::generate_builtins;
use crate::traits::SynthDefs;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use crate::compat::RwLock;

/// Handler for synthdef operations.
pub struct SynthDefsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

impl<B: Backend> SynthDefsHandler<B> {
    /// Create a new synthdefs handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }

    /// Load all built-in synthdefs.
    ///
    /// This should be called during runtime initialization to ensure
    /// essential synthdefs are available. Loads synthdefs from vibelang-dsp
    /// including sample voices, SFZ instruments, MIDI output, routing, and recording.
    pub async fn load_builtins(&self) -> Result<()> {
        let builtins = generate_builtins();
        let count = builtins.len();

        for (name, bytes) in builtins {
            self.load(&name, &bytes).await?;
            tracing::debug!("Loaded built-in synthdef: {}", name);
        }

        tracing::info!("Loaded {} built-in synthdefs from vibelang-dsp", count);
        Ok(())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> SynthDefs for SynthDefsHandler<B> {
    async fn load(&self, name: &str, data: &[u8]) -> Result<()> {
        tracing::debug!("SynthDefsHandler: loading synthdef '{}' ({} bytes)", name, data.len());

        // Load in backend
        self.backend
            .load_synthdef(name, data)
            .await
            .map_err(Error::backend)?;

        tracing::debug!("SynthDefsHandler: sent d_recv for '{}', now syncing", name);

        // Sync with backend to ensure synthdef is loaded before continuing.
        // This prevents race conditions where we try to create synths before
        // the synthdef is available.
        self.backend.sync().await.map_err(Error::backend)?;

        tracing::debug!("SynthDefsHandler: sync completed for '{}', registered in state", name);

        // Track in state
        let mut state = self.state.write().await;
        state.synthdefs.insert(name.to_string());

        Ok(())
    }

    async fn is_loaded(&self, name: &str) -> bool {
        self.state.read().await.synthdefs.contains(name)
    }
}
