//! MIDI device management with separate input/output namespaces.
//!
//! This module provides proper device management with:
//! - Separate ID namespaces for inputs and outputs
//! - Device enumeration and hot-plug detection
//! - Connection state tracking

use std::collections::HashMap;
use std::sync::Arc;

use super::events::TimestampedMidiEvent;
use super::parser::parse_midi_bytes;
use super::queue::{MidiEventQueue, MidiEventSender};
use super::{MidiOutputOpenProfile, PacedPanicClearOutput};

#[cfg(feature = "native")]
use midir::{MidiInput, MidiInputConnection, MidiOutput};

use crossbeam_channel::Sender;
use parking_lot::RwLock;
use std::time::Instant;

// ============================================================================
// Device IDs
// ============================================================================

/// ID for a MIDI input device.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub struct MidiInputId(pub u32);

impl MidiInputId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

/// ID for a MIDI output device.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub struct MidiOutputId(pub u32);

impl MidiOutputId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

// ============================================================================
// Device Info
// ============================================================================

/// Information about a MIDI input device.
#[derive(Clone, Debug)]
pub struct MidiInputInfo {
    /// Device ID.
    pub id: MidiInputId,
    /// Device name.
    pub name: String,
    /// Whether the device is currently connected.
    pub connected: bool,
}

/// Information about a MIDI output device.
#[derive(Clone, Debug)]
pub struct MidiOutputInfo {
    /// Device ID.
    pub id: MidiOutputId,
    /// Device name.
    pub name: String,
    /// Whether the device is currently connected.
    pub connected: bool,
}

/// Combined device info for devices that have both input and output.
#[derive(Clone, Debug)]
pub struct MidiDeviceInfo {
    /// Device name.
    pub name: String,
    /// Input capability.
    pub input: Option<MidiInputId>,
    /// Output capability.
    pub output: Option<MidiOutputId>,
}

// ============================================================================
// Device Manager
// ============================================================================

/// Central manager for MIDI devices.
///
/// Provides:
/// - Device enumeration
/// - Connection management
/// - Event routing
pub struct MidiDeviceManager {
    /// Known input devices.
    inputs: RwLock<HashMap<MidiInputId, InputDeviceState>>,
    /// Known output devices.
    outputs: RwLock<HashMap<MidiOutputId, OutputDeviceState>>,
    /// Event queue for incoming MIDI messages.
    event_queue: Arc<MidiEventQueue>,
    /// Next input ID to assign.
    /// Reserved for future dynamic device ID allocation.
    #[allow(dead_code)]
    next_input_id: RwLock<u32>,
    /// Next output ID to assign.
    /// Reserved for future dynamic device ID allocation.
    #[allow(dead_code)]
    next_output_id: RwLock<u32>,
}

/// Internal state for an input device.
struct InputDeviceState {
    info: MidiInputInfo,
    #[cfg(feature = "native")]
    connection: Option<MidiInputConnection<()>>,
    #[cfg(not(feature = "native"))]
    connection: Option<()>,
}

/// Internal state for an output device.
struct OutputDeviceState {
    info: MidiOutputInfo,
    #[cfg(feature = "native")]
    connection: Option<PacedPanicClearOutput>,
    #[cfg(not(feature = "native"))]
    connection: Option<()>,
    /// Channel for the output thread.
    output_tx: Option<Sender<super::realtime::QueuedMidiEvent>>,
}

impl MidiDeviceManager {
    /// Create a new device manager.
    pub fn new() -> Self {
        Self {
            inputs: RwLock::new(HashMap::new()),
            outputs: RwLock::new(HashMap::new()),
            event_queue: Arc::new(MidiEventQueue::with_default_capacity()),
            next_input_id: RwLock::new(0),
            next_output_id: RwLock::new(0),
        }
    }

    /// Get the event queue for receiving MIDI input events.
    pub fn event_queue(&self) -> Arc<MidiEventQueue> {
        Arc::clone(&self.event_queue)
    }

    /// Get a sender for the event queue.
    pub fn event_sender(&self) -> MidiEventSender {
        self.event_queue.sender()
    }

    /// Enumerate all available MIDI input devices.
    #[cfg(feature = "native")]
    pub fn list_inputs(&self) -> Vec<MidiInputInfo> {
        let midi_in = match MidiInput::new("vibelang-enumerate") {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to create MIDI input for enumeration: {}", e);
                return Vec::new();
            }
        };

        midi_in
            .ports()
            .iter()
            .enumerate()
            .filter_map(|(idx, port)| {
                let name = midi_in.port_name(port).ok()?;
                Some(MidiInputInfo {
                    id: MidiInputId::new(idx as u32),
                    name,
                    connected: false,
                })
            })
            .collect()
    }

    /// Enumerate all available MIDI output devices.
    #[cfg(feature = "native")]
    pub fn list_outputs(&self) -> Vec<MidiOutputInfo> {
        let midi_out = match MidiOutput::new("vibelang-enumerate") {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to create MIDI output for enumeration: {}", e);
                return Vec::new();
            }
        };

        midi_out
            .ports()
            .iter()
            .enumerate()
            .filter_map(|(idx, port)| {
                let name = midi_out.port_name(port).ok()?;
                Some(MidiOutputInfo {
                    id: MidiOutputId::new(idx as u32),
                    name,
                    connected: false,
                })
            })
            .collect()
    }

    /// List all devices with their capabilities.
    #[cfg(feature = "native")]
    pub fn list_all(&self) -> Vec<MidiDeviceInfo> {
        let inputs = self.list_inputs();
        let outputs = self.list_outputs();

        // Merge by name
        let mut by_name: HashMap<String, MidiDeviceInfo> = HashMap::new();

        for input in inputs {
            by_name
                .entry(input.name.clone())
                .or_insert_with(|| MidiDeviceInfo {
                    name: input.name.clone(),
                    input: None,
                    output: None,
                })
                .input = Some(input.id);
        }

        for output in outputs {
            by_name
                .entry(output.name.clone())
                .or_insert_with(|| MidiDeviceInfo {
                    name: output.name.clone(),
                    input: None,
                    output: None,
                })
                .output = Some(output.id);
        }

        by_name.into_values().collect()
    }

    /// Open a MIDI input device.
    #[cfg(feature = "native")]
    pub fn open_input(&self, id: MidiInputId) -> Result<(), String> {
        // Check if already open
        {
            let inputs = self.inputs.read();
            if let Some(state) = inputs.get(&id) {
                if state.connection.is_some() {
                    return Ok(()); // Already open
                }
            }
        }

        let midi_in = MidiInput::new("vibelang-input")
            .map_err(|e| format!("Failed to create MIDI input: {}", e))?;

        let ports = midi_in.ports();
        let port = ports
            .get(id.0 as usize)
            .ok_or_else(|| format!("MIDI input device {} not found", id.0))?;

        let port_name = midi_in
            .port_name(port)
            .unwrap_or_else(|_| format!("Input {}", id.0));

        // Create sender for this device
        let sender = self.event_queue.sender();
        let device_id = crate::types::ids::MidiDeviceId::new(id.0);

        // Connect with callback
        let connection = midi_in
            .connect(
                port,
                "vibelang-input",
                move |timestamp, data, _| {
                    if let Some(msg) = parse_midi_bytes(data) {
                        let event =
                            TimestampedMidiEvent::new(timestamp, Instant::now(), device_id, msg);
                        sender.try_send(event);
                    }
                },
                (),
            )
            .map_err(|e| format!("Failed to connect MIDI input: {}", e))?;

        // Store state
        let mut inputs = self.inputs.write();
        inputs.insert(
            id,
            InputDeviceState {
                info: MidiInputInfo {
                    id,
                    name: port_name.clone(),
                    connected: true,
                },
                connection: Some(connection),
            },
        );

        tracing::info!("Opened MIDI input: {} (id={})", port_name, id.0);
        Ok(())
    }

    /// Close a MIDI input device.
    pub fn close_input(&self, id: MidiInputId) -> Result<(), String> {
        let mut inputs = self.inputs.write();
        if let Some(state) = inputs.get_mut(&id) {
            state.connection = None;
            state.info.connected = false;
            tracing::info!("Closed MIDI input: {} (id={})", state.info.name, id.0);
            Ok(())
        } else {
            Err(format!("MIDI input {} not found", id.0))
        }
    }

    /// Open a MIDI output device.
    #[cfg(feature = "native")]
    pub fn open_output(&self, id: MidiOutputId) -> Result<(), String> {
        // Check if already open
        {
            let outputs = self.outputs.read();
            if let Some(state) = outputs.get(&id) {
                if state.connection.is_some() {
                    return Ok(()); // Already open
                }
            }
        }

        let midi_out = MidiOutput::new("vibelang-output")
            .map_err(|e| format!("Failed to create MIDI output: {}", e))?;

        let ports = midi_out.ports();
        let port = ports
            .get(id.0 as usize)
            .ok_or_else(|| format!("MIDI output device {} not found", id.0))?;

        let port_name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| format!("Output {}", id.0));

        let connection = midi_out
            .connect(port, "vibelang-output")
            .map_err(|e| format!("Failed to connect MIDI output: {}", e))?;
        let connection = PacedPanicClearOutput::new(
            connection,
            format!("{} (device-manager id={})", port_name, id.0),
            MidiOutputOpenProfile::for_device_name(&port_name),
        )?;

        // Store state
        let mut outputs = self.outputs.write();
        outputs.insert(
            id,
            OutputDeviceState {
                info: MidiOutputInfo {
                    id,
                    name: port_name.clone(),
                    connected: true,
                },
                connection: Some(connection),
                output_tx: None,
            },
        );

        tracing::info!("Opened MIDI output: {} (id={})", port_name, id.0);
        Ok(())
    }

    /// Close a MIDI output device.
    pub fn close_output(&self, id: MidiOutputId) -> Result<(), String> {
        let mut outputs = self.outputs.write();
        if let Some(state) = outputs.get_mut(&id) {
            state.connection = None;
            state.output_tx = None;
            state.info.connected = false;
            tracing::info!("Closed MIDI output: {} (id={})", state.info.name, id.0);
            Ok(())
        } else {
            Err(format!("MIDI output {} not found", id.0))
        }
    }

    /// Send MIDI bytes to an output device.
    #[cfg(feature = "native")]
    pub fn send(&self, id: MidiOutputId, data: &[u8]) -> Result<(), String> {
        let outputs = self.outputs.read();
        let state = outputs
            .get(&id)
            .ok_or_else(|| format!("MIDI output {} not found", id.0))?;

        let conn = state
            .connection
            .as_ref()
            .ok_or_else(|| format!("MIDI output {} not connected", id.0))?;

        conn.send(data)
    }

    /// Get info about a connected input device.
    pub fn get_input_info(&self, id: MidiInputId) -> Option<MidiInputInfo> {
        self.inputs.read().get(&id).map(|s| s.info.clone())
    }

    /// Get info about a connected output device.
    pub fn get_output_info(&self, id: MidiOutputId) -> Option<MidiOutputInfo> {
        self.outputs.read().get(&id).map(|s| s.info.clone())
    }

    /// Check if an input is connected.
    pub fn is_input_connected(&self, id: MidiInputId) -> bool {
        self.inputs
            .read()
            .get(&id)
            .map(|s| s.connection.is_some())
            .unwrap_or(false)
    }

    /// Check if an output is connected.
    pub fn is_output_connected(&self, id: MidiOutputId) -> bool {
        self.outputs
            .read()
            .get(&id)
            .map(|s| s.connection.is_some())
            .unwrap_or(false)
    }

    /// Close all connections.
    pub fn close_all(&self) {
        let mut inputs = self.inputs.write();
        for state in inputs.values_mut() {
            state.connection = None;
            state.info.connected = false;
        }

        let mut outputs = self.outputs.write();
        for state in outputs.values_mut() {
            state.connection = None;
            state.output_tx = None;
            state.info.connected = false;
        }

        tracing::info!("Closed all MIDI connections");
    }
}

impl Default for MidiDeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MidiDeviceManager {
    fn drop(&mut self) {
        self.close_all();
    }
}

// ============================================================================
// Stubs for non-native platforms
// ============================================================================

#[cfg(not(feature = "native"))]
impl MidiDeviceManager {
    pub fn list_inputs(&self) -> Vec<MidiInputInfo> {
        Vec::new()
    }

    pub fn list_outputs(&self) -> Vec<MidiOutputInfo> {
        Vec::new()
    }

    pub fn list_all(&self) -> Vec<MidiDeviceInfo> {
        Vec::new()
    }

    pub fn open_input(&self, _id: MidiInputId) -> Result<(), String> {
        Err("MIDI not supported on this platform".to_string())
    }

    pub fn open_output(&self, _id: MidiOutputId) -> Result<(), String> {
        Err("MIDI not supported on this platform".to_string())
    }

    pub fn send(&self, _id: MidiOutputId, _data: &[u8]) -> Result<(), String> {
        Err("MIDI not supported on this platform".to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_ids() {
        let input1 = MidiInputId::new(1);
        let input2 = MidiInputId::new(1);
        let output1 = MidiOutputId::new(1);

        assert_eq!(input1, input2);
        // Different types, can't compare directly
        assert_eq!(input1.0, output1.0);
    }

    #[test]
    fn test_device_manager_creation() {
        let manager = MidiDeviceManager::new();
        let queue = manager.event_queue();
        assert!(queue.is_empty());
    }
}
