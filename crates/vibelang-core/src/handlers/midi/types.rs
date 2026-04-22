//! MIDI types and message conversion.
//!
//! This module contains the internal MIDI message types used by the handler
//! and routing configuration types.

use crate::midi::{
    CcRouteBuilder, KeyboardRouteBuilder, MidiMessage as NewMidiMessage, NoteRouteBuilder,
};
use crate::traits::FadeTarget;
use crate::types::ids::MidiDeviceId;
use crate::types::VoiceId;
use std::collections::HashMap;

/// A parsed MIDI message (legacy format for internal routing).
#[derive(Clone, Debug)]
#[allow(dead_code)] // All fields intentionally kept for future routing features
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    ControlChange {
        channel: u8,
        cc: u8,
        value: u8,
    },
    PitchBend {
        channel: u8,
        value: i16,
    },
    /// MIDI Clock (0xF8) - 24 pulses per quarter note.
    Clock,
    /// MIDI Start (0xFA) - Start playback from beginning.
    Start,
    /// MIDI Stop (0xFC) - Stop playback.
    Stop,
    /// MIDI Continue (0xFB) - Continue playback from current position.
    Continue,
}

/// Convert new infrastructure MidiMessage to legacy MidiMessage.
///
/// Returns None for message types that the legacy system doesn't support.
pub fn convert_new_to_legacy_message(msg: &NewMidiMessage) -> Option<MidiMessage> {
    match msg {
        NewMidiMessage::NoteOn {
            channel,
            note,
            velocity,
        } => {
            let vel = velocity.to_midi1();
            if vel > 0 {
                Some(MidiMessage::NoteOn {
                    channel: channel.get(),
                    note: *note,
                    velocity: vel,
                })
            } else {
                Some(MidiMessage::NoteOff {
                    channel: channel.get(),
                    note: *note,
                })
            }
        }
        NewMidiMessage::NoteOff { channel, note, .. } => Some(MidiMessage::NoteOff {
            channel: channel.get(),
            note: *note,
        }),
        NewMidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => Some(MidiMessage::ControlChange {
            channel: channel.get(),
            cc: *controller,
            value: *value,
        }),
        NewMidiMessage::PitchBend { channel, value } => Some(MidiMessage::PitchBend {
            channel: channel.get(),
            value: *value,
        }),
        NewMidiMessage::Clock => Some(MidiMessage::Clock),
        NewMidiMessage::Start => Some(MidiMessage::Start),
        NewMidiMessage::Continue => Some(MidiMessage::Continue),
        NewMidiMessage::Stop => Some(MidiMessage::Stop),
        // New message types not supported by legacy system
        NewMidiMessage::PolyAftertouch { .. }
        | NewMidiMessage::ProgramChange { .. }
        | NewMidiMessage::ChannelAftertouch { .. }
        | NewMidiMessage::SysEx(_)
        | NewMidiMessage::TimeCode { .. }
        | NewMidiMessage::SongPosition { .. }
        | NewMidiMessage::SongSelect { .. }
        | NewMidiMessage::TuneRequest
        | NewMidiMessage::ActiveSensing
        | NewMidiMessage::Reset => None,

        // MIDI 2.0 messages are handled separately via the MIDI 2.0 processing path.
        // They don't convert to legacy MIDI 1.0 messages.
        _ => None,
    }
}

/// Keyboard routing configuration.
#[derive(Clone, Debug)]
pub struct KeyboardRoute {
    pub device_id: MidiDeviceId,
    pub channel: Option<u8>, // None = all channels
    pub voice_id: VoiceId,
}

/// CC routing configuration.
#[derive(Clone, Debug)]
pub struct CcRoute {
    pub device_id: MidiDeviceId,
    pub cc: u8,
    pub target: FadeTarget,
    pub param: String,
    /// Min value when CC is 0.
    pub min_value: f32,
    /// Max value when CC is 127.
    pub max_value: f32,
}

/// MIDI 2.0 keyboard routing configuration.
/// Reserved for future MIDI 2.0 routing support.

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Midi2KeyboardRoute {
    pub device_id: MidiDeviceId,
    pub group: Option<u8>,
    pub channel: Option<u8>,
    pub note_min: u8,
    pub note_max: u8,
    pub transpose: i8,
    pub velocity_curve: String,
    pub voice_id: VoiceId,
}

/// MIDI 2.0 per-note route configuration.

#[derive(Clone, Debug)]
pub struct Midi2PerNoteRoute {
    pub device_id: MidiDeviceId,
    pub group: Option<u8>,
    pub channel: Option<u8>,
    pub controller_type: Midi2ControllerType,
    pub voice_id: VoiceId,
    pub param: String,
    pub min_value: f32,
    pub max_value: f32,
    pub curve: String,
}

/// MIDI 2.0 controller type for per-note routing.

#[derive(Clone, Debug)]
pub enum Midi2ControllerType {
    PitchBend { range: u8 },
    Pressure,
    Timbre,
    Controller(u8),
}

/// MIDI 2.0 high-resolution CC route configuration.

#[derive(Clone, Debug)]
pub struct Midi2CcRoute {
    pub device_id: MidiDeviceId,
    pub group: Option<u8>,
    pub channel: Option<u8>,
    pub cc: u8,
    pub voice_id: VoiceId,
    pub param: String,
    pub min_value: f32,
    pub max_value: f32,
    pub curve: String,
}

/// Internal routing state.
#[derive(Default)]
pub struct MidiRouting {
    pub keyboard_routes: Vec<KeyboardRoute>,
    pub cc_routes: Vec<CcRoute>,
    /// Advanced keyboard routes with full configuration.
    pub advanced_keyboard_routes: Vec<KeyboardRouteBuilder>,
    /// Advanced note routes (for drums/pads).
    pub note_routes: Vec<NoteRouteBuilder>,
    /// Advanced CC routes with curves.
    pub advanced_cc_routes: Vec<CcRouteBuilder>,
    /// Choke group tracking: group name -> active node IDs.
    pub choke_groups: HashMap<String, Vec<crate::types::NodeId>>,

    // MIDI 2.0 routes
    pub midi2_keyboard_routes: Vec<Midi2KeyboardRoute>,

    pub midi2_per_note_routes: Vec<Midi2PerNoteRoute>,

    pub midi2_cc_routes: Vec<Midi2CcRoute>,
}

/// A MIDI event notification sent to registered callbacks.
#[derive(Clone, Debug)]
pub struct MidiEventNotification {
    /// Callback ID that should handle this event.
    pub callback_id: u64,
    /// Device the event came from.
    pub device_id: MidiDeviceId,
    /// The MIDI message.
    pub message: MidiMessage,
}

/// Map a value from one range to another with optional curve transformation.
pub fn map_to_range(
    value: f32,
    in_min: f32,
    in_max: f32,
    out_min: f32,
    out_max: f32,
    curve: &str,
) -> f32 {
    // Normalize input to 0-1
    let normalized = if (in_max - in_min).abs() > f32::EPSILON {
        (value - in_min) / (in_max - in_min)
    } else {
        0.5
    };

    // Apply curve
    let curved = match curve {
        "logarithmic" => {
            // Logarithmic curve (steeper at low values)
            if normalized <= 0.0 {
                0.0
            } else {
                (normalized.ln() + 5.0) / 5.0
            }
        }
        "exponential" => {
            // Exponential curve (steeper at high values)
            normalized * normalized
        }
        "s_curve" | "scurve" => {
            // S-curve (smooth transition)
            let t = normalized.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        }
        _ => normalized, // Linear
    };

    // Map to output range
    out_min + curved.clamp(0.0, 1.0) * (out_max - out_min)
}
