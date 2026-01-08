//! Sequence, Fade, and Fx API for Rhai scripts.
//!
//! Sequences arrange patterns, melodies, fades, and other sequences on a timeline.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::collections::HashMap;
use std::ops::Range;
use vibelang_core2::traits::{Clip, FadeConfig, SequenceConfig};
use vibelang_core2::types::Beat;
use vibelang_core2::reload::EffectConfig;

use crate::context;
use super::pattern::Pattern;
use super::melody::Melody;

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
    Fade { _name: String, start: f64 },
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
            _name: fade.name.clone(),
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
            return self;
        };

        if let Some(p) = source.clone().try_cast::<Pattern>() {
            self.clips.push(ClipInfo::Pattern {
                name: p.name.clone(),
                start,
                end,
            });
        } else if let Some(m) = source.clone().try_cast::<Melody>() {
            self.clips.push(ClipInfo::Melody {
                name: m.name.clone(),
                start,
                end,
            });
        } else if let Some(f) = source.clone().try_cast::<Fade>() {
            self.clips.push(ClipInfo::Fade {
                _name: f.name.clone(),
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
                ClipInfo::Fade { _name: _, start } => {
                    // Look up the fade config by name
                    // For now, create a placeholder - the actual config would be in the fades map
                    Clip::Fade {
                        config: FadeConfig::group(
                            context::get_or_create_group_id("main"),
                            "amp",
                            1.0,
                            4.0,
                        ),
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
        }
    }

    /// Target a group.
    pub fn on_group(mut self, group_name: String) -> Self {
        self.target_type = FadeTargetType::Group;
        self.target_name = group_name;
        self
    }

    /// Target a voice.
    pub fn on_voice(mut self, voice_name: String) -> Self {
        self.target_type = FadeTargetType::Voice;
        self.target_name = voice_name;
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

/// Create a new sequence builder.
pub fn sequence(ctx: NativeCallContext, name: String) -> Sequence {
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
    engine.register_fn("on_effect", Fade::on_effect);
    engine.register_fn("param", Fade::param);
    engine.register_fn("from", Fade::from);
    engine.register_fn("to", Fade::to);
    engine.register_fn("over", Fade::over);
    engine.register_fn("over_bars", Fade::over_bars);
    engine.register_fn("apply", Fade::apply);

    // Fx builder methods
    engine.register_fn("synth", Fx::synth);
    engine.register_fn("param", Fx::param);
    engine.register_fn("apply", Fx::apply);
}
