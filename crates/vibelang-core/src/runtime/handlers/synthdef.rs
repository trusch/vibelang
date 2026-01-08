//! SynthDef loading handlers.
//!
//! Handles synthdef-related messages:
//! - Loading synthdefs into SuperCollider

use super::RuntimeContext;

/// Handle LoadSynthDef message - load a synthdef into SuperCollider.
pub fn handle_load_synthdef(ctx: &mut RuntimeContext<'_>, name: String, bytes: Vec<u8>) {
    log::debug!("Loading synthdef '{}'", name);

    // Store bytes in state for score capture
    ctx.shared.with_state_write(|state| {
        state.synthdefs.insert(name.clone(), bytes.clone());
    });

    // Capture to score if enabled - add /d_recv at time 0
    if let Some(writer) = ctx.osc_sender.score_writer_mut() {
        let packet = rosc::OscPacket::Message(rosc::OscMessage {
            addr: "/d_recv".to_string(),
            args: vec![rosc::OscType::Blob(bytes.clone())],
        });
        // Synthdefs should be at time 0 (before any notes play)
        writer.add_packet(0.0, packet);
        log::debug!("[SCORE] Captured synthdef '{}' at time 0", name);
    }

    if let Err(e) = ctx.sc.d_recv_bytes(bytes) {
        log::error!("Failed to load synthdef '{}': {}", name, e);
    }
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
    fn test_handle_load_synthdef_stores_in_state() {
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

        let synthdef_bytes = vec![1, 2, 3, 4];
        handle_load_synthdef(&mut ctx, "test_synth".to_string(), synthdef_bytes.clone());

        // Verify synthdef was stored in state
        let stored = shared.with_state_read(|s| s.synthdefs.get("test_synth").cloned());
        assert_eq!(stored, Some(synthdef_bytes));
    }
}
