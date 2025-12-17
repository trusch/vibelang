//! Sequence scheduling types for declarative clip arrangement.
//!
//! Sequences allow arranging patterns, melodies, fades, and nested
//! sequences on a looping timeline. This provides a high-level way
//! to compose musical sections.
//!
//! # Example
//!
//! ```ignore
//! let seq = SequenceDefinition::new("intro")
//!     .with_loop_beats(16.0)
//!     .with_clip(SequenceClip::new(
//!         0.0, 16.0,
//!         ClipSource::Pattern("kick".to_string()),
//!         ClipMode::Loop,
//!     ));
//! ```

use crate::api::context::SourceLocation;
use crate::events::FadeTargetType;

/// Source that can be placed into a [`SequenceClip`].
#[derive(Clone, Debug, PartialEq)]
pub enum ClipSource {
    /// Reference a pattern by name.
    Pattern(String),
    /// Reference a melody by name.
    Melody(String),
    /// Reference a fade automation by name.
    Fade(String),
    /// Reference another sequence by name (for nesting).
    Sequence(String),
}

impl ClipSource {
    /// Get the name of the source.
    pub fn name(&self) -> &str {
        match self {
            ClipSource::Pattern(name) => name,
            ClipSource::Melody(name) => name,
            ClipSource::Fade(name) => name,
            ClipSource::Sequence(name) => name,
        }
    }

    /// Get a type identifier for the source.
    pub fn type_name(&self) -> &'static str {
        match self {
            ClipSource::Pattern(_) => "pattern",
            ClipSource::Melody(_) => "melody",
            ClipSource::Fade(_) => "fade",
            ClipSource::Sequence(_) => "sequence",
        }
    }
}

/// Playback mode for a clip.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipMode {
    /// Loop the source for as long as the parent sequence is playing.
    Loop,
    /// Play the source once starting at the clip start, then fall silent.
    Once,
    /// Loop the source a fixed number of times, then fall silent.
    LoopCount(i64),
}

impl ClipMode {
    /// Check if this mode allows looping.
    pub fn loops(&self) -> bool {
        matches!(self, ClipMode::Loop | ClipMode::LoopCount(_))
    }
}

/// A clip on the sequence timeline.
///
/// Clips define when a source (pattern, melody, fade, or sequence)
/// plays within the parent sequence's timeline.
#[derive(Clone, Debug)]
pub struct SequenceClip {
    /// Start beat within the sequence.
    pub start: f64,
    /// End beat within the sequence.
    pub end: f64,
    /// The source to play.
    pub source: ClipSource,
    /// Playback mode (loop, once, or loop count).
    pub mode: ClipMode,
}

impl SequenceClip {
    /// Create a new sequence clip.
    pub fn new(start: f64, end: f64, source: ClipSource, mode: ClipMode) -> Self {
        Self {
            start,
            end,
            source,
            mode,
        }
    }

    /// Get the duration of the clip in beats.
    pub fn duration(&self) -> f64 {
        self.end - self.start
    }

    /// Check if a beat position falls within this clip.
    pub fn contains_beat(&self, beat: f64) -> bool {
        beat >= self.start && beat < self.end
    }

    /// Generate a unique identifier for this clip.
    pub fn clip_id(&self) -> String {
        format!("{}:{}", self.source.type_name(), self.source.name())
    }
}

/// Definition of a sequence that can be started and looped.
///
/// Sequences are the primary way to arrange musical material
/// in VibeLang. They contain clips that reference patterns,
/// melodies, fades, or other sequences.
#[derive(Clone, Debug)]
pub struct SequenceDefinition {
    /// Unique name identifying this sequence.
    pub name: String,
    /// Loop length in beats.
    pub loop_beats: f64,
    /// Clips arranged on the timeline.
    pub clips: Vec<SequenceClip>,
    /// Generation counter for tracking stale sequences across reloads.
    pub generation: u64,
    /// If true, sequence stops after one iteration instead of looping.
    pub play_once: bool,
    /// Source location where this sequence was defined.
    pub source_location: SourceLocation,
}

impl SequenceDefinition {
    /// Create a new empty sequence with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            loop_beats: 16.0,
            clips: Vec::new(),
            generation: 0,
            play_once: false,
            source_location: SourceLocation::default(),
        }
    }

    /// Set source location for this sequence.
    pub fn with_source_location(mut self, source_location: SourceLocation) -> Self {
        self.source_location = source_location;
        self
    }

    /// Set the loop length in beats.
    pub fn with_loop_beats(mut self, beats: f64) -> Self {
        self.loop_beats = beats;
        self
    }

    /// Add a clip to the sequence.
    pub fn with_clip(mut self, clip: SequenceClip) -> Self {
        self.clips.push(clip);
        self
    }

    /// Add a clip builder-style (for chaining).
    pub fn add_clip(&mut self, clip: SequenceClip) -> &mut Self {
        self.clips.push(clip);
        self
    }

    /// Get clips that are active at the given beat position.
    pub fn clips_at_beat(&self, beat: f64) -> impl Iterator<Item = &SequenceClip> {
        self.clips.iter().filter(move |c| c.contains_beat(beat))
    }
}

/// Definition of a fade automation that can be scheduled from sequences.
///
/// FadeDefinitions are created in scripts and can be placed as clips
/// in sequences to trigger parameter automation at specific times.
#[derive(Clone, Debug)]
pub struct FadeDefinition {
    /// Unique name identifying this fade.
    pub name: String,
    /// Type of entity being faded.
    pub target_type: FadeTargetType,
    /// Name of the target entity.
    pub target_name: String,
    /// Name of the parameter being faded.
    pub param_name: String,
    /// Starting value.
    pub from: f32,
    /// Ending value.
    pub to: f32,
    /// Duration in beats.
    pub duration_beats: f64,
}

impl FadeDefinition {
    /// Create a new fade definition.
    pub fn new(
        name: impl Into<String>,
        target_type: FadeTargetType,
        target_name: impl Into<String>,
        param_name: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            target_type,
            target_name: target_name.into(),
            param_name: param_name.into(),
            from: 0.0,
            to: 1.0,
            duration_beats: 4.0,
        }
    }

    /// Set the from/to values.
    pub fn with_range(mut self, from: f32, to: f32) -> Self {
        self.from = from;
        self.to = to;
        self
    }

    /// Set the duration in beats.
    pub fn with_duration(mut self, beats: f64) -> Self {
        self.duration_beats = beats;
        self
    }
}

// ============================================================================
// Content Hashing for Reload Diffing
// ============================================================================

use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

impl SequenceDefinition {
    /// Compute a content hash of this sequence's configuration.
    /// Excludes ephemeral state like generation.
    pub fn content_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.name.hash(&mut hasher);
        self.loop_beats.to_bits().hash(&mut hasher);
        self.play_once.hash(&mut hasher);
        // Hash clips in order
        for clip in &self.clips {
            clip.start.to_bits().hash(&mut hasher);
            clip.end.to_bits().hash(&mut hasher);
            // Hash clip source
            match &clip.source {
                ClipSource::Pattern(name) => {
                    "pattern".hash(&mut hasher);
                    name.hash(&mut hasher);
                }
                ClipSource::Melody(name) => {
                    "melody".hash(&mut hasher);
                    name.hash(&mut hasher);
                }
                ClipSource::Fade(name) => {
                    "fade".hash(&mut hasher);
                    name.hash(&mut hasher);
                }
                ClipSource::Sequence(name) => {
                    "sequence".hash(&mut hasher);
                    name.hash(&mut hasher);
                }
            }
            // Hash clip mode
            match &clip.mode {
                ClipMode::Loop => "loop".hash(&mut hasher),
                ClipMode::Once => "once".hash(&mut hasher),
                ClipMode::LoopCount(n) => {
                    "loop_count".hash(&mut hasher);
                    n.hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clip_source_name() {
        let pat = ClipSource::Pattern("kick".to_string());
        assert_eq!(pat.name(), "kick");
        assert_eq!(pat.type_name(), "pattern");
    }

    #[test]
    fn test_sequence_clip_contains() {
        let clip = SequenceClip::new(
            4.0,
            8.0,
            ClipSource::Pattern("test".to_string()),
            ClipMode::Loop,
        );
        assert!(!clip.contains_beat(3.0));
        assert!(clip.contains_beat(4.0));
        assert!(clip.contains_beat(6.0));
        assert!(!clip.contains_beat(8.0)); // end is exclusive
    }

    #[test]
    fn test_sequence_definition_builder() {
        let seq = SequenceDefinition::new("my_seq")
            .with_loop_beats(32.0)
            .with_clip(SequenceClip::new(
                0.0,
                16.0,
                ClipSource::Pattern("kick".to_string()),
                ClipMode::Loop,
            ))
            .with_clip(SequenceClip::new(
                0.0,
                32.0,
                ClipSource::Melody("bass".to_string()),
                ClipMode::Once,
            ));

        assert_eq!(seq.name, "my_seq");
        assert_eq!(seq.loop_beats, 32.0);
        assert_eq!(seq.clips.len(), 2);
    }

    #[test]
    fn test_clips_at_beat() {
        let seq = SequenceDefinition::new("test")
            .with_clip(SequenceClip::new(
                0.0,
                8.0,
                ClipSource::Pattern("a".to_string()),
                ClipMode::Loop,
            ))
            .with_clip(SequenceClip::new(
                4.0,
                12.0,
                ClipSource::Pattern("b".to_string()),
                ClipMode::Loop,
            ));

        let at_2: Vec<_> = seq.clips_at_beat(2.0).collect();
        assert_eq!(at_2.len(), 1);

        let at_6: Vec<_> = seq.clips_at_beat(6.0).collect();
        assert_eq!(at_6.len(), 2); // Both clips active
    }
}
