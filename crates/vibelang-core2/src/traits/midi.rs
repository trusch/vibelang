//! MIDI trait for external MIDI device control.
//!
//! This module is only available with the `midi` feature.

#![cfg(feature = "midi")]

use crate::traits::FadeTarget;
use crate::types::ids::MidiDeviceId;
use crate::types::VoiceId;
use crate::Result;
use async_trait::async_trait;

/// Information about a MIDI device.
#[derive(Clone, Debug)]
pub struct MidiDeviceInfo {
    /// Device ID.
    pub id: MidiDeviceId,

    /// Device name.
    pub name: String,

    /// Whether the device has input capability.
    pub has_input: bool,

    /// Whether the device has output capability.
    pub has_output: bool,
}

/// MIDI device management and routing.
///
/// Provides control over external MIDI devices for input (keyboard controllers)
/// and output (hardware synths, drum machines).
#[async_trait]
pub trait Midi: Send + Sync {
    /// List available MIDI devices.
    fn list_devices(&self) -> Vec<MidiDeviceInfo>;

    /// Open a MIDI input device.
    async fn open_input(&self, id: MidiDeviceId) -> Result<()>;

    /// Open a MIDI output device.
    async fn open_output(&self, id: MidiDeviceId) -> Result<()>;

    /// Close a MIDI device.
    async fn close(&self, id: MidiDeviceId) -> Result<()>;

    // =========================================================================
    // Output
    // =========================================================================

    /// Send a MIDI note-on.
    async fn send_note_on(
        &self,
        device: MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
    ) -> Result<()>;

    /// Send a MIDI note-off.
    async fn send_note_off(&self, device: MidiDeviceId, channel: u8, note: u8) -> Result<()>;

    /// Send a MIDI control change.
    async fn send_cc(&self, device: MidiDeviceId, channel: u8, cc: u8, value: u8) -> Result<()>;

    // =========================================================================
    // Routing
    // =========================================================================

    /// Route a MIDI keyboard to a voice.
    ///
    /// Incoming notes from the device will trigger the voice.
    async fn route_keyboard(&self, device: MidiDeviceId, voice: VoiceId) -> Result<()>;

    /// Route a MIDI CC to a parameter.
    ///
    /// Incoming CC values will control the specified parameter.
    async fn route_cc(
        &self,
        device: MidiDeviceId,
        cc: u8,
        target: FadeTarget,
        param: &str,
    ) -> Result<()>;
}
