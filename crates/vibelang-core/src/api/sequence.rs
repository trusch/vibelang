//! Sequence API for Rhai scripts.
//!
//! Sequences arrange patterns, melodies, fades, and other sequences
//! on a timeline for structured musical composition.

use crate::sequences::{ClipMode, ClipSource, FadeDefinition, SequenceClip, SequenceDefinition};
use crate::state::StateMessage;
use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::ops::Range;

use super::context::{self, SourceLocation};
use super::require_handle;

/// A Sequence builder for creating timeline arrangements.
#[derive(Debug, Clone, CustomType)]
pub struct Sequence {
    /// Sequence name.
    pub name: String,
    /// Loop length in beats.
    loop_beats: f64,
    /// Clips in the sequence.
    clips: Vec<(f64, f64, ClipSource, ClipMode)>,
    /// Group path.
    group_path: String,
    /// Source location where this sequence was defined.
    source_location: SourceLocation,
}

impl Sequence {
    /// Create a new sequence with the given name and source location from NativeCallContext.
    pub fn new(ctx: NativeCallContext, name: String) -> Self {
        let pos = ctx.call_position();
        let source_location = SourceLocation::new(
            context::get_current_script_file(),
            if pos.is_none() { None } else { pos.line().map(|l| l as u32) },
            if pos.is_none() { None } else { pos.position().map(|c| c as u32) },
        );
        Self {
            name,
            loop_beats: 16.0,
            clips: Vec::new(),
            group_path: context::current_group_path(),
            source_location,
        }
    }

    // === Builder methods ===

    /// Set the loop length in bars.
    pub fn loop_bars(mut self, bars: f64) -> Self {
        // Assuming 4 beats per bar
        self.loop_beats = bars * 4.0;
        self
    }

    /// Set the loop length in bars (integer version).
    pub fn loop_bars_int(mut self, bars: i64) -> Self {
        self.loop_beats = bars as f64 * 4.0;
        self
    }

    /// Set the loop length in beats.
    pub fn loop_beats(mut self, beats: f64) -> Self {
        self.loop_beats = beats;
        self
    }

    /// Set the loop length in beats (integer version).
    pub fn loop_beats_int(mut self, beats: i64) -> Self {
        self.loop_beats = beats as f64;
        self
    }

    /// Add a clip from a Pattern.
    pub fn clip_pattern(mut self, range: Range<f64>, mut pattern: super::pattern::Pattern) -> Self {
        // Apply the pattern to register it
        pattern.apply();
        self.clips.push((
            range.start,
            range.end,
            ClipSource::Pattern(pattern.name.clone()),
            ClipMode::Loop,
        ));
        self
    }

    /// Add a clip from a Melody.
    pub fn clip_melody(mut self, range: Range<f64>, mut melody: super::melody::Melody) -> Self {
        // Apply the melody to register it
        melody.apply();
        self.clips.push((
            range.start,
            range.end,
            ClipSource::Melody(melody.name.clone()),
            ClipMode::Loop,
        ));
        self
    }

    /// Add a clip from a Fade.
    pub fn clip_fade(mut self, range: Range<f64>, mut fade: Fade) -> Self {
        // First register the fade
        fade.apply();
        self.clips.push((
            range.start,
            range.end,
            ClipSource::Fade(fade.name.clone()),
            ClipMode::Once,
        ));
        self
    }

    /// Add a clip from another Sequence.
    pub fn clip_sequence(mut self, range: Range<f64>, seq: Sequence) -> Self {
        self.clips.push((
            range.start,
            range.end,
            ClipSource::Sequence(seq.name.clone()),
            ClipMode::Loop,
        ));
        self
    }

    /// Add a clip by name (generic version).
    pub fn clip_name(mut self, range: Range<f64>, name: String) -> Self {
        // Detect type by checking if pattern, melody, fade, or sequence exists
        // Default to pattern for now
        self.clips.push((
            range.start,
            range.end,
            ClipSource::Pattern(name),
            ClipMode::Loop,
        ));
        self
    }

    /// Add a clip from a Range and a Clip source (Dynamic).
    pub fn clip_dynamic(mut self, range: rhai::Dynamic, source: rhai::Dynamic) -> Self {
        // Try to get the range - it's passed as a Range<i64> from Rhai
        let (start, end) = if let Some(r) = range.clone().try_cast::<std::ops::Range<i64>>() {
            (r.start as f64, r.end as f64)
        } else if let Ok(arr) = range.into_array() {
            if arr.len() >= 2 {
                let s = arr[0].as_int().unwrap_or(0) as f64;
                let e = arr[1].as_int().unwrap_or(0) as f64;
                (s, e)
            } else {
                return self;
            }
        } else {
            return self;
        };

        // Detect source type
        if let Some(mut p) = source.clone().try_cast::<super::pattern::Pattern>() {
            p.apply();
            self.clips.push((start, end, ClipSource::Pattern(p.name.clone()), ClipMode::Loop));
        } else if let Some(mut m) = source.clone().try_cast::<super::melody::Melody>() {
            m.apply();
            self.clips.push((start, end, ClipSource::Melody(m.name.clone()), ClipMode::Loop));
        } else if let Some(mut f) = source.clone().try_cast::<Fade>() {
            f.apply();
            self.clips.push((start, end, ClipSource::Fade(f.name.clone()), ClipMode::Once));
        } else if let Some(s) = source.clone().try_cast::<Sequence>() {
            self.clips.push((start, end, ClipSource::Sequence(s.name.clone()), ClipMode::Loop));
        } else if let Ok(name) = source.into_immutable_string() {
            self.clips.push((start, end, ClipSource::Pattern(name.to_string()), ClipMode::Loop));
        }

        self
    }

    // === Actions ===

    /// Register and apply the sequence - internal version
    fn do_apply(&self) {
        let handle = require_handle();

        let clips: Vec<SequenceClip> = self.clips.iter().map(|(start, end, source, mode)| {
            SequenceClip::new(*start, *end, source.clone(), mode.clone())
        }).collect();

        let def = SequenceDefinition {
            name: self.name.clone(),
            loop_beats: self.loop_beats,
            clips,
            generation: 0,
            play_once: false,
            source_location: self.source_location.clone(),
        };

        let _ = handle.send(StateMessage::CreateSequence {
            sequence: def,
        });
    }

    /// Register and apply the sequence.
    pub fn apply(self) -> Self {
        self.do_apply();
        self
    }

    /// Start the sequence playing.
    pub fn start(&mut self) {
        self.do_apply();
        let handle = require_handle();
        log::info!("Sending StartSequence message for '{}'", self.name);
        let _ = handle.send(StateMessage::StartSequence {
            name: self.name.clone(),
        });
    }

    /// Start the sequence playing once (no loop).
    pub fn start_once(&mut self) {
        self.do_apply();
        let handle = require_handle();
        log::info!("Sending StartSequenceOnce message for '{}'", self.name);
        let _ = handle.send(StateMessage::StartSequenceOnce {
            name: self.name.clone(),
        });
    }

    /// Stop the sequence.
    pub fn stop(&mut self) {
        let handle = require_handle();
        let _ = handle.send(StateMessage::StopSequence {
            name: self.name.clone(),
        });
    }

    /// Launch the sequence (start if not playing).
    pub fn launch(&mut self) {
        self.start();
    }
}

/// A Fade builder for creating parameter automation.
#[derive(Debug, Clone, CustomType)]
pub struct Fade {
    /// Fade name.
    pub name: String,
    /// Target type (group, voice, effect).
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

    // === Builder methods ===

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

    /// Set duration in bars (assuming 4 beats per bar).
    pub fn over_bars(mut self, bars: i64) -> Self {
        self.duration_beats = bars as f64 * 4.0;
        self
    }

    // === Actions ===

    /// Register and apply the fade definition.
    pub fn apply(&mut self) {
        let handle = require_handle();

        let target_type = match self.target_type {
            FadeTargetType::Group => crate::events::FadeTargetType::Group,
            FadeTargetType::Voice => crate::events::FadeTargetType::Voice,
            FadeTargetType::Effect => crate::events::FadeTargetType::Effect,
        };

        let def = FadeDefinition::new(
            &self.name,
            target_type,
            &self.target_name,
            &self.param_name,
        )
        .with_range(self.from_value as f32, self.to_value as f32)
        .with_duration(self.duration_beats);

        let _ = handle.send(StateMessage::CreateFadeDefinition {
            fade: def,
        });
    }

    /// Start the fade immediately.
    pub fn start(&mut self) {
        self.apply();
        // For now, starting is handled by sequences that include this fade
        // Immediate fades could use a different mechanism
    }
}

/// An FX builder for creating audio effects.
#[derive(Debug, Clone, CustomType)]
pub struct Fx {
    /// Effect ID.
    pub id: String,
    /// Synthdef name.
    synth_name: Option<String>,
    /// Parameters.
    params: std::collections::HashMap<String, f64>,
    /// Group path.
    group_path: String,
    /// Source location where this effect was defined.
    source_location: SourceLocation,
}

impl Fx {
    /// Create a new effect with the given ID and source location from NativeCallContext.
    pub fn new(ctx: NativeCallContext, id: String) -> Self {
        let pos = ctx.call_position();
        let source_location = SourceLocation::new(
            context::get_current_script_file(),
            if pos.is_none() { None } else { pos.line().map(|l| l as u32) },
            if pos.is_none() { None } else { pos.position().map(|c| c as u32) },
        );
        Self {
            id,
            synth_name: None,
            params: std::collections::HashMap::new(),
            group_path: context::current_group_path(),
            source_location,
        }
    }

    // === Builder methods ===

    /// Set the synthdef for this effect.
    pub fn synth(mut self, synth_name: String) -> Self {
        self.synth_name = Some(synth_name);
        self
    }

    /// Set a parameter.
    pub fn param(mut self, key: String, value: f64) -> Self {
        self.params.insert(key, value);
        self
    }

    // === Actions ===

    /// Apply the effect to the current group.
    pub fn apply(self) {
        let handle = require_handle();

        let params: std::collections::HashMap<String, f32> = self
            .params
            .iter()
            .map(|(k, v)| (k.clone(), *v as f32))
            .collect();

        let _ = handle.send(StateMessage::AddEffect {
            id: self.id,
            synthdef: self.synth_name.unwrap_or_default(),
            group_path: self.group_path,
            params,
            bus_in: 0,
            bus_out: 0,
            source_location: self.source_location.clone(),
        });
    }
}

/// Create a new sequence builder with source location tracking.
pub fn sequence(ctx: NativeCallContext, name: String) -> Sequence {
    Sequence::new(ctx, name)
}

/// Create a new fade builder.
pub fn fade(name: String) -> Fade {
    Fade::new(name)
}

/// Create a new fx builder with source location tracking.
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
    engine.register_fn("loop_beats", Sequence::loop_beats);
    engine.register_fn("loop_beats", Sequence::loop_beats_int);
    engine.register_fn("clip", Sequence::clip_dynamic);
    engine.register_fn("clip", Sequence::clip_pattern);
    engine.register_fn("clip", Sequence::clip_melody);
    engine.register_fn("clip", Sequence::clip_fade);
    engine.register_fn("clip", Sequence::clip_sequence);
    engine.register_fn("clip", Sequence::clip_name);

    // Sequence actions
    engine.register_fn("apply", Sequence::apply);
    engine.register_fn("start", Sequence::start);
    engine.register_fn("start_once", Sequence::start_once);
    engine.register_fn("stop", Sequence::stop);
    engine.register_fn("launch", Sequence::launch);
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

    // Fade actions
    engine.register_fn("apply", Fade::apply);
    engine.register_fn("start", Fade::start);

    // Fx builder methods
    engine.register_fn("synth", Fx::synth);
    engine.register_fn("param", Fx::param);

    // Fx actions
    engine.register_fn("apply", Fx::apply);
}
