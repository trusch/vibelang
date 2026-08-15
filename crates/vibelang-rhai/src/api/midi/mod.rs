//! MIDI API for Rhai scripts.
//!
//! Provides MIDI device management, routing, and output functions.
//! For a full user-facing reference (conventions, builders, examples), see **`README.md`** in this module directory.
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

mod bend_mapping;
mod cc_mapping;
mod device;
mod looper_builder;
mod midi2;
mod recording;
mod routing;

pub use bend_mapping::BendMapping;
pub use cc_mapping::CcMapping;
pub use device::MidiDevice;
pub use looper_builder::LooperBuilder;

pub use midi2::{
    Cc32Route, GroupRoute, PerNoteControllerBuilder, PerNotePitchBendBuilder,
    PerNotePressureBuilder,
};
pub use recording::MidiRecordingHandle;
pub use routing::{CcRoute, KeyboardRoute, NoteRoute};

use rhai::{Array, Dynamic, Engine};
use vibelang_core::midi::{list_pipewire_midi2_inputs, MidiInputIntent};
use vibelang_core::types::MidiDeviceId;

use crate::context;

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
                    channel: 0,
                    default_note: None,
                };
                devices.push(Dynamic::from(device));
                seen_names.insert(name, idx);
            }
        }
    }

    // List output devices
    if let Ok(midi_out) = MidiOutput::new("vibelang-rhai-list") {
        for port in midi_out.ports().iter() {
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
                        channel: 0,
                        default_note: None,
                    };
                    devices.push(Dynamic::from(device));
                }
            }
        }
    }

    for input in list_pipewire_midi2_inputs() {
        let device = MidiDevice {
            id: input.id,
            name: input.name,
            has_input: true,
            has_output: false,
            channel: 0,
            default_note: None,
        };
        devices.push(Dynamic::from(device));
    }

    devices
}

/// Get a MIDI device by name (partial match) or index.
pub fn midi_device(name_or_idx: String) -> MidiDevice {
    use midir::{MidiInput, MidiOutput};

    // Try to parse as index first
    if let Ok(idx) = name_or_idx.parse::<u32>() {
        if let Ok(midi_in) = MidiInput::new("vibelang-rhai-get") {
            if let Some(port) = midi_in.ports().get(idx as usize) {
                if let Ok(name) = midi_in.port_name(port) {
                    return MidiDevice {
                        id: MidiDeviceId::new(idx),
                        name,
                        has_input: true,
                        has_output: false,
                        channel: 0,
                        default_note: None,
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
                        channel: 0,
                        default_note: None,
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
                        channel: 0,
                        default_note: None,
                    };
                }
            }
        }
    }

    let needle = name_or_idx.to_lowercase();
    for input in list_pipewire_midi2_inputs() {
        if input.name.to_lowercase().contains(&needle)
            || input.target_object.to_lowercase().contains(&needle)
        {
            return MidiDevice {
                id: input.id,
                name: input.name,
                has_input: true,
                has_output: false,
                channel: 0,
                default_note: None,
            };
        }
    }

    // Return a placeholder if not found — use sentinel ID so open_output/clock
    // registration fail loudly instead of silently targeting device 0.
    log::warn!(
        "[MIDI] midi_device(\"{}\"): device not found, MIDI operations will be no-ops",
        name_or_idx
    );
    MidiDevice {
        id: MidiDeviceId::new(u32::MAX),
        name: format!("Unknown: {}", name_or_idx),
        has_input: false,
        has_output: false,
        channel: 0,
        default_note: None,
    }
}

/// Get a MIDI device by index.
pub fn midi_device_by_id(id: i64) -> MidiDevice {
    midi_device(id.to_string())
}

/// Declare a stable, optional MIDI-1 input by logical role and exact ALSA client.
pub fn midi_input(role: String, exact_client: String) -> MidiDevice {
    let intent = MidiInputIntent::new(role, exact_client);
    context::with_state(|state| {
        if !state.midi_input_intents.iter().any(|existing| {
            existing.role.eq_ignore_ascii_case(&intent.role)
                && existing
                    .exact_client
                    .eq_ignore_ascii_case(&intent.exact_client)
        }) {
            state.midi_input_intents.push(intent.clone());
        }
    });

    MidiDevice {
        id: intent.device_id,
        name: intent.exact_client,
        has_input: true,
        has_output: false,
        channel: 0,
        default_note: None,
    }
}

/// Resolve a device name (or full port name) to an **output**-port index.
///
/// `midi_device()` searches the *input* port list before the output list, so a
/// device that exposes both directions (e.g. an app like "MODEL 15" that
/// presents a bidirectional ALSA port) is returned with the *input* port index
/// — which is meaningless for output and, once the input list grows (another
/// keyboard plugged in), no longer coincides with the output index. Anything
/// that uses a `MidiDevice` for output (`.on(...)`, MIDI clock out, ...) must
/// re-resolve against the output list; this is that lookup.
pub fn resolve_output_device_id(name: &str) -> Option<MidiDeviceId> {
    use midir::MidiOutput;
    let midi_out = MidiOutput::new("vibelang-rhai-out-resolve").ok()?;
    let ports = midi_out.ports();
    // Exact port-name match first (midi_device stores the full resolved name).
    for (idx, port) in ports.iter().enumerate() {
        if midi_out.port_name(port).is_ok_and(|n| n == name) {
            return Some(MidiDeviceId::new(idx as u32));
        }
    }
    // Fall back to a case-insensitive substring match, mirroring midi_device().
    let needle = name.to_lowercase();
    for (idx, port) in ports.iter().enumerate() {
        if midi_out
            .port_name(port)
            .is_ok_and(|n| n.to_lowercase().contains(&needle))
        {
            return Some(MidiDeviceId::new(idx as u32));
        }
    }
    None
}

/// Register MIDI API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register MidiDevice type
    engine.build_type::<MidiDevice>();

    // Device discovery
    engine.register_fn("list_midi_devices", list_midi_devices);
    engine.register_fn("midi_device", midi_device);
    engine.register_fn("midi_device", midi_device_by_id);
    engine.register_fn("midi_input", midi_input);

    // Getters
    engine.register_fn("id", MidiDevice::get_id);
    engine.register_get("id", MidiDevice::get_id);
    engine.register_fn("name", MidiDevice::get_name);
    engine.register_get("name", MidiDevice::get_name);
    engine.register_fn("has_input", MidiDevice::get_has_input);
    engine.register_get("has_input", MidiDevice::get_has_input);
    engine.register_fn("has_output", MidiDevice::get_has_output);
    engine.register_get("has_output", MidiDevice::get_has_output);
    engine.register_fn("default_note", MidiDevice::get_default_note);
    engine.register_get("default_note", MidiDevice::get_default_note);
    engine.register_fn("get_channel", MidiDevice::get_channel);
    engine.register_get("channel", MidiDevice::get_channel);

    // Channel setter (returns new MidiDevice with channel configured)
    engine.register_fn("channel", MidiDevice::channel);

    // Default note setter (returns new MidiDevice with note configured)
    engine.register_fn("note", MidiDevice::note);

    // Routing methods
    engine.register_fn("route_to", MidiDevice::route_to);
    engine.register_fn("route_to", MidiDevice::route_to_name);
    engine.register_fn("route_to_channel", MidiDevice::route_to_channel);
    // Deprecated CC routing methods
    #[allow(deprecated)]
    {
        engine.register_fn("route_cc", MidiDevice::route_cc_to_voice);
        engine.register_fn("route_cc_to_group", MidiDevice::route_cc_to_group);
    }

    // Device opening (deprecated)
    #[allow(deprecated)]
    {
        engine.register_fn("open_input", MidiDevice::open_input);
        engine.register_fn("open_output", MidiDevice::open_output);
    }

    // Direct MIDI output (MIDI 1.0)
    engine.register_fn("note_on", MidiDevice::note_on);
    engine.register_fn("note_off", MidiDevice::note_off);
    engine.register_fn("cc", MidiDevice::cc);
    engine.register_fn("program_change", MidiDevice::program_change);
    engine.register_fn("pitch_bend", MidiDevice::pitch_bend);

    // Direct MIDI output (MIDI 2.0 high-resolution)
    engine.register_fn("note_on_hires", MidiDevice::note_on_hires);
    engine.register_fn("note_off_hires", MidiDevice::note_off_hires);
    engine.register_fn("cc_hires", MidiDevice::cc_hires);
    engine.register_fn("pitch_bend_hires", MidiDevice::pitch_bend_hires);
    engine.register_fn("send_per_note_bend", MidiDevice::send_per_note_bend);
    engine.register_fn("send_per_note_cc", MidiDevice::send_per_note_cc);
    engine.register_fn("poly_pressure_hires", MidiDevice::poly_pressure_hires);

    // Advanced routing builder methods
    engine.register_fn("keys", MidiDevice::keys);
    engine.register_fn("pad", MidiDevice::pad);
    engine.register_fn("map_cc", MidiDevice::map_cc);
    engine.register_fn("map_bend", MidiDevice::map_bend);
    engine.register_fn("looper", MidiDevice::looper);

    // Register LooperBuilder type
    engine.build_type::<LooperBuilder>();
    engine.register_fn("channel", LooperBuilder::channel);
    engine.register_fn("silence", LooperBuilder::silence);
    engine.register_fn("quantize", LooperBuilder::quantize);
    engine.register_fn("to", LooperBuilder::to);
    // Deprecated aliases
    #[allow(deprecated)]
    {
        engine.register_fn("keyboard_route", MidiDevice::keyboard_route);
        engine.register_fn("note_route", MidiDevice::note_route);
        engine.register_fn("cc_route", MidiDevice::cc_route);
    }

    // Register CcMapping type
    engine.build_type::<CcMapping>();
    engine.register_fn("channel", CcMapping::channel);
    engine.register_fn("curve", CcMapping::curve);
    engine.register_fn("to", CcMapping::to);

    // Register BendMapping type
    engine.build_type::<BendMapping>();
    engine.register_fn("channel", BendMapping::channel);
    engine.register_fn("curve", BendMapping::curve);
    engine.register_fn("to", BendMapping::to);

    // Callback methods
    engine.register_fn("on_note", MidiDevice::on_note);
    engine.register_fn("on_note_channel", MidiDevice::on_note_channel);
    engine.register_fn("on_cc", MidiDevice::on_cc);
    engine.register_fn("on_cc_num", MidiDevice::on_cc_num);
    engine.register_fn("on_clock_sync", MidiDevice::on_clock_sync);
    engine.register_fn("on_midi", MidiDevice::on_midi);

    // Recording methods
    engine.register_fn("start_recording", MidiDevice::start_recording);
    engine.register_fn(
        "start_recording_channel",
        MidiDevice::start_recording_channel,
    );

    // Clock output methods
    engine.register_fn("enable_clock", MidiDevice::enable_clock);
    engine.register_fn("disable_clock", MidiDevice::disable_clock);

    // MIDI transport messages (Start/Stop/Continue)
    engine.register_fn("send_start", MidiDevice::send_start);
    engine.register_fn("send_stop", MidiDevice::send_stop);
    engine.register_fn("send_continue", MidiDevice::send_continue);

    // Register MidiRecordingHandle type
    engine.build_type::<MidiRecordingHandle>();
    engine.register_fn("note_count", MidiRecordingHandle::get_note_count);
    engine.register_get("note_count", MidiRecordingHandle::get_note_count);
    engine.register_fn("cc_count", MidiRecordingHandle::get_cc_count);
    engine.register_get("cc_count", MidiRecordingHandle::get_cc_count);
    engine.register_fn("duration", MidiRecordingHandle::get_duration);
    engine.register_get("duration", MidiRecordingHandle::get_duration);
    engine.register_fn("notes", MidiRecordingHandle::get_notes);
    engine.register_get("notes", MidiRecordingHandle::get_notes);
    engine.register_fn("to_pattern", MidiRecordingHandle::to_pattern_string);

    // Register KeyboardRoute type
    engine.build_type::<KeyboardRoute>();
    engine.register_fn("channel", KeyboardRoute::channel);
    engine.register_fn("range_midi", KeyboardRoute::range_midi);
    engine.register_fn("range", KeyboardRoute::range_midi);
    engine.register_fn("range", KeyboardRoute::range);
    engine.register_fn("transpose", KeyboardRoute::transpose);
    engine.register_fn("octave", KeyboardRoute::octave);
    engine.register_fn("velocity", KeyboardRoute::velocity);
    engine.register_fn("fixed_velocity", KeyboardRoute::fixed_velocity);
    engine.register_fn("to", KeyboardRoute::to);
    engine.register_fn("to", KeyboardRoute::to_name);
    // Deprecated alias
    #[allow(deprecated)]
    engine.register_fn("velocity_curve", KeyboardRoute::velocity_curve);

    // Register NoteRoute type
    engine.build_type::<NoteRoute>();
    engine.register_fn("channel", NoteRoute::channel);
    engine.register_fn("choke", NoteRoute::choke);
    engine.register_fn("velocity_to", NoteRoute::velocity_to);
    engine.register_fn("fixed_velocity", NoteRoute::fixed_velocity);
    engine.register_fn("to", NoteRoute::to);
    // Deprecated alias
    #[allow(deprecated)]
    engine.register_fn("choke_group", NoteRoute::choke_group);

    // Register CcRoute type
    engine.build_type::<CcRoute>();
    engine.register_fn("channel", CcRoute::channel);
    engine.register_fn("curve", CcRoute::curve);
    engine.register_fn("to_param", CcRoute::to_param);
    engine.register_fn("to_param", CcRoute::to_param_name);

    // MIDI 2.0 types and methods

    register_midi2(engine);
}

/// Register MIDI 2.0 types and methods.

fn register_midi2(engine: &mut Engine) {
    // MIDI 2.0 device methods
    engine.register_fn("group", MidiDevice::group);
    engine.register_fn("per_note_pitch_bend", MidiDevice::per_note_pitch_bend);
    engine.register_fn("per_note_controller", MidiDevice::per_note_controller);
    engine.register_fn("per_note_pressure", MidiDevice::per_note_pressure);
    engine.register_fn("cc32", MidiDevice::cc32);

    // Register GroupRoute type
    engine.build_type::<GroupRoute>();
    engine.register_fn("channel", GroupRoute::channel);
    engine.register_fn("range_midi", GroupRoute::range_midi);
    engine.register_fn("range", GroupRoute::range);
    engine.register_fn("transpose", GroupRoute::transpose);
    engine.register_fn("velocity_curve", GroupRoute::velocity_curve);
    engine.register_fn("route_to", GroupRoute::route_to);
    engine.register_fn("route_to", GroupRoute::route_to_name);

    // Register PerNotePitchBendBuilder type
    engine.build_type::<PerNotePitchBendBuilder>();
    engine.register_fn("group", PerNotePitchBendBuilder::group);
    engine.register_fn("channel", PerNotePitchBendBuilder::channel);
    engine.register_fn("range", PerNotePitchBendBuilder::range);
    engine.register_fn("to", PerNotePitchBendBuilder::to);
    engine.register_fn("to", PerNotePitchBendBuilder::to_name);

    // Register PerNoteControllerBuilder type
    engine.build_type::<PerNoteControllerBuilder>();
    engine.register_fn("group", PerNoteControllerBuilder::group);
    engine.register_fn("channel", PerNoteControllerBuilder::channel);
    engine.register_fn("curve", PerNoteControllerBuilder::curve);
    engine.register_fn("to", PerNoteControllerBuilder::to);
    engine.register_fn("to", PerNoteControllerBuilder::to_name);

    // Register PerNotePressureBuilder type
    engine.build_type::<PerNotePressureBuilder>();
    engine.register_fn("group", PerNotePressureBuilder::group);
    engine.register_fn("channel", PerNotePressureBuilder::channel);
    engine.register_fn("curve", PerNotePressureBuilder::curve);
    engine.register_fn("to", PerNotePressureBuilder::to);
    engine.register_fn("to", PerNotePressureBuilder::to_name);

    // Register Cc32Route type
    engine.build_type::<Cc32Route>();
    engine.register_fn("group", Cc32Route::group);
    engine.register_fn("channel", Cc32Route::channel);
    engine.register_fn("curve", Cc32Route::curve);
    engine.register_fn("to", Cc32Route::to);
    engine.register_fn("to", Cc32Route::to_name);
}
