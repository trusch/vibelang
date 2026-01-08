//! MIDI input handlers.
//!
//! Handles MIDI input-related messages:
//! - Device management (open, close)
//! - Routing configuration (keyboard, note, CC, pitch bend)
//! - Callback registration
//! - Recording configuration

use super::RuntimeContext;
use crate::midi::{CcCallback, CcRoute, KeyboardRoute, MidiBackend, MidiDeviceInfo, NoteCallback, NoteRoute};
use crate::state::{MidiCallbackInfo, MidiCallbackType, MidiDeviceState};

// =============================================================================
// Device Management
// =============================================================================

/// Handle MidiOpenDevice message - register a MIDI input device.
pub fn handle_open_device(
    ctx: &mut RuntimeContext<'_>,
    device_id: u32,
    info: MidiDeviceInfo,
    backend: MidiBackend,
) {
    let generation = ctx.shared.with_state_read(|s| s.reload_generation);
    ctx.shared.with_state_write(|state| {
        let device_state = MidiDeviceState {
            id: device_id,
            info,
            backend,
            generation,
        };
        state.midi_config.devices.insert(device_id, device_state);
        state.bump_version();
    });
    log::info!("[MIDI] Device {} registered in state", device_id);
}

/// Handle MidiCloseDevice message - unregister a MIDI input device.
pub fn handle_close_device(ctx: &mut RuntimeContext<'_>, device_id: u32) {
    ctx.shared.with_state_write(|state| {
        state.midi_config.devices.remove(&device_id);
        state.bump_version();
    });
    log::info!("[MIDI] Device {} removed from state", device_id);
}

/// Handle MidiCloseAllDevices message - unregister all MIDI input devices.
pub fn handle_close_all_devices(ctx: &mut RuntimeContext<'_>) {
    ctx.shared.with_state_write(|state| {
        state.midi_config.devices.clear();
        state.bump_version();
    });
    log::info!("[MIDI] All devices removed from state");
}

// =============================================================================
// Routing Configuration
// =============================================================================

/// Handle MidiAddKeyboardRoute message - add a keyboard route.
pub fn handle_add_keyboard_route(ctx: &mut RuntimeContext<'_>, route: KeyboardRoute) {
    ctx.shared.with_state_write(|state| {
        state.midi_config.routing.add_keyboard_route(route);
        state.bump_version();
    });
}

/// Handle MidiAddNoteRoute message - add a note route.
pub fn handle_add_note_route(
    ctx: &mut RuntimeContext<'_>,
    channel: Option<u8>,
    note: u8,
    route: NoteRoute,
) {
    ctx.shared.with_state_write(|state| {
        state.midi_config.routing.add_note_route(channel, note, route);
        state.bump_version();
    });
}

/// Handle MidiAddCcRoute message - add a CC route.
pub fn handle_add_cc_route(
    ctx: &mut RuntimeContext<'_>,
    channel: Option<u8>,
    cc_number: u8,
    route: CcRoute,
) {
    ctx.shared.with_state_write(|state| {
        state.midi_config.routing.add_cc_route(channel, cc_number, route);
        state.bump_version();
    });
}

/// Handle MidiAddPitchBendRoute message - add a pitch bend route.
pub fn handle_add_pitch_bend_route(
    ctx: &mut RuntimeContext<'_>,
    channel: Option<u8>,
    route: CcRoute,
) {
    ctx.shared.with_state_write(|state| {
        state.midi_config.routing.add_pitch_bend_route(channel, route);
        state.bump_version();
    });
}

/// Handle MidiClearRouting message - clear all routing configuration.
pub fn handle_clear_routing(ctx: &mut RuntimeContext<'_>) {
    ctx.shared.with_state_write(|state| {
        state.midi_config.clear_routing();
        state.bump_version();
    });
    log::info!("[MIDI] All routing cleared");
}

// =============================================================================
// Callback Registration
// =============================================================================

/// Handle MidiRegisterNoteCallback message - register a note callback.
pub fn handle_register_note_callback(
    ctx: &mut RuntimeContext<'_>,
    callback_id: u64,
    channel: Option<u8>,
    note: u8,
    on_note_on: bool,
    on_note_off: bool,
) {
    let generation = ctx.shared.with_state_read(|s| s.reload_generation);
    ctx.shared.with_state_write(|state| {
        // Add to routing
        let callback = NoteCallback {
            channel,
            note,
            on_note_on,
            on_note_off,
            callback_id,
        };
        state.midi_config.routing.add_note_callback(callback);

        // Track callback metadata
        state.midi_config.callbacks.insert(
            callback_id,
            MidiCallbackInfo {
                id: callback_id,
                callback_type: MidiCallbackType::Note {
                    channel,
                    note,
                    on_note_on,
                    on_note_off,
                },
                generation,
            },
        );
        state.bump_version();
    });
}

/// Handle MidiRegisterCcCallback message - register a CC callback.
pub fn handle_register_cc_callback(
    ctx: &mut RuntimeContext<'_>,
    callback_id: u64,
    channel: Option<u8>,
    cc_number: u8,
    threshold: Option<u8>,
    above_threshold: bool,
) {
    let generation = ctx.shared.with_state_read(|s| s.reload_generation);
    ctx.shared.with_state_write(|state| {
        // Add to routing
        let callback = CcCallback {
            channel,
            cc_number,
            threshold,
            above_threshold,
            callback_id,
        };
        state.midi_config.routing.add_cc_callback(callback);

        // Track callback metadata
        state.midi_config.callbacks.insert(
            callback_id,
            MidiCallbackInfo {
                id: callback_id,
                callback_type: MidiCallbackType::Cc {
                    channel,
                    cc_number,
                    threshold,
                    above_threshold,
                },
                generation,
            },
        );
        state.bump_version();
    });
}

// =============================================================================
// Monitoring
// =============================================================================

/// Handle MidiSetMonitoring message - enable/disable MIDI monitoring.
pub fn handle_set_monitoring(ctx: &mut RuntimeContext<'_>, enabled: bool) {
    ctx.shared.with_state_write(|state| {
        state.midi_config.monitor_enabled = enabled;
        state.midi_config.routing.monitor_enabled = enabled;
        state.bump_version();
    });
    log::info!("[MIDI] Monitoring set to {}", enabled);
}

// =============================================================================
// Recording Configuration
// =============================================================================

/// Handle MidiSetRecordingQuantization message - set recording quantization.
pub fn handle_set_recording_quantization(ctx: &mut RuntimeContext<'_>, positions_per_bar: u8) {
    // Validate: must be 4, 8, 16, 32, or 64
    if [4, 8, 16, 32, 64].contains(&positions_per_bar) {
        ctx.shared.with_state_write(|state| {
            state.midi_recording.quantization = positions_per_bar;
            state.bump_version();
        });
        log::debug!("[MIDI] Recording quantization set to 1/{}", positions_per_bar);
    } else {
        log::warn!(
            "[MIDI] Invalid recording quantization {}, must be 4, 8, 16, 32, or 64",
            positions_per_bar
        );
    }
}

/// Handle MidiSetRecordingEnabled message - enable/disable MIDI recording.
pub fn handle_set_recording_enabled(ctx: &mut RuntimeContext<'_>, enabled: bool) {
    ctx.shared.with_state_write(|state| {
        state.midi_recording.recording_enabled = enabled;
        state.bump_version();
    });
    log::info!("[MIDI] Recording {}", if enabled { "enabled" } else { "disabled" });
}

/// Handle MidiClearRecording message - clear recording history.
pub fn handle_clear_recording(ctx: &mut RuntimeContext<'_>) {
    ctx.shared.with_state_write(|state| {
        state.midi_recording.clear();
        state.bump_version();
    });
    log::info!("[MIDI] Recording history cleared");
}

#[cfg(test)]
mod tests {
    use super::*;
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

        let info = MidiDeviceInfo {
            name: "Test MIDI".to_string(),
            port_index: 0,
            backend: MidiBackend::Alsa,
        };

        // Open device
        handle_open_device(&mut ctx, 1, info, MidiBackend::Alsa);

        assert!(shared.with_state_read(|s| s.midi_config.devices.contains_key(&1)));

        // Close device
        handle_close_device(&mut ctx, 1);

        assert!(!shared.with_state_read(|s| s.midi_config.devices.contains_key(&1)));
    }

    #[test]
    fn test_handle_set_monitoring() {
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

        // Initially false
        assert!(!shared.with_state_read(|s| s.midi_config.monitor_enabled));

        // Enable
        handle_set_monitoring(&mut ctx, true);
        assert!(shared.with_state_read(|s| s.midi_config.monitor_enabled));

        // Disable
        handle_set_monitoring(&mut ctx, false);
        assert!(!shared.with_state_read(|s| s.midi_config.monitor_enabled));
    }

    #[test]
    fn test_handle_set_recording_quantization() {
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

        // Valid quantization
        handle_set_recording_quantization(&mut ctx, 16);
        assert_eq!(shared.with_state_read(|s| s.midi_recording.quantization), 16);

        // Invalid quantization (should not change)
        handle_set_recording_quantization(&mut ctx, 12);
        assert_eq!(shared.with_state_read(|s| s.midi_recording.quantization), 16);
    }
}
