//! Transport control handlers.
//!
//! Handles transport-related messages:
//! - BPM/tempo changes
//! - Time signature changes
//! - Scheduler start/stop
//! - Transport seeking
//! - Hot reload initiation

use super::RuntimeContext;
use crate::osc_sender::OscTiming;
use crate::reload::StateSnapshot;
use crate::scsynth::NodeId;
use crate::timing::{BeatTime, TimeSignature};
use std::time::Instant;

const EPSILON: f64 = 1e-6;

/// Handle SetBpm message - change the tempo.
pub fn handle_set_bpm(ctx: &mut RuntimeContext<'_>, bpm: f64) {
    let now = Instant::now();
    ctx.transport.set_bpm(bpm, now);
    ctx.shared.with_state_write(|state| {
        state.tempo = bpm;
        state.bump_version();
    });
    // Also update OscSender's tempo for score capture timing
    ctx.osc_sender.set_tempo(bpm);

    // Update SC-managed MIDI clock synth tempo if running
    update_sc_midi_clock_tempo(ctx, bpm);
}

/// Handle SetQuantization message - set quantization grid.
pub fn handle_set_quantization(ctx: &mut RuntimeContext<'_>, beats: f64) {
    ctx.shared.with_state_write(|state| {
        state.quantization_beats = beats.max(EPSILON);
        state.bump_version();
    });
}

/// Handle SetTimeSignature message - change time signature.
pub fn handle_set_time_signature(ctx: &mut RuntimeContext<'_>, numerator: u32, denominator: u32) {
    ctx.transport
        .set_time_signature(numerator, denominator, Instant::now());
    ctx.shared.with_state_write(|state| {
        state.time_signature = TimeSignature::new(numerator, denominator);
        state.bump_version();
    });
}

/// Handle StartScheduler message - start transport playback.
pub fn handle_start_scheduler(ctx: &mut RuntimeContext<'_>) {
    let now = Instant::now();
    ctx.transport.start(now);
    // Don't reset scheduler here - seek handles that
    ctx.shared.with_state_write(|state| {
        state.transport_running = true;
        state.bump_version();
    });

    // Send MIDI start message if clock output is enabled
    let clock_enabled = ctx
        .shared
        .with_state_read(|state| state.midi_output_config.clock_output_enabled);
    if clock_enabled {
        ctx.send_midi_clock_message(crate::midi::QueuedMidiEvent::start());
        log::info!("[MIDI CLOCK] Sent START message");
    }
}

/// Handle StopScheduler message - stop transport playback.
pub fn handle_stop_scheduler(ctx: &mut RuntimeContext<'_>) {
    let now = Instant::now();
    let current_beat = ctx.transport.beat_at(now).to_float();

    // Send MIDI stop message if clock output is enabled
    let clock_enabled = ctx
        .shared
        .with_state_read(|state| state.midi_output_config.clock_output_enabled);
    if clock_enabled {
        ctx.send_midi_clock_message(crate::midi::QueuedMidiEvent::stop());
        log::info!("[MIDI CLOCK] Sent STOP message");
    }

    // Stop transport and prevent new events
    ctx.transport.stop(now);
    ctx.scheduler.sync_to_beat(current_beat);
    ctx.shared.with_state_write(|state| {
        state.transport_running = false;
        state.bump_version();
    });

    // Collect all nodes that need gate=0 (active notes + pending)
    let nodes_to_release: Vec<i32> = ctx.shared.with_state_write(|state| {
        use std::collections::HashSet;
        let mut nodes: HashSet<i32> = HashSet::new();

        // All active notes
        for voice in state.voices.values_mut() {
            for node_ids in voice.active_notes.values() {
                nodes.extend(node_ids.iter().copied());
            }
            voice.active_notes.clear();
        }

        // All active synths
        for &node_id in state.active_synths.keys() {
            nodes.insert(node_id);
        }

        // All pending nodes (scheduled but may not have started yet)
        for &node_id in state.pending_nodes.keys() {
            nodes.insert(node_id);
        }

        // Clear scheduled note-offs since we're releasing everything
        state.scheduled_note_offs.clear();

        nodes.into_iter().collect()
    });

    // Send gate=0 to release all notes
    log::debug!(
        "[STOP] Pausing at beat {}, sending gate=0 to {} nodes",
        current_beat,
        nodes_to_release.len()
    );
    for node_id in nodes_to_release {
        let _ = ctx.osc_sender.n_set(
            OscTiming::Now,
            NodeId::new(node_id),
            &[("gate", 0.0f32)],
            current_beat,
        );
    }
}

/// Handle SeekTransport message - seek to a specific beat.
pub fn handle_seek_transport(ctx: &mut RuntimeContext<'_>, beat: f64) {
    let now = Instant::now();
    let target_beat = beat.max(0.0);
    ctx.transport.seek(BeatTime::from_float(target_beat), now);
    // Reset scheduler to target beat to prevent event burst
    ctx.scheduler.reset_to_beat(target_beat);
    ctx.shared.with_state_write(|state| {
        state.current_beat = target_beat;
        // Reset sequence anchors so they restart cleanly from target beat
        for active in state.active_sequences.values_mut() {
            active.anchor_beat = target_beat;
            active.triggered_clips.clear();
            active.last_iteration = 0;
            active.completed = false;
        }
        state.bump_version();
    });
}

/// Handle BeginReload message - start a hot reload cycle.
///
/// The snapshot should be captured before calling this function to avoid
/// borrow checker issues with the RuntimeContext.
pub fn handle_begin_reload(ctx: &mut RuntimeContext<'_>, snapshot: StateSnapshot) {
    // Begin reload with the pre-captured snapshot
    ctx.reload_manager.begin_reload(snapshot);

    // Update quantization from state
    let quantization = ctx.shared.with_state_read(|s| s.quantization_beats);
    ctx.reload_manager.set_quantization(quantization);

    // Increment generation for tracking which entities were touched
    // Also clear MIDI routing (but keep devices connected) so scripts can re-register routes
    ctx.shared.with_state_write(|state| {
        state.reload_generation += 1;
        // Clear MIDI routing but keep devices - routes will be re-registered by script
        state.midi_config.routing.clear();
        state.midi_config.callbacks.clear();
        state.bump_version();
    });
    log::debug!("[MIDI] Cleared routing on reload (devices preserved)");
}

/// Handle SetScrubMute message - mute during scrubbing.
pub fn handle_set_scrub_mute(ctx: &mut RuntimeContext<'_>, muted: bool) {
    ctx.shared.with_state_write(|state| {
        state.scrub_muted = muted;
        state.bump_version();
    });
}

/// Update the SC-managed MIDI clock synth tempo.
fn update_sc_midi_clock_tempo(ctx: &mut RuntimeContext<'_>, bpm: f64) {
    if let Some(node_id) = *ctx.sc_midi_clock_node_id {
        let clock_freq = bpm / 60.0 * 24.0;

        log::debug!(
            "[SC-MIDI CLOCK] Updating clock tempo: node={} bpm={} freq={}Hz",
            node_id,
            bpm,
            clock_freq
        );

        if let Err(e) = ctx.osc_sender.n_set(
            OscTiming::Now,
            NodeId::new(node_id),
            &[("freq", clock_freq as f32)],
            ctx.transport.beat_at(Instant::now()).to_float(),
        ) {
            log::error!("[SC-MIDI CLOCK] Failed to update clock tempo: {}", e);
        }
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
    fn test_handle_set_bpm() {
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

        // Initial tempo is 120
        let initial_tempo = shared.with_state_read(|s| s.tempo);
        assert_eq!(initial_tempo, 120.0);

        // Set new tempo
        handle_set_bpm(&mut ctx, 140.0);

        let new_tempo = shared.with_state_read(|s| s.tempo);
        assert_eq!(new_tempo, 140.0);
    }

    #[test]
    fn test_handle_set_quantization() {
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

        handle_set_quantization(&mut ctx, 0.5);

        let quant = shared.with_state_read(|s| s.quantization_beats);
        assert_eq!(quant, 0.5);
    }

    #[test]
    fn test_handle_set_time_signature() {
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

        handle_set_time_signature(&mut ctx, 3, 4);

        let (num, denom) = shared.with_state_read(|s| {
            (s.time_signature.numerator, s.time_signature.denominator)
        });
        assert_eq!(num, 3);
        assert_eq!(denom, 4);
    }

    #[test]
    fn test_handle_start_stop_scheduler() {
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

        // Initially not running
        assert!(!shared.with_state_read(|s| s.transport_running));

        // Start scheduler
        handle_start_scheduler(&mut ctx);
        assert!(shared.with_state_read(|s| s.transport_running));

        // Stop scheduler
        handle_stop_scheduler(&mut ctx);
        assert!(!shared.with_state_read(|s| s.transport_running));
    }

    #[test]
    fn test_handle_seek_transport() {
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

        // Seek to beat 16.0
        handle_seek_transport(&mut ctx, 16.0);

        let current_beat = shared.with_state_read(|s| s.current_beat);
        assert_eq!(current_beat, 16.0);
    }

    #[test]
    fn test_handle_seek_transport_clamps_negative() {
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

        // Seek to negative beat should clamp to 0
        handle_seek_transport(&mut ctx, -5.0);

        let current_beat = shared.with_state_read(|s| s.current_beat);
        assert_eq!(current_beat, 0.0);
    }

    #[test]
    fn test_handle_set_scrub_mute() {
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

        // Initially not muted
        assert!(!shared.with_state_read(|s| s.scrub_muted));

        // Mute
        handle_set_scrub_mute(&mut ctx, true);
        assert!(shared.with_state_read(|s| s.scrub_muted));

        // Unmute
        handle_set_scrub_mute(&mut ctx, false);
        assert!(!shared.with_state_read(|s| s.scrub_muted));
    }
}
