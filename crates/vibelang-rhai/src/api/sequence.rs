//! Sequence, Fade, and Fx API for Rhai scripts.
//!
//! Sequences arrange patterns, melodies, fades, and other sequences on a timeline.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use vibelang_core2::traits::{Clip, FadeConfig, FadeCurve, SequenceConfig};
use vibelang_core2::types::Beat;
use vibelang_core2::reload::EffectConfig;

use crate::context;
use super::pattern::Pattern;
use super::melody::Melody;
use super::voice::Voice;

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
        r.borrow_mut().insert(sequence.name.clone(), sequence.clone());
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
            self.clips.push(ClipInfo::Fade {
                fade: f,
                start,
            });
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
                    use vibelang_core2::traits::FadeTarget;
                    use vibelang_core2::types::Duration;

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
    /// This schedules the sequence to start at the next quantization boundary.
    /// Uses the global quantization setting.
    pub fn launch(&mut self) {
        // For now, launch behaves the same as start.
        // The runtime will use the quantization setting to determine when to actually start.
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
        context::with_state(|state| {
            state.playing_sequences.contains(&sequence_id)
        })
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
    pub fn on_voice_handle(mut self, voice: Voice) -> Self {
        self.target_type = FadeTargetType::Voice;
        self.target_name = voice.name.clone();
        self
    }

    /// Target an effect.
    pub fn on_effect(mut self, effect_name: String) -> Self {
        self.target_type = FadeTargetType::Effect;
        self.target_name = effect_name;
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
        self.duration_beats = beats;
        self
    }

    /// Set duration in bars.
    pub fn over_bars(mut self, bars: i64) -> Self {
        self.duration_beats = bars as f64 * 4.0;
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

    /// Set a parameter.
    pub fn param(mut self, key: String, value: f64) -> Self {
        self.params.insert(key, value as f32);
        self
    }

    /// Apply the effect to the current group.
    pub fn apply(self) {
        let effect_id = context::get_or_create_effect_id(&self.id);
        let group_id = context::get_or_create_group_id(&self.group_path);

        let config = EffectConfig {
            synthdef: self.synth_name.unwrap_or_default(),
            group: group_id,
            params: self.params,
        };

        context::with_state(|state| {
            state.effects.insert(effect_id, config);

            // Also add to the group's effects list
            if let Some(group_config) = state.groups.get_mut(&group_id) {
                group_config.effects.push(effect_id);
            }
        });
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

    // Constructors
    engine.register_fn("sequence", sequence);
    engine.register_fn("fade", fade);
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
    engine.register_fn("launch", Sequence::launch);
    engine.register_fn("stop", Sequence::stop);
    engine.register_fn("is_playing", Sequence::is_playing);
    engine.register_get("playing", Sequence::is_playing);
    engine.register_get("name", |s: &mut Sequence| s.name.clone());

    // Fade builder methods
    engine.register_fn("on_group", Fade::on_group);
    engine.register_fn("on_voice", Fade::on_voice);
    engine.register_fn("on_voice", Fade::on_voice_handle);  // Overload for Voice handle
    engine.register_fn("on_effect", Fade::on_effect);
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

    // Fx builder methods
    engine.register_fn("synth", Fx::synth);
    engine.register_fn("param", Fx::param);
    engine.register_fn("apply", Fx::apply);
}
