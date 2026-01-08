//! Fades trait for parameter automation.
//!
//! Fades smoothly transition parameters over time.

use crate::types::{Duration, EffectId, GroupId, MelodyId, PatternId, VoiceId};
use crate::Result;
use async_trait::async_trait;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FadeCurve {
    /// Linear interpolation.
    #[default]
    Linear,

    /// Exponential curve (faster at start).
    Exponential,

    /// Sine curve (smooth start and end).
    Sine,
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
#[async_trait]
pub trait Fades: Send + Sync {
    /// Start a parameter fade.
    async fn fade(&self, config: FadeConfig) -> Result<()>;

    /// Cancel an active fade.
    async fn cancel(&self, target: &FadeTarget, param: &str) -> Result<()>;
}
