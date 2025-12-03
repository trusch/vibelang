//! Group API for Rhai scripts.
//!
//! Groups organize voices and provide hierarchical mixing.

use crate::state::StateMessage;
use rhai::{CustomType, Engine, FnPtr, NativeCallContext, TypeBuilder};

use super::{context, require_handle};

/// Handle to a defined group.
#[derive(Debug, Clone, CustomType)]
pub struct GroupHandle {
    /// Full path to the group.
    path: String,
    /// Group name (last segment of path).
    name: String,
}

impl GroupHandle {
    /// Create a new group handle.
    pub fn new(path: String) -> Self {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        Self { path, name }
    }

    /// Get the group name.
    pub fn name(&mut self) -> String {
        self.name.clone()
    }

    /// Get the parent group path.
    pub fn parent(&mut self) -> String {
        if let Some(pos) = self.path.rfind('/') {
            self.path[..pos].to_string()
        } else {
            "main".to_string()
        }
    }

    /// Set the group gain.
    pub fn gain(self, value: f64) -> Self {
        let handle = require_handle();
        let _ = handle.send(StateMessage::SetGroupParam {
            path: self.path.clone(),
            param: "amp".to_string(),
            value: value as f32,
        });
        self
    }

    /// Mute the group.
    pub fn mute(&mut self) -> MuteBuilder {
        MuteBuilder {
            path: self.path.clone(),
        }
    }

    /// Unmute the group.
    pub fn unmute(&mut self) -> UnmuteBuilder {
        UnmuteBuilder {
            path: self.path.clone(),
        }
    }

    /// Solo the group.
    pub fn solo(self, flag: bool) -> Self {
        let handle = require_handle();
        let _ = handle.send(StateMessage::SoloGroup {
            path: self.path.clone(),
            solo: flag,
        });
        self
    }

    /// Fade gain to a target value over duration.
    pub fn fade_gain_to(self, target: f64, duration: f64) -> Self {
        let handle = require_handle();
        let _ = handle.send(StateMessage::FadeGroupParam {
            path: self.path.clone(),
            param: "amp".to_string(),
            target: target as f32,
            duration: format!("{}b", duration),
            delay: None,
            quantize: None,
        });
        self
    }

    /// Create a fade builder for a parameter.
    pub fn fade_param(&mut self, _param: String) -> ParamFadeBuilder {
        ParamFadeBuilder {
            path: self.path.clone(),
            param: _param,
            target: 0.0,
            duration: 1.0,
        }
    }

    /// Set a parameter on the group.
    pub fn set_param(&mut self, param: String, value: f64) -> ScheduledParamSetter {
        ScheduledParamSetter {
            path: self.path.clone(),
            param,
            value,
        }
    }

    /// Route this group to another group.
    pub fn route_to(self, _group: String) -> Self {
        // TODO: Implement routing
        self
    }

    /// Add a send effect.
    pub fn send(self, _send_name: String, _amount: f64) -> Self {
        // TODO: Implement sends
        self
    }

    /// Check if the group is active.
    pub fn is_active(&mut self) -> bool {
        let handle = require_handle();
        handle.with_state(|state| state.groups.contains_key(&self.path))
    }

    /// Add an effect to the group.
    pub fn add_effect(&mut self, id: String, synthdef: String, params: rhai::Map) {
        let handle = require_handle();
        let mut param_map = std::collections::HashMap::new();

        for (key, value) in params {
            if let Ok(v) = value.as_float() {
                param_map.insert(key.to_string(), v as f32);
            } else if let Ok(v) = value.as_int() {
                param_map.insert(key.to_string(), v as f32);
            }
        }

        let _ = handle.send(StateMessage::AddEffect {
            id,
            synthdef,
            group_path: self.path.clone(),
            params: param_map,
            bus_in: 0,
            bus_out: 0,
        });
    }

    /// Get an effect by ID.
    pub fn get_effect(&mut self, _id: String) -> rhai::Dynamic {
        // TODO: Return effect handle
        rhai::Dynamic::UNIT
    }

    /// Remove an effect.
    pub fn remove_effect(&mut self, id: String) {
        let handle = require_handle();
        let _ = handle.send(StateMessage::RemoveEffect { id });
    }

    /// Get all effects.
    pub fn get_effects(&mut self) -> rhai::Array {
        // TODO: Return effect list
        rhai::Array::new()
    }

    /// Clear all effects.
    pub fn clear_effects(&mut self) {
        // TODO: Implement clear effects
    }

    /// Get effect count.
    pub fn effect_count(&mut self) -> i64 {
        let handle = require_handle();
        handle.with_state(|state| {
            state
                .effects
                .values()
                .filter(|e| e.group_path == self.path)
                .count() as i64
        })
    }

    /// Add a stutter effect.
    pub fn stutter(self, _length: f64, _count: i64) -> Self {
        // TODO: Implement stutter
        self
    }

    /// Set stutter probability.
    pub fn stutter_probability(self, _prob: f64) -> Self {
        // TODO: Implement stutter probability
        self
    }

    /// Add filter sweep.
    pub fn filter_sweep(
        self,
        _filter_type: String,
        _min_freq: f64,
        _max_freq: f64,
        _duration: f64,
    ) -> Self {
        // TODO: Implement filter sweep
        self
    }
}

/// Builder for mute operation.
#[derive(Debug, Clone, CustomType)]
pub struct MuteBuilder {
    path: String,
}

impl MuteBuilder {
    /// Mute immediately.
    pub fn now(&mut self) {
        let handle = require_handle();
        let _ = handle.send(StateMessage::MuteGroup {
            path: self.path.clone(),
        });
    }

    /// Mute after a delay.
    pub fn after(&mut self, _time: String) {
        // TODO: Implement delayed mute
        self.now();
    }

    /// Mute at a specific time.
    pub fn at(&mut self, _time: SequenceTime) {
        // TODO: Implement scheduled mute
        self.now();
    }
}

/// Builder for unmute operation.
#[derive(Debug, Clone, CustomType)]
pub struct UnmuteBuilder {
    path: String,
}

impl UnmuteBuilder {
    /// Unmute immediately.
    pub fn now(&mut self) {
        let handle = require_handle();
        let _ = handle.send(StateMessage::UnmuteGroup {
            path: self.path.clone(),
        });
    }

    /// Unmute after a delay.
    pub fn after(&mut self, _time: String) {
        self.now();
    }

    /// Unmute at a specific time.
    pub fn at(&mut self, _time: SequenceTime) {
        self.now();
    }
}

/// Builder for parameter fades.
#[derive(Debug, Clone, CustomType)]
pub struct ParamFadeBuilder {
    path: String,
    param: String,
    target: f64,
    duration: f64,
}

impl ParamFadeBuilder {
    /// Set target value.
    pub fn to(mut self, value: f64) -> Self {
        self.target = value;
        self
    }

    /// Set duration.
    pub fn over(mut self, duration: String) -> Self {
        let handle = require_handle();
        let tempo = handle.with_state(|s| s.tempo);
        self.duration = super::helpers::parse_time_spec(&duration, tempo);
        self
    }

    /// Set delay.
    pub fn after(self, _delay: String) -> Self {
        // TODO: Implement delay
        self
    }

    /// Set quantization.
    pub fn at(self, _quantize: String) -> Self {
        // TODO: Implement quantization
        self
    }

    /// Set time.
    pub fn at_time(self, _time: SequenceTime) -> Self {
        // TODO: Implement time-based scheduling
        self
    }

    /// Apply the fade.
    pub fn apply(self) {
        let handle = require_handle();
        let _ = handle.send(StateMessage::FadeGroupParam {
            path: self.path,
            param: self.param,
            target: self.target as f32,
            duration: format!("{}b", self.duration),
            delay: None,
            quantize: None,
        });
    }
}

/// Builder for scheduled parameter setting.
#[derive(Debug, Clone, CustomType)]
pub struct ScheduledParamSetter {
    path: String,
    param: String,
    value: f64,
}

impl ScheduledParamSetter {
    /// Set immediately.
    pub fn now(self) {
        let handle = require_handle();
        let _ = handle.send(StateMessage::SetGroupParam {
            path: self.path,
            param: self.param,
            value: self.value as f32,
        });
    }

    /// Set after delay.
    pub fn after(self, _delay: String) {
        // TODO: Implement delayed set
        self.now();
    }

    /// Set at time.
    pub fn at(self, _time: SequenceTime) {
        // TODO: Implement scheduled set
        self.now();
    }
}

/// Sequence time reference.
#[derive(Debug, Clone, CustomType)]
pub struct SequenceTime {
    beat: f64,
}

impl SequenceTime {
    /// Get the beat value.
    pub fn beat(&mut self) -> f64 {
        self.beat
    }

    /// Add beats.
    pub fn add_beats(mut self, beats: f64) -> Self {
        self.beat += beats;
        self
    }

    /// Add time string.
    pub fn add_time_string(mut self, spec: String) -> Self {
        let handle = require_handle();
        let tempo = handle.with_state(|s| s.tempo);
        self.beat += super::helpers::parse_time_spec(&spec, tempo);
        self
    }
}

/// Define a group with a closure.
pub fn define_group(ctx: NativeCallContext, name: String, closure: FnPtr) -> GroupHandle {
    let handle = require_handle();

    // Build the full path
    let parent_path = context::current_group_path();
    let full_path = if parent_path == "main" {
        format!("main/{}", name)
    } else {
        format!("{}/{}", parent_path, name)
    };

    // Node ID will be allocated by the runtime when processing the message
    // Pass 0 as a placeholder - the runtime will assign the real ID
    let node_id = 0;

    // Create the group in the runtime
    let _ = handle.send(StateMessage::RegisterGroup {
        name: name.clone(),
        path: full_path.clone(),
        parent_path: Some(parent_path.clone()),
        node_id,
    });

    // Push group context
    context::push_group(&name);

    // Execute closure
    if let Err(e) = closure.call_within_context::<()>(&ctx, ()) {
        log::error!("Error in define_group '{}': {}", name, e);
    }

    // Pop group context
    context::pop_group();

    GroupHandle::new(full_path)
}

/// Get a group handle by path.
pub fn group(path: String) -> GroupHandle {
    let full_path = if path.starts_with("main/") || path == "main" {
        path
    } else {
        format!("{}/{}", context::current_group_path(), path)
    };
    GroupHandle::new(full_path)
}

/// Set a group's gain.
pub fn set_group_gain(path: String, value: f64) {
    let handle = require_handle();
    let full_path = if path.starts_with("main/") || path == "main" {
        path
    } else {
        format!("{}/{}", context::current_group_path(), path)
    };
    let _ = handle.send(StateMessage::SetGroupParam {
        path: full_path,
        param: "amp".to_string(),
        value: value as f32,
    });
}

/// Create the main group.
/// Main group constants
const MAIN_GROUP_NODE_ID: i32 = 1; // Fixed node ID for the main group

pub fn create_main_group() {
    let handle = require_handle();

    // First create the group in SuperCollider directly
    use crate::scsynth::{AddAction, NodeId, Target};
    if let Err(e) = handle.scsynth().g_new(
        NodeId::new(MAIN_GROUP_NODE_ID),
        AddAction::AddToTail,
        Target::root(),
    ) {
        log::error!("Failed to create main group in SuperCollider: {}", e);
        return;
    }

    // Then register it with the state manager
    let _ = handle.send(StateMessage::RegisterGroup {
        name: "main".to_string(),
        path: "main".to_string(),
        parent_path: None,
        node_id: MAIN_GROUP_NODE_ID,
    });
}

/// Register group API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register types
    engine.build_type::<GroupHandle>();
    engine.build_type::<MuteBuilder>();
    engine.build_type::<UnmuteBuilder>();
    engine.build_type::<ParamFadeBuilder>();
    engine.build_type::<ScheduledParamSetter>();
    engine.build_type::<SequenceTime>();

    // Constructors
    engine.register_fn("define_group", define_group);
    engine.register_fn("group", group);
    engine.register_fn("set_group_gain", set_group_gain);

    // GroupHandle methods
    engine.register_fn("name", GroupHandle::name);
    engine.register_fn("parent", GroupHandle::parent);
    engine.register_fn("gain", GroupHandle::gain);
    engine.register_fn("mute", GroupHandle::mute);
    engine.register_fn("unmute", GroupHandle::unmute);
    engine.register_fn("solo", GroupHandle::solo);
    engine.register_fn("fade_gain_to", GroupHandle::fade_gain_to);
    engine.register_fn("fade_param", GroupHandle::fade_param);
    engine.register_fn("set_param", GroupHandle::set_param);
    engine.register_fn("route_to", GroupHandle::route_to);
    engine.register_fn("send", GroupHandle::send);
    engine.register_fn("is_active", GroupHandle::is_active);
    engine.register_fn("add_effect", GroupHandle::add_effect);
    engine.register_fn("get_effect", GroupHandle::get_effect);
    engine.register_fn("remove_effect", GroupHandle::remove_effect);
    engine.register_fn("get_effects", GroupHandle::get_effects);
    engine.register_fn("clear_effects", GroupHandle::clear_effects);
    engine.register_fn("effect_count", GroupHandle::effect_count);

    // MuteBuilder methods
    engine.register_fn("now", MuteBuilder::now);
    engine.register_fn("after", MuteBuilder::after);
    engine.register_fn("at", MuteBuilder::at);

    // UnmuteBuilder methods
    engine.register_fn("now", UnmuteBuilder::now);
    engine.register_fn("after", UnmuteBuilder::after);
    engine.register_fn("at", UnmuteBuilder::at);

    // ParamFadeBuilder methods
    engine.register_fn("to", ParamFadeBuilder::to);
    engine.register_fn("over", ParamFadeBuilder::over);
    engine.register_fn("after", ParamFadeBuilder::after);
    engine.register_fn("at", ParamFadeBuilder::at);
    engine.register_fn("at", ParamFadeBuilder::at_time);
    engine.register_fn("apply", ParamFadeBuilder::apply);

    // ScheduledParamSetter methods
    engine.register_fn("now", ScheduledParamSetter::now);
    engine.register_fn("after", ScheduledParamSetter::after);
    engine.register_fn("at", ScheduledParamSetter::at);

    // SequenceTime methods
    engine.register_fn("beat", SequenceTime::beat);
    engine.register_fn("+", SequenceTime::add_beats);
    engine.register_fn("+", |t: SequenceTime, b: i64| t.add_beats(b as f64));
    engine.register_fn("+", SequenceTime::add_time_string);
}
