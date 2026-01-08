//! Transport handler implementation.

use crate::backend::Backend;
use crate::clock::TransportClock;
use crate::state::State;
use crate::traits::Transport;
use crate::types::{Beat, TimeSignature};
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Handler for transport operations.
pub struct TransportHandler<B: Backend> {
    #[allow(dead_code)]
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Transport clock for wall-clock to beat conversion.
    clock: Arc<RwLock<TransportClock>>,
}

impl<B: Backend> TransportHandler<B> {
    /// Create a new transport handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self {
            backend,
            state,
            clock: Arc::new(RwLock::new(TransportClock::default())),
        }
    }

    /// Update the current beat from the transport clock.
    ///
    /// Called by the runtime's tick loop.
    pub async fn tick(&self, now: Instant) {
        let clock = self.clock.read().await;
        if clock.is_running() {
            let current_beat = clock.instant_to_beat(now);
            let mut state = self.state.write().await;
            state.current_beat = current_beat;
        }
    }
}

#[async_trait]
impl<B: Backend> Transport for TransportHandler<B> {
    async fn tempo(&self) -> f64 {
        self.clock.read().await.tempo()
    }

    async fn set_tempo(&self, bpm: f64) -> Result<()> {
        let clamped_bpm = bpm.clamp(1.0, 999.0);
        let now = Instant::now();

        // Update clock with new tempo
        let mut clock = self.clock.write().await;
        clock.set_tempo(clamped_bpm, now);

        // Also update state for external queries
        let mut state = self.state.write().await;
        state.tempo = clamped_bpm;

        Ok(())
    }

    async fn time_signature(&self) -> TimeSignature {
        self.state.read().await.time_sig
    }

    async fn set_time_signature(&self, sig: TimeSignature) -> Result<()> {
        let mut state = self.state.write().await;
        state.time_sig = sig;
        Ok(())
    }

    async fn current_beat(&self) -> Beat {
        let clock = self.clock.read().await;
        clock.instant_to_beat(Instant::now())
    }

    async fn seek(&self, beat: Beat) -> Result<()> {
        let now = Instant::now();

        // Update clock
        let mut clock = self.clock.write().await;
        clock.seek(beat, now);

        // Update state
        let mut state = self.state.write().await;
        state.current_beat = beat;

        Ok(())
    }

    async fn is_playing(&self) -> bool {
        self.clock.read().await.is_running()
    }

    async fn start(&self) -> Result<()> {
        let now = Instant::now();

        // Get current position from state
        let current_beat = {
            let state = self.state.read().await;
            state.current_beat
        };

        // Start the clock from current position
        let mut clock = self.clock.write().await;
        clock.start(current_beat, now);

        // Update state
        let mut state = self.state.write().await;
        state.playing = true;

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let now = Instant::now();

        // Stop the clock
        let mut clock = self.clock.write().await;
        let beat = clock.stop(now);

        // Update state
        let mut state = self.state.write().await;
        state.playing = false;
        state.current_beat = beat;

        Ok(())
    }
}
