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
use vibelang_core::midi::list_pipewire_midi2_inputs;
use vibelang_core::types::MidiDeviceId;

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

/// Resolve a device name (or full port name) to an **output**-port index.
///
/// `midi_device()` searches the *input* port list before the output list, so a
/// device that exposes both directions (e.g. an app like "MODEL 15" that
/// presents a bidirectional ALSA port) is returned with the *input* port index
/// — which is meaningless for output and, once the input list grows (another
/// keyboard plugged in), no longer coincides with the output index. Anything
/// that uses a `MidiDevice` for voice output via `.on(...)` must re-resolve
/// against the output list; this is the compatibility lookup for that path.
/// Clock and transport use the stricter exact-name endpoint resolver so an
/// ambiguous partial match can never send a realtime message.
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

#[cfg(test)]
mod tests {
    use vibelang_core::midi::resolve_midi_output_endpoint_from;
    use vibelang_core::reload::MidiOutputMessage;
    use vibelang_core::types::MidiDeviceId;

    #[test]
    fn transport_endpoint_retains_output_namespace_identity() {
        // The selected device is input[0], while output[0] is deliberately a
        // different device and the matching stable output is output[1].
        let outputs = vec![
            (MidiDeviceId::new(0), "MPD232".to_string()),
            (MidiDeviceId::new(1), "EP-133".to_string()),
        ];
        let endpoint = resolve_midi_output_endpoint_from("EP-133", &outputs).unwrap();
        let message = MidiOutputMessage::Start {
            endpoint: endpoint.clone(),
        };

        assert_eq!(endpoint.id, MidiDeviceId::new(1));
        assert!(matches!(
            message,
            MidiOutputMessage::Start { endpoint: retained }
                if retained.stable_name == "EP-133" && retained.id == MidiDeviceId::new(1)
        ));
    }

    #[test]
    fn missing_or_ambiguous_transport_output_cannot_form_a_binding() {
        let missing = vec![(MidiDeviceId::new(0), "MPD232".to_string())];
        assert!(resolve_midi_output_endpoint_from("EP-133", &missing).is_err());

        let duplicate = vec![
            (MidiDeviceId::new(0), "EP-133".to_string()),
            (MidiDeviceId::new(2), "EP-133".to_string()),
        ];
        assert!(resolve_midi_output_endpoint_from("EP-133", &duplicate).is_err());
    }
}

// ============================================================================
// Detached v2 MIDI family (M09). Everything below this line sits past the
// frozen v1 manifest anchors; imports live only in this detached section.
// The shared install_v2_api root is owned by the M09 registration
// integration gate; until then only cfg(test) installs reference the
// install helpers.
//
// V1 names with no v2 respelling (migration classifications):
// - `midi_device("partial name")` partial case-insensitive matching and the
//   not-found `u32::MAX` sentinel device: v2 devices are declared with an
//   exact port name and resolution failures are typed
//   `MidiDeviceUnavailability` results, never placeholder handles or logs.
// - channel/value clamping (`channel(99)` → 16, `velocity 300` → 127): v2
//   channels, groups, and width-qualified values reject out-of-range input
//   strictly.
// - `get_channel()` returning the zero-based index while `channel(n)` took
//   1-16: the v2 public surface is uniformly one-based; the zero-based
//   index exists only behind the explicit `channel_index`/`group_index`
//   spellings.
// - `open_input()` / `open_output()` usage-driven auto-open: v2 device
//   declarations carry an explicit direction.
// - `on_note(closure)` captured-AST callbacks: v2 callbacks name a script
//   handler function — candidate IR is pure data.
// - direct `note_on(...)` fire-and-forget queueing into script state: v2
//   output commands are explicit caller-keyed best-effort external-effect
//   submissions and are receipt-bearing through the mutation ledger.
// ============================================================================

use rhai::{EvalAltResult, Position};
use thiserror::Error;
use vibelang_core::candidate::{
    AuthoringDeclaration, CallbackAuthoring, CallbackKind, CallbackTriggerAuthoring,
    CandidateError, CanonicalF64, Composition, DeclarationOwner, DeclarationPayload, EffectKind,
    GroupScope, LifecycleMetadata, MidiChannel, MidiDeviceAuthoring, MidiDeviceDirectionAuthoring,
    MidiDeviceKind, MidiGroup, MidiRouteAuthoring, MidiRouteKind, MidiValue, RouteTargetAuthoring,
    TypedRef, VoiceKind,
};
use vibelang_core::mutation::{
    ExternalEffectDomain, ExternalEffectError, ExternalEffectOperation, ExternalEffectSubmission,
    MutationSource,
};

use super::sequence::EffectRef;
use super::voice::VoiceRef;
use crate::foundation::{self, FoundationError, Observation, RefBase};

/// Structured failure surface of the v2 MIDI family: strict authoring
/// boundaries reject as candidate errors, keyed output commands reject as
/// external-effect errors, and evaluation misuse surfaces as foundation
/// errors — never a log, clamp, or sentinel.
#[derive(Debug, Error)]
pub enum MidiCommandError {
    #[error(transparent)]
    Candidate(#[from] CandidateError),
    #[error(transparent)]
    External(#[from] ExternalEffectError),
    #[error(transparent)]
    Foundation(#[from] FoundationError),
}

/// Stable typed handle to a v2 MIDI device declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiDeviceRef {
    base: RefBase,
}

impl MidiDeviceRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<MidiDeviceKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        let source = foundation::operation_source(&self.base, "remove")?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(
                vibelang_core::candidate::TerminalEffect::Cancel,
                vibelang_core::candidate::Cancellation::RemoveDeclaration,
            ),
            vibelang_core::candidate::LifecycleAction::Remove,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

/// Stable typed handle to a v2 MIDI route declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiRouteRef {
    base: RefBase,
}

impl MidiRouteRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<MidiRouteKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    /// Explicit disconnect terminal: removes this route edge from the next
    /// applied revision. Distinct from `remove` only in its recorded
    /// cancellation mode — both are real operations, never no-ops.
    pub fn disconnect(self) -> Result<Self, FoundationError> {
        let source = foundation::operation_source(&self.base, "disconnect")?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(
                vibelang_core::candidate::TerminalEffect::Cancel,
                vibelang_core::candidate::Cancellation::DisconnectEdge,
            ),
            vibelang_core::candidate::LifecycleAction::Remove,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        let source = foundation::operation_source(&self.base, "remove")?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(
                vibelang_core::candidate::TerminalEffect::Cancel,
                vibelang_core::candidate::Cancellation::RemoveDeclaration,
            ),
            vibelang_core::candidate::LifecycleAction::Remove,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

/// Stable typed handle to a v2 MIDI callback declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallbackRef {
    base: RefBase,
}

impl CallbackRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<CallbackKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        let source = foundation::operation_source(&self.base, "remove")?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(
                vibelang_core::candidate::TerminalEffect::Cancel,
                vibelang_core::candidate::Cancellation::RemoveDeclaration,
            ),
            vibelang_core::candidate::LifecycleAction::Remove,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

fn commit_midi_route(route: MidiRouteAuthoring) -> Result<MidiRouteRef, FoundationError> {
    let key = route.canonical_key()?;
    let base = foundation::authoring_builder::<MidiRouteKind>(key.as_str(), GroupScope::root())?;
    let payload = DeclarationPayload::authoring(AuthoringDeclaration::MidiRoute(route))?;
    let owner = DeclarationOwner::Structural(base.source().syntax_key().clone());
    MidiRouteRef::new(base.apply(
        owner,
        LifecycleMetadata::register(Composition::Standalone),
        payload,
    )?)
}

fn commit_callback(callback: CallbackAuthoring) -> Result<CallbackRef, FoundationError> {
    let key = callback.canonical_key()?;
    let base = foundation::authoring_builder::<CallbackKind>(key.as_str(), GroupScope::root())?;
    let payload = DeclarationPayload::authoring(AuthoringDeclaration::Callback(callback))?;
    let owner = DeclarationOwner::Structural(base.source().syntax_key().clone());
    CallbackRef::new(base.apply(
        owner,
        LifecycleMetadata::register(Composition::Standalone),
        payload,
    )?)
}

/// Pure v2 MIDI device builder: one exact port name, an explicit
/// direction, and a register terminal returning a typed Ref.
#[derive(Clone, Debug)]
pub struct MidiDeviceBuilder {
    base: foundation::BuilderBase,
    port: Option<String>,
    direction: MidiDeviceDirectionAuthoring,
}

impl MidiDeviceBuilder {
    #[must_use]
    pub fn new(base: foundation::BuilderBase) -> Self {
        Self {
            base,
            port: None,
            direction: MidiDeviceDirectionAuthoring::Bidirectional,
        }
    }

    /// Bind the exact port name. Partial matching has no v2 respelling.
    #[must_use]
    pub fn port(mut self, port: String) -> Self {
        self.port = Some(port);
        self
    }

    #[must_use]
    pub fn input(mut self) -> Self {
        self.direction = MidiDeviceDirectionAuthoring::Input;
        self
    }

    #[must_use]
    pub fn output(mut self) -> Self {
        self.direction = MidiDeviceDirectionAuthoring::Output;
        self
    }

    #[must_use]
    pub fn bidirectional(mut self) -> Self {
        self.direction = MidiDeviceDirectionAuthoring::Bidirectional;
        self
    }

    /// Register terminal: declare the device binding and return its Ref.
    pub fn apply(self) -> Result<MidiDeviceRef, FoundationError> {
        let port = self.port.ok_or_else(|| {
            CandidateError::InvalidAuthoring(
                "a v2 MIDI device declares its exact port name before apply: use .port(name)"
                    .into(),
            )
        })?;
        let payload =
            DeclarationPayload::authoring(AuthoringDeclaration::MidiDevice(MidiDeviceAuthoring {
                port,
                direction: self.direction,
            }))?;
        let owner = DeclarationOwner::Structural(self.base.source().syntax_key().clone());
        MidiDeviceRef::new(self.base.apply(
            owner,
            LifecycleMetadata::register(Composition::Standalone),
            payload,
        )?)
    }
}

/// A point-in-time inventory of the host's MIDI ports, injected so
/// resolution stays pure and testable. The integration gate wires the
/// live midir/pipewire enumeration into this shape.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MidiPortDirectory {
    inputs: Vec<String>,
    outputs: Vec<String>,
}

impl MidiPortDirectory {
    #[must_use]
    pub fn new(inputs: Vec<String>, outputs: Vec<String>) -> Self {
        Self { inputs, outputs }
    }

    fn count(&self, port: &str, direction: &'static str) -> usize {
        let list = match direction {
            "input" => &self.inputs,
            _ => &self.outputs,
        };
        list.iter().filter(|entry| entry.as_str() == port).count()
    }
}

/// Typed unavailable-device result. The v1 surface logged a warning and
/// handed back a `u32::MAX` sentinel device; v2 resolution failures are
/// values a script or transport can match on.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum MidiDeviceUnavailability {
    #[error("MIDI {direction} port {port:?} is not present in the current port inventory")]
    NotFound {
        port: String,
        direction: &'static str,
    },
    #[error(
        "MIDI {direction} port {port:?} was resolved earlier but has disappeared from the \
         current port inventory"
    )]
    Disappeared {
        port: String,
        direction: &'static str,
    },
    #[error(
        "MIDI {direction} port {port:?} appears {count} times in the current port inventory; \
         an ambiguous binding cannot be used"
    )]
    Ambiguous {
        port: String,
        direction: &'static str,
        count: usize,
    },
}

fn required_directions(direction: MidiDeviceDirectionAuthoring) -> &'static [&'static str] {
    match direction {
        MidiDeviceDirectionAuthoring::Input => &["input"],
        MidiDeviceDirectionAuthoring::Output => &["output"],
        MidiDeviceDirectionAuthoring::Bidirectional => &["input", "output"],
    }
}

fn check_port(
    port: &str,
    direction: MidiDeviceDirectionAuthoring,
    directory: &MidiPortDirectory,
    missing_is_disappearance: bool,
) -> Result<(), MidiDeviceUnavailability> {
    for required in required_directions(direction) {
        match directory.count(port, required) {
            0 if missing_is_disappearance => {
                return Err(MidiDeviceUnavailability::Disappeared {
                    port: port.into(),
                    direction: required,
                })
            }
            0 => {
                return Err(MidiDeviceUnavailability::NotFound {
                    port: port.into(),
                    direction: required,
                })
            }
            1 => {}
            count => {
                return Err(MidiDeviceUnavailability::Ambiguous {
                    port: port.into(),
                    direction: required,
                    count,
                })
            }
        }
    }
    Ok(())
}

/// A device binding proven present in one port inventory. Re-verifying
/// against a fresh inventory turns a vanished port into the typed
/// disappearance result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedMidiDevice {
    port: String,
    direction: MidiDeviceDirectionAuthoring,
}

impl ResolvedMidiDevice {
    #[must_use]
    pub fn port(&self) -> &str {
        &self.port
    }

    /// Re-check this binding against a fresh inventory.
    pub fn verify(&self, directory: &MidiPortDirectory) -> Result<(), MidiDeviceUnavailability> {
        check_port(&self.port, self.direction, directory, true)
    }
}

/// Resolve an exact port name against an injected inventory.
pub fn resolve_midi_device_v2(
    port: &str,
    direction: MidiDeviceDirectionAuthoring,
    directory: &MidiPortDirectory,
) -> Result<ResolvedMidiDevice, MidiDeviceUnavailability> {
    check_port(port, direction, directory, false)?;
    Ok(ResolvedMidiDevice {
        port: port.into(),
        direction,
    })
}

/// Pure keyboard-route builder: one edge per device/channel/voice triple.
#[derive(Clone, Debug)]
pub struct MidiKeyboardRouteBuilder {
    device: TypedRef<MidiDeviceKind>,
    channel: Option<MidiChannel>,
}

impl MidiKeyboardRouteBuilder {
    pub fn new(device: &MidiDeviceRef) -> Result<Self, FoundationError> {
        Ok(Self {
            device: device.base().typed::<MidiDeviceKind>()?,
            channel: None,
        })
    }

    /// Listen on one public channel, 1..=16.
    pub fn channel(mut self, number: i64) -> Result<Self, FoundationError> {
        self.channel = Some(MidiChannel::from_number(number)?);
        Ok(self)
    }

    /// Explicit zero-based spelling, 0..=15.
    pub fn channel_index(mut self, index: i64) -> Result<Self, FoundationError> {
        self.channel = Some(MidiChannel::from_index(index)?);
        Ok(self)
    }

    /// Terminal: declare the keyboard edge onto a voice.
    pub fn to(self, voice: &VoiceRef) -> Result<MidiRouteRef, FoundationError> {
        commit_midi_route(MidiRouteAuthoring::Keyboard {
            device: self.device,
            channel: self.channel,
            voice: voice.base().typed::<VoiceKind>()?,
        })
    }

    /// Effective forwarding alias for [`Self::to`] (v1 spelling).
    pub fn route_to(self, voice: &VoiceRef) -> Result<MidiRouteRef, FoundationError> {
        self.to(voice)
    }
}

/// Pure CC-route builder: target-side single writer per target parameter.
#[derive(Clone, Debug)]
pub struct MidiCcRouteBuilder {
    device: TypedRef<MidiDeviceKind>,
    channel: Option<MidiChannel>,
    controller: u8,
}

impl MidiCcRouteBuilder {
    pub fn new(device: &MidiDeviceRef, controller: i64) -> Result<Self, FoundationError> {
        if !(0..=127).contains(&controller) {
            return Err(CandidateError::InvalidAuthoring(format!(
                "MIDI controller numbers are 0..=127, got {controller}"
            ))
            .into());
        }
        Ok(Self {
            device: device.base().typed::<MidiDeviceKind>()?,
            channel: None,
            controller: controller as u8,
        })
    }

    pub fn channel(mut self, number: i64) -> Result<Self, FoundationError> {
        self.channel = Some(MidiChannel::from_number(number)?);
        Ok(self)
    }

    pub fn channel_index(mut self, index: i64) -> Result<Self, FoundationError> {
        self.channel = Some(MidiChannel::from_index(index)?);
        Ok(self)
    }

    fn commit(
        self,
        target: RouteTargetAuthoring,
        target_param: String,
        min: f64,
        max: f64,
    ) -> Result<MidiRouteRef, FoundationError> {
        commit_midi_route(MidiRouteAuthoring::Cc {
            device: self.device,
            channel: self.channel,
            controller: self.controller,
            target,
            target_param,
            min: CanonicalF64::new(min)?,
            max: CanonicalF64::new(max)?,
        })
    }

    /// Terminal: this controller becomes the single CC writer of the
    /// target voice parameter.
    pub fn to(
        self,
        target: &VoiceRef,
        target_param: String,
        min: f64,
        max: f64,
    ) -> Result<MidiRouteRef, FoundationError> {
        let target = RouteTargetAuthoring::Voice(target.base().typed::<VoiceKind>()?);
        self.commit(target, target_param, min, max)
    }

    /// Fx-target variant.
    pub fn to_fx(
        self,
        target: &EffectRef,
        target_param: String,
        min: f64,
        max: f64,
    ) -> Result<MidiRouteRef, FoundationError> {
        let target = RouteTargetAuthoring::Effect(target.base().typed::<EffectKind>()?);
        self.commit(target, target_param, min, max)
    }
}

#[derive(Clone, Debug)]
enum CallbackTriggerConfig {
    NoteOn {
        channel: Option<MidiChannel>,
        note: Option<u8>,
    },
    ControlChange {
        channel: Option<MidiChannel>,
        controller: Option<u8>,
    },
    ClockSync,
    AnyMessage,
}

/// Pure v2 MIDI callback builder. The handler is a named script function.
#[derive(Clone, Debug)]
pub struct MidiCallbackBuilder {
    device: TypedRef<MidiDeviceKind>,
    trigger: CallbackTriggerConfig,
    handler: Option<String>,
}

impl MidiCallbackBuilder {
    pub fn new(device: &MidiDeviceRef) -> Result<Self, FoundationError> {
        Ok(Self {
            device: device.base().typed::<MidiDeviceKind>()?,
            trigger: CallbackTriggerConfig::AnyMessage,
            handler: None,
        })
    }

    #[must_use]
    pub fn on_note(mut self) -> Self {
        self.trigger = CallbackTriggerConfig::NoteOn {
            channel: None,
            note: None,
        };
        self
    }

    #[must_use]
    pub fn on_cc(mut self) -> Self {
        self.trigger = CallbackTriggerConfig::ControlChange {
            channel: None,
            controller: None,
        };
        self
    }

    #[must_use]
    pub fn on_clock(mut self) -> Self {
        self.trigger = CallbackTriggerConfig::ClockSync;
        self
    }

    #[must_use]
    pub fn on_any(mut self) -> Self {
        self.trigger = CallbackTriggerConfig::AnyMessage;
        self
    }

    pub fn note(mut self, note: i64) -> Result<Self, FoundationError> {
        let CallbackTriggerConfig::NoteOn { channel, .. } = self.trigger else {
            return Err(CandidateError::InvalidAuthoring(
                "a note filter needs an on_note trigger".into(),
            )
            .into());
        };
        if !(0..=127).contains(&note) {
            return Err(CandidateError::InvalidAuthoring(format!(
                "MIDI note numbers are 0..=127, got {note}"
            ))
            .into());
        }
        self.trigger = CallbackTriggerConfig::NoteOn {
            channel,
            note: Some(note as u8),
        };
        Ok(self)
    }

    pub fn controller(mut self, controller: i64) -> Result<Self, FoundationError> {
        let CallbackTriggerConfig::ControlChange { channel, .. } = self.trigger else {
            return Err(CandidateError::InvalidAuthoring(
                "a controller filter needs an on_cc trigger".into(),
            )
            .into());
        };
        if !(0..=127).contains(&controller) {
            return Err(CandidateError::InvalidAuthoring(format!(
                "MIDI controller numbers are 0..=127, got {controller}"
            ))
            .into());
        }
        self.trigger = CallbackTriggerConfig::ControlChange {
            channel,
            controller: Some(controller as u8),
        };
        Ok(self)
    }

    fn set_channel(mut self, channel: MidiChannel) -> Result<Self, FoundationError> {
        self.trigger = match self.trigger {
            CallbackTriggerConfig::NoteOn { note, .. } => CallbackTriggerConfig::NoteOn {
                channel: Some(channel),
                note,
            },
            CallbackTriggerConfig::ControlChange { controller, .. } => {
                CallbackTriggerConfig::ControlChange {
                    channel: Some(channel),
                    controller,
                }
            }
            CallbackTriggerConfig::ClockSync | CallbackTriggerConfig::AnyMessage => {
                return Err(CandidateError::InvalidAuthoring(
                    "a channel filter needs an on_note or on_cc trigger".into(),
                )
                .into())
            }
        };
        Ok(self)
    }

    pub fn channel(self, number: i64) -> Result<Self, FoundationError> {
        let channel = MidiChannel::from_number(number)?;
        self.set_channel(channel)
    }

    pub fn channel_index(self, index: i64) -> Result<Self, FoundationError> {
        let channel = MidiChannel::from_index(index)?;
        self.set_channel(channel)
    }

    #[must_use]
    pub fn handler(mut self, handler: String) -> Self {
        self.handler = Some(handler);
        self
    }

    /// Register terminal: declare the callback edge and return its Ref.
    pub fn apply(self) -> Result<CallbackRef, FoundationError> {
        let handler = self.handler.ok_or_else(|| {
            CandidateError::InvalidAuthoring(
                "a v2 MIDI callback names its script handler before apply: use .handler(name)"
                    .into(),
            )
        })?;
        let trigger = match self.trigger {
            CallbackTriggerConfig::NoteOn { channel, note } => {
                CallbackTriggerAuthoring::NoteOn { channel, note }
            }
            CallbackTriggerConfig::ControlChange {
                channel,
                controller,
            } => CallbackTriggerAuthoring::ControlChange {
                channel,
                controller,
            },
            CallbackTriggerConfig::ClockSync => CallbackTriggerAuthoring::ClockSync,
            CallbackTriggerConfig::AnyMessage => CallbackTriggerAuthoring::AnyMessage,
        };
        commit_callback(CallbackAuthoring {
            device: self.device,
            trigger,
            handler,
        })
    }
}

/// One authored, keyed, receipt-bearing MIDI output command: a wrapped
/// external-effect submission on the M09 best-effort model.
#[derive(Clone, Debug)]
pub struct MidiOutputCommand {
    submission: ExternalEffectSubmission,
}

impl MidiOutputCommand {
    #[must_use]
    pub fn submission(&self) -> &ExternalEffectSubmission {
        &self.submission
    }

    #[must_use]
    pub fn idempotency_key(&self) -> &str {
        self.submission.idempotency_key()
    }

    #[must_use]
    pub fn qualified_operation(&self) -> String {
        self.submission.operation().qualified_operation()
    }
}

/// Pure v2 MIDI output-command builder over one declared device.
///
/// Channels and MIDI 2.0 groups are public 1-16 (zero-based only via the
/// explicit `_index` spellings), every data value is width-qualified and
/// strict, and every terminal requires a caller-provided idempotency key
/// before it will author the best-effort submission.
#[derive(Clone, Debug)]
pub struct MidiOutBuilder {
    device: RefBase,
    channel: MidiChannel,
    group: MidiGroup,
    key: Option<String>,
}

impl MidiOutBuilder {
    pub fn new(device: &MidiDeviceRef) -> Result<Self, FoundationError> {
        device.base().typed::<MidiDeviceKind>()?;
        Ok(Self {
            device: device.base().clone(),
            channel: MidiChannel::from_number(1).expect("channel 1 is canonical"),
            group: MidiGroup::from_number(1).expect("group 1 is canonical"),
            key: None,
        })
    }

    pub fn channel(mut self, number: i64) -> Result<Self, MidiCommandError> {
        self.channel = MidiChannel::from_number(number)?;
        Ok(self)
    }

    pub fn channel_index(mut self, index: i64) -> Result<Self, MidiCommandError> {
        self.channel = MidiChannel::from_index(index)?;
        Ok(self)
    }

    pub fn group(mut self, number: i64) -> Result<Self, MidiCommandError> {
        self.group = MidiGroup::from_number(number)?;
        Ok(self)
    }

    pub fn group_index(mut self, index: i64) -> Result<Self, MidiCommandError> {
        self.group = MidiGroup::from_index(index)?;
        Ok(self)
    }

    /// The caller-provided idempotency key for the next terminal.
    #[must_use]
    pub fn key(mut self, key: String) -> Self {
        self.key = Some(key);
        self
    }

    fn command(
        &self,
        operation: &'static str,
        values: Vec<(u8, u32)>,
    ) -> Result<MidiOutputCommand, MidiCommandError> {
        let Some(key) = self.key.clone() else {
            return Err(ExternalEffectError::MissingIdempotencyKey {
                domain: ExternalEffectDomain::Midi,
                operation: operation.into(),
            }
            .into());
        };
        let semantic = (
            "vibelang.v2.midi-output",
            operation,
            self.device.address().to_string(),
            self.channel.number(),
            self.group.number(),
            values,
        );
        let operation_ir = ExternalEffectOperation::new(
            ExternalEffectDomain::Midi,
            operation,
            &semantic,
            Some(&semantic),
        )?;
        let submission = ExternalEffectSubmission::new(
            operation_ir,
            key,
            MutationSource::Rhai {
                engine_id: self.device.identity().engine_instance().to_string(),
            },
            "vibelang.v2.local",
        )?;
        Ok(MidiOutputCommand { submission })
    }

    fn pair(value: MidiValue) -> (u8, u32) {
        (value.width_bits(), value.raw())
    }

    /// 7-bit note-on.
    pub fn note_on(&self, note: i64, velocity: i64) -> Result<MidiOutputCommand, MidiCommandError> {
        let note = MidiValue::v7(note)?;
        let velocity = MidiValue::v7(velocity)?;
        self.command("note_on", vec![Self::pair(note), Self::pair(velocity)])
    }

    /// 16-bit-velocity note-on (MIDI 2.0).
    pub fn note_on16(
        &self,
        note: i64,
        velocity: i64,
    ) -> Result<MidiOutputCommand, MidiCommandError> {
        let note = MidiValue::v7(note)?;
        let velocity = MidiValue::v16(velocity)?;
        self.command("note_on16", vec![Self::pair(note), Self::pair(velocity)])
    }

    /// 7-bit note-off.
    pub fn note_off(&self, note: i64) -> Result<MidiOutputCommand, MidiCommandError> {
        let note = MidiValue::v7(note)?;
        self.command("note_off", vec![Self::pair(note)])
    }

    /// 16-bit-velocity note-off (MIDI 2.0).
    pub fn note_off16(
        &self,
        note: i64,
        velocity: i64,
    ) -> Result<MidiOutputCommand, MidiCommandError> {
        let note = MidiValue::v7(note)?;
        let velocity = MidiValue::v16(velocity)?;
        self.command("note_off16", vec![Self::pair(note), Self::pair(velocity)])
    }

    /// 7-bit control change.
    pub fn cc(&self, controller: i64, value: i64) -> Result<MidiOutputCommand, MidiCommandError> {
        let controller = MidiValue::v7(controller)?;
        let value = MidiValue::v7(value)?;
        self.command("cc", vec![Self::pair(controller), Self::pair(value)])
    }

    /// 32-bit control change (MIDI 2.0).
    pub fn cc32(&self, controller: i64, value: i64) -> Result<MidiOutputCommand, MidiCommandError> {
        let controller = MidiValue::v7(controller)?;
        let value = MidiValue::v32(value)?;
        self.command("cc32", vec![Self::pair(controller), Self::pair(value)])
    }

    /// 7-bit program change.
    pub fn program_change(&self, program: i64) -> Result<MidiOutputCommand, MidiCommandError> {
        let program = MidiValue::v7(program)?;
        self.command("program_change", vec![Self::pair(program)])
    }

    /// 14-bit pitch bend, signed musician spelling -8192..=8191.
    pub fn pitch_bend14(&self, value: i64) -> Result<MidiOutputCommand, MidiCommandError> {
        if !(-8192..=8191).contains(&value) {
            return Err(CandidateError::InvalidAuthoring(format!(
                "14-bit pitch bend values are -8192..=8191, got {value}"
            ))
            .into());
        }
        let raw = MidiValue::v14(value + 8192)?;
        self.command("pitch_bend14", vec![Self::pair(raw)])
    }

    /// 32-bit pitch bend (MIDI 2.0), raw unsigned.
    pub fn pitch_bend32(&self, value: i64) -> Result<MidiOutputCommand, MidiCommandError> {
        let raw = MidiValue::v32(value)?;
        self.command("pitch_bend32", vec![Self::pair(raw)])
    }

    /// Effective forwarding alias for [`Self::note_on16`] (v1 spelling).
    pub fn note_on_hires(
        &self,
        note: i64,
        velocity: i64,
    ) -> Result<MidiOutputCommand, MidiCommandError> {
        self.note_on16(note, velocity)
    }

    /// Effective forwarding alias for [`Self::note_off16`] (v1 spelling).
    pub fn note_off_hires(
        &self,
        note: i64,
        velocity: i64,
    ) -> Result<MidiOutputCommand, MidiCommandError> {
        self.note_off16(note, velocity)
    }

    /// Effective forwarding alias for [`Self::cc32`] (v1 spelling).
    pub fn cc_hires(
        &self,
        controller: i64,
        value: i64,
    ) -> Result<MidiOutputCommand, MidiCommandError> {
        self.cc32(controller, value)
    }

    /// Effective forwarding alias for [`Self::pitch_bend32`] (v1 spelling).
    pub fn pitch_bend_hires(&self, value: i64) -> Result<MidiOutputCommand, MidiCommandError> {
        self.pitch_bend32(value)
    }
}

// Wired into the shared install_v2_api root by the M09 registration
// integration gate; until then only cfg(test) installs reference it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn midi_device_v2(name: String) -> Result<MidiDeviceBuilder, Box<EvalAltResult>> {
    Ok(MidiDeviceBuilder::new(
        foundation::authoring_builder::<MidiDeviceKind>(&name, GroupScope::root())
            .map_err(|error| midi_v2_error(&error))?,
    ))
}

// Wired into the shared install_v2_api root by the M09 registration
// integration gate; until then only cfg(test) installs reference it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn midi_device_ref_v2(name: String) -> Result<MidiDeviceRef, Box<EvalAltResult>> {
    MidiDeviceRef::new(
        foundation::authoring_ref::<MidiDeviceKind>(&name, GroupScope::root())
            .map_err(|error| midi_v2_error(&error))?,
    )
    .map_err(|error| midi_v2_error(&error))
}

// Wired into the shared install_v2_api root by the M09 registration
// integration gate; until then only cfg(test) installs reference it.
#[cfg_attr(not(test), allow(dead_code))]
fn midi_v2_error(error: &dyn std::fmt::Display) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        error.to_string().into(),
        Position::NONE,
    ))
}

#[cfg(test)]
fn install_v2_for_tests(engine: &mut Engine) {
    fn strict<T, E: std::fmt::Display>(result: Result<T, E>) -> Result<T, Box<EvalAltResult>> {
        result.map_err(|error| midi_v2_error(&error))
    }

    engine
        .register_type_with_name::<MidiDeviceBuilder>("MidiDeviceBuilder")
        .register_type_with_name::<MidiDeviceRef>("MidiDeviceRef")
        .register_type_with_name::<MidiRouteRef>("MidiRouteRef")
        .register_type_with_name::<CallbackRef>("CallbackRef")
        .register_type_with_name::<MidiKeyboardRouteBuilder>("MidiKeyboardRouteBuilder")
        .register_type_with_name::<MidiCcRouteBuilder>("MidiCcRouteBuilder")
        .register_type_with_name::<MidiCallbackBuilder>("MidiCallbackBuilder")
        .register_type_with_name::<MidiOutBuilder>("MidiOutBuilder")
        .register_type_with_name::<MidiOutputCommand>("MidiOutputCommand")
        .register_type_with_name::<MidiChannel>("MidiChannel")
        .register_type_with_name::<MidiGroup>("MidiGroup")
        .register_fn("voice_ref", super::voice::voice_ref_v2)
        .register_fn("midi_device", midi_device_v2)
        .register_fn("midi_device_ref", midi_device_ref_v2)
        .register_fn("midi_channel", |number: i64| {
            strict(MidiChannel::from_number(number))
        })
        .register_fn("midi_channel_index", |index: i64| {
            strict(MidiChannel::from_index(index))
        })
        .register_fn("midi_group", |number: i64| {
            strict(MidiGroup::from_number(number))
        })
        .register_fn("midi_group_index", |index: i64| {
            strict(MidiGroup::from_index(index))
        })
        .register_fn("port", MidiDeviceBuilder::port)
        .register_fn("input", MidiDeviceBuilder::input)
        .register_fn("output", MidiDeviceBuilder::output)
        .register_fn("bidirectional", MidiDeviceBuilder::bidirectional)
        .register_fn("apply", |builder: MidiDeviceBuilder| {
            strict(builder.apply())
        })
        .register_fn("keyboard_route", |device: MidiDeviceRef| {
            strict(MidiKeyboardRouteBuilder::new(&device))
        })
        .register_fn(
            "channel",
            |builder: MidiKeyboardRouteBuilder, number: i64| strict(builder.channel(number)),
        )
        .register_fn(
            "channel_index",
            |builder: MidiKeyboardRouteBuilder, index: i64| strict(builder.channel_index(index)),
        )
        .register_fn(
            "to",
            |builder: MidiKeyboardRouteBuilder, voice: VoiceRef| strict(builder.to(&voice)),
        )
        .register_fn(
            "route_to",
            |builder: MidiKeyboardRouteBuilder, voice: VoiceRef| strict(builder.route_to(&voice)),
        )
        .register_fn("cc_route", |device: MidiDeviceRef, controller: i64| {
            strict(MidiCcRouteBuilder::new(&device, controller))
        })
        .register_fn("channel", |builder: MidiCcRouteBuilder, number: i64| {
            strict(builder.channel(number))
        })
        .register_fn(
            "to",
            |builder: MidiCcRouteBuilder, target: VoiceRef, param: String, min: f64, max: f64| {
                strict(builder.to(&target, param, min, max))
            },
        )
        .register_fn("midi_callback", |device: MidiDeviceRef| {
            strict(MidiCallbackBuilder::new(&device))
        })
        .register_fn("on_note", MidiCallbackBuilder::on_note)
        .register_fn("on_cc", MidiCallbackBuilder::on_cc)
        .register_fn("on_clock", MidiCallbackBuilder::on_clock)
        .register_fn("on_any", MidiCallbackBuilder::on_any)
        .register_fn("note", |builder: MidiCallbackBuilder, note: i64| {
            strict(builder.note(note))
        })
        .register_fn(
            "controller",
            |builder: MidiCallbackBuilder, controller: i64| strict(builder.controller(controller)),
        )
        .register_fn("channel", |builder: MidiCallbackBuilder, number: i64| {
            strict(builder.channel(number))
        })
        .register_fn(
            "channel_index",
            |builder: MidiCallbackBuilder, index: i64| strict(builder.channel_index(index)),
        )
        .register_fn("handler", MidiCallbackBuilder::handler)
        .register_fn("apply", |builder: MidiCallbackBuilder| {
            strict(builder.apply())
        })
        .register_fn("midi_out", |device: MidiDeviceRef| {
            strict(MidiOutBuilder::new(&device))
        })
        .register_fn("channel", |builder: MidiOutBuilder, number: i64| {
            strict(builder.channel(number))
        })
        .register_fn("channel_index", |builder: MidiOutBuilder, index: i64| {
            strict(builder.channel_index(index))
        })
        .register_fn("group", |builder: MidiOutBuilder, number: i64| {
            strict(builder.group(number))
        })
        .register_fn("group_index", |builder: MidiOutBuilder, index: i64| {
            strict(builder.group_index(index))
        })
        .register_fn("key", MidiOutBuilder::key)
        .register_fn(
            "note_on",
            |builder: MidiOutBuilder, note: i64, velocity: i64| {
                strict(builder.note_on(note, velocity))
            },
        )
        .register_fn(
            "note_on16",
            |builder: MidiOutBuilder, note: i64, velocity: i64| {
                strict(builder.note_on16(note, velocity))
            },
        )
        .register_fn("note_off", |builder: MidiOutBuilder, note: i64| {
            strict(builder.note_off(note))
        })
        .register_fn(
            "note_off16",
            |builder: MidiOutBuilder, note: i64, velocity: i64| {
                strict(builder.note_off16(note, velocity))
            },
        )
        .register_fn(
            "cc",
            |builder: MidiOutBuilder, controller: i64, value: i64| {
                strict(builder.cc(controller, value))
            },
        )
        .register_fn(
            "cc32",
            |builder: MidiOutBuilder, controller: i64, value: i64| {
                strict(builder.cc32(controller, value))
            },
        )
        .register_fn("program_change", |builder: MidiOutBuilder, program: i64| {
            strict(builder.program_change(program))
        })
        .register_fn("pitch_bend14", |builder: MidiOutBuilder, value: i64| {
            strict(builder.pitch_bend14(value))
        })
        .register_fn("pitch_bend32", |builder: MidiOutBuilder, value: i64| {
            strict(builder.pitch_bend32(value))
        })
        .register_fn(
            "note_on_hires",
            |builder: MidiOutBuilder, note: i64, velocity: i64| {
                strict(builder.note_on_hires(note, velocity))
            },
        )
        .register_fn(
            "cc_hires",
            |builder: MidiOutBuilder, controller: i64, value: i64| {
                strict(builder.cc_hires(controller, value))
            },
        )
        .register_fn("pitch_bend_hires", |builder: MidiOutBuilder, value: i64| {
            strict(builder.pitch_bend_hires(value))
        })
        .register_fn("remove", |reference: MidiDeviceRef| {
            strict(reference.remove())
        })
        .register_fn("remove", |reference: MidiRouteRef| {
            strict(reference.remove())
        })
        .register_fn("disconnect", |reference: MidiRouteRef| {
            strict(reference.disconnect())
        })
        .register_fn("remove", |reference: CallbackRef| {
            strict(reference.remove())
        })
        .register_fn("status", |reference: MidiDeviceRef| {
            strict(reference.status())
        })
        .register_fn("status", |reference: MidiRouteRef| {
            strict(reference.status())
        })
        .register_fn("status", |reference: CallbackRef| {
            strict(reference.status())
        });
}

#[cfg(test)]
mod v2_tests {
    use super::*;
    use vibelang_core::candidate::{EntityKind, RefKind};
    use vibelang_core::mutation::{
        Atomicity, CandidateOrigin, CandidateSubmission, MessageDomain, MutationKind,
        RequestMaterial, RuntimeEpoch, Submission, SupersessionPolicy,
    };

    fn v2_identity() -> vibelang_core::candidate::EvaluationIdentity {
        vibelang_core::candidate::EvaluationIdentity::new(
            vibelang_core::candidate::LanguageContract::v2(
                vibelang_core::candidate::ContractDigest::from_bytes(b"midi-v2-test"),
            ),
            vibelang_core::candidate::EngineInstanceId::new(),
            RuntimeEpoch::new(),
        )
    }

    fn declare_empty<K: RefKind>(key: &str) {
        let builder = foundation::authoring_builder::<K>(key, GroupScope::root()).unwrap();
        let owner = DeclarationOwner::Structural(builder.source().syntax_key().clone());
        builder
            .apply(
                owner,
                LifecycleMetadata::register(Composition::Standalone),
                DeclarationPayload::Empty,
            )
            .unwrap();
    }

    fn voice_ref(name: &str) -> VoiceRef {
        super::super::voice::voice_ref_v2(name.into()).unwrap()
    }

    fn declared_device(key: &str, port: &str) -> MidiDeviceRef {
        midi_device_v2(key.into())
            .unwrap()
            .port(port.into())
            .input()
            .apply()
            .unwrap()
    }

    #[test]
    fn v2_midi_device_resolution_failures_are_typed_values() {
        let directory = MidiPortDirectory::new(
            vec!["MPK Mini".into()],
            vec!["MPK Mini".into(), "EP-133".into(), "EP-133".into()],
        );

        let resolved = resolve_midi_device_v2(
            "MPK Mini",
            MidiDeviceDirectionAuthoring::Bidirectional,
            &directory,
        )
        .unwrap();
        assert_eq!(resolved.port(), "MPK Mini");
        assert_eq!(resolved.verify(&directory), Ok(()));

        assert!(matches!(
            resolve_midi_device_v2("Ghost", MidiDeviceDirectionAuthoring::Input, &directory),
            Err(MidiDeviceUnavailability::NotFound {
                port,
                direction: "input",
            }) if port == "Ghost"
        ));
        assert!(matches!(
            resolve_midi_device_v2("EP-133", MidiDeviceDirectionAuthoring::Output, &directory),
            Err(MidiDeviceUnavailability::Ambiguous { count: 2, .. })
        ));
        assert!(
            matches!(
                resolve_midi_device_v2(
                    "EP-133",
                    MidiDeviceDirectionAuthoring::Bidirectional,
                    &directory,
                ),
                Err(MidiDeviceUnavailability::NotFound {
                    direction: "input",
                    ..
                })
            ),
            "a bidirectional binding needs both directions present"
        );

        let unplugged = MidiPortDirectory::default();
        assert!(
            matches!(
                resolved.verify(&unplugged),
                Err(MidiDeviceUnavailability::Disappeared {
                    direction: "input",
                    ..
                })
            ),
            "a vanished port is the typed disappearance result, not a stale handle"
        );
    }

    #[test]
    fn v2_midi_builders_reject_out_of_range_strictly() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let device = declared_device("mpk", "MPK Mini");
        let out = MidiOutBuilder::new(&device).unwrap();

        for number in [0, 17, -1] {
            assert!(out.clone().channel(number).is_err(), "channel {number}");
            assert!(out.clone().group(number).is_err(), "group {number}");
            assert!(
                MidiKeyboardRouteBuilder::new(&device)
                    .unwrap()
                    .channel(number)
                    .is_err(),
                "keyboard channel {number}"
            );
        }
        for number in [1, 16] {
            assert!(out.clone().channel(number).is_ok(), "channel {number}");
            assert!(out.clone().group(number).is_ok(), "group {number}");
        }
        assert_eq!(out.clone().channel_index(15).unwrap().channel.number(), 16);
        assert!(out.clone().channel_index(16).is_err());
        assert!(out.clone().group_index(-1).is_err());

        assert!(MidiCcRouteBuilder::new(&device, 128).is_err());
        assert!(MidiCcRouteBuilder::new(&device, -1).is_err());
        let callback = MidiCallbackBuilder::new(&device).unwrap();
        assert!(callback.clone().on_note().note(128).is_err());
        assert!(callback.clone().on_cc().controller(128).is_err());
        assert!(
            callback.clone().note(60).is_err(),
            "a note filter needs an on_note trigger"
        );
        assert!(
            callback.clone().channel(1).is_err(),
            "a channel filter needs an on_note or on_cc trigger"
        );

        let keyed = out.key("bounds".into());
        assert!(keyed.note_on(128, 100).is_err(), "7-bit note bound");
        assert!(keyed.note_on(60, 128).is_err(), "7-bit velocity bound");
        assert!(keyed.note_on(-1, 100).is_err(), "negative note");
        assert!(
            keyed.note_on16(60, 65_536).is_err(),
            "16-bit velocity bound"
        );
        assert!(keyed.note_on16(60, 65_535).is_ok());
        assert!(keyed.cc(128, 0).is_err(), "7-bit controller bound");
        assert!(keyed.cc(1, 128).is_err(), "7-bit value bound");
        assert!(keyed.cc32(1, i64::from(u32::MAX) + 1).is_err());
        assert!(keyed.cc32(1, i64::from(u32::MAX)).is_ok());
        assert!(keyed.program_change(128).is_err());
        assert!(keyed.pitch_bend14(8_192).is_err(), "signed 14-bit high");
        assert!(keyed.pitch_bend14(-8_193).is_err(), "signed 14-bit low");
        assert!(keyed.pitch_bend14(-8_192).is_ok());
        assert!(keyed.pitch_bend14(8_191).is_ok());
        assert!(keyed.pitch_bend32(i64::from(u32::MAX) + 1).is_err());
        foundation::abort_evaluation();
    }

    #[test]
    fn v2_midi_output_commands_are_keyed_best_effort_submissions() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let device = declared_device("mpk", "MPK Mini");
        let out = MidiOutBuilder::new(&device).unwrap();

        assert!(
            matches!(
                out.note_on(60, 100),
                Err(MidiCommandError::External(
                    ExternalEffectError::MissingIdempotencyKey {
                        domain: ExternalEffectDomain::Midi,
                        ..
                    }
                ))
            ),
            "the runtime never issues an implicit key"
        );

        let keyed = out
            .clone()
            .channel(3)
            .unwrap()
            .group(2)
            .unwrap()
            .key("note-1".into());
        let command = keyed.note_on(60, 100).unwrap();
        assert_eq!(command.idempotency_key(), "note-1");
        assert_eq!(command.qualified_operation(), "external.midi.note_on");
        assert!(matches!(
            command.submission().operation().domain(),
            ExternalEffectDomain::Midi
        ));

        let ledger_submission = command
            .submission()
            .submission(RuntimeEpoch::new())
            .unwrap();
        assert!(matches!(ledger_submission.atomicity, Atomicity::BestEffort));
        assert!(ledger_submission.require_idempotency_key);
        assert_eq!(
            ledger_submission.idempotency_key.as_deref(),
            Some("note-1"),
            "the ledger submission carries the caller's key verbatim"
        );
        assert!(matches!(
            ledger_submission.kind,
            MutationKind::Command {
                domain: MessageDomain::Midi,
                ref operation,
            } if operation == "external.midi.note_on"
        ));

        assert_eq!(
            keyed.note_off(60).unwrap().qualified_operation(),
            "external.midi.note_off"
        );
        assert_eq!(
            keyed.cc_hires(74, 1_000).unwrap().qualified_operation(),
            "external.midi.cc32",
            "hires forwarding aliases author the v2 operation"
        );
        foundation::abort_evaluation();
    }

    #[test]
    fn v2_midi_commands_never_embed_in_candidate_submissions() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let device = declared_device("mpk", "MPK Mini");
        let command = MidiOutBuilder::new(&device)
            .unwrap()
            .key("cc-1".into())
            .cc(74, 100)
            .unwrap();

        let candidate = |atomicity: Atomicity| Submission {
            kind: MutationKind::Candidate {
                origin: CandidateOrigin::RhaiHost,
            },
            source: MutationSource::Rhai {
                engine_id: "midi-v2-test".into(),
            },
            caller_namespace: "vibelang.v2.test".into(),
            idempotency_key: None,
            require_idempotency_key: false,
            retry_epoch: None,
            expected_revision: None,
            atomicity,
            supersession: SupersessionPolicy::Fifo,
            material: RequestMaterial::new(&("candidate", true), Some(&())).unwrap(),
        };

        assert!(matches!(
            CandidateSubmission::with_embedded_external(
                candidate(Atomicity::Required),
                vec![command.submission().operation().clone()],
            ),
            Err(ExternalEffectError::MixedRequiredAtomicSubmission {
                domain: ExternalEffectDomain::Midi,
                ..
            })
        ));
        assert!(matches!(
            CandidateSubmission::with_embedded_external(
                candidate(Atomicity::BestEffort),
                vec![command.submission().operation().clone()],
            ),
            Err(ExternalEffectError::EmbeddedExternalEffect {
                domain: ExternalEffectDomain::Midi,
                ..
            })
        ));
        foundation::abort_evaluation();
    }

    #[test]
    fn v2_midi_terminals_return_typed_refs_and_declare_edges() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        declare_empty::<VoiceKind>("lead");
        let lead = voice_ref("lead");
        let device = declared_device("mpk", "MPK Mini");
        assert_eq!(device.base().kind(), EntityKind::MidiDevice);

        let keyboard = MidiKeyboardRouteBuilder::new(&device)
            .unwrap()
            .channel(2)
            .unwrap()
            .to(&lead)
            .unwrap();
        assert_eq!(keyboard.base().kind(), EntityKind::MidiRoute);
        assert!(
            matches!(
                MidiKeyboardRouteBuilder::new(&device)
                    .unwrap()
                    .channel(2)
                    .unwrap()
                    .route_to(&lead),
                Err(FoundationError::Candidate(
                    CandidateError::DuplicateDeclaration { .. }
                ))
            ),
            "keyboard routes are edge-wise: the same edge redeclares"
        );

        let cc = MidiCcRouteBuilder::new(&device, 74)
            .unwrap()
            .to(&lead, "cutoff".into(), 0.0, 1.0)
            .unwrap();
        assert_eq!(cc.base().kind(), EntityKind::MidiRoute);
        assert!(
            matches!(
                MidiCcRouteBuilder::new(&device, 75)
                    .unwrap()
                    .to(&lead, "cutoff".into(), 0.0, 1.0),
                Err(FoundationError::Candidate(
                    CandidateError::DuplicateDeclaration { .. }
                ))
            ),
            "CC slots are target-side single-writer"
        );

        let callback = MidiCallbackBuilder::new(&device)
            .unwrap()
            .on_note()
            .channel(1)
            .unwrap()
            .handler("on_pad".into())
            .apply()
            .unwrap();
        assert_eq!(callback.base().kind(), EntityKind::Callback);

        cc.disconnect().unwrap();
        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(
            candidate
                .declarations()
                .iter()
                .filter(|declaration| {
                    matches!(
                        declaration.address().kind(),
                        EntityKind::MidiDevice | EntityKind::MidiRoute | EntityKind::Callback
                    )
                })
                .count(),
            4,
            "device, two routes, and the callback all declare"
        );
        assert_eq!(
            candidate.operations().len(),
            1,
            "MidiRouteRef::disconnect commits a real lifecycle operation"
        );
    }

    #[test]
    fn v2_midi_rhai_surface_authors_from_script() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        declare_empty::<VoiceKind>("lead");

        let mut engine = Engine::new();
        crate::foundation::register(&mut engine);
        install_v2_for_tests(&mut engine);
        let command = engine
            .eval::<MidiOutputCommand>(
                r#"
                let mpk = midi_device("mpk").port("MPK Mini").input().apply();
                keyboard_route(mpk).channel(2).to(voice_ref("lead"));
                midi_callback(mpk).on_cc().controller(64).channel(1)
                    .handler("on_sustain").apply();
                midi_out(mpk).channel(3).key("script-note").note_on(60, 100)
                "#,
            )
            .unwrap();
        assert_eq!(command.idempotency_key(), "script-note");
        assert_eq!(command.qualified_operation(), "external.midi.note_on");

        assert!(
            engine
                .eval::<MidiOutputCommand>(
                    r#"midi_out(midi_device_ref("mpk")).channel(17).key("x").note_on(60, 100)"#,
                )
                .is_err(),
            "the public channel surface is 1-16"
        );
        assert!(
            engine
                .eval::<MidiOutputCommand>(r#"midi_out(midi_device_ref("mpk")).note_on(60, 100)"#,)
                .is_err(),
            "a keyless output command rejects"
        );

        let candidate = foundation::finish_evaluation().unwrap();
        assert!(!candidate.declarations().is_empty());
    }
}
