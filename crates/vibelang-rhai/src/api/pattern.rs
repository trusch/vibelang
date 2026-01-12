//! Pattern API for Rhai scripts.
//!
//! Patterns are rhythmic sequences that trigger voices.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::cell::RefCell;
use std::collections::HashMap;
use vibelang_core2::traits::{PatternConfig, Step};
use vibelang_core2::types::Beat;

use super::voice::Voice;
use crate::context;

// Global registry for patterns - allows looking up patterns by name
thread_local! {
    static PATTERN_REGISTRY: RefCell<HashMap<String, Pattern>> = RefCell::new(HashMap::new());
}

/// Clear the pattern registry (called when context is cleared).
pub fn clear_registry() {
    PATTERN_REGISTRY.with(|r| r.borrow_mut().clear());
}

/// Get a pattern from the registry by name.
fn get_pattern(name: &str) -> Option<Pattern> {
    PATTERN_REGISTRY.with(|r| r.borrow().get(name).cloned())
}

/// Store a pattern in the registry.
fn store_pattern(pattern: &Pattern) {
    PATTERN_REGISTRY.with(|r| {
        r.borrow_mut().insert(pattern.name.clone(), pattern.clone());
    });
}

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
    swing: f32,
    /// Group path.
    group_path: String,
    /// Parameters to pass to voice.
    params: HashMap<String, f32>,
}

impl Pattern {
    /// Create a new pattern with the given name.
    pub fn new(_ctx: NativeCallContext, name: String) -> Self {
        Self {
            name,
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
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
    ///
    /// This also syncs the voice's current state in case it has been modified.
    pub fn on_voice(mut self, voice: Voice) -> Self {
        voice.sync_to_state();
        self.voice_name = Some(voice.name);
        self
    }

    /// Set the step pattern.
    pub fn step(mut self, steps: String) -> Self {
        self.steps = Some(steps);
        self
    }

    /// Generate a Euclidean rhythm.
    pub fn euclid(mut self, hits: i64, total_steps: i64) -> Self {
        let pattern = generate_euclidean(hits as usize, total_steps as usize, 0);
        self.steps = Some(pattern);
        self
    }

    /// Generate a Euclidean rhythm with rotation.
    pub fn euclid_rotated(mut self, hits: i64, total_steps: i64, rotation: i64) -> Self {
        let pattern = generate_euclidean(hits as usize, total_steps as usize, rotation);
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
        self.swing = amount.clamp(0.0, 1.0) as f32;
        self
    }

    /// Set a parameter.
    pub fn set_param(mut self, param: String, value: f64) -> Self {
        self.params.insert(param, value as f32);
        self
    }

    /// Sync pattern to script state.
    fn sync_to_state(&self) {
        let pattern_id = context::get_or_create_pattern_id(&self.name);
        let voice_id = self
            .voice_name
            .as_ref()
            .map(|n| context::get_or_create_voice_id(n));

        // Calculate loop length from pattern if available
        let loop_length = if let Some(ref steps) = self.steps {
            calculate_loop_length_from_pattern(steps)
        } else {
            self.length
        };

        // Parse steps into Step events
        let steps = if let Some(ref step_str) = self.steps {
            parse_pattern_steps(step_str, loop_length, self.swing)
        } else {
            Vec::new()
        };

        let config = PatternConfig {
            name: self.name.clone(),
            voice: voice_id,
            steps,
            length: Beat::from_f64(loop_length),
            swing: self.swing,
        };

        context::with_state(|state| {
            state.patterns.insert(pattern_id, config);
        });
    }

    /// Register and apply the pattern (chainable).
    pub fn apply(self) -> Self {
        self.sync_to_state();
        // Store in registry for later lookup
        store_pattern(&self);
        self
    }

    /// Start the pattern playing (chainable).
    pub fn start(self) -> Self {
        self.sync_to_state();

        // Register that this pattern should start
        let pattern_id = context::get_or_create_pattern_id(&self.name);
        context::with_state(|state| {
            state.playing_patterns.insert(pattern_id);
        });

        self
    }

    /// Launch the pattern with quantization (chainable).
    ///
    /// This schedules the pattern to start at the next quantization boundary.
    /// Uses the global quantization setting.
    pub fn launch(self) -> Self {
        // For now, launch behaves the same as start.
        // The runtime will use the quantization setting to determine when to actually start.
        self.start()
    }

    /// Stop the pattern.
    pub fn stop(&mut self) {
        let pattern_id = context::get_or_create_pattern_id(&self.name);
        context::with_state(|state| {
            state.playing_patterns.remove(&pattern_id);
        });
    }

    /// Check if the pattern is playing.
    pub fn is_playing(&mut self) -> bool {
        let pattern_id = context::get_or_create_pattern_id(&self.name);
        context::with_state(|state| state.playing_patterns.contains(&pattern_id))
    }
}

/// Split pattern string into bars.
fn split_into_bars(pattern: &str) -> Vec<String> {
    pattern
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Count bars in pattern.
fn count_bars(pattern: &str) -> usize {
    split_into_bars(pattern).len().max(1)
}

/// Calculate loop length from pattern.
fn calculate_loop_length_from_pattern(pattern: &str) -> f64 {
    let num_bars = count_bars(pattern);
    let beats_per_bar = 4.0;
    num_bars as f64 * beats_per_bar
}

/// Parse pattern steps into Step events.
fn parse_pattern_steps(steps: &str, _length: f64, swing: f32) -> Vec<Step> {
    let mut result = Vec::new();
    let bars = split_into_bars(steps);
    let beats_per_bar = 4.0;

    let mut current_beat = 0.0;
    let mut step_index = 0;

    for bar in bars {
        let tokens: Vec<char> = bar.chars().filter(|c| !c.is_whitespace()).collect();

        if tokens.is_empty() {
            current_beat += beats_per_bar;
            continue;
        }

        let beat_per_token = beats_per_bar / tokens.len() as f64;

        for (i, ch) in tokens.iter().enumerate() {
            let beat = current_beat + i as f64 * beat_per_token;

            // Apply swing to off-beats
            let swung_beat = if step_index % 2 == 1 {
                beat + swing as f64 * beat_per_token * 0.5
            } else {
                beat
            };

            // Parse velocity from token character
            // x = normal (0.7), X = accent (1.0), o = ghost (0.3)
            // 1-9 = scaled velocity (0.1 to 1.0)
            let velocity = match ch {
                'x' => Some(0.7),       // Normal hit
                'X' => Some(1.0),       // Accent/loud
                'o' | 'O' => Some(0.3), // Ghost note/soft
                '1'..='9' => {
                    // 1 = 0.11, 5 = 0.55, 9 = 1.0
                    let digit = (*ch as u8 - b'0') as f32;
                    Some(digit / 9.0)
                }
                '.' | '_' | '0' | '-' => None, // Rest
                _ => None,
            };

            if let Some(vel) = velocity {
                let mut params = HashMap::new();
                params.insert("amp".to_string(), vel);
                result.push(Step {
                    beat: Beat::from_f64(swung_beat),
                    params,
                });
            }

            step_index += 1;
        }

        current_beat += beats_per_bar;
    }

    result
}

/// Generate a Euclidean rhythm pattern with optional rotation.
/// Uses a Bresenham-style algorithm for even distribution.
fn generate_euclidean(hits: usize, steps: usize, rotation: i64) -> String {
    if steps == 0 {
        return String::new();
    }
    if hits >= steps {
        return "x".repeat(steps);
    }
    if hits == 0 {
        return ".".repeat(steps);
    }

    // Bresenham-style algorithm: evenly distribute hits across steps
    let mut pattern = vec!['.'; steps];

    for i in 0..hits {
        // Calculate position for each hit, scaled to step range
        let pos = (i * steps + steps / (2 * hits)) / hits;
        pattern[pos] = 'x';
    }

    // Apply rotation (positive = shift right, negative = shift left)
    if rotation != 0 {
        let rot = rotation.rem_euclid(steps as i64) as usize;
        pattern.rotate_right(rot);
    }

    pattern.into_iter().collect()
}

/// Create a new pattern builder or return an existing one.
///
/// If a pattern with this name already exists in the registry,
/// returns a clone of it. Otherwise creates a new empty pattern.
pub fn pattern(ctx: NativeCallContext, name: String) -> Pattern {
    // Check if pattern already exists in registry
    if let Some(existing) = get_pattern(&name) {
        return existing;
    }
    // Create new pattern
    Pattern::new(ctx, name)
}

/// Register pattern API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register Pattern type
    engine.build_type::<Pattern>();

    // Constructor
    engine.register_fn("pattern", pattern);

    // Builder methods
    engine.register_fn("on", Pattern::on);
    engine.register_fn("on", Pattern::on_voice);
    engine.register_fn("step", Pattern::step);
    engine.register_fn("euclid", Pattern::euclid);
    engine.register_fn("euclid", Pattern::euclid_rotated);
    engine.register_fn("len", Pattern::len);
    engine.register_fn("swing", Pattern::swing);
    engine.register_fn("set_param", Pattern::set_param);

    // Actions
    engine.register_fn("apply", Pattern::apply);
    engine.register_fn("start", Pattern::start);
    engine.register_fn("launch", Pattern::launch);
    engine.register_fn("stop", Pattern::stop);
    engine.register_fn("is_playing", Pattern::is_playing);
    engine.register_get("playing", Pattern::is_playing);
    engine.register_get("name", |p: &mut Pattern| p.name.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Euclidean Pattern Tests ====================

    #[test]
    fn test_generate_euclidean() {
        // E(3,8) - 3 hits in 8 steps, evenly distributed
        let pattern = generate_euclidean(3, 8, 0);
        assert_eq!(pattern.chars().filter(|&c| c == 'x').count(), 3);
        assert_eq!(pattern.len(), 8);

        // E(4,8) - 4 hits in 8 steps
        let pattern = generate_euclidean(4, 8, 0);
        assert_eq!(pattern.chars().filter(|&c| c == 'x').count(), 4);
        assert_eq!(pattern.len(), 8);

        // Edge cases
        assert_eq!(generate_euclidean(0, 8, 0), "........");
        assert_eq!(generate_euclidean(8, 8, 0), "xxxxxxxx");
        assert_eq!(generate_euclidean(3, 0, 0), "");
    }

    #[test]
    fn test_generate_euclidean_common_rhythms() {
        // E(5,8) - common pattern (Cuban tresillo variant)
        let pattern = generate_euclidean(5, 8, 0);
        assert_eq!(pattern.chars().filter(|&c| c == 'x').count(), 5);
        assert_eq!(pattern.len(), 8);

        // E(3,4) - basic on-beat pattern
        let pattern = generate_euclidean(3, 4, 0);
        assert_eq!(pattern.chars().filter(|&c| c == 'x').count(), 3);
        assert_eq!(pattern.len(), 4);

        // E(7,12) - more complex
        let pattern = generate_euclidean(7, 12, 0);
        assert_eq!(pattern.chars().filter(|&c| c == 'x').count(), 7);
        assert_eq!(pattern.len(), 12);
    }

    #[test]
    fn test_generate_euclidean_all_hits() {
        // When hits >= steps, all should be 'x'
        assert_eq!(generate_euclidean(4, 4, 0), "xxxx");
        assert_eq!(generate_euclidean(5, 4, 0), "xxxx"); // Clamped
        assert_eq!(generate_euclidean(16, 16, 0), "xxxxxxxxxxxxxxxx");
    }

    #[test]
    fn test_generate_euclidean_no_hits() {
        assert_eq!(generate_euclidean(0, 4, 0), "....");
        assert_eq!(generate_euclidean(0, 8, 0), "........");
        assert_eq!(generate_euclidean(0, 16, 0), "................");
    }

    #[test]
    fn test_generate_euclidean_single_hit() {
        let pattern = generate_euclidean(1, 8, 0);
        assert_eq!(pattern.chars().filter(|&c| c == 'x').count(), 1);
        assert_eq!(pattern.len(), 8);
    }

    #[test]
    fn test_generate_euclidean_rotation() {
        // Base pattern: x..x..x. (3 hits in 8 steps)
        let base = generate_euclidean(3, 8, 0);

        // Rotate by 1: should shift pattern right by 1
        let rotated1 = generate_euclidean(3, 8, 1);
        assert_eq!(rotated1.chars().filter(|&c| c == 'x').count(), 3);
        assert_eq!(rotated1.len(), 8);
        assert_ne!(base, rotated1);

        // Rotate by steps should give same pattern
        let rotated_full = generate_euclidean(3, 8, 8);
        assert_eq!(base, rotated_full);

        // Negative rotation should work
        let rotated_neg = generate_euclidean(3, 8, -1);
        assert_eq!(rotated_neg.chars().filter(|&c| c == 'x').count(), 3);
    }

    // ==================== Bar Splitting Tests ====================

    #[test]
    fn test_split_into_bars() {
        let bars = split_into_bars("x..x|..x.|x...");
        assert_eq!(bars.len(), 3);
        assert_eq!(bars[0], "x..x");
        assert_eq!(bars[1], "..x.");
        assert_eq!(bars[2], "x...");
    }

    #[test]
    fn test_split_into_bars_with_whitespace() {
        let bars = split_into_bars("x..x | ..x. | x...");
        assert_eq!(bars.len(), 3);
        assert_eq!(bars[0], "x..x");
        assert_eq!(bars[1], "..x.");
        assert_eq!(bars[2], "x...");
    }

    #[test]
    fn test_split_into_bars_single_bar() {
        let bars = split_into_bars("x.x.x.x.");
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0], "x.x.x.x.");
    }

    #[test]
    fn test_split_into_bars_empty() {
        let bars = split_into_bars("");
        assert_eq!(bars.len(), 0);
    }

    #[test]
    fn test_split_into_bars_empty_segments() {
        let bars = split_into_bars("x..x||..x.");
        assert_eq!(bars.len(), 2); // Empty segment filtered out
    }

    // ==================== Count Bars Tests ====================

    #[test]
    fn test_count_bars() {
        assert_eq!(count_bars("x..x|..x.|x..."), 3);
        assert_eq!(count_bars("x.x.x.x."), 1);
        assert_eq!(count_bars(""), 1); // min 1
    }

    // ==================== Loop Length Tests ====================

    #[test]
    fn test_calculate_loop_length_from_pattern() {
        // Single bar = 4 beats
        assert!((calculate_loop_length_from_pattern("x.x.x.x.") - 4.0).abs() < 0.001);

        // Two bars = 8 beats
        assert!((calculate_loop_length_from_pattern("x..x|..x.") - 8.0).abs() < 0.001);

        // Four bars = 16 beats
        assert!((calculate_loop_length_from_pattern("x...|..x.|.x..|...x") - 16.0).abs() < 0.001);
    }

    // ==================== Pattern Step Parsing Tests ====================

    #[test]
    fn test_parse_pattern_steps_basic() {
        let steps = parse_pattern_steps("x...", 4.0, 0.0);
        assert_eq!(steps.len(), 1);
        assert!((steps[0].beat.to_f64() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_pattern_steps_multiple_hits() {
        let steps = parse_pattern_steps("x.x.", 4.0, 0.0);
        assert_eq!(steps.len(), 2);
        assert!((steps[0].beat.to_f64() - 0.0).abs() < 0.001);
        assert!((steps[1].beat.to_f64() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_pattern_steps_all_hits() {
        let steps = parse_pattern_steps("xxxx", 4.0, 0.0);
        assert_eq!(steps.len(), 4);
    }

    #[test]
    fn test_parse_pattern_steps_no_hits() {
        let steps = parse_pattern_steps("....", 4.0, 0.0);
        assert_eq!(steps.len(), 0);
    }

    #[test]
    fn test_parse_pattern_steps_velocity() {
        // 'x' = velocity 0.7 (normal)
        let steps = parse_pattern_steps("x", 4.0, 0.0);
        assert_eq!(steps[0].params.get("amp"), Some(&0.7));

        // 'X' = velocity 1.0 (accent)
        let steps = parse_pattern_steps("X", 4.0, 0.0);
        assert_eq!(steps[0].params.get("amp"), Some(&1.0));

        // 'o' = velocity 0.3 (ghost note)
        let steps = parse_pattern_steps("o", 4.0, 0.0);
        assert_eq!(steps[0].params.get("amp"), Some(&0.3));

        // Numeric values 1-9 = scaled velocity (1/9 to 9/9)
        let steps = parse_pattern_steps("1", 4.0, 0.0);
        let vel = *steps[0].params.get("amp").unwrap();
        assert!((vel - 1.0 / 9.0).abs() < 0.01); // ~0.11

        let steps = parse_pattern_steps("5", 4.0, 0.0);
        let vel = *steps[0].params.get("amp").unwrap();
        assert!((vel - 5.0 / 9.0).abs() < 0.01); // ~0.55

        let steps = parse_pattern_steps("9", 4.0, 0.0);
        let vel = *steps[0].params.get("amp").unwrap();
        assert!((vel - 1.0).abs() < 0.01); // 1.0
    }

    #[test]
    fn test_parse_pattern_steps_rest_markers() {
        // '.', '_', '0', '-' are all rests
        let steps = parse_pattern_steps("._0-", 4.0, 0.0);
        assert_eq!(steps.len(), 0);
    }

    #[test]
    fn test_parse_pattern_steps_two_bars() {
        let steps = parse_pattern_steps("x...|..x.", 8.0, 0.0);
        assert_eq!(steps.len(), 2);
        assert!((steps[0].beat.to_f64() - 0.0).abs() < 0.001);
        assert!((steps[1].beat.to_f64() - 6.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_pattern_steps_with_swing() {
        let steps = parse_pattern_steps("xx", 4.0, 0.5);
        assert_eq!(steps.len(), 2);
        // First beat should be unswung
        assert!((steps[0].beat.to_f64() - 0.0).abs() < 0.001);
        // Second beat should be swung (delayed)
        assert!(steps[1].beat.to_f64() > 2.0);
    }

    #[test]
    fn test_parse_pattern_steps_whitespace_ignored() {
        let steps = parse_pattern_steps("x . x .", 4.0, 0.0);
        assert_eq!(steps.len(), 2);
    }

    // ==================== Pattern Builder Tests ====================

    #[test]
    fn test_pattern_default_values() {
        // Create a mock pattern (without NativeCallContext)
        let pattern = Pattern {
            name: "test".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        };

        assert_eq!(pattern.name, "test");
        assert!(pattern.voice_name.is_none());
        assert!(pattern.steps.is_none());
        assert!((pattern.length - 4.0).abs() < 0.001);
        assert!((pattern.swing - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_pattern_on() {
        let pattern = Pattern {
            name: "test".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        };

        let pattern = pattern.on("kick".to_string());
        assert_eq!(pattern.voice_name, Some("kick".to_string()));
    }

    #[test]
    fn test_pattern_step() {
        let pattern = Pattern {
            name: "test".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        };

        let pattern = pattern.step("x..x..x.".to_string());
        assert_eq!(pattern.steps, Some("x..x..x.".to_string()));
    }

    #[test]
    fn test_pattern_euclid() {
        let pattern = Pattern {
            name: "test".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        };

        let pattern = pattern.euclid(3, 8);
        assert!(pattern.steps.is_some());
        let steps = pattern.steps.unwrap();
        assert_eq!(steps.chars().filter(|&c| c == 'x').count(), 3);
        assert_eq!(steps.len(), 8);
    }

    #[test]
    fn test_pattern_len() {
        let pattern = Pattern {
            name: "test".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        };

        let pattern = pattern.len(8.0);
        assert!((pattern.length - 8.0).abs() < 0.001);
    }

    #[test]
    fn test_pattern_swing() {
        let pattern = Pattern {
            name: "test".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        };

        let pattern = pattern.swing(0.5);
        assert!((pattern.swing - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_pattern_swing_clamping() {
        let pattern = Pattern {
            name: "test".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        };

        // Should clamp to 0.0-1.0
        let pattern = pattern.swing(1.5);
        assert!((pattern.swing - 1.0).abs() < 0.001);

        let pattern = Pattern {
            name: "test".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        };

        let pattern = pattern.swing(-0.5);
        assert!((pattern.swing - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_pattern_set_param() {
        let pattern = Pattern {
            name: "test".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        };

        let pattern = pattern.set_param("decay".to_string(), 0.2);
        assert_eq!(pattern.params.get("decay"), Some(&0.2_f32));
    }

    #[test]
    fn test_pattern_chained_builders() {
        let pattern = Pattern {
            name: "kick".to_string(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: "main".to_string(),
            params: HashMap::new(),
        }
        .on("kick_voice".to_string())
        .step("x..x..x.".to_string())
        .len(8.0)
        .swing(0.3)
        .set_param("decay".to_string(), 0.1);

        assert_eq!(pattern.voice_name, Some("kick_voice".to_string()));
        assert_eq!(pattern.steps, Some("x..x..x.".to_string()));
        assert!((pattern.length - 8.0).abs() < 0.001);
        assert!((pattern.swing - 0.3).abs() < 0.001);
        assert_eq!(pattern.params.get("decay"), Some(&0.1_f32));
    }
}
