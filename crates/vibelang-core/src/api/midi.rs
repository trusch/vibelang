//! MIDI API for VibeLang Rhai scripts.
//!
//! This module provides the Rhai bindings for MIDI input support:
//! - Device discovery and opening
//! - Keyboard-to-voice routing
//! - Drum pad mapping
//! - CC/fader mapping
//! - Callbacks for custom logic

use crate::api::require_handle;
use crate::api::voice::Voice;
use crate::midi::{
    CcRoute, CcTarget, KeyboardRoute, MidiDeviceInfo, MidiInputManager,
    NoteRoute, ParameterCurve, VelocityCurve,
};
use crate::state::StateMessage;
use rhai::{Array, Dynamic, Engine, EvalAltResult, FnPtr, Map};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

// === Callback Storage ===

/// Global storage for MIDI callback FnPtrs.
/// Callbacks are stored by ID and executed when MIDI events trigger them.
static CALLBACK_STORAGE: std::sync::LazyLock<RwLock<HashMap<u64, FnPtr>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Counter for generating unique callback IDs.
static CALLBACK_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

// === Active MIDI Device Storage ===

/// Global storage for active MIDI devices, keyed by device name.
/// This keeps MidiDevice handles alive across script reloads,
/// allowing smart reloading without disconnecting MIDI devices.
static ACTIVE_MIDI_DEVICES: std::sync::LazyLock<RwLock<HashMap<String, MidiDevice>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Store a MIDI device to keep it alive, keyed by name for reuse.
fn store_active_device(device: MidiDevice) {
    ACTIVE_MIDI_DEVICES
        .write()
        .unwrap()
        .insert(device.info.name.clone(), device);
}

/// Get an existing device by name if it's already connected.
fn get_existing_device(name: &str) -> Option<MidiDevice> {
    ACTIVE_MIDI_DEVICES.read().unwrap().get(name).cloned()
}

/// Clear all active MIDI devices (only called on full shutdown, not reload).
pub fn clear_midi_devices() {
    ACTIVE_MIDI_DEVICES.write().unwrap().clear();
}

/// Register a callback and return its ID.
fn register_callback_fnptr(fn_ptr: FnPtr) -> u64 {
    let id = CALLBACK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    CALLBACK_STORAGE.write().unwrap().insert(id, fn_ptr);
    id
}

/// Get a callback FnPtr by ID.
pub fn get_callback_fnptr(id: u64) -> Option<FnPtr> {
    CALLBACK_STORAGE.read().unwrap().get(&id).cloned()
}

/// Clear all stored callbacks (called on script reload).
pub fn clear_callbacks() {
    CALLBACK_STORAGE.write().unwrap().clear();
}

/// Execute all pending MIDI callbacks.
///
/// This should be called periodically by the main execution loop.
/// Returns the number of callbacks executed.
pub fn execute_pending_callbacks(
    engine: &rhai::Engine,
    ast: &rhai::AST,
    _scope: &mut rhai::Scope,
) -> usize {
    let handle = match crate::api::get_handle() {
        Some(h) => h,
        None => return 0,
    };

    // Drain pending callbacks from the state-based MIDI routing
    let pending = handle.with_state_mut(|state| {
        state.midi_config.routing.drain_pending_callbacks()
    });

    let mut executed = 0;

    for callback in pending {
        if let Some(fn_ptr) = get_callback_fnptr(callback.callback_id) {
            // Call the callback with the velocity/value as argument
            let result: Result<(), _> = fn_ptr.call(engine, ast, (callback.value,));

            match result {
                Ok(_) => {
                    executed += 1;
                    log::debug!(
                        "Executed MIDI callback {} with value {}",
                        callback.callback_id,
                        callback.value
                    );
                }
                Err(e) => {
                    log::warn!("MIDI callback {} failed: {}", callback.callback_id, e);
                }
            }
        } else {
            log::warn!(
                "MIDI callback {} not found in storage",
                callback.callback_id
            );
        }
    }

    executed
}

/// A MIDI device handle for Rhai scripts.
#[derive(Clone)]
pub struct MidiDevice {
    /// Device info
    pub info: MidiDeviceInfo,
    /// The MIDI input manager (wrapped in Arc<Mutex> for thread safety).
    /// Kept alive to maintain the MIDI connection.
    #[allow(dead_code)]
    manager: Arc<Mutex<MidiInputManager>>,
}

impl MidiDevice {
    /// Get the device name.
    pub fn name(&mut self) -> String {
        self.info.name.clone()
    }

    /// Get the port index.
    pub fn port_index(&mut self) -> i64 {
        self.info.port_index as i64
    }

    /// Check if the device is connected.
    pub fn is_open(&mut self) -> bool {
        true // If we have a MidiDevice, it's open
    }
}

/// Builder for keyboard routing.
#[derive(Clone)]
pub struct KeyboardRouteBuilder {
    /// MIDI channel filter (None = all channels)
    channel: Option<u8>,
    /// Note range filter (low, high)
    note_range: Option<(u8, u8)>,
    /// Transpose in semitones
    transpose: i8,
    /// Velocity curve
    velocity_curve: VelocityCurve,
}

impl KeyboardRouteBuilder {
    fn new() -> Self {
        Self {
            channel: None,
            note_range: None,
            transpose: 0,
            velocity_curve: VelocityCurve::Linear,
        }
    }

    /// Filter by MIDI channel (1-16).
    pub fn channel(&mut self, ch: i64) -> Self {
        let mut new = self.clone();
        new.channel = Some((ch.clamp(1, 16) - 1) as u8); // Convert 1-16 to 0-15
        new
    }

    /// Filter by note range.
    pub fn range(&mut self, low: Dynamic, high: Dynamic) -> Result<Self, Box<EvalAltResult>> {
        let low_note = parse_note(&low)?;
        let high_note = parse_note(&high)?;
        let mut new = self.clone();
        new.note_range = Some((low_note, high_note));
        Ok(new)
    }

    /// Transpose by semitones.
    pub fn transpose(&mut self, semitones: i64) -> Self {
        let mut new = self.clone();
        new.transpose = semitones.clamp(-127, 127) as i8;
        new
    }

    /// Shift by octaves.
    pub fn octave(&mut self, octaves: i64) -> Self {
        self.transpose(octaves * 12)
    }

    /// Set velocity curve.
    pub fn velocity_curve(&mut self, curve: &str) -> Self {
        let mut new = self.clone();
        new.velocity_curve = match curve.to_lowercase().as_str() {
            "linear" => VelocityCurve::Linear,
            "exponential" | "exp" => VelocityCurve::Exponential,
            "compressed" | "comp" => VelocityCurve::Compressed,
            _ => VelocityCurve::Linear,
        };
        new
    }

    /// Set fixed velocity.
    pub fn velocity_fixed(&mut self, value: f64) -> Self {
        let mut new = self.clone();
        new.velocity_curve = VelocityCurve::Fixed(value.clamp(0.0, 1.0) as f32);
        new
    }

    /// Route to a voice.
    pub fn to(&mut self, voice: Dynamic) -> Result<(), Box<EvalAltResult>> {
        let voice_name = get_voice_name(&voice)?;
        let handle = require_handle();

        let route = KeyboardRoute {
            voice_name,
            channel: self.channel,
            note_range: self.note_range,
            transpose: self.transpose,
            velocity_curve: self.velocity_curve.clone(),
        };

        handle
            .send(StateMessage::MidiAddKeyboardRoute { route })
            .map_err(|e| Box::new(EvalAltResult::from(e.to_string())) as Box<EvalAltResult>)?;

        Ok(())
    }
}

/// Builder for note-specific routing (drum pads).
#[derive(Clone)]
pub struct NoteRouteBuilder {
    /// MIDI note number
    note: u8,
    /// MIDI channel filter
    channel: Option<u8>,
    /// Velocity curve
    velocity_curve: VelocityCurve,
    /// Choke group
    choke_group: Option<String>,
    /// Velocity-to-parameter mappings
    velocity_params: Vec<(String, f32, f32)>,
}

impl NoteRouteBuilder {
    fn new(note: u8) -> Self {
        Self {
            note,
            channel: None,
            velocity_curve: VelocityCurve::Linear,
            choke_group: None,
            velocity_params: Vec::new(),
        }
    }

    /// Filter by MIDI channel (1-16).
    pub fn channel(&mut self, ch: i64) -> Self {
        let mut new = self.clone();
        new.channel = Some((ch.clamp(1, 16) - 1) as u8);
        new
    }

    /// Set velocity curve.
    pub fn velocity_curve(&mut self, curve: &str) -> Self {
        let mut new = self.clone();
        new.velocity_curve = match curve.to_lowercase().as_str() {
            "linear" => VelocityCurve::Linear,
            "exponential" | "exp" => VelocityCurve::Exponential,
            "compressed" | "comp" => VelocityCurve::Compressed,
            _ => VelocityCurve::Linear,
        };
        new
    }

    /// Add to a choke group.
    pub fn choke_group(&mut self, group: &str) -> Self {
        let mut new = self.clone();
        new.choke_group = Some(group.to_string());
        new
    }

    /// Map velocity to a parameter.
    pub fn velocity_to(&mut self, param: &str, min: f64, max: f64) -> Self {
        let mut new = self.clone();
        new.velocity_params
            .push((param.to_string(), min as f32, max as f32));
        new
    }

    /// Route to a voice.
    pub fn to(&mut self, voice: Dynamic) -> Result<(), Box<EvalAltResult>> {
        let voice_name = get_voice_name(&voice)?;
        let handle = require_handle();

        let route = NoteRoute {
            voice_name,
            channel: self.channel,
            choke_group: self.choke_group.clone(),
            velocity_curve: self.velocity_curve.clone(),
            velocity_params: self.velocity_params.clone(),
        };

        handle
            .send(StateMessage::MidiAddNoteRoute {
                channel: self.channel,
                note: self.note,
                route,
            })
            .map_err(|e| Box::new(EvalAltResult::from(e.to_string())) as Box<EvalAltResult>)?;

        Ok(())
    }
}

/// Builder for CC routing.
#[derive(Clone)]
pub struct CcRouteBuilder {
    /// CC number
    cc_number: u8,
    /// MIDI channel filter
    channel: Option<u8>,
    /// Parameter curve
    curve: ParameterCurve,
}

impl CcRouteBuilder {
    fn new(cc_number: u8) -> Self {
        Self {
            cc_number,
            channel: None,
            curve: ParameterCurve::Linear,
        }
    }

    /// Filter by MIDI channel (1-16).
    pub fn channel(&mut self, ch: i64) -> Self {
        let mut new = self.clone();
        new.channel = Some((ch.clamp(1, 16) - 1) as u8);
        new
    }

    /// Set parameter curve.
    pub fn curve(&mut self, curve: &str) -> Self {
        let mut new = self.clone();
        new.curve = match curve.to_lowercase().as_str() {
            "linear" => ParameterCurve::Linear,
            "logarithmic" | "log" => ParameterCurve::Logarithmic,
            "exponential" | "exp" => ParameterCurve::Exponential,
            _ => ParameterCurve::Linear,
        };
        new
    }

    /// Route to a voice parameter.
    pub fn to_voice(
        &mut self,
        voice: Dynamic,
        param: &str,
        min: f64,
        max: f64,
    ) -> Result<(), Box<EvalAltResult>> {
        let voice_name = get_voice_name(&voice)?;
        let handle = require_handle();

        let route = CcRoute {
            target: CcTarget::Voice(voice_name),
            param_name: param.to_string(),
            min_value: min as f32,
            max_value: max as f32,
            curve: self.curve.clone(),
            channel: self.channel,
        };

        handle
            .send(StateMessage::MidiAddCcRoute {
                channel: self.channel,
                cc_number: self.cc_number,
                route,
            })
            .map_err(|e| Box::new(EvalAltResult::from(e.to_string())) as Box<EvalAltResult>)?;

        Ok(())
    }

    /// Route to an effect parameter.
    pub fn to_effect(
        &mut self,
        effect_id: &str,
        param: &str,
        min: f64,
        max: f64,
    ) -> Result<(), Box<EvalAltResult>> {
        let handle = require_handle();

        let route = CcRoute {
            target: CcTarget::Effect(effect_id.to_string()),
            param_name: param.to_string(),
            min_value: min as f32,
            max_value: max as f32,
            curve: self.curve.clone(),
            channel: self.channel,
        };

        handle
            .send(StateMessage::MidiAddCcRoute {
                channel: self.channel,
                cc_number: self.cc_number,
                route,
            })
            .map_err(|e| Box::new(EvalAltResult::from(e.to_string())) as Box<EvalAltResult>)?;

        Ok(())
    }

    /// Route to a group parameter.
    pub fn to_group(
        &mut self,
        group_path: &str,
        param: &str,
        min: f64,
        max: f64,
    ) -> Result<(), Box<EvalAltResult>> {
        let handle = require_handle();

        let route = CcRoute {
            target: CcTarget::Group(group_path.to_string()),
            param_name: param.to_string(),
            min_value: min as f32,
            max_value: max as f32,
            curve: self.curve.clone(),
            channel: self.channel,
        };

        handle
            .send(StateMessage::MidiAddCcRoute {
                channel: self.channel,
                cc_number: self.cc_number,
                route,
            })
            .map_err(|e| Box::new(EvalAltResult::from(e.to_string())) as Box<EvalAltResult>)?;

        Ok(())
    }

    /// Route to a global parameter (e.g., tempo).
    pub fn to_global(
        &mut self,
        param: &str,
        min: f64,
        max: f64,
    ) -> Result<(), Box<EvalAltResult>> {
        let handle = require_handle();

        let route = CcRoute {
            target: CcTarget::Global(param.to_string()),
            param_name: param.to_string(),
            min_value: min as f32,
            max_value: max as f32,
            curve: self.curve.clone(),
            channel: self.channel,
        };

        handle
            .send(StateMessage::MidiAddCcRoute {
                channel: self.channel,
                cc_number: self.cc_number,
                route,
            })
            .map_err(|e| Box::new(EvalAltResult::from(e.to_string())) as Box<EvalAltResult>)?;

        Ok(())
    }
}

/// Builder for pitch bend routing.
#[derive(Clone)]
pub struct PitchBendRouteBuilder {
    /// MIDI channel filter
    channel: Option<u8>,
    /// Parameter curve
    curve: ParameterCurve,
}

impl PitchBendRouteBuilder {
    fn new() -> Self {
        Self {
            channel: None,
            curve: ParameterCurve::Linear,
        }
    }

    /// Filter by MIDI channel (1-16).
    pub fn channel(&mut self, ch: i64) -> Self {
        let mut new = self.clone();
        new.channel = Some((ch.clamp(1, 16) - 1) as u8);
        new
    }

    /// Route to a voice parameter.
    pub fn to_voice(
        &mut self,
        voice: Dynamic,
        param: &str,
        min: f64,
        max: f64,
    ) -> Result<(), Box<EvalAltResult>> {
        let voice_name = get_voice_name(&voice)?;
        let handle = require_handle();

        let route = CcRoute {
            target: CcTarget::Voice(voice_name),
            param_name: param.to_string(),
            min_value: min as f32,
            max_value: max as f32,
            curve: self.curve.clone(),
            channel: self.channel,
        };

        handle
            .send(StateMessage::MidiAddPitchBendRoute {
                channel: self.channel,
                route,
            })
            .map_err(|e| Box::new(EvalAltResult::from(e.to_string())) as Box<EvalAltResult>)?;

        Ok(())
    }
}

/// Builder for note callbacks.
///
/// Usage:
/// ```rhai
/// midi.on_note("C3").callback(|vel| {
///     print("Note pressed with velocity: " + vel);
///     sequence_start("my_sequence");
/// });
///
/// // Trigger on note-off instead:
/// midi.on_note("C3").on_off().callback(|vel| {
///     sequence_stop("my_sequence");
/// });
/// ```
#[derive(Clone)]
pub struct NoteCallbackBuilder {
    /// MIDI note number
    note: u8,
    /// MIDI channel filter
    channel: Option<u8>,
    /// Trigger on note-on
    on_note_on: bool,
    /// Trigger on note-off
    on_note_off: bool,
}

impl NoteCallbackBuilder {
    fn new(note: u8) -> Self {
        Self {
            note,
            channel: None,
            on_note_on: true,
            on_note_off: false,
        }
    }

    /// Filter by MIDI channel (1-16).
    pub fn channel(&mut self, ch: i64) -> Self {
        let mut new = self.clone();
        new.channel = Some((ch.clamp(1, 16) - 1) as u8);
        new
    }

    /// Only trigger on note-off instead of note-on.
    pub fn on_off(&mut self) -> Self {
        let mut new = self.clone();
        new.on_note_on = false;
        new.on_note_off = true;
        new
    }

    /// Trigger on both note-on and note-off.
    pub fn on_both(&mut self) -> Self {
        let mut new = self.clone();
        new.on_note_on = true;
        new.on_note_off = true;
        new
    }

    /// Register a callback closure that will be called when this note is triggered.
    /// The closure receives the velocity (0-127) as its argument.
    pub fn callback(&mut self, fn_ptr: FnPtr) -> Result<(), Box<EvalAltResult>> {
        let handle = require_handle();
        let callback_id = register_callback_fnptr(fn_ptr);

        handle
            .send(StateMessage::MidiRegisterNoteCallback {
                callback_id,
                channel: self.channel,
                note: self.note,
                on_note_on: self.on_note_on,
                on_note_off: self.on_note_off,
            })
            .map_err(|e| Box::new(EvalAltResult::from(e.to_string())) as Box<EvalAltResult>)?;

        Ok(())
    }
}

/// Builder for CC callbacks.
///
/// Usage:
/// ```rhai
/// // Trigger when CC crosses threshold (going up)
/// midi.on_cc(20).threshold(64).callback(|val| {
///     print("CC 20 crossed 64, now at: " + val);
///     transport_toggle();
/// });
///
/// // Trigger on every CC change (no threshold)
/// midi.on_cc(1).callback(|val| {
///     set_tempo(60.0 + val);  // Map CC to tempo 60-187
/// });
/// ```
#[derive(Clone)]
pub struct CcCallbackBuilder {
    /// CC number
    cc_number: u8,
    /// MIDI channel filter
    channel: Option<u8>,
    /// Threshold value (None = trigger on every change)
    threshold: Option<u8>,
    /// Trigger when going above threshold (vs below)
    above_threshold: bool,
}

impl CcCallbackBuilder {
    fn new(cc_number: u8) -> Self {
        Self {
            cc_number,
            channel: None,
            threshold: None, // No threshold by default - triggers on every change
            above_threshold: true,
        }
    }

    /// Filter by MIDI channel (1-16).
    pub fn channel(&mut self, ch: i64) -> Self {
        let mut new = self.clone();
        new.channel = Some((ch.clamp(1, 16) - 1) as u8);
        new
    }

    /// Set threshold for triggering (0-127).
    /// Callback only fires when crossing this threshold.
    pub fn threshold(&mut self, value: i64) -> Self {
        let mut new = self.clone();
        new.threshold = Some(value.clamp(0, 127) as u8);
        new
    }

    /// Trigger when CC goes below threshold instead of above.
    pub fn below(&mut self) -> Self {
        let mut new = self.clone();
        new.above_threshold = false;
        new
    }

    /// Register a callback closure that will be called when this CC triggers.
    /// The closure receives the CC value (0-127) as its argument.
    pub fn callback(&mut self, fn_ptr: FnPtr) -> Result<(), Box<EvalAltResult>> {
        let handle = require_handle();
        let callback_id = register_callback_fnptr(fn_ptr);

        handle
            .send(StateMessage::MidiRegisterCcCallback {
                callback_id,
                channel: self.channel,
                cc_number: self.cc_number,
                threshold: self.threshold,
                above_threshold: self.above_threshold,
            })
            .map_err(|e| Box::new(EvalAltResult::from(e.to_string())) as Box<EvalAltResult>)?;

        Ok(())
    }
}

/// Create a note callback builder from a device.
fn midi_device_on_note(
    _device: &mut MidiDevice,
    note: Dynamic,
) -> Result<NoteCallbackBuilder, Box<EvalAltResult>> {
    let note_num = parse_note(&note)?;
    Ok(NoteCallbackBuilder::new(note_num))
}

/// Create a CC callback builder from a device.
fn midi_device_on_cc(_device: &mut MidiDevice, cc: i64) -> CcCallbackBuilder {
    CcCallbackBuilder::new(cc.clamp(0, 127) as u8)
}

// === Global functions ===

/// List available MIDI input devices (both ALSA and JACK).
fn midi_devices() -> Array {
    let devices = crate::midi::list_all_midi_devices();

    devices
        .into_iter()
        .enumerate()
        .map(|(index, d)| {
            let mut map = Map::new();
            map.insert("name".into(), Dynamic::from(d.name));
            // Use the position in the combined list as the user-facing index
            map.insert("index".into(), Dynamic::from(index as i64));
            Dynamic::from(map)
        })
        .collect()
}

/// Open a MIDI device by name (partial match, case-insensitive).
///
/// Searches through all available MIDI devices (ALSA and JACK) and opens
/// the first one whose name contains the search string.
/// If the device is already connected (from a previous reload), reuses the existing connection.
fn midi_open_by_name(name: &str) -> Result<MidiDevice, Box<EvalAltResult>> {
    let handle = require_handle();
    let name_lower = name.to_lowercase();

    // Get all available devices to find the full device name
    let devices = crate::midi::list_all_midi_devices();

    // Find a matching device
    let device_info = devices
        .iter()
        .find(|d| d.name.to_lowercase().contains(&name_lower))
        .ok_or_else(|| {
            Box::new(EvalAltResult::from(format!(
                "No MIDI device found matching '{}'. Available devices: {}",
                name,
                devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", ")
            ))) as Box<EvalAltResult>
        })?;

    // Check if device is already connected (smart reload)
    if let Some(existing_device) = get_existing_device(&device_info.name) {
        log::info!(
            "[MIDI] Reusing existing connection to '{}'",
            device_info.name
        );
        return Ok(existing_device);
    }

    // Device not connected, open it
    let midi_tx = handle.midi_sender();
    let (mut manager, rx) = MidiInputManager::new();

    // Open using the unified method
    let info = manager
        .open_device(device_info)
        .map_err(|e| Box::new(EvalAltResult::from(e)) as Box<EvalAltResult>)?;

    // Spawn a thread to forward messages to the runtime
    std::thread::spawn(move || {
        log::info!("[MIDI] Forwarding thread started");
        while let Ok(msg) = rx.recv() {
            log::debug!("[MIDI FORWARD] {:?}", msg);
            let _ = midi_tx.send(msg);
        }
        log::warn!("[MIDI] Forwarding thread ended");
    });

    let device = MidiDevice {
        info,
        manager: Arc::new(Mutex::new(manager)),
    };

    // Store a clone to keep the connection alive after script variables go out of scope
    store_active_device(device.clone());

    Ok(device)
}

/// Open a MIDI device by index in the combined device list.
///
/// The index corresponds to the position in the list returned by `midi_devices()`.
/// If the device is already connected (from a previous reload), reuses the existing connection.
fn midi_open_by_index(index: i64) -> Result<MidiDevice, Box<EvalAltResult>> {
    let handle = require_handle();

    // Get all available devices
    let devices = crate::midi::list_all_midi_devices();

    let idx = index as usize;
    if idx >= devices.len() {
        return Err(Box::new(EvalAltResult::from(format!(
            "MIDI device index {} out of range (0-{})",
            index,
            devices.len().saturating_sub(1)
        ))));
    }

    let device_info = &devices[idx];

    // Check if device is already connected (smart reload)
    if let Some(existing_device) = get_existing_device(&device_info.name) {
        log::info!(
            "[MIDI] Reusing existing connection to '{}'",
            device_info.name
        );
        return Ok(existing_device);
    }

    // Device not connected, open it
    let midi_tx = handle.midi_sender();
    let (mut manager, rx) = MidiInputManager::new();

    // Open using the unified method
    let info = manager
        .open_device(device_info)
        .map_err(|e| Box::new(EvalAltResult::from(e)) as Box<EvalAltResult>)?;

    // Spawn a thread to forward messages
    std::thread::spawn(move || {
        log::info!("[MIDI] Forwarding thread started");
        while let Ok(msg) = rx.recv() {
            log::debug!("[MIDI FORWARD] {:?}", msg);
            let _ = midi_tx.send(msg);
        }
        log::warn!("[MIDI] Forwarding thread ended");
    });

    let device = MidiDevice {
        info,
        manager: Arc::new(Mutex::new(manager)),
    };

    // Store a clone to keep the connection alive after script variables go out of scope
    store_active_device(device.clone());

    Ok(device)
}

/// Open the first available MIDI device.
fn midi_open_first() -> Result<MidiDevice, Box<EvalAltResult>> {
    midi_open_by_index(0)
}

/// Enable or disable MIDI monitoring.
fn midi_monitor(enabled: bool) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::MidiSetMonitoring { enabled });
}

/// Clear all MIDI routing.
fn midi_clear() {
    let handle = require_handle();
    let _ = handle.send(StateMessage::MidiClearRouting);
}

// === MidiDevice methods ===

/// Create a keyboard route builder from the device.
fn midi_device_keyboard(_device: &mut MidiDevice) -> KeyboardRouteBuilder {
    KeyboardRouteBuilder::new()
}

/// Create a keyboard route builder with channel filter.
fn midi_device_channel(_device: &mut MidiDevice, ch: i64) -> KeyboardRouteBuilder {
    let mut builder = KeyboardRouteBuilder::new();
    builder.channel = Some((ch.clamp(1, 16) - 1) as u8);
    builder
}

/// Create a keyboard route builder with range filter.
fn midi_device_range(
    _device: &mut MidiDevice,
    low: Dynamic,
    high: Dynamic,
) -> Result<KeyboardRouteBuilder, Box<EvalAltResult>> {
    let low_note = parse_note(&low)?;
    let high_note = parse_note(&high)?;
    let mut builder = KeyboardRouteBuilder::new();
    builder.note_range = Some((low_note, high_note));
    Ok(builder)
}

/// Route entire device to a voice.
fn midi_device_to(_device: &mut MidiDevice, voice: Dynamic) -> Result<(), Box<EvalAltResult>> {
    let mut builder = KeyboardRouteBuilder::new();
    builder.to(voice)
}

/// Create a note route builder.
fn midi_device_note(_device: &mut MidiDevice, note: Dynamic) -> Result<NoteRouteBuilder, Box<EvalAltResult>> {
    let note_num = parse_note(&note)?;
    Ok(NoteRouteBuilder::new(note_num))
}

/// Create a CC route builder.
fn midi_device_cc(_device: &mut MidiDevice, cc: i64) -> CcRouteBuilder {
    CcRouteBuilder::new(cc.clamp(0, 127) as u8)
}

/// Create a pitch bend route builder.
fn midi_device_pitch_bend(_device: &mut MidiDevice) -> PitchBendRouteBuilder {
    PitchBendRouteBuilder::new()
}

// === Helper functions ===

/// Parse a note from string or number.
fn parse_note(value: &Dynamic) -> Result<u8, Box<EvalAltResult>> {
    if let Some(n) = value.as_int().ok() {
        return Ok(n.clamp(0, 127) as u8);
    }
    if let Some(n) = value.as_float().ok() {
        return Ok((n as i64).clamp(0, 127) as u8);
    }
    if let Some(s) = value.clone().into_string().ok() {
        return crate::api::helpers::parse_note_name(&s)
            .ok_or_else(|| Box::new(EvalAltResult::from(format!("Invalid note: {}", s))) as Box<EvalAltResult>);
    }
    Err(Box::new(EvalAltResult::from("Invalid note value")))
}

/// Get voice name from a Voice object or string.
fn get_voice_name(voice: &Dynamic) -> Result<String, Box<EvalAltResult>> {
    // Try to get the name from a Voice custom type
    if let Some(v) = voice.read_lock::<Voice>() {
        return Ok(v.name.clone());
    }

    // Try to get the name from a Map (legacy support)
    if let Some(map) = voice.read_lock::<Map>() {
        if let Some(name) = map.get("name") {
            if let Some(s) = name.clone().into_string().ok() {
                return Ok(s);
            }
        }
    }

    // Try as a string (voice name directly)
    if let Some(s) = voice.clone().into_string().ok() {
        return Ok(s);
    }

    Err(Box::new(EvalAltResult::from(
        "Cannot get voice name: expected Voice object or string",
    )))
}

/// Register MIDI API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register types
    engine.register_type_with_name::<MidiDevice>("MidiDevice");
    engine.register_type_with_name::<KeyboardRouteBuilder>("KeyboardRouteBuilder");
    engine.register_type_with_name::<NoteRouteBuilder>("NoteRouteBuilder");
    engine.register_type_with_name::<CcRouteBuilder>("CcRouteBuilder");
    engine.register_type_with_name::<PitchBendRouteBuilder>("PitchBendRouteBuilder");
    engine.register_type_with_name::<NoteCallbackBuilder>("NoteCallbackBuilder");
    engine.register_type_with_name::<CcCallbackBuilder>("CcCallbackBuilder");

    // Global functions
    engine.register_fn("midi_devices", midi_devices);
    engine.register_fn("midi_open", midi_open_by_name);
    engine.register_fn("midi_open", midi_open_by_index);
    engine.register_fn("midi_open", midi_open_first);
    engine.register_fn("midi_monitor", midi_monitor);
    engine.register_fn("midi_clear", midi_clear);

    // MidiDevice methods
    engine.register_fn("name", MidiDevice::name);
    engine.register_fn("port_index", MidiDevice::port_index);
    engine.register_fn("is_open", MidiDevice::is_open);
    engine.register_fn("keyboard", midi_device_keyboard);
    engine.register_fn("channel", midi_device_channel);
    engine.register_fn("range", midi_device_range);
    engine.register_fn("to", midi_device_to);
    engine.register_fn("note", midi_device_note);
    engine.register_fn("cc", midi_device_cc);
    engine.register_fn("pitch_bend", midi_device_pitch_bend);
    engine.register_fn("on_note", midi_device_on_note);
    engine.register_fn("on_cc", midi_device_on_cc);

    // KeyboardRouteBuilder methods
    engine.register_fn("channel", KeyboardRouteBuilder::channel);
    engine.register_fn("range", KeyboardRouteBuilder::range);
    engine.register_fn("transpose", KeyboardRouteBuilder::transpose);
    engine.register_fn("octave", KeyboardRouteBuilder::octave);
    engine.register_fn("velocity_curve", KeyboardRouteBuilder::velocity_curve);
    engine.register_fn("velocity_fixed", KeyboardRouteBuilder::velocity_fixed);
    engine.register_fn("to", KeyboardRouteBuilder::to);

    // NoteRouteBuilder methods
    engine.register_fn("channel", NoteRouteBuilder::channel);
    engine.register_fn("velocity_curve", NoteRouteBuilder::velocity_curve);
    engine.register_fn("choke_group", NoteRouteBuilder::choke_group);
    engine.register_fn("velocity_to", NoteRouteBuilder::velocity_to);
    engine.register_fn("to", NoteRouteBuilder::to);

    // CcRouteBuilder methods
    engine.register_fn("channel", CcRouteBuilder::channel);
    engine.register_fn("curve", CcRouteBuilder::curve);
    engine.register_fn("to", CcRouteBuilder::to_voice);
    engine.register_fn("to_effect", CcRouteBuilder::to_effect);
    engine.register_fn("to_group", CcRouteBuilder::to_group);
    engine.register_fn("to_global", CcRouteBuilder::to_global);

    // PitchBendRouteBuilder methods
    engine.register_fn("channel", PitchBendRouteBuilder::channel);
    engine.register_fn("to", PitchBendRouteBuilder::to_voice);

    // NoteCallbackBuilder methods
    engine.register_fn("channel", NoteCallbackBuilder::channel);
    engine.register_fn("on_off", NoteCallbackBuilder::on_off);
    engine.register_fn("on_both", NoteCallbackBuilder::on_both);
    engine.register_fn("callback", NoteCallbackBuilder::callback);

    // CcCallbackBuilder methods
    engine.register_fn("channel", CcCallbackBuilder::channel);
    engine.register_fn("threshold", CcCallbackBuilder::threshold);
    engine.register_fn("below", CcCallbackBuilder::below);
    engine.register_fn("callback", CcCallbackBuilder::callback);
}
