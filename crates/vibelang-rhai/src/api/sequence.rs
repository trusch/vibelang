//! Sequence, Fade, and Fx API for Rhai scripts.
//!
//! Sequences arrange patterns, melodies, fades, and other sequences on a timeline.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use vibelang_core::handlers::ParamRouteTarget;
use vibelang_core::reload::EffectConfig;
use vibelang_core::traits::{Clip, FadeConfig, FadeCurve, FadeTarget, SequenceConfig};
use vibelang_core::types::{Beat, Duration};

use super::melody::{Melody, MelodyBuilder, MelodyRef};
use super::pattern::{Pattern, PatternBuilder, PatternRef};
use super::route::ParamHandle;
use super::voice::Voice;
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
    AuthoringDeclaration, Cancellation, CandidateError, CandidateFragment, Composition,
    DeclarationOwner, DeclarationPayload, DesiredLifecycle, FadeKind, GroupScope, LifecycleAction,
    LifecycleMetadata, SequenceAuthoring, SequenceClipAuthoring, SequenceContentAuthoring,
    SequenceKind, StartMode, TerminalEffect,
};

use crate::foundation::{self, BuilderBase, FoundationError, Observation, RefBase};

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
    Fade(RefBase),
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
            Self::Fade(reference) => reference,
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
            Self::Fade(reference) => SequenceContentAuthoring::Fade(reference.typed()?),
            Self::Sequence { reference, .. } => {
                SequenceContentAuthoring::Sequence(reference.base().typed()?)
            }
        })
    }

    fn take_fragment(&mut self) -> Option<CandidateFragment> {
        match self {
            Self::Pattern { fragment, .. }
            | Self::Melody { fragment, .. }
            | Self::Sequence { fragment, .. } => fragment.take(),
            Self::Fade(_) => None,
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

    pub fn clip_fade(self, range: Range<f64>, reference: RefBase) -> Result<Self, FoundationError> {
        reference.typed::<FadeKind>()?;
        self.push_clip(range, SequenceClipContentV2::Fade(reference))
    }

    pub fn clip_fade_int(
        self,
        range: Range<i64>,
        reference: RefBase,
    ) -> Result<Self, FoundationError> {
        self.clip_fade(range.start as f64..range.end as f64, reference)
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

#[cfg(test)]
fn install_v2_for_tests(engine: &mut Engine) {
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
            |builder: SequenceBuilder, range: Range<f64>, reference: RefBase| {
                builder
                    .clip_fade(range, reference)
                    .map_err(|error| sequence_v2_error(error, Position::NONE))
            },
        )
        .register_fn(
            "clip",
            |builder: SequenceBuilder, range: Range<i64>, reference: RefBase| {
                builder
                    .clip_fade_int(range, reference)
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
        AuthoringDeclaration, ContractDigest, DeclarationOwner, EngineInstanceId,
        EvaluationIdentity, LanguageContract, VoiceKind,
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
        let fade = foundation::authoring_ref::<FadeKind>("fade", GroupScope::root()).unwrap();
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
            SequenceClipContentV2::Fade(_)
        ));
        assert!(matches!(
            &builder.clips[3].content,
            SequenceClipContentV2::Sequence { .. }
        ));
        let wrong = foundation::authoring_ref::<VoiceKind>("voice", GroupScope::root()).unwrap();
        assert!(matches!(
            SequenceBuilder::new(
                foundation::authoring_builder::<SequenceKind>("wrong", GroupScope::root()).unwrap()
            )
            .clip_fade(0.0..1.0, wrong),
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
}
