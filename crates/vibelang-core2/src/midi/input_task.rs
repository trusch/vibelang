//! Dedicated MIDI input processing task.
//!
//! This module provides an async task that processes MIDI input events
//! immediately as they arrive, replacing the 100Hz polling approach.
//!
//! ## Architecture
//!
//! ```text
//! midir callback thread(s)
//!       │
//!       │ MidiEventSender::try_send()
//!       ▼
//! ┌─────────────────────────────┐
//! │     MidiEventQueue          │
//! │   (lock-free bounded)       │
//! └─────────────────────────────┘
//!       │
//!       │ MidiInputTask::recv()
//!       ▼
//! ┌─────────────────────────────┐
//! │    MidiInputTask            │
//! │  - Jitter compensation      │
//! │  - Routing dispatch         │
//! │  - Callback invocation      │
//! └─────────────────────────────┘
//! ```

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::events::{MidiMessage, TimestampedMidiEvent};
use super::jitter::JitterCompensator;
use super::queue::MidiEventQueue;

// ============================================================================
// Event Handler Trait
// ============================================================================

/// Trait for handling processed MIDI events.
///
/// Implement this to receive MIDI events from the input task.
#[async_trait::async_trait]
pub trait MidiEventHandler: Send + Sync + 'static {
    /// Handle a MIDI event.
    ///
    /// This is called for each event after jitter compensation.
    /// The implementation should be fast to avoid backing up the queue.
    async fn handle_event(&self, event: TimestampedMidiEvent);

    /// Called when the input task starts.
    fn on_start(&self) {}

    /// Called when the input task stops.
    fn on_stop(&self) {}
}

// ============================================================================
// Input Task Configuration
// ============================================================================

/// Configuration for the MIDI input task.
#[derive(Clone, Debug)]
pub struct MidiInputTaskConfig {
    /// Enable jitter compensation.
    pub enable_jitter_compensation: bool,
    /// Jitter compensation buffer (μs).
    pub jitter_buffer_us: i64,
    /// Maximum events to process per batch.
    pub batch_size: usize,
    /// How often to log statistics (0 = never).
    pub stats_log_interval_secs: u64,
}

impl Default for MidiInputTaskConfig {
    fn default() -> Self {
        Self {
            enable_jitter_compensation: true,
            jitter_buffer_us: 1000, // 1ms
            batch_size: 64,
            stats_log_interval_secs: 60,
        }
    }
}

// ============================================================================
// Input Task
// ============================================================================

/// Dedicated async task for processing MIDI input.
///
/// This task:
/// - Receives events from the lock-free queue
/// - Applies jitter compensation
/// - Dispatches to the event handler
pub struct MidiInputTask<H: MidiEventHandler> {
    /// Event queue to receive from.
    queue: Arc<MidiEventQueue>,
    /// Event handler.
    handler: Arc<H>,
    /// Configuration.
    config: MidiInputTaskConfig,
    /// Cancellation token for shutdown.
    shutdown: CancellationToken,
}

impl<H: MidiEventHandler> MidiInputTask<H> {
    /// Create a new input task.
    pub fn new(
        queue: Arc<MidiEventQueue>,
        handler: Arc<H>,
        config: MidiInputTaskConfig,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            queue,
            handler,
            config,
            shutdown,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults(
        queue: Arc<MidiEventQueue>,
        handler: Arc<H>,
        shutdown: CancellationToken,
    ) -> Self {
        Self::new(queue, handler, MidiInputTaskConfig::default(), shutdown)
    }

    /// Spawn the input task.
    ///
    /// Returns a handle that can be used to await task completion.
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    /// Run the input task.
    async fn run(self) {
        tracing::info!("[MIDI_INPUT] Task started");
        self.handler.on_start();

        let mut jitter_comp = if self.config.enable_jitter_compensation {
            let mut comp = JitterCompensator::new();
            comp.set_buffer(self.config.jitter_buffer_us);
            Some(comp)
        } else {
            None
        };

        let mut last_stats_log = std::time::Instant::now();
        let stats_interval = if self.config.stats_log_interval_secs > 0 {
            Some(Duration::from_secs(self.config.stats_log_interval_secs))
        } else {
            None
        };

        loop {
            tokio::select! {
                biased;

                _ = self.shutdown.cancelled() => {
                    tracing::info!("[MIDI_INPUT] Shutdown requested");
                    break;
                }

                // Try to receive events with a small timeout
                _ = tokio::time::sleep(Duration::from_micros(100)) => {
                    // Process available events
                    let events = self.queue.recv_batch(self.config.batch_size);

                    for event in events {
                        // Update jitter compensation
                        if let Some(ref mut comp) = jitter_comp {
                            comp.add_event(&event);

                            // Apply compensation delay if needed
                            if let Some(delay) = comp.delay_for(&event) {
                                if delay < Duration::from_millis(10) {
                                    // Only delay for reasonable amounts
                                    tokio::time::sleep(delay).await;
                                }
                            }
                        }

                        // Dispatch to handler
                        self.handler.handle_event(event).await;
                    }

                    // Periodic stats logging
                    if let Some(interval) = stats_interval {
                        if last_stats_log.elapsed() >= interval {
                            self.log_stats(&jitter_comp);
                            last_stats_log = std::time::Instant::now();
                        }
                    }
                }
            }
        }

        self.handler.on_stop();
        self.log_stats(&jitter_comp);
        tracing::info!("[MIDI_INPUT] Task stopped");
    }

    /// Log statistics.
    fn log_stats(&self, jitter_comp: &Option<JitterCompensator>) {
        let queue_stats = self.queue.stats();
        tracing::info!(
            "[MIDI_INPUT] Queue stats: sent={} received={} dropped={} in_flight={}",
            queue_stats.events_sent.load(std::sync::atomic::Ordering::Relaxed),
            queue_stats.events_received.load(std::sync::atomic::Ordering::Relaxed),
            queue_stats.events_dropped.load(std::sync::atomic::Ordering::Relaxed),
            queue_stats.in_flight(),
        );

        if let Some(comp) = jitter_comp {
            let jitter_stats = comp.stats();
            tracing::info!(
                "[MIDI_INPUT] Jitter stats: samples={} mean={:.1}μs stddev={:.1}μs drift={:.2}μs/s",
                jitter_stats.sample_count,
                jitter_stats.mean_latency_us,
                jitter_stats.stddev_latency_us,
                jitter_stats.drift_estimate,
            );
        }
    }
}

// ============================================================================
// Channel-Based Handler
// ============================================================================

/// A simple handler that forwards events to an mpsc channel.
///
/// Useful for integrating with existing async code.
pub struct ChannelHandler {
    tx: mpsc::Sender<TimestampedMidiEvent>,
}

impl ChannelHandler {
    /// Create a new channel handler with the given capacity.
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<TimestampedMidiEvent>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }
}

#[async_trait::async_trait]
impl MidiEventHandler for ChannelHandler {
    async fn handle_event(&self, event: TimestampedMidiEvent) {
        if let Err(e) = self.tx.send(event).await {
            tracing::warn!("[MIDI_INPUT] Channel send failed: {}", e);
        }
    }
}

// ============================================================================
// Callback-Based Handler
// ============================================================================

/// Type alias for MIDI event callback functions to reduce type complexity.
type MidiEventCallback = Box<dyn Fn(&TimestampedMidiEvent) + Send + Sync>;

/// A handler that invokes callbacks based on message type.
pub struct CallbackHandler {
    /// Callback for note messages.
    on_note: Option<MidiEventCallback>,
    /// Callback for control change messages.
    on_cc: Option<MidiEventCallback>,
    /// Callback for all messages.
    on_any: Option<MidiEventCallback>,
}

impl CallbackHandler {
    /// Create a new callback handler.
    pub fn new() -> Self {
        Self {
            on_note: None,
            on_cc: None,
            on_any: None,
        }
    }

    /// Set the callback for note messages.
    pub fn with_note_callback<F>(mut self, f: F) -> Self
    where
        F: Fn(&TimestampedMidiEvent) + Send + Sync + 'static,
    {
        self.on_note = Some(Box::new(f));
        self
    }

    /// Set the callback for control change messages.
    pub fn with_cc_callback<F>(mut self, f: F) -> Self
    where
        F: Fn(&TimestampedMidiEvent) + Send + Sync + 'static,
    {
        self.on_cc = Some(Box::new(f));
        self
    }

    /// Set the callback for all messages.
    pub fn with_any_callback<F>(mut self, f: F) -> Self
    where
        F: Fn(&TimestampedMidiEvent) + Send + Sync + 'static,
    {
        self.on_any = Some(Box::new(f));
        self
    }
}

impl Default for CallbackHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl MidiEventHandler for CallbackHandler {
    async fn handle_event(&self, event: TimestampedMidiEvent) {
        // Call type-specific callback
        match &event.message {
            MidiMessage::NoteOn { .. } | MidiMessage::NoteOff { .. } => {
                if let Some(ref cb) = self.on_note {
                    cb(&event);
                }
            }
            MidiMessage::ControlChange { .. } => {
                if let Some(ref cb) = self.on_cc {
                    cb(&event);
                }
            }
            _ => {}
        }

        // Call generic callback
        if let Some(ref cb) = self.on_any {
            cb(&event);
        }
    }
}

// ============================================================================
// Builder
// ============================================================================

/// Builder for creating and spawning MIDI input tasks.
pub struct MidiInputTaskBuilder<H: MidiEventHandler> {
    queue: Arc<MidiEventQueue>,
    handler: Arc<H>,
    config: MidiInputTaskConfig,
}

impl<H: MidiEventHandler> MidiInputTaskBuilder<H> {
    /// Create a new builder.
    pub fn new(queue: Arc<MidiEventQueue>, handler: H) -> Self {
        Self {
            queue,
            handler: Arc::new(handler),
            config: MidiInputTaskConfig::default(),
        }
    }

    /// Set configuration.
    pub fn with_config(mut self, config: MidiInputTaskConfig) -> Self {
        self.config = config;
        self
    }

    /// Enable or disable jitter compensation.
    pub fn jitter_compensation(mut self, enable: bool) -> Self {
        self.config.enable_jitter_compensation = enable;
        self
    }

    /// Set the jitter buffer size in microseconds.
    pub fn jitter_buffer_us(mut self, buffer_us: i64) -> Self {
        self.config.jitter_buffer_us = buffer_us;
        self
    }

    /// Set the batch size.
    pub fn batch_size(mut self, size: usize) -> Self {
        self.config.batch_size = size;
        self
    }

    /// Build and spawn the task.
    ///
    /// Returns the cancellation token and join handle.
    pub fn spawn(self) -> (CancellationToken, JoinHandle<()>) {
        let shutdown = CancellationToken::new();
        let task = MidiInputTask::new(
            self.queue,
            self.handler,
            self.config,
            shutdown.clone(),
        );
        let handle = task.spawn();
        (shutdown, handle)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::events::{Channel, Velocity};
    use crate::types::ids::MidiDeviceId;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    struct CountingHandler {
        count: AtomicUsize,
    }

    impl CountingHandler {
        fn new() -> Self {
            Self {
                count: AtomicUsize::new(0),
            }
        }

        fn count(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl MidiEventHandler for CountingHandler {
        async fn handle_event(&self, _event: TimestampedMidiEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn make_test_event(note: u8) -> TimestampedMidiEvent {
        TimestampedMidiEvent {
            timestamp_us: 12345,
            received_at: Instant::now(),
            device_id: MidiDeviceId::new(0),
            message: MidiMessage::NoteOn {
                channel: Channel::new(0),
                note,
                velocity: Velocity::from_midi1(100),
            },
        }
    }

    #[tokio::test]
    async fn test_channel_handler() {
        let (handler, mut rx) = ChannelHandler::new(16);
        let queue = Arc::new(MidiEventQueue::new(16));
        let sender = queue.sender();

        // Send some events
        sender.try_send(make_test_event(60));
        sender.try_send(make_test_event(64));

        // Spawn task
        let shutdown = CancellationToken::new();
        let task = MidiInputTask::with_defaults(queue, Arc::new(handler), shutdown.clone());
        let handle = task.spawn();

        // Wait a bit for processing
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Should receive events
        let event1 = rx.try_recv().ok();
        assert!(event1.is_some());

        // Shutdown
        shutdown.cancel();
        handle.await.unwrap();
    }
}
