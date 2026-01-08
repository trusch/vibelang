//! Time-related types with fixed-point precision.
//!
//! This module provides types for representing musical time:
//!
//! - [`Beat`]: A beat position with sub-beat precision (fixed-point)
//! - [`Duration`]: A duration in beats
//! - [`TimeSignature`]: Musical time signature (e.g., 4/4, 3/4)
//!
//! # Fixed-Point Precision
//!
//! [`Beat`] uses fixed-point arithmetic (16 fractional bits) to avoid
//! floating-point drift over long sessions. This ensures deterministic
//! timing even after hours of playback.

use std::fmt;
use std::ops::{Add, AddAssign, Rem, Sub, SubAssign};

/// Number of fractional bits for fixed-point beat representation.
const FRACTIONAL_BITS: i64 = 16;

/// Scale factor for fixed-point conversion.
const SCALE: f64 = (1 << FRACTIONAL_BITS) as f64;

/// A beat position with fixed-point precision.
///
/// Uses 16 fractional bits, providing precision to 1/65536 of a beat.
/// This avoids floating-point drift over long sessions.
///
/// # Example
///
/// ```
/// use vibelang_core2::types::time::Beat;
///
/// let beat = Beat::from_f64(1.5);
/// assert!((beat.to_f64() - 1.5).abs() < 0.0001);
///
/// let sum = beat + Beat::from_f64(0.5);
/// assert!((sum.to_f64() - 2.0).abs() < 0.0001);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Beat(i64);

impl Beat {
    /// Beat zero (the start).
    pub const ZERO: Beat = Beat(0);

    /// One beat.
    pub const ONE: Beat = Beat(1 << FRACTIONAL_BITS);

    /// Create a beat from a floating-point value.
    #[inline]
    pub fn from_f64(beats: f64) -> Self {
        Beat((beats * SCALE) as i64)
    }

    /// Convert to a floating-point value.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / SCALE
    }

    /// Get the raw fixed-point value.
    #[inline]
    pub const fn raw(self) -> i64 {
        self.0
    }

    /// Create from a raw fixed-point value.
    #[inline]
    pub const fn from_raw(raw: i64) -> Self {
        Beat(raw)
    }

    /// Get the whole beat number (floor).
    #[inline]
    pub fn floor(self) -> i64 {
        self.0 >> FRACTIONAL_BITS
    }

    /// Get the fractional part as a float (0.0 to 1.0).
    #[inline]
    pub fn frac(self) -> f64 {
        let mask = (1 << FRACTIONAL_BITS) - 1;
        (self.0 & mask) as f64 / SCALE
    }

    /// Check if this is exactly on a beat boundary.
    #[inline]
    pub fn is_whole(self) -> bool {
        let mask = (1 << FRACTIONAL_BITS) - 1;
        (self.0 & mask) == 0
    }

    /// Quantize to the nearest grid position.
    pub fn quantize(self, grid: Beat) -> Beat {
        if grid.0 == 0 {
            return self;
        }
        let half = grid.0 / 2;
        Beat(((self.0 + half) / grid.0) * grid.0)
    }

    /// Modulo operation for looping.
    pub fn modulo(self, length: Beat) -> Beat {
        if length.0 == 0 {
            return Beat::ZERO;
        }
        let mut result = self.0 % length.0;
        if result < 0 {
            result += length.0;
        }
        Beat(result)
    }
}

impl Add for Beat {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        Beat(self.0 + rhs.0)
    }
}

impl AddAssign for Beat {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Beat {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Beat(self.0 - rhs.0)
    }
}

impl SubAssign for Beat {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Rem for Beat {
    type Output = Self;

    #[inline]
    fn rem(self, rhs: Self) -> Self {
        self.modulo(rhs)
    }
}

impl fmt::Debug for Beat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Beat({:.4})", self.to_f64())
    }
}

impl fmt::Display for Beat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.to_f64())
    }
}

/// A duration in beats.
///
/// Wraps [`Beat`] to semantically distinguish positions from lengths.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Duration(Beat);

impl Duration {
    /// Zero duration.
    pub const ZERO: Duration = Duration(Beat::ZERO);

    /// One beat duration.
    pub const ONE_BEAT: Duration = Duration(Beat::ONE);

    /// Create a duration from beats.
    #[inline]
    pub fn from_beats(beats: f64) -> Self {
        Duration(Beat::from_f64(beats))
    }

    /// Convert to beats as f64.
    #[inline]
    pub fn to_beats(self) -> f64 {
        self.0.to_f64()
    }

    /// Get the underlying beat value.
    #[inline]
    pub fn as_beat(self) -> Beat {
        self.0
    }

    /// Create a duration in bars given a time signature.
    #[inline]
    pub fn from_bars(bars: f64, time_sig: TimeSignature) -> Self {
        Duration::from_beats(bars * time_sig.beats_per_bar())
    }
}

impl fmt::Debug for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Duration({:.4} beats)", self.to_beats())
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.to_beats())
    }
}

/// Musical time signature.
///
/// Represents the number of beats per bar and the beat unit.
///
/// # Example
///
/// ```
/// use vibelang_core2::types::time::TimeSignature;
///
/// let ts = TimeSignature::new(4, 4);
/// assert_eq!(ts.beats_per_bar(), 4.0);
///
/// let ts_34 = TimeSignature::new(3, 4);
/// assert_eq!(ts_34.beats_per_bar(), 3.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimeSignature {
    /// Number of beats per bar.
    pub numerator: u8,
    /// Beat unit (4 = quarter note, 8 = eighth note, etc.).
    pub denominator: u8,
}

impl TimeSignature {
    /// Create a new time signature.
    #[inline]
    pub const fn new(numerator: u8, denominator: u8) -> Self {
        Self { numerator, denominator }
    }

    /// Standard 4/4 time.
    pub const COMMON_TIME: TimeSignature = TimeSignature::new(4, 4);

    /// Number of quarter-note beats per bar.
    ///
    /// For 4/4, this is 4. For 6/8, this is 3 (6 eighths = 3 quarters).
    #[inline]
    pub fn beats_per_bar(self) -> f64 {
        (self.numerator as f64) * (4.0 / self.denominator as f64)
    }

    /// Duration of one bar.
    #[inline]
    pub fn bar_duration(self) -> Duration {
        Duration::from_beats(self.beats_per_bar())
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::COMMON_TIME
    }
}

impl fmt::Display for TimeSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_conversion() {
        let beat = Beat::from_f64(2.5);
        assert!((beat.to_f64() - 2.5).abs() < 0.0001);
    }

    #[test]
    fn test_beat_arithmetic() {
        let a = Beat::from_f64(1.0);
        let b = Beat::from_f64(0.5);

        let sum = a + b;
        assert!((sum.to_f64() - 1.5).abs() < 0.0001);

        let diff = a - b;
        assert!((diff.to_f64() - 0.5).abs() < 0.0001);
    }

    #[test]
    fn test_beat_quantize() {
        let beat = Beat::from_f64(1.3);
        let grid = Beat::from_f64(0.5);

        let quantized = beat.quantize(grid);
        assert!((quantized.to_f64() - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_beat_modulo() {
        let beat = Beat::from_f64(5.5);
        let length = Beat::from_f64(4.0);

        let result = beat.modulo(length);
        assert!((result.to_f64() - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_time_signature() {
        let ts = TimeSignature::new(4, 4);
        assert_eq!(ts.beats_per_bar(), 4.0);

        let ts_68 = TimeSignature::new(6, 8);
        assert_eq!(ts_68.beats_per_bar(), 3.0);
    }

    #[test]
    fn test_duration() {
        let dur = Duration::from_beats(2.0);
        assert_eq!(dur.to_beats(), 2.0);

        let dur_bars = Duration::from_bars(2.0, TimeSignature::COMMON_TIME);
        assert_eq!(dur_bars.to_beats(), 8.0);
    }

    #[test]
    fn test_beat_constants() {
        assert_eq!(Beat::ZERO.to_f64(), 0.0);
        assert!((Beat::ONE.to_f64() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_beat_floor_and_frac() {
        let beat = Beat::from_f64(3.75);
        assert_eq!(beat.floor(), 3);
        assert!((beat.frac() - 0.75).abs() < 0.0001);
    }

    #[test]
    fn test_beat_is_whole() {
        assert!(Beat::from_f64(4.0).is_whole());
        assert!(!Beat::from_f64(4.5).is_whole());
        assert!(Beat::ZERO.is_whole());
    }

    #[test]
    fn test_beat_add_assign() {
        let mut beat = Beat::from_f64(1.0);
        beat += Beat::from_f64(0.5);
        assert!((beat.to_f64() - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_beat_sub_assign() {
        let mut beat = Beat::from_f64(2.0);
        beat -= Beat::from_f64(0.5);
        assert!((beat.to_f64() - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_beat_rem_operator() {
        let beat = Beat::from_f64(5.5);
        let length = Beat::from_f64(4.0);
        let result = beat % length;
        assert!((result.to_f64() - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_beat_negative_modulo() {
        // Negative beat positions should wrap correctly
        let beat = Beat::from_f64(-1.0);
        let length = Beat::from_f64(4.0);
        let result = beat.modulo(length);
        assert!((result.to_f64() - 3.0).abs() < 0.0001);
    }

    #[test]
    fn test_beat_ordering() {
        let a = Beat::from_f64(1.0);
        let b = Beat::from_f64(2.0);
        assert!(a < b);
        assert!(b > a);
        assert!(a == Beat::from_f64(1.0));
    }

    #[test]
    fn test_beat_debug_display() {
        let beat = Beat::from_f64(2.5);
        assert!(format!("{:?}", beat).contains("2.5"));
        assert!(format!("{}", beat).contains("2.5"));
    }

    #[test]
    fn test_time_signature_display() {
        let ts = TimeSignature::new(4, 4);
        assert_eq!(format!("{}", ts), "4/4");

        let ts_34 = TimeSignature::new(3, 4);
        assert_eq!(format!("{}", ts_34), "3/4");
    }

    #[test]
    fn test_duration_as_beat() {
        let dur = Duration::from_beats(2.5);
        let beat = dur.as_beat();
        assert!((beat.to_f64() - 2.5).abs() < 0.0001);
    }

    #[test]
    fn test_time_signature_bar_duration() {
        let ts = TimeSignature::new(4, 4);
        assert!((ts.bar_duration().to_beats() - 4.0).abs() < 0.0001);

        let ts_34 = TimeSignature::new(3, 4);
        assert!((ts_34.bar_duration().to_beats() - 3.0).abs() < 0.0001);
    }

    #[test]
    fn test_quantize_edge_cases() {
        // Quantize to zero grid should return original
        let beat = Beat::from_f64(1.3);
        let quantized = beat.quantize(Beat::ZERO);
        assert_eq!(beat, quantized);

        // Quantize already-on-grid beat
        let beat = Beat::from_f64(2.0);
        let quantized = beat.quantize(Beat::ONE);
        assert!((quantized.to_f64() - 2.0).abs() < 0.0001);
    }
}
