//! Patterns trait for rhythmic sequences.
//!
//! Patterns trigger a voice at specific beat positions, creating rhythms.

use crate::types::{Beat, ParamMap, PatternId, VoiceId};
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Tolerance for floating-point comparisons in pattern data.
/// This prevents false "updates" during reload due to float precision issues.
const FLOAT_TOLERANCE: f32 = 1e-6;

/// A single step in a pattern.
#[derive(Clone, Debug)]
pub struct Step {
    /// Beat position within the pattern (0 to length).
    pub beat: Beat,

    /// Parameters for this step.
    pub params: ParamMap,
}

impl PartialEq for Step {
    fn eq(&self, other: &Self) -> bool {
        if self.beat != other.beat {
            return false;
        }
        if self.params.len() != other.params.len() {
            return false;
        }
        // Compare params with tolerance for f32 values
        for (key, &value) in &self.params {
            match other.params.get(key) {
                Some(&other_value) if (value - other_value).abs() < FLOAT_TOLERANCE => {}
                _ => return false,
            }
        }
        true
    }
}

impl Eq for Step {}

impl Step {
    /// Create a new step at the given beat.
    pub fn new(beat: Beat) -> Self {
        Self {
            beat,
            params: ParamMap::new(),
        }
    }

    /// Create a step at a beat position from f64.
    pub fn at(beat: f64) -> Self {
        Self::new(Beat::from_f64(beat))
    }

    /// Add a parameter to this step.
    pub fn with_param(mut self, name: impl Into<String>, value: f32) -> Self {
        self.params.insert(name.into(), value);
        self
    }
}

/// Configuration for creating a pattern.
#[derive(Clone, Debug)]
pub struct PatternConfig {
    /// Pattern name (for display in TUI/API).
    pub name: String,

    /// Voice to trigger (None if pattern is just for timing).
    pub voice: Option<VoiceId>,

    /// Steps in the pattern.
    pub steps: Vec<Step>,

    /// Length of the pattern in beats.
    pub length: Beat,

    /// Swing amount (0.0 = none, 1.0 = full).
    pub swing: f32,
}

impl PartialEq for PatternConfig {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.voice == other.voice
            && self.steps == other.steps
            && self.length == other.length
            && (self.swing - other.swing).abs() < FLOAT_TOLERANCE
    }
}

impl Eq for PatternConfig {}

impl PatternConfig {
    /// Create a new pattern configuration.
    pub fn new(name: impl Into<String>, voice: VoiceId, length: Beat) -> Self {
        Self {
            name: name.into(),
            voice: Some(voice),
            steps: Vec::new(),
            length,
            swing: 0.0,
        }
    }

    /// Create a pattern configuration without a voice.
    pub fn without_voice(name: impl Into<String>, length: Beat) -> Self {
        Self {
            name: name.into(),
            voice: None,
            steps: Vec::new(),
            length,
            swing: 0.0,
        }
    }

    /// Create a pattern with length in beats as f64.
    pub fn with_length(name: impl Into<String>, voice: VoiceId, length: f64) -> Self {
        Self::new(name, voice, Beat::from_f64(length))
    }

    /// Add a step to the pattern.
    pub fn with_step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    /// Set the swing amount.
    pub fn with_swing(mut self, swing: f32) -> Self {
        self.swing = swing;
        self
    }

    /// Borrowing equality against live [`PatternContent`] — used by the reload
    /// diff to compare a running pattern's `Arc<PatternContent>` against a
    /// proposed config without materializing a `PatternConfig` clone (which
    /// would deep-copy every `Step`). Mirrors [`PatternConfig`]'s `PartialEq`
    /// exactly (name/voice/steps/length, swing within `FLOAT_TOLERANCE`).
    pub fn matches_content(&self, content: &PatternContent) -> bool {
        self.name == content.name
            && self.voice == content.voice
            && self.steps == content.steps
            && self.length == content.length
            && (self.swing - content.swing).abs() < FLOAT_TOLERANCE
    }
}

/// The content of a pattern that can be swapped during hot reload.
///
/// This struct is separate from playback state (playing, loop_position) to enable
/// seamless hot reload. The content can be atomically swapped while the pattern
/// continues playing, allowing changes to take effect at musical boundaries
/// without any audio disruption.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternContent {
    /// Pattern name (for display in TUI/API).
    pub name: String,

    /// Voice to trigger (None if pattern is just for timing).
    pub voice: Option<VoiceId>,

    /// Steps in the pattern.
    pub steps: Vec<Step>,

    /// Length of the pattern in beats.
    pub length: Beat,

    /// Swing amount (0.0 = none, 1.0 = full).
    pub swing: f32,
}

impl PatternContent {
    /// Create new pattern content from a config.
    pub fn from_config(config: &PatternConfig) -> Self {
        Self {
            name: config.name.clone(),
            voice: config.voice,
            steps: config.steps.clone(),
            length: config.length,
            swing: config.swing,
        }
    }

    /// Create an Arc-wrapped content from config.
    pub fn arc_from_config(config: &PatternConfig) -> Arc<Self> {
        Arc::new(Self::from_config(config))
    }
}

impl From<&PatternConfig> for PatternContent {
    fn from(config: &PatternConfig) -> Self {
        Self::from_config(config)
    }
}

impl From<PatternConfig> for PatternContent {
    fn from(config: PatternConfig) -> Self {
        Self {
            name: config.name,
            voice: config.voice,
            steps: config.steps,
            length: config.length,
            swing: config.swing,
        }
    }
}

/// Pattern management for rhythmic sequences.
///
/// Patterns loop continuously, triggering their voice at each step.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait Patterns: Send + Sync {
    /// Create a new pattern.
    async fn create(&self, id: PatternId, config: PatternConfig) -> Result<()>;

    /// Delete a pattern.
    async fn delete(&self, id: PatternId) -> Result<()>;

    /// Start playing a pattern.
    async fn start(&self, id: PatternId) -> Result<()>;

    /// Stop playing a pattern.
    async fn stop(&self, id: PatternId) -> Result<()>;

    /// Set a pattern parameter.
    ///
    /// This affects all future triggers from this pattern.
    async fn set_param(&self, id: PatternId, param: &str, value: f32) -> Result<()>;
}

/// Pattern management for rhythmic sequences (WASM version).
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait Patterns {
    /// Create a new pattern.
    async fn create(&self, id: PatternId, config: PatternConfig) -> Result<()>;

    /// Delete a pattern.
    async fn delete(&self, id: PatternId) -> Result<()>;

    /// Start playing a pattern.
    async fn start(&self, id: PatternId) -> Result<()>;

    /// Stop playing a pattern.
    async fn stop(&self, id: PatternId) -> Result<()>;

    /// Set a pattern parameter.
    async fn set_param(&self, id: PatternId, param: &str, value: f32) -> Result<()>;
}
