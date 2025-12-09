//! MIDI output backends
//!
//! Currently supports JACK MIDI output. Other backends could be added in the future.

use crate::config::MidiSettings;
use crate::error::Result;
use std::sync::mpsc::{channel, Sender};

/// MIDI backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiBackend {
    /// JACK MIDI output
    Jack,
    /// No MIDI output (useful for testing)
    None,
}

/// MIDI message types
#[derive(Debug, Clone, Copy)]
pub enum MidiMessage {
    /// Note on: channel, note, velocity
    NoteOn { channel: u8, note: u8, velocity: u8 },
    /// Note off: channel, note
    NoteOff { channel: u8, note: u8 },
    /// Control change: channel, controller, value
    ControlChange { channel: u8, controller: u8, value: u8 },
}

impl MidiMessage {
    /// Convert to raw MIDI bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            MidiMessage::NoteOn { channel, note, velocity } => {
                vec![0x90 | (channel & 0x0F), *note & 0x7F, *velocity & 0x7F]
            }
            MidiMessage::NoteOff { channel, note } => {
                vec![0x80 | (channel & 0x0F), *note & 0x7F, 0]
            }
            MidiMessage::ControlChange { channel, controller, value } => {
                vec![0xB0 | (channel & 0x0F), *controller & 0x7F, *value & 0x7F]
            }
        }
    }
}

/// MIDI output trait
pub trait MidiOutput: Send {
    /// Send a note on message
    fn note_on(&self, channel: u8, note: u8, velocity: u8);

    /// Send a note off message
    fn note_off(&self, channel: u8, note: u8);

    /// Send a control change message
    fn control_change(&self, channel: u8, controller: u8, value: u8);

    /// Get the port name
    fn port_name(&self) -> &str;

    /// Check if connected
    fn is_connected(&self) -> bool;
}

/// JACK MIDI output
pub struct JackMidiOutput {
    /// Sender for MIDI messages to the JACK process callback
    tx: Sender<MidiMessage>,
    /// Port name
    port_name: String,
    /// Keep the client alive
    _client: jack::AsyncClient<(), JackMidiHandler>,
}

impl JackMidiOutput {
    /// Create a new JACK MIDI output
    pub fn new(client_name: &str, port_name: &str) -> Result<Self> {
        // Create JACK client
        let (client, _status) = jack::Client::new(client_name, jack::ClientOptions::NO_START_SERVER)?;

        // Create MIDI output port
        let midi_out = client.register_port(port_name, jack::MidiOut::default())?;

        // Create channel for sending MIDI messages
        let (tx, rx) = channel();

        let handler = JackMidiHandler {
            midi_out,
            rx,
        };

        // Activate the client
        let active_client = client.activate_async((), handler)?;

        let full_port_name = format!("{}:{}", client_name, port_name);

        Ok(Self {
            tx,
            port_name: full_port_name,
            _client: active_client,
        })
    }

    /// Create from settings
    pub fn from_settings(settings: &MidiSettings) -> Result<Self> {
        let output = Self::new(&settings.client_name, &settings.port_name)?;

        // Auto-connect if configured
        if let Some(ref destinations) = settings.auto_connect {
            for dest in destinations {
                if let Err(e) = output.connect_to(dest) {
                    log::warn!("Failed to auto-connect to {}: {}", dest, e);
                }
            }
        }

        Ok(output)
    }

    /// Connect to a JACK MIDI input port
    pub fn connect_to(&self, destination: &str) -> Result<()> {
        // Note: This requires the client to be active, which it is after new()
        // But we can't easily get access to it here since it's wrapped in AsyncClient
        // For now, users can use jack_connect externally or qjackctl
        log::info!("To connect: jack_connect {} {}", self.port_name, destination);
        Ok(())
    }
}

impl MidiOutput for JackMidiOutput {
    fn note_on(&self, channel: u8, note: u8, velocity: u8) {
        let _ = self.tx.send(MidiMessage::NoteOn { channel, note, velocity });
    }

    fn note_off(&self, channel: u8, note: u8) {
        let _ = self.tx.send(MidiMessage::NoteOff { channel, note });
    }

    fn control_change(&self, channel: u8, controller: u8, value: u8) {
        let _ = self.tx.send(MidiMessage::ControlChange { channel, controller, value });
    }

    fn port_name(&self) -> &str {
        &self.port_name
    }

    fn is_connected(&self) -> bool {
        true // If we got this far, we're connected
    }
}

/// JACK process handler for MIDI output
struct JackMidiHandler {
    midi_out: jack::Port<jack::MidiOut>,
    rx: std::sync::mpsc::Receiver<MidiMessage>,
}

impl jack::ProcessHandler for JackMidiHandler {
    fn process(&mut self, _client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let mut writer = self.midi_out.writer(ps);

        // Process all pending MIDI messages
        while let Ok(msg) = self.rx.try_recv() {
            let bytes = msg.to_bytes();
            let raw = jack::RawMidi {
                time: 0, // Immediate
                bytes: &bytes,
            };
            let _ = writer.write(&raw);
        }

        jack::Control::Continue
    }
}

/// Dummy MIDI output (for testing or when no backend is available)
pub struct DummyMidiOutput;

impl MidiOutput for DummyMidiOutput {
    fn note_on(&self, channel: u8, note: u8, velocity: u8) {
        log::debug!("MIDI Note On: ch={} note={} vel={}", channel, note, velocity);
    }

    fn note_off(&self, channel: u8, note: u8) {
        log::debug!("MIDI Note Off: ch={} note={}", channel, note);
    }

    fn control_change(&self, channel: u8, controller: u8, value: u8) {
        log::debug!("MIDI CC: ch={} cc={} val={}", channel, controller, value);
    }

    fn port_name(&self) -> &str {
        "dummy"
    }

    fn is_connected(&self) -> bool {
        false
    }
}

/// Check if JACK is running
pub fn is_jack_running() -> bool {
    jack::Client::new("term-keys-probe", jack::ClientOptions::NO_START_SERVER).is_ok()
}

/// List available JACK MIDI ports
pub fn list_jack_midi_ports() -> Vec<String> {
    if let Ok((client, _)) = jack::Client::new("term-keys-list", jack::ClientOptions::NO_START_SERVER) {
        client.ports(None, Some("midi"), jack::PortFlags::IS_INPUT)
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_message_bytes() {
        let note_on = MidiMessage::NoteOn { channel: 0, note: 60, velocity: 100 };
        assert_eq!(note_on.to_bytes(), vec![0x90, 60, 100]);

        let note_off = MidiMessage::NoteOff { channel: 1, note: 48 };
        assert_eq!(note_off.to_bytes(), vec![0x81, 48, 0]);

        let cc = MidiMessage::ControlChange { channel: 0, controller: 1, value: 64 };
        assert_eq!(cc.to_bytes(), vec![0xB0, 1, 64]);
    }

    #[test]
    fn test_dummy_output() {
        let output = DummyMidiOutput;
        output.note_on(0, 60, 100);
        output.note_off(0, 60);
        assert!(!output.is_connected());
    }
}
