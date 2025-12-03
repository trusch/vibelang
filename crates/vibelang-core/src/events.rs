//! Beat-based musical events and patterns.
//!
//! This module defines the core event types used for scheduling:
//!
//! - [`BeatEvent`] - A single scheduled event with controls
//! - [`Pattern`] - A collection of events with loop length
//! - [`FadeClip`] - Parameter automation trigger
//! - [`ActiveFade`] - Runtime state for an active fade

use std::time::Instant;

/// An event to be scheduled at a specific beat position.
///
/// Events carry all the information needed to trigger a synth,
/// including the synth definition name, control values, and
/// metadata about which pattern/melody/voice created them.
#[derive(Clone, Debug)]
pub struct BeatEvent {
    /// Beat position relative to pattern start (0.0 to loop_length).
    pub beat: f64,
    /// Name of the SynthDef to instantiate.
    pub synth_def: String,
    /// Control name/value pairs to set on the synth.
    pub controls: Vec<(String, f32)>,
    /// Path of the group this event belongs to.
    pub group_path: Option<String>,
    /// Name of the pattern that created this event.
    pub pattern_name: Option<String>,
    /// Name of the melody that created this event.
    pub melody_name: Option<String>,
    /// Name of the voice that created this event.
    pub voice_name: Option<String>,
    /// Optional automation trigger attached to this event.
    pub fade: Option<FadeClip>,
}

impl BeatEvent {
    /// Create a new beat event with minimal required fields.
    pub fn new(beat: f64, synth_def: impl Into<String>) -> Self {
        Self {
            beat,
            synth_def: synth_def.into(),
            controls: Vec::new(),
            group_path: None,
            pattern_name: None,
            melody_name: None,
            voice_name: None,
            fade: None,
        }
    }

    /// Add a control value to the event.
    pub fn with_control(mut self, name: impl Into<String>, value: f32) -> Self {
        self.controls.push((name.into(), value));
        self
    }

    /// Set the group path for this event.
    pub fn with_group_path(mut self, path: impl Into<String>) -> Self {
        self.group_path = Some(path.into());
        self
    }

    /// Set the voice name for this event.
    pub fn with_voice_name(mut self, name: impl Into<String>) -> Self {
        self.voice_name = Some(name.into());
        self
    }
}

/// A pattern containing multiple events scheduled at specific beats.
///
/// Events have relative beat positions (0.0 to loop_length_beats).
/// The pattern loops continuously when played.
#[derive(Clone, Debug)]
pub struct Pattern {
    /// Unique name identifying this pattern.
    pub name: String,
    /// Events within the pattern (positions relative to pattern start).
    pub events: Vec<BeatEvent>,
    /// Length of the pattern in beats (loop point).
    pub loop_length_beats: f64,
    /// Phase offset from pattern start for quantization alignment.
    pub phase_offset: f64,
}

impl Pattern {
    /// Create a new empty pattern with the given name and loop length.
    pub fn new(name: impl Into<String>, loop_length_beats: f64) -> Self {
        Self {
            name: name.into(),
            events: Vec::new(),
            loop_length_beats,
            phase_offset: 0.0,
        }
    }

    /// Add an event to the pattern.
    pub fn with_event(mut self, event: BeatEvent) -> Self {
        self.events.push(event);
        self
    }

    /// Set the phase offset for quantization.
    pub fn with_phase_offset(mut self, offset: f64) -> Self {
        self.phase_offset = offset;
        self
    }
}

/// Target type for parameter fades.
///
/// Fades can target different entity types in the audio graph.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FadeTargetType {
    /// Fade a group parameter.
    Group,
    /// Fade a voice parameter.
    Voice,
    /// Fade a pattern parameter.
    Pattern,
    /// Fade a melody parameter.
    Melody,
    /// Fade an effect parameter.
    Effect,
}

/// A fade automation trigger scheduled at a specific beat.
///
/// FadeClips are created from FadeDefinitions and attached to BeatEvents
/// to schedule parameter automation.
#[derive(Clone, Debug)]
pub struct FadeClip {
    /// Name of the fade definition (for tracking clip_once mode).
    pub name: String,
    /// Sequence that triggered this fade (for clip_once tracking).
    pub sequence_name: Option<String>,
    /// Type of entity being faded.
    pub target_type: FadeTargetType,
    /// Name of the target entity.
    pub target_name: String,
    /// Name of the parameter being faded.
    pub param_name: String,
    /// Starting value of the fade.
    pub start_value: f32,
    /// Ending value of the fade.
    pub target_value: f32,
    /// Duration of the fade in beats.
    pub duration_beats: f64,
}

/// Runtime state for an active parameter fade operation.
///
/// This tracks the progress of a fade that is currently executing.
#[derive(Clone, Debug)]
pub struct ActiveFade {
    /// Synth node IDs to fade (empty if targeting pattern/melody).
    pub node_ids: Vec<i32>,
    /// Name of the parameter being faded.
    pub param_name: String,
    /// Starting value.
    pub start_value: f32,
    /// Target value.
    pub target_value: f32,
    /// When the fade started.
    pub start_time: Instant,
    /// Duration in seconds.
    pub duration_seconds: f64,
    /// Type of entity being faded.
    pub fade_target_type: FadeTargetType,
    /// Name of the target entity.
    pub fade_target_name: String,
    /// Delay before fade starts (for syncing with unmute, etc.).
    pub delay_seconds: f64,
    /// Last time we sent an update (for throttling).
    pub last_update_time: Option<Instant>,
    /// Last value sent (for deduplication).
    pub last_sent_value: Option<f32>,
}

impl ActiveFade {
    /// Calculate the current interpolated value based on elapsed time.
    pub fn current_value(&self) -> f32 {
        let elapsed = self.start_time.elapsed().as_secs_f64() - self.delay_seconds;
        if elapsed < 0.0 {
            return self.start_value;
        }
        if elapsed >= self.duration_seconds {
            return self.target_value;
        }
        let t = elapsed / self.duration_seconds;
        self.start_value + (self.target_value - self.start_value) * t as f32
    }

    /// Check if the fade has completed.
    pub fn is_complete(&self) -> bool {
        let elapsed = self.start_time.elapsed().as_secs_f64() - self.delay_seconds;
        elapsed >= self.duration_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_event_builder() {
        let event = BeatEvent::new(1.5, "kick")
            .with_control("amp", 0.8)
            .with_control("freq", 60.0)
            .with_group_path("main.drums");

        assert!((event.beat - 1.5).abs() < 0.001);
        assert_eq!(event.synth_def, "kick");
        assert_eq!(event.controls.len(), 2);
        assert_eq!(event.group_path, Some("main.drums".to_string()));
    }

    #[test]
    fn test_pattern_builder() {
        let pattern = Pattern::new("my_pattern", 4.0)
            .with_event(BeatEvent::new(0.0, "kick"))
            .with_event(BeatEvent::new(2.0, "kick"))
            .with_phase_offset(0.5);

        assert_eq!(pattern.name, "my_pattern");
        assert_eq!(pattern.events.len(), 2);
        assert!((pattern.loop_length_beats - 4.0).abs() < 0.001);
        assert!((pattern.phase_offset - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_active_fade_interpolation() {
        let fade = ActiveFade {
            node_ids: vec![],
            param_name: "amp".to_string(),
            start_value: 0.0,
            target_value: 1.0,
            start_time: Instant::now(),
            duration_seconds: 1.0,
            fade_target_type: FadeTargetType::Voice,
            fade_target_name: "test".to_string(),
            delay_seconds: 0.0,
            last_update_time: None,
            last_sent_value: None,
        };

        // At the start, value should be close to start_value
        let val = fade.current_value();
        assert!(val >= 0.0 && val <= 1.0);
    }
}
