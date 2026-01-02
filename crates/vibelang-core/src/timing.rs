//! Timing primitives for transport and scheduling.
//!
//! This module provides the fundamental timing types used throughout VibeLang:
//!
//! - [`BeatTime`] - Fixed-point beat representation for precise timing
//! - [`TimeSignature`] - Musical time signature (e.g., 4/4, 3/4)
//! - [`TransportClock`] - Transport-aware clock for beat/time conversion
//! - [`LatencyCompensation`] - Configurable latency for network/audio compensation

#[cfg(feature = "native")]
use rosc::OscTime;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Fixed-point beat representation with 16 fractional bits.
///
/// This provides sub-beat precision while maintaining deterministic arithmetic.
/// Using fixed-point avoids floating-point drift over long sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BeatTime {
    beats: i64,
}

impl BeatTime {
    const SCALE: i64 = 65_536;

    /// Zero beat time constant.
    pub const ZERO: BeatTime = BeatTime { beats: 0 };

    /// Create a BeatTime from a floating-point beat value.
    #[inline]
    pub fn from_float(value: f64) -> Self {
        Self {
            beats: (value * Self::SCALE as f64).round() as i64,
        }
    }

    /// Convert to a floating-point beat value.
    #[inline]
    pub fn to_float(self) -> f64 {
        self.beats as f64 / Self::SCALE as f64
    }
}

impl std::ops::Add for BeatTime {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            beats: self.beats.saturating_add(rhs.beats),
        }
    }
}

impl std::ops::Sub for BeatTime {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            beats: self.beats.saturating_sub(rhs.beats),
        }
    }
}

impl From<f64> for BeatTime {
    fn from(value: f64) -> Self {
        BeatTime::from_float(value)
    }
}

impl From<BeatTime> for f64 {
    fn from(value: BeatTime) -> Self {
        value.to_float()
    }
}

/// Wrapper type for beats (floating-point).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beats(pub f64);

impl Beats {
    /// Get the beat value as f64.
    pub fn as_f64(self) -> f64 {
        self.0
    }
}

/// Wrapper type for bars (floating-point).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bars(pub f64);

impl Bars {
    /// Convert bars to beats using the given time signature.
    pub fn to_beats(self, signature: TimeSignature) -> Beats {
        Beats(self.0 * signature.beats_per_bar())
    }
}

/// Musical time signature (numerator/denominator).
///
/// The numerator indicates beats per bar, and the denominator indicates
/// the note value that gets one beat (4 = quarter note, 8 = eighth note).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeSignature {
    pub numerator: u32,
    pub denominator: u32,
}

impl TimeSignature {
    /// Create a new time signature.
    ///
    /// Values are clamped to at least 1 to prevent division by zero.
    pub fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator: numerator.max(1),
            denominator: denominator.max(1),
        }
    }

    /// Calculate the number of quarter-note beats per bar.
    ///
    /// For 4/4: 4 beats per bar
    /// For 3/4: 3 beats per bar
    /// For 6/8: 3 beats per bar (6 eighth notes = 3 quarter notes)
    pub fn beats_per_bar(&self) -> f64 {
        self.numerator as f64 * (4.0 / self.denominator as f64)
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::new(4, 4)
    }
}

/// Latency compensation configuration in milliseconds.
///
/// These values are added to scheduled event times to account for
/// network transmission, server processing, and audio buffering delays.
#[derive(Clone, Debug)]
pub struct LatencyCompensation {
    /// Network round-trip latency.
    pub network_latency_ms: f64,
    /// Server-side processing latency.
    pub server_processing_ms: f64,
    /// Audio buffer latency.
    pub audio_buffer_ms: f64,
    /// Additional safety margin.
    pub safety_margin_ms: f64,
}

impl Default for LatencyCompensation {
    fn default() -> Self {
        Self {
            network_latency_ms: 20.0,
            server_processing_ms: 10.0,
            audio_buffer_ms: 2.0,
            safety_margin_ms: 20.0,
        }
    }
}

impl LatencyCompensation {
    /// Total latency in milliseconds.
    pub fn total_ms(&self) -> f64 {
        self.network_latency_ms
            + self.server_processing_ms
            + self.audio_buffer_ms
            + self.safety_margin_ms
    }

    /// Total latency in seconds.
    pub fn total_seconds(&self) -> f64 {
        self.total_ms() / 1000.0
    }
}

/// Transport-aware clock for converting between wall-clock time and beats.
///
/// The clock maintains an anchor point (beat position at a specific instant)
/// and uses BPM to calculate beat positions at other times.
#[derive(Clone, Debug)]
pub struct TransportClock {
    bpm: f64,
    signature: TimeSignature,
    latency: LatencyCompensation,
    running: bool,
    anchor_instant: Instant,
    anchor_beat: BeatTime,
}

impl Default for TransportClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportClock {
    /// Create a new transport clock at 120 BPM, 4/4 time, stopped at beat 0.
    pub fn new() -> Self {
        Self {
            bpm: 120.0,
            signature: TimeSignature::default(),
            latency: LatencyCompensation::default(),
            running: false,
            anchor_instant: Instant::now(),
            anchor_beat: BeatTime::ZERO,
        }
    }

    /// Set the BPM, preserving the current beat position.
    pub fn set_bpm(&mut self, bpm: f64, now: Instant) {
        let beat = self.beat_at(now);
        self.anchor_beat = beat;
        self.anchor_instant = now;
        self.bpm = bpm.clamp(1.0, 999.0);
    }

    /// Set the time signature, preserving the current beat position.
    pub fn set_time_signature(&mut self, numerator: u32, denominator: u32, now: Instant) {
        let beat = self.beat_at(now);
        self.anchor_beat = beat;
        self.anchor_instant = now;
        self.signature = TimeSignature::new(numerator, denominator);
    }

    /// Get the current time signature.
    pub fn time_signature(&self) -> TimeSignature {
        self.signature
    }

    /// Get the current BPM.
    pub fn bpm(&self) -> f64 {
        self.bpm
    }

    /// Start the transport at the given instant.
    pub fn start(&mut self, now: Instant) {
        self.anchor_instant = now;
        self.running = true;
    }

    /// Stop the transport, preserving the current beat position.
    pub fn stop(&mut self, now: Instant) {
        self.anchor_beat = self.beat_at(now);
        self.running = false;
    }

    /// Seek to a specific beat position.
    pub fn seek(&mut self, beat: BeatTime, now: Instant) {
        self.anchor_beat = beat;
        self.anchor_instant = now;
    }

    /// Calculate the beat position at a given instant.
    pub fn beat_at(&self, time: Instant) -> BeatTime {
        if !self.running || time <= self.anchor_instant {
            return self.anchor_beat;
        }

        let elapsed = time.duration_since(self.anchor_instant).as_secs_f64();
        let beats_elapsed = (elapsed / 60.0) * self.bpm;
        self.anchor_beat + BeatTime::from_float(beats_elapsed)
    }

    /// Update the anchor to the current beat position.
    ///
    /// Call this periodically to prevent drift accumulation.
    pub fn update(&mut self, now: Instant) -> BeatTime {
        let beat = self.beat_at(now);
        if self.running {
            self.anchor_beat = beat;
            self.anchor_instant = now;
        }
        beat
    }

    /// Get the current beat as a float.
    pub fn current_beat(&self) -> f64 {
        self.anchor_beat.to_float()
    }

    /// Convert a beat position to an OSC timestamp for scheduling (native only).
    #[cfg(feature = "native")]
    pub fn beat_to_timestamp(&self, beat: BeatTime, now: Instant) -> OscTime {
        let (_, osc_time) = self.beat_to_timestamp_and_instant(beat, now);
        osc_time
    }

    /// Convert a beat position to both an OSC timestamp and an Instant (native only).
    /// The Instant represents when the synth will be "live" on scsynth.
    #[cfg(feature = "native")]
    pub fn beat_to_timestamp_and_instant(&self, beat: BeatTime, now: Instant) -> (Instant, OscTime) {
        let current = self.beat_at(now);
        let beats_until_target = beat.to_float() - current.to_float();
        let beats_per_second = self.bpm / 60.0;
        let mut seconds_until_target = beats_until_target / beats_per_second;
        seconds_until_target += self.latency.total_seconds();

        if seconds_until_target < 0.0 {
            seconds_until_target = self.latency.total_seconds().max(0.01);
        }

        let target_instant = now + Duration::from_secs_f64(seconds_until_target);
        let target_system = SystemTime::now() + Duration::from_secs_f64(seconds_until_target);
        (target_instant, self.system_time_to_ntp(target_system))
    }

    /// Convert a beat position to seconds from now.
    pub fn beat_to_seconds(&self, beat: BeatTime, now: Instant) -> f64 {
        let current = self.beat_at(now);
        let beats_until_target = beat.to_float() - current.to_float();
        let beats_per_second = self.bpm / 60.0;
        let mut seconds_until_target = beats_until_target / beats_per_second;
        seconds_until_target += self.latency.total_seconds();

        if seconds_until_target < 0.0 {
            seconds_until_target = self.latency.total_seconds().max(0.01);
        }

        seconds_until_target
    }

    #[cfg(feature = "native")]
    fn system_time_to_ntp(&self, time: SystemTime) -> OscTime {
        let elapsed = time
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));
        let ntp_seconds_total = elapsed.as_secs() + 2_208_988_800;
        let ntp_seconds = (ntp_seconds_total % (u32::MAX as u64 + 1)) as u32;
        let fractional = ((elapsed.subsec_nanos() as u64) << 32) / 1_000_000_000u64;
        let ntp_fractional = fractional as u32;
        OscTime::from((ntp_seconds, ntp_fractional))
    }

    /// Get the latency configuration.
    pub fn latency(&self) -> &LatencyCompensation {
        &self.latency
    }

    /// Get mutable access to the latency configuration.
    pub fn latency_mut(&mut self) -> &mut LatencyCompensation {
        &mut self.latency
    }

    /// Calculate how many beats fit in the given lookahead window.
    pub fn lookahead_beats(&self, lookahead_ms: u64) -> f64 {
        let seconds = lookahead_ms as f64 / 1000.0;
        seconds * (self.bpm / 60.0)
    }

    /// Check if the transport is running.
    pub fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_time_roundtrip() {
        for val in [0.0, 1.0, 1.5, 3.75, 100.0, -5.0] {
            let bt = BeatTime::from_float(val);
            let back = bt.to_float();
            assert!((back - val).abs() < 0.0001, "Roundtrip failed for {val}");
        }
    }

    #[test]
    fn test_transport_clock_beat_calculation() {
        let mut clock = TransportClock::new();
        clock.set_bpm(120.0, Instant::now());
        // At 120 BPM, 1 beat = 0.5 seconds
        let now = Instant::now();
        clock.start(now);
        // Simulate 0.5 seconds passing (should be ~1 beat)
        let later = now + Duration::from_millis(500);
        let beat = clock.beat_at(later);
        assert!((beat.to_float() - 1.0).abs() < 0.05);
    }

    #[test]
    fn test_latency_compensation() {
        let latency = LatencyCompensation::default();
        assert!((latency.total_ms() - 52.0).abs() < 0.001);
        assert!((latency.total_seconds() - 0.052).abs() < 0.0001);
    }
}
