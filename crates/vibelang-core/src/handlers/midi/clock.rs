//! MIDI clock output management.
//!
//! This module handles sending MIDI clock signals (24 PPQN) to external
//! devices for synchronization. Clock signals are sent directly from the
//! Rust runtime tick loop, synchronized with the transport beat position.

use crate::compat::RwLock;
use crate::midi::{QueuedMidiEvent, ScheduledMidiEvent};
use crate::types::ids::MidiDeviceId;
use crossbeam_channel::Sender;
use midir::MidiOutputConnection;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Manager for MIDI clock output.
///
/// Sends 24 pulses per quarter note (PPQN) directly from the runtime tick loop.
/// This provides tight synchronization with the transport without going through
/// SuperCollider.
pub struct MidiClockManager {
    /// Devices with clock output enabled.
    clock_output_devices: Arc<RwLock<HashSet<MidiDeviceId>>>,

    /// Last beat position at which we sent a MIDI clock tick.
    /// Used to calculate how many clock ticks to send on each tick.
    last_clock_beat: Arc<RwLock<f64>>,
}

impl Default for MidiClockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiClockManager {
    /// Create a new clock manager.
    pub fn new() -> Self {
        Self {
            clock_output_devices: Arc::new(RwLock::new(HashSet::new())),
            last_clock_beat: Arc::new(RwLock::new(0.0)),
        }
    }

    /// Get the clock output devices set.
    #[allow(dead_code)]
    pub fn clock_output_devices(&self) -> Arc<RwLock<HashSet<MidiDeviceId>>> {
        Arc::clone(&self.clock_output_devices)
    }

    /// Send MIDI clock tick to all devices with clock output enabled.
    pub async fn send_clock_tick(
        &self,
        outputs: &Mutex<HashMap<MidiDeviceId, MidiOutputConnection>>,
    ) -> crate::Result<()> {
        let clock_devices = self.clock_output_devices.read().await;
        if clock_devices.is_empty() {
            return Ok(());
        }

        let mut outputs = outputs
            .lock()
            .map_err(|e| crate::Error::MidiError(format!("Failed to lock MIDI outputs: {e}")))?;
        for device_id in clock_devices.iter() {
            if let Some(conn) = outputs.get_mut(device_id) {
                // MIDI Clock message is 0xF8
                if let Err(e) = conn.send(&[0xF8]) {
                    tracing::warn!("Failed to send MIDI clock to device {}: {}", device_id.0, e);
                }
            }
        }

        Ok(())
    }

    /// Tick MIDI clock output based on current beat position.
    ///
    /// Sends the appropriate number of clock ticks (24 PPQN) based on
    /// the beat position change since the last tick.
    pub async fn tick_clock(
        &self,
        current_beat: f64,
        is_playing: bool,
        outputs: &Mutex<HashMap<MidiDeviceId, MidiOutputConnection>>,
    ) -> crate::Result<()> {
        let clock_devices = self.clock_output_devices.read().await;
        if clock_devices.is_empty() {
            return Ok(());
        }

        // Only send clock when transport is playing
        if !is_playing {
            // Reset clock position when stopped
            let mut last_beat = self.last_clock_beat.write().await;
            *last_beat = current_beat;
            return Ok(());
        }

        let mut last_beat = self.last_clock_beat.write().await;

        // Calculate how many clock ticks to send
        // 24 PPQN = 24 pulses per quarter note = 24 pulses per beat
        const PPQN: f64 = 24.0;

        // Calculate ticks since last position
        let beat_diff = current_beat - *last_beat;

        // Handle negative beat diff (e.g., seek backwards)
        if beat_diff < 0.0 {
            *last_beat = current_beat;
            return Ok(());
        }

        let ticks_to_send = (beat_diff * PPQN).floor() as u32;

        if ticks_to_send > 0 {
            let mut outputs = outputs.lock().map_err(|e| {
                crate::Error::MidiError(format!("Failed to lock MIDI outputs: {e}"))
            })?;

            for device_id in clock_devices.iter() {
                if let Some(conn) = outputs.get_mut(device_id) {
                    for _ in 0..ticks_to_send {
                        // MIDI Clock message is 0xF8
                        if let Err(e) = conn.send(&[0xF8]) {
                            tracing::warn!(
                                "Failed to send MIDI clock to device {}: {}",
                                device_id.0,
                                e
                            );
                        }
                    }
                }
            }

            // Update last beat position based on ticks sent
            *last_beat += (ticks_to_send as f64) / PPQN;
        }

        Ok(())
    }

    /// Reset the clock output position (e.g., when transport seeks).
    pub async fn reset_clock_position(&self, beat: f64) {
        let mut last_beat = self.last_clock_beat.write().await;
        *last_beat = beat;
    }

    /// Enable clock output for a device.
    ///
    /// Clock signals will be sent from the runtime tick loop via `tick_clock()`.
    pub async fn enable_clock_output(
        &self,
        device: MidiDeviceId,
        outputs: &Mutex<HashMap<MidiDeviceId, MidiOutputConnection>>,
        output_channels: &Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>>,
    ) -> crate::Result<()> {
        // Ensure output device is open (or has an output channel)
        {
            let outputs = outputs.lock().map_err(|e| {
                crate::Error::MidiError(format!("Failed to lock MIDI outputs: {e}"))
            })?;
            let channels = output_channels.lock().map_err(|e| {
                crate::Error::MidiError(format!("Failed to lock MIDI output channels: {e}"))
            })?;
            if !outputs.contains_key(&device) && !channels.contains_key(&device) {
                return Err(crate::Error::MidiError(format!(
                    "MIDI output device {} not open",
                    device.0
                )));
            }
        }

        // Check if clock is already enabled for this device
        {
            let clock_devices = self.clock_output_devices.read().await;
            if clock_devices.contains(&device) {
                tracing::debug!("Clock output already enabled for device {}", device.0);
                return Ok(());
            }
        }

        // Add to clock devices set - tick_clock() will handle the actual clock sending
        self.clock_output_devices.write().await.insert(device);

        tracing::info!("Enabled MIDI clock output to device {}", device.0);

        Ok(())
    }

    /// Disable clock output for a device.
    pub async fn disable_clock_output(&self, device: MidiDeviceId) -> crate::Result<()> {
        // Remove from clock devices set
        self.clock_output_devices.write().await.remove(&device);

        tracing::info!("Disabled MIDI clock output to device {}", device.0);

        Ok(())
    }

    /// Check if clock output is enabled for a device.
    pub async fn is_clock_output_enabled(&self, device: MidiDeviceId) -> bool {
        let clock_devices = self.clock_output_devices.read().await;
        clock_devices.contains(&device)
    }

    /// Send MIDI Start message to all devices with clock output enabled.
    pub async fn send_start_to_all_clock_devices(
        &self,
        output_channels: &Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>>,
        outputs: &Mutex<HashMap<MidiDeviceId, MidiOutputConnection>>,
    ) -> crate::Result<()> {
        let clock_devices = self.clock_output_devices.read().await;
        for device in clock_devices.iter() {
            // Try to send via output channel (for realtime service) or direct connection
            let sent_via_channel = if let Ok(channels) = output_channels.lock() {
                if let Some(sender) = channels.get(device) {
                    let _ = sender.try_send(QueuedMidiEvent::Start.immediate());
                    true
                } else {
                    false
                }
            } else {
                tracing::warn!("Failed to lock MIDI output channels for Start message");
                false
            };
            if !sent_via_channel {
                if let Ok(mut out) = outputs.lock() {
                    if let Some(conn) = out.get_mut(device) {
                        if let Err(e) = conn.send(&[0xFA]) {
                            tracing::warn!(
                                "Failed to send MIDI Start to device {}: {}",
                                device.0,
                                e
                            );
                        }
                    }
                } else {
                    tracing::warn!("Failed to lock MIDI outputs for Start message");
                }
            }
        }
        tracing::debug!("Sent MIDI Start to {} clock device(s)", clock_devices.len());
        Ok(())
    }

    /// Send MIDI Stop message to all devices with clock output enabled.
    pub async fn send_stop_to_all_clock_devices(
        &self,
        output_channels: &Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>>,
        outputs: &Mutex<HashMap<MidiDeviceId, MidiOutputConnection>>,
    ) -> crate::Result<()> {
        let clock_devices = self.clock_output_devices.read().await;
        for device in clock_devices.iter() {
            // Try to send via output channel (for realtime service) or direct connection
            let sent_via_channel = if let Ok(channels) = output_channels.lock() {
                if let Some(sender) = channels.get(device) {
                    let _ = sender.try_send(QueuedMidiEvent::Stop.immediate());
                    true
                } else {
                    false
                }
            } else {
                tracing::warn!("Failed to lock MIDI output channels for Stop message");
                false
            };
            if !sent_via_channel {
                if let Ok(mut out) = outputs.lock() {
                    if let Some(conn) = out.get_mut(device) {
                        if let Err(e) = conn.send(&[0xFC]) {
                            tracing::warn!(
                                "Failed to send MIDI Stop to device {}: {}",
                                device.0,
                                e
                            );
                        }
                    }
                } else {
                    tracing::warn!("Failed to lock MIDI outputs for Stop message");
                }
            }
        }
        tracing::debug!("Sent MIDI Stop to {} clock device(s)", clock_devices.len());
        Ok(())
    }
}
