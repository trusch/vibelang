//! MIDI API for Rhai scripts.
//!
//! Provides MIDI device management, routing, and output functions.
//!
//! # Example
//!
//! ```ignore
//! // List available MIDI devices
//! let devices = list_midi_devices();
//! for dev in devices {
//!     print("Device: " + dev.name);
//! }
//!
//! // Get a device by name or index
//! let keyboard = midi_device("Arturia KeyStep");
//!
//! // Route keyboard to a voice
//! let lead = voice("lead").synth("saw_lead");
//! keyboard.route_to(lead);
//!
//! // Route a CC to a parameter
//! keyboard.route_cc(1, lead, "filter_cutoff", 0.0, 1.0);
//!
//! // Open for input (will be applied by runtime)
//! keyboard.open_input();
//!
//! // Advanced routing with range and transpose
//! keyboard.keyboard_route()
//!     .channel(1)
//!     .range("C2", "C6")
//!     .transpose(12)
//!     .velocity_curve("soft")
//!     .to(lead);
//!
//! // Advanced CC routing with curves
//! keyboard.cc_route(74)
//!     .curve("logarithmic")
//!     .to_param(lead, "cutoff", 100.0, 10000.0);
//! ```

#![cfg(feature = "midi")]

use rhai::{Array, CustomType, Dynamic, Engine, FnPtr, TypeBuilder};
use vibelang_core2::midi::{
    parse_note_name, CcRouteBuilder, KeyboardRouteBuilder, NoteRouteBuilder, ParameterCurve,
    VelocityCurve,
};
use vibelang_core2::reload::{MidiCallbackConfig, MidiCcRoute, MidiKeyboardRoute, MidiOutputMessage};
use vibelang_core2::traits::FadeTarget;
use vibelang_core2::types::MidiDeviceId;

use crate::context;
use super::voice::Voice;

/// Handle to a MIDI device.
#[derive(Debug, Clone, CustomType)]
pub struct MidiDevice {
    /// Device ID.
    pub id: MidiDeviceId,

    /// Device name.
    pub name: String,

    /// Whether the device has input capability.
    pub has_input: bool,

    /// Whether the device has output capability.
    pub has_output: bool,
}

impl MidiDevice {
    /// Get the device ID as an integer.
    pub fn get_id(&mut self) -> i64 {
        self.id.raw() as i64
    }

    /// Get the device name.
    pub fn get_name(&mut self) -> String {
        self.name.clone()
    }

    /// Check if device has input.
    pub fn get_has_input(&mut self) -> bool {
        self.has_input
    }

    /// Check if device has output.
    pub fn get_has_output(&mut self) -> bool {
        self.has_output
    }

    /// Route this device's keyboard input to a voice.
    ///
    /// When notes are played on the MIDI device, they will trigger the specified voice.
    pub fn route_to(self, voice: Voice) -> Self {
        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = MidiKeyboardRoute {
            device_id: self.id,
            channel: None, // All channels
            voice: voice_id,
        };

        context::with_state(|state| {
            state.midi_keyboard_routes.push(route);
        });

        self
    }

    /// Route this device's keyboard input to a voice (by name).
    pub fn route_to_name(self, voice_name: String) -> Self {
        let voice_id = context::get_or_create_voice_id(&voice_name);

        let route = MidiKeyboardRoute {
            device_id: self.id,
            channel: None,
            voice: voice_id,
        };

        context::with_state(|state| {
            state.midi_keyboard_routes.push(route);
        });

        self
    }

    /// Route this device's keyboard input to a voice on a specific channel.
    pub fn route_to_channel(self, channel: i64, voice: Voice) -> Self {
        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = MidiKeyboardRoute {
            device_id: self.id,
            channel: Some(channel.clamp(0, 15) as u8),
            voice: voice_id,
        };

        context::with_state(|state| {
            state.midi_keyboard_routes.push(route);
        });

        self
    }

    /// Route a CC from this device to a voice parameter.
    ///
    /// # Arguments
    /// * `cc` - CC number (0-127)
    /// * `voice` - Target voice
    /// * `param` - Parameter name
    /// * `min` - Value when CC is 0
    /// * `max` - Value when CC is 127
    pub fn route_cc_to_voice(
        self,
        cc: i64,
        voice: Voice,
        param: String,
        min: f64,
        max: f64,
    ) -> Self {
        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = MidiCcRoute {
            device_id: self.id,
            cc: cc.clamp(0, 127) as u8,
            target: FadeTarget::Voice(voice_id),
            param,
            min_value: min as f32,
            max_value: max as f32,
        };

        context::with_state(|state| {
            state.midi_cc_routes.push(route);
        });

        self
    }

    /// Route a CC from this device to a group parameter.
    pub fn route_cc_to_group(
        self,
        cc: i64,
        group_path: String,
        param: String,
        min: f64,
        max: f64,
    ) -> Self {
        let group_id = context::get_or_create_group_id(&group_path);

        let route = MidiCcRoute {
            device_id: self.id,
            cc: cc.clamp(0, 127) as u8,
            target: FadeTarget::Group(group_id),
            param,
            min_value: min as f32,
            max_value: max as f32,
        };

        context::with_state(|state| {
            state.midi_cc_routes.push(route);
        });

        self
    }

    /// Mark this device to be opened for input.
    ///
    /// The runtime will open the device when the script is applied.
    pub fn open_input(self) -> Self {
        context::with_state(|state| {
            state.midi_inputs.insert(self.id);
        });
        self
    }

    /// Mark this device to be opened for output.
    ///
    /// The runtime will open the device when the script is applied.
    pub fn open_output(self) -> Self {
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
        });
        self
    }

    // === Direct MIDI Output Methods ===

    /// Send a note-on message to this device.
    ///
    /// # Arguments
    /// * `channel` - MIDI channel (0-15)
    /// * `note` - Note number (0-127)
    /// * `velocity` - Velocity (0-127)
    pub fn note_on(&mut self, channel: i64, note: i64, velocity: i64) {
        let msg = MidiOutputMessage::NoteOn {
            device_id: self.id,
            channel: channel.clamp(0, 15) as u8,
            note: note.clamp(0, 127) as u8,
            velocity: velocity.clamp(0, 127) as u8,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a note-off message to this device.
    ///
    /// # Arguments
    /// * `channel` - MIDI channel (0-15)
    /// * `note` - Note number (0-127)
    pub fn note_off(&mut self, channel: i64, note: i64) {
        let msg = MidiOutputMessage::NoteOff {
            device_id: self.id,
            channel: channel.clamp(0, 15) as u8,
            note: note.clamp(0, 127) as u8,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a control change message to this device.
    ///
    /// # Arguments
    /// * `channel` - MIDI channel (0-15)
    /// * `cc` - Controller number (0-127)
    /// * `value` - Controller value (0-127)
    pub fn cc(&mut self, channel: i64, cc: i64, value: i64) {
        let msg = MidiOutputMessage::ControlChange {
            device_id: self.id,
            channel: channel.clamp(0, 15) as u8,
            cc: cc.clamp(0, 127) as u8,
            value: value.clamp(0, 127) as u8,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a program change message to this device.
    ///
    /// # Arguments
    /// * `channel` - MIDI channel (0-15)
    /// * `program` - Program number (0-127)
    pub fn program_change(&mut self, channel: i64, program: i64) {
        let msg = MidiOutputMessage::ProgramChange {
            device_id: self.id,
            channel: channel.clamp(0, 15) as u8,
            program: program.clamp(0, 127) as u8,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    // === Advanced Routing Builders ===

    /// Start building an advanced keyboard route.
    ///
    /// Returns a KeyboardRoute that can be configured with range, transpose, and velocity curves.
    pub fn keyboard_route(&self) -> KeyboardRoute {
        KeyboardRoute {
            device_id: self.id,
            builder: KeyboardRouteBuilder::new(self.id),
        }
    }

    /// Start building an advanced note route (for drums/pads).
    ///
    /// # Arguments
    /// * `note` - The MIDI note number (0-127) or note name ("C4", "D#3")
    pub fn note_route(&self, note: Dynamic) -> NoteRoute {
        let note_num = if note.is_int() {
            note.as_int().unwrap_or(60).clamp(0, 127) as u8
        } else if note.is_string() {
            let name = note.into_string().unwrap_or_default();
            parse_note_name(&name).unwrap_or(60)
        } else {
            60
        };

        NoteRoute {
            device_id: self.id,
            builder: NoteRouteBuilder::new(self.id, note_num),
        }
    }

    /// Start building an advanced CC route with curves.
    ///
    /// # Arguments
    /// * `cc` - The CC number (0-127)
    pub fn cc_route(&self, cc: i64) -> CcRoute {
        CcRoute {
            device_id: self.id,
            builder: CcRouteBuilder::new(self.id, cc.clamp(0, 127) as u8),
        }
    }

    // === MIDI Callback Methods ===

    /// Register a callback for note events (note on/off).
    ///
    /// The callback will be invoked with: (note, velocity, channel, is_on)
    ///
    /// # Arguments
    /// * `callback` - Rhai function to call on note events
    pub fn on_note(&self, callback: FnPtr) -> Self {
        let callback_id = context::register_midi_callback(callback);

        let config = MidiCallbackConfig {
            callback_id,
            device_id: self.id,
            callback_type: "note".to_string(),
            channel: None,
        };

        context::with_state(|state| {
            state.midi_callbacks.push(config);
            state.midi_inputs.insert(self.id);
        });

        self.clone()
    }

    /// Register a callback for note events on a specific channel.
    ///
    /// # Arguments
    /// * `channel` - MIDI channel (1-16)
    /// * `callback` - Rhai function to call on note events
    pub fn on_note_channel(&self, channel: i64, callback: FnPtr) -> Self {
        let callback_id = context::register_midi_callback(callback);

        let config = MidiCallbackConfig {
            callback_id,
            device_id: self.id,
            callback_type: "note".to_string(),
            channel: Some(channel.clamp(1, 16) as u8 - 1), // Convert to 0-indexed
        };

        context::with_state(|state| {
            state.midi_callbacks.push(config);
            state.midi_inputs.insert(self.id);
        });

        self.clone()
    }

    /// Register a callback for all CC events.
    ///
    /// The callback will be invoked with: (cc, value, channel)
    ///
    /// # Arguments
    /// * `callback` - Rhai function to call on CC events
    pub fn on_cc(&self, callback: FnPtr) -> Self {
        let callback_id = context::register_midi_callback(callback);

        let config = MidiCallbackConfig {
            callback_id,
            device_id: self.id,
            callback_type: "cc".to_string(),
            channel: None,
        };

        context::with_state(|state| {
            state.midi_callbacks.push(config);
            state.midi_inputs.insert(self.id);
        });

        self.clone()
    }

    /// Register a callback for a specific CC number.
    ///
    /// # Arguments
    /// * `cc` - CC number (0-127)
    /// * `callback` - Rhai function to call when this CC changes
    pub fn on_cc_num(&self, cc: i64, callback: FnPtr) -> Self {
        let callback_id = context::register_midi_callback(callback);

        let config = MidiCallbackConfig {
            callback_id,
            device_id: self.id,
            callback_type: format!("cc:{}", cc.clamp(0, 127)),
            channel: None,
        };

        context::with_state(|state| {
            state.midi_callbacks.push(config);
            state.midi_inputs.insert(self.id);
        });

        self.clone()
    }

    /// Register a callback for MIDI clock sync events (start/stop/continue).
    ///
    /// The callback will be invoked with: (event_type) where event_type is
    /// "clock", "start", "stop", or "continue".
    ///
    /// # Arguments
    /// * `callback` - Rhai function to call on clock events
    pub fn on_clock_sync(&self, callback: FnPtr) -> Self {
        let callback_id = context::register_midi_callback(callback);

        let config = MidiCallbackConfig {
            callback_id,
            device_id: self.id,
            callback_type: "clock".to_string(),
            channel: None,
        };

        context::with_state(|state| {
            state.midi_callbacks.push(config);
            state.midi_inputs.insert(self.id);
        });

        self.clone()
    }

    /// Register a callback for all MIDI events.
    ///
    /// The callback will be invoked with a map containing event details.
    ///
    /// # Arguments
    /// * `callback` - Rhai function to call on any MIDI event
    pub fn on_midi(&self, callback: FnPtr) -> Self {
        let callback_id = context::register_midi_callback(callback);

        let config = MidiCallbackConfig {
            callback_id,
            device_id: self.id,
            callback_type: "all".to_string(),
            channel: None,
        };

        context::with_state(|state| {
            state.midi_callbacks.push(config);
            state.midi_inputs.insert(self.id);
        });

        self.clone()
    }
}

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
    /// Filter to a specific MIDI channel (1-16).
    pub fn channel(mut self, channel: i64) -> Self {
        self.builder = self.builder.channel(channel.clamp(1, 16) as u8);
        self
    }

    /// Set the note range using MIDI note numbers.
    pub fn range_midi(mut self, min: i64, max: i64) -> Self {
        self.builder = self.builder.range(min.clamp(0, 127) as u8, max.clamp(0, 127) as u8);
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
    pub fn velocity_curve(mut self, curve_name: String) -> Self {
        self.builder = self.builder.velocity_curve_name(&curve_name);
        self
    }

    /// Set a fixed velocity (0-127).
    pub fn fixed_velocity(mut self, velocity: i64) -> Self {
        self.builder.velocity_curve = VelocityCurve::Fixed(velocity.clamp(0, 127) as u8);
        self
    }

    /// Route to a voice and apply the configuration.
    pub fn to(self, voice: Voice) {
        use vibelang_core2::reload::AdvancedMidiKeyboardRoute;

        let voice_id = context::get_or_create_voice_id(&voice.name);
        let mut builder = self.builder;
        builder.target_voice = Some(voice_id);

        let route = AdvancedMidiKeyboardRoute {
            device_id: self.device_id,
            channel: builder.channel,
            note_min: builder.note_min,
            note_max: builder.note_max,
            transpose: builder.transpose,
            velocity_curve: match builder.velocity_curve {
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

    /// Route to a voice by name.
    pub fn to_name(self, voice_name: String) {
        use vibelang_core2::reload::AdvancedMidiKeyboardRoute;

        let voice_id = context::get_or_create_voice_id(&voice_name);
        let builder = self.builder;

        let route = AdvancedMidiKeyboardRoute {
            device_id: self.device_id,
            channel: builder.channel,
            note_min: builder.note_min,
            note_max: builder.note_max,
            transpose: builder.transpose,
            velocity_curve: match builder.velocity_curve {
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
    /// Filter to a specific MIDI channel (1-16).
    pub fn channel(mut self, channel: i64) -> Self {
        self.builder = self.builder.channel(channel.clamp(1, 16) as u8);
        self
    }

    /// Set choke group (notes in same group stop each other).
    pub fn choke_group(mut self, group: String) -> Self {
        self.builder = self.builder.choke_group(&group);
        self
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

    /// Route to a voice and apply the configuration.
    pub fn to(self, voice: Voice) {
        use vibelang_core2::reload::AdvancedMidiNoteRoute;

        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = AdvancedMidiNoteRoute {
            device_id: self.device_id,
            source_note: self.builder.source_note,
            channel: self.builder.channel,
            choke_group: self.builder.choke_group.clone(),
            velocity_param: self.builder.velocity_mapping.as_ref().map(|m| m.param.clone()),
            velocity_min: self.builder.velocity_mapping.as_ref().map(|m| m.min),
            velocity_max: self.builder.velocity_mapping.as_ref().map(|m| m.max),
            voice: voice_id,
        };

        context::with_state(|state| {
            state.advanced_note_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
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
        use vibelang_core2::reload::AdvancedMidiCcRoute;

        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = AdvancedMidiCcRoute {
            device_id: self.device_id,
            cc: self.builder.cc,
            channel: self.builder.channel,
            curve: match self.builder.curve {
                ParameterCurve::Linear => "linear".to_string(),
                ParameterCurve::Logarithmic => "logarithmic".to_string(),
                ParameterCurve::Exponential => "exponential".to_string(),
            },
            voice: voice_id,
            param,
            min: min as f32,
            max: max as f32,
        };

        context::with_state(|state| {
            state.advanced_cc_routes.push(route);
            state.midi_inputs.insert(self.device_id);
        });
    }

    /// Route to a voice parameter by name with range.
    pub fn to_param_name(self, voice_name: String, param: String, min: f64, max: f64) {
        use vibelang_core2::reload::AdvancedMidiCcRoute;

        let voice_id = context::get_or_create_voice_id(&voice_name);

        let route = AdvancedMidiCcRoute {
            device_id: self.device_id,
            cc: self.builder.cc,
            channel: self.builder.channel,
            curve: match self.builder.curve {
                ParameterCurve::Linear => "linear".to_string(),
                ParameterCurve::Logarithmic => "logarithmic".to_string(),
                ParameterCurve::Exponential => "exponential".to_string(),
            },
            voice: voice_id,
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

/// List all available MIDI devices.
///
/// Returns an array of MidiDevice objects.
pub fn list_midi_devices() -> Array {
    use midir::{MidiInput, MidiOutput};
    use std::collections::HashMap;

    let mut devices = Vec::new();
    let mut seen_names: HashMap<String, usize> = HashMap::new();

    // List input devices
    if let Ok(midi_in) = MidiInput::new("vibelang-rhai-list") {
        for (idx, port) in midi_in.ports().iter().enumerate() {
            if let Ok(name) = midi_in.port_name(port) {
                let device = MidiDevice {
                    id: MidiDeviceId::new(idx as u32),
                    name: name.clone(),
                    has_input: true,
                    has_output: false,
                };
                devices.push(Dynamic::from(device));
                seen_names.insert(name, idx);
            }
        }
    }

    // List output devices
    if let Ok(midi_out) = MidiOutput::new("vibelang-rhai-list") {
        for (_idx, port) in midi_out.ports().iter().enumerate() {
            if let Ok(name) = midi_out.port_name(port) {
                // Check if we already have this device from input
                if let Some(&existing_idx) = seen_names.get(&name) {
                    // Update the existing device
                    if let Some(dev) = devices.get_mut(existing_idx) {
                        if let Some(d) = dev.clone().try_cast::<MidiDevice>() {
                            let mut updated = d;
                            updated.has_output = true;
                            *dev = Dynamic::from(updated);
                        }
                    }
                } else {
                    let device = MidiDevice {
                        id: MidiDeviceId::new(devices.len() as u32),
                        name,
                        has_input: false,
                        has_output: true,
                    };
                    devices.push(Dynamic::from(device));
                }
            }
        }
    }

    devices
}

/// Get a MIDI device by name (partial match) or index.
///
/// # Arguments
/// * `name_or_idx` - Device name (partial match) or index as string
pub fn midi_device(name_or_idx: String) -> MidiDevice {
    use midir::{MidiInput, MidiOutput};

    // Try to parse as index first
    if let Ok(idx) = name_or_idx.parse::<u32>() {
        // Get device by index
        if let Ok(midi_in) = MidiInput::new("vibelang-rhai-get") {
            if let Some(port) = midi_in.ports().get(idx as usize) {
                if let Ok(name) = midi_in.port_name(port) {
                    return MidiDevice {
                        id: MidiDeviceId::new(idx),
                        name,
                        has_input: true,
                        has_output: false, // Would need to check output separately
                    };
                }
            }
        }
    }

    // Search by name (partial match)
    if let Ok(midi_in) = MidiInput::new("vibelang-rhai-get") {
        for (idx, port) in midi_in.ports().iter().enumerate() {
            if let Ok(name) = midi_in.port_name(port) {
                if name.to_lowercase().contains(&name_or_idx.to_lowercase()) {
                    return MidiDevice {
                        id: MidiDeviceId::new(idx as u32),
                        name,
                        has_input: true,
                        has_output: false,
                    };
                }
            }
        }
    }

    // If not found in inputs, try outputs
    if let Ok(midi_out) = MidiOutput::new("vibelang-rhai-get") {
        for (idx, port) in midi_out.ports().iter().enumerate() {
            if let Ok(name) = midi_out.port_name(port) {
                if name.to_lowercase().contains(&name_or_idx.to_lowercase()) {
                    return MidiDevice {
                        id: MidiDeviceId::new(idx as u32),
                        name,
                        has_input: false,
                        has_output: true,
                    };
                }
            }
        }
    }

    // Return a placeholder if not found
    MidiDevice {
        id: MidiDeviceId::new(0),
        name: format!("Unknown: {}", name_or_idx),
        has_input: false,
        has_output: false,
    }
}

/// Get a MIDI device by index.
pub fn midi_device_by_id(id: i64) -> MidiDevice {
    midi_device(id.to_string())
}

/// Register MIDI API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register MidiDevice type
    engine.build_type::<MidiDevice>();

    // Device discovery
    engine.register_fn("list_midi_devices", list_midi_devices);
    engine.register_fn("midi_device", midi_device);
    engine.register_fn("midi_device", midi_device_by_id);

    // Getters
    engine.register_fn("id", MidiDevice::get_id);
    engine.register_get("id", MidiDevice::get_id);
    engine.register_fn("name", MidiDevice::get_name);
    engine.register_get("name", MidiDevice::get_name);
    engine.register_fn("has_input", MidiDevice::get_has_input);
    engine.register_get("has_input", MidiDevice::get_has_input);
    engine.register_fn("has_output", MidiDevice::get_has_output);
    engine.register_get("has_output", MidiDevice::get_has_output);

    // Routing methods
    engine.register_fn("route_to", MidiDevice::route_to);
    engine.register_fn("route_to", MidiDevice::route_to_name);
    engine.register_fn("route_to_channel", MidiDevice::route_to_channel);
    engine.register_fn("route_cc", MidiDevice::route_cc_to_voice);
    engine.register_fn("route_cc_to_group", MidiDevice::route_cc_to_group);

    // Device opening
    engine.register_fn("open_input", MidiDevice::open_input);
    engine.register_fn("open_output", MidiDevice::open_output);

    // Direct MIDI output
    engine.register_fn("note_on", MidiDevice::note_on);
    engine.register_fn("note_off", MidiDevice::note_off);
    engine.register_fn("cc", MidiDevice::cc);
    engine.register_fn("program_change", MidiDevice::program_change);

    // Advanced routing builder methods
    engine.register_fn("keyboard_route", MidiDevice::keyboard_route);
    engine.register_fn("note_route", MidiDevice::note_route);
    engine.register_fn("cc_route", MidiDevice::cc_route);

    // Callback methods
    engine.register_fn("on_note", MidiDevice::on_note);
    engine.register_fn("on_note_channel", MidiDevice::on_note_channel);
    engine.register_fn("on_cc", MidiDevice::on_cc);
    engine.register_fn("on_cc_num", MidiDevice::on_cc_num);
    engine.register_fn("on_clock_sync", MidiDevice::on_clock_sync);
    engine.register_fn("on_midi", MidiDevice::on_midi);

    // Register KeyboardRoute type
    engine.build_type::<KeyboardRoute>();
    engine.register_fn("channel", KeyboardRoute::channel);
    engine.register_fn("range_midi", KeyboardRoute::range_midi);
    engine.register_fn("range", KeyboardRoute::range);
    engine.register_fn("transpose", KeyboardRoute::transpose);
    engine.register_fn("octave", KeyboardRoute::octave);
    engine.register_fn("velocity_curve", KeyboardRoute::velocity_curve);
    engine.register_fn("fixed_velocity", KeyboardRoute::fixed_velocity);
    engine.register_fn("to", KeyboardRoute::to);
    engine.register_fn("to", KeyboardRoute::to_name);

    // Register NoteRoute type
    engine.build_type::<NoteRoute>();
    engine.register_fn("channel", NoteRoute::channel);
    engine.register_fn("choke_group", NoteRoute::choke_group);
    engine.register_fn("velocity_to", NoteRoute::velocity_to);
    engine.register_fn("to", NoteRoute::to);

    // Register CcRoute type
    engine.build_type::<CcRoute>();
    engine.register_fn("channel", CcRoute::channel);
    engine.register_fn("curve", CcRoute::curve);
    engine.register_fn("to_param", CcRoute::to_param);
    engine.register_fn("to_param", CcRoute::to_param_name);
}
