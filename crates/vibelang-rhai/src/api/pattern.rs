//! Pattern API for Rhai scripts.
//!
//! Patterns are rhythmic sequences that trigger voices.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use vibelang_core::traits::{PatternConfig, Step};
use vibelang_core::types::Beat;

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

    /// Create an anonymous pattern (name resolved at finalization).
    pub fn new_anon(_ctx: NativeCallContext) -> Self {
        Self {
            name: String::new(),
            voice_name: None,
            steps: None,
            length: 4.0,
            swing: 0.0,
            group_path: context::current_group_path(),
            params: HashMap::new(),
        }
    }

    /// Resolve the name for an anonymous pattern from its voice target.
    ///
    /// Uses the voice name to generate `_{voice_name}_pat`.
    /// No-op if the pattern already has a name.
    fn resolve_name(&mut self) {
        if !self.name.is_empty() {
            return;
        }
        let voice = self.voice_name.as_deref().unwrap_or("unknown");
        let base = format!("_{}_pat", voice);
        self.name = context::resolve_auto_name(&base);
    }

    // === Builder methods ===

    /// Set the voice to trigger (by name).
    pub fn on(mut self, voice_name: String) -> Self {
        self.voice_name = Some(voice_name);
        self
    }

    /// Set the voice to trigger (by Voice object).
    ///
    /// This also resolves anonymous voice names and syncs the voice's current state.
    pub fn on_voice(mut self, mut voice: Voice) -> Self {
        voice.resolve_name();
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
    ///
    /// Skips registration for anonymous patterns (empty name) not yet resolved.
    fn sync_to_state(&self) -> Result<(), Box<EvalAltResult>> {
        if self.name.is_empty() {
            return Ok(()); // Anonymous pattern not yet resolved — defer registration
        }
        let pattern_id = context::get_or_create_pattern_id(&self.name);
        let voice_id = self
            .voice_name
            .as_ref()
            .map(|n| context::get_or_create_voice_id(n));

        // Warn if the voice doesn't exist in the script state yet
        if let Some(ref voice_name) = self.voice_name {
            if let Some(vid) = voice_id {
                context::with_state(|state| {
                    if !state.voices.contains_key(&vid) {
                        tracing::warn!(
                            "Pattern '{}': voice '{}' not found — make sure to create it with voice(\"{}\")",
                            self.name, voice_name, voice_name
                        );
                    }
                });
            }
        }

        // Calculate loop length from pattern if available
        let loop_length = if let Some(ref steps) = self.steps {
            calculate_loop_length_from_pattern(steps)
        } else {
            self.length
        };

        // Parse steps into Step events
        let steps = if let Some(ref step_str) = self.steps {
            parse_pattern_steps(step_str, loop_length, self.swing)?
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

        Ok(())
    }

    /// Register and apply the pattern (chainable).
    ///
    /// For anonymous patterns, resolves the name from the voice target before registering.
    pub fn apply(mut self) -> Result<Self, Box<EvalAltResult>> {
        self.resolve_name();
        self.sync_to_state()?;
        // Store in registry for later lookup
        store_pattern(&self);
        Ok(self)
    }

    /// Start the pattern playing (chainable).
    ///
    /// For anonymous patterns, resolves the name from the voice target before registering.
    pub fn start(mut self) -> Result<Self, Box<EvalAltResult>> {
        self.resolve_name();
        self.sync_to_state()?;

        // Register that this pattern should start
        let pattern_id = context::get_or_create_pattern_id(&self.name);
        context::with_state(|state| {
            state.playing_patterns.insert(pattern_id);
        });

        Ok(self)
    }

    /// Launch the pattern (chainable).
    ///
    /// **Currently an alias of [`Self::start`]** — quantized launch (starting
    /// at the next quantization boundary) is not yet implemented. The pattern
    /// starts exactly as if `.start()` had been called.
    pub fn launch(self) -> Result<Self, Box<EvalAltResult>> {
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
fn parse_pattern_steps(
    steps: &str,
    _length: f64,
    swing: f32,
) -> Result<Vec<Step>, Box<EvalAltResult>> {
    let mut result = Vec::new();
    let bars = split_into_bars(steps);
    let beats_per_bar = 4.0;

    let mut current_beat = 0.0;
    let mut step_index = 0;
    let mut global_pos: usize = 0;

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
                _ => {
                    return Err(Box::new(EvalAltResult::ErrorRuntime(
                        format!(
                            "Pattern parse error: invalid step character '{}' at position {} \
                             — valid chars are x X o O 1-9 . _ 0 - |",
                            ch, global_pos
                        )
                        .into(),
                        Position::NONE,
                    )));
                }
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
            global_pos += 1;
        }

        current_beat += beats_per_bar;
        global_pos += 1; // account for '|' separator
    }

    Ok(result)
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

/// Create an anonymous pattern builder (name resolved from voice target).
pub fn pattern_anon(ctx: NativeCallContext) -> Pattern {
    Pattern::new_anon(ctx)
}

/// Register pattern API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register Pattern type
    engine.build_type::<Pattern>();

    // Constructor (named and anonymous overloads)
    engine.register_fn("pattern", pattern);
    engine.register_fn("pattern", pattern_anon);

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

use vibelang_core::candidate::{
    AuthoringDeclaration, Cancellation, CandidateError, CandidateFragment, CanonicalF64,
    Composition, DeclarationOwner, DeclarationPayload, DesiredLifecycle, GroupScope,
    LifecycleAction, LifecycleMetadata, PatternAuthoring, PatternKind, PatternStepAuthoring,
    StartMode, TerminalEffect, VoiceKind,
};

use super::voice::VoiceRef;
use crate::foundation::{self, BuilderBase, FoundationError, Observation, RefBase};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternRef {
    base: RefBase,
}

impl PatternRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<PatternKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    fn action(self, action: LifecycleAction, role: &str) -> Result<Self, FoundationError> {
        let (effect, cancellation) = match &action {
            LifecycleAction::Start(_) => (TerminalEffect::Start, Cancellation::BeforePlanning),
            LifecycleAction::Stop => (TerminalEffect::Stop, Cancellation::NotCancellable),
            LifecycleAction::Remove => (TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::Cancel => (TerminalEffect::Cancel, Cancellation::BeforePlanning),
            _ => {
                return Err(CandidateError::InvalidLifecycle(
                    "unsupported PatternRef lifecycle action".into(),
                )
                .into())
            }
        };
        let source = foundation::operation_source(&self.base, role)?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(effect, cancellation),
            action,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn start(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Start(StartMode::Normal), "start")
    }

    pub fn start_now(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Start(StartMode::Immediate), "start-now")
    }

    pub fn stop(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Stop, "stop")
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Remove, "remove")
    }

    pub fn cancel(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Cancel, "cancel")
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatternContent {
    Steps(String),
    Euclidean {
        hits: usize,
        steps: usize,
        rotation: i64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatternBuilder {
    base: BuilderBase,
    voice: Option<VoiceRef>,
    content: Option<PatternContent>,
    length: Option<f64>,
    swing: f64,
    params: BTreeMap<String, f64>,
}

impl PatternBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        Self {
            base,
            voice: None,
            content: None,
            length: None,
            swing: 0.0,
            params: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn on(mut self, voice: VoiceRef) -> Self {
        self.voice = Some(voice);
        self
    }

    pub fn step(mut self, steps: String) -> Result<Self, FoundationError> {
        if matches!(&self.content, Some(PatternContent::Euclidean { .. })) {
            return Err(CandidateError::InvalidAuthoring(
                "Pattern step text and Euclidean content cannot be combined".into(),
            )
            .into());
        }
        validate_pattern_text_v2(&steps)?;
        parse_pattern_steps(
            &steps,
            calculate_loop_length_from_pattern(&steps),
            self.swing as f32,
        )
        .map_err(|error| {
            FoundationError::Candidate(CandidateError::InvalidAuthoring(error.to_string()))
        })?;
        self.content = Some(PatternContent::Steps(steps));
        Ok(self)
    }

    pub fn euclid(mut self, hits: i64, steps: i64) -> Result<Self, FoundationError> {
        self = self.euclid_rotated(hits, steps, 0)?;
        Ok(self)
    }

    pub fn euclid_rotated(
        mut self,
        hits: i64,
        steps: i64,
        rotation: i64,
    ) -> Result<Self, FoundationError> {
        if matches!(&self.content, Some(PatternContent::Steps(_))) {
            return Err(CandidateError::InvalidAuthoring(
                "Pattern step text and Euclidean content cannot be combined".into(),
            )
            .into());
        }
        if steps <= 0 || hits < 0 || hits > steps || steps > 4096 {
            return Err(CandidateError::InvalidAuthoring(
                "Euclidean rhythm needs 0 <= hits <= steps <= 4096 and steps > 0".into(),
            )
            .into());
        }
        self.content = Some(PatternContent::Euclidean {
            hits: hits as usize,
            steps: steps as usize,
            rotation,
        });
        Ok(self)
    }

    pub fn len(mut self, beats: f64) -> Result<Self, FoundationError> {
        if !beats.is_finite() || beats <= 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Pattern length must be finite and positive".into(),
            )
            .into());
        }
        self.length = Some(beats);
        Ok(self)
    }

    pub fn swing(mut self, amount: f64) -> Result<Self, FoundationError> {
        if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
            return Err(CandidateError::InvalidAuthoring(
                "Pattern swing must be in 0.0..=1.0".into(),
            )
            .into());
        }
        self.swing = amount;
        Ok(self)
    }

    pub fn set_param(mut self, name: String, value: f64) -> Result<Self, FoundationError> {
        self.base = self
            .base
            .configured(format!("param.{name}"), value.to_bits().to_be_bytes())?;
        self.params.insert(name, value);
        Ok(self)
    }

    pub(crate) fn build_fragment(
        self,
        lifecycle: DesiredLifecycle,
    ) -> Result<(CandidateFragment, PatternRef), FoundationError> {
        let voice = self.voice.ok_or_else(|| {
            CandidateError::InvalidAuthoring("PatternBuilder needs a typed VoiceRef".into())
        })?;
        let content = self.content.ok_or_else(|| {
            CandidateError::InvalidAuthoring("PatternBuilder needs tagged step content".into())
        })?;
        let step_string = match &content {
            PatternContent::Steps(steps) => steps.clone(),
            PatternContent::Euclidean {
                hits,
                steps,
                rotation,
            } => generate_euclidean(*hits, *steps, *rotation),
        };
        let length = self.length.unwrap_or_else(|| match &content {
            PatternContent::Steps(steps) => calculate_loop_length_from_pattern(steps),
            PatternContent::Euclidean { .. } => 4.0,
        });
        let parsed =
            parse_pattern_steps(&step_string, length, self.swing as f32).map_err(|error| {
                FoundationError::Candidate(CandidateError::InvalidAuthoring(error.to_string()))
            })?;
        let steps = parsed
            .into_iter()
            .map(|step| {
                Ok(PatternStepAuthoring {
                    beat_ticks: step.beat.raw(),
                    velocity: CanonicalF64::new(f64::from(
                        *step.params.get("amp").unwrap_or(&0.0),
                    ))?,
                })
            })
            .collect::<Result<Vec<_>, CandidateError>>()?;
        let params = self
            .params
            .into_iter()
            .map(|(name, value)| Ok((name, CanonicalF64::new(value)?)))
            .collect::<Result<BTreeMap<_, _>, CandidateError>>()?;
        let declaration = PatternAuthoring {
            voice: voice.base().typed::<VoiceKind>()?,
            steps,
            length_ticks: Beat::from_f64(length).raw(),
            swing: CanonicalF64::new(self.swing)?,
            params,
            lifecycle,
        };
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::Pattern(declaration))?;
        let owner = DeclarationOwner::Structural(self.base.source().syntax_key().clone());
        let metadata = match lifecycle {
            DesiredLifecycle::Dormant => LifecycleMetadata::register(Composition::Standalone),
            DesiredLifecycle::Start(_) => LifecycleMetadata::start(Composition::Standalone),
        };
        let dependency_source = self.base.source().clone();
        let (fragment, reference) = self.base.fragment(
            owner,
            metadata,
            payload,
            [(voice.base().clone(), dependency_source)],
        )?;
        Ok((fragment, PatternRef::new(reference)?))
    }

    fn terminal(self, lifecycle: DesiredLifecycle) -> Result<PatternRef, FoundationError> {
        let (fragment, reference) = self.build_fragment(lifecycle)?;
        foundation::commit_fragment(fragment)?;
        Ok(reference)
    }

    pub fn apply(self) -> Result<PatternRef, FoundationError> {
        self.terminal(DesiredLifecycle::Dormant)
    }

    pub fn start(self) -> Result<PatternRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Normal))
    }

    pub fn start_now(self) -> Result<PatternRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Immediate))
    }

    pub fn launch(self) -> Result<PatternRef, FoundationError> {
        self.start()
    }
}

pub(crate) fn pattern_builder_v2(name: String) -> Result<PatternBuilder, Box<EvalAltResult>> {
    Ok(PatternBuilder::new(
        foundation::authoring_builder::<PatternKind>(&name, GroupScope::root())
            .map_err(|error| v2_error(error, Position::NONE))?,
    ))
}

pub(crate) fn pattern_ref_v2(name: String) -> Result<PatternRef, Box<EvalAltResult>> {
    PatternRef::new(
        foundation::authoring_ref::<PatternKind>(&name, GroupScope::root())
            .map_err(|error| v2_error(error, Position::NONE))?,
    )
    .map_err(|error| v2_error(error, Position::NONE))
}

fn v2_error(error: FoundationError, position: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        error.to_string().into(),
        position,
    ))
}

#[cfg(test)]
fn install_v2_for_tests(engine: &mut Engine) {
    engine
        .register_type_with_name::<PatternContent>("PatternContent")
        .register_type_with_name::<PatternBuilder>("PatternBuilder")
        .register_type_with_name::<PatternRef>("PatternRef")
        .register_fn("pattern", pattern_builder_v2)
        .register_fn("pattern_ref", pattern_ref_v2)
        .register_fn("on", PatternBuilder::on)
        .register_fn("step", |builder: PatternBuilder, steps: String| {
            builder
                .step(steps)
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn(
            "euclid",
            |builder: PatternBuilder, hits: i64, steps: i64| {
                builder
                    .euclid(hits, steps)
                    .map_err(|error| v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "euclid",
            |builder: PatternBuilder, hits: i64, steps: i64, rotation: i64| {
                builder
                    .euclid_rotated(hits, steps, rotation)
                    .map_err(|error| v2_error(error, Position::NONE))
            },
        )
        .register_fn("len", |builder: PatternBuilder, beats: f64| {
            builder
                .len(beats)
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("swing", |builder: PatternBuilder, amount: f64| {
            builder
                .swing(amount)
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn(
            "set_param",
            |builder: PatternBuilder, name: String, value: f64| {
                builder
                    .set_param(name, value)
                    .map_err(|error| v2_error(error, Position::NONE))
            },
        )
        .register_fn("apply", |builder: PatternBuilder| {
            builder
                .apply()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("start", |builder: PatternBuilder| {
            builder
                .start()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("start_now", |builder: PatternBuilder| {
            builder
                .start_now()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("launch", |builder: PatternBuilder| {
            builder
                .launch()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("start", |reference: PatternRef| {
            reference
                .start()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("start_now", |reference: PatternRef| {
            reference
                .start_now()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("stop", |reference: PatternRef| {
            reference
                .stop()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("remove", |reference: PatternRef| {
            reference
                .remove()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("cancel", |reference: PatternRef| {
            reference
                .cancel()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("status", |reference: PatternRef| {
            reference
                .status()
                .map_err(|error| v2_error(error, Position::NONE))
        });
}

fn validate_pattern_text_v2(steps: &str) -> Result<(), FoundationError> {
    if steps.trim().is_empty() || steps.split('|').any(|bar| bar.trim().is_empty()) {
        return Err(CandidateError::InvalidAuthoring(
            "Pattern text needs one or more non-empty bars".into(),
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v2_identity() -> vibelang_core::candidate::EvaluationIdentity {
        vibelang_core::candidate::EvaluationIdentity::new(
            vibelang_core::candidate::LanguageContract::v2(
                vibelang_core::candidate::ContractDigest::from_bytes(b"pattern-v2-test"),
            ),
            vibelang_core::candidate::EngineInstanceId::new(),
            vibelang_core::mutation::RuntimeEpoch::new(),
        )
    }

    fn v2_voice_ref(name: &str) -> VoiceRef {
        VoiceRef::new(foundation::authoring_ref::<VoiceKind>(name, GroupScope::root()).unwrap())
            .unwrap()
    }

    #[test]
    fn v2_pattern_configuration_is_pure_strict_and_tagged() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let voice = v2_voice_ref("lead");
        let steps = PatternBuilder::new(
            foundation::authoring_builder::<PatternKind>("steps", GroupScope::root()).unwrap(),
        )
        .on(voice.clone())
        .step("x...".into())
        .unwrap();
        let euclidean = PatternBuilder::new(
            foundation::authoring_builder::<PatternKind>("euclid", GroupScope::root()).unwrap(),
        )
        .on(voice)
        .euclid_rotated(3, 8, -1)
        .unwrap();

        assert!(matches!(steps.content, Some(PatternContent::Steps(_))));
        assert!(matches!(
            euclidean.content,
            Some(PatternContent::Euclidean { .. })
        ));
        assert!(matches!(
            PatternBuilder::new(
                foundation::authoring_builder::<PatternKind>("bad", GroupScope::root()).unwrap()
            )
            .step("x||x".into()),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            steps.clone().euclid(3, 8),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            euclidean.clone().step("x...".into()),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        let candidate = foundation::finish_evaluation().unwrap();
        assert!(candidate.declarations().is_empty());
    }

    #[test]
    fn v2_pattern_launch_alias_matches_start_and_immediate_remains_distinct() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let voice = crate::api::voice::VoiceBuilder::new(
            foundation::authoring_builder::<VoiceKind>("lead", GroupScope::root()).unwrap(),
        )
        .synth("sine".into())
        .unwrap()
        .apply()
        .unwrap();
        let build = |name: &str| {
            PatternBuilder::new(
                foundation::authoring_builder::<PatternKind>(name, GroupScope::root()).unwrap(),
            )
            .on(voice.clone())
            .step("x...".into())
            .unwrap()
        };
        let started = build("started").start().unwrap();
        build("launched").launch().unwrap();
        build("immediate").start_now().unwrap();

        assert!(matches!(
            started.status(),
            Err(FoundationError::ObservationUnavailable)
        ));
        let candidate = foundation::finish_evaluation().unwrap();
        let lifecycles = candidate
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration.payload() {
                DeclarationPayload::Authoring {
                    declaration: AuthoringDeclaration::Pattern(pattern),
                    ..
                } => Some((
                    declaration.address().key().as_str(),
                    (pattern.lifecycle, declaration.lifecycle().terminal_effect),
                )),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            lifecycles["started"],
            (
                DesiredLifecycle::Start(StartMode::Normal),
                TerminalEffect::Start
            )
        );
        assert_eq!(lifecycles["launched"], lifecycles["started"]);
        assert_eq!(
            lifecycles["immediate"].0,
            DesiredLifecycle::Start(StartMode::Immediate)
        );
    }

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
        let steps = parse_pattern_steps("x...", 4.0, 0.0).unwrap();
        assert_eq!(steps.len(), 1);
        assert!((steps[0].beat.to_f64() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_pattern_steps_multiple_hits() {
        let steps = parse_pattern_steps("x.x.", 4.0, 0.0).unwrap();
        assert_eq!(steps.len(), 2);
        assert!((steps[0].beat.to_f64() - 0.0).abs() < 0.001);
        assert!((steps[1].beat.to_f64() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_pattern_steps_all_hits() {
        let steps = parse_pattern_steps("xxxx", 4.0, 0.0).unwrap();
        assert_eq!(steps.len(), 4);
    }

    #[test]
    fn test_parse_pattern_steps_no_hits() {
        let steps = parse_pattern_steps("....", 4.0, 0.0).unwrap();
        assert_eq!(steps.len(), 0);
    }

    #[test]
    fn test_parse_pattern_steps_velocity() {
        // 'x' = velocity 0.7 (normal)
        let steps = parse_pattern_steps("x", 4.0, 0.0).unwrap();
        assert_eq!(steps[0].params.get("amp"), Some(&0.7));

        // 'X' = velocity 1.0 (accent)
        let steps = parse_pattern_steps("X", 4.0, 0.0).unwrap();
        assert_eq!(steps[0].params.get("amp"), Some(&1.0));

        // 'o' = velocity 0.3 (ghost note)
        let steps = parse_pattern_steps("o", 4.0, 0.0).unwrap();
        assert_eq!(steps[0].params.get("amp"), Some(&0.3));

        // Numeric values 1-9 = scaled velocity (1/9 to 9/9)
        let steps = parse_pattern_steps("1", 4.0, 0.0).unwrap();
        let vel = *steps[0].params.get("amp").unwrap();
        assert!((vel - 1.0 / 9.0).abs() < 0.01); // ~0.11

        let steps = parse_pattern_steps("5", 4.0, 0.0).unwrap();
        let vel = *steps[0].params.get("amp").unwrap();
        assert!((vel - 5.0 / 9.0).abs() < 0.01); // ~0.55

        let steps = parse_pattern_steps("9", 4.0, 0.0).unwrap();
        let vel = *steps[0].params.get("amp").unwrap();
        assert!((vel - 1.0).abs() < 0.01); // 1.0
    }

    #[test]
    fn test_parse_pattern_steps_rest_markers() {
        // '.', '_', '0', '-' are all rests
        let steps = parse_pattern_steps("._0-", 4.0, 0.0).unwrap();
        assert_eq!(steps.len(), 0);
    }

    #[test]
    fn test_parse_pattern_steps_invalid_char() {
        let err = parse_pattern_steps("x.Z.", 4.0, 0.0);
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("invalid step character 'Z'"));
        assert!(msg.contains("position 2"));
    }

    #[test]
    fn test_parse_pattern_steps_two_bars() {
        let steps = parse_pattern_steps("x...|..x.", 8.0, 0.0).unwrap();
        assert_eq!(steps.len(), 2);
        assert!((steps[0].beat.to_f64() - 0.0).abs() < 0.001);
        assert!((steps[1].beat.to_f64() - 6.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_pattern_steps_with_swing() {
        let steps = parse_pattern_steps("xx", 4.0, 0.5).unwrap();
        assert_eq!(steps.len(), 2);
        // First beat should be unswung
        assert!((steps[0].beat.to_f64() - 0.0).abs() < 0.001);
        // Second beat should be swung (delayed)
        assert!(steps[1].beat.to_f64() > 2.0);
    }

    #[test]
    fn test_parse_pattern_steps_whitespace_ignored() {
        let steps = parse_pattern_steps("x . x .", 4.0, 0.0).unwrap();
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
