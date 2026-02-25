//! Lock-free transport state snapshot for thread-safe sharing.
//!
//! This module provides a `TransportSnapshot` struct that allows the main loop
//! to share transport state (beat position, tempo, playing status) with dedicated
//! threads (MIDI clock, modulator polling) without blocking.
//!
//! ## Design
//!
//! Uses atomic operations for zero-copy, wait-free access:
//! - `f64` values stored as `AtomicU64` bit patterns
//! - Sequence counter for detecting torn reads
//! - Separate flags for playing state and seek events
//!
//! ## Usage
//!
//! ```ignore
//! let snapshot = Arc::new(TransportSnapshot::new());
//!
//! // Main loop (writer)
//! snapshot.update(current_beat, tempo, playing);
//!
//! // Background thread (reader)
//! let (beat, tempo, playing) = snapshot.read();
//! ```

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Flag bits for the flags field.
const FLAG_PLAYING: u32 = 1 << 0;
const FLAG_SEEK_PENDING: u32 = 1 << 1;

/// Lock-free transport state snapshot.
///
/// Allows the main loop to share transport state with background threads
/// without blocking. Uses atomic operations for wait-free access.
#[derive(Debug)]
pub struct TransportSnapshot {
    /// Current beat position as f64 bits.
    beat_bits: AtomicU64,
    /// Tempo in BPM as f64 bits.
    tempo_bits: AtomicU64,
    /// Flags: bit 0 = playing, bit 1 = seek_pending.
    flags: AtomicU32,
    /// Write sequence number for consistent reads.
    /// Odd = write in progress, even = stable.
    sequence: AtomicU64,
}

impl Default for TransportSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportSnapshot {
    /// Create a new transport snapshot with default values.
    ///
    /// Initial state: beat=0.0, tempo=120.0, stopped.
    pub fn new() -> Self {
        Self {
            beat_bits: AtomicU64::new(0.0_f64.to_bits()),
            tempo_bits: AtomicU64::new(120.0_f64.to_bits()),
            flags: AtomicU32::new(0),
            sequence: AtomicU64::new(0),
        }
    }

    /// Update the transport state (single writer - main loop only).
    ///
    /// This should only be called from the main runtime tick loop.
    /// Multiple concurrent writers will corrupt the state.
    #[inline]
    pub fn update(&self, beat: f64, tempo: f64, playing: bool) {
        // Increment sequence to odd (write in progress)
        let seq = self.sequence.fetch_add(1, Ordering::Release);
        debug_assert!(seq % 2 == 0, "Concurrent writers detected");

        // Write the values
        self.beat_bits.store(beat.to_bits(), Ordering::Relaxed);
        self.tempo_bits.store(tempo.to_bits(), Ordering::Relaxed);

        // Update playing flag (preserve seek flag)
        let current_flags = self.flags.load(Ordering::Relaxed);
        let new_flags = if playing {
            current_flags | FLAG_PLAYING
        } else {
            current_flags & !FLAG_PLAYING
        };
        self.flags.store(new_flags, Ordering::Relaxed);

        // Increment sequence to even (write complete)
        self.sequence.fetch_add(1, Ordering::Release);
    }

    /// Read the current transport state (multiple readers - lock-free).
    ///
    /// Returns (beat, tempo, playing).
    ///
    /// Uses a seqlock pattern: retries if a write was in progress.
    #[inline]
    pub fn read(&self) -> (f64, f64, bool) {
        loop {
            // Read sequence before
            let seq1 = self.sequence.load(Ordering::Acquire);

            // If odd, a write is in progress - spin
            if seq1 % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }

            // Read values
            let beat_bits = self.beat_bits.load(Ordering::Relaxed);
            let tempo_bits = self.tempo_bits.load(Ordering::Relaxed);
            let flags = self.flags.load(Ordering::Relaxed);

            // Read sequence after
            let seq2 = self.sequence.load(Ordering::Acquire);

            // If sequence changed, a write occurred - retry
            if seq1 != seq2 {
                std::hint::spin_loop();
                continue;
            }

            // Values are consistent
            let beat = f64::from_bits(beat_bits);
            let tempo = f64::from_bits(tempo_bits);
            let playing = (flags & FLAG_PLAYING) != 0;

            return (beat, tempo, playing);
        }
    }

    /// Read only the beat position (faster than full read).
    #[inline]
    pub fn read_beat(&self) -> f64 {
        loop {
            let seq1 = self.sequence.load(Ordering::Acquire);
            if seq1 % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }

            let beat_bits = self.beat_bits.load(Ordering::Relaxed);

            let seq2 = self.sequence.load(Ordering::Acquire);
            if seq1 != seq2 {
                std::hint::spin_loop();
                continue;
            }

            return f64::from_bits(beat_bits);
        }
    }

    /// Signal that a seek operation occurred.
    ///
    /// The clock thread should call `check_and_clear_seek()` to detect this
    /// and reset its internal state.
    pub fn signal_seek(&self) {
        self.flags.fetch_or(FLAG_SEEK_PENDING, Ordering::Release);
    }

    /// Check if a seek occurred since the last check, and clear the flag.
    ///
    /// Returns true if a seek was pending.
    pub fn check_and_clear_seek(&self) -> bool {
        let old_flags = self.flags.fetch_and(!FLAG_SEEK_PENDING, Ordering::AcqRel);
        (old_flags & FLAG_SEEK_PENDING) != 0
    }

    /// Check if the transport is currently playing.
    #[inline]
    pub fn is_playing(&self) -> bool {
        let flags = self.flags.load(Ordering::Acquire);
        (flags & FLAG_PLAYING) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_update_and_read() {
        let snapshot = TransportSnapshot::new();

        // Initial state
        let (beat, tempo, playing) = snapshot.read();
        assert_eq!(beat, 0.0);
        assert_eq!(tempo, 120.0);
        assert!(!playing);

        // Update
        snapshot.update(4.5, 140.0, true);

        let (beat, tempo, playing) = snapshot.read();
        assert_eq!(beat, 4.5);
        assert_eq!(tempo, 140.0);
        assert!(playing);
    }

    #[test]
    fn test_seek_flag() {
        let snapshot = TransportSnapshot::new();

        // No seek pending initially
        assert!(!snapshot.check_and_clear_seek());

        // Signal seek
        snapshot.signal_seek();
        assert!(snapshot.check_and_clear_seek());

        // Flag should be cleared
        assert!(!snapshot.check_and_clear_seek());
    }

    #[test]
    fn test_concurrent_readers() {
        let snapshot = Arc::new(TransportSnapshot::new());

        // Spawn multiple reader threads
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let snap = Arc::clone(&snapshot);
                thread::spawn(move || {
                    for _ in 0..1000 {
                        let (beat, tempo, playing) = snap.read();
                        // Values should be consistent (not torn)
                        assert!(beat >= 0.0);
                        assert!(tempo > 0.0);
                        let _ = playing;
                    }
                })
            })
            .collect();

        // Writer thread
        for i in 0..1000 {
            snapshot.update(i as f64 * 0.1, 120.0 + (i as f64 * 0.01), i % 2 == 0);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_read_beat() {
        let snapshot = TransportSnapshot::new();
        snapshot.update(42.5, 180.0, true);
        assert_eq!(snapshot.read_beat(), 42.5);
    }

    #[test]
    fn test_is_playing() {
        let snapshot = TransportSnapshot::new();
        assert!(!snapshot.is_playing());

        snapshot.update(0.0, 120.0, true);
        assert!(snapshot.is_playing());

        snapshot.update(1.0, 120.0, false);
        assert!(!snapshot.is_playing());
    }
}
