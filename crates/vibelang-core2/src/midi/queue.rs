//! Lock-free MIDI event queue for high-performance event passing.
//!
//! This module provides a bounded multi-producer single-consumer queue
//! optimized for MIDI event passing from device callbacks to the processing task.
//!
//! Uses `crossbeam-channel` for lock-free operation on the fast path.

use super::events::TimestampedMidiEvent;
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// Queue Statistics
// ============================================================================

/// Statistics for monitoring queue health.
#[derive(Debug, Default)]
pub struct QueueStats {
    /// Total events sent.
    pub events_sent: AtomicU64,
    /// Total events received.
    pub events_received: AtomicU64,
    /// Events dropped due to full queue.
    pub events_dropped: AtomicU64,
    /// Number of times the queue was empty on receive.
    pub empty_receives: AtomicU64,
}

impl QueueStats {
    /// Create new stats tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the number of events currently in flight (sent but not received).
    pub fn in_flight(&self) -> u64 {
        let sent = self.events_sent.load(Ordering::Relaxed);
        let received = self.events_received.load(Ordering::Relaxed);
        sent.saturating_sub(received)
    }

    /// Reset all counters.
    pub fn reset(&self) {
        self.events_sent.store(0, Ordering::Relaxed);
        self.events_received.store(0, Ordering::Relaxed);
        self.events_dropped.store(0, Ordering::Relaxed);
        self.empty_receives.store(0, Ordering::Relaxed);
    }

    /// Log current statistics.
    pub fn log(&self) {
        tracing::info!(
            "[MIDI_QUEUE] Stats: sent={} received={} dropped={} in_flight={}",
            self.events_sent.load(Ordering::Relaxed),
            self.events_received.load(Ordering::Relaxed),
            self.events_dropped.load(Ordering::Relaxed),
            self.in_flight(),
        );
    }
}

// ============================================================================
// Event Queue
// ============================================================================

/// Default queue capacity (events).
pub const DEFAULT_QUEUE_CAPACITY: usize = 4096;

/// High-performance MIDI event queue.
///
/// This queue is designed for the MIDI hot path:
/// - Lock-free send/receive on the fast path
/// - Bounded to prevent unbounded memory growth
/// - Non-blocking operations for real-time safety
pub struct MidiEventQueue {
    /// Sender side (can be cloned for multiple producers).
    tx: Sender<TimestampedMidiEvent>,
    /// Receiver side (single consumer).
    rx: Receiver<TimestampedMidiEvent>,
    /// Statistics for monitoring.
    stats: Arc<QueueStats>,
    /// Queue capacity.
    capacity: usize,
}

impl MidiEventQueue {
    /// Create a new event queue with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = crossbeam_channel::bounded(capacity);
        Self {
            tx,
            rx,
            stats: Arc::new(QueueStats::new()),
            capacity,
        }
    }

    /// Create a new event queue with default capacity.
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_QUEUE_CAPACITY)
    }

    /// Get a sender handle that can be cloned and sent to other threads.
    pub fn sender(&self) -> MidiEventSender {
        MidiEventSender {
            tx: self.tx.clone(),
            stats: Arc::clone(&self.stats),
        }
    }

    /// Get a reference to the queue statistics.
    pub fn stats(&self) -> Arc<QueueStats> {
        Arc::clone(&self.stats)
    }

    /// Get the queue capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.rx.is_empty()
    }

    /// Get the current number of events in the queue.
    pub fn len(&self) -> usize {
        self.rx.len()
    }

    /// Try to receive a single event without blocking.
    ///
    /// Returns `None` if the queue is empty.
    #[inline]
    pub fn try_recv(&self) -> Option<TimestampedMidiEvent> {
        match self.rx.try_recv() {
            Ok(event) => {
                self.stats.events_received.fetch_add(1, Ordering::Relaxed);
                Some(event)
            }
            Err(TryRecvError::Empty) => {
                self.stats.empty_receives.fetch_add(1, Ordering::Relaxed);
                None
            }
            Err(TryRecvError::Disconnected) => None,
        }
    }

    /// Receive all available events without blocking.
    ///
    /// Returns an iterator that drains the queue.
    pub fn drain(&self) -> DrainIter<'_> {
        DrainIter { queue: self }
    }

    /// Receive events into a vector, up to a maximum count.
    ///
    /// This is useful when you want to batch process events.
    pub fn recv_batch(&self, max_count: usize) -> Vec<TimestampedMidiEvent> {
        let mut events = Vec::with_capacity(max_count.min(self.len()));
        for event in self.drain().take(max_count) {
            events.push(event);
        }
        events
    }

    /// Blocking receive with timeout.
    ///
    /// Returns `None` if the timeout expires or the channel is disconnected.
    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Option<TimestampedMidiEvent> {
        match self.rx.recv_timeout(timeout) {
            Ok(event) => {
                self.stats.events_received.fetch_add(1, Ordering::Relaxed);
                Some(event)
            }
            Err(_) => None,
        }
    }

    /// Check if there are any events in the queue.
    pub fn has_events(&self) -> bool {
        !self.rx.is_empty()
    }
}

impl Default for MidiEventQueue {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}

// ============================================================================
// Sender Handle
// ============================================================================

/// A cloneable sender handle for the event queue.
///
/// This can be sent to MIDI device callback threads for sending events.
#[derive(Clone)]
pub struct MidiEventSender {
    tx: Sender<TimestampedMidiEvent>,
    stats: Arc<QueueStats>,
}

impl MidiEventSender {
    /// Try to send an event without blocking.
    ///
    /// Returns `true` if the event was sent, `false` if the queue is full.
    #[inline]
    pub fn try_send(&self, event: TimestampedMidiEvent) -> bool {
        match self.tx.try_send(event) {
            Ok(()) => {
                self.stats.events_sent.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Full(_)) => {
                self.stats.events_dropped.fetch_add(1, Ordering::Relaxed);
                tracing::warn!("[MIDI_QUEUE] Event dropped: queue full");
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                tracing::debug!("[MIDI_QUEUE] Send failed: channel disconnected");
                false
            }
        }
    }

    /// Send an event, blocking if the queue is full.
    ///
    /// Returns `true` if sent, `false` if the channel is disconnected.
    pub fn send(&self, event: TimestampedMidiEvent) -> bool {
        match self.tx.send(event) {
            Ok(()) => {
                self.stats.events_sent.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(_) => {
                tracing::debug!("[MIDI_QUEUE] Send failed: channel disconnected");
                false
            }
        }
    }

    /// Check if the queue is full.
    pub fn is_full(&self) -> bool {
        self.tx.is_full()
    }
}

// ============================================================================
// Drain Iterator
// ============================================================================

/// Iterator that drains events from the queue.
pub struct DrainIter<'a> {
    queue: &'a MidiEventQueue,
}

impl<'a> Iterator for DrainIter<'a> {
    type Item = TimestampedMidiEvent;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.queue.try_recv()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.queue.len();
        (0, Some(len)) // Lower bound is 0 because other threads might consume
    }
}

// ============================================================================
// Async Support
// ============================================================================

/// Async wrapper for receiving MIDI events.
///
/// This uses tokio's blocking task pool to avoid blocking the async runtime.
#[cfg(feature = "native")]
pub struct AsyncMidiEventReceiver {
    rx: Receiver<TimestampedMidiEvent>,
    stats: Arc<QueueStats>,
}

#[cfg(feature = "native")]
impl AsyncMidiEventReceiver {
    /// Create from an event queue.
    pub fn new(queue: &MidiEventQueue) -> Self {
        Self {
            rx: queue.rx.clone(),
            stats: queue.stats(),
        }
    }

    /// Receive an event asynchronously.
    ///
    /// This is cancel-safe.
    pub async fn recv(&self) -> Option<TimestampedMidiEvent> {
        let rx = self.rx.clone();
        let stats = Arc::clone(&self.stats);

        // Use tokio's spawn_blocking to avoid blocking the async runtime
        tokio::task::spawn_blocking(move || match rx.recv() {
            Ok(event) => {
                stats.events_received.fetch_add(1, Ordering::Relaxed);
                Some(event)
            }
            Err(_) => None,
        })
        .await
        .ok()
        .flatten()
    }

    /// Try to receive without blocking.
    pub fn try_recv(&self) -> Option<TimestampedMidiEvent> {
        match self.rx.try_recv() {
            Ok(event) => {
                self.stats.events_received.fetch_add(1, Ordering::Relaxed);
                Some(event)
            }
            Err(_) => None,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::events::{Channel, MidiMessage, Velocity};
    use crate::types::ids::MidiDeviceId;
    use std::time::Instant;

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

    #[test]
    fn test_basic_send_recv() {
        let queue = MidiEventQueue::new(16);
        let sender = queue.sender();

        assert!(sender.try_send(make_test_event(60)));
        assert!(sender.try_send(make_test_event(64)));

        let event1 = queue.try_recv().unwrap();
        assert!(matches!(event1.message, MidiMessage::NoteOn { note: 60, .. }));

        let event2 = queue.try_recv().unwrap();
        assert!(matches!(event2.message, MidiMessage::NoteOn { note: 64, .. }));

        assert!(queue.try_recv().is_none());
    }

    #[test]
    fn test_queue_full() {
        let queue = MidiEventQueue::new(2);
        let sender = queue.sender();

        assert!(sender.try_send(make_test_event(60)));
        assert!(sender.try_send(make_test_event(64)));
        assert!(!sender.try_send(make_test_event(67))); // Should fail

        assert_eq!(queue.stats.events_dropped.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_drain() {
        let queue = MidiEventQueue::new(16);
        let sender = queue.sender();

        for i in 0..5 {
            sender.try_send(make_test_event(60 + i));
        }

        let events: Vec<_> = queue.drain().collect();
        assert_eq!(events.len(), 5);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_recv_batch() {
        let queue = MidiEventQueue::new(16);
        let sender = queue.sender();

        for i in 0..10 {
            sender.try_send(make_test_event(60 + i));
        }

        let batch = queue.recv_batch(5);
        assert_eq!(batch.len(), 5);
        assert_eq!(queue.len(), 5);
    }

    #[test]
    fn test_stats() {
        let queue = MidiEventQueue::new(16);
        let sender = queue.sender();

        sender.try_send(make_test_event(60));
        sender.try_send(make_test_event(64));
        queue.try_recv();

        let stats = queue.stats();
        assert_eq!(stats.events_sent.load(Ordering::Relaxed), 2);
        assert_eq!(stats.events_received.load(Ordering::Relaxed), 1);
        assert_eq!(stats.in_flight(), 1);
    }

    #[test]
    fn test_multiple_senders() {
        let queue = MidiEventQueue::new(16);
        let sender1 = queue.sender();
        let sender2 = queue.sender();

        sender1.try_send(make_test_event(60));
        sender2.try_send(make_test_event(64));

        assert_eq!(queue.len(), 2);
    }
}
