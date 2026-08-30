//! Midi trait implementation for MidiHandler.

use super::output::{detect_midi2_capability, send_midi2_event_to_device};
use super::types::convert_new_to_legacy_message;
use super::MidiHandler;
use crate::backend::Backend;
#[cfg(feature = "pipewire-midi2")]
use crate::midi::open_pipewire_midi2_input;
use crate::midi::{
    is_pipewire_midi_input_id, list_pipewire_midi2_inputs,
    parse_midi_bytes as new_parse_midi_bytes, send_panic_clear, MidiRecording, MidiRecordingInfo,
    QueuedMidiEvent, TimestampedMidiEvent,
};
use crate::traits::{Midi, MidiDeviceInfo, MidiOutputCapability};
use crate::types::ids::MidiDeviceId;
use crate::types::VoiceId;
use crate::{Error, Result};
use async_trait::async_trait;
use midir::{MidiInput, MidiOutput};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

impl<B: Backend> MidiHandler<B> {
    pub(super) async fn open_legacy_input_as(
        &self,
        port_id: MidiDeviceId,
        logical_id: MidiDeviceId,
    ) -> Result<()> {
        {
            let inputs = self
                .inputs
                .lock()
                .map_err(|e| Error::MidiError(format!("MIDI inputs lock poisoned: {}", e)))?;
            if inputs.contains_key(&logical_id) {
                tracing::trace!("MIDI input {} already open", logical_id.0);
                return Ok(());
            }
        }

        let midi_in = MidiInput::new("vibelang-core2-in")
            .map_err(|e| Error::MidiError(format!("Failed to create MIDI input: {}", e)))?;
        let ports = midi_in.ports();
        let port = ports
            .get(port_id.0 as usize)
            .ok_or_else(|| Error::MidiError(format!("MIDI device {} not found", port_id.0)))?;
        let port_name = midi_in
            .port_name(port)
            .unwrap_or_else(|_| format!("Unknown {}", port_id.0));

        let tx = self.tx.clone();
        let event_sender = self.event_queue.sender();
        let midi_clock = Arc::clone(&self.midi_clock);
        let callback_id = logical_id;
        let input_drops = Arc::clone(&self.input_callback_drops);

        let conn = midi_in
            .connect(
                port,
                "vibelang-input",
                move |timestamp_us, data, _| {
                    let received_at = std::time::Instant::now();
                    midi_clock.calibrate(timestamp_us);

                    if let Some(new_msg) = new_parse_midi_bytes(data) {
                        let timestamped_event = TimestampedMidiEvent {
                            timestamp_us,
                            received_at,
                            device_id: callback_id,
                            message: new_msg.clone(),
                        };

                        if !event_sender.try_send(timestamped_event) {
                            let fell_back = convert_new_to_legacy_message(&new_msg)
                                .map(|legacy_msg| tx.try_send((callback_id, legacy_msg)).is_ok())
                                .unwrap_or(false);
                            if !fell_back {
                                input_drops.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                },
                (),
            )
            .map_err(|e| Error::MidiError(format!("Failed to connect MIDI input: {}", e)))?;

        self.inputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI inputs lock poisoned: {}", e)))?
            .insert(logical_id, conn);
        tracing::info!(
            "Opened MIDI input: {} (port={}, logical={}) with timestamp preservation",
            port_name,
            port_id.0,
            logical_id.0
        );
        Ok(())
    }

    pub(super) fn close_legacy_input(&self, logical_id: MidiDeviceId) {
        match self.inputs.lock() {
            Ok(mut inputs) => {
                if inputs.remove(&logical_id).is_some() {
                    tracing::info!("Closed MIDI input: logical={}", logical_id.0);
                }
            }
            Err(error) => tracing::error!("MIDI inputs lock poisoned: {}", error),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Midi for MidiHandler<B> {
    fn list_devices(&self) -> Vec<MidiDeviceInfo> {
        let mut devices = Vec::new();
        let mut seen_names = HashMap::new();

        // List input devices
        if let Ok(midi_in) = MidiInput::new("vibelang-core2-list") {
            for (idx, port) in midi_in.ports().iter().enumerate() {
                if let Ok(name) = midi_in.port_name(port) {
                    let id = MidiDeviceId::new(idx as u32);
                    devices.push(MidiDeviceInfo {
                        id,
                        name: name.clone(),
                        has_input: true,
                        has_output: false,
                        midi2_capability: MidiOutputCapability::Midi1Only,
                    });
                    seen_names.insert(name, idx);
                }
            }
        }

        // List output devices
        if let Ok(midi_out) = MidiOutput::new("vibelang-core2-list") {
            for port in midi_out.ports().iter() {
                if let Ok(name) = midi_out.port_name(port) {
                    if let Some(&existing_idx) = seen_names.get(&name) {
                        devices[existing_idx].has_output = true;
                        devices[existing_idx].midi2_capability = detect_midi2_capability(&name);
                    } else {
                        let id = MidiDeviceId::new((devices.len()) as u32);
                        devices.push(MidiDeviceInfo {
                            id,
                            name: name.clone(),
                            has_input: false,
                            has_output: true,
                            midi2_capability: detect_midi2_capability(&name),
                        });
                    }
                }
            }
        }

        for input in list_pipewire_midi2_inputs() {
            devices.push(MidiDeviceInfo {
                id: input.id,
                name: input.name,
                has_input: true,
                has_output: false,
                midi2_capability: MidiOutputCapability::Midi2Native,
            });
        }

        for input in crate::midi::list_alsa_ump_inputs() {
            devices.push(MidiDeviceInfo {
                id: input.id,
                name: input.name,
                has_input: true,
                has_output: false,
                midi2_capability: MidiOutputCapability::Midi2Native,
            });
        }

        devices
    }

    async fn open_input(&self, id: MidiDeviceId) -> Result<()> {
        #[cfg(target_os = "linux")]
        if crate::midi::is_alsa_ump_input_id(id) {
            let stale = {
                let mut inputs = self.alsa_ump_inputs.lock().map_err(|e| {
                    Error::MidiError(format!("ALSA UMP inputs lock poisoned: {}", e))
                })?;
                if inputs
                    .get(&id)
                    .map(|input| input.is_alive())
                    .unwrap_or(false)
                {
                    tracing::trace!("ALSA UMP input {:?} already open", id);
                    return Ok(());
                }
                inputs.remove(&id)
            };
            drop(stale);

            let conn = crate::midi::open_alsa_ump_input(
                id,
                self.event_queue.sender(),
                Arc::clone(&self.midi_clock),
            )
            .map_err(Error::MidiError)?;

            self.alsa_ump_inputs
                .lock()
                .map_err(|e| Error::MidiError(format!("ALSA UMP inputs lock poisoned: {}", e)))?
                .insert(id, conn);
            tracing::info!("Opened ALSA UMP input {:?}", id);
            return Ok(());
        }

        #[cfg(feature = "pipewire-midi2")]
        if is_pipewire_midi_input_id(id) {
            {
                let inputs = self.pipewire_inputs.lock().map_err(|e| {
                    Error::MidiError(format!("PipeWire MIDI inputs lock poisoned: {}", e))
                })?;
                if inputs.contains_key(&id) {
                    tracing::trace!("PipeWire MIDI2 input {:?} already open", id);
                    return Ok(());
                }
            }

            let conn = open_pipewire_midi2_input(
                id,
                self.event_queue.sender(),
                Arc::clone(&self.midi_clock),
            )
            .map_err(Error::MidiError)?;

            self.pipewire_inputs
                .lock()
                .map_err(|e| {
                    Error::MidiError(format!("PipeWire MIDI inputs lock poisoned: {}", e))
                })?
                .insert(id, conn);
            tracing::info!("Opened PipeWire MIDI2 input {:?}", id);
            return Ok(());
        }

        self.open_legacy_input_as(id, id).await
    }

    async fn open_output(&self, id: MidiDeviceId) -> Result<()> {
        if is_pipewire_midi_input_id(id) {
            return Err(Error::MidiError(format!(
                "PipeWire MIDI2 input {:?} cannot be opened for output",
                id
            )));
        }

        // Check if already open (idempotent operation)
        {
            let outputs = self
                .outputs
                .lock()
                .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
            if outputs.contains_key(&id) {
                tracing::trace!("MIDI output {} already open", id.0);
                return Ok(());
            }
        }

        // Auto-start the realtime service on first output open
        // This enables sample-accurate MIDI output via SuperCollider synthdefs
        if !self.is_realtime_service_running() {
            if let Err(e) = self.start_realtime_service(None) {
                tracing::warn!(
                    "Could not start MIDI realtime service: {} (direct output still available)",
                    e
                );
            }
        }

        let midi_out = MidiOutput::new("vibelang-core2-out")
            .map_err(|e| Error::MidiError(format!("Failed to create MIDI output: {}", e)))?;

        let ports = midi_out.ports();
        let port = ports
            .get(id.0 as usize)
            .ok_or_else(|| Error::MidiError(format!("MIDI device {} not found", id.0)))?;

        let port_name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| format!("Unknown {}", id.0));

        let mut conn = midi_out
            .connect(port, "vibelang-output")
            .map_err(|e| Error::MidiError(format!("Failed to connect MIDI output: {}", e)))?;

        send_panic_clear(|message| {
            let _ = conn.send(message);
        });

        self.outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?
            .insert(id, conn);
        tracing::info!("Opened MIDI output: {} (id={})", port_name, id.0);

        // Also create the output channel for async/voice-based sending
        // This is used by VoicesHandler.note_on() for MIDI voices
        if let Err(e) = self.create_output_channel(id) {
            tracing::warn!(
                "Failed to create output channel for device {}: {} (direct output still available)",
                id.0,
                e
            );
        }

        Ok(())
    }

    async fn close(&self, id: MidiDeviceId) -> Result<()> {
        let mut removed = false;

        #[cfg(target_os = "linux")]
        let alsa_ump_input = self
            .alsa_ump_inputs
            .lock()
            .map_err(|e| Error::MidiError(format!("ALSA UMP inputs lock poisoned: {}", e)))?
            .remove(&id);
        #[cfg(target_os = "linux")]
        if alsa_ump_input.is_some() {
            drop(alsa_ump_input);
            tracing::info!("Closed ALSA UMP input: id={}", id.0);
            removed = true;
        }

        #[cfg(feature = "pipewire-midi2")]
        if self
            .pipewire_inputs
            .lock()
            .map_err(|e| Error::MidiError(format!("PipeWire MIDI inputs lock poisoned: {}", e)))?
            .remove(&id)
            .is_some()
        {
            tracing::info!("Closed PipeWire MIDI2 input: id={}", id.0);
            removed = true;
        }

        if self
            .inputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI inputs lock poisoned: {}", e)))?
            .remove(&id)
            .is_some()
        {
            tracing::info!("Closed MIDI input: id={}", id.0);
            removed = true;
        }

        if self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?
            .remove(&id)
            .is_some()
        {
            tracing::info!("Closed MIDI output: id={}", id.0);
            removed = true;
        }

        if removed {
            Ok(())
        } else {
            Err(Error::MidiError(format!("MIDI device {} not open", id.0)))
        }
    }

    async fn send_note_on(
        &self,
        device: MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
    ) -> Result<()> {
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        let msg = [0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F];
        conn.send(&msg)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI: {}", e)))?;

        Ok(())
    }

    async fn send_note_off(&self, device: MidiDeviceId, channel: u8, note: u8) -> Result<()> {
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        let msg = [0x80 | (channel & 0x0F), note & 0x7F, 0];
        conn.send(&msg)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI: {}", e)))?;

        Ok(())
    }

    async fn send_cc(&self, device: MidiDeviceId, channel: u8, cc: u8, value: u8) -> Result<()> {
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        let msg = [0xB0 | (channel & 0x0F), cc & 0x7F, value & 0x7F];
        conn.send(&msg)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI: {}", e)))?;

        Ok(())
    }

    async fn route_keyboard(&self, device: MidiDeviceId, voice: VoiceId) -> Result<()> {
        self.routing_manager
            .add_basic_keyboard_route(device, voice)
            .await;
        Ok(())
    }

    async fn route_cc(
        &self,
        device: MidiDeviceId,
        cc: u8,
        target: crate::traits::FadeTarget,
        param: &str,
    ) -> Result<()> {
        self.routing_manager
            .add_basic_cc_route(super::types::CcRoute {
                device_id: device,
                cc,
                target: target.clone(),
                param: param.to_string(),
                min_value: 0.0,
                max_value: 1.0,
            })
            .await;

        tracing::info!(
            "Routed MIDI CC {} from device {} to {:?}.{}",
            cc,
            device.0,
            target,
            param
        );

        Ok(())
    }

    // =========================================================================
    // Recording
    // =========================================================================

    async fn start_recording(&self, device: MidiDeviceId) -> Result<()> {
        let current_beat = {
            let state = self.state.read().await;
            state.current_beat
        };

        let recordings_arc = self.recording_manager.recordings();
        let mut recordings = recordings_arc.write().await;
        if recordings.contains_key(&device) {
            return Err(Error::MidiError(format!(
                "Already recording from MIDI device {}",
                device.0
            )));
        }

        let recording = MidiRecording::new(device, current_beat);
        recordings.insert(device, recording);

        tracing::info!(
            "Started MIDI recording from device {} at beat {}",
            device.0,
            current_beat.to_f64()
        );

        Ok(())
    }

    async fn start_recording_channel(&self, device: MidiDeviceId, channel: u8) -> Result<()> {
        let current_beat = {
            let state = self.state.read().await;
            state.current_beat
        };

        let recordings_arc = self.recording_manager.recordings();
        let mut recordings = recordings_arc.write().await;
        if recordings.contains_key(&device) {
            return Err(Error::MidiError(format!(
                "Already recording from MIDI device {}",
                device.0
            )));
        }

        let recording = MidiRecording::new(device, current_beat).with_channel_filter(channel);
        recordings.insert(device, recording);

        tracing::info!(
            "Started MIDI recording from device {} channel {} at beat {}",
            device.0,
            channel,
            current_beat.to_f64()
        );

        Ok(())
    }

    async fn stop_recording(&self, device: MidiDeviceId) -> Result<MidiRecording> {
        let current_beat = {
            let state = self.state.read().await;
            state.current_beat
        };

        let recordings_arc = self.recording_manager.recordings();
        let mut recordings = recordings_arc.write().await;
        let mut recording = recordings.remove(&device).ok_or_else(|| {
            Error::MidiError(format!("Not recording from MIDI device {}", device.0))
        })?;

        recording.stop(current_beat);

        tracing::info!(
            "Stopped MIDI recording from device {}: {} notes, {} CCs, {} beats",
            device.0,
            recording.note_count(),
            recording.cc_count(),
            recording.duration().to_f64()
        );

        Ok(recording)
    }

    async fn is_recording(&self, device: MidiDeviceId) -> bool {
        let recordings_arc = self.recording_manager.recordings();
        let recordings = recordings_arc.read().await;
        recordings
            .get(&device)
            .map(|r| r.is_recording)
            .unwrap_or(false)
    }

    async fn recording_info(&self, device: MidiDeviceId) -> Option<MidiRecordingInfo> {
        let recordings_arc = self.recording_manager.recordings();
        let recordings = recordings_arc.read().await;
        recordings.get(&device).map(MidiRecordingInfo::from)
    }

    // =========================================================================
    // Clock Output
    // =========================================================================

    async fn enable_clock_output(&self, device: MidiDeviceId) -> Result<()> {
        // Register with clock manager (for backward compat and direct sending)
        self.clock_manager
            .enable_clock_output(device, &self.outputs, &self.output_manager.output_channels)
            .await?;

        // Also enable on clock thread if running (handles threaded clock output)
        #[cfg(not(target_arch = "wasm32"))]
        self.enable_clock_output_threaded(device);

        Ok(())
    }

    async fn disable_clock_output(&self, device: MidiDeviceId) -> Result<()> {
        // Disable on clock thread if running
        #[cfg(not(target_arch = "wasm32"))]
        self.disable_clock_output_threaded(device);

        // Remove from clock manager
        self.clock_manager.disable_clock_output(device).await
    }

    async fn is_clock_output_enabled(&self, device: MidiDeviceId) -> bool {
        self.clock_manager.is_clock_output_enabled(device).await
    }

    async fn update_clock_tempo(&self, _bpm: f64) -> Result<()> {
        // Clock tempo is now implicit in the beat position, no action needed
        Ok(())
    }

    async fn send_start_to_all_clock_devices(&self) -> Result<()> {
        self.clock_manager
            .send_start_to_all_clock_devices(&self.output_manager.output_channels, &self.outputs)
            .await
    }

    async fn send_stop_to_all_clock_devices(&self) -> Result<()> {
        self.clock_manager
            .send_stop_to_all_clock_devices(&self.output_manager.output_channels, &self.outputs)
            .await
    }

    async fn send_start(&self, device: MidiDeviceId) -> Result<()> {
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        conn.send(&[0xFA])
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI Start: {}", e)))?;

        tracing::debug!("Sent MIDI Start to device {}", device.0);

        Ok(())
    }

    async fn send_stop(&self, device: MidiDeviceId) -> Result<()> {
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        conn.send(&[0xFC])
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI Stop: {}", e)))?;

        tracing::debug!("Sent MIDI Stop to device {}", device.0);

        Ok(())
    }

    async fn send_continue(&self, device: MidiDeviceId) -> Result<()> {
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        conn.send(&[0xFB])
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI Continue: {}", e)))?;

        tracing::debug!("Sent MIDI Continue to device {}", device.0);

        Ok(())
    }

    // =========================================================================
    // MIDI 1.0 Additional Methods
    // =========================================================================

    async fn send_pitch_bend(&self, device: MidiDeviceId, channel: u8, value: i16) -> Result<()> {
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        let unsigned = (value + 8192) as u16;
        let lsb = (unsigned & 0x7F) as u8;
        let msb = ((unsigned >> 7) & 0x7F) as u8;

        let msg = [0xE0 | (channel & 0x0F), lsb, msb];
        conn.send(&msg)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI pitch bend: {}", e)))?;

        Ok(())
    }

    // =========================================================================
    // MIDI 2.0 Output Methods
    // =========================================================================

    fn midi2_capability(&self, device: MidiDeviceId) -> MidiOutputCapability {
        // Use cached capability to avoid re-enumerating devices on every call
        self.get_device_capability(device)
    }

    async fn send_midi2_note_on(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
    ) -> Result<()> {
        self.send_midi2_note_on_with_attribute(device, group, channel, note, velocity, 0, 0)
            .await
    }

    async fn send_midi2_note_on_with_attribute(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
        attribute_type: u8,
        attribute_value: u16,
    ) -> Result<()> {
        let event = QueuedMidiEvent::Midi2NoteOn {
            group: group & 0x0F,
            channel: channel & 0x0F,
            note: note & 0x7F,
            velocity,
            attribute_type,
            attribute_value,
        };

        let capability = self.midi2_capability(device);
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        send_midi2_event_to_device(conn, &event, capability)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI 2.0 note on: {}", e)))?;

        tracing::trace!(
            "Sent MIDI 2.0 note on: device={}, g={}, ch={}, note={}, vel={}",
            device.0,
            group,
            channel,
            note,
            velocity
        );

        Ok(())
    }

    async fn send_midi2_note_off(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
    ) -> Result<()> {
        let event = QueuedMidiEvent::Midi2NoteOff {
            group: group & 0x0F,
            channel: channel & 0x0F,
            note: note & 0x7F,
            velocity,
            attribute_type: 0,
            attribute_value: 0,
        };

        let capability = self.midi2_capability(device);
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        send_midi2_event_to_device(conn, &event, capability)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI 2.0 note off: {}", e)))?;

        Ok(())
    }

    async fn send_midi2_cc(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        controller: u8,
        value: u32,
    ) -> Result<()> {
        let event = QueuedMidiEvent::Midi2ControlChange {
            group: group & 0x0F,
            channel: channel & 0x0F,
            controller: controller & 0x7F,
            value,
        };

        let capability = self.midi2_capability(device);
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        send_midi2_event_to_device(conn, &event, capability)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI 2.0 CC: {}", e)))?;

        tracing::trace!(
            "Sent MIDI 2.0 CC: device={}, g={}, ch={}, cc={}, val={}",
            device.0,
            group,
            channel,
            controller,
            value
        );

        Ok(())
    }

    async fn send_midi2_pitch_bend(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        value: u32,
    ) -> Result<()> {
        let event = QueuedMidiEvent::Midi2PitchBend {
            group: group & 0x0F,
            channel: channel & 0x0F,
            value,
        };

        let capability = self.midi2_capability(device);
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        send_midi2_event_to_device(conn, &event, capability)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI 2.0 pitch bend: {}", e)))?;

        Ok(())
    }

    async fn send_per_note_pitch_bend(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        value: u32,
    ) -> Result<()> {
        let capability = self.midi2_capability(device);

        if capability == MidiOutputCapability::Midi1Only {
            tracing::debug!(
                "Skipping per-note pitch bend to MIDI 1.0 device {}",
                device.0
            );
            return Ok(());
        }

        let event = QueuedMidiEvent::Midi2PerNotePitchBend {
            group: group & 0x0F,
            channel: channel & 0x0F,
            note: note & 0x7F,
            value,
        };

        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        send_midi2_event_to_device(conn, &event, capability)
            .map_err(|e| Error::MidiError(format!("Failed to send per-note pitch bend: {}", e)))?;

        tracing::trace!(
            "Sent per-note pitch bend: device={}, note={}, val={}",
            device.0,
            note,
            value
        );

        Ok(())
    }

    async fn send_per_note_controller(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        controller: u8,
        value: u32,
    ) -> Result<()> {
        let capability = self.midi2_capability(device);

        if capability == MidiOutputCapability::Midi1Only {
            tracing::debug!(
                "Skipping per-note controller to MIDI 1.0 device {}",
                device.0
            );
            return Ok(());
        }

        let event = QueuedMidiEvent::Midi2PerNoteController {
            group: group & 0x0F,
            channel: channel & 0x0F,
            note: note & 0x7F,
            controller,
            value,
        };

        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        send_midi2_event_to_device(conn, &event, capability)
            .map_err(|e| Error::MidiError(format!("Failed to send per-note controller: {}", e)))?;

        Ok(())
    }

    async fn send_midi2_poly_pressure(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        pressure: u32,
    ) -> Result<()> {
        let event = QueuedMidiEvent::Midi2PolyPressure {
            group: group & 0x0F,
            channel: channel & 0x0F,
            note: note & 0x7F,
            pressure,
        };

        let capability = self.midi2_capability(device);
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        send_midi2_event_to_device(conn, &event, capability).map_err(|e| {
            Error::MidiError(format!("Failed to send MIDI 2.0 poly pressure: {}", e))
        })?;

        Ok(())
    }

    async fn send_midi2_channel_pressure(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        pressure: u32,
    ) -> Result<()> {
        let event = QueuedMidiEvent::Midi2ChannelPressure {
            group: group & 0x0F,
            channel: channel & 0x0F,
            pressure,
        };

        let capability = self.midi2_capability(device);
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        send_midi2_event_to_device(conn, &event, capability).map_err(|e| {
            Error::MidiError(format!("Failed to send MIDI 2.0 channel pressure: {}", e))
        })?;

        Ok(())
    }

    async fn send_midi2_program_change(
        &self,
        device: MidiDeviceId,
        group: u8,
        channel: u8,
        program: u8,
        bank: Option<(u8, u8)>,
    ) -> Result<()> {
        let (bank_valid, bank_msb, bank_lsb) = match bank {
            Some((msb, lsb)) => (true, msb, lsb),
            None => (false, 0, 0),
        };

        let event = QueuedMidiEvent::Midi2ProgramChange {
            group: group & 0x0F,
            channel: channel & 0x0F,
            program: program & 0x7F,
            bank_valid,
            bank_msb,
            bank_lsb,
        };

        let capability = self.midi2_capability(device);
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|e| Error::MidiError(format!("MIDI outputs lock poisoned: {}", e)))?;
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        send_midi2_event_to_device(conn, &event, capability).map_err(|e| {
            Error::MidiError(format!("Failed to send MIDI 2.0 program change: {}", e))
        })?;

        Ok(())
    }
}
