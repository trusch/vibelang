//! MIDI device handle and methods.

use rhai::{CustomType, Dynamic, EvalAltResult, FnPtr, Position, TypeBuilder};
use vibelang_core::midi::{
    parse_note_name, CcRouteBuilder, KeyboardRouteBuilder, NoteRouteBuilder,
};
use vibelang_core::reload::{
    MidiCallbackConfig, MidiCcRoute, MidiClockOutputRequest, MidiKeyboardRoute, MidiOutputMessage,
    MidiRecordingRequest,
};
use vibelang_core::traits::FadeTarget;
use vibelang_core::types::MidiDeviceId;

use crate::api::voice::Voice;
use crate::context;

use super::routing::{CcRoute, KeyboardRoute, NoteRoute};

use super::midi2::{Cc32Route, GroupRoute, PerNoteControllerBuilder, PerNotePitchBendBuilder, PerNotePressureBuilder};

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

    /// MIDI channel (0-15) for output.
    pub channel: u8,

    /// Default note to send when triggered by patterns (without explicit note).
    /// When set, pattern triggers on voices using this device will send this note.
    pub default_note: Option<u8>,
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

    /// Get the default note (if set).
    pub fn get_default_note(&mut self) -> i64 {
        self.default_note.map(|n| n as i64).unwrap_or(-1)
    }

    /// Set the default note for this MIDI device.
    ///
    /// When a voice is configured with `.on(midi_device)` and this device has
    /// a default note set, pattern triggers will send this note to the MIDI device.
    ///
    /// # Arguments
    /// * `note` - Note name (e.g., "A3", "C#4") or MIDI note number (0-127)
    ///
    /// # Example
    /// ```rhai
    /// let model15 = midi_device("MODEL 15").note("A3");
    /// let synth = voice("synth").on(model15).channel(1).apply();
    /// pattern("bass").on(synth).step("x...").start();  // Sends A3 on each trigger
    /// ```
    pub fn note(mut self, note: Dynamic) -> Self {
        let note_num = if note.is_int() {
            note.as_int().unwrap_or(60).clamp(0, 127) as u8
        } else if note.is_string() {
            let name = note.into_string().unwrap_or_default();
            match parse_note_name(&name) {
                Some(n) => n,
                None => {
                    tracing::warn!(
                        "Invalid note name '{}' for MIDI device '{}', defaulting to C4 (60)",
                        name,
                        self.name
                    );
                    60
                }
            }
        } else {
            tracing::warn!(
                "Invalid note type for MIDI device '{}', defaulting to C4 (60)",
                self.name
            );
            60
        };
        self.default_note = Some(note_num);
        self
    }

    /// Set the MIDI channel (1-16) for this device handle.
    ///
    /// User-facing API uses 1-16 (musician convention). Internally stored as 0-15.
    /// This is consistent with `on_note_channel()` which also accepts 1-16.
    ///
    /// # Arguments
    /// * `ch` - MIDI channel (1-16, will be clamped)
    ///
    /// # Example
    /// ```rhai
    /// let synth_ch1 = midi_device("External Synth").channel(1);
    /// let synth_ch2 = midi_device("External Synth").channel(2);
    /// voice("bass").on(synth_ch1).apply();
    /// voice("lead").on(synth_ch2).apply();
    /// ```
    pub fn channel(mut self, ch: i64) -> Self {
        // User-facing: 1-16, internally: 0-15
        self.channel = (ch.clamp(1, 16) - 1) as u8;
        self
    }

    /// Get the MIDI channel.
    pub fn get_channel(&mut self) -> i64 {
        self.channel as i64
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

    /// Route this device's keyboard input to a voice on a specific channel (1-16).
    pub fn route_to_channel(self, channel: i64, voice: Voice) -> Self {
        let voice_id = context::get_or_create_voice_id(&voice.name);

        let route = MidiKeyboardRoute {
            device_id: self.id,
            channel: Some((channel.clamp(1, 16) - 1) as u8), // User-facing: 1-16, internal: 0-15
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
    /// **Deprecated**: Devices are now automatically opened based on usage.
    /// You can safely remove this call - methods like `route_to()` and
    /// `start_recording()` will automatically open the input.
    #[deprecated(note = "Devices are now auto-opened based on usage. This call can be removed.")]
    pub fn open_input(self) -> Self {
        context::with_state(|state| {
            state.midi_inputs.insert(self.id);
        });
        self
    }

    /// Mark this device to be opened for output.
    ///
    /// **Deprecated**: Devices are now automatically opened based on usage.
    /// You can safely remove this call - methods like `note_on()`, `send_cc()`,
    /// and `enable_clock()` will automatically open the output.
    #[deprecated(note = "Devices are now auto-opened based on usage. This call can be removed.")]
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
    /// * `channel` - MIDI channel (1-16, internally converted to 0-15)
    /// * `note` - Note number (0-127)
    /// * `velocity` - Velocity (0-127)
    pub fn note_on(&mut self, channel: i64, note: i64, velocity: i64) {
        let msg = MidiOutputMessage::NoteOn {
            device_id: self.id,
            channel: (channel.clamp(1, 16) - 1) as u8,
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
    /// * `channel` - MIDI channel (1-16, internally converted to 0-15)
    /// * `note` - Note number (0-127)
    pub fn note_off(&mut self, channel: i64, note: i64) {
        let msg = MidiOutputMessage::NoteOff {
            device_id: self.id,
            channel: (channel.clamp(1, 16) - 1) as u8,
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
    /// * `channel` - MIDI channel (1-16, internally converted to 0-15)
    /// * `cc` - Controller number (0-127)
    /// * `value` - Controller value (0-127)
    pub fn cc(&mut self, channel: i64, cc: i64, value: i64) {
        let msg = MidiOutputMessage::ControlChange {
            device_id: self.id,
            channel: (channel.clamp(1, 16) - 1) as u8,
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
    /// * `channel` - MIDI channel (1-16, internally converted to 0-15)
    /// * `program` - Program number (0-127)
    pub fn program_change(&mut self, channel: i64, program: i64) {
        let msg = MidiOutputMessage::ProgramChange {
            device_id: self.id,
            channel: (channel.clamp(1, 16) - 1) as u8,
            program: program.clamp(0, 127) as u8,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a pitch bend message to this device.
    ///
    /// # Arguments
    /// * `channel` - MIDI channel (1-16, internally converted to 0-15)
    /// * `value` - Pitch bend value (-8192 to +8191, 0 = center)
    pub fn pitch_bend(&mut self, channel: i64, value: i64) {
        let msg = MidiOutputMessage::PitchBend {
            device_id: self.id,
            channel: (channel.clamp(1, 16) - 1) as u8,
            value: value.clamp(-8192, 8191) as i16,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    // === MIDI 2.0 High-Resolution Output Methods ===

    /// Send a high-resolution note-on message (MIDI 2.0).
    ///
    /// # Arguments
    /// * `channel` - MIDI channel (1-16, internally converted to 0-15)
    /// * `note` - Note number (0-127)
    /// * `velocity` - 16-bit velocity (0-65535). For MIDI 1.0 devices, this is automatically downscaled.
    pub fn note_on_hires(&mut self, channel: i64, note: i64, velocity: i64) {
        let msg = MidiOutputMessage::Midi2NoteOn {
            device_id: self.id,
            group: 0,
            channel: (channel.clamp(1, 16) - 1) as u8, // User-facing: 1-16
            note: note.clamp(0, 127) as u8,
            velocity: velocity.clamp(0, 65535) as u16,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a high-resolution note-off message (MIDI 2.0).
    pub fn note_off_hires(&mut self, channel: i64, note: i64, velocity: i64) {
        let msg = MidiOutputMessage::Midi2NoteOff {
            device_id: self.id,
            group: 0,
            channel: (channel.clamp(1, 16) - 1) as u8, // User-facing: 1-16
            note: note.clamp(0, 127) as u8,
            velocity: velocity.clamp(0, 65535) as u16,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a high-resolution control change message (MIDI 2.0).
    pub fn cc_hires(&mut self, channel: i64, cc: i64, value: i64) {
        let msg = MidiOutputMessage::Midi2ControlChange {
            device_id: self.id,
            group: 0,
            channel: (channel.clamp(1, 16) - 1) as u8, // User-facing: 1-16
            controller: cc.clamp(0, 127) as u8,
            value: value.clamp(0, 0xFFFFFFFF) as u32,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a high-resolution pitch bend message (MIDI 2.0).
    pub fn pitch_bend_hires(&mut self, channel: i64, value: i64) {
        let msg = MidiOutputMessage::Midi2PitchBend {
            device_id: self.id,
            group: 0,
            channel: (channel.clamp(1, 16) - 1) as u8, // User-facing: 1-16
            value: value.clamp(0, 0xFFFFFFFF) as u32,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a per-note pitch bend message (MIDI 2.0).
    pub fn send_per_note_bend(&mut self, channel: i64, note: i64, value: i64) {
        let msg = MidiOutputMessage::Midi2PerNotePitchBend {
            device_id: self.id,
            group: 0,
            channel: (channel.clamp(1, 16) - 1) as u8, // User-facing: 1-16
            note: note.clamp(0, 127) as u8,
            value: value.clamp(0, 0xFFFFFFFF) as u32,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a per-note controller message (MIDI 2.0).
    pub fn send_per_note_cc(&mut self, channel: i64, note: i64, controller: i64, value: i64) {
        let msg = MidiOutputMessage::Midi2PerNoteController {
            device_id: self.id,
            group: 0,
            channel: (channel.clamp(1, 16) - 1) as u8, // User-facing: 1-16
            note: note.clamp(0, 127) as u8,
            controller: controller.clamp(0, 127) as u8,
            value: value.clamp(0, 0xFFFFFFFF) as u32,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a poly pressure (per-note aftertouch) message (MIDI 2.0).
    pub fn poly_pressure_hires(&mut self, channel: i64, note: i64, pressure: i64) {
        let msg = MidiOutputMessage::Midi2PolyPressure {
            device_id: self.id,
            group: 0,
            channel: (channel.clamp(1, 16) - 1) as u8, // User-facing: 1-16
            note: note.clamp(0, 127) as u8,
            pressure: pressure.clamp(0, 0xFFFFFFFF) as u32,
        };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    // === Advanced Routing Builders ===

    /// Start building an advanced keyboard route.
    pub fn keyboard_route(self) -> KeyboardRoute {
        KeyboardRoute::new(self.id, KeyboardRouteBuilder::new(self.id))
    }

    /// Start building an advanced note route (for drums/pads).
    pub fn note_route(self, note: Dynamic) -> NoteRoute {
        let note_num = if note.is_int() {
            note.as_int().unwrap_or(60).clamp(0, 127) as u8
        } else if note.is_string() {
            let name = note.into_string().unwrap_or_default();
            match parse_note_name(&name) {
                Some(n) => n,
                None => {
                    tracing::warn!(
                        "Invalid note name '{}' for note_route on device '{}', defaulting to C4 (60)",
                        name,
                        self.name
                    );
                    60
                }
            }
        } else {
            tracing::warn!(
                "Invalid note type for note_route on device '{}', defaulting to C4 (60)",
                self.name
            );
            60
        };

        NoteRoute::new(self.id, NoteRouteBuilder::new(self.id, note_num))
    }

    /// Start building an advanced CC route with curves.
    pub fn cc_route(self, cc: i64) -> CcRoute {
        CcRoute::new(self.id, CcRouteBuilder::new(self.id, cc.clamp(0, 127) as u8))
    }

    // === MIDI Callback Methods ===

    /// Register a callback for note events (note on/off).
    pub fn on_note(self, callback: FnPtr) -> Self {
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

        self
    }

    /// Register a callback for note events on a specific channel.
    pub fn on_note_channel(self, channel: i64, callback: FnPtr) -> Self {
        let callback_id = context::register_midi_callback(callback);

        let config = MidiCallbackConfig {
            callback_id,
            device_id: self.id,
            callback_type: "note".to_string(),
            channel: Some(channel.clamp(1, 16) as u8 - 1),
        };

        context::with_state(|state| {
            state.midi_callbacks.push(config);
            state.midi_inputs.insert(self.id);
        });

        self
    }

    /// Register a callback for all CC events.
    pub fn on_cc(self, callback: FnPtr) -> Self {
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

        self
    }

    /// Register a callback for a specific CC number.
    pub fn on_cc_num(self, cc: i64, callback: FnPtr) -> Self {
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

        self
    }

    /// Register a callback for MIDI clock sync events.
    pub fn on_clock_sync(self, callback: FnPtr) -> Self {
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

        self
    }

    /// Register a callback for all MIDI events.
    pub fn on_midi(self, callback: FnPtr) -> Self {
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

        self
    }

    // === MIDI Recording Methods ===

    /// Mark this device for MIDI recording.
    pub fn start_recording(self) -> Self {
        context::with_state(|state| {
            state.midi_inputs.insert(self.id);
            state.midi_recording_requests.push(MidiRecordingRequest {
                device_id: self.id,
                channel: None,
                start: true,
            });
        });
        self
    }

    /// Mark this device for MIDI recording on a specific channel.
    pub fn start_recording_channel(self, channel: i64) -> Self {
        context::with_state(|state| {
            state.midi_inputs.insert(self.id);
            state.midi_recording_requests.push(MidiRecordingRequest {
                device_id: self.id,
                channel: Some(channel.clamp(1, 16) as u8 - 1),
                start: true,
            });
        });
        self
    }

    // === MIDI Clock Output Methods ===

    /// Enable MIDI clock output to this device.
    pub fn enable_clock(self) -> Self {
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_clock_outputs.push(MidiClockOutputRequest {
                device_id: self.id,
                enabled: true,
            });
        });
        self
    }

    /// Disable MIDI clock output to this device.
    pub fn disable_clock(self) -> Self {
        context::with_state(|state| {
            state.midi_clock_outputs.push(MidiClockOutputRequest {
                device_id: self.id,
                enabled: false,
            });
        });
        self
    }

    // === MIDI Transport Messages ===

    /// Send a MIDI Start message (0xFA) to this device.
    ///
    /// This tells the device to start playback from the beginning.
    /// Use this to synchronize external sequencers/drum machines with VibeLang.
    ///
    /// The message is quantized and sent during the next reconciliation cycle,
    /// ensuring it aligns with other musical events like melodies and patterns.
    ///
    /// # Example
    /// ```rhai
    /// let ko2 = midi_device("KO2");
    /// ko2.send_start();  // External device starts in sync
    /// ```
    pub fn send_start(&mut self) {
        let msg = MidiOutputMessage::Start { device_id: self.id };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a MIDI Stop message (0xFC) to this device.
    ///
    /// This tells the device to stop playback.
    ///
    /// # Example
    /// ```rhai
    /// let ko2 = midi_device("KO2");
    /// ko2.send_stop();  // External device stops
    /// ```
    pub fn send_stop(&mut self) {
        let msg = MidiOutputMessage::Stop { device_id: self.id };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    /// Send a MIDI Continue message (0xFB) to this device.
    ///
    /// This tells the device to resume playback from its current position.
    ///
    /// # Example
    /// ```rhai
    /// let ko2 = midi_device("KO2");
    /// ko2.send_continue();  // External device resumes
    /// ```
    pub fn send_continue(&mut self) {
        let msg = MidiOutputMessage::Continue { device_id: self.id };
        context::with_state(|state| {
            state.midi_outputs.insert(self.id);
            state.midi_output_messages.push(msg);
        });
    }

    // === MIDI 2.0 Methods ===

    /// Start building a route for a specific UMP group (0-15).
    
    pub fn group(self, group: i64) -> GroupRoute {
        GroupRoute::new(self.id, group.clamp(0, 15) as u8)
    }

    /// Start building a per-note pitch bend route.
    
    pub fn per_note_pitch_bend(self) -> PerNotePitchBendBuilder {
        PerNotePitchBendBuilder::new(self.id)
    }

    /// Start building a per-note controller route.
    
    pub fn per_note_controller(self, controller: i64) -> PerNoteControllerBuilder {
        PerNoteControllerBuilder::new(self.id, controller.clamp(0, 127) as u8)
    }

    /// Start building a per-note pressure route.
    
    pub fn per_note_pressure(self) -> PerNotePressureBuilder {
        PerNotePressureBuilder::new(self.id)
    }

    /// Start building a high-resolution 32-bit CC route.
    
    pub fn cc32(self, cc: i64) -> Cc32Route {
        Cc32Route::new(self.id, cc.clamp(0, 127) as u8)
    }
}
