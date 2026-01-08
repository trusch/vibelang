//! SynthDefs handler implementation.

use crate::backend::Backend;
use crate::state::State;
use crate::synthdefs::generate_builtins;
use crate::traits::SynthDefs;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

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

#[async_trait]
impl<B: Backend> SynthDefs for SynthDefsHandler<B> {
    async fn load(&self, name: &str, data: &[u8]) -> Result<()> {
        // Load in backend
        self.backend
            .load_synthdef(name, data)
            .await
            .map_err(Error::backend)?;

        // Track in state
        let mut state = self.state.write().await;
        state.synthdefs.insert(name.to_string());

        Ok(())
    }

    async fn is_loaded(&self, name: &str) -> bool {
        self.state.read().await.synthdefs.contains(name)
    }
}
