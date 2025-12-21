//! MIDI input support for VibeLang.
//!
//! This module provides:
//! - Device discovery and connection (ALSA and JACK MIDI)
//! - MIDI message parsing and routing
//! - Keyboard-to-voice mapping
//! - CC-to-parameter mapping
//! - Callback support for custom logic

use crossbeam_channel::{unbounded, Receiver, Sender};
use jack::{Client, ClientOptions, MidiIn, Port, ProcessScope};
use midir::{MidiInput, MidiInputConnection};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

/// MIDI backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiBackend {
    /// ALSA MIDI (via midir)
    Alsa,
    /// JACK MIDI
    Jack,
}

impl std::fmt::Display for MidiBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MidiBackend::Alsa => write!(f, "ALSA"),
            MidiBackend::Jack => write!(f, "JACK"),
        }
    }
}

/// MIDI message types parsed from raw MIDI bytes.
#[derive(Debug, Clone)]
pub enum MidiMessage {
    /// Note on event (channel 0-15, note 0-127, velocity 0-127)
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
        timestamp: u64,
    },
    /// Note off event (channel 0-15, note 0-127)
    NoteOff {
        channel: u8,
        note: u8,
        timestamp: u64,
    },
    /// Control change (channel, controller number, value)
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
        timestamp: u64,
    },
    /// Pitch bend (channel, 14-bit value centered at 8192)
    PitchBend {
        channel: u8,
        value: i16,
        timestamp: u64,
    },
    /// Channel aftertouch (channel pressure)
    ChannelAftertouch {
        channel: u8,
        pressure: u8,
        timestamp: u64,
    },
    /// Polyphonic aftertouch (per-note pressure)
    PolyAftertouch {
        channel: u8,
        note: u8,
        pressure: u8,
        timestamp: u64,
    },
    /// Program change
    ProgramChange {
        channel: u8,
        program: u8,
        timestamp: u64,
    },
    /// MIDI clock tick (24 per quarter note)
    Clock { timestamp: u64 },
    /// Start playback
    Start { timestamp: u64 },
    /// Stop playback
    Stop { timestamp: u64 },
    /// Continue playback
    Continue { timestamp: u64 },
}

impl MidiMessage {
    /// Parse raw MIDI bytes into a MidiMessage.
    pub fn from_bytes(bytes: &[u8], timestamp: u64) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }

        let status = bytes[0];

        // System real-time messages (single byte, can appear anywhere)
        match status {
            0xF8 => return Some(MidiMessage::Clock { timestamp }),
            0xFA => return Some(MidiMessage::Start { timestamp }),
            0xFB => return Some(MidiMessage::Continue { timestamp }),
            0xFC => return Some(MidiMessage::Stop { timestamp }),
            _ => {}
        }

        // Channel messages
        let msg_type = status & 0xF0;
        let channel = status & 0x0F;

        match msg_type {
            0x90 if bytes.len() >= 3 => {
                let note = bytes[1];
                let velocity = bytes[2];
                if velocity == 0 {
                    // Note on with velocity 0 is treated as note off
                    Some(MidiMessage::NoteOff {
                        channel,
                        note,
                        timestamp,
                    })
                } else {
                    Some(MidiMessage::NoteOn {
                        channel,
                        note,
                        velocity,
                        timestamp,
                    })
                }
            }
            0x80 if bytes.len() >= 3 => Some(MidiMessage::NoteOff {
                channel,
                note: bytes[1],
                timestamp,
            }),
            0xB0 if bytes.len() >= 3 => Some(MidiMessage::ControlChange {
                channel,
                controller: bytes[1],
                value: bytes[2],
                timestamp,
            }),
            0xE0 if bytes.len() >= 3 => {
                // Pitch bend is 14-bit: LSB + MSB
                let lsb = bytes[1] as i16;
                let msb = bytes[2] as i16;
                let value = (msb << 7) | lsb;
                // Center at 0 (-8192 to +8191)
                Some(MidiMessage::PitchBend {
                    channel,
                    value: value - 8192,
                    timestamp,
                })
            }
            0xD0 if bytes.len() >= 2 => Some(MidiMessage::ChannelAftertouch {
                channel,
                pressure: bytes[1],
                timestamp,
            }),
            0xA0 if bytes.len() >= 3 => Some(MidiMessage::PolyAftertouch {
                channel,
                note: bytes[1],
                pressure: bytes[2],
                timestamp,
            }),
            0xC0 if bytes.len() >= 2 => Some(MidiMessage::ProgramChange {
                channel,
                program: bytes[1],
                timestamp,
            }),
            _ => None,
        }
    }
}

/// Velocity curve for mapping MIDI velocity to amplitude.
#[derive(Debug, Clone, Default)]
pub enum VelocityCurve {
    /// Linear mapping (default)
    #[default]
    Linear,
    /// Fixed velocity (ignores input)
    Fixed(f32),
    /// Exponential curve (more dynamic range)
    Exponential,
    /// Compressed curve (softer dynamics)
    Compressed,
}

impl VelocityCurve {
    /// Apply the velocity curve to a MIDI velocity (0-127).
    /// Returns a value in the range 0.0-1.0.
    pub fn apply(&self, velocity: u8) -> f32 {
        let v = velocity as f32 / 127.0;
        match self {
            VelocityCurve::Linear => v,
            VelocityCurve::Fixed(fixed) => *fixed,
            VelocityCurve::Exponential => v * v,
            VelocityCurve::Compressed => v.sqrt(),
        }
    }
}

/// Parameter curve for CC-to-parameter mapping.
#[derive(Debug, Clone, Default)]
pub enum ParameterCurve {
    /// Linear interpolation (default)
    #[default]
    Linear,
    /// Logarithmic (good for frequency)
    Logarithmic,
    /// Exponential (good for volume)
    Exponential,
}

impl ParameterCurve {
    /// Apply the curve to a normalized value (0.0-1.0).
    /// Returns a value in the range min-max.
    pub fn apply(&self, value: f32, min: f32, max: f32) -> f32 {
        let v = value.clamp(0.0, 1.0);
        let curved = match self {
            ParameterCurve::Linear => v,
            ParameterCurve::Logarithmic => {
                // Log curve: more resolution at low end
                if v <= 0.0 {
                    0.0
                } else {
                    (v.ln() + 1.0).max(0.0) / 1.0
                }
            }
            ParameterCurve::Exponential => v * v,
        };
        min + curved * (max - min)
    }
}

/// Route for keyboard (note) input to a voice.
#[derive(Debug, Clone)]
pub struct KeyboardRoute {
    /// Target voice name
    pub voice_name: String,
    /// MIDI channel filter (None = all channels)
    pub channel: Option<u8>,
    /// Note range filter (inclusive)
    pub note_range: Option<(u8, u8)>,
    /// Semitone transposition
    pub transpose: i8,
    /// Velocity curve
    pub velocity_curve: VelocityCurve,
}

impl KeyboardRoute {
    /// Create a new keyboard route to a voice.
    pub fn new(voice_name: String) -> Self {
        Self {
            voice_name,
            channel: None,
            note_range: None,
            transpose: 0,
            velocity_curve: VelocityCurve::default(),
        }
    }

    /// Check if this route matches a MIDI note event.
    pub fn matches(&self, channel: u8, note: u8) -> bool {
        // Check channel filter
        if let Some(ch) = self.channel {
            if ch != channel {
                return false;
            }
        }

        // Check note range filter
        if let Some((low, high)) = self.note_range {
            if note < low || note > high {
                return false;
            }
        }

        true
    }

    /// Apply transposition to a note.
    pub fn transpose_note(&self, note: u8) -> u8 {
        let transposed = note as i16 + self.transpose as i16;
        transposed.clamp(0, 127) as u8
    }
}

/// Route for a specific MIDI note to a voice (for drum pads).
#[derive(Debug, Clone)]
pub struct NoteRoute {
    /// Target voice name
    pub voice_name: String,
    /// MIDI channel filter (None = all channels)
    pub channel: Option<u8>,
    /// Choke group (notes in same group cut each other off)
    pub choke_group: Option<String>,
    /// Velocity curve
    pub velocity_curve: VelocityCurve,
    /// Additional parameter mappings from velocity
    pub velocity_params: Vec<(String, f32, f32)>, // (param_name, min, max)
}

impl NoteRoute {
    /// Create a new note route to a voice.
    pub fn new(voice_name: String) -> Self {
        Self {
            voice_name,
            channel: None,
            choke_group: None,
            velocity_curve: VelocityCurve::default(),
            velocity_params: Vec::new(),
        }
    }
}

/// Route for CC to a parameter.
#[derive(Debug, Clone)]
pub struct CcRoute {
    /// Target type and name
    pub target: CcTarget,
    /// Parameter name
    pub param_name: String,
    /// Minimum value
    pub min_value: f32,
    /// Maximum value
    pub max_value: f32,
    /// Parameter curve
    pub curve: ParameterCurve,
    /// MIDI channel filter (None = all channels)
    pub channel: Option<u8>,
}

impl CcRoute {
    /// Create a new CC route to a voice parameter.
    pub fn new_voice(voice_name: String, param_name: String, min: f32, max: f32) -> Self {
        Self {
            target: CcTarget::Voice(voice_name),
            param_name,
            min_value: min,
            max_value: max,
            curve: ParameterCurve::default(),
            channel: None,
        }
    }

    /// Create a new CC route to an effect parameter.
    pub fn new_effect(effect_id: String, param_name: String, min: f32, max: f32) -> Self {
        Self {
            target: CcTarget::Effect(effect_id),
            param_name,
            min_value: min,
            max_value: max,
            curve: ParameterCurve::default(),
            channel: None,
        }
    }

    /// Create a new CC route to a group parameter.
    pub fn new_group(group_path: String, param_name: String, min: f32, max: f32) -> Self {
        Self {
            target: CcTarget::Group(group_path),
            param_name,
            min_value: min,
            max_value: max,
            curve: ParameterCurve::default(),
            channel: None,
        }
    }

    /// Apply the CC value (0-127) to get the parameter value.
    pub fn apply(&self, cc_value: u8) -> f32 {
        let normalized = cc_value as f32 / 127.0;
        self.curve.apply(normalized, self.min_value, self.max_value)
    }
}

/// Target for CC routing.
#[derive(Debug, Clone)]
pub enum CcTarget {
    /// Voice parameter
    Voice(String),
    /// Effect parameter
    Effect(String),
    /// Group parameter
    Group(String),
    /// Global parameter (tempo, master volume, etc.)
    Global(String),
}

/// A pending MIDI callback waiting to be executed.
///
/// When a MIDI event triggers a callback, we queue it here for execution
/// by the main script thread (which has access to the Rhai engine).
#[derive(Debug, Clone)]
pub struct PendingMidiCallback {
    /// Unique callback ID (used to look up the FnPtr)
    pub callback_id: u64,
    /// Velocity (for note callbacks) or CC value (for CC callbacks)
    pub value: i64,
}

/// Callback registration for a specific MIDI note.
#[derive(Debug, Clone)]
pub struct NoteCallback {
    /// MIDI channel filter (None = all channels)
    pub channel: Option<u8>,
    /// MIDI note number
    pub note: u8,
    /// Only trigger on note-on (not note-off)
    pub on_note_on: bool,
    /// Only trigger on note-off
    pub on_note_off: bool,
    /// Unique callback ID (used to look up the FnPtr)
    pub callback_id: u64,
}

/// Callback registration for a CC change.
#[derive(Debug, Clone)]
pub struct CcCallback {
    /// MIDI channel filter (None = all channels)
    pub channel: Option<u8>,
    /// CC number
    pub cc_number: u8,
    /// Threshold for triggering (0-127). If set, only triggers when value crosses threshold.
    pub threshold: Option<u8>,
    /// Trigger when going above threshold (vs below)
    pub above_threshold: bool,
    /// Unique callback ID (used to look up the FnPtr)
    pub callback_id: u64,
}

/// Central MIDI routing configuration.
#[derive(Debug, Clone, Default)]
pub struct MidiRouting {
    /// Keyboard routes (checked in order, first match wins)
    pub keyboard_routes: Vec<KeyboardRoute>,
    /// Note-specific routes (for drum pads): (channel, note) -> route
    pub note_routes: HashMap<(u8, u8), NoteRoute>,
    /// CC routes: (channel, cc_number) -> routes
    pub cc_routes: HashMap<(u8, u8), Vec<CcRoute>>,
    /// Pitch bend routes: channel -> routes
    pub pitch_bend_routes: HashMap<u8, Vec<CcRoute>>,
    /// Aftertouch routes: channel -> routes
    pub aftertouch_routes: HashMap<u8, Vec<CcRoute>>,
    /// Choke groups: group_name -> list of (voice_name) currently active
    pub choke_groups: HashMap<String, Vec<String>>,
    /// Note callbacks: (channel, note) -> callbacks
    pub note_callbacks: HashMap<(u8, u8), Vec<NoteCallback>>,
    /// CC callbacks: (channel, cc_number) -> callbacks
    pub cc_callbacks: HashMap<(u8, u8), Vec<CcCallback>>,
    /// Last CC values for threshold detection: (channel, cc_number) -> last_value
    pub last_cc_values: HashMap<(u8, u8), u8>,
    /// Pending callbacks waiting to be executed
    pub pending_callbacks: Vec<PendingMidiCallback>,
    /// Enable MIDI monitoring (print all events)
    pub monitor_enabled: bool,
}

impl MidiRouting {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a keyboard route.
    pub fn add_keyboard_route(&mut self, route: KeyboardRoute) {
        self.keyboard_routes.push(route);
    }

    /// Add a note-specific route (for drum pads).
    /// If channel is None, route applies to all channels.
    pub fn add_note_route(&mut self, channel: Option<u8>, note: u8, route: NoteRoute) {
        // Use channel 255 to indicate "all channels"
        let ch = channel.unwrap_or(255);
        self.note_routes.insert((ch, note), route);
    }

    /// Add a CC route.
    pub fn add_cc_route(&mut self, channel: Option<u8>, cc_number: u8, route: CcRoute) {
        let ch = channel.unwrap_or(255);
        self.cc_routes
            .entry((ch, cc_number))
            .or_default()
            .push(route);
    }

    /// Add a pitch bend route.
    pub fn add_pitch_bend_route(&mut self, channel: Option<u8>, route: CcRoute) {
        let ch = channel.unwrap_or(255);
        self.pitch_bend_routes.entry(ch).or_default().push(route);
    }

    /// Find matching keyboard routes for a note event.
    pub fn find_keyboard_routes(&self, channel: u8, note: u8) -> Vec<&KeyboardRoute> {
        self.keyboard_routes
            .iter()
            .filter(|r| r.matches(channel, note))
            .collect()
    }

    /// Find a note-specific route (for drum pads).
    pub fn find_note_route(&self, channel: u8, note: u8) -> Option<&NoteRoute> {
        // First try channel-specific route
        if let Some(route) = self.note_routes.get(&(channel, note)) {
            return Some(route);
        }
        // Then try all-channel route
        self.note_routes.get(&(255, note))
    }

    /// Find CC routes for a controller.
    pub fn find_cc_routes(&self, channel: u8, cc_number: u8) -> Vec<&CcRoute> {
        let mut routes = Vec::new();
        // Channel-specific routes
        if let Some(r) = self.cc_routes.get(&(channel, cc_number)) {
            routes.extend(r.iter());
        }
        // All-channel routes
        if let Some(r) = self.cc_routes.get(&(255, cc_number)) {
            routes.extend(r.iter());
        }
        routes
    }

    /// Find pitch bend routes.
    pub fn find_pitch_bend_routes(&self, channel: u8) -> Vec<&CcRoute> {
        let mut routes = Vec::new();
        if let Some(r) = self.pitch_bend_routes.get(&channel) {
            routes.extend(r.iter());
        }
        if let Some(r) = self.pitch_bend_routes.get(&255) {
            routes.extend(r.iter());
        }
        routes
    }

    /// Clear all routes.
    pub fn clear(&mut self) {
        self.keyboard_routes.clear();
        self.note_routes.clear();
        self.cc_routes.clear();
        self.pitch_bend_routes.clear();
        self.aftertouch_routes.clear();
        self.choke_groups.clear();
        self.note_callbacks.clear();
        self.cc_callbacks.clear();
        self.last_cc_values.clear();
        self.pending_callbacks.clear();
    }

    /// Queue a callback for execution.
    pub fn queue_callback(&mut self, callback_id: u64, value: i64) {
        self.pending_callbacks.push(PendingMidiCallback {
            callback_id,
            value,
        });
    }

    /// Drain all pending callbacks.
    pub fn drain_pending_callbacks(&mut self) -> Vec<PendingMidiCallback> {
        std::mem::take(&mut self.pending_callbacks)
    }

    /// Add a note callback.
    pub fn add_note_callback(&mut self, callback: NoteCallback) {
        let ch = callback.channel.unwrap_or(255);
        self.note_callbacks
            .entry((ch, callback.note))
            .or_default()
            .push(callback);
    }

    /// Add a CC callback.
    pub fn add_cc_callback(&mut self, callback: CcCallback) {
        let ch = callback.channel.unwrap_or(255);
        self.cc_callbacks
            .entry((ch, callback.cc_number))
            .or_default()
            .push(callback);
    }

    /// Find note callbacks that match a note event.
    /// Returns matching callbacks for note-on or note-off based on is_note_on flag.
    pub fn find_note_callbacks(&self, channel: u8, note: u8, is_note_on: bool) -> Vec<&NoteCallback> {
        let mut callbacks = Vec::new();

        // Channel-specific callbacks
        if let Some(cbs) = self.note_callbacks.get(&(channel, note)) {
            for cb in cbs {
                if (is_note_on && cb.on_note_on) || (!is_note_on && cb.on_note_off) {
                    callbacks.push(cb);
                }
            }
        }

        // All-channel callbacks
        if let Some(cbs) = self.note_callbacks.get(&(255, note)) {
            for cb in cbs {
                if (is_note_on && cb.on_note_on) || (!is_note_on && cb.on_note_off) {
                    callbacks.push(cb);
                }
            }
        }

        callbacks
    }

    /// Check CC callbacks and queue any that should trigger for a CC value change.
    /// This handles threshold detection - callbacks only trigger when crossing the threshold.
    pub fn check_and_queue_cc_callbacks(
        &mut self,
        channel: u8,
        cc_number: u8,
        new_value: u8,
    ) {
        let key = (channel, cc_number);
        let last_value = self.last_cc_values.get(&key).copied().unwrap_or(0);
        self.last_cc_values.insert(key, new_value);

        // Collect callback IDs to trigger (to avoid borrow issues)
        let mut to_trigger = Vec::new();

        // Check channel-specific callbacks
        for ch_key in [(channel, cc_number), (255, cc_number)] {
            if let Some(cbs) = self.cc_callbacks.get(&ch_key) {
                for cb in cbs {
                    if let Some(threshold) = cb.threshold {
                        // Only trigger when crossing the threshold
                        let was_above = last_value >= threshold;
                        let is_above = new_value >= threshold;

                        if cb.above_threshold {
                            // Trigger when going above threshold
                            if !was_above && is_above {
                                to_trigger.push(cb.callback_id);
                            }
                        } else {
                            // Trigger when going below threshold
                            if was_above && !is_above {
                                to_trigger.push(cb.callback_id);
                            }
                        }
                    } else {
                        // No threshold - trigger on every CC change
                        to_trigger.push(cb.callback_id);
                    }
                }
            }
        }

        // Queue all triggered callbacks
        for callback_id in to_trigger {
            self.queue_callback(callback_id, new_value as i64);
        }
    }
}

/// Information about a connected MIDI device.
#[derive(Debug, Clone)]
pub struct MidiDeviceInfo {
    /// Device name (as reported by the system)
    pub name: String,
    /// Port index (for opening)
    pub port_index: usize,
    /// MIDI backend (ALSA or JACK)
    pub backend: MidiBackend,
}

/// MIDI input manager.
///
/// Handles device discovery, connection, and message routing.
/// Supports both ALSA (via midir) and JACK MIDI backends.
pub struct MidiInputManager {
    /// Channel for sending MIDI messages to the runtime
    message_tx: Sender<MidiMessage>,
    /// Active ALSA connections (kept alive)
    alsa_connections: Vec<MidiInputConnection<()>>,
    /// Active JACK MIDI client (kept alive)
    jack_client: Option<JackMidiClient>,
    /// Device info for connected devices
    connected_devices: Vec<MidiDeviceInfo>,
}

impl MidiInputManager {
    /// Create a new MIDI input manager.
    pub fn new() -> (Self, Receiver<MidiMessage>) {
        let (tx, rx) = unbounded();
        (
            Self {
                message_tx: tx,
                alsa_connections: Vec::new(),
                jack_client: None,
                connected_devices: Vec::new(),
            },
            rx,
        )
    }

    /// List available MIDI input devices.
    pub fn list_devices() -> Result<Vec<MidiDeviceInfo>, String> {
        let midi_in =
            MidiInput::new("vibelang-probe").map_err(|e| format!("Failed to create MIDI input: {}", e))?;

        let ports = midi_in.ports();
        let mut devices = Vec::new();

        for (index, port) in ports.iter().enumerate() {
            let name = midi_in
                .port_name(port)
                .unwrap_or_else(|_| format!("Unknown Device {}", index));
            devices.push(MidiDeviceInfo {
                name,
                port_index: index,
                backend: MidiBackend::Alsa,
            });
        }

        Ok(devices)
    }

    /// Open a MIDI input device by name (partial match, case-insensitive).
    pub fn open_by_name(&mut self, name: &str) -> Result<MidiDeviceInfo, String> {
        let devices = Self::list_devices()?;
        let name_lower = name.to_lowercase();

        let device = devices
            .into_iter()
            .find(|d| d.name.to_lowercase().contains(&name_lower))
            .ok_or_else(|| format!("No MIDI device found matching '{}'", name))?;

        self.open_by_index(device.port_index)
    }

    /// Open a MIDI input device by port index.
    pub fn open_by_index(&mut self, port_index: usize) -> Result<MidiDeviceInfo, String> {
        let midi_in = MidiInput::new("vibelang")
            .map_err(|e| format!("Failed to create MIDI input: {}", e))?;

        let ports = midi_in.ports();
        let port = ports
            .get(port_index)
            .ok_or_else(|| format!("Invalid MIDI port index: {}", port_index))?;

        let name = midi_in
            .port_name(port)
            .unwrap_or_else(|_| format!("Unknown Device {}", port_index));

        let device_info = MidiDeviceInfo {
            name: name.clone(),
            port_index,
            backend: MidiBackend::Alsa,
        };

        let tx = self.message_tx.clone();

        let connection = midi_in
            .connect(
                port,
                "vibelang-input",
                move |timestamp, bytes, _| {
                    log::debug!("[MIDI RAW] timestamp={} bytes={:?}", timestamp, bytes);
                    if let Some(msg) = MidiMessage::from_bytes(bytes, timestamp) {
                        log::debug!("[MIDI PARSED] {:?}", msg);
                        let _ = tx.send(msg);
                    }
                },
                (),
            )
            .map_err(|e| format!("Failed to connect to MIDI device: {}", e))?;

        self.alsa_connections.push(connection);
        self.connected_devices.push(device_info.clone());

        log::info!("Connected to ALSA MIDI device: {} (port {})", name, port_index);

        Ok(device_info)
    }

    /// Open a JACK MIDI input port.
    ///
    /// This creates a JACK MIDI input port that can be connected to other
    /// JACK MIDI sources using `jack_connect` or a patchbay.
    ///
    /// If `auto_connect` is Some, it will try to connect to that JACK port.
    pub fn open_jack(&mut self, auto_connect: Option<&str>) -> Result<MidiDeviceInfo, String> {
        if self.jack_client.is_some() {
            return Err("JACK MIDI client already open".to_string());
        }

        let client = JackMidiClient::new("vibelang", "midi_in", self.message_tx.clone())?;
        let device_info = client.device_info().clone();

        // Auto-connect if requested
        if let Some(source_port) = auto_connect {
            let dest_port = "vibelang:midi_in".to_string();
            if let Err(e) = connect_jack_midi("vibelang-connect", source_port, &dest_port) {
                log::warn!("Failed to auto-connect JACK MIDI: {}", e);
            }
        }

        self.jack_client = Some(client);
        self.connected_devices.push(device_info.clone());

        Ok(device_info)
    }

    /// Open a JACK MIDI port and connect to a specific source.
    pub fn open_jack_source(&mut self, source_name: &str) -> Result<MidiDeviceInfo, String> {
        self.open_jack(Some(source_name))
    }

    /// Open a MIDI device based on its device info.
    ///
    /// This is the unified method that automatically uses the correct backend
    /// based on the device info.
    pub fn open_device(&mut self, device: &MidiDeviceInfo) -> Result<MidiDeviceInfo, String> {
        match device.backend {
            MidiBackend::Alsa => self.open_by_index(device.port_index),
            MidiBackend::Jack => self.open_jack(Some(&device.name)),
        }
    }

    /// Open the first available MIDI device.
    pub fn open_first(&mut self) -> Result<MidiDeviceInfo, String> {
        let devices = list_all_midi_devices();
        if devices.is_empty() {
            return Err("No MIDI devices available".to_string());
        }
        self.open_device(&devices[0])
    }

    /// Get list of currently connected devices.
    pub fn connected_devices(&self) -> &[MidiDeviceInfo] {
        &self.connected_devices
    }

    /// Close all connections (both ALSA and JACK).
    pub fn close_all(&mut self) {
        self.alsa_connections.clear();
        self.jack_client = None;
        self.connected_devices.clear();
    }
}

impl Default for MidiInputManager {
    fn default() -> Self {
        Self::new().0
    }
}

// ============================================================================
// JACK MIDI Support
// ============================================================================

/// JACK MIDI client for receiving MIDI from JACK.
///
/// This creates a JACK client with a MIDI input port that can be connected
/// to other JACK MIDI sources (hardware, software synthesizers, etc.).
pub struct JackMidiClient {
    /// The active JACK client (kept alive)
    _async_client: jack::AsyncClient<JackNotifications, JackMidiProcessor>,
    /// Device info
    device_info: MidiDeviceInfo,
}

/// JACK notification handler.
struct JackNotifications;

impl jack::NotificationHandler for JackNotifications {
    unsafe fn shutdown(&mut self, status: jack::ClientStatus, reason: &str) {
        log::warn!("JACK client shutdown: {:?} - {}", status, reason);
    }
}

/// JACK MIDI processor - runs in the JACK realtime thread.
struct JackMidiProcessor {
    /// MIDI input port
    midi_in: Port<MidiIn>,
    /// Channel for sending MIDI messages
    tx: Sender<MidiMessage>,
    /// Frame time for timestamps
    frame_time: Arc<std::sync::atomic::AtomicU64>,
}

impl jack::ProcessHandler for JackMidiProcessor {
    fn process(&mut self, _client: &jack::Client, ps: &ProcessScope) -> jack::Control {
        // Update frame time for timestamps
        let frames = ps.last_frame_time();
        self.frame_time.store(frames as u64, Ordering::Relaxed);

        // Read all MIDI events from the port
        for event in self.midi_in.iter(ps) {
            let bytes = event.bytes;
            let timestamp = frames as u64 + event.time as u64;

            if let Some(msg) = MidiMessage::from_bytes(bytes, timestamp) {
                let _ = self.tx.send(msg);
            }
        }

        jack::Control::Continue
    }
}

impl JackMidiClient {
    /// Create a new JACK MIDI client with a MIDI input port.
    ///
    /// The port will be named "vibelang:midi_in" and can be connected
    /// to other JACK MIDI sources using `jack_connect` or a patchbay.
    pub fn new(
        client_name: &str,
        port_name: &str,
        tx: Sender<MidiMessage>,
    ) -> Result<Self, String> {
        // Create JACK client
        let (client, _status) = Client::new(client_name, ClientOptions::NO_START_SERVER)
            .map_err(|e| format!("Failed to create JACK client: {}", e))?;

        // Register MIDI input port
        let midi_in = client
            .register_port(port_name, MidiIn::default())
            .map_err(|e| format!("Failed to register JACK MIDI port: {}", e))?;

        let full_port_name = format!("{}:{}", client_name, port_name);
        let device_info = MidiDeviceInfo {
            name: full_port_name.clone(),
            port_index: 0, // JACK doesn't use port indices
            backend: MidiBackend::Jack,
        };

        let frame_time = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let processor = JackMidiProcessor {
            midi_in,
            tx,
            frame_time,
        };

        let notifications = JackNotifications;

        // Activate the client
        let async_client = client
            .activate_async(notifications, processor)
            .map_err(|e| format!("Failed to activate JACK client: {}", e))?;

        log::info!("JACK MIDI client '{}' created with port '{}'", client_name, port_name);

        Ok(Self {
            _async_client: async_client,
            device_info,
        })
    }

    /// Get the device info for this JACK MIDI client.
    pub fn device_info(&self) -> &MidiDeviceInfo {
        &self.device_info
    }
}

/// List available JACK MIDI output ports that can be connected to.
///
/// These are ports from other JACK clients that output MIDI data.
pub fn list_jack_midi_sources() -> Result<Vec<MidiDeviceInfo>, String> {
    // Create a temporary client just for listing ports
    let (client, _status) = Client::new("vibelang-probe", ClientOptions::NO_START_SERVER)
        .map_err(|e| format!("Failed to create JACK client for probing: {}", e))?;

    let mut devices = Vec::new();

    // Get all MIDI output ports (these can be connected to our input)
    let port_names = client.ports(
        None,                    // No name filter
        Some("8 bit raw midi"), // MIDI port type
        jack::PortFlags::IS_OUTPUT, // Only output ports (we connect to them)
    );

    for (index, name) in port_names.iter().enumerate() {
        devices.push(MidiDeviceInfo {
            name: name.clone(),
            port_index: index,
            backend: MidiBackend::Jack,
        });
    }

    Ok(devices)
}

/// Check if JACK is running.
pub fn is_jack_running() -> bool {
    Client::new("vibelang-check", ClientOptions::NO_START_SERVER).is_ok()
}

/// Connect a JACK MIDI source to our input port.
pub fn connect_jack_midi(client_name: &str, source_port: &str, dest_port: &str) -> Result<(), String> {
    let (client, _status) = Client::new(client_name, ClientOptions::NO_START_SERVER)
        .map_err(|e| format!("Failed to create JACK client: {}", e))?;

    client
        .connect_ports_by_name(source_port, dest_port)
        .map_err(|e| format!("Failed to connect JACK MIDI ports: {}", e))?;

    log::info!("Connected JACK MIDI: {} -> {}", source_port, dest_port);
    Ok(())
}

// ============================================================================
// JACK MIDI Output (Virtual Keyboard)
// ============================================================================

/// A queued MIDI event for output
#[derive(Clone)]
pub struct QueuedMidiEvent {
    /// Raw MIDI bytes (up to 3 bytes for standard messages)
    pub bytes: Vec<u8>,
}

impl QueuedMidiEvent {
    /// Create a note-on event
    pub fn note_on(channel: u8, note: u8, velocity: u8) -> Self {
        Self {
            bytes: vec![0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F],
        }
    }

    /// Create a note-off event
    pub fn note_off(channel: u8, note: u8) -> Self {
        Self {
            bytes: vec![0x80 | (channel & 0x0F), note & 0x7F, 0],
        }
    }

    /// Create a control change event
    pub fn control_change(channel: u8, controller: u8, value: u8) -> Self {
        Self {
            bytes: vec![0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F],
        }
    }
}

/// JACK MIDI output client for the virtual keyboard.
///
/// Creates a JACK client with a MIDI output port that appears as a MIDI source.
/// Other applications (including vibelang's own MIDI input) can connect to it.
pub struct JackMidiOutput {
    /// The active JACK client
    _async_client: jack::AsyncClient<JackNotifications, JackMidiOutputProcessor>,
    /// Queue for sending MIDI events to the JACK thread
    event_tx: Sender<QueuedMidiEvent>,
    /// Full port name for connections
    pub port_name: String,
}

/// JACK MIDI output processor - runs in the JACK realtime thread.
struct JackMidiOutputProcessor {
    /// MIDI output port
    midi_out: Port<jack::MidiOut>,
    /// Channel for receiving MIDI events to send
    event_rx: Receiver<QueuedMidiEvent>,
}

impl jack::ProcessHandler for JackMidiOutputProcessor {
    fn process(&mut self, _client: &jack::Client, ps: &ProcessScope) -> jack::Control {
        let mut writer = self.midi_out.writer(ps);

        // Drain all queued events and write them to the output port
        while let Ok(event) = self.event_rx.try_recv() {
            // Write at frame 0 (immediate)
            let raw = jack::RawMidi {
                time: 0,
                bytes: &event.bytes,
            };
            let _ = writer.write(&raw);
        }

        jack::Control::Continue
    }
}

impl JackMidiOutput {
    /// Create a new JACK MIDI output client.
    ///
    /// The port will be named "{client_name}:{port_name}" and can be connected
    /// to other JACK MIDI inputs.
    pub fn new(client_name: &str, port_name: &str) -> Result<Self, String> {
        // Create JACK client
        let (client, _status) = Client::new(client_name, ClientOptions::NO_START_SERVER)
            .map_err(|e| format!("Failed to create JACK client: {}", e))?;

        // Register MIDI output port
        let midi_out = client
            .register_port(port_name, jack::MidiOut::default())
            .map_err(|e| format!("Failed to register JACK MIDI output port: {}", e))?;

        let full_port_name = format!("{}:{}", client.name(), port_name);

        // Create channel for sending events
        let (event_tx, event_rx) = unbounded();

        let processor = JackMidiOutputProcessor { midi_out, event_rx };
        let notifications = JackNotifications;

        // Activate the client
        let async_client = client
            .activate_async(notifications, processor)
            .map_err(|e| format!("Failed to activate JACK client: {}", e))?;

        log::info!(
            "JACK MIDI output '{}' created - connect to '{}'",
            client_name,
            full_port_name
        );

        Ok(Self {
            _async_client: async_client,
            event_tx,
            port_name: full_port_name,
        })
    }

    /// Send a MIDI event to the output port.
    pub fn send(&self, event: QueuedMidiEvent) -> Result<(), String> {
        self.event_tx
            .send(event)
            .map_err(|e| format!("Failed to queue MIDI event: {}", e))
    }

    /// Send a note-on event.
    pub fn note_on(&self, channel: u8, note: u8, velocity: u8) -> Result<(), String> {
        self.send(QueuedMidiEvent::note_on(channel, note, velocity))
    }

    /// Send a note-off event.
    pub fn note_off(&self, channel: u8, note: u8) -> Result<(), String> {
        self.send(QueuedMidiEvent::note_off(channel, note))
    }

    /// Get the full port name for connections.
    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}

// ============================================================================
// Combined Device Discovery
// ============================================================================

/// List all available MIDI devices (both ALSA and JACK).
pub fn list_all_midi_devices() -> Vec<MidiDeviceInfo> {
    let mut all_devices = Vec::new();

    // Get ALSA devices
    if let Ok(alsa_devices) = MidiInputManager::list_devices() {
        all_devices.extend(alsa_devices);
    }

    // Get JACK MIDI sources if JACK is running
    if is_jack_running() {
        if let Ok(jack_devices) = list_jack_midi_sources() {
            all_devices.extend(jack_devices);
        }
    }

    all_devices
}

/// Shared MIDI state accessible from multiple threads.
#[derive(Clone)]
pub struct SharedMidiState {
    inner: Arc<RwLock<MidiStateInner>>,
}

struct MidiStateInner {
    routing: MidiRouting,
}

impl SharedMidiState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MidiStateInner {
                routing: MidiRouting::new(),
            })),
        }
    }

    /// Get a clone of the current routing configuration.
    pub fn get_routing(&self) -> MidiRouting {
        self.inner.read().unwrap().routing.clone()
    }

    /// Update the routing configuration.
    pub fn update_routing<F>(&self, f: F)
    where
        F: FnOnce(&mut MidiRouting),
    {
        let mut inner = self.inner.write().unwrap();
        f(&mut inner.routing);
    }

    /// Update the routing configuration and return a value.
    pub fn update_routing_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut MidiRouting) -> R,
    {
        let mut inner = self.inner.write().unwrap();
        f(&mut inner.routing)
    }

    /// Clear all routing.
    pub fn clear_routing(&self) {
        self.inner.write().unwrap().routing.clear();
    }
}

impl Default for SharedMidiState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_note_on() {
        let bytes = [0x90, 60, 100]; // Note on, channel 0, middle C, velocity 100
        let msg = MidiMessage::from_bytes(&bytes, 0).unwrap();
        match msg {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
                ..
            } => {
                assert_eq!(channel, 0);
                assert_eq!(note, 60);
                assert_eq!(velocity, 100);
            }
            _ => panic!("Expected NoteOn"),
        }
    }

    #[test]
    fn test_parse_note_on_velocity_zero() {
        let bytes = [0x90, 60, 0]; // Note on with velocity 0 = note off
        let msg = MidiMessage::from_bytes(&bytes, 0).unwrap();
        match msg {
            MidiMessage::NoteOff { channel, note, .. } => {
                assert_eq!(channel, 0);
                assert_eq!(note, 60);
            }
            _ => panic!("Expected NoteOff"),
        }
    }

    #[test]
    fn test_parse_cc() {
        let bytes = [0xB0, 1, 64]; // CC, channel 0, mod wheel, value 64
        let msg = MidiMessage::from_bytes(&bytes, 0).unwrap();
        match msg {
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
                ..
            } => {
                assert_eq!(channel, 0);
                assert_eq!(controller, 1);
                assert_eq!(value, 64);
            }
            _ => panic!("Expected ControlChange"),
        }
    }

    #[test]
    fn test_parse_pitch_bend() {
        let bytes = [0xE0, 0, 64]; // Pitch bend, channel 0, center position
        let msg = MidiMessage::from_bytes(&bytes, 0).unwrap();
        match msg {
            MidiMessage::PitchBend { channel, value, .. } => {
                assert_eq!(channel, 0);
                assert_eq!(value, 0); // Center = 8192, so 8192 - 8192 = 0
            }
            _ => panic!("Expected PitchBend"),
        }
    }

    #[test]
    fn test_velocity_curves() {
        let linear = VelocityCurve::Linear;
        assert!((linear.apply(127) - 1.0).abs() < 0.01);
        assert!((linear.apply(64) - 0.5).abs() < 0.01);

        let fixed = VelocityCurve::Fixed(0.8);
        assert!((fixed.apply(0) - 0.8).abs() < 0.01);
        assert!((fixed.apply(127) - 0.8).abs() < 0.01);

        let exp = VelocityCurve::Exponential;
        assert!((exp.apply(127) - 1.0).abs() < 0.01);
        assert!(exp.apply(64) < 0.5); // Exponential is below linear
    }

    #[test]
    fn test_keyboard_route_matching() {
        let mut route = KeyboardRoute::new("bass".to_string());

        // No filter - matches everything
        assert!(route.matches(0, 60));
        assert!(route.matches(15, 127));

        // Channel filter
        route.channel = Some(1);
        assert!(!route.matches(0, 60));
        assert!(route.matches(1, 60));

        // Note range filter
        route.channel = None;
        route.note_range = Some((36, 59));
        assert!(route.matches(0, 36));
        assert!(route.matches(0, 59));
        assert!(!route.matches(0, 60));
        assert!(!route.matches(0, 35));
    }

    #[test]
    fn test_cc_route_apply() {
        let route = CcRoute::new_voice("lead".to_string(), "cutoff".to_string(), 200.0, 8000.0);

        assert!((route.apply(0) - 200.0).abs() < 1.0);
        assert!((route.apply(127) - 8000.0).abs() < 1.0);
        assert!((route.apply(64) - 4100.0).abs() < 100.0); // ~midpoint
    }
}
