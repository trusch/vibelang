//! Pattern API for Rhai scripts.
//!
//! Patterns are rhythmic sequences that trigger voices.

use crate::events::{BeatEvent, Pattern as PatternData};
use crate::sequences::{ClipMode, ClipSource, SequenceClip, SequenceDefinition};
use crate::state::{LoopStatus, StateMessage};
use rhai::{CustomType, Dynamic, Engine, EvalAltResult, Position, TypeBuilder};
use std::collections::HashMap;

use super::{context, require_handle};

/// A Pattern builder for creating rhythmic patterns.
#[derive(Debug, Clone, CustomType)]
pub struct Pattern {
    /// Pattern name.
    pub name: String,
    /// Voice name to trigger.
    voice_name: Option<String>,
    /// Step pattern string (e.g., "x..x..x.").
    steps: Option<String>,
    /// Loop length in beats.
    length: f64,
    /// Swing amount (0.0 to 1.0).
    swing: f64,
    /// Quantization in beats.
    quantize: f64,
    /// Group path.
    group_path: String,
    /// Parameters to pass to voice.
    params: HashMap<String, f64>,
}

impl Pattern {
    /// Create a new pattern with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            quantize: 0.0,
            group_path: context::current_group_path(),
            params: HashMap::new(),
        }
    }

    // === Builder methods ===

    /// Set the voice to trigger (by name).
    pub fn on(mut self, voice_name: String) -> Self {
        self.voice_name = Some(voice_name);
        self
    }

    /// Set the voice to trigger (by Voice object).
    pub fn on_voice(mut self, voice: super::voice::Voice) -> Self {
        self.voice_name = Some(voice.name.clone());
        self
    }

    /// Set the step pattern.
    ///
    /// # Example
    /// ```rhai
    /// pattern("kick").on("kick_voice").step("x...x...x...x...")
    /// ```
    pub fn step(mut self, steps: String) -> Self {
        self.steps = Some(steps);
        self
    }

    /// Generate a Euclidean rhythm.
    ///
    /// # Arguments
    /// * `hits` - Number of hits
    /// * `steps` - Total number of steps
    pub fn euclid(mut self, hits: i64, total_steps: i64) -> Self {
        let pattern = generate_euclidean(hits as usize, total_steps as usize);
        self.steps = Some(pattern);
        self
    }

    /// Set the loop length in beats.
    pub fn len(mut self, beats: f64) -> Self {
        self.length = beats;
        self
    }

    /// Set the swing amount.
    pub fn swing(mut self, amount: f64) -> Self {
        self.swing = amount.clamp(0.0, 1.0);
        self
    }

    /// Set the quantization.
    pub fn quantize(mut self, beats: f64) -> Self {
        self.quantize = beats;
        self
    }

    /// Set a parameter.
    pub fn set_param(mut self, param: String, value: f64) -> Self {
        self.params.insert(param, value);
        self
    }

    /// Create a lane for multi-parameter patterns.
    pub fn lane(self, _param: String) -> PatternLaneBuilder {
        PatternLaneBuilder {
            pattern: self,
            param: _param,
        }
    }

    // === Actions ===

    /// Register and apply the pattern.
    pub fn apply(&mut self) {
        let handle = require_handle();

        // Calculate loop length from pattern if available, otherwise use explicit length
        let loop_length = if let Some(ref steps) = self.steps {
            calculate_loop_length_from_pattern(steps)
        } else {
            self.length
        };

        // Parse steps into events
        let events = if let Some(ref steps) = self.steps {
            parse_pattern_steps(steps, loop_length, self.swing)
        } else {
            Vec::new()
        };

        let loop_pattern = PatternData {
            name: self.name.clone(),
            events,
            loop_length_beats: loop_length,
            phase_offset: 0.0,
        };

        let _ = handle.send(StateMessage::CreatePattern {
            name: self.name.clone(),
            group_path: self.group_path.clone(),
            voice_name: self.voice_name.clone(),
            pattern: loop_pattern,
        });
    }

    /// Start the pattern playing (chainable).
    ///
    /// This creates an implicit sequence containing the pattern as a looping clip,
    /// then starts that sequence.
    pub fn start(mut self) -> Self {
        self.apply();
        let handle = require_handle();

        // Calculate loop length from pattern if available, otherwise use explicit length
        let loop_length = if let Some(ref steps) = self.steps {
            calculate_loop_length_from_pattern(steps)
        } else {
            self.length
        };

        // Create an implicit sequence for this pattern
        let seq_name = format!("_seq_{}", self.name);
        let seq_def = SequenceDefinition::new(seq_name.clone())
            .with_loop_beats(loop_length)
            .with_clip(SequenceClip::new(
                0.0,
                loop_length,
                ClipSource::Pattern(self.name.clone()),
                ClipMode::Loop,
            ));

        // Register and start the sequence
        let _ = handle.send(StateMessage::CreateSequence { sequence: seq_def });
        let _ = handle.send(StateMessage::StartSequence { name: seq_name });

        self
    }

    /// Stop the pattern.
    ///
    /// This stops the implicit sequence that was created by `start()`.
    pub fn stop(&mut self) {
        let handle = require_handle();
        // Stop the implicit sequence
        let seq_name = format!("_seq_{}", self.name);
        let _ = handle.send(StateMessage::StopSequence { name: seq_name });
    }

    /// Launch the pattern (start if not playing, chainable).
    pub fn launch(self) -> Self {
        self.start()
    }

    /// Check if the pattern is playing.
    pub fn is_playing(&mut self) -> bool {
        let handle = require_handle();
        handle.with_state(|state| {
            state
                .patterns
                .get(&self.name)
                .map(|p| matches!(p.status, LoopStatus::Playing { .. }))
                .unwrap_or(false)
        })
    }

    /// Create a fade builder for a parameter.
    pub fn fade_param(&mut self, _param: String) {
        // TODO: Implement fade builder
        log::warn!("Pattern fade not yet implemented");
    }
}

/// Lane builder for multi-parameter patterns.
#[derive(Debug, Clone, CustomType)]
pub struct PatternLaneBuilder {
    pattern: Pattern,
    param: String,
}

impl PatternLaneBuilder {
    /// Set values for this lane.
    pub fn values(self, _values: rhai::Array) -> Pattern {
        // TODO: Implement lane values
        self.pattern
    }
}

/// Create a new pattern builder.
pub fn pattern(name: String) -> Pattern {
    Pattern::new(name)
}

/// Parse a step pattern string into beat events.
/// Uses bar-aware parsing: each bar separated by `|` is 4 beats.
fn parse_pattern_steps(steps: &str, _length: f64, swing: f64) -> Vec<BeatEvent> {
    let mut events = Vec::new();

    // Split by bar separator
    let bars: Vec<&str> = steps.split('|').collect();
    let beats_per_bar = 4.0; // Standard 4/4 time

    let mut current_beat = 0.0;
    let mut step_index = 0;

    for bar in bars {
        // Tokenize the bar - always split each character since compact notation is used
        let bar_tokens: Vec<char> = tokenize_bar_chars(bar);

        if bar_tokens.is_empty() {
            current_beat += beats_per_bar;
            continue;
        }

        let beat_per_token = beats_per_bar / bar_tokens.len() as f64;

        for (i, ch) in bar_tokens.iter().enumerate() {
            let beat = current_beat + i as f64 * beat_per_token;

            // Apply swing to off-beats
            let swung_beat = if step_index % 2 == 1 {
                beat + swing * beat_per_token * 0.5
            } else {
                beat
            };

            // Parse velocity from token character
            let velocity = match ch {
                'x' => Some(1.0),
                'X' | 'o' | 'O' => Some(1.2),
                '1'..='9' => {
                    let digit = (*ch as u8 - b'0') as f64;
                    Some(0.1 + (digit / 9.0) * 0.9)
                }
                '.' | '_' | '0' | '-' => None, // Rest or hold
                _ => None,
            };

            if let Some(vel) = velocity {
                let mut event = BeatEvent::new(swung_beat, "trigger");
                event.controls.push(("amp".to_string(), vel as f32));
                events.push(event);
            }

            step_index += 1;
        }

        current_beat += beats_per_bar;
    }

    events
}

/// Tokenize a bar into individual characters, filtering whitespace.
/// This handles both space-separated and compact notation (e.g., "x.x." or "x . x .")
fn tokenize_bar_chars(bar: &str) -> Vec<char> {
    bar.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Calculate loop length from pattern: number of bars × 4 beats
fn calculate_loop_length_from_pattern(pattern: &str) -> f64 {
    let num_bars = pattern.split('|').count();
    let beats_per_bar = 4.0;
    num_bars as f64 * beats_per_bar
}

/// Generate a Euclidean rhythm pattern.
fn generate_euclidean(hits: usize, steps: usize) -> String {
    if steps == 0 {
        return String::new();
    }
    if hits >= steps {
        return "x".repeat(steps);
    }
    if hits == 0 {
        return ".".repeat(steps);
    }

    // Bresenham-style Euclidean algorithm
    let mut pattern = vec![false; steps];
    let mut bucket = 0;

    for slot in pattern.iter_mut() {
        bucket += hits;
        if bucket >= steps {
            bucket -= steps;
            *slot = true;
        }
    }

    pattern
        .into_iter()
        .map(|hit| if hit { 'x' } else { '.' })
        .collect()
}

/// Register pattern API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register Pattern type
    engine.build_type::<Pattern>();
    engine.build_type::<PatternLaneBuilder>();

    // Constructor
    engine.register_fn("pattern", pattern);

    // Builder methods
    engine.register_fn("on", Pattern::on);
    engine.register_fn("on", Pattern::on_voice);
    engine.register_fn("step", Pattern::step);
    engine.register_fn("euclid", Pattern::euclid);
    engine.register_fn("len", Pattern::len);
    engine.register_fn("swing", Pattern::swing);
    engine.register_fn("quantize", Pattern::quantize);
    engine.register_fn("set_param", Pattern::set_param);
    engine.register_fn("lane", Pattern::lane);

    // Actions
    engine.register_fn("apply", Pattern::apply);
    engine.register_fn("start", Pattern::start);
    engine.register_fn("stop", Pattern::stop);
    engine.register_fn("launch", Pattern::launch);
    engine.register_fn("is_playing", Pattern::is_playing);
    engine.register_get("is_playing", |p: &mut Pattern| p.is_playing());
    engine.register_get("name", |p: &mut Pattern| p.name.clone());

    // Lane builder
    engine.register_fn("values", PatternLaneBuilder::values);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pattern_steps() {
        let events = parse_pattern_steps("x.x.", 4.0, 0.0);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_generate_euclidean() {
        assert_eq!(generate_euclidean(3, 8), "x..x..x.");
        assert_eq!(generate_euclidean(4, 8), "x.x.x.x.");
        assert_eq!(generate_euclidean(5, 8), "x.xx.xx.");
    }
}
