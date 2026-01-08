//! Sequences trait for arrangement of clips.
//!
//! Sequences arrange patterns, melodies, fades, and other sequences
//! on a timeline.

use crate::traits::FadeConfig;
use crate::types::{Beat, MelodyId, PatternId, SequenceId};
use crate::Result;
use async_trait::async_trait;

/// A clip in a sequence.
#[derive(Clone, Debug, PartialEq)]
pub enum Clip {
    /// A pattern clip.
    Pattern {
        id: PatternId,
        start: Beat,
        end: Beat,
    },

    /// A melody clip.
    Melody {
        id: MelodyId,
        start: Beat,
        end: Beat,
    },

    /// A fade automation clip.
    Fade {
        config: FadeConfig,
        start: Beat,
    },

    /// A nested sequence.
    Sequence {
        id: SequenceId,
        start: Beat,
    },
}

impl Clip {
    /// Create a pattern clip.
    pub fn pattern(id: PatternId, start: f64, end: f64) -> Self {
        Clip::Pattern {
            id,
            start: Beat::from_f64(start),
            end: Beat::from_f64(end),
        }
    }

    /// Create a melody clip.
    pub fn melody(id: MelodyId, start: f64, end: f64) -> Self {
        Clip::Melody {
            id,
            start: Beat::from_f64(start),
            end: Beat::from_f64(end),
        }
    }

    /// Create a fade clip.
    pub fn fade(config: FadeConfig, start: f64) -> Self {
        Clip::Fade {
            config,
            start: Beat::from_f64(start),
        }
    }

    /// Create a nested sequence clip.
    pub fn sequence(id: SequenceId, start: f64) -> Self {
        Clip::Sequence {
            id,
            start: Beat::from_f64(start),
        }
    }
}

/// Configuration for creating a sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceConfig {
    /// Length of the sequence in beats.
    pub length: Beat,

    /// Clips in the sequence.
    pub clips: Vec<Clip>,
}

impl SequenceConfig {
    /// Create a new sequence configuration.
    pub fn new(length: Beat) -> Self {
        Self {
            length,
            clips: Vec::new(),
        }
    }

    /// Create a sequence with length in beats as f64.
    pub fn with_length(length: f64) -> Self {
        Self::new(Beat::from_f64(length))
    }

    /// Add a clip to the sequence.
    pub fn with_clip(mut self, clip: Clip) -> Self {
        self.clips.push(clip);
        self
    }
}

/// Sequence management for arrangements.
///
/// Sequences can play once or loop, and can contain nested sequences
/// for complex arrangements.
#[async_trait]
pub trait Sequences: Send + Sync {
    /// Create a new sequence.
    async fn create(&self, id: SequenceId, config: SequenceConfig) -> Result<()>;

    /// Delete a sequence.
    async fn delete(&self, id: SequenceId) -> Result<()>;

    /// Start playing a sequence.
    ///
    /// # Arguments
    ///
    /// * `id` - Sequence ID
    /// * `looping` - Whether to loop or play once
    async fn start(&self, id: SequenceId, looping: bool) -> Result<()>;

    /// Stop playing a sequence.
    async fn stop(&self, id: SequenceId) -> Result<()>;

    /// Pause a sequence (retains position).
    async fn pause(&self, id: SequenceId) -> Result<()>;

    /// Resume a paused sequence.
    async fn resume(&self, id: SequenceId) -> Result<()>;
}
