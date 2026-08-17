//! MIDI 2.0 routing builders (feature-gated).

use rhai::{CustomType, Dynamic, EvalAltResult, Position, TypeBuilder};
use vibelang_core::midi::{canonical_cc_curve_name, parse_note_name};
use vibelang_core::reload::{
    AdvancedMidiCcRoute, Midi2KeyboardRoute, Midi2PerNoteControllerType, Midi2PerNoteRoute,
};
use vibelang_core::traits::FadeTarget;
use vibelang_core::types::MidiDeviceId;

use crate::api::voice::Voice;
use crate::context;

// ============================================================================
// MIDI 2.0 Group Route Builder
// ============================================================================

/// Builder for MIDI 2.0 group-based routing.
///
/// Routes MIDI messages from a specific UMP group (0-15) to a voice.
#[derive(Debug, Clone, CustomType)]
pub struct GroupRoute {
    device_id: MidiDeviceId,
    group: u8,
    channel: Option<u8>,
    note_min: u8,
    note_max: u8,
    transpose: i8,
    velocity_curve: String,
}

impl GroupRoute {
    /// Create a new GroupRoute.
    pub fn new(device_id: MidiDeviceId, group: u8) -> Self {
        Self {
            device_id,
            group,
            channel: None,
            note_min: 0,
            note_max: 127,
            transpose: 0,
            velocity_curve: "linear".to_string(),
        }
    }

    /// Filter to a specific MIDI channel (1-16) within this group.
    pub fn channel(mut self, channel: i64) -> Self {
        self.channel = Some((channel.clamp(1, 16) - 1) as u8); // User-facing: 1-16, internal: 0-15
        self
    }

    /// Set the note range using MIDI note numbers.
    pub fn range_midi(mut self, min: i64, max: i64) -> Self {
        self.note_min = min.clamp(0, 127) as u8;
        self.note_max = max.clamp(0, 127) as u8;
        self
    }

    /// Set the note range using note names (e.g., "C2", "C6").
    pub fn range(mut self, min: String, max: String) -> Self {
        self.note_min = parse_note_name(&min).unwrap_or(0);
        self.note_max = parse_note_name(&max).unwrap_or(127);
        self
    }

    /// Transpose by semitones.
    pub fn transpose(mut self, semitones: i64) -> Self {
        self.transpose = semitones.clamp(-128, 127) as i8;
        self
    }

    /// Set the velocity curve.
    pub fn velocity_curve(mut self, curve_name: String) -> Self {
        self.velocity_curve = curve_name;
        self
    }

    /// Route this group to a voice.
    pub fn route_to(self, voice: Voice) {
        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = Midi2KeyboardRoute {
            device_id: self.device_id,
            group: Some(self.group),
            channel: self.channel,
            note_min: self.note_min,
            note_max: self.note_max,
            transpose: self.transpose,
            velocity_curve: self.velocity_curve,
            voice: voice_id,
        };

        context::with_state(|state| {
            state.midi2_keyboard_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }

    /// Route this group to a voice by name.
    pub fn route_to_name(self, voice_name: String) {
        let voice_id = context::get_or_create_voice_id(&voice_name);

        let route = Midi2KeyboardRoute {
            device_id: self.device_id,
            group: Some(self.group),
            channel: self.channel,
            note_min: self.note_min,
            note_max: self.note_max,
            transpose: self.transpose,
            velocity_curve: self.velocity_curve,
            voice: voice_id,
        };

        context::with_state(|state| {
            state.midi2_keyboard_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }
}

// ============================================================================
// MIDI 2.0 Per-Note Pitch Bend Builder
// ============================================================================

/// Builder for per-note pitch bend routing.
#[derive(Debug, Clone, CustomType)]
pub struct PerNotePitchBendBuilder {
    device_id: MidiDeviceId,
    group: Option<u8>,
    channel: Option<u8>,
    range: u8,
}

impl PerNotePitchBendBuilder {
    /// Create a new PerNotePitchBendBuilder.
    pub fn new(device_id: MidiDeviceId) -> Self {
        Self {
            device_id,
            group: None,
            channel: None,
            range: 48, // Default MIDI 2.0 range
        }
    }

    /// Filter to a specific UMP group (0-15).
    pub fn group(mut self, group: i64) -> Self {
        self.group = Some(group.clamp(0, 15) as u8);
        self
    }

    /// Filter to a specific MIDI channel (1-16). Internally stored as 0-15.
    pub fn channel(mut self, channel: i64) -> Self {
        self.channel = Some((channel.clamp(1, 16) - 1) as u8); // User-facing: 1-16, internal: 0-15
        self
    }

    /// Set the pitch bend range in semitones.
    pub fn range(mut self, semitones: i64) -> Self {
        self.range = semitones.clamp(1, 96) as u8;
        self
    }

    /// Route per-note pitch bend to a voice parameter.
    pub fn to(self, voice: Voice, param: String, min: f64, max: f64) {
        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = Midi2PerNoteRoute {
            device_id: self.device_id,
            group: self.group,
            channel: self.channel,
            controller_type: Midi2PerNoteControllerType::PitchBend { range: self.range },
            voice: voice_id,
            param,
            min_value: min as f32,
            max_value: max as f32,
            curve: "linear".to_string(),
        };

        context::with_state(|state| {
            state.midi2_per_note_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }

    /// Route per-note pitch bend to a voice parameter by name.
    pub fn to_name(self, voice_name: String, param: String, min: f64, max: f64) {
        let voice_id = context::get_or_create_voice_id(&voice_name);

        let route = Midi2PerNoteRoute {
            device_id: self.device_id,
            group: self.group,
            channel: self.channel,
            controller_type: Midi2PerNoteControllerType::PitchBend { range: self.range },
            voice: voice_id,
            param,
            min_value: min as f32,
            max_value: max as f32,
            curve: "linear".to_string(),
        };

        context::with_state(|state| {
            state.midi2_per_note_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }
}

// ============================================================================
// MIDI 2.0 Per-Note Controller Builder
// ============================================================================

/// Builder for per-note controller routing.
#[derive(Debug, Clone, CustomType)]
pub struct PerNoteControllerBuilder {
    device_id: MidiDeviceId,
    group: Option<u8>,
    channel: Option<u8>,
    controller: u8,
    curve: String,
}

impl PerNoteControllerBuilder {
    /// Create a new PerNoteControllerBuilder.
    pub fn new(device_id: MidiDeviceId, controller: u8) -> Self {
        Self {
            device_id,
            group: None,
            channel: None,
            controller,
            curve: "linear".to_string(),
        }
    }

    /// Filter to a specific UMP group (0-15).
    pub fn group(mut self, group: i64) -> Self {
        self.group = Some(group.clamp(0, 15) as u8);
        self
    }

    /// Filter to a specific MIDI channel (1-16). Internally stored as 0-15.
    pub fn channel(mut self, channel: i64) -> Self {
        self.channel = Some((channel.clamp(1, 16) - 1) as u8); // User-facing: 1-16, internal: 0-15
        self
    }

    /// Set the mapping curve.
    pub fn curve(mut self, curve_name: String) -> Self {
        self.curve = curve_name;
        self
    }

    /// Route per-note controller to a voice parameter.
    pub fn to(self, voice: Voice, param: String, min: f64, max: f64) {
        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = Midi2PerNoteRoute {
            device_id: self.device_id,
            group: self.group,
            channel: self.channel,
            controller_type: Midi2PerNoteControllerType::Controller(self.controller),
            voice: voice_id,
            param,
            min_value: min as f32,
            max_value: max as f32,
            curve: self.curve,
        };

        context::with_state(|state| {
            state.midi2_per_note_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }

    /// Route per-note controller to a voice parameter by name.
    pub fn to_name(self, voice_name: String, param: String, min: f64, max: f64) {
        let voice_id = context::get_or_create_voice_id(&voice_name);

        let route = Midi2PerNoteRoute {
            device_id: self.device_id,
            group: self.group,
            channel: self.channel,
            controller_type: Midi2PerNoteControllerType::Controller(self.controller),
            voice: voice_id,
            param,
            min_value: min as f32,
            max_value: max as f32,
            curve: self.curve,
        };

        context::with_state(|state| {
            state.midi2_per_note_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }
}

// ============================================================================
// MIDI 2.0 Per-Note Pressure Builder
// ============================================================================

/// Builder for per-note pressure (polyphonic aftertouch) routing.
#[derive(Debug, Clone, CustomType)]
pub struct PerNotePressureBuilder {
    device_id: MidiDeviceId,
    group: Option<u8>,
    channel: Option<u8>,
    curve: String,
}

impl PerNotePressureBuilder {
    /// Create a new PerNotePressureBuilder.
    pub fn new(device_id: MidiDeviceId) -> Self {
        Self {
            device_id,
            group: None,
            channel: None,
            curve: "linear".to_string(),
        }
    }

    /// Filter to a specific UMP group (0-15).
    pub fn group(mut self, group: i64) -> Self {
        self.group = Some(group.clamp(0, 15) as u8);
        self
    }

    /// Filter to a specific MIDI channel (1-16). Internally stored as 0-15.
    pub fn channel(mut self, channel: i64) -> Self {
        self.channel = Some((channel.clamp(1, 16) - 1) as u8); // User-facing: 1-16, internal: 0-15
        self
    }

    /// Set the mapping curve.
    pub fn curve(mut self, curve_name: String) -> Self {
        self.curve = curve_name;
        self
    }

    /// Route per-note pressure to a voice parameter.
    pub fn to(self, voice: Voice, param: String, min: f64, max: f64) {
        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = Midi2PerNoteRoute {
            device_id: self.device_id,
            group: self.group,
            channel: self.channel,
            controller_type: Midi2PerNoteControllerType::Pressure,
            voice: voice_id,
            param,
            min_value: min as f32,
            max_value: max as f32,
            curve: self.curve,
        };

        context::with_state(|state| {
            state.midi2_per_note_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }

    /// Route per-note pressure to a voice parameter by name.
    pub fn to_name(self, voice_name: String, param: String, min: f64, max: f64) {
        let voice_id = context::get_or_create_voice_id(&voice_name);

        let route = Midi2PerNoteRoute {
            device_id: self.device_id,
            group: self.group,
            channel: self.channel,
            controller_type: Midi2PerNoteControllerType::Pressure,
            voice: voice_id,
            param,
            min_value: min as f32,
            max_value: max as f32,
            curve: self.curve,
        };

        context::with_state(|state| {
            state.midi2_per_note_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }
}

// ============================================================================
// MIDI 2.0 High-Resolution CC (32-bit) Builder
// ============================================================================

/// Deprecated compatibility builder for transport-transparent CC routing.
#[derive(Debug, Clone, CustomType)]
pub struct Cc32Route {
    device_id: MidiDeviceId,
    group: Option<u8>,
    channel: Option<u8>,
    cc: u8,
    curve: String,
}

impl Cc32Route {
    /// Create a new Cc32Route.
    pub fn new(device_id: MidiDeviceId, cc: u8) -> Self {
        Self {
            device_id,
            group: None,
            channel: None,
            cc,
            curve: "linear".to_string(),
        }
    }

    /// Filter to a specific UMP group (0-15).
    pub fn group(mut self, group: i64) -> Self {
        self.group = Some(group.clamp(0, 15) as u8);
        self
    }

    /// Filter to a specific MIDI channel (1-16). Internally stored as 0-15.
    pub fn channel(mut self, channel: i64) -> Self {
        self.channel = Some((channel.clamp(1, 16) - 1) as u8); // User-facing: 1-16, internal: 0-15
        self
    }

    /// Set the mapping curve.
    pub fn curve(mut self, curve_name: String) -> Self {
        self.curve = match canonical_cc_curve_name(&curve_name) {
            Some(curve) => curve.to_string(),
            None => {
                tracing::warn!(
                    "Unknown MIDI CC curve '{}'; falling back to linear",
                    curve_name
                );
                "linear".to_string()
            }
        };
        self
    }

    /// Route CC to a voice parameter through the unified registry.
    pub fn to(self, voice: Voice, param: String, min: f64, max: f64) {
        let voice_id = context::get_or_create_voice_id(&voice.name);

        tracing::warn!("cc32() is deprecated; use map_cc() for transport-transparent CC routing");

        let route = AdvancedMidiCcRoute {
            device_id: self.device_id,
            group: self.group,
            channel: self.channel,
            cc: self.cc,
            target: FadeTarget::Voice(voice_id),
            param,
            min: min as f32,
            max: max as f32,
            curve: self.curve,
        };

        context::with_state(|state| {
            state.advanced_cc_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }

    /// Route CC to a voice parameter by name through the unified registry.
    pub fn to_name(self, voice_name: String, param: String, min: f64, max: f64) {
        let voice_id = context::get_or_create_voice_id(&voice_name);

        tracing::warn!("cc32() is deprecated; use map_cc() for transport-transparent CC routing");

        let route = AdvancedMidiCcRoute {
            device_id: self.device_id,
            group: self.group,
            channel: self.channel,
            cc: self.cc,
            target: FadeTarget::Voice(voice_id),
            param,
            min: min as f32,
            max: max as f32,
            curve: self.curve,
        };

        context::with_state(|state| {
            state.advanced_cc_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }
}
