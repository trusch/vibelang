//! Fades trait for parameter automation.
//!
//! Fades smoothly transition parameters over time.

use crate::types::{Duration, EffectId, GroupId, MelodyId, PatternId, VoiceId};
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI};

/// Target for a fade operation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FadeTarget {
    /// Fade a group parameter.
    Group(GroupId),

    /// Fade a voice parameter.
    Voice(VoiceId),

    /// Fade a pattern parameter.
    Pattern(PatternId),

    /// Fade a melody parameter.
    Melody(MelodyId),

    /// Fade an effect parameter.
    Effect(EffectId),
}

/// Curve type for fade interpolation.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FadeCurve {
    /// Linear interpolation (t).
    #[default]
    Linear,

    // === Ease curves (quadratic) ===
    /// Ease-in: slow start, fast end (t²).
    EaseIn,

    /// Ease-out: fast start, slow end (1 - (1-t)²).
    EaseOut,

    /// Ease-in-out: smooth S-curve using quadratic blend.
    EaseInOut,

    // === Sine-based curves ===
    /// Sine-in: slow start using cosine (1 - cos(t * π/2)).
    SineIn,

    /// Sine-out: slow end using sine (sin(t * π/2)).
    SineOut,

    /// Sine-in-out: smooth start and end ((1 - cos(t * π)) / 2).
    SineInOut,

    // === Cubic curves ===
    /// Cubic-in: very slow start (t³).
    CubicIn,

    /// Cubic-out: very slow end (1 - (1-t)³).
    CubicOut,

    /// Cubic-in-out: smooth cubic S-curve.
    CubicInOut,

    // === Configurable curves ===
    /// Exponential curve with configurable exponent (t^exponent).
    Exponential {
        #[serde(default = "default_exponent")]
        exponent: f32,
    },

    /// Logarithmic curve (fast start, slow end).
    Logarithmic,

    /// Step function (instant jump at end).
    Step,

    /// Custom cubic spline with user-defined control points.
    /// Points are (time, value) pairs normalized 0-1.
    CubicSpline { points: Vec<(f32, f32)> },
}

fn default_exponent() -> f32 {
    2.0
}

impl FadeCurve {
    /// Apply the curve transformation to a normalized time value (0.0 to 1.0).
    /// Returns a normalized output value (0.0 to 1.0).
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            FadeCurve::Linear => t,

            // Ease (quadratic)
            FadeCurve::EaseIn => t * t,
            FadeCurve::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            FadeCurve::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }

            // Sine
            FadeCurve::SineIn => 1.0 - (t * FRAC_PI_2).cos(),
            FadeCurve::SineOut => (t * FRAC_PI_2).sin(),
            FadeCurve::SineInOut => (1.0 - (t * PI).cos()) / 2.0,

            // Cubic
            FadeCurve::CubicIn => t * t * t,
            FadeCurve::CubicOut => 1.0 - (1.0 - t).powi(3),
            FadeCurve::CubicInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }

            // Configurable exponential
            FadeCurve::Exponential { exponent } => t.powf(*exponent),

            // Logarithmic (maps 0-1 to log10(1) to log10(10) = 0 to 1)
            FadeCurve::Logarithmic => (1.0 + t * 9.0).log10(),

            // Step (instant jump at end)
            FadeCurve::Step => {
                if t < 1.0 {
                    0.0
                } else {
                    1.0
                }
            }

            // Cubic spline with Catmull-Rom interpolation
            FadeCurve::CubicSpline { points } => cubic_spline_interpolate(t, points),
        }
    }
}

/// Cubic spline interpolation with user-defined control points using Catmull-Rom.
fn cubic_spline_interpolate(t: f32, points: &[(f32, f32)]) -> f32 {
    if points.is_empty() {
        return t; // Fallback to linear
    }

    // Build complete point list with implicit start (0,0) and end (1,1)
    let mut all_points = vec![(0.0f32, 0.0f32)];
    all_points.extend(points.iter().cloned());
    all_points.push((1.0, 1.0));
    all_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Find the segment containing t
    let mut i = 0;
    while i < all_points.len() - 1 && all_points[i + 1].0 < t {
        i += 1;
    }

    if i >= all_points.len() - 1 {
        return all_points.last().map(|p| p.1).unwrap_or(1.0);
    }

    // Get four points for Catmull-Rom (p0, p1, p2, p3)
    let p0 = if i > 0 {
        all_points[i - 1]
    } else {
        all_points[i]
    };
    let p1 = all_points[i];
    let p2 = all_points[i + 1];
    let p3 = if i + 2 < all_points.len() {
        all_points[i + 2]
    } else {
        all_points[i + 1]
    };

    // Normalize t to segment
    let segment_t = if (p2.0 - p1.0).abs() > f32::EPSILON {
        (t - p1.0) / (p2.0 - p1.0)
    } else {
        0.0
    };

    catmull_rom(segment_t, p0.1, p1.1, p2.1, p3.1)
}

/// Catmull-Rom spline interpolation between p1 and p2.
fn catmull_rom(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;

    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Configuration for a fade operation.
#[derive(Clone, Debug, PartialEq)]
pub struct FadeConfig {
    /// Target to fade.
    pub target: FadeTarget,

    /// Parameter name to fade.
    pub param: String,

    /// Starting value (None = use current value).
    pub from: Option<f32>,

    /// Target value.
    pub to: f32,

    /// Duration of the fade.
    pub duration: Duration,

    /// Interpolation curve.
    pub curve: FadeCurve,
}

impl FadeConfig {
    /// Create a new fade configuration.
    pub fn new(target: FadeTarget, param: impl Into<String>, to: f32, duration: Duration) -> Self {
        Self {
            target,
            param: param.into(),
            from: None,
            to,
            duration,
            curve: FadeCurve::default(),
        }
    }

    /// Set the starting value.
    pub fn from(mut self, from: f32) -> Self {
        self.from = Some(from);
        self
    }

    /// Set the interpolation curve.
    pub fn with_curve(mut self, curve: FadeCurve) -> Self {
        self.curve = curve;
        self
    }

    /// Create a fade for a group.
    pub fn group(id: GroupId, param: impl Into<String>, to: f32, duration_beats: f64) -> Self {
        Self::new(
            FadeTarget::Group(id),
            param,
            to,
            Duration::from_beats(duration_beats),
        )
    }

    /// Create a fade for a voice.
    pub fn voice(id: VoiceId, param: impl Into<String>, to: f32, duration_beats: f64) -> Self {
        Self::new(
            FadeTarget::Voice(id),
            param,
            to,
            Duration::from_beats(duration_beats),
        )
    }

    /// Create a fade for an effect.
    pub fn effect(id: EffectId, param: impl Into<String>, to: f32, duration_beats: f64) -> Self {
        Self::new(
            FadeTarget::Effect(id),
            param,
            to,
            Duration::from_beats(duration_beats),
        )
    }
}

/// Fade management for parameter automation.
///
/// Fades smoothly interpolate parameter values over time, supporting
/// different interpolation curves.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait Fades: Send + Sync {
    /// Start a parameter fade.
    async fn fade(&self, config: FadeConfig) -> Result<()>;

    /// Cancel an active fade.
    async fn cancel(&self, target: &FadeTarget, param: &str) -> Result<()>;
}

/// Fade management for parameter automation (WASM version).
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait Fades {
    /// Start a parameter fade.
    async fn fade(&self, config: FadeConfig) -> Result<()>;

    /// Cancel an active fade.
    async fn cancel(&self, target: &FadeTarget, param: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to assert approximate equality for floating point
    fn assert_approx(actual: f32, expected: f32, tolerance: f32, msg: &str) {
        assert!(
            (actual - expected).abs() < tolerance,
            "{}: expected {} but got {} (diff: {})",
            msg,
            expected,
            actual,
            (actual - expected).abs()
        );
    }

    #[test]
    fn test_linear_curve() {
        let curve = FadeCurve::Linear;
        assert_approx(curve.apply(0.0), 0.0, 0.001, "linear at 0");
        assert_approx(curve.apply(0.25), 0.25, 0.001, "linear at 0.25");
        assert_approx(curve.apply(0.5), 0.5, 0.001, "linear at 0.5");
        assert_approx(curve.apply(0.75), 0.75, 0.001, "linear at 0.75");
        assert_approx(curve.apply(1.0), 1.0, 0.001, "linear at 1");
    }

    #[test]
    fn test_ease_in_curve() {
        let curve = FadeCurve::EaseIn;
        // EaseIn is t^2, so slower at start
        assert_approx(curve.apply(0.0), 0.0, 0.001, "ease_in at 0");
        assert_approx(curve.apply(0.5), 0.25, 0.001, "ease_in at 0.5 (t^2 = 0.25)");
        assert_approx(curve.apply(1.0), 1.0, 0.001, "ease_in at 1");
        // At 0.5, value should be less than 0.5 (slow start)
        assert!(curve.apply(0.5) < 0.5, "ease_in should be below linear at midpoint");
    }

    #[test]
    fn test_ease_out_curve() {
        let curve = FadeCurve::EaseOut;
        assert_approx(curve.apply(0.0), 0.0, 0.001, "ease_out at 0");
        assert_approx(curve.apply(0.5), 0.75, 0.001, "ease_out at 0.5");
        assert_approx(curve.apply(1.0), 1.0, 0.001, "ease_out at 1");
        // At 0.5, value should be more than 0.5 (fast start)
        assert!(curve.apply(0.5) > 0.5, "ease_out should be above linear at midpoint");
    }

    #[test]
    fn test_ease_in_out_curve() {
        let curve = FadeCurve::EaseInOut;
        assert_approx(curve.apply(0.0), 0.0, 0.001, "ease_in_out at 0");
        assert_approx(curve.apply(0.5), 0.5, 0.001, "ease_in_out at 0.5 (midpoint)");
        assert_approx(curve.apply(1.0), 1.0, 0.001, "ease_in_out at 1");
        // Should be symmetric around midpoint
        let at_25 = curve.apply(0.25);
        let at_75 = curve.apply(0.75);
        assert_approx(at_25 + at_75, 1.0, 0.01, "ease_in_out should be symmetric");
    }

    #[test]
    fn test_sine_curves() {
        // SineIn: 1 - cos(t * PI/2)
        let sine_in = FadeCurve::SineIn;
        assert_approx(sine_in.apply(0.0), 0.0, 0.001, "sine_in at 0");
        assert_approx(sine_in.apply(1.0), 1.0, 0.001, "sine_in at 1");
        assert!(sine_in.apply(0.5) < 0.5, "sine_in should be below linear at midpoint");

        // SineOut: sin(t * PI/2)
        let sine_out = FadeCurve::SineOut;
        assert_approx(sine_out.apply(0.0), 0.0, 0.001, "sine_out at 0");
        assert_approx(sine_out.apply(1.0), 1.0, 0.001, "sine_out at 1");
        assert!(sine_out.apply(0.5) > 0.5, "sine_out should be above linear at midpoint");

        // SineInOut: (1 - cos(t * PI)) / 2
        let sine_inout = FadeCurve::SineInOut;
        assert_approx(sine_inout.apply(0.0), 0.0, 0.001, "sine_inout at 0");
        assert_approx(sine_inout.apply(0.5), 0.5, 0.001, "sine_inout at 0.5");
        assert_approx(sine_inout.apply(1.0), 1.0, 0.001, "sine_inout at 1");
    }

    #[test]
    fn test_cubic_curves() {
        // CubicIn: t^3
        let cubic_in = FadeCurve::CubicIn;
        assert_approx(cubic_in.apply(0.0), 0.0, 0.001, "cubic_in at 0");
        assert_approx(cubic_in.apply(0.5), 0.125, 0.001, "cubic_in at 0.5 (0.5^3)");
        assert_approx(cubic_in.apply(1.0), 1.0, 0.001, "cubic_in at 1");

        // CubicOut: 1 - (1-t)^3
        let cubic_out = FadeCurve::CubicOut;
        assert_approx(cubic_out.apply(0.0), 0.0, 0.001, "cubic_out at 0");
        assert_approx(cubic_out.apply(0.5), 0.875, 0.001, "cubic_out at 0.5");
        assert_approx(cubic_out.apply(1.0), 1.0, 0.001, "cubic_out at 1");

        // CubicInOut
        let cubic_inout = FadeCurve::CubicInOut;
        assert_approx(cubic_inout.apply(0.0), 0.0, 0.001, "cubic_inout at 0");
        assert_approx(cubic_inout.apply(0.5), 0.5, 0.001, "cubic_inout at 0.5");
        assert_approx(cubic_inout.apply(1.0), 1.0, 0.001, "cubic_inout at 1");
    }

    #[test]
    fn test_exponential_curve() {
        // Default exponent (2.0) should match ease_in
        let exp2 = FadeCurve::Exponential { exponent: 2.0 };
        let ease_in = FadeCurve::EaseIn;
        assert_approx(exp2.apply(0.5), ease_in.apply(0.5), 0.001, "exp(2) should match ease_in");

        // Exponent 1.0 should be linear
        let exp1 = FadeCurve::Exponential { exponent: 1.0 };
        assert_approx(exp1.apply(0.5), 0.5, 0.001, "exp(1) should be linear");

        // Exponent 3.0 should match cubic_in
        let exp3 = FadeCurve::Exponential { exponent: 3.0 };
        let cubic_in = FadeCurve::CubicIn;
        assert_approx(exp3.apply(0.5), cubic_in.apply(0.5), 0.001, "exp(3) should match cubic_in");

        // Higher exponent = slower start
        let exp5 = FadeCurve::Exponential { exponent: 5.0 };
        assert!(exp5.apply(0.5) < exp3.apply(0.5), "higher exponent = slower start");
    }

    #[test]
    fn test_logarithmic_curve() {
        let log = FadeCurve::Logarithmic;
        assert_approx(log.apply(0.0), 0.0, 0.001, "log at 0");
        assert_approx(log.apply(1.0), 1.0, 0.001, "log at 1");
        // Logarithmic should be above linear (fast start)
        assert!(log.apply(0.5) > 0.5, "log should be above linear at midpoint");
    }

    #[test]
    fn test_step_curve() {
        let step = FadeCurve::Step;
        assert_approx(step.apply(0.0), 0.0, 0.001, "step at 0");
        assert_approx(step.apply(0.5), 0.0, 0.001, "step at 0.5");
        assert_approx(step.apply(0.99), 0.0, 0.001, "step at 0.99");
        assert_approx(step.apply(1.0), 1.0, 0.001, "step at 1");
    }

    #[test]
    fn test_cubic_spline_empty() {
        // Empty spline should fall back to linear
        let spline = FadeCurve::CubicSpline { points: vec![] };
        assert_approx(spline.apply(0.0), 0.0, 0.001, "empty spline at 0");
        assert_approx(spline.apply(0.5), 0.5, 0.001, "empty spline at 0.5");
        assert_approx(spline.apply(1.0), 1.0, 0.001, "empty spline at 1");
    }

    #[test]
    fn test_cubic_spline_single_point() {
        // Single point at (0.5, 0.8) should pull curve up at midpoint
        let spline = FadeCurve::CubicSpline {
            points: vec![(0.5, 0.8)],
        };
        assert_approx(spline.apply(0.0), 0.0, 0.001, "spline at 0");
        assert_approx(spline.apply(1.0), 1.0, 0.001, "spline at 1");
        // At midpoint, should be near our control point
        let at_mid = spline.apply(0.5);
        assert!(at_mid > 0.7 && at_mid < 0.9, "spline should pass near control point");
    }

    #[test]
    fn test_cubic_spline_multiple_points() {
        // Multiple points creating a wobble shape
        let spline = FadeCurve::CubicSpline {
            points: vec![(0.25, 0.8), (0.5, 0.2), (0.75, 0.9)],
        };
        assert_approx(spline.apply(0.0), 0.0, 0.001, "spline at 0");
        assert_approx(spline.apply(1.0), 1.0, 0.001, "spline at 1");

        // Values should reflect the control points (approximately)
        let at_25 = spline.apply(0.25);
        let at_50 = spline.apply(0.5);
        let at_75 = spline.apply(0.75);

        // Near control points, values should be close
        assert!(at_25 > 0.5, "spline should be high at 0.25");
        assert!(at_50 < 0.5, "spline should be low at 0.5");
        assert!(at_75 > 0.5, "spline should be high at 0.75");
    }

    #[test]
    fn test_curve_clamping() {
        // All curves should clamp input to 0-1 range
        let curves = vec![
            FadeCurve::Linear,
            FadeCurve::EaseIn,
            FadeCurve::EaseOut,
            FadeCurve::CubicIn,
            FadeCurve::Exponential { exponent: 2.0 },
        ];

        for curve in curves {
            // Values below 0 should clamp to 0
            assert_approx(curve.apply(-0.5), curve.apply(0.0), 0.001, "should clamp below 0");
            // Values above 1 should clamp to 1
            assert_approx(curve.apply(1.5), curve.apply(1.0), 0.001, "should clamp above 1");
        }
    }

    #[test]
    fn test_all_curves_start_at_zero_end_at_one() {
        let curves: Vec<FadeCurve> = vec![
            FadeCurve::Linear,
            FadeCurve::EaseIn,
            FadeCurve::EaseOut,
            FadeCurve::EaseInOut,
            FadeCurve::SineIn,
            FadeCurve::SineOut,
            FadeCurve::SineInOut,
            FadeCurve::CubicIn,
            FadeCurve::CubicOut,
            FadeCurve::CubicInOut,
            FadeCurve::Exponential { exponent: 2.0 },
            FadeCurve::Logarithmic,
            FadeCurve::Step,
            FadeCurve::CubicSpline { points: vec![(0.5, 0.5)] },
        ];

        for curve in curves {
            assert_approx(curve.apply(0.0), 0.0, 0.001, &format!("{:?} should start at 0", curve));
            assert_approx(curve.apply(1.0), 1.0, 0.001, &format!("{:?} should end at 1", curve));
        }
    }

    #[test]
    fn test_curve_monotonicity() {
        // Most curves should be monotonically increasing
        let curves = vec![
            FadeCurve::Linear,
            FadeCurve::EaseIn,
            FadeCurve::EaseOut,
            FadeCurve::EaseInOut,
            FadeCurve::CubicIn,
            FadeCurve::CubicOut,
            FadeCurve::CubicInOut,
            FadeCurve::Logarithmic,
        ];

        for curve in curves {
            let mut prev = 0.0;
            for i in 0..=100 {
                let t = i as f32 / 100.0;
                let val = curve.apply(t);
                assert!(
                    val >= prev - 0.001,
                    "{:?} should be monotonic at t={}: {} >= {}",
                    curve,
                    t,
                    val,
                    prev
                );
                prev = val;
            }
        }
    }
}
