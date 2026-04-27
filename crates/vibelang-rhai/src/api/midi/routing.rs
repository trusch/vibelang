//! MIDI routing builders - KeyboardRoute, NoteRoute, CcRoute.

use rhai::{CustomType, TypeBuilder};
use vibelang_core::midi::{
    CcRouteBuilder, KeyboardRouteBuilder, NoteRouteBuilder, ParameterCurve, VelocityCurve,
};
use vibelang_core::types::MidiDeviceId;

use crate::api::voice::Voice;
use crate::context;

// ============================================================================
// Advanced Keyboard Route Builder
// ============================================================================

/// Builder for advanced keyboard routing.
///
/// Supports note range filtering, transpose, and velocity curves.
#[derive(Debug, Clone, CustomType)]
pub struct KeyboardRoute {
    device_id: MidiDeviceId,
    builder: KeyboardRouteBuilder,
}

impl KeyboardRoute {
    /// Create a new KeyboardRoute.
    pub fn new(device_id: MidiDeviceId, builder: KeyboardRouteBuilder) -> Self {
        Self { device_id, builder }
    }

    /// Filter to a specific MIDI channel (1-16).
    pub fn channel(mut self, channel: i64) -> Self {
        self.builder = self.builder.channel(channel.clamp(1, 16) as u8);
        self
    }

    /// Set the note range using MIDI note numbers.
    pub fn range_midi(mut self, min: i64, max: i64) -> Self {
        self.builder = self
            .builder
            .range(min.clamp(0, 127) as u8, max.clamp(0, 127) as u8);
        self
    }

    /// Set the note range using note names (e.g., "C2", "C6").
    pub fn range(mut self, min: String, max: String) -> Self {
        self.builder = self.builder.range_notes(&min, &max);
        self
    }

    /// Transpose by semitones.
    pub fn transpose(mut self, semitones: i64) -> Self {
        self.builder = self.builder.transpose(semitones.clamp(-128, 127) as i8);
        self
    }

    /// Transpose by octaves.
    pub fn octave(mut self, octaves: i64) -> Self {
        self.builder = self.builder.octave(octaves.clamp(-10, 10) as i8);
        self
    }

    /// Set the velocity curve.
    ///
    /// Available curves: "linear", "soft", "hard", "exponential", "compressed"
    pub fn velocity(mut self, curve_name: String) -> Self {
        self.builder = self.builder.velocity_curve_name(&curve_name);
        self
    }

    /// Set the velocity curve.
    ///
    /// **Deprecated**: Use `velocity(name)` instead.
    #[deprecated(note = "Use velocity() instead")]
    pub fn velocity_curve(self, curve_name: String) -> Self {
        self.velocity(curve_name)
    }

    /// Set a fixed velocity (0-127).
    pub fn fixed_velocity(mut self, velocity: i64) -> Self {
        self.builder.velocity_curve = VelocityCurve::Fixed(velocity.clamp(0, 127) as u8);
        self
    }

    /// Route to a voice and apply the configuration.
    pub fn to(self, voice: Voice) -> Self {
        self.apply_route(&voice.name);
        self
    }

    /// Route to a voice by name.
    pub fn to_name(self, voice_name: String) -> Self {
        self.apply_route(&voice_name);
        self
    }

    /// Internal helper to apply the keyboard route configuration.
    fn apply_route(&self, voice_name: &str) {
        use vibelang_core::reload::AdvancedMidiKeyboardRoute;

        let voice_id = context::get_or_create_voice_id(voice_name);

        let route = AdvancedMidiKeyboardRoute {
            device_id: self.device_id,
            channel: self.builder.channel,
            note_min: self.builder.note_min,
            note_max: self.builder.note_max,
            transpose: self.builder.transpose,
            velocity_curve: match self.builder.velocity_curve {
                VelocityCurve::Linear => "linear".to_string(),
                VelocityCurve::Soft => "soft".to_string(),
                VelocityCurve::Hard => "hard".to_string(),
                VelocityCurve::Exponential => "exponential".to_string(),
                VelocityCurve::Compressed => "compressed".to_string(),
                VelocityCurve::Fixed(v) => format!("fixed:{}", v),
            },
            voice: voice_id,
        };

        context::with_state(|state| {
            state.advanced_keyboard_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }
}

// ============================================================================
// Advanced Note Route Builder (for drums/pads)
// ============================================================================

/// Builder for advanced note routing (drums/pads).
///
/// Maps a single MIDI note to a voice with optional choke groups and velocity mapping.
#[derive(Debug, Clone, CustomType)]
pub struct NoteRoute {
    device_id: MidiDeviceId,
    builder: NoteRouteBuilder,
}

impl NoteRoute {
    /// Create a new NoteRoute.
    pub fn new(device_id: MidiDeviceId, builder: NoteRouteBuilder) -> Self {
        Self { device_id, builder }
    }

    /// Filter to a specific MIDI channel (1-16).
    pub fn channel(mut self, channel: i64) -> Self {
        self.builder = self.builder.channel(channel.clamp(1, 16) as u8);
        self
    }

    /// Set choke group (notes in same group stop each other).
    pub fn choke(mut self, group: String) -> Self {
        self.builder = self.builder.choke_group(&group);
        self
    }

    /// Set choke group (notes in same group stop each other).
    ///
    /// **Deprecated**: Use `choke(name)` instead.
    #[deprecated(note = "Use choke() instead")]
    pub fn choke_group(self, group: String) -> Self {
        self.choke(group)
    }

    /// Map velocity to a parameter.
    ///
    /// # Arguments
    /// * `param` - Parameter name
    /// * `min` - Value at velocity 0
    /// * `max` - Value at velocity 127
    pub fn velocity_to(mut self, param: String, min: f64, max: f64) -> Self {
        self.builder = self.builder.velocity_to(&param, min as f32, max as f32);
        self
    }

    /// Set a fixed velocity (0-127), ignoring how hard the pad is hit.
    pub fn fixed_velocity(mut self, velocity: i64) -> Self {
        self.builder.velocity_curve = VelocityCurve::Fixed(velocity.clamp(0, 127) as u8);
        self
    }

    /// Route to a voice and apply the configuration.
    pub fn to(self, voice: Voice) -> Self {
        use vibelang_core::reload::AdvancedMidiNoteRoute;

        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = AdvancedMidiNoteRoute {
            device_id: self.device_id,
            source_note: self.builder.source_note,
            channel: self.builder.channel,
            choke_group: self.builder.choke_group.clone(),
            velocity_param: self
                .builder
                .velocity_mapping
                .as_ref()
                .map(|m| m.param.clone()),
            velocity_min: self.builder.velocity_mapping.as_ref().map(|m| m.min),
            velocity_max: self.builder.velocity_mapping.as_ref().map(|m| m.max),
            voice: voice_id,
        };

        context::with_state(|state| {
            state.advanced_note_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
        self
    }
}

// ============================================================================
// Advanced CC Route Builder
// ============================================================================

/// Builder for advanced CC routing with curves.
///
/// Maps a CC to a voice parameter with configurable curves and ranges.
#[derive(Debug, Clone, CustomType)]
pub struct CcRoute {
    device_id: MidiDeviceId,
    builder: CcRouteBuilder,
}

impl CcRoute {
    /// Create a new CcRoute.
    pub fn new(device_id: MidiDeviceId, builder: CcRouteBuilder) -> Self {
        Self { device_id, builder }
    }

    /// Filter to a specific MIDI channel (1-16).
    pub fn channel(mut self, channel: i64) -> Self {
        self.builder = self.builder.channel(channel.clamp(1, 16) as u8);
        self
    }

    /// Set the parameter curve.
    ///
    /// Available curves: "linear", "logarithmic", "exponential"
    pub fn curve(mut self, curve_name: String) -> Self {
        self.builder = self.builder.curve_name(&curve_name);
        self
    }

    /// Route to a voice parameter with range.
    ///
    /// # Arguments
    /// * `voice` - Target voice
    /// * `param` - Parameter name
    /// * `min` - Value when CC is 0
    /// * `max` - Value when CC is 127
    pub fn to_param(self, voice: Voice, param: String, min: f64, max: f64) {
        self.apply_route(&voice.name, param, min, max);
    }

    /// Route to a voice parameter by name with range.
    pub fn to_param_name(self, voice_name: String, param: String, min: f64, max: f64) {
        self.apply_route(&voice_name, param, min, max);
    }

    /// Internal helper to apply the CC route configuration.
    fn apply_route(self, voice_name: &str, param: String, min: f64, max: f64) {
        use vibelang_core::reload::AdvancedMidiCcRoute;
        use vibelang_core::traits::FadeTarget;

        let voice_id = context::get_or_create_voice_id(voice_name);

        let route = AdvancedMidiCcRoute {
            device_id: self.device_id,
            cc: self.builder.cc,
            channel: self.builder.channel,
            curve: match self.builder.curve {
                ParameterCurve::Linear => "linear".to_string(),
                ParameterCurve::Logarithmic => "logarithmic".to_string(),
                ParameterCurve::Exponential => "exponential".to_string(),
            },
            target: FadeTarget::Voice(voice_id),
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
