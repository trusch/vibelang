//! MIDI output handlers.
//!
//! Handles MIDI output-related messages:
//! - Device management (open, close)
//! - Note/CC/Pitch Bend output
//! - Clock configuration

use super::RuntimeContext;
use crate::midi::{MidiOutputDeviceInfo, MidiOutputHandle, QueuedMidiEvent};
use crate::state::MidiOutputDeviceState;
use crossbeam_channel::Sender;

// =============================================================================
// Device Management
// =============================================================================

/// Handle MidiOutputOpenDevice message - open a MIDI output device.
pub fn handle_open_device(
    ctx: &mut RuntimeContext<'_>,
    device_id: u32,
    info: MidiOutputDeviceInfo,
    event_tx: Sender<QueuedMidiEvent>,
) {
    // Create a MidiOutputHandle for the realtime service
    let handle = MidiOutputHandle::new(device_id, info.clone(), event_tx.clone());

    // Register with the MIDI realtime service
    ctx.midi_rt.register_device_from_handle(&handle);

    ctx.shared.with_state_write(|state| {
        let device_state = MidiOutputDeviceState::new(device_id, info, event_tx);
        state.midi_output_config.devices.insert(device_id, device_state);
        state.bump_version();
    });
    log::info!(
        "[MIDI OUTPUT] Opened device {} (registered with realtime service)",
        device_id
    );
}

/// Handle MidiOutputCloseDevice message - close a MIDI output device.
pub fn handle_close_device(ctx: &mut RuntimeContext<'_>, device_id: u32) {
    // Unregister from MIDI realtime service
    ctx.midi_rt.unregister_device(device_id);

    ctx.shared.with_state_write(|state| {
        state.midi_output_config.devices.remove(&device_id);
        state.bump_version();
    });
    log::info!("[MIDI OUTPUT] Closed device {}", device_id);
}

/// Handle MidiOutputCloseAllDevices message - close all MIDI output devices.
pub fn handle_close_all_devices(ctx: &mut RuntimeContext<'_>) {
    ctx.shared.with_state_write(|state| {
        state.midi_output_config.devices.clear();
        state.bump_version();
    });
    log::info!("[MIDI OUTPUT] Closed all devices");
}

// =============================================================================
// MIDI Message Output
// =============================================================================

/// Handle MidiOutputNoteOn message - send a note on message.
pub fn handle_note_on(ctx: &RuntimeContext<'_>, device_id: u32, channel: u8, note: u8, velocity: u8) {
    if let Some(device) = ctx
        .shared
        .with_state_read(|state| state.midi_output_config.devices.get(&device_id).cloned())
    {
        let _ = device.event_tx.send(QueuedMidiEvent::note_on(channel, note, velocity));
    }
}

/// Handle MidiOutputNoteOff message - send a note off message.
pub fn handle_note_off(ctx: &RuntimeContext<'_>, device_id: u32, channel: u8, note: u8) {
    if let Some(device) = ctx
        .shared
        .with_state_read(|state| state.midi_output_config.devices.get(&device_id).cloned())
    {
        let _ = device.event_tx.send(QueuedMidiEvent::note_off(channel, note));
    }
}

/// Handle MidiOutputControlChange message - send a CC message.
pub fn handle_control_change(
    ctx: &RuntimeContext<'_>,
    device_id: u32,
    channel: u8,
    controller: u8,
    value: u8,
) {
    if let Some(device) = ctx
        .shared
        .with_state_read(|state| state.midi_output_config.devices.get(&device_id).cloned())
    {
        let _ = device
            .event_tx
            .send(QueuedMidiEvent::control_change(channel, controller, value));
    }
}

/// Handle MidiOutputPitchBend message - send a pitch bend message.
pub fn handle_pitch_bend(ctx: &RuntimeContext<'_>, device_id: u32, channel: u8, value: i16) {
    if let Some(device) = ctx
        .shared
        .with_state_read(|state| state.midi_output_config.devices.get(&device_id).cloned())
    {
        let _ = device.event_tx.send(QueuedMidiEvent::pitch_bend(channel, value));
    }
}

// =============================================================================
// Clock Configuration
// =============================================================================

/// Handle MidiOutputSetClockDevice message - set the clock device.
pub fn handle_set_clock_device(ctx: &mut RuntimeContext<'_>, device_id: Option<u32>) {
    ctx.shared.with_state_write(|state| {
        state.midi_output_config.clock_device_id = device_id;
        state.bump_version();
    });
}

/// Handle MidiOutputSendStart message - send MIDI start.
pub fn handle_send_start(ctx: &mut RuntimeContext<'_>) {
    ctx.send_midi_clock_message(QueuedMidiEvent::start());
}

/// Handle MidiOutputSendStop message - send MIDI stop.
pub fn handle_send_stop(ctx: &mut RuntimeContext<'_>) {
    ctx.send_midi_clock_message(QueuedMidiEvent::stop());
}

/// Handle MidiOutputSendContinue message - send MIDI continue.
pub fn handle_send_continue(ctx: &mut RuntimeContext<'_>) {
    ctx.send_midi_clock_message(QueuedMidiEvent::continue_msg());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::MidiBackend;
    use crate::midi_realtime::MidiRealtimeService;
    use crate::osc_sender::OscSender;
    use crate::reload::ReloadManager;
    use crate::scheduler::EventScheduler;
    use crate::scsynth::Scsynth;
    use crate::state::StateManager;
    use crate::timing::TransportClock;

    fn create_test_context() -> (
        OscSender,
        Scsynth,
        StateManager,
        EventScheduler,
        TransportClock,
        ReloadManager,
        Option<crossbeam_channel::Sender<String>>,
        Option<i32>,
        MidiRealtimeService,
    ) {
        let sc = Scsynth::noop();
        let osc_sender = OscSender::new(sc.clone());
        let shared = StateManager::new();
        let scheduler = EventScheduler::new();
        let transport = TransportClock::new();
        let reload_manager = ReloadManager::new();
        let completion_tx = None;
        let sc_midi_clock_node_id = None;
        let midi_rt = MidiRealtimeService::noop();

        (
            osc_sender,
            sc,
            shared,
            scheduler,
            transport,
            reload_manager,
            completion_tx,
            sc_midi_clock_node_id,
            midi_rt,
        )
    }

    #[test]
    fn test_handle_open_close_device() {
        let (
            mut osc_sender,
            sc,
            shared,
            mut scheduler,
            mut transport,
            mut reload_manager,
            completion_tx,
            mut sc_midi_clock_node_id,
            mut midi_rt,
        ) = create_test_context();

        let mut ctx = RuntimeContext {
            osc_sender: &mut osc_sender,
            sc: &sc,
            shared: &shared,
            scheduler: &mut scheduler,
            transport: &mut transport,
            reload_manager: &mut reload_manager,
            completion_tx: &completion_tx,
            sc_midi_clock_node_id: &mut sc_midi_clock_node_id,
            midi_rt: &mut midi_rt,
        };

        let info = MidiOutputDeviceInfo {
            name: "Test MIDI Output".to_string(),
            port_index: 0,
            backend: MidiBackend::Alsa,
        };
        let (event_tx, _event_rx) = crossbeam_channel::unbounded();

        // Open device
        handle_open_device(&mut ctx, 1, info, event_tx);

        assert!(shared.with_state_read(|s| s.midi_output_config.devices.contains_key(&1)));

        // Close device
        handle_close_device(&mut ctx, 1);

        assert!(!shared.with_state_read(|s| s.midi_output_config.devices.contains_key(&1)));
    }

    #[test]
    fn test_handle_set_clock_device() {
        let (
            mut osc_sender,
            sc,
            shared,
            mut scheduler,
            mut transport,
            mut reload_manager,
            completion_tx,
            mut sc_midi_clock_node_id,
            mut midi_rt,
        ) = create_test_context();

        let mut ctx = RuntimeContext {
            osc_sender: &mut osc_sender,
            sc: &sc,
            shared: &shared,
            scheduler: &mut scheduler,
            transport: &mut transport,
            reload_manager: &mut reload_manager,
            completion_tx: &completion_tx,
            sc_midi_clock_node_id: &mut sc_midi_clock_node_id,
            midi_rt: &mut midi_rt,
        };

        // Initially None
        assert!(shared.with_state_read(|s| s.midi_output_config.clock_device_id.is_none()));

        // Set to device 5
        handle_set_clock_device(&mut ctx, Some(5));
        assert_eq!(
            shared.with_state_read(|s| s.midi_output_config.clock_device_id),
            Some(5)
        );

        // Clear
        handle_set_clock_device(&mut ctx, None);
        assert!(shared.with_state_read(|s| s.midi_output_config.clock_device_id.is_none()));
    }

    #[test]
    fn test_handle_note_on_off() {
        let (
            mut osc_sender,
            sc,
            shared,
            mut scheduler,
            mut transport,
            mut reload_manager,
            completion_tx,
            mut sc_midi_clock_node_id,
            mut midi_rt,
        ) = create_test_context();

        let mut ctx = RuntimeContext {
            osc_sender: &mut osc_sender,
            sc: &sc,
            shared: &shared,
            scheduler: &mut scheduler,
            transport: &mut transport,
            reload_manager: &mut reload_manager,
            completion_tx: &completion_tx,
            sc_midi_clock_node_id: &mut sc_midi_clock_node_id,
            midi_rt: &mut midi_rt,
        };

        let info = MidiOutputDeviceInfo {
            name: "Test MIDI Output".to_string(),
            port_index: 0,
            backend: MidiBackend::Alsa,
        };
        let (event_tx, event_rx) = crossbeam_channel::unbounded();

        // Open device first
        handle_open_device(&mut ctx, 1, info, event_tx);

        // Send note on
        handle_note_on(&ctx, 1, 0, 60, 100);

        // Verify message was sent (note on: 0x90, note 60, velocity 100)
        let msg = event_rx.try_recv().unwrap();
        assert_eq!(msg.bytes, vec![0x90, 60, 100]);

        // Send note off
        handle_note_off(&ctx, 1, 0, 60);

        // Verify message was sent (note off: 0x80, note 60, velocity 0)
        let msg = event_rx.try_recv().unwrap();
        assert_eq!(msg.bytes, vec![0x80, 60, 0]);
    }
}
