//! Sequence, Fade, and Fx API for Rhai scripts.
//!
//! Sequences arrange patterns, melodies, fades, and other sequences on a timeline.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use vibelang_core::handlers::ParamRouteTarget;
use vibelang_core::reload::EffectConfig;
use vibelang_core::traits::{Clip, FadeConfig, FadeCurve, FadeTarget, SequenceConfig};
use vibelang_core::types::{Beat, Duration};

use super::melody::{Melody, MelodyBuilder, MelodyRef};
use super::pattern::{Pattern, PatternBuilder, PatternRef};
use super::route::ParamHandle;
use super::voice::{Voice, VoiceRef};
use crate::context;

// Global registry for sequences - allows looking up sequences by name
thread_local! {
    static SEQUENCE_REGISTRY: RefCell<HashMap<String, Sequence>> = RefCell::new(HashMap::new());
}

/// Clear the sequence registry (called when context is cleared).
pub fn clear_registry() {
    SEQUENCE_REGISTRY.with(|r| r.borrow_mut().clear());
}

/// Get a sequence from the registry by name.
fn get_sequence(name: &str) -> Option<Sequence> {
    SEQUENCE_REGISTRY.with(|r| r.borrow().get(name).cloned())
}

/// Store a sequence in the registry.
fn store_sequence(sequence: &Sequence) {
    SEQUENCE_REGISTRY.with(|r| {
        r.borrow_mut()
            .insert(sequence.name.clone(), sequence.clone());
    });
}

/// A Sequence builder for creating timeline arrangements.
#[derive(Debug, Clone, CustomType)]
pub struct Sequence {
    /// Sequence name.
    pub name: String,
    /// Loop length in beats.
    loop_beats: f64,
    /// Clips in the sequence.
    clips: Vec<ClipInfo>,
    /// Group path.
    group_path: String,
}

#[derive(Debug, Clone)]
enum ClipInfo {
    Pattern { name: String, start: f64, end: f64 },
    Melody { name: String, start: f64, end: f64 },
    Fade { fade: Fade, start: f64 },
    Sequence { name: String, start: f64 },
}

impl Sequence {
    /// Create a new sequence with the given name.
    pub fn new(_ctx: NativeCallContext, name: String) -> Self {
        Self {
            name,
            loop_beats: 16.0,
            clips: Vec::new(),
            group_path: context::current_group_path(),
        }
    }

    /// Set the loop length in bars.
    pub fn loop_bars(mut self, bars: f64) -> Self {
        self.loop_beats = bars * 4.0;
        self
    }

    /// Set the loop length in bars (integer version).
    pub fn loop_bars_int(mut self, bars: i64) -> Self {
        self.loop_beats = bars as f64 * 4.0;
        self
    }

    /// Set the loop length in beats.
    pub fn loop_beats_fn(mut self, beats: f64) -> Self {
        self.loop_beats = beats;
        self
    }

    /// Add a clip from a Pattern.
    pub fn clip_pattern(mut self, range: Range<f64>, pattern: Pattern) -> Self {
        self.clips.push(ClipInfo::Pattern {
            name: pattern.name.clone(),
            start: range.start,
            end: range.end,
        });
        self
    }

    /// Add a clip from a Melody.
    pub fn clip_melody(mut self, range: Range<f64>, melody: Melody) -> Self {
        // Ensure the melody is synced to state so it's available at runtime
        melody.sync_to_state();
        self.clips.push(ClipInfo::Melody {
            name: melody.name.clone(),
            start: range.start,
            end: range.end,
        });
        self
    }

    /// Add a clip from a Fade.
    pub fn clip_fade(mut self, range: Range<f64>, fade: Fade) -> Self {
        self.clips.push(ClipInfo::Fade {
            fade,
            start: range.start,
        });
        self
    }

    /// Add a clip from another Sequence.
    pub fn clip_sequence(mut self, range: Range<f64>, seq: Sequence) -> Self {
        self.clips.push(ClipInfo::Sequence {
            name: seq.name.clone(),
            start: range.start,
        });
        self
    }

    /// Add a clip using dynamic dispatch (for Range<i64> from Rhai).
    pub fn clip_dynamic(mut self, range: rhai::Dynamic, source: rhai::Dynamic) -> Self {
        let (start, end) = if let Some(r) = range.clone().try_cast::<Range<i64>>() {
            (r.start as f64, r.end as f64)
        } else {
            tracing::debug!("clip_dynamic: range type mismatch, expected Range<i64>");
            return self;
        };

        if let Some(p) = source.clone().try_cast::<Pattern>() {
            self.clips.push(ClipInfo::Pattern {
                name: p.name.clone(),
                start,
                end,
            });
        } else if let Some(m) = source.clone().try_cast::<Melody>() {
            // Ensure the melody is synced to state so it's available at runtime
            m.sync_to_state();
            self.clips.push(ClipInfo::Melody {
                name: m.name.clone(),
                start,
                end,
            });
        } else if let Some(f) = source.clone().try_cast::<Fade>() {
            self.clips.push(ClipInfo::Fade { fade: f, start });
        } else if let Some(s) = source.clone().try_cast::<Sequence>() {
            self.clips.push(ClipInfo::Sequence {
                name: s.name.clone(),
                start,
            });
        }

        self
    }

    /// Sync sequence to script state.
    fn sync_to_state(&self) {
        let sequence_id = context::get_or_create_sequence_id(&self.name);

        // Convert clips to Clip enum
        let clips: Vec<Clip> = self
            .clips
            .iter()
            .map(|c| match c {
                ClipInfo::Pattern { name, start, end } => {
                    let pattern_id = context::get_or_create_pattern_id(name);
                    Clip::Pattern {
                        id: pattern_id,
                        start: Beat::from_f64(*start),
                        end: Beat::from_f64(*end),
                    }
                }
                ClipInfo::Melody { name, start, end } => {
                    let melody_id = context::get_or_create_melody_id(name);
                    Clip::Melody {
                        id: melody_id,
                        start: Beat::from_f64(*start),
                        end: Beat::from_f64(*end),
                    }
                }
                ClipInfo::Fade { fade, start } => {
                    // Convert Fade to FadeConfig based on target type
                    use vibelang_core::traits::FadeTarget;
                    use vibelang_core::types::Duration;

                    let target = match fade.target_type {
                        FadeTargetType::Group => {
                            FadeTarget::Group(context::get_or_create_group_id(&fade.target_name))
                        }
                        FadeTargetType::Voice => {
                            FadeTarget::Voice(context::get_or_create_voice_id(&fade.target_name))
                        }
                        FadeTargetType::Effect => {
                            FadeTarget::Effect(context::get_or_create_effect_id(&fade.target_name))
                        }
                    };

                    let mut config = FadeConfig::new(
                        target,
                        &fade.param_name,
                        fade.to_value as f32,
                        Duration::from_beats(fade.duration_beats),
                    );
                    config.from = Some(fade.from_value as f32);
                    config.curve = fade.curve.clone();

                    Clip::Fade {
                        config,
                        start: Beat::from_f64(*start),
                    }
                }
                ClipInfo::Sequence { name, start } => {
                    let seq_id = context::get_or_create_sequence_id(name);
                    Clip::Sequence {
                        id: seq_id,
                        start: Beat::from_f64(*start),
                    }
                }
            })
            .collect();

        let config = SequenceConfig {
            name: self.name.clone(),
            length: Beat::from_f64(self.loop_beats),
            clips,
        };

        context::with_state(|state| {
            state.sequences.insert(sequence_id, config);
        });
    }

    /// Register and apply the sequence.
    pub fn apply(self) -> Self {
        self.sync_to_state();
        // Store in registry for later lookup
        store_sequence(&self);
        self
    }

    /// Start the sequence playing.
    pub fn start(&mut self) {
        self.sync_to_state();

        let sequence_id = context::get_or_create_sequence_id(&self.name);
        context::with_state(|state| {
            state.playing_sequences.insert(sequence_id);
        });
    }

    /// Launch the sequence with quantization.
    ///
    /// Deprecated: use `start()` instead.
    #[deprecated(note = "use start() instead")]
    pub fn launch(&mut self) {
        self.start()
    }

    /// Stop the sequence.
    pub fn stop(&mut self) {
        let sequence_id = context::get_or_create_sequence_id(&self.name);
        context::with_state(|state| {
            state.playing_sequences.remove(&sequence_id);
        });
    }

    /// Check if the sequence is playing.
    pub fn is_playing(&mut self) -> bool {
        let sequence_id = context::get_or_create_sequence_id(&self.name);
        context::with_state(|state| state.playing_sequences.contains(&sequence_id))
    }
}

/// A Fade builder for creating parameter automation.
#[derive(Debug, Clone, CustomType)]
pub struct Fade {
    /// Fade name.
    pub name: String,
    /// Target type.
    target_type: FadeTargetType,
    /// Target name.
    target_name: String,
    /// Parameter name.
    param_name: String,
    /// Start value.
    from_value: f64,
    /// End value.
    to_value: f64,
    /// Duration in beats.
    duration_beats: f64,
    /// Interpolation curve.
    curve: FadeCurve,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FadeTargetType {
    Group,
    Voice,
    Effect,
}

impl Fade {
    /// Create a new fade with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            target_type: FadeTargetType::Group,
            target_name: String::new(),
            param_name: "amp".to_string(),
            from_value: 0.0,
            to_value: 1.0,
            duration_beats: 4.0,
            curve: FadeCurve::Linear,
        }
    }

    /// Create an anonymous fade (name resolved at finalization).
    pub fn new_anon() -> Self {
        Self {
            name: String::new(),
            target_type: FadeTargetType::Group,
            target_name: String::new(),
            param_name: "amp".to_string(),
            from_value: 0.0,
            to_value: 1.0,
            duration_beats: 4.0,
            curve: FadeCurve::Linear,
        }
    }

    /// Resolve the name for an anonymous fade from its target and parameter.
    ///
    /// Uses `_{target}_{param}` as the name formula.
    /// No-op if the fade already has a name.
    fn resolve_name(&mut self) {
        if !self.name.is_empty() {
            return;
        }
        let base = if self.target_name.is_empty() {
            format!("_fade_{}", self.param_name)
        } else {
            format!("_{}_{}", self.target_name, self.param_name)
        };
        self.name = context::resolve_auto_name(&base);
    }

    /// Target a group.
    pub fn on_group(mut self, group_name: String) -> Self {
        self.target_type = FadeTargetType::Group;
        self.target_name = group_name;
        self
    }

    /// Target a voice by name.
    pub fn on_voice(mut self, voice_name: String) -> Self {
        self.target_type = FadeTargetType::Voice;
        self.target_name = voice_name;
        self
    }

    /// Target a voice by handle.
    pub fn on_voice_handle(mut self, mut voice: Voice) -> Self {
        voice.resolve_name();
        self.target_type = FadeTargetType::Voice;
        self.target_name = voice.name.clone();
        self
    }

    /// Target an effect by name.
    pub fn on_effect(mut self, effect_name: String) -> Self {
        self.target_type = FadeTargetType::Effect;
        self.target_name = effect_name;
        self
    }

    /// Target an effect by handle.
    pub fn on_effect_handle(mut self, effect: Fx) -> Self {
        self.target_type = FadeTargetType::Effect;
        self.target_name = effect.id.clone();
        self
    }

    /// Generic target method that accepts various handle types.
    ///
    /// Accepts Voice, Fx, or falls back to treating Dynamic as a string (group name).
    pub fn on_dynamic(mut self, target: rhai::Dynamic) -> Self {
        if let Some(mut voice) = target.clone().try_cast::<Voice>() {
            voice.resolve_name();
            self.target_type = FadeTargetType::Voice;
            self.target_name = voice.name.clone();
        } else if let Some(fx) = target.clone().try_cast::<Fx>() {
            self.target_type = FadeTargetType::Effect;
            self.target_name = fx.id.clone();
        } else if let Some(name) = target.clone().try_cast::<String>() {
            // Default to group for string names
            self.target_type = FadeTargetType::Group;
            self.target_name = name;
        }
        self
    }

    /// Set the parameter to fade.
    pub fn param(mut self, param_name: String) -> Self {
        self.param_name = param_name;
        self
    }

    /// Set the start value.
    pub fn from(mut self, value: f64) -> Self {
        self.from_value = value;
        self
    }

    /// Set the end value.
    pub fn to(mut self, value: f64) -> Self {
        self.to_value = value;
        self
    }

    /// Set duration in beats.
    pub fn over(mut self, beats: f64) -> Self {
        self.duration_beats = beats.max(0.0625); // minimum 1/64th note
        self
    }

    /// Set duration in bars. Minimum 1/64th note.
    pub fn over_bars(mut self, bars: i64) -> Self {
        self.duration_beats = (bars as f64 * 4.0).max(0.0625);
        self
    }

    /// Set the interpolation curve.
    ///
    /// Available curves:
    /// - "linear" (default): Constant rate of change
    /// - "ease_in", "easein": Slow start, fast end (t²)
    /// - "ease_out", "easeout": Fast start, slow end
    /// - "ease_in_out", "easeinout": Smooth S-curve
    /// - "sine_in", "sinein": Slow start using sine
    /// - "sine_out", "sineout": Slow end using sine
    /// - "sine", "sine_in_out": Smooth start and end
    /// - "cubic_in", "cubicin": Very slow start (t³)
    /// - "cubic_out", "cubicout": Very slow end
    /// - "cubic_in_out", "cubicinout": Smooth cubic S-curve
    /// - "exponential", "exp": Configurable exponent (default 2.0)
    /// - "log", "logarithmic": Fast start, slow end
    /// - "step", "instant": Jump at end
    pub fn curve(mut self, curve_name: String) -> Self {
        self.curve = match curve_name.to_lowercase().as_str() {
            // Ease (quadratic)
            "ease_in" | "easein" | "ease-in" => FadeCurve::EaseIn,
            "ease_out" | "easeout" | "ease-out" => FadeCurve::EaseOut,
            "ease_in_out" | "easeinout" | "ease-in-out" | "ease" => FadeCurve::EaseInOut,

            // Sine
            "sine_in" | "sinein" | "sine-in" => FadeCurve::SineIn,
            "sine_out" | "sineout" | "sine-out" => FadeCurve::SineOut,
            "sine" | "sine_in_out" | "sineinout" | "sin" | "smooth" => FadeCurve::SineInOut,

            // Cubic
            "cubic_in" | "cubicin" | "cubic-in" => FadeCurve::CubicIn,
            "cubic_out" | "cubicout" | "cubic-out" => FadeCurve::CubicOut,
            "cubic_in_out" | "cubicinout" | "cubic-in-out" | "cubic" => FadeCurve::CubicInOut,

            // Exponential (default exponent 2.0)
            "exponential" | "exp" => FadeCurve::Exponential { exponent: 2.0 },

            // Logarithmic
            "log" | "logarithmic" => FadeCurve::Logarithmic,

            // Step
            "step" | "instant" => FadeCurve::Step,

            // Default to linear
            _ => FadeCurve::Linear,
        };
        self
    }

    /// Set an exponential curve with custom exponent.
    ///
    /// # Arguments
    /// * `exponent` - The exponent for the curve (t^exponent).
    ///   - 1.0 = linear
    ///   - 2.0 = quadratic (default)
    ///   - 3.0 = cubic
    ///   - Higher = slower start, faster end
    pub fn exp(mut self, exponent: f64) -> Self {
        self.curve = FadeCurve::Exponential {
            exponent: exponent as f32,
        };
        self
    }

    /// Set a cubic spline curve with control points.
    ///
    /// Points are provided as a flat array: [t1, v1, t2, v2, ...]
    /// where t is time (0-1) and v is value (0-1).
    /// Start (0,0) and end (1,1) are added automatically.
    ///
    /// # Example
    /// ```rhai
    /// fade("swell").spline([0.25, 0.1, 0.5, 0.9, 0.75, 0.3])
    /// ```
    pub fn spline(mut self, points: rhai::Array) -> Self {
        let parsed: Vec<(f32, f32)> = points
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    let t = chunk[0].clone().try_cast::<f64>().map(|v| v as f32)?;
                    let v = chunk[1].clone().try_cast::<f64>().map(|v| v as f32)?;
                    Some((t, v))
                } else {
                    None
                }
            })
            .collect();
        self.curve = FadeCurve::CubicSpline { points: parsed };
        self
    }

    /// Add a control point for a spline curve.
    ///
    /// If the current curve is not a spline, this creates a new spline curve.
    /// Points are (time, value) pairs normalized 0-1.
    ///
    /// # Example
    /// ```rhai
    /// fade("complex")
    ///     .point(0.2, 0.8)   // quick rise
    ///     .point(0.4, 0.2)   // dip
    ///     .point(0.6, 0.9)   // rise again
    ///     .point(0.8, 0.5)   // settle
    /// ```
    pub fn point(mut self, time: f64, value: f64) -> Self {
        match &mut self.curve {
            FadeCurve::CubicSpline { points } => {
                points.push((time as f32, value as f32));
            }
            _ => {
                self.curve = FadeCurve::CubicSpline {
                    points: vec![(time as f32, value as f32)],
                };
            }
        }
        self
    }

    /// Apply the fade definition (chainable).
    pub fn apply(self) -> Self {
        // Store the fade in the script state for later use in sequences
        // Fades are applied through sequences, not directly
        self
    }

    /// Convert this Fade to a FadeConfig.
    fn to_config(&self) -> FadeConfig {
        let target = match self.target_type {
            FadeTargetType::Group => {
                FadeTarget::Group(context::get_or_create_group_id(&self.target_name))
            }
            FadeTargetType::Voice => {
                FadeTarget::Voice(context::get_or_create_voice_id(&self.target_name))
            }
            FadeTargetType::Effect => {
                FadeTarget::Effect(context::get_or_create_effect_id(&self.target_name))
            }
        };

        let mut config = FadeConfig::new(
            target,
            &self.param_name,
            self.to_value as f32,
            Duration::from_beats(self.duration_beats),
        );
        config.from = Some(self.from_value as f32);
        config.curve = self.curve.clone();
        config
    }

    /// Start the fade with quantization (chainable).
    ///
    /// Registers the fade as a stateful entity in the script state.
    /// On reload, unchanged fades are not re-fired — only new or modified
    /// fades are started. This prevents fades from resetting on every save.
    ///
    /// For anonymous fades, resolves the name from target + param before registering.
    pub fn start(mut self) -> Self {
        self.resolve_name();
        let config = self.to_config();
        let fade_id = context::get_or_create_fade_id(&self.name);
        context::with_state(|state| {
            state.fades.insert(fade_id, config);
            state.playing_fades.insert(fade_id);
        });
        self
    }

    /// Launch the fade with quantization (alias for start).
    ///
    /// This is an alias for `start()` to match the pattern/melody API.
    pub fn launch(self) -> Self {
        self.start()
    }

    /// Restart the fade (force re-fire even if config is unchanged).
    ///
    /// Useful for re-firing a fade that has already completed or is still running.
    /// Unlike `start()` and `now()`, this will always re-trigger the fade on reload,
    /// even if the configuration hasn't changed.
    pub fn restart(mut self) -> Self {
        self.resolve_name();
        let config = self.to_config();
        let fade_id = context::get_or_create_fade_id(&self.name);
        context::with_state(|state| {
            state.fades.insert(fade_id, config);
            state.playing_fades.insert(fade_id);
            state.force_restart_fades.insert(fade_id);
        });
        self
    }

    /// Start the fade immediately without quantization (chainable).
    ///
    /// Registers the fade as a stateful entity. On reload, unchanged fades
    /// are not re-fired — only new or modified fades are started.
    pub fn now(mut self) -> Self {
        self.resolve_name();
        let config = self.to_config();
        let fade_id = context::get_or_create_fade_id(&self.name);
        context::with_state(|state| {
            state.fades.insert(fade_id, config);
            state.playing_fades.insert(fade_id);
        });
        self
    }
}

/// An Fx builder for creating audio effects.
#[derive(Debug, Clone, CustomType)]
pub struct Fx {
    /// Effect ID.
    pub id: String,
    /// Synthdef name.
    synth_name: Option<String>,
    /// Parameters.
    params: HashMap<String, f32>,
    /// Group path.
    group_path: String,
}

impl Fx {
    /// Create a new effect with the given ID.
    pub fn new(_ctx: NativeCallContext, id: String) -> Self {
        Self {
            id,
            synth_name: None,
            params: HashMap::new(),
            group_path: context::current_group_path(),
        }
    }

    /// Set the synthdef for this effect.
    pub fn synth(mut self, synth_name: String) -> Self {
        self.synth_name = Some(synth_name);
        self
    }

    /// Read the resolved synthdef name. Empty string when no `.synth(...)`
    /// has been called yet (the route layer surfaces a clean
    /// "no param '<x>'" error in that case rather than panicking).
    pub(crate) fn synth_name(&self) -> String {
        self.synth_name.clone().unwrap_or_default()
    }

    /// Set a parameter to an initial value (two-arg overload).
    pub fn param(mut self, key: String, value: f64) -> Self {
        self.params.insert(key, value as f32);
        self
    }

    /// Begin a target-first CV-to-param wiring on this fx's named param
    /// (one-arg overload — Rhai dispatches by arity).
    ///
    /// Dual to [`crate::api::route::RouteHandle::to_param`] for fx targets:
    /// `fx.param("pan").modulate_by(source, "out")` records the same
    /// `(source_voice, source_port) → (Effect(fx), param)` entry in
    /// [`vibelang_core::reload::ScriptState::param_routes_bend`] that the
    /// source-first form does, so either reading installs an identical
    /// registry row.
    ///
    /// Infallible — the param-name and source-port-rate validation runs in
    /// [`ParamHandle::modulate_by`] where both sides are known. Bare
    /// construction here just resolves the effect's id and snapshots the
    /// synthdef name for the rate-validation step later.
    pub fn param_handle(&mut self, name: &str) -> ParamHandle {
        let effect_id = context::get_or_create_effect_id(&self.id);
        let synth = self.synth_name();
        ParamHandle::new(
            ParamRouteTarget::Effect(effect_id),
            self.id.clone(),
            synth,
            name.to_string(),
        )
    }

    /// Apply the effect to the current group.
    ///
    /// Returns self so the effect handle can be used with fade operations.
    pub fn apply(self) -> Self {
        let effect_id = context::get_or_create_effect_id(&self.id);
        let group_id = context::get_or_create_group_id(&self.group_path);

        let config = EffectConfig {
            synthdef: self.synth_name.clone().unwrap_or_default(),
            group: group_id,
            params: self.params.clone(),
        };

        context::with_state(|state| {
            state.add_effect(effect_id, config);

            // Also add to the group's effects list
            if let Some(group_config) = state.groups.get_mut(&group_id) {
                group_config.effects.push(effect_id);
            }
        });

        self
    }
}

/// Create a new sequence builder or return an existing one.
///
/// If a sequence with this name already exists in the registry,
/// returns a clone of it. Otherwise creates a new empty sequence.
pub fn sequence(ctx: NativeCallContext, name: String) -> Sequence {
    // Check if sequence already exists in registry
    if let Some(existing) = get_sequence(&name) {
        return existing;
    }
    // Create new sequence
    Sequence::new(ctx, name)
}

/// Create a new fade builder.
pub fn fade(name: String) -> Fade {
    Fade::new(name)
}

/// Create an anonymous fade builder (name resolved from target + param).
pub fn fade_anon() -> Fade {
    Fade::new_anon()
}

/// Create a new fx builder.
pub fn fx(ctx: NativeCallContext, id: String) -> Fx {
    Fx::new(ctx, id)
}

/// Register sequence, fade, and fx API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register types
    engine.build_type::<Sequence>();
    engine.build_type::<Fade>();
    engine.build_type::<Fx>();

    // Constructors (named and anonymous overloads)
    engine.register_fn("sequence", sequence);
    engine.register_fn("fade", fade);
    engine.register_fn("fade", fade_anon);
    engine.register_fn("fx", fx);

    // Sequence builder methods
    engine.register_fn("loop_bars", Sequence::loop_bars);
    engine.register_fn("loop_bars", Sequence::loop_bars_int);
    engine.register_fn("loop_beats", Sequence::loop_beats_fn);
    engine.register_fn("clip", Sequence::clip_dynamic);
    engine.register_fn("clip", Sequence::clip_pattern);
    engine.register_fn("clip", Sequence::clip_melody);
    engine.register_fn("clip", Sequence::clip_fade);
    engine.register_fn("clip", Sequence::clip_sequence);

    // Sequence actions
    engine.register_fn("apply", Sequence::apply);
    engine.register_fn("start", Sequence::start);
    #[allow(deprecated)]
    engine.register_fn("launch", Sequence::launch);
    engine.register_fn("stop", Sequence::stop);
    engine.register_fn("is_playing", Sequence::is_playing);
    engine.register_get("playing", Sequence::is_playing);
    engine.register_get("name", |s: &mut Sequence| s.name.clone());

    // Fade builder methods
    engine.register_fn("on_group", Fade::on_group);
    engine.register_fn("on_voice", Fade::on_voice);
    engine.register_fn("on_voice", Fade::on_voice_handle); // Overload for Voice handle
    engine.register_fn("on_effect", Fade::on_effect);
    engine.register_fn("on_effect", Fade::on_effect_handle); // Overload for Fx handle
    engine.register_fn("on", Fade::on_dynamic); // Generic target method
    engine.register_fn("param", Fade::param);
    engine.register_fn("from", Fade::from);
    engine.register_fn("to", Fade::to);
    engine.register_fn("over", Fade::over);
    engine.register_fn("over_bars", Fade::over_bars);
    engine.register_fn("curve", Fade::curve);
    engine.register_fn("exp", Fade::exp);
    engine.register_fn("spline", Fade::spline);
    engine.register_fn("point", Fade::point);
    engine.register_fn("apply", Fade::apply);
    engine.register_fn("start", Fade::start);
    engine.register_fn("launch", Fade::launch);
    engine.register_fn("now", Fade::now);
    engine.register_fn("restart", Fade::restart);

    // Fx builder methods
    engine.register_fn("synth", Fx::synth);
    engine.register_fn("param", Fx::param);
    // 1-arg overload of `.param`: returns a ParamHandle for the named fx
    // param, used to install fx-target modulation routes via
    // `fx.param("name").modulate_by(source, "port")`.
    engine.register_fn("param", Fx::param_handle);
    engine.register_fn("apply", Fx::apply);
}

use vibelang_core::candidate::{
    AuthoringDeclaration, Cancellation, CandidateError, CandidateFragment, CanonicalF64,
    Composition, DeclarationOwner, DeclarationPayload, DesiredLifecycle, DspDefinitionAuthoring,
    EffectAuthoring, EffectDefKind, EffectKind, FadeAuthoring, FadeCurveAuthoring, FadeKind,
    FadePointAuthoring, FadeTargetAuthoring, GroupScope, LifecycleAction, LifecycleMetadata,
    SequenceAuthoring, SequenceClipAuthoring, SequenceContentAuthoring, SequenceKind, StartMode,
    SynthDefKind, TerminalEffect,
};

use super::group::GroupRef;
use crate::foundation::{self, BuilderBase, FoundationError, Observation, RefBase};

fn dsp_definition_error(error: impl std::fmt::Display) -> FoundationError {
    CandidateError::InvalidDspDefinition(error.to_string()).into()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FadeRef {
    base: RefBase,
}

impl FadeRef {
    fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<FadeKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    fn action(self, action: LifecycleAction, role: &str) -> Result<Self, FoundationError> {
        let (effect, cancellation) = match &action {
            LifecycleAction::Start(_) | LifecycleAction::Restart => {
                (TerminalEffect::Start, Cancellation::BeforePlanning)
            }
            LifecycleAction::Stop => (TerminalEffect::Stop, Cancellation::NotCancellable),
            LifecycleAction::Remove => (TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::Cancel => (TerminalEffect::Cancel, Cancellation::BeforePlanning),
            _ => {
                return Err(CandidateError::InvalidLifecycle(
                    "unsupported FadeRef lifecycle action".into(),
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

    pub fn launch(self) -> Result<Self, FoundationError> {
        self.start()
    }

    pub fn now(self) -> Result<Self, FoundationError> {
        self.start_now()
    }

    pub fn restart(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Restart, "restart")
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
enum FadeTargetV2 {
    Group(GroupRef),
    Voice(VoiceRef),
    Effect(EffectRef),
    Pattern(PatternRef),
    Melody(MelodyRef),
}

impl FadeTargetV2 {
    fn base(&self) -> &RefBase {
        match self {
            Self::Group(reference) => reference.base(),
            Self::Voice(reference) => reference.base(),
            Self::Effect(reference) => reference.base(),
            Self::Pattern(reference) => reference.base(),
            Self::Melody(reference) => reference.base(),
        }
    }

    fn authoring(&self) -> Result<FadeTargetAuthoring, FoundationError> {
        Ok(match self {
            Self::Group(reference) => FadeTargetAuthoring::Group(reference.base().typed()?),
            Self::Voice(reference) => FadeTargetAuthoring::Voice(reference.base().typed()?),
            Self::Effect(reference) => FadeTargetAuthoring::Effect(reference.base().typed()?),
            Self::Pattern(reference) => FadeTargetAuthoring::Pattern(reference.base().typed()?),
            Self::Melody(reference) => FadeTargetAuthoring::Melody(reference.base().typed()?),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FadeBuilder {
    base: BuilderBase,
    target: Option<FadeTargetV2>,
    parameter: String,
    from: f64,
    to: f64,
    duration_beats: f64,
    curve: FadeCurveAuthoring,
}

impl FadeBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        Self {
            base,
            target: None,
            parameter: "amp".into(),
            from: 0.0,
            to: 1.0,
            duration_beats: 4.0,
            curve: FadeCurveAuthoring::Linear,
        }
    }

    fn target(mut self, target: FadeTargetV2) -> Result<Self, FoundationError> {
        if self.target.is_some() {
            return Err(CandidateError::InvalidAuthoring(
                "Fade target is already configured; alternative target forms cannot be composed"
                    .into(),
            )
            .into());
        }
        target.base().validate(self.base.identity())?;
        self.target = Some(target);
        Ok(self)
    }

    pub fn on_group(self, target: GroupRef) -> Result<Self, FoundationError> {
        self.target(FadeTargetV2::Group(target))
    }

    pub fn on_voice(self, target: VoiceRef) -> Result<Self, FoundationError> {
        self.target(FadeTargetV2::Voice(target))
    }

    pub fn on_effect(self, target: EffectRef) -> Result<Self, FoundationError> {
        self.target(FadeTargetV2::Effect(target))
    }

    pub fn on_pattern(self, target: PatternRef) -> Result<Self, FoundationError> {
        self.target(FadeTargetV2::Pattern(target))
    }

    pub fn on_melody(self, target: MelodyRef) -> Result<Self, FoundationError> {
        self.target(FadeTargetV2::Melody(target))
    }

    pub fn param(mut self, parameter: String) -> Result<Self, FoundationError> {
        if parameter.is_empty() {
            return Err(
                CandidateError::InvalidAuthoring("Fade parameter cannot be empty".into()).into(),
            );
        }
        self.parameter = parameter;
        Ok(self)
    }

    #[must_use]
    pub fn from(mut self, value: f64) -> Self {
        self.from = value;
        self
    }

    #[must_use]
    pub fn to(mut self, value: f64) -> Self {
        self.to = value;
        self
    }

    pub fn over(mut self, beats: f64) -> Result<Self, FoundationError> {
        if !beats.is_finite() || beats <= 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Fade duration must be finite and positive".into(),
            )
            .into());
        }
        self.duration_beats = beats;
        Ok(self)
    }

    pub fn over_bars(self, bars: f64) -> Result<Self, FoundationError> {
        self.over(bars * 4.0)
    }

    pub fn curve(mut self, name: String) -> Result<Self, FoundationError> {
        self.curve = match name.to_ascii_lowercase().as_str() {
            "linear" => FadeCurveAuthoring::Linear,
            "ease_in" | "easein" | "ease-in" => FadeCurveAuthoring::EaseIn,
            "ease_out" | "easeout" | "ease-out" => FadeCurveAuthoring::EaseOut,
            "ease_in_out" | "easeinout" | "ease-in-out" | "ease" => FadeCurveAuthoring::EaseInOut,
            "sine_in" | "sinein" | "sine-in" => FadeCurveAuthoring::SineIn,
            "sine_out" | "sineout" | "sine-out" => FadeCurveAuthoring::SineOut,
            "sine" | "sine_in_out" | "sineinout" | "sin" | "smooth" => {
                FadeCurveAuthoring::SineInOut
            }
            "cubic_in" | "cubicin" | "cubic-in" => FadeCurveAuthoring::CubicIn,
            "cubic_out" | "cubicout" | "cubic-out" => FadeCurveAuthoring::CubicOut,
            "cubic_in_out" | "cubicinout" | "cubic-in-out" | "cubic" => {
                FadeCurveAuthoring::CubicInOut
            }
            "exponential" | "exp" => FadeCurveAuthoring::Exponential(CanonicalF64::new(2.0)?),
            "log" | "logarithmic" => FadeCurveAuthoring::Logarithmic,
            "step" | "instant" => FadeCurveAuthoring::Step,
            _ => {
                return Err(
                    CandidateError::InvalidAuthoring(format!("unknown Fade curve: {name}")).into(),
                )
            }
        };
        Ok(self)
    }

    pub fn exp(mut self, exponent: f64) -> Result<Self, FoundationError> {
        if !exponent.is_finite() || exponent <= 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Fade exponent must be finite and positive".into(),
            )
            .into());
        }
        self.curve = FadeCurveAuthoring::Exponential(CanonicalF64::new(exponent)?);
        Ok(self)
    }

    pub fn spline(mut self, points: rhai::Array) -> Result<Self, FoundationError> {
        if points.len() % 2 != 0 {
            return Err(CandidateError::InvalidAuthoring(
                "Fade spline requires complete time/value pairs".into(),
            )
            .into());
        }
        let mut parsed = Vec::with_capacity(points.len() / 2);
        for pair in points.chunks_exact(2) {
            let time = pair[0].as_float().map_err(|_| {
                CandidateError::InvalidAuthoring("Fade spline time must be numeric".into())
            })?;
            let value = pair[1].as_float().map_err(|_| {
                CandidateError::InvalidAuthoring("Fade spline value must be numeric".into())
            })?;
            if !time.is_finite()
                || !value.is_finite()
                || !(0.0..1.0).contains(&time)
                || parsed
                    .last()
                    .is_some_and(|previous: &FadePointAuthoring| previous.time.get() >= time)
            {
                return Err(CandidateError::InvalidAuthoring(
                    "Fade spline needs finite values and strictly increasing times within 0.0..1.0"
                        .into(),
                )
                .into());
            }
            parsed.push(FadePointAuthoring {
                time: CanonicalF64::new(time)?,
                value: CanonicalF64::new(value)?,
            });
        }
        self.curve = FadeCurveAuthoring::CubicSpline(parsed);
        Ok(self)
    }

    pub fn point(mut self, time: f64, value: f64) -> Result<Self, FoundationError> {
        if !time.is_finite() || !value.is_finite() || !(0.0..1.0).contains(&time) {
            return Err(CandidateError::InvalidAuthoring(
                "Fade spline points need finite values and time within 0.0..1.0".into(),
            )
            .into());
        }
        let point = FadePointAuthoring {
            time: CanonicalF64::new(time)?,
            value: CanonicalF64::new(value)?,
        };
        match &mut self.curve {
            FadeCurveAuthoring::CubicSpline(points) => {
                if points
                    .last()
                    .is_some_and(|previous| previous.time.get() >= time)
                {
                    return Err(CandidateError::InvalidAuthoring(
                        "Fade spline point times must be strictly increasing".into(),
                    )
                    .into());
                }
                points.push(point);
            }
            FadeCurveAuthoring::Linear => {
                self.curve = FadeCurveAuthoring::CubicSpline(vec![point]);
            }
            _ => {
                return Err(CandidateError::InvalidAuthoring(
                    "Fade point composition cannot replace another configured curve".into(),
                )
                .into())
            }
        }
        Ok(self)
    }

    pub(crate) fn build_fragment(
        self,
        lifecycle: DesiredLifecycle,
    ) -> Result<(CandidateFragment, FadeRef), FoundationError> {
        let target = self.target.ok_or_else(|| {
            CandidateError::InvalidAuthoring("Fade requires one typed target before apply".into())
        })?;
        let target_base = target.base().clone();
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::Fade(FadeAuthoring {
            target: target.authoring()?,
            parameter: self.parameter,
            from: CanonicalF64::new(self.from)?,
            to: CanonicalF64::new(self.to)?,
            duration_ticks: Beat::from_f64(self.duration_beats).raw(),
            curve: self.curve,
            lifecycle,
        }))?;
        let metadata = match lifecycle {
            DesiredLifecycle::Dormant => LifecycleMetadata::register(Composition::Standalone),
            DesiredLifecycle::Start(_) => LifecycleMetadata::start(Composition::Standalone),
        };
        let source = self.base.source().clone();
        let owner = DeclarationOwner::Structural(source.syntax_key().clone());
        let (fragment, reference) =
            self.base
                .fragment(owner, metadata, payload, [(target_base, source)])?;
        Ok((fragment, FadeRef::new(reference)?))
    }

    fn terminal(self, lifecycle: DesiredLifecycle) -> Result<FadeRef, FoundationError> {
        let (fragment, reference) = self.build_fragment(lifecycle)?;
        foundation::commit_fragment(fragment)?;
        Ok(reference)
    }

    pub fn apply(self) -> Result<FadeRef, FoundationError> {
        self.terminal(DesiredLifecycle::Dormant)
    }

    pub fn start(self) -> Result<FadeRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Normal))
    }

    pub fn start_now(self) -> Result<FadeRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Immediate))
    }

    pub fn launch(self) -> Result<FadeRef, FoundationError> {
        self.start()
    }

    pub fn now(self) -> Result<FadeRef, FoundationError> {
        self.start_now()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectRef {
    base: RefBase,
}

impl EffectRef {
    fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<EffectKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        let source = foundation::operation_source(&self.base, "remove")?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::Remove,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectDefRef {
    base: RefBase,
}

impl EffectDefRef {
    fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<EffectDefKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        let source = foundation::operation_source(&self.base, "remove")?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::Remove,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynthDefRef {
    base: RefBase,
}

impl SynthDefRef {
    fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<SynthDefKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        let source = foundation::operation_source(&self.base, "remove")?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::Remove,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectBuilder {
    base: BuilderBase,
    definition: Option<EffectDefRef>,
    params: BTreeMap<String, f64>,
}

impl EffectBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        Self {
            base,
            definition: None,
            params: BTreeMap::new(),
        }
    }

    pub fn definition(mut self, definition: EffectDefRef) -> Result<Self, FoundationError> {
        if self.definition.is_some() {
            return Err(CandidateError::InvalidAuthoring(
                "Effect definition is already configured; alternative definition forms cannot be composed"
                    .into(),
            )
            .into());
        }
        definition.base().validate(self.base.identity())?;
        self.definition = Some(definition);
        Ok(self)
    }

    pub fn synth(self, name: String) -> Result<Self, FoundationError> {
        let reference = EffectDefRef::new(foundation::authoring_ref::<EffectDefKind>(
            &name,
            GroupScope::root(),
        )?)?;
        self.definition(reference)
    }

    pub fn param(mut self, name: String, value: f64) -> Result<Self, FoundationError> {
        if name.is_empty() {
            return Err(CandidateError::InvalidAuthoring(
                "Effect parameter cannot be empty".into(),
            )
            .into());
        }
        CanonicalF64::new(value)?;
        self.params.insert(name, value);
        Ok(self)
    }

    pub fn apply(self) -> Result<EffectRef, FoundationError> {
        let definition = self.definition.ok_or_else(|| {
            CandidateError::InvalidAuthoring("Effect requires one EffectDefRef before apply".into())
        })?;
        let source = self.base.source().clone();
        let params = self
            .params
            .into_iter()
            .map(|(name, value)| Ok((name, CanonicalF64::new(value)?)))
            .collect::<Result<BTreeMap<_, _>, CandidateError>>()?;
        let payload =
            DeclarationPayload::authoring(AuthoringDeclaration::Effect(EffectAuthoring {
                definition: definition.base().typed()?,
                params,
            }))?;
        let definition_base = definition.base().clone();
        let owner = DeclarationOwner::Structural(source.syntax_key().clone());
        let (fragment, reference) = self.base.fragment(
            owner,
            LifecycleMetadata::register(Composition::Standalone),
            payload,
            [(definition_base, source)],
        )?;
        foundation::commit_fragment(fragment)?;
        EffectRef::new(reference)
    }
}

#[derive(Clone, Debug)]
pub struct SynthDefBuilder {
    base: BuilderBase,
    builder: Option<vibelang_dsp::SynthDefBuilderHandle>,
    definition: Option<vibelang_dsp::DspDefinitionIr>,
}

impl SynthDefBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        let qualified_name = base.address().to_string();
        Self {
            base,
            builder: Some(vibelang_dsp::SynthDefBuilderHandle::new(qualified_name)),
            definition: None,
        }
    }

    fn configure(
        mut self,
        configure: impl FnOnce(
            vibelang_dsp::SynthDefBuilderHandle,
        ) -> Result<vibelang_dsp::SynthDefBuilderHandle, FoundationError>,
    ) -> Result<Self, FoundationError> {
        let builder = self.builder.take().ok_or_else(|| {
            CandidateError::InvalidDspDefinition(
                "SynthDef configuration cannot continue after body/body_map".into(),
            )
        })?;
        self.builder = Some(configure(builder)?);
        Ok(self)
    }

    pub fn param(self, name: String, default: f64) -> Result<Self, FoundationError> {
        CanonicalF64::new(default)?;
        self.configure(|builder| Ok(builder.param(name.into(), default)))
    }

    pub fn glide_ms(self, name: String, milliseconds: f64) -> Result<Self, FoundationError> {
        if !milliseconds.is_finite() || milliseconds < 0.0 {
            return Err(CandidateError::InvalidDspDefinition(
                "SynthDef glide_ms must be finite and non-negative".into(),
            )
            .into());
        }
        self.configure(|builder| Ok(builder.glide_ms(name.into(), milliseconds)))
    }

    pub fn out_bus(self, tag: String) -> Result<Self, FoundationError> {
        self.configure(|builder| Ok(builder.out_bus(tag.into())))
    }

    pub fn input(self, name: String, channels: i64) -> Result<Self, FoundationError> {
        self.configure(|builder| {
            builder
                .input_with_channels(name.into(), channels)
                .map_err(dsp_definition_error)
        })
    }

    pub fn output(self, name: String, channels: i64) -> Result<Self, FoundationError> {
        self.configure(|builder| {
            builder
                .output_with_channels(name.into(), channels)
                .map_err(dsp_definition_error)
        })
    }

    pub fn output_kr(self, name: String, channels: i64) -> Result<Self, FoundationError> {
        self.configure(|builder| {
            builder
                .output_kr_with_channels(name.into(), channels)
                .map_err(dsp_definition_error)
        })
    }

    pub fn output_tr(self, name: String, channels: i64) -> Result<Self, FoundationError> {
        self.configure(|builder| {
            builder
                .output_tr_with_channels(name.into(), channels)
                .map_err(dsp_definition_error)
        })
    }

    pub fn outputs(self, outputs: rhai::Array) -> Result<Self, FoundationError> {
        self.configure(|builder| builder.outputs(outputs).map_err(dsp_definition_error))
    }

    pub fn body(mut self, body: rhai::FnPtr) -> Result<Self, FoundationError> {
        let builder = self.builder.take().ok_or_else(|| {
            CandidateError::InvalidDspDefinition(
                "SynthDef body/body_map can be configured exactly once".into(),
            )
        })?;
        self.definition = Some(
            builder
                .build_detached_body(body)
                .map_err(dsp_definition_error)?,
        );
        Ok(self)
    }

    pub fn body_map(mut self, body: rhai::FnPtr) -> Result<Self, FoundationError> {
        let builder = self.builder.take().ok_or_else(|| {
            CandidateError::InvalidDspDefinition(
                "SynthDef body/body_map can be configured exactly once".into(),
            )
        })?;
        self.definition = Some(
            builder
                .build_detached_body_map(body)
                .map_err(dsp_definition_error)?,
        );
        Ok(self)
    }

    pub fn apply(self) -> Result<SynthDefRef, FoundationError> {
        let definition = self.definition.ok_or_else(|| {
            CandidateError::InvalidDspDefinition(
                "SynthDef requires detached body or body_map IR before apply".into(),
            )
        })?;
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::SynthDef(
            DspDefinitionAuthoring { definition },
        ))?;
        let source = self.base.source().clone();
        let reference = self.base.apply(
            DeclarationOwner::Structural(source.syntax_key().clone()),
            LifecycleMetadata::register(Composition::Standalone),
            payload,
        )?;
        SynthDefRef::new(reference)
    }
}

#[derive(Clone, Debug)]
pub struct EffectDefBuilder {
    base: BuilderBase,
    builder: Option<vibelang_dsp::FxBuilderHandle>,
    definition: Option<vibelang_dsp::DspDefinitionIr>,
}

impl EffectDefBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        let qualified_name = base.address().to_string();
        Self {
            base,
            builder: Some(vibelang_dsp::FxBuilderHandle::new(qualified_name)),
            definition: None,
        }
    }

    fn configure(
        mut self,
        configure: impl FnOnce(
            vibelang_dsp::FxBuilderHandle,
        ) -> Result<vibelang_dsp::FxBuilderHandle, FoundationError>,
    ) -> Result<Self, FoundationError> {
        let builder = self.builder.take().ok_or_else(|| {
            CandidateError::InvalidDspDefinition(
                "EffectDef configuration cannot continue after body/body_map".into(),
            )
        })?;
        self.builder = Some(configure(builder)?);
        Ok(self)
    }

    pub fn param(self, name: String, default: f64) -> Result<Self, FoundationError> {
        CanonicalF64::new(default)?;
        self.configure(|builder| Ok(builder.param(name.into(), default)))
    }

    pub fn glide_ms(self, name: String, milliseconds: f64) -> Result<Self, FoundationError> {
        if !milliseconds.is_finite() || milliseconds < 0.0 {
            return Err(CandidateError::InvalidDspDefinition(
                "EffectDef glide_ms must be finite and non-negative".into(),
            )
            .into());
        }
        self.configure(|builder| Ok(builder.glide_ms(name.into(), milliseconds)))
    }

    pub fn channels(self, channels: i64) -> Result<Self, FoundationError> {
        if !(1..=255).contains(&channels) {
            return Err(CandidateError::InvalidDspDefinition(
                "EffectDef channels must be in 1..=255".into(),
            )
            .into());
        }
        self.configure(|builder| Ok(builder.channels(channels)))
    }

    pub fn body(mut self, body: rhai::FnPtr) -> Result<Self, FoundationError> {
        let builder = self.builder.take().ok_or_else(|| {
            CandidateError::InvalidDspDefinition(
                "EffectDef body/body_map can be configured exactly once".into(),
            )
        })?;
        self.definition = Some(
            builder
                .build_detached_body(body)
                .map_err(dsp_definition_error)?,
        );
        Ok(self)
    }

    pub fn body_map(mut self, body: rhai::FnPtr) -> Result<Self, FoundationError> {
        let builder = self.builder.take().ok_or_else(|| {
            CandidateError::InvalidDspDefinition(
                "EffectDef body/body_map can be configured exactly once".into(),
            )
        })?;
        self.definition = Some(
            builder
                .build_detached_body_map(body)
                .map_err(dsp_definition_error)?,
        );
        Ok(self)
    }

    pub fn apply(self) -> Result<EffectDefRef, FoundationError> {
        let definition = self.definition.ok_or_else(|| {
            CandidateError::InvalidDspDefinition(
                "EffectDef requires detached body or body_map IR before apply".into(),
            )
        })?;
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::EffectDef(
            DspDefinitionAuthoring { definition },
        ))?;
        let source = self.base.source().clone();
        let reference = self.base.apply(
            DeclarationOwner::Structural(source.syntax_key().clone()),
            LifecycleMetadata::register(Composition::Standalone),
            payload,
        )?;
        EffectDefRef::new(reference)
    }
}

fn fade_builder_v2(name: String) -> Result<FadeBuilder, Box<EvalAltResult>> {
    Ok(FadeBuilder::new(
        foundation::authoring_builder::<FadeKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    ))
}

fn fade_ref_v2(name: String) -> Result<FadeRef, Box<EvalAltResult>> {
    FadeRef::new(
        foundation::authoring_ref::<FadeKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| sequence_v2_error(error, Position::NONE))
}

fn effect_builder_v2(name: String) -> Result<EffectBuilder, Box<EvalAltResult>> {
    Ok(EffectBuilder::new(
        foundation::authoring_builder::<EffectKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    ))
}

fn fx_builder_v2(name: String) -> Result<EffectBuilder, Box<EvalAltResult>> {
    effect_builder_v2(name)
}

fn effect_ref_v2(name: String) -> Result<EffectRef, Box<EvalAltResult>> {
    EffectRef::new(
        foundation::authoring_ref::<EffectKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| sequence_v2_error(error, Position::NONE))
}

fn define_synthdef_v2(name: String) -> Result<SynthDefBuilder, Box<EvalAltResult>> {
    Ok(SynthDefBuilder::new(
        foundation::authoring_builder::<SynthDefKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    ))
}

fn synthdef_ref_v2(name: String) -> Result<SynthDefRef, Box<EvalAltResult>> {
    SynthDefRef::new(
        foundation::authoring_ref::<SynthDefKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| sequence_v2_error(error, Position::NONE))
}

fn define_effect_v2(name: String) -> Result<EffectDefBuilder, Box<EvalAltResult>> {
    Ok(EffectDefBuilder::new(
        foundation::authoring_builder::<EffectDefKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    ))
}

fn define_fx_v2(name: String) -> Result<EffectDefBuilder, Box<EvalAltResult>> {
    define_effect_v2(name)
}

fn dsp_closure_migration_error(function: &str) -> Box<EvalAltResult> {
    sequence_v2_error(
        CandidateError::InvalidDspDefinition(format!(
            "{function}(name, |builder| ...) evaluates detached under vibe-api 2: the closure \
             must return its builder chain ending in body/body_map; migrate to the canonical \
             form {function}(name).param(...).body(...)"
        ))
        .into(),
        Position::NONE,
    )
}

// Legacy 2-arg closure overloads, re-bound under v2 so they shadow the eager
// v1 registrations from `register_dsp_api` (which mutate the process-global
// DSP registry and deploy at evaluation time). The closure receives the v2
// detached builder — its method surface is a spelling-compatible superset of
// the legacy handle — and the returned chain is committed via the same
// `.apply()` path as the canonical form, so no global state is touched.
fn define_synthdef_closure_v2(
    ctx: NativeCallContext,
    name: String,
    closure: rhai::FnPtr,
) -> Result<SynthDefRef, Box<EvalAltResult>> {
    let builder = define_synthdef_v2(name)?;
    let configured = closure.call_within_context::<Dynamic>(&ctx, (builder,))?;
    let configured = configured
        .try_cast::<SynthDefBuilder>()
        .ok_or_else(|| dsp_closure_migration_error("define_synthdef"))?;
    configured
        .apply()
        .map_err(|error| sequence_v2_error(error, Position::NONE))
}

fn define_fx_closure_v2(
    ctx: NativeCallContext,
    name: String,
    closure: rhai::FnPtr,
) -> Result<EffectDefRef, Box<EvalAltResult>> {
    let builder = define_fx_v2(name)?;
    let configured = closure.call_within_context::<Dynamic>(&ctx, (builder,))?;
    let configured = configured
        .try_cast::<EffectDefBuilder>()
        .ok_or_else(|| dsp_closure_migration_error("define_fx"))?;
    configured
        .apply()
        .map_err(|error| sequence_v2_error(error, Position::NONE))
}

fn effectdef_ref_v2(name: String) -> Result<EffectDefRef, Box<EvalAltResult>> {
    EffectDefRef::new(
        foundation::authoring_ref::<EffectDefKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| sequence_v2_error(error, Position::NONE))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequenceRef {
    base: RefBase,
}

impl SequenceRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<SequenceKind>()?;
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
                    "unsupported SequenceRef lifecycle action".into(),
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
enum SequenceClipContentV2 {
    Pattern {
        reference: PatternRef,
        fragment: Option<CandidateFragment>,
    },
    Melody {
        reference: MelodyRef,
        fragment: Option<CandidateFragment>,
    },
    Fade {
        reference: FadeRef,
        fragment: Option<CandidateFragment>,
    },
    Sequence {
        reference: SequenceRef,
        fragment: Option<CandidateFragment>,
    },
}

impl SequenceClipContentV2 {
    fn reference(&self) -> &RefBase {
        match self {
            Self::Pattern { reference, .. } => reference.base(),
            Self::Melody { reference, .. } => reference.base(),
            Self::Fade { reference, .. } => reference.base(),
            Self::Sequence { reference, .. } => reference.base(),
        }
    }

    fn authoring(&self) -> Result<SequenceContentAuthoring, FoundationError> {
        Ok(match self {
            Self::Pattern { reference, .. } => {
                SequenceContentAuthoring::Pattern(reference.base().typed()?)
            }
            Self::Melody { reference, .. } => {
                SequenceContentAuthoring::Melody(reference.base().typed()?)
            }
            Self::Fade { reference, .. } => {
                SequenceContentAuthoring::Fade(reference.base().typed()?)
            }
            Self::Sequence { reference, .. } => {
                SequenceContentAuthoring::Sequence(reference.base().typed()?)
            }
        })
    }

    fn take_fragment(&mut self) -> Option<CandidateFragment> {
        match self {
            Self::Pattern { fragment, .. }
            | Self::Melody { fragment, .. }
            | Self::Fade { fragment, .. }
            | Self::Sequence { fragment, .. } => fragment.take(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SequenceClipV2 {
    start: f64,
    end: f64,
    content: SequenceClipContentV2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SequenceBuilder {
    base: BuilderBase,
    length: f64,
    looping: bool,
    clips: Vec<SequenceClipV2>,
}

impl SequenceBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        Self {
            base,
            length: 16.0,
            looping: true,
            clips: Vec::new(),
        }
    }

    pub fn loop_bars(mut self, bars: f64) -> Result<Self, FoundationError> {
        if !bars.is_finite() || bars <= 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Sequence loop bars must be finite and positive".into(),
            )
            .into());
        }
        self.length = bars * 4.0;
        Ok(self)
    }

    pub fn loop_bars_int(self, bars: i64) -> Result<Self, FoundationError> {
        self.loop_bars(bars as f64)
    }

    pub fn loop_beats(mut self, beats: f64) -> Result<Self, FoundationError> {
        if !beats.is_finite() || beats <= 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Sequence loop beats must be finite and positive".into(),
            )
            .into());
        }
        self.length = beats;
        Ok(self)
    }

    #[must_use]
    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    fn validate_range(range: &Range<f64>) -> Result<(), FoundationError> {
        if !range.start.is_finite()
            || !range.end.is_finite()
            || range.start < 0.0
            || range.end <= range.start
        {
            return Err(CandidateError::InvalidAuthoring(
                "Sequence clip range must be finite, non-negative, and non-empty".into(),
            )
            .into());
        }
        Ok(())
    }

    fn push_clip(
        mut self,
        range: Range<f64>,
        content: SequenceClipContentV2,
    ) -> Result<Self, FoundationError> {
        Self::validate_range(&range)?;
        self.clips.push(SequenceClipV2 {
            start: range.start,
            end: range.end,
            content,
        });
        Ok(self)
    }

    pub fn clip_pattern(
        self,
        range: Range<f64>,
        reference: PatternRef,
    ) -> Result<Self, FoundationError> {
        self.push_clip(
            range,
            SequenceClipContentV2::Pattern {
                reference,
                fragment: None,
            },
        )
    }

    pub fn clip_pattern_int(
        self,
        range: Range<i64>,
        reference: PatternRef,
    ) -> Result<Self, FoundationError> {
        self.clip_pattern(range.start as f64..range.end as f64, reference)
    }

    pub fn clip_pattern_inline(
        self,
        range: Range<f64>,
        builder: PatternBuilder,
    ) -> Result<Self, FoundationError> {
        Self::validate_range(&range)?;
        let parent = self.base.reference();
        let (reference, fragment) = foundation::capture_fragment(
            DeclarationOwner::Parent(parent.address().clone()),
            || builder.apply(),
        )?;
        self.push_clip(
            range,
            SequenceClipContentV2::Pattern {
                reference,
                fragment: Some(fragment),
            },
        )
    }

    pub fn clip_pattern_inline_int(
        self,
        range: Range<i64>,
        builder: PatternBuilder,
    ) -> Result<Self, FoundationError> {
        self.clip_pattern_inline(range.start as f64..range.end as f64, builder)
    }

    pub fn clip_melody(
        self,
        range: Range<f64>,
        reference: MelodyRef,
    ) -> Result<Self, FoundationError> {
        self.push_clip(
            range,
            SequenceClipContentV2::Melody {
                reference,
                fragment: None,
            },
        )
    }

    pub fn clip_melody_int(
        self,
        range: Range<i64>,
        reference: MelodyRef,
    ) -> Result<Self, FoundationError> {
        self.clip_melody(range.start as f64..range.end as f64, reference)
    }

    pub fn clip_melody_inline(
        self,
        range: Range<f64>,
        builder: MelodyBuilder,
    ) -> Result<Self, FoundationError> {
        Self::validate_range(&range)?;
        let parent = self.base.reference();
        let (reference, fragment) = foundation::capture_fragment(
            DeclarationOwner::Parent(parent.address().clone()),
            || builder.apply(),
        )?;
        self.push_clip(
            range,
            SequenceClipContentV2::Melody {
                reference,
                fragment: Some(fragment),
            },
        )
    }

    pub fn clip_melody_inline_int(
        self,
        range: Range<i64>,
        builder: MelodyBuilder,
    ) -> Result<Self, FoundationError> {
        self.clip_melody_inline(range.start as f64..range.end as f64, builder)
    }

    pub fn clip_fade(self, range: Range<f64>, reference: FadeRef) -> Result<Self, FoundationError> {
        self.push_clip(
            range,
            SequenceClipContentV2::Fade {
                reference,
                fragment: None,
            },
        )
    }

    pub fn clip_fade_int(
        self,
        range: Range<i64>,
        reference: FadeRef,
    ) -> Result<Self, FoundationError> {
        self.clip_fade(range.start as f64..range.end as f64, reference)
    }

    pub fn clip_fade_inline(
        self,
        range: Range<f64>,
        builder: FadeBuilder,
    ) -> Result<Self, FoundationError> {
        Self::validate_range(&range)?;
        let parent = self.base.reference();
        let (reference, fragment) = foundation::capture_fragment(
            DeclarationOwner::Parent(parent.address().clone()),
            || builder.apply(),
        )?;
        self.push_clip(
            range,
            SequenceClipContentV2::Fade {
                reference,
                fragment: Some(fragment),
            },
        )
    }

    pub fn clip_fade_inline_int(
        self,
        range: Range<i64>,
        builder: FadeBuilder,
    ) -> Result<Self, FoundationError> {
        self.clip_fade_inline(range.start as f64..range.end as f64, builder)
    }

    pub fn clip_sequence(
        self,
        range: Range<f64>,
        reference: SequenceRef,
    ) -> Result<Self, FoundationError> {
        self.push_clip(
            range,
            SequenceClipContentV2::Sequence {
                reference,
                fragment: None,
            },
        )
    }

    pub fn clip_sequence_int(
        self,
        range: Range<i64>,
        reference: SequenceRef,
    ) -> Result<Self, FoundationError> {
        self.clip_sequence(range.start as f64..range.end as f64, reference)
    }

    pub fn clip_sequence_inline(
        self,
        range: Range<f64>,
        builder: SequenceBuilder,
    ) -> Result<Self, FoundationError> {
        Self::validate_range(&range)?;
        let parent = self.base.reference();
        let (reference, fragment) = foundation::capture_fragment(
            DeclarationOwner::Parent(parent.address().clone()),
            || builder.apply(),
        )?;
        self.push_clip(
            range,
            SequenceClipContentV2::Sequence {
                reference,
                fragment: Some(fragment),
            },
        )
    }

    pub fn clip_sequence_inline_int(
        self,
        range: Range<i64>,
        builder: SequenceBuilder,
    ) -> Result<Self, FoundationError> {
        self.clip_sequence_inline(range.start as f64..range.end as f64, builder)
    }

    pub(crate) fn build_fragment(
        self,
        lifecycle: DesiredLifecycle,
    ) -> Result<(CandidateFragment, SequenceRef), FoundationError> {
        if !self.length.is_finite() || self.length <= 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Sequence length must be finite and positive".into(),
            )
            .into());
        }
        let source = self.base.source().clone();
        let mut clips = Vec::with_capacity(self.clips.len());
        let mut dependencies = Vec::with_capacity(self.clips.len());
        let mut inline_fragments = Vec::new();
        for mut clip in self.clips {
            if clip.end > self.length {
                return Err(CandidateError::InvalidAuthoring(
                    "Sequence clip extends beyond the declared length".into(),
                )
                .into());
            }
            let start_ticks = Beat::from_f64(clip.start).raw();
            let end_ticks = Beat::from_f64(clip.end).raw();
            let content = clip.content.authoring()?;
            dependencies.push((clip.content.reference().clone(), source.clone()));
            if let Some(fragment) = clip.content.take_fragment() {
                inline_fragments.push(fragment);
            }
            clips.push(SequenceClipAuthoring {
                start_ticks,
                end_ticks,
                content,
            });
        }
        let payload =
            DeclarationPayload::authoring(AuthoringDeclaration::Sequence(SequenceAuthoring {
                clips,
                length_ticks: Beat::from_f64(self.length).raw(),
                looping: self.looping,
                lifecycle,
            }))?;
        let metadata = match lifecycle {
            DesiredLifecycle::Dormant => LifecycleMetadata::register(Composition::Standalone),
            DesiredLifecycle::Start(_) => LifecycleMetadata::start(Composition::Standalone),
        };
        let owner = DeclarationOwner::Structural(source.syntax_key().clone());
        let (mut fragment, reference) =
            self.base.fragment(owner, metadata, payload, dependencies)?;
        for inline in inline_fragments {
            fragment.extend(inline);
        }
        Ok((fragment, SequenceRef::new(reference)?))
    }

    fn terminal(self, lifecycle: DesiredLifecycle) -> Result<SequenceRef, FoundationError> {
        let (fragment, reference) = self.build_fragment(lifecycle)?;
        foundation::commit_fragment(fragment)?;
        Ok(reference)
    }

    pub fn apply(self) -> Result<SequenceRef, FoundationError> {
        self.terminal(DesiredLifecycle::Dormant)
    }

    pub fn start(self) -> Result<SequenceRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Normal))
    }

    pub fn start_now(self) -> Result<SequenceRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Immediate))
    }

    pub fn launch(self) -> Result<SequenceRef, FoundationError> {
        self.start()
    }
}

pub(crate) fn sequence_builder_v2(name: String) -> Result<SequenceBuilder, Box<EvalAltResult>> {
    Ok(SequenceBuilder::new(
        foundation::authoring_builder::<SequenceKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    ))
}

pub(crate) fn sequence_ref_v2(name: String) -> Result<SequenceRef, Box<EvalAltResult>> {
    SequenceRef::new(
        foundation::authoring_ref::<SequenceKind>(&name, GroupScope::root())
            .map_err(|error| sequence_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| sequence_v2_error(error, Position::NONE))
}

fn sequence_v2_error(error: FoundationError, position: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        error.to_string().into(),
        position,
    ))
}

pub(crate) fn install_v2(engine: &mut Engine) {
    vibelang_dsp::register_dsp_api(engine);
    engine
        .register_type_with_name::<FadeBuilder>("FadeBuilder")
        .register_type_with_name::<FadeRef>("FadeRef")
        .register_type_with_name::<EffectBuilder>("EffectBuilder")
        .register_type_with_name::<EffectRef>("EffectRef")
        .register_type_with_name::<SynthDefBuilder>("SynthDefBuilder")
        .register_type_with_name::<SynthDefRef>("SynthDefRef")
        .register_type_with_name::<EffectDefBuilder>("EffectDefBuilder")
        .register_type_with_name::<EffectDefRef>("EffectDefRef")
        .register_fn("fade", fade_builder_v2)
        .register_fn("fade_ref", fade_ref_v2)
        .register_fn("effect", effect_builder_v2)
        .register_fn("fx", fx_builder_v2)
        .register_fn("effect_ref", effect_ref_v2)
        .register_fn("define_synthdef", define_synthdef_v2)
        .register_fn("define_synthdef", define_synthdef_closure_v2)
        .register_fn("synthdef_ref", synthdef_ref_v2)
        .register_fn("define_effect", define_effect_v2)
        .register_fn("define_fx", define_fx_v2)
        .register_fn("define_fx", define_fx_closure_v2)
        .register_fn("effectdef_ref", effectdef_ref_v2)
        .register_fn("on_group", |builder: FadeBuilder, target: GroupRef| {
            builder
                .on_group(target)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("on_voice", |builder: FadeBuilder, target: VoiceRef| {
            builder
                .on_voice(target)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("on_effect", |builder: FadeBuilder, target: EffectRef| {
            builder
                .on_effect(target)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("on_pattern", |builder: FadeBuilder, target: PatternRef| {
            builder
                .on_pattern(target)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("on_melody", |builder: FadeBuilder, target: MelodyRef| {
            builder
                .on_melody(target)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("param", |builder: FadeBuilder, name: String| {
            builder
                .param(name)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("from", FadeBuilder::from)
        .register_fn("to", FadeBuilder::to)
        .register_fn("over", |builder: FadeBuilder, beats: f64| {
            builder
                .over(beats)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("over_bars", |builder: FadeBuilder, bars: f64| {
            builder
                .over_bars(bars)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("over_bars", |builder: FadeBuilder, bars: i64| {
            builder
                .over_bars(bars as f64)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("curve", |builder: FadeBuilder, name: String| {
            builder
                .curve(name)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("exp", |builder: FadeBuilder, exponent: f64| {
            builder
                .exp(exponent)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("spline", |builder: FadeBuilder, points: rhai::Array| {
            builder
                .spline(points)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("point", |builder: FadeBuilder, time: f64, value: f64| {
            builder
                .point(time, value)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("apply", |builder: FadeBuilder| {
            builder
                .apply()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("start", |builder: FadeBuilder| {
            builder
                .start()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("start_now", |builder: FadeBuilder| {
            builder
                .start_now()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("launch", |builder: FadeBuilder| {
            builder
                .launch()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("now", |builder: FadeBuilder| {
            builder
                .now()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("start", |reference: FadeRef| {
            reference
                .start()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("start_now", |reference: FadeRef| {
            reference
                .start_now()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("launch", |reference: FadeRef| {
            reference
                .launch()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("now", |reference: FadeRef| {
            reference
                .now()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("restart", |reference: FadeRef| {
            reference
                .restart()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("stop", |reference: FadeRef| {
            reference
                .stop()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("remove", |reference: FadeRef| {
            reference
                .remove()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("cancel", |reference: FadeRef| {
            reference
                .cancel()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("status", |reference: FadeRef| {
            reference
                .status()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn(
            "definition",
            |builder: EffectBuilder, definition: EffectDefRef| {
                builder
                    .definition(definition)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("synth", |builder: EffectBuilder, name: String| {
            builder
                .synth(name)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn(
            "param",
            |builder: EffectBuilder, name: String, value: f64| {
                builder
                    .param(name, value)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("apply", |builder: EffectBuilder| {
            builder
                .apply()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("remove", |reference: EffectRef| {
            reference
                .remove()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("status", |reference: EffectRef| {
            reference
                .status()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn(
            "param",
            |builder: SynthDefBuilder, name: String, value: f64| {
                builder
                    .param(name, value)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "glide_ms",
            |builder: SynthDefBuilder, name: String, value: f64| {
                builder
                    .glide_ms(name, value)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("out_bus", |builder: SynthDefBuilder, tag: String| {
            builder
                .out_bus(tag)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("input", |builder: SynthDefBuilder, name: String| {
            builder
                .input(name, 1)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn(
            "input",
            |builder: SynthDefBuilder, name: String, channels: i64| {
                builder
                    .input(name, channels)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("output", |builder: SynthDefBuilder, name: String| {
            builder
                .output(name, 1)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn(
            "output",
            |builder: SynthDefBuilder, name: String, channels: i64| {
                builder
                    .output(name, channels)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("output_kr", |builder: SynthDefBuilder, name: String| {
            builder
                .output_kr(name, 1)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn(
            "output_kr",
            |builder: SynthDefBuilder, name: String, channels: i64| {
                builder
                    .output_kr(name, channels)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("output_tr", |builder: SynthDefBuilder, name: String| {
            builder
                .output_tr(name, 1)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn(
            "output_tr",
            |builder: SynthDefBuilder, name: String, channels: i64| {
                builder
                    .output_tr(name, channels)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "outputs",
            |builder: SynthDefBuilder, outputs: rhai::Array| {
                builder
                    .outputs(outputs)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("body", |builder: SynthDefBuilder, body: rhai::FnPtr| {
            builder
                .body(body)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("body_map", |builder: SynthDefBuilder, body: rhai::FnPtr| {
            builder
                .body_map(body)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("apply", |builder: SynthDefBuilder| {
            builder
                .apply()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn(
            "param",
            |builder: EffectDefBuilder, name: String, value: f64| {
                builder
                    .param(name, value)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "glide_ms",
            |builder: EffectDefBuilder, name: String, value: f64| {
                builder
                    .glide_ms(name, value)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("channels", |builder: EffectDefBuilder, channels: i64| {
            builder
                .channels(channels)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("body", |builder: EffectDefBuilder, body: rhai::FnPtr| {
            builder
                .body(body)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn(
            "body_map",
            |builder: EffectDefBuilder, body: rhai::FnPtr| {
                builder
                    .body_map(body)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("apply", |builder: EffectDefBuilder| {
            builder
                .apply()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("remove", |reference: SynthDefRef| {
            reference
                .remove()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("status", |reference: SynthDefRef| {
            reference
                .status()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("remove", |reference: EffectDefRef| {
            reference
                .remove()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("status", |reference: EffectDefRef| {
            reference
                .status()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        });
    engine
        .register_type_with_name::<SequenceBuilder>("SequenceBuilder")
        .register_type_with_name::<SequenceRef>("SequenceRef")
        .register_fn("sequence", sequence_builder_v2)
        .register_fn("sequence_ref", sequence_ref_v2)
        .register_fn("loop_bars", |builder: SequenceBuilder, bars: f64| {
            builder
                .loop_bars(bars)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("loop_bars", |builder: SequenceBuilder, bars: i64| {
            builder
                .loop_bars_int(bars)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("loop_beats", |builder: SequenceBuilder, beats: f64| {
            builder
                .loop_beats(beats)
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("looping", SequenceBuilder::looping)
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<f64>, reference: PatternRef| {
                builder
                    .clip_pattern(range, reference)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<i64>, reference: PatternRef| {
                builder
                    .clip_pattern_int(range, reference)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<f64>, inline: PatternBuilder| {
                builder
                    .clip_pattern_inline(range, inline)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<i64>, inline: PatternBuilder| {
                builder
                    .clip_pattern_inline_int(range, inline)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<f64>, reference: MelodyRef| {
                builder
                    .clip_melody(range, reference)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<i64>, reference: MelodyRef| {
                builder
                    .clip_melody_int(range, reference)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<f64>, inline: MelodyBuilder| {
                builder
                    .clip_melody_inline(range, inline)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<i64>, inline: MelodyBuilder| {
                builder
                    .clip_melody_inline_int(range, inline)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<f64>, reference: FadeRef| {
                builder
                    .clip_fade(range, reference)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<i64>, reference: FadeRef| {
                builder
                    .clip_fade_int(range, reference)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<f64>, inline: FadeBuilder| {
                builder
                    .clip_fade_inline(range, inline)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<i64>, inline: FadeBuilder| {
                builder
                    .clip_fade_inline_int(range, inline)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<f64>, reference: SequenceRef| {
                builder
                    .clip_sequence(range, reference)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<i64>, reference: SequenceRef| {
                builder
                    .clip_sequence_int(range, reference)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<f64>, inline: SequenceBuilder| {
                builder
                    .clip_sequence_inline(range, inline)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<i64>, inline: SequenceBuilder| {
                builder
                    .clip_sequence_inline_int(range, inline)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn("apply", |builder: SequenceBuilder| {
            builder
                .apply()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("start", |builder: SequenceBuilder| {
            builder
                .start()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("start_now", |builder: SequenceBuilder| {
            builder
                .start_now()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("launch", |builder: SequenceBuilder| {
            builder
                .launch()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("start", |reference: SequenceRef| {
            reference
                .start()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("start_now", |reference: SequenceRef| {
            reference
                .start_now()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("stop", |reference: SequenceRef| {
            reference
                .stop()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("remove", |reference: SequenceRef| {
            reference
                .remove()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("cancel", |reference: SequenceRef| {
            reference
                .cancel()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        })
        .register_fn("status", |reference: SequenceRef| {
            reference
                .status()
                .map_err(|error| sequence_v2_error(error, Position::NONE))
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibelang_core::candidate::{
        AuthoringDeclaration, ContractDigest, ContributionId, ContributionIr, DeclarationKey,
        DeclarationOwner, EngineInstanceId, EvaluationIdentity, LanguageContract, SourceAnchor,
        SyntaxKey, VoiceKind,
    };
    use vibelang_core::mutation::RuntimeEpoch;

    fn v2_identity() -> EvaluationIdentity {
        EvaluationIdentity::new(
            LanguageContract::v2(ContractDigest::from_bytes(b"sequence-v2-test")),
            EngineInstanceId::new(),
            RuntimeEpoch::new(),
        )
    }

    fn voice_ref(name: &str) -> super::super::voice::VoiceRef {
        super::super::voice::VoiceRef::new(
            foundation::authoring_ref::<VoiceKind>(name, GroupScope::root()).unwrap(),
        )
        .unwrap()
    }

    fn group_ref(name: &str) -> GroupRef {
        crate::api::group::group_builder_v2(name.into())
            .unwrap()
            .apply()
            .unwrap()
    }

    fn capture_effect_contribution(
        group: &GroupRef,
        id: &str,
        capture: impl FnOnce() -> Result<EffectRef, FoundationError>,
    ) -> Result<EffectRef, FoundationError> {
        let module = group.base().address().module().clone();
        let syntax = SyntaxKey::Explicit(DeclarationKey::new(id)?);
        let contribution_id = ContributionId::new(module.clone(), syntax.clone());
        let source = SourceAnchor::new(module, syntax, None);
        let (reference, effect_fragment) = foundation::capture_fragment(
            DeclarationOwner::Contribution(contribution_id.clone()),
            capture,
        )?;
        let mut fragment = CandidateFragment::default().contribution(ContributionIr::new(
            contribution_id,
            group.base().typed()?,
            None,
            source,
        ));
        fragment.extend(effect_fragment);
        foundation::commit_fragment(fragment)?;
        Ok(reference)
    }

    #[test]
    fn v2_inline_sequence_fragments_are_atomic_and_owned_by_their_direct_parent() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let rejected_pattern = PatternBuilder::new(
            foundation::authoring_builder::<vibelang_core::candidate::PatternKind>(
                "rejected-pattern",
                GroupScope::root(),
            )
            .unwrap(),
        )
        .on(voice_ref("unapplied-voice"))
        .step("x...".into())
        .unwrap();
        let rejected = SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("rejected-sequence", GroupScope::root())
                .unwrap(),
        )
        .clip_pattern_inline(0.0..4.0, rejected_pattern)
        .unwrap()
        .loop_beats(2.0)
        .unwrap()
        .apply();
        assert!(matches!(
            rejected,
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        let candidate = foundation::finish_evaluation().unwrap();
        assert!(candidate.declarations().is_empty());

        foundation::begin_evaluation(v2_identity()).unwrap();
        let voice = crate::api::voice::VoiceBuilder::new(
            foundation::authoring_builder::<VoiceKind>("lead", GroupScope::root()).unwrap(),
        )
        .synth("sine".into())
        .unwrap()
        .apply()
        .unwrap();
        let pattern = PatternBuilder::new(
            foundation::authoring_builder::<vibelang_core::candidate::PatternKind>(
                "inline-pattern",
                GroupScope::root(),
            )
            .unwrap(),
        )
        .on(voice)
        .step("x...".into())
        .unwrap();
        let child = SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("child", GroupScope::root()).unwrap(),
        )
        .loop_beats(4.0)
        .unwrap()
        .clip_pattern_inline(0.0..4.0, pattern)
        .unwrap();
        SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("parent", GroupScope::root()).unwrap(),
        )
        .loop_beats(4.0)
        .unwrap()
        .clip_sequence_inline(0.0..4.0, child)
        .unwrap()
        .apply()
        .unwrap();
        let candidate = foundation::finish_evaluation().unwrap();

        assert_eq!(candidate.declarations().len(), 4);
        let owner_of = |key: &str| {
            candidate
                .declarations()
                .iter()
                .find(|declaration| declaration.address().key().as_str() == key)
                .unwrap()
                .owner()
        };
        assert!(matches!(
            owner_of("child"),
            DeclarationOwner::Parent(parent) if parent.key().as_str() == "parent"
        ));
        assert!(matches!(
            owner_of("inline-pattern"),
            DeclarationOwner::Parent(parent) if parent.key().as_str() == "child"
        ));
    }

    #[test]
    fn v2_sequence_content_union_is_tagged_and_rejects_wrong_ref_kinds_during_configuration() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let pattern = PatternRef::new(
            foundation::authoring_ref::<vibelang_core::candidate::PatternKind>(
                "pattern",
                GroupScope::root(),
            )
            .unwrap(),
        )
        .unwrap();
        let melody = MelodyRef::new(
            foundation::authoring_ref::<vibelang_core::candidate::MelodyKind>(
                "melody",
                GroupScope::root(),
            )
            .unwrap(),
        )
        .unwrap();
        let fade = FadeRef::new(
            foundation::authoring_ref::<FadeKind>("fade", GroupScope::root()).unwrap(),
        )
        .unwrap();
        let nested = SequenceRef::new(
            foundation::authoring_ref::<SequenceKind>("nested", GroupScope::root()).unwrap(),
        )
        .unwrap();
        let builder = SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("song", GroupScope::root()).unwrap(),
        )
        .clip_pattern(0.0..1.0, pattern)
        .unwrap()
        .clip_melody(1.0..2.0, melody)
        .unwrap()
        .clip_fade(2.0..3.0, fade)
        .unwrap()
        .clip_sequence(3.0..4.0, nested)
        .unwrap();
        assert!(matches!(
            &builder.clips[0].content,
            SequenceClipContentV2::Pattern { .. }
        ));
        assert!(matches!(
            &builder.clips[1].content,
            SequenceClipContentV2::Melody { .. }
        ));
        assert!(matches!(
            &builder.clips[2].content,
            SequenceClipContentV2::Fade { .. }
        ));
        assert!(matches!(
            &builder.clips[3].content,
            SequenceClipContentV2::Sequence { .. }
        ));
        let wrong = foundation::authoring_ref::<VoiceKind>("voice", GroupScope::root()).unwrap();
        assert!(matches!(
            FadeRef::new(wrong),
            Err(FoundationError::Candidate(
                CandidateError::WrongRefKind { .. }
            ))
        ));
        let candidate = foundation::finish_evaluation().unwrap();
        assert!(candidate.declarations().is_empty());
    }

    #[test]
    fn v2_sequence_reference_cycles_report_the_dependency_path() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let a_ref = SequenceRef::new(
            foundation::authoring_ref::<SequenceKind>("a", GroupScope::root()).unwrap(),
        )
        .unwrap();
        let b_ref = SequenceRef::new(
            foundation::authoring_ref::<SequenceKind>("b", GroupScope::root()).unwrap(),
        )
        .unwrap();
        SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("a", GroupScope::root()).unwrap(),
        )
        .clip_sequence(0.0..1.0, b_ref)
        .unwrap()
        .apply()
        .unwrap();
        SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("b", GroupScope::root()).unwrap(),
        )
        .clip_sequence(0.0..1.0, a_ref)
        .unwrap()
        .apply()
        .unwrap();

        assert!(matches!(
            foundation::finish_evaluation(),
            Err(FoundationError::Candidate(CandidateError::DependencyCycle(path)))
                if path.len() == 3
        ));
    }

    #[test]
    fn v2_sequence_launch_alias_matches_start_and_status_is_live_source_only() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let started = SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("started", GroupScope::root()).unwrap(),
        )
        .start()
        .unwrap();
        SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("launched", GroupScope::root()).unwrap(),
        )
        .launch()
        .unwrap();
        SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("immediate", GroupScope::root()).unwrap(),
        )
        .start_now()
        .unwrap();
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
                    declaration: AuthoringDeclaration::Sequence(sequence),
                    ..
                } => Some((declaration.address().key().as_str(), sequence.lifecycle)),
                _ => None,
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            lifecycles["started"],
            DesiredLifecycle::Start(StartMode::Normal)
        );
        assert_eq!(lifecycles["launched"], lifecycles["started"]);
        assert_eq!(
            lifecycles["immediate"],
            DesiredLifecycle::Start(StartMode::Immediate)
        );
    }

    #[test]
    fn v2_fade_terminals_are_typed_effectful_and_timing_distinct() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let group = group_ref("mix");
        let mut engine = Engine::new();
        install_v2(&mut engine);
        let spline = engine
            .eval::<FadeBuilder>(r#"fade("spline").spline([0.0, 0.0, 0.5, 0.75])"#)
            .unwrap();
        assert!(matches!(
            spline.curve,
            FadeCurveAuthoring::CubicSpline(ref points)
                if points.len() == 2 && points[0].time.get() == 0.0
        ));
        let bars = engine
            .eval::<FadeBuilder>(r#"fade("bars").over_bars(2)"#)
            .unwrap();
        assert_eq!(bars.duration_beats, 8.0);
        assert!(engine
            .eval::<FadeBuilder>(r#"fade("invalid-spline").spline([0.25, 0.5, 0.75])"#)
            .unwrap_err()
            .to_string()
            .contains("complete time/value pairs"));

        let detached = FadeBuilder::new(
            foundation::authoring_builder::<FadeKind>("detached", GroupScope::root()).unwrap(),
        )
        .from(0.25);
        let configured_clone = detached.clone().to(0.75);
        assert_eq!(detached.to, 1.0);
        assert_eq!(configured_clone.to, 0.75);
        assert!(matches!(
            detached.apply(),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            FadeBuilder::new(
                foundation::authoring_builder::<FadeKind>("bad-curve", GroupScope::root()).unwrap()
            )
            .on_group(group.clone())
            .unwrap()
            .curve("mystery".into()),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            FadeBuilder::new(
                foundation::authoring_builder::<FadeKind>("ambiguous", GroupScope::root()).unwrap()
            )
            .on_group(group.clone())
            .unwrap()
            .on_group(group.clone()),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));

        let dormant = FadeBuilder::new(
            foundation::authoring_builder::<FadeKind>("dormant", GroupScope::root()).unwrap(),
        )
        .on_group(group.clone())
        .unwrap()
        .point(0.0, 0.0)
        .unwrap()
        .apply()
        .unwrap();
        FadeBuilder::new(
            foundation::authoring_builder::<FadeKind>("normal", GroupScope::root()).unwrap(),
        )
        .on_group(group.clone())
        .unwrap()
        .start()
        .unwrap();
        FadeBuilder::new(
            foundation::authoring_builder::<FadeKind>("launch-alias", GroupScope::root()).unwrap(),
        )
        .on_group(group.clone())
        .unwrap()
        .launch()
        .unwrap();
        FadeBuilder::new(
            foundation::authoring_builder::<FadeKind>("immediate", GroupScope::root()).unwrap(),
        )
        .on_group(group.clone())
        .unwrap()
        .start_now()
        .unwrap();
        FadeBuilder::new(
            foundation::authoring_builder::<FadeKind>("now-alias", GroupScope::root()).unwrap(),
        )
        .on_group(group)
        .unwrap()
        .now()
        .unwrap();
        assert!(matches!(
            dormant.status(),
            Err(FoundationError::ObservationUnavailable)
        ));
        dormant.clone().restart().unwrap();
        dormant.clone().stop().unwrap();
        dormant.cancel().unwrap();

        let candidate = foundation::finish_evaluation().unwrap();
        let lifecycles = candidate
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration.payload() {
                DeclarationPayload::Authoring {
                    declaration: AuthoringDeclaration::Fade(fade),
                    ..
                } => Some((declaration.address().key().as_str(), fade.lifecycle)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(lifecycles["dormant"], DesiredLifecycle::Dormant);
        assert_eq!(
            lifecycles["normal"],
            DesiredLifecycle::Start(StartMode::Normal)
        );
        assert_eq!(lifecycles["launch-alias"], lifecycles["normal"]);
        assert_eq!(
            lifecycles["immediate"],
            DesiredLifecycle::Start(StartMode::Immediate)
        );
        assert_eq!(lifecycles["now-alias"], lifecycles["immediate"]);
        assert!(candidate
            .operations()
            .iter()
            .any(|operation| operation.action() == &LifecycleAction::Restart));
    }

    #[test]
    fn v2_inline_fade_is_detached_and_owned_by_the_sequence_terminal() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let group = group_ref("mix");
        let inline = FadeBuilder::new(
            foundation::authoring_builder::<FadeKind>("inline-fade", GroupScope::root()).unwrap(),
        )
        .on_group(group)
        .unwrap()
        .over(2.0)
        .unwrap();
        SequenceBuilder::new(
            foundation::authoring_builder::<SequenceKind>("arrangement", GroupScope::root())
                .unwrap(),
        )
        .loop_beats(4.0)
        .unwrap()
        .clip_fade_inline(0.0..2.0, inline)
        .unwrap()
        .apply()
        .unwrap();
        let candidate = foundation::finish_evaluation().unwrap();
        let fade = candidate
            .declarations()
            .iter()
            .find(|declaration| declaration.address().key().as_str() == "inline-fade")
            .unwrap();
        assert!(matches!(
            fade.owner(),
            DeclarationOwner::Parent(parent) if parent.key().as_str() == "arrangement"
        ));
    }

    #[test]
    fn v2_dsp_definitions_are_detached_module_qualified_and_candidate_owned() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        install_v2(&mut engine);

        let synth = engine
            .eval::<SynthDefBuilder>(
                r#"define_synthdef("candidate_tone").param("freq", 440.0).body(|freq| dc_ar(0.0))"#,
            )
            .unwrap();
        let canonical_effect = engine
            .eval::<EffectDefBuilder>(
                r#"define_effect("candidate_room").param("mix", 0.5).body(|input, mix| input)"#,
            )
            .unwrap();
        let alias_effect = engine
            .eval::<EffectDefBuilder>(
                r#"define_fx("candidate_room").param("mix", 0.5).body(|input, mix| input)"#,
            )
            .unwrap();
        assert_eq!(
            canonical_effect.definition.as_ref().unwrap(),
            alias_effect.definition.as_ref().unwrap()
        );
        let qualified_synth = synth.base.address().to_string();
        let qualified_effect = canonical_effect.base.address().to_string();
        assert_eq!(synth.definition.as_ref().unwrap().name(), qualified_synth);
        assert_eq!(
            canonical_effect.definition.as_ref().unwrap().name(),
            qualified_effect
        );
        assert!(!vibelang_dsp::synthdef_exists(&qualified_synth));
        assert!(!vibelang_dsp::effect_exists(&qualified_effect));
        assert_eq!(vibelang_dsp::get_synthdef_hash(&qualified_synth), None);

        synth.apply().unwrap();
        let effect_definition = canonical_effect.apply().unwrap();
        let group = group_ref("fx-bus");
        assert!(matches!(
            EffectBuilder::new(
                foundation::authoring_builder::<EffectKind>("missing", GroupScope::root()).unwrap()
            )
            .apply(),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            EffectBuilder::new(
                foundation::authoring_builder::<EffectKind>("outside", GroupScope::root()).unwrap()
            )
            .definition(effect_definition.clone())
            .unwrap()
            .apply(),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        let effect = capture_effect_contribution(&group, "room-edge", || {
            EffectBuilder::new(
                foundation::authoring_builder::<EffectKind>("room", GroupScope::root()).unwrap(),
            )
            .definition(effect_definition)
            .and_then(|builder| builder.param("mix".into(), 0.25))
            .and_then(EffectBuilder::apply)
        })
        .unwrap();
        assert!(matches!(
            effect.status(),
            Err(FoundationError::ObservationUnavailable)
        ));

        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.dsp_definitions().definitions().count(), 2);
        assert!(candidate
            .dsp_definitions()
            .definitions()
            .all(|definition| definition.name().starts_with("vibelang::main::")));
        let effect = candidate
            .declarations()
            .iter()
            .find(|declaration| declaration.address().key().as_str() == "room")
            .unwrap();
        assert!(matches!(effect.owner(), DeclarationOwner::Contribution(_)));
        assert!(!vibelang_dsp::synthdef_exists(&qualified_synth));
        assert!(!vibelang_dsp::effect_exists(&qualified_effect));
        assert_eq!(vibelang_dsp::get_synthdef_hash(&qualified_synth), None);
    }

    #[test]
    fn v2_effect_and_definition_aliases_forward_and_invalid_composition_is_structured() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        install_v2(&mut engine);
        let canonical = engine.eval::<EffectBuilder>(r#"effect("room")"#).unwrap();
        let alias = engine.eval::<EffectBuilder>(r#"fx("room")"#).unwrap();
        assert_eq!(canonical, alias);
        let error = engine
            .eval::<rhai::Dynamic>(
                r#"define_synthdef("double_body").body(|| dc_ar(0.0)).body(|| dc_ar(1.0))"#,
            )
            .unwrap_err();
        assert!(error.to_string().contains("exactly once"));
        assert!(matches!(
            EffectRef::new(
                foundation::authoring_ref::<FadeKind>("wrong-kind", GroupScope::root()).unwrap()
            ),
            Err(FoundationError::Candidate(
                CandidateError::WrongRefKind { .. }
            ))
        ));
        let candidate = foundation::finish_evaluation().unwrap();
        assert!(candidate.declarations().is_empty());
        assert_eq!(candidate.dsp_definitions().definitions().count(), 0);
    }

    #[test]
    fn v2_legacy_closure_overloads_leave_no_residue_when_evaluation_is_rejected() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        const PREFIX: &str = "b1_purity";

        foundation::abort_evaluation();

        // Census restricted to this test's unique name prefix so concurrent
        // tests that legitimately use the process-global DSP registries and
        // deploy callback cannot perturb the counts.
        let registry_census = || {
            vibelang_dsp::get_all_synthdefs_encoded()
                .into_iter()
                .filter(|(name, _)| name.contains(PREFIX))
                .count()
                + vibelang_dsp::get_all_effect_names()
                    .into_iter()
                    .filter(|name| name.contains(PREFIX))
                    .count()
        };
        // Backend call census: encoded synthdef bytes embed the def name.
        let deploys = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&deploys);
        vibelang_dsp::set_deploy_callback(move |bytes| {
            if bytes
                .windows(PREFIX.len())
                .any(|window| window == PREFIX.as_bytes())
            {
                counted.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        });

        let registry_before = registry_census();

        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        install_v2(&mut engine);
        engine
            .eval::<Dynamic>(&format!(
                r#"
                define_synthdef("{PREFIX}_synth", |b| b.param("freq", 440.0).body(|freq| dc_ar(0.0)));
                define_fx("{PREFIX}_fx", |b| b.param("mix", 0.5).body(|input, mix| input));
                this_function_does_not_exist();
                "#
            ))
            .expect_err("evaluation must fail after the legacy overloads ran");
        foundation::abort_evaluation();

        assert_eq!(
            deploys.load(Ordering::SeqCst),
            0,
            "rejected evaluation must make zero backend deploy calls"
        );
        assert_eq!(
            registry_before,
            registry_census(),
            "rejected evaluation must leave the DSP registry census unchanged"
        );
        for name in [
            format!("{PREFIX}_synth"),
            format!("vibelang::main::{PREFIX}_synth"),
        ] {
            assert!(!vibelang_dsp::synthdef_exists(&name));
            assert_eq!(vibelang_dsp::get_synthdef_hash(&name), None);
        }
        for name in [
            format!("{PREFIX}_fx"),
            format!("vibelang::main::{PREFIX}_fx"),
        ] {
            assert!(!vibelang_dsp::effect_exists(&name));
            assert_eq!(vibelang_dsp::get_synthdef_hash(&name), None);
        }
    }

    #[test]
    fn v2_legacy_closure_overloads_match_canonical_candidate_payload() {
        foundation::abort_evaluation();
        let mut engine = Engine::new();
        install_v2(&mut engine);

        foundation::begin_evaluation(v2_identity()).unwrap();
        engine
            .eval::<Dynamic>(
                r#"
                define_synthdef("b1_eq_synth", |b| b.param("freq", 440.0).body(|freq| dc_ar(0.0)));
                define_fx("b1_eq_fx", |b| b.param("mix", 0.5).body(|input, mix| input));
                "#,
            )
            .unwrap();
        let closure_candidate = foundation::finish_evaluation().unwrap();

        foundation::begin_evaluation(v2_identity()).unwrap();
        engine
            .eval::<Dynamic>(
                r#"
                define_synthdef("b1_eq_synth").param("freq", 440.0).body(|freq| dc_ar(0.0)).apply();
                define_fx("b1_eq_fx").param("mix", 0.5).body(|input, mix| input).apply();
                "#,
            )
            .unwrap();
        let canonical_candidate = foundation::finish_evaluation().unwrap();

        let mut closure_definitions: Vec<_> =
            closure_candidate.dsp_definitions().definitions().collect();
        closure_definitions.sort_by_key(|definition| definition.name().to_string());
        let mut canonical_definitions: Vec<_> = canonical_candidate
            .dsp_definitions()
            .definitions()
            .collect();
        canonical_definitions.sort_by_key(|definition| definition.name().to_string());
        assert_eq!(closure_definitions.len(), 2);
        assert_eq!(closure_definitions, canonical_definitions);

        let mut closure_addresses: Vec<String> = closure_candidate
            .declarations()
            .iter()
            .map(|declaration| declaration.address().to_string())
            .collect();
        closure_addresses.sort();
        let mut canonical_addresses: Vec<String> = canonical_candidate
            .declarations()
            .iter()
            .map(|declaration| declaration.address().to_string())
            .collect();
        canonical_addresses.sort();
        assert_eq!(closure_addresses, canonical_addresses);
        assert!(!vibelang_dsp::synthdef_exists(
            "vibelang::main::b1_eq_synth"
        ));
        assert!(!vibelang_dsp::effect_exists("vibelang::main::b1_eq_fx"));
    }

    #[test]
    fn v2_legacy_closure_overload_discarding_builder_is_structural_migration_error() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        install_v2(&mut engine);
        let error = engine
            .eval::<Dynamic>(
                r#"define_synthdef("b1_migrate_synth", |b| { let ignored = b.param("freq", 440.0).body(|freq| dc_ar(0.0)); })"#,
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("migrate to the canonical form"),
            "{error}"
        );
        let error = engine
            .eval::<Dynamic>(
                r#"define_fx("b1_migrate_fx", |b| { let ignored = b.param("mix", 0.5).body(|input, mix| input); })"#,
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("migrate to the canonical form"),
            "{error}"
        );
        let candidate = foundation::finish_evaluation().unwrap();
        assert!(candidate.declarations().is_empty());
        assert_eq!(candidate.dsp_definitions().definitions().count(), 0);
        assert!(!vibelang_dsp::synthdef_exists("b1_migrate_synth"));
        assert!(!vibelang_dsp::synthdef_exists(
            "vibelang::main::b1_migrate_synth"
        ));
        assert!(!vibelang_dsp::effect_exists("b1_migrate_fx"));
        assert!(!vibelang_dsp::effect_exists(
            "vibelang::main::b1_migrate_fx"
        ));
    }
}
