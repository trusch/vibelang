//! MIDI jitter compensation for accurate timing.
//!
//! This module provides algorithms for compensating timing jitter
//! in MIDI input, enabling sample-accurate event scheduling.
//!
//! ## How It Works
//!
//! MIDI messages arrive with two timing sources:
//! 1. midir timestamp (μs) - from the MIDI driver, most accurate
//! 2. received_at (Instant) - when our callback received the message
//!
//! Jitter occurs because:
//! - USB polling intervals (typically 1ms)
//! - Operating system scheduling delays
//! - Buffer processing delays
//!
//! The compensator analyzes the relationship between these timestamps
//! to estimate and correct for jitter.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::events::TimestampedMidiEvent;

// ============================================================================
// Jitter Statistics
// ============================================================================

/// Statistics about timing jitter.
#[derive(Clone, Debug, Default)]
pub struct JitterStats {
    /// Number of samples collected.
    pub sample_count: usize,
    /// Mean latency (receive time - MIDI timestamp).
    pub mean_latency_us: f64,
    /// Standard deviation of latency.
    pub stddev_latency_us: f64,
    /// Minimum observed latency.
    pub min_latency_us: i64,
    /// Maximum observed latency.
    pub max_latency_us: i64,
    /// Estimated clock drift (μs per second).
    pub drift_estimate: f64,
}

// ============================================================================
// Jitter Compensator
// ============================================================================

/// Sample for jitter analysis.
#[derive(Clone, Copy, Debug)]
struct TimingSample {
    /// MIDI timestamp (μs).
    midi_timestamp_us: u64,
    /// System time when received.
    received_at: Instant,
    /// Calculated latency (received_at - midi_timestamp in our reference frame).
    latency_us: i64,
}

/// MIDI jitter compensator.
///
/// Analyzes timing samples to estimate jitter characteristics
/// and provide compensated scheduling times.
pub struct JitterCompensator {
    /// Rolling window of timing samples.
    samples: VecDeque<TimingSample>,
    /// Maximum samples to keep.
    max_samples: usize,
    /// Reference instant for timestamp conversion.
    reference_instant: Instant,
    /// Reference MIDI timestamp.
    reference_midi_us: u64,
    /// Whether we've been calibrated.
    calibrated: bool,
    /// Estimated base latency.
    base_latency_us: i64,
    /// Estimated clock drift (μs per second).
    drift_us_per_sec: f64,
    /// Compensation buffer (extra delay to smooth jitter).
    compensation_buffer_us: i64,
}

impl JitterCompensator {
    /// Default window size for sample collection.
    pub const DEFAULT_WINDOW_SIZE: usize = 256;

    /// Default compensation buffer (μs).
    pub const DEFAULT_BUFFER_US: i64 = 1000; // 1ms

    /// Create a new jitter compensator.
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(Self::DEFAULT_WINDOW_SIZE),
            max_samples: Self::DEFAULT_WINDOW_SIZE,
            reference_instant: Instant::now(),
            reference_midi_us: 0,
            calibrated: false,
            base_latency_us: 0,
            drift_us_per_sec: 0.0,
            compensation_buffer_us: Self::DEFAULT_BUFFER_US,
        }
    }

    /// Create with custom settings.
    pub fn with_settings(window_size: usize, buffer_us: i64) -> Self {
        Self {
            samples: VecDeque::with_capacity(window_size),
            max_samples: window_size,
            compensation_buffer_us: buffer_us,
            ..Self::new()
        }
    }

    /// Set the compensation buffer size.
    pub fn set_buffer(&mut self, buffer_us: i64) {
        self.compensation_buffer_us = buffer_us;
    }

    /// Add a timing sample and update estimates.
    pub fn add_sample(&mut self, midi_timestamp_us: u64, received_at: Instant) {
        // Calibrate on first sample
        if !self.calibrated {
            self.reference_instant = received_at;
            self.reference_midi_us = midi_timestamp_us;
            self.calibrated = true;
        }

        // Calculate latency in our reference frame
        let expected_instant = self.midi_timestamp_to_instant(midi_timestamp_us);
        let latency = if received_at >= expected_instant {
            received_at.duration_since(expected_instant).as_micros() as i64
        } else {
            -(expected_instant.duration_since(received_at).as_micros() as i64)
        };

        let sample = TimingSample {
            midi_timestamp_us,
            received_at,
            latency_us: latency,
        };

        self.samples.push_back(sample);

        // Trim to window size
        while self.samples.len() > self.max_samples {
            self.samples.pop_front();
        }

        // Update estimates periodically
        if self.samples.len() >= 16 && self.samples.len() % 16 == 0 {
            self.update_estimates();
        }
    }

    /// Add a sample from an event.
    pub fn add_event(&mut self, event: &TimestampedMidiEvent) {
        self.add_sample(event.timestamp_us, event.received_at);
    }

    /// Update latency and drift estimates.
    fn update_estimates(&mut self) {
        if self.samples.len() < 4 {
            return;
        }

        // Calculate mean latency
        let sum: i64 = self.samples.iter().map(|s| s.latency_us).sum();
        let mean = sum as f64 / self.samples.len() as f64;
        self.base_latency_us = mean as i64;

        // Calculate drift using linear regression
        // We compare (midi_timestamp - ref) vs (received_at - ref)
        if self.samples.len() >= 32 {
            self.estimate_drift();
        }
    }

    /// Estimate clock drift using linear regression.
    fn estimate_drift(&mut self) {
        let n = self.samples.len() as f64;
        if n < 2.0 {
            return;
        }

        // x = MIDI timestamp offset (in seconds)
        // y = measured latency
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;

        for sample in &self.samples {
            let x = (sample.midi_timestamp_us - self.reference_midi_us) as f64 / 1_000_000.0;
            let y = sample.latency_us as f64;

            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        let denom = n * sum_xx - sum_x * sum_x;
        if denom.abs() > 1e-10 {
            // Slope = drift in μs per second of MIDI time
            self.drift_us_per_sec = (n * sum_xy - sum_x * sum_y) / denom;
        }
    }

    /// Convert a MIDI timestamp to an Instant in our reference frame.
    fn midi_timestamp_to_instant(&self, midi_timestamp_us: u64) -> Instant {
        let offset_us = midi_timestamp_us.saturating_sub(self.reference_midi_us);
        self.reference_instant + Duration::from_micros(offset_us)
    }

    /// Get the compensated scheduling instant for an event.
    ///
    /// This returns the instant when the event should be processed
    /// to maintain consistent timing despite jitter.
    pub fn compensate(&self, event: &TimestampedMidiEvent) -> Instant {
        if !self.calibrated {
            // Not calibrated - just use received time
            return event.received_at;
        }

        // Calculate expected instant from MIDI timestamp
        let midi_instant = self.midi_timestamp_to_instant(event.timestamp_us);

        // Apply drift correction
        let elapsed_sec =
            (event.timestamp_us - self.reference_midi_us) as f64 / 1_000_000.0;
        let drift_correction_us = (self.drift_us_per_sec * elapsed_sec) as i64;

        // Calculate target instant:
        // MIDI instant + base latency + drift correction + buffer
        let total_offset_us =
            self.base_latency_us + drift_correction_us + self.compensation_buffer_us;

        if total_offset_us >= 0 {
            midi_instant + Duration::from_micros(total_offset_us as u64)
        } else {
            midi_instant - Duration::from_micros((-total_offset_us) as u64)
        }
    }

    /// Get the delay needed before processing an event.
    ///
    /// Returns `None` if the event should be processed immediately.
    pub fn delay_for(&self, event: &TimestampedMidiEvent) -> Option<Duration> {
        let target = self.compensate(event);
        let now = Instant::now();

        if target > now {
            Some(target - now)
        } else {
            None // Process immediately
        }
    }

    /// Get current jitter statistics.
    pub fn stats(&self) -> JitterStats {
        if self.samples.is_empty() {
            return JitterStats::default();
        }

        let n = self.samples.len();
        let sum: i64 = self.samples.iter().map(|s| s.latency_us).sum();
        let mean = sum as f64 / n as f64;

        // Calculate standard deviation
        let variance: f64 = self.samples
            .iter()
            .map(|s| {
                let diff = s.latency_us as f64 - mean;
                diff * diff
            })
            .sum::<f64>()
            / n as f64;
        let stddev = variance.sqrt();

        // Min/max
        let min = self.samples.iter().map(|s| s.latency_us).min().unwrap_or(0);
        let max = self.samples.iter().map(|s| s.latency_us).max().unwrap_or(0);

        JitterStats {
            sample_count: n,
            mean_latency_us: mean,
            stddev_latency_us: stddev,
            min_latency_us: min,
            max_latency_us: max,
            drift_estimate: self.drift_us_per_sec,
        }
    }

    /// Check if the compensator is calibrated.
    pub fn is_calibrated(&self) -> bool {
        self.calibrated
    }

    /// Reset the compensator state.
    pub fn reset(&mut self) {
        self.samples.clear();
        self.calibrated = false;
        self.base_latency_us = 0;
        self.drift_us_per_sec = 0.0;
    }

    /// Get the estimated base latency in microseconds.
    pub fn base_latency(&self) -> Duration {
        Duration::from_micros(self.base_latency_us.unsigned_abs())
    }

    /// Get the estimated clock drift in microseconds per second.
    pub fn drift(&self) -> f64 {
        self.drift_us_per_sec
    }
}

impl Default for JitterCompensator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Simple Moving Average Filter
// ============================================================================

/// Simple moving average filter for smoothing timing values.
pub struct MovingAverage {
    values: VecDeque<f64>,
    window_size: usize,
    sum: f64,
}

impl MovingAverage {
    /// Create a new moving average filter.
    pub fn new(window_size: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(window_size),
            window_size,
            sum: 0.0,
        }
    }

    /// Add a value and get the current average.
    pub fn add(&mut self, value: f64) -> f64 {
        self.sum += value;
        self.values.push_back(value);

        if self.values.len() > self.window_size {
            if let Some(old) = self.values.pop_front() {
                self.sum -= old;
            }
        }

        self.average()
    }

    /// Get the current average.
    pub fn average(&self) -> f64 {
        if self.values.is_empty() {
            0.0
        } else {
            self.sum / self.values.len() as f64
        }
    }

    /// Reset the filter.
    pub fn reset(&mut self) {
        self.values.clear();
        self.sum = 0.0;
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

    fn make_event(timestamp_us: u64, received_at: Instant) -> TimestampedMidiEvent {
        TimestampedMidiEvent {
            timestamp_us,
            received_at,
            device_id: MidiDeviceId::new(0),
            message: MidiMessage::NoteOn {
                channel: Channel::new(0),
                note: 60,
                velocity: Velocity::from_midi1(100),
            },
        }
    }

    #[test]
    fn test_jitter_compensator_calibration() {
        let mut comp = JitterCompensator::new();
        assert!(!comp.is_calibrated());

        let now = Instant::now();
        comp.add_sample(1000, now);
        assert!(comp.is_calibrated());
    }

    #[test]
    fn test_jitter_stats() {
        let mut comp = JitterCompensator::new();
        let start = Instant::now();

        // Add samples with varying latency
        for i in 0..100 {
            let midi_ts = i as u64 * 1000; // 1ms apart
            let receive_time = start + Duration::from_micros(midi_ts + 500 + (i % 10) as u64 * 100);
            comp.add_sample(midi_ts, receive_time);
        }

        let stats = comp.stats();
        assert_eq!(stats.sample_count, 100);
        assert!(stats.mean_latency_us > 0.0);
        assert!(stats.stddev_latency_us >= 0.0);
    }

    #[test]
    fn test_moving_average() {
        let mut avg = MovingAverage::new(4);

        assert_eq!(avg.add(10.0), 10.0);
        assert_eq!(avg.add(20.0), 15.0);
        assert_eq!(avg.add(30.0), 20.0);
        assert_eq!(avg.add(40.0), 25.0);
        assert_eq!(avg.add(50.0), 35.0); // Window: 20, 30, 40, 50
    }

    #[test]
    fn test_compensation_delay() {
        let mut comp = JitterCompensator::with_settings(64, 2000); // 2ms buffer
        let start = Instant::now();

        // Calibrate with consistent latency
        for i in 0..32 {
            let midi_ts = i as u64 * 1000;
            let receive_time = start + Duration::from_micros(midi_ts + 500);
            comp.add_sample(midi_ts, receive_time);
        }

        // Create an event
        let event = make_event(32000, start + Duration::from_micros(32500));

        // The delay should account for our buffer
        let target = comp.compensate(&event);
        assert!(target >= event.received_at);
    }
}
