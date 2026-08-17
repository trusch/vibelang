//! CcMapping builder — polymorphic MIDI CC routing to Voice, Group, Fx, or String targets.

use rhai::{CustomType, Dynamic, EvalAltResult, Position, TypeBuilder};
use vibelang_core::midi::canonical_cc_curve_name;
use vibelang_core::traits::FadeTarget;
use vibelang_core::types::MidiDeviceId;

use crate::api::group::GroupHandle;
use crate::api::sequence::Fx;
use crate::api::voice::Voice;
use crate::context;

/// Builder for MIDI CC routing with a polymorphic target.
///
/// Supports routing a CC to a Voice, Group, Fx, or a string name.
/// The target is resolved at `.to()` call time.
#[derive(Debug, Clone, CustomType)]
pub struct CcMapping {
    device_id: MidiDeviceId,
    cc: u8,
    channel: Option<u8>,
    group: Option<u8>,
    curve: String,
}

impl CcMapping {
    /// Create a new CcMapping for the given device and CC number.
    pub fn new(device_id: MidiDeviceId, cc: u8) -> Self {
        Self {
            device_id,
            cc,
            channel: None,
            group: None,
            curve: "linear".to_string(),
        }
    }

    /// Filter to a specific MIDI channel (1-16, stored as 0-15).
    pub fn channel(mut self, ch: i64) -> Self {
        self.channel = Some((ch.clamp(1, 16) - 1) as u8);
        self
    }

    /// Filter to a specific UMP group (0-15).
    pub fn group(mut self, group: i64) -> Self {
        self.group = Some(group.clamp(0, 15) as u8);
        self
    }

    /// Set the parameter curve.
    ///
    /// Available curves: linear, log/logarithmic, exp/exponential, and s-curve aliases.
    pub fn curve(mut self, name: String) -> Self {
        self.curve = match canonical_cc_curve_name(&name) {
            Some(curve) => curve.to_string(),
            None => {
                tracing::warn!("Unknown MIDI CC curve '{}'; falling back to linear", name);
                "linear".to_string()
            }
        };
        self
    }

    /// Route this CC to a target parameter with range.
    ///
    /// # Arguments
    /// * `target` - Voice, GroupHandle, Fx, or String name
    /// * `param` - Parameter name
    /// * `min` - Value when CC is 0
    /// * `max` - Value when CC is 127
    pub fn to(self, target: Dynamic, param: String, min: f64, max: f64) {
        use vibelang_core::reload::AdvancedMidiCcRoute;

        let fade_target = if let Some(voice) = target.clone().try_cast::<Voice>() {
            let id = context::get_or_create_voice_id(&voice.name);
            FadeTarget::Voice(id)
        } else if let Some(handle) = target.clone().try_cast::<GroupHandle>() {
            let id = context::get_or_create_group_id(&handle.path);
            FadeTarget::Group(id)
        } else if let Some(fx) = target.clone().try_cast::<Fx>() {
            let id = context::get_or_create_effect_id(&fx.id);
            FadeTarget::Effect(id)
        } else if target.is_string() {
            let name = target.into_string().unwrap_or_default();
            if let Some(id) = context::get_voice_id(&name) {
                FadeTarget::Voice(id)
            } else if let Some(id) = context::get_group_id(&name) {
                FadeTarget::Group(id)
            } else {
                // Default to Voice — will be created on demand
                let id = context::get_or_create_voice_id(&name);
                FadeTarget::Voice(id)
            }
        } else {
            tracing::warn!(
                "CcMapping.to(): unsupported target type '{}', defaulting to Voice",
                target.type_name()
            );
            let id = context::get_or_create_voice_id("unknown");
            FadeTarget::Voice(id)
        };

        let route = AdvancedMidiCcRoute {
            device_id: self.device_id,
            cc: self.cc,
            channel: self.channel,
            group: self.group,
            curve: self.curve,
            target: fade_target,
            param,
            min: min as f32,
            max: max as f32,
        };

        context::with_state(|state| {
            state.advanced_cc_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }
}
