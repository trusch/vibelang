//! MIDI trait for external MIDI device control.
//!
//! This module is only available with the `midi` feature.

use crate::midi::{MidiRecording, MidiRecordingInfo};
use crate::traits::FadeTarget;
use crate::types::ids::MidiDeviceId;
use crate::types::VoiceId;
use crate::Result;
use async_trait::async_trait;

/// MIDI output capability of a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MidiOutputCapability {
    /// Device only supports MIDI 1.0.
    #[default]
    Midi1Only,
    /// Device supports MIDI 2.0 (UMP format) natively.
    Midi2Native,
    /// Device supports MIDI 2.0 via translation from MIDI 1.0.
    Midi2ViaTranslation,
}

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

    /// MIDI 2.0 capability of the device.
    pub midi2_capability: MidiOutputCapability,
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

    // =========================================================================
    // Recording
    // =========================================================================

    /// Start recording MIDI input from a device.
    ///
    /// Records all incoming note and CC events with beat timestamps.
    /// Returns an error if already recording from this device.
    async fn start_recording(&self, device: MidiDeviceId) -> Result<()>;

    /// Start recording MIDI input with a channel filter.
    ///
    /// Only events on the specified channel (0-15) will be recorded.
    async fn start_recording_channel(&self, device: MidiDeviceId, channel: u8) -> Result<()>;

    /// Stop recording from a device and return the recording.
    ///
    /// Returns an error if not currently recording from this device.
    async fn stop_recording(&self, device: MidiDeviceId) -> Result<MidiRecording>;

    /// Check if currently recording from a device.
    async fn is_recording(&self, device: MidiDeviceId) -> bool;

    /// Get information about an active recording.
    async fn recording_info(&self, device: MidiDeviceId) -> Option<MidiRecordingInfo>;

    // =========================================================================
    // Clock Output
    // =========================================================================

    /// Enable MIDI clock output to a device.
    ///
    /// When enabled, MIDI clock messages (24 PPQN) will be sent to the device
    /// synchronized with the transport.
    async fn enable_clock_output(&self, device: MidiDeviceId) -> Result<()>;

    /// Disable MIDI clock output to a device.
    async fn disable_clock_output(&self, device: MidiDeviceId) -> Result<()>;

    /// Check if MIDI clock output is enabled for a device.
    async fn is_clock_output_enabled(&self, device: MidiDeviceId) -> bool;

    /// Send MIDI start message to a device.
    async fn send_start(&self, device: MidiDeviceId) -> Result<()>;

    /// Send MIDI stop message to a device.
    async fn send_stop(&self, device: MidiDeviceId) -> Result<()>;

    /// Send MIDI continue message to a device.
    async fn send_continue(&self, device: MidiDeviceId) -> Result<()>;

    /// Update tempo for all MIDI clock outputs.
    async fn update_clock_tempo(&self, bpm: f64) -> Result<()>;

    /// Send MIDI start to all devices with clock output enabled.
    async fn send_start_to_all_clock_devices(&self) -> Result<()>;

    /// Send MIDI stop to all devices with clock output enabled.
    async fn send_stop_to_all_clock_devices(&self) -> Result<()>;

    /// Send a MIDI pitch bend message.
    async fn send_pitch_bend(&self, device: MidiDeviceId, channel: u8, value: i16) -> Result<()>;

    // =========================================================================
    // MIDI 2.0 Output
    // =========================================================================

    /// Get the MIDI 2.0 capability of a device.
    fn midi2_capability(&self, device: MidiDeviceId) -> MidiOutputCapability;

    /// Send a MIDI 2.0 note on.
    async fn send_midi2_note_on(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
    ) -> Result<()>;

    /// Send a MIDI 2.0 note on with attribute.
    async fn send_midi2_note_on_with_attribute(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
        attribute_type: u8,
        attribute_value: u16,
    ) -> Result<()>;

    /// Send a MIDI 2.0 note off.
    async fn send_midi2_note_off(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
    ) -> Result<()>;

    /// Send a MIDI 2.0 control change (32-bit value).
    async fn send_midi2_cc(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        controller: u8,
        value: u32,
    ) -> Result<()>;

    /// Send a MIDI 2.0 pitch bend (32-bit value).
    async fn send_midi2_pitch_bend(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        value: u32,
    ) -> Result<()>;

    /// Send a per-note pitch bend (MIDI 2.0 only).
    async fn send_per_note_pitch_bend(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        value: u32,
    ) -> Result<()>;

    /// Send a per-note controller (MIDI 2.0 only).
    async fn send_per_note_controller(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        controller: u8,
        value: u32,
    ) -> Result<()>;

    /// Send MIDI 2.0 poly pressure.
    async fn send_midi2_poly_pressure(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        pressure: u32,
    ) -> Result<()>;

    /// Send MIDI 2.0 channel pressure.
    async fn send_midi2_channel_pressure(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        pressure: u32,
    ) -> Result<()>;

    /// Send MIDI 2.0 program change with optional bank select.
    async fn send_midi2_program_change(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        program: u8,
        bank: Option<(u8, u8)>,
    ) -> Result<()>;
}
