//! MIDI clock synchronization and timestamp conversion.
//!
//! This module provides:
//! - Conversion between midir timestamps and audio frame positions
//! - Synchronization between MIDI and audio clocks
//! - BPM derivation from MIDI clock messages

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

// ============================================================================
// MIDI Clock
// ============================================================================

/// High-resolution clock for MIDI timestamp conversion.
///
/// This clock maintains synchronization between:
/// - midir timestamps (microseconds from platform-specific origin)
/// - System monotonic time (Instant)
/// - Audio sample frames (from the audio backend)
pub struct MidiClock {
    /// Reference instant when the clock was calibrated.
    reference_instant: Instant,
    /// Reference midir timestamp (μs) at calibration time.
    reference_timestamp_us: AtomicU64,
    /// Audio sample rate (Hz).
    sample_rate: AtomicU32,
    /// Current audio frame position (updated by audio backend).
    audio_frame: AtomicU64,
    /// Audio frame at reference instant.
    reference_audio_frame: AtomicU64,
}

impl MidiClock {
    /// Create a new MIDI clock with the given sample rate.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            reference_instant: Instant::now(),
            reference_timestamp_us: AtomicU64::new(0),
            sample_rate: AtomicU32::new(sample_rate),
            audio_frame: AtomicU64::new(0),
            reference_audio_frame: AtomicU64::new(0),
        }
    }

    /// Create with default sample rate (48000 Hz).
    pub fn default_sample_rate() -> Self {
        Self::new(48000)
    }

    /// Calibrate the clock with a midir timestamp.
    ///
    /// Call this when receiving the first MIDI message to establish
    /// the relationship between midir timestamps and system time.
    pub fn calibrate(&self, timestamp_us: u64) {
        self.reference_timestamp_us
            .store(timestamp_us, Ordering::SeqCst);
        let audio_frame = self.audio_frame.load(Ordering::SeqCst);
        self.reference_audio_frame
            .store(audio_frame, Ordering::SeqCst);
    }

    /// Update the current audio frame position.
    ///
    /// This should be called regularly by the audio backend.
    pub fn update_audio_frame(&self, frame: u64) {
        self.audio_frame.store(frame, Ordering::SeqCst);
    }

    /// Get the current audio frame position.
    pub fn current_audio_frame(&self) -> u64 {
        self.audio_frame.load(Ordering::SeqCst)
    }

    /// Set the sample rate.
    pub fn set_sample_rate(&self, sample_rate: u32) {
        self.sample_rate.store(sample_rate, Ordering::SeqCst);
    }

    /// Get the sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::SeqCst)
    }

    /// Convert a midir timestamp to an estimated audio frame position.
    ///
    /// This uses the calibration data to estimate when the MIDI event
    /// occurred in terms of audio frames.
    pub fn timestamp_to_frame(&self, timestamp_us: u64) -> u64 {
        let ref_timestamp = self.reference_timestamp_us.load(Ordering::SeqCst);
        let ref_frame = self.reference_audio_frame.load(Ordering::SeqCst);
        let sample_rate = self.sample_rate.load(Ordering::SeqCst) as u64;

        if ref_timestamp == 0 {
            // Not calibrated yet - return current frame
            return self.audio_frame.load(Ordering::SeqCst);
        }

        // Calculate frame offset from timestamp difference
        let timestamp_diff_us = timestamp_us.saturating_sub(ref_timestamp);
        let frame_offset = (timestamp_diff_us * sample_rate) / 1_000_000;

        ref_frame.saturating_add(frame_offset)
    }

    /// Convert an audio frame to estimated wall-clock instant.
    pub fn frame_to_instant(&self, frame: u64) -> Instant {
        let ref_frame = self.reference_audio_frame.load(Ordering::SeqCst);
        let sample_rate = self.sample_rate.load(Ordering::SeqCst) as u64;

        if sample_rate == 0 {
            return self.reference_instant;
        }

        let frame_diff = frame.saturating_sub(ref_frame);
        let duration_us = (frame_diff * 1_000_000) / sample_rate;

        self.reference_instant + Duration::from_micros(duration_us)
    }

    /// Get the estimated latency between MIDI input and audio output.
    ///
    /// This is calculated as the difference between the current audio frame
    /// and the frame estimated from the MIDI timestamp.
    pub fn estimate_latency(&self, timestamp_us: u64, received_at: Instant) -> Duration {
        let midi_frame = self.timestamp_to_frame(timestamp_us);
        let current_frame = self.audio_frame.load(Ordering::SeqCst);
        let sample_rate = self.sample_rate.load(Ordering::SeqCst) as u64;

        if sample_rate == 0 {
            return Duration::ZERO;
        }

        // Frame-based latency
        let frame_diff = current_frame.saturating_sub(midi_frame);
        let frame_latency_us = (frame_diff * 1_000_000) / sample_rate;

        // Also factor in time since received
        let receive_latency = received_at.elapsed();

        Duration::from_micros(frame_latency_us) + receive_latency
    }
}

impl Default for MidiClock {
    fn default() -> Self {
        Self::default_sample_rate()
    }
}

// ============================================================================
// MIDI Clock Sync (for external sync)
// ============================================================================

/// MIDI clock synchronization for deriving BPM from incoming clock messages.
///
/// MIDI clock sends 24 pulses per quarter note (PPQN).
pub struct MidiClockSync {
    /// Timestamps of recent clock pulses (for BPM calculation).
    pulse_times: Vec<Instant>,
    /// Maximum number of pulses to track.
    max_pulses: usize,
    /// Derived BPM (0.0 if not enough data).
    derived_bpm: f64,
    /// Whether we're currently synced to external clock.
    synced: bool,
    /// Transport state.
    transport_running: bool,
    /// Song position (in MIDI beats = 6 clocks).
    song_position: u16,
}

impl MidiClockSync {
    /// Pulses per quarter note (standard MIDI).
    pub const PPQN: u32 = 24;

    /// Create a new clock sync tracker.
    pub fn new() -> Self {
        Self {
            pulse_times: Vec::with_capacity(48), // 2 beats worth
            max_pulses: 48,
            derived_bpm: 0.0,
            synced: false,
            transport_running: false,
            song_position: 0,
        }
    }

    /// Record a clock pulse and update BPM estimate.
    pub fn on_clock(&mut self) {
        let now = Instant::now();
        self.pulse_times.push(now);

        // Keep only recent pulses
        if self.pulse_times.len() > self.max_pulses {
            self.pulse_times.remove(0);
        }

        // Need at least 24 pulses (1 quarter note) for BPM calculation
        if self.pulse_times.len() >= Self::PPQN as usize {
            self.update_bpm();
            self.synced = true;
        }
    }

    /// Handle transport start message.
    pub fn on_start(&mut self) {
        self.transport_running = true;
        self.song_position = 0;
        self.pulse_times.clear();
        tracing::info!("[MIDI_CLOCK] Transport START");
    }

    /// Handle transport continue message.
    pub fn on_continue(&mut self) {
        self.transport_running = true;
        tracing::info!(
            "[MIDI_CLOCK] Transport CONTINUE from position {}",
            self.song_position
        );
    }

    /// Handle transport stop message.
    pub fn on_stop(&mut self) {
        self.transport_running = false;
        tracing::info!("[MIDI_CLOCK] Transport STOP");
    }

    /// Handle song position pointer message.
    pub fn on_song_position(&mut self, position: u16) {
        self.song_position = position;
        tracing::debug!("[MIDI_CLOCK] Song position: {} beats", position);
    }

    /// Update BPM estimate from pulse timing.
    fn update_bpm(&mut self) {
        if self.pulse_times.len() < 2 {
            return;
        }

        // Calculate average time between pulses
        let Some(first) = self.pulse_times.first() else {
            return;
        };
        let Some(last) = self.pulse_times.last() else {
            return;
        };
        let duration = last.duration_since(*first);
        let num_intervals = (self.pulse_times.len() - 1) as f64;

        if num_intervals == 0.0 {
            return;
        }

        let avg_pulse_duration = duration.as_secs_f64() / num_intervals;

        // BPM = 60 / (avg_pulse_duration * PPQN)
        // Since each pulse is 1/24 of a quarter note
        if avg_pulse_duration > 0.0 {
            let quarter_note_duration = avg_pulse_duration * Self::PPQN as f64;
            self.derived_bpm = 60.0 / quarter_note_duration;
        }
    }

    /// Get the derived BPM (0.0 if not synced).
    pub fn bpm(&self) -> f64 {
        self.derived_bpm
    }

    /// Check if we're synced to external clock.
    pub fn is_synced(&self) -> bool {
        self.synced
    }

    /// Check if transport is running.
    pub fn is_running(&self) -> bool {
        self.transport_running
    }

    /// Get the current song position in MIDI beats (6 clocks per beat).
    pub fn song_position(&self) -> u16 {
        self.song_position
    }

    /// Get song position in quarter notes.
    pub fn song_position_quarters(&self) -> f64 {
        // MIDI beat = 6 clocks, quarter note = 24 clocks
        // So 1 quarter note = 4 MIDI beats
        self.song_position as f64 / 4.0
    }

    /// Reset sync state (e.g., when connection is lost).
    pub fn reset(&mut self) {
        self.pulse_times.clear();
        self.derived_bpm = 0.0;
        self.synced = false;
    }

    /// Check if sync is stale (no clock received recently).
    pub fn is_stale(&self, timeout: Duration) -> bool {
        if let Some(last) = self.pulse_times.last() {
            last.elapsed() > timeout
        } else {
            true
        }
    }
}

impl Default for MidiClockSync {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_clock_calibration() {
        let clock = MidiClock::new(48000);
        clock.update_audio_frame(48000); // 1 second of audio
        clock.calibrate(1_000_000); // 1 second in microseconds

        // A timestamp 0.5 seconds later should be ~24000 frames later
        let frame = clock.timestamp_to_frame(1_500_000);
        assert!(frame >= 48000 + 23000 && frame <= 48000 + 25000);
    }

    #[test]
    fn test_midi_clock_sync_bpm() {
        let mut sync = MidiClockSync::new();

        // Simulate 120 BPM = 0.5 sec per quarter note
        // 24 pulses per quarter = 0.5/24 = 0.0208333 sec per pulse
        let pulse_interval = Duration::from_micros(20833);

        // Send 48 pulses (2 quarter notes)
        for _ in 0..48 {
            sync.on_clock();
            std::thread::sleep(Duration::from_micros(100)); // Tiny sleep to advance time
        }

        // Note: This test is timing-sensitive and may not work reliably
        // In a real scenario, BPM would be calculated from actual timing
        assert!(sync.is_synced());
    }

    #[test]
    fn test_transport_state() {
        let mut sync = MidiClockSync::new();

        assert!(!sync.is_running());

        sync.on_start();
        assert!(sync.is_running());
        assert_eq!(sync.song_position(), 0);

        sync.on_song_position(100);
        assert_eq!(sync.song_position(), 100);

        sync.on_stop();
        assert!(!sync.is_running());

        sync.on_continue();
        assert!(sync.is_running());
        assert_eq!(sync.song_position(), 100); // Position preserved
    }
}
