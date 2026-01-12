//! Transport handler implementation.

use crate::backend::Backend;
use crate::clock::TransportClock;
use crate::compat::{Instant, RwLock};
use crate::state::State;
use crate::traits::Transport;
use crate::types::{Beat, TimeSignature};
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

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

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{AddAction, BufferInfo};
    use crate::types::{BufferId, NodeId, ParamMap};
    use std::path::Path;

    // =========================================================================
    // Mock Backend for Testing
    // =========================================================================

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock error")
        }
    }

    impl std::error::Error for MockError {}

    struct MockBackend;

    #[async_trait]
    impl Backend for MockBackend {
        type Error = MockError;

        async fn load_synthdef(&self, _name: &str, _data: &[u8]) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn create_synth(
            &self,
            _def: &str,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
            _params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn create_group(
            &self,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_node(&self, _node: NodeId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn run_node(&self, _node: NodeId, _running: bool) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn set_param(&self, _node: NodeId, _param: &str, _value: f32) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn load_buffer(&self, _id: BufferId, _path: &Path) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames: 44100,
                channels: 2,
                sample_rate: 44100.0,
            })
        }

        async fn alloc_buffer(
            &self,
            _id: BufferId,
            frames: u32,
            channels: u16,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames,
                channels,
                sample_rate: 44100.0,
            })
        }

        async fn write_buffer(&self, _id: BufferId, _path: &Path) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_buffer(&self, _id: BufferId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn map_param_to_bus(
            &self,
            _node: NodeId,
            _param: &str,
            _bus: u32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        fn current_time(&self) -> Instant {
            Instant::now()
        }
    }

    // =========================================================================
    // Helper Functions
    // =========================================================================

    fn create_handler() -> TransportHandler<MockBackend> {
        let backend = Arc::new(MockBackend);
        let state = Arc::new(RwLock::new(State::default()));
        TransportHandler::new(backend, state)
    }

    // =========================================================================
    // Tempo Tests
    // =========================================================================

    #[tokio::test]
    async fn test_default_tempo() {
        let handler = create_handler();
        let tempo = handler.tempo().await;
        assert_eq!(tempo, 120.0, "Default tempo should be 120 BPM");
    }

    #[tokio::test]
    async fn test_set_tempo() {
        let handler = create_handler();
        handler.set_tempo(140.0).await.unwrap();
        let tempo = handler.tempo().await;
        assert_eq!(tempo, 140.0, "Tempo should be updated to 140 BPM");
    }

    #[tokio::test]
    async fn test_tempo_clamp_minimum() {
        let handler = create_handler();
        // Try to set tempo below minimum
        handler.set_tempo(0.5).await.unwrap();
        let tempo = handler.tempo().await;
        assert_eq!(tempo, 1.0, "Tempo should be clamped to minimum 1.0 BPM");
    }

    #[tokio::test]
    async fn test_tempo_clamp_maximum() {
        let handler = create_handler();
        // Try to set tempo above maximum
        handler.set_tempo(1500.0).await.unwrap();
        let tempo = handler.tempo().await;
        assert_eq!(tempo, 999.0, "Tempo should be clamped to maximum 999.0 BPM");
    }

    #[tokio::test]
    async fn test_tempo_updates_state() {
        let backend = Arc::new(MockBackend);
        let state = Arc::new(RwLock::new(State::default()));
        let handler = TransportHandler::new(backend, state.clone());

        handler.set_tempo(180.0).await.unwrap();

        let state_read = state.read().await;
        assert_eq!(state_read.tempo, 180.0, "State tempo should be updated");
    }

    // =========================================================================
    // Time Signature Tests
    // =========================================================================

    #[tokio::test]
    async fn test_default_time_signature() {
        let handler = create_handler();
        let sig = handler.time_signature().await;
        assert_eq!(sig.numerator, 4, "Default numerator should be 4");
        assert_eq!(sig.denominator, 4, "Default denominator should be 4");
    }

    #[tokio::test]
    async fn test_set_time_signature_3_4() {
        let handler = create_handler();
        handler.set_time_signature(TimeSignature { numerator: 3, denominator: 4 }).await.unwrap();
        let sig = handler.time_signature().await;
        assert_eq!(sig.numerator, 3);
        assert_eq!(sig.denominator, 4);
    }

    #[tokio::test]
    async fn test_set_time_signature_6_8() {
        let handler = create_handler();
        handler.set_time_signature(TimeSignature { numerator: 6, denominator: 8 }).await.unwrap();
        let sig = handler.time_signature().await;
        assert_eq!(sig.numerator, 6);
        assert_eq!(sig.denominator, 8);
    }

    #[tokio::test]
    async fn test_set_time_signature_5_4() {
        let handler = create_handler();
        handler.set_time_signature(TimeSignature { numerator: 5, denominator: 4 }).await.unwrap();
        let sig = handler.time_signature().await;
        assert_eq!(sig.numerator, 5);
        assert_eq!(sig.denominator, 4);
    }

    // =========================================================================
    // Start/Stop Tests
    // =========================================================================

    #[tokio::test]
    async fn test_default_not_playing() {
        let handler = create_handler();
        assert!(!handler.is_playing().await, "Should not be playing by default");
    }

    #[tokio::test]
    async fn test_start() {
        let handler = create_handler();
        handler.start().await.unwrap();
        assert!(handler.is_playing().await, "Should be playing after start");
    }

    #[tokio::test]
    async fn test_stop() {
        let handler = create_handler();
        handler.start().await.unwrap();
        handler.stop().await.unwrap();
        assert!(!handler.is_playing().await, "Should not be playing after stop");
    }

    #[tokio::test]
    async fn test_start_updates_state() {
        let backend = Arc::new(MockBackend);
        let state = Arc::new(RwLock::new(State::default()));
        let handler = TransportHandler::new(backend, state.clone());

        handler.start().await.unwrap();

        let state_read = state.read().await;
        assert!(state_read.playing, "State playing should be true after start");
    }

    #[tokio::test]
    async fn test_stop_updates_state() {
        let backend = Arc::new(MockBackend);
        let state = Arc::new(RwLock::new(State::default()));
        let handler = TransportHandler::new(backend, state.clone());

        handler.start().await.unwrap();
        handler.stop().await.unwrap();

        let state_read = state.read().await;
        assert!(!state_read.playing, "State playing should be false after stop");
    }

    // =========================================================================
    // Seek Tests
    // =========================================================================

    #[tokio::test]
    async fn test_seek() {
        let handler = create_handler();
        handler.seek(Beat::from_f64(8.0)).await.unwrap();

        let beat = handler.current_beat().await;
        // Allow small floating point tolerance
        assert!((beat.to_f64() - 8.0).abs() < 0.01, "Current beat should be near 8.0 after seek");
    }

    #[tokio::test]
    async fn test_seek_updates_state() {
        let backend = Arc::new(MockBackend);
        let state = Arc::new(RwLock::new(State::default()));
        let handler = TransportHandler::new(backend, state.clone());

        handler.seek(Beat::from_f64(16.0)).await.unwrap();

        let state_read = state.read().await;
        assert!((state_read.current_beat.to_f64() - 16.0).abs() < 0.01, "State current_beat should be updated");
    }

    #[tokio::test]
    async fn test_seek_while_playing() {
        let handler = create_handler();
        handler.start().await.unwrap();
        handler.seek(Beat::from_f64(32.0)).await.unwrap();

        // Should still be playing
        assert!(handler.is_playing().await);

        let beat = handler.current_beat().await;
        // Beat should be at or near 32.0 (may have advanced slightly)
        assert!(beat.to_f64() >= 32.0, "Current beat should be >= 32.0 after seek");
    }

    // =========================================================================
    // Tick Tests
    // =========================================================================

    #[tokio::test]
    async fn test_tick_updates_beat_when_playing() {
        let backend = Arc::new(MockBackend);
        let state = Arc::new(RwLock::new(State::default()));
        let handler = TransportHandler::new(backend, state.clone());

        handler.start().await.unwrap();

        // Simulate time passing
        let now = Instant::now();
        handler.tick(now).await;

        // State should have been updated
        let state_read = state.read().await;
        // Current beat should be at or near 0 (just started)
        assert!(state_read.current_beat.to_f64() >= 0.0);
    }

    #[tokio::test]
    async fn test_tick_does_not_update_when_stopped() {
        let backend = Arc::new(MockBackend);
        let state = Arc::new(RwLock::new(State::default()));
        let handler = TransportHandler::new(backend, state.clone());

        // Explicitly set a beat value
        {
            let mut state_write = state.write().await;
            state_write.current_beat = Beat::from_f64(5.0);
        }

        // Don't start - transport is stopped
        let now = Instant::now();
        handler.tick(now).await;

        // State should NOT have been updated (transport is stopped)
        let state_read = state.read().await;
        assert_eq!(state_read.current_beat.to_f64(), 5.0, "Beat should not change when stopped");
    }

    // =========================================================================
    // Current Beat Tests
    // =========================================================================

    #[tokio::test]
    async fn test_current_beat_default() {
        let handler = create_handler();
        let beat = handler.current_beat().await;
        assert_eq!(beat.to_f64(), 0.0, "Default current beat should be 0");
    }

    #[tokio::test]
    async fn test_current_beat_after_start_is_at_least_zero() {
        let handler = create_handler();
        handler.start().await.unwrap();
        let beat = handler.current_beat().await;
        assert!(beat.to_f64() >= 0.0, "Current beat after start should be >= 0");
    }

    // =========================================================================
    // Multiple Operations Tests
    // =========================================================================

    #[tokio::test]
    async fn test_start_stop_start() {
        let handler = create_handler();

        handler.start().await.unwrap();
        assert!(handler.is_playing().await);

        handler.stop().await.unwrap();
        assert!(!handler.is_playing().await);

        handler.start().await.unwrap();
        assert!(handler.is_playing().await, "Should be playing after re-start");
    }

    #[tokio::test]
    async fn test_tempo_change_while_playing() {
        let handler = create_handler();

        handler.start().await.unwrap();
        handler.set_tempo(180.0).await.unwrap();

        assert!(handler.is_playing().await, "Should still be playing after tempo change");
        assert_eq!(handler.tempo().await, 180.0);
    }

    #[tokio::test]
    async fn test_time_signature_change_while_playing() {
        let handler = create_handler();

        handler.start().await.unwrap();
        handler.set_time_signature(TimeSignature { numerator: 7, denominator: 8 }).await.unwrap();

        assert!(handler.is_playing().await, "Should still be playing after time signature change");
        let sig = handler.time_signature().await;
        assert_eq!(sig.numerator, 7);
        assert_eq!(sig.denominator, 8);
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[tokio::test]
    async fn test_seek_to_zero() {
        let handler = create_handler();

        handler.seek(Beat::from_f64(16.0)).await.unwrap();
        handler.seek(Beat::ZERO).await.unwrap();

        let beat = handler.current_beat().await;
        assert_eq!(beat.to_f64(), 0.0, "Should be at beat 0 after seeking to zero");
    }

    #[tokio::test]
    async fn test_multiple_tempo_changes() {
        let handler = create_handler();

        for bpm in [60.0, 90.0, 120.0, 180.0, 240.0] {
            handler.set_tempo(bpm).await.unwrap();
            assert_eq!(handler.tempo().await, bpm);
        }
    }

    #[tokio::test]
    async fn test_extreme_tempo_values() {
        let handler = create_handler();

        // Minimum valid tempo
        handler.set_tempo(1.0).await.unwrap();
        assert_eq!(handler.tempo().await, 1.0);

        // Maximum valid tempo
        handler.set_tempo(999.0).await.unwrap();
        assert_eq!(handler.tempo().await, 999.0);
    }

    #[tokio::test]
    async fn test_seek_to_large_beat() {
        let handler = create_handler();

        handler.seek(Beat::from_f64(1000.0)).await.unwrap();

        let beat = handler.current_beat().await;
        assert!((beat.to_f64() - 1000.0).abs() < 0.01, "Should be at beat 1000");
    }
}
