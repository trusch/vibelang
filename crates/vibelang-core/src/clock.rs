//! Transport clock for wall-clock to beat-time conversion.
//!
//! The [`TransportClock`] manages the relationship between real time (wall-clock)
//! and musical time (beats). It handles tempo changes while preserving the current
//! beat position.

use crate::compat::Instant;
use crate::types::Beat;

/// Manages wall-clock to beat-time conversion.
///
/// The clock converts between real time (Instant) and musical time (Beat)
/// based on the current tempo. It supports:
///
/// - Starting/stopping playback
/// - Tempo changes that preserve beat position
/// - Converting between time domains
///
/// # Example
///
/// ```ignore
/// let mut clock = TransportClock::new(120.0);
///
/// // Start playback
/// clock.start(Beat::ZERO, Instant::now());
///
/// // Later, get current beat
/// let beat = clock.instant_to_beat(Instant::now());
///
/// // Change tempo (beat position preserved)
/// clock.set_tempo(140.0, Instant::now());
/// ```
#[derive(Debug, Clone)]
pub struct TransportClock {
    /// When playback started (wall clock). None if stopped.
    start_instant: Option<Instant>,

    /// Beat position when playback started.
    start_beat: Beat,

    /// Current tempo in BPM.
    tempo: f64,
}

impl TransportClock {
    /// Create a new transport clock with the given tempo.
    pub fn new(tempo: f64) -> Self {
        Self {
            start_instant: None,
            start_beat: Beat::ZERO,
            tempo,
        }
    }

    /// Check if the clock is running.
    pub fn is_running(&self) -> bool {
        self.start_instant.is_some()
    }

    /// Get the current tempo.
    pub fn tempo(&self) -> f64 {
        self.tempo
    }

    /// Start playback from the given beat position.
    ///
    /// If already running, this resets the start point.
    pub fn start(&mut self, from_beat: Beat, now: Instant) {
        self.start_instant = Some(now);
        self.start_beat = from_beat;
    }

    /// Stop playback and return the current beat position.
    ///
    /// Returns the beat position at the moment of stopping.
    pub fn stop(&mut self, now: Instant) -> Beat {
        let beat = self.instant_to_beat(now);
        self.start_instant = None;
        self.start_beat = beat;
        beat
    }

    /// Update tempo while preserving the current beat position.
    ///
    /// This recalculates the start instant so that the current beat
    /// remains the same despite the tempo change.
    pub fn set_tempo(&mut self, bpm: f64, now: Instant) {
        if self.is_running() {
            // Calculate current beat position
            let current_beat = self.instant_to_beat(now);
            // Update start point to preserve position
            self.start_instant = Some(now);
            self.start_beat = current_beat;
        }
        self.tempo = bpm;
    }

    /// Convert a wall-clock instant to beat position.
    ///
    /// If the clock is stopped, returns the last known beat position.
    pub fn instant_to_beat(&self, instant: Instant) -> Beat {
        match self.start_instant {
            Some(start) => {
                let elapsed_secs = instant.duration_since(start).as_secs_f64();
                let elapsed_beats = elapsed_secs * self.tempo / 60.0;
                self.start_beat + Beat::from_f64(elapsed_beats)
            }
            None => self.start_beat,
        }
    }

    /// Convert a beat position to wall-clock instant.
    ///
    /// Returns None if the clock is stopped.
    ///
    /// Useful for scheduling events at specific beat positions.
    #[allow(dead_code)] // Public API for future use
    pub fn beat_to_instant(&self, beat: Beat) -> Option<Instant> {
        self.start_instant.map(|start| {
            let beats_from_start = (beat - self.start_beat).to_f64();
            let secs_from_start = beats_from_start * 60.0 / self.tempo;

            if secs_from_start >= 0.0 {
                start + std::time::Duration::from_secs_f64(secs_from_start)
            } else {
                // Beat is before start, subtract duration
                start - std::time::Duration::from_secs_f64(-secs_from_start)
            }
        })
    }

    /// Get the current beat position.
    ///
    /// Convenience method that uses the current instant.
    #[allow(dead_code)] // Public API for future use
    pub fn current_beat(&self) -> Beat {
        self.instant_to_beat(Instant::now())
    }

    /// Seek to a specific beat position.
    ///
    /// If running, adjusts the start point to make the current time
    /// correspond to the given beat.
    pub fn seek(&mut self, beat: Beat, now: Instant) {
        if self.is_running() {
            self.start_instant = Some(now);
        }
        self.start_beat = beat;
    }
}

impl Default for TransportClock {
    fn default() -> Self {
        Self::new(120.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_clock_creation() {
        let clock = TransportClock::new(120.0);
        assert_eq!(clock.tempo(), 120.0);
        assert!(!clock.is_running());
    }

    #[test]
    fn test_start_stop() {
        let mut clock = TransportClock::new(120.0);
        let now = Instant::now();

        clock.start(Beat::ZERO, now);
        assert!(clock.is_running());

        let beat = clock.stop(now);
        assert!(!clock.is_running());
        assert_eq!(beat, Beat::ZERO);
    }

    #[test]
    fn test_instant_to_beat() {
        let mut clock = TransportClock::new(120.0); // 2 beats per second
        let now = Instant::now();

        clock.start(Beat::ZERO, now);

        // After 1 second at 120 BPM, should be at beat 2
        let later = now + Duration::from_secs(1);
        let beat = clock.instant_to_beat(later);
        assert!((beat.to_f64() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_beat_to_instant() {
        let mut clock = TransportClock::new(120.0);
        let now = Instant::now();

        clock.start(Beat::ZERO, now);

        // Beat 2 should be 1 second after start
        let instant = clock.beat_to_instant(Beat::from_f64(2.0)).unwrap();
        let diff = instant.duration_since(now).as_secs_f64();
        assert!((diff - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_tempo_change_preserves_position() {
        let mut clock = TransportClock::new(120.0);
        let start = Instant::now();

        clock.start(Beat::ZERO, start);

        // After 1 second at 120 BPM = beat 2
        let later = start + Duration::from_secs(1);
        let beat_before = clock.instant_to_beat(later);

        // Change tempo
        clock.set_tempo(240.0, later);

        // Beat position should still be 2
        let beat_after = clock.instant_to_beat(later);
        assert!((beat_before.to_f64() - beat_after.to_f64()).abs() < 0.001);
    }

    #[test]
    fn test_seek() {
        let mut clock = TransportClock::new(120.0);
        let now = Instant::now();

        clock.start(Beat::ZERO, now);
        clock.seek(Beat::from_f64(16.0), now);

        let beat = clock.instant_to_beat(now);
        assert!((beat.to_f64() - 16.0).abs() < 0.001);
    }
}
