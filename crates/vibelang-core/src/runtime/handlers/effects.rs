//! Effect chain handlers.
//!
//! Handles effect-related messages:
//! - Effect removal
//! - Effect parameter changes

use super::RuntimeContext;
use crate::osc_sender::OscTiming;
use crate::scsynth::NodeId;
use std::time::Instant;

/// Handle RemoveEffect message - remove an effect from the chain.
pub fn handle_remove_effect(ctx: &mut RuntimeContext<'_>, id: String) {
    let node_to_free = ctx.shared.with_state_write(|state| {
        let node = state.effects.remove(&id).and_then(|e| e.node_id);
        state.bump_version();
        node
    });
    if let Some(node_id) = node_to_free {
        let current_beat = ctx.transport.beat_at(Instant::now()).to_float();
        let _ = ctx
            .osc_sender
            .n_free(OscTiming::Now, NodeId::new(node_id), current_beat);
    }
}

/// Handle SetEffectParam message - set a parameter on an effect.
pub fn handle_set_effect_param(ctx: &mut RuntimeContext<'_>, id: &str, param: String, value: f32) {
    let node_to_update = ctx.shared.with_state_write(|state| {
        let node_id = state.effects.get_mut(id).and_then(|effect| {
            effect.params.insert(param.clone(), value);
            effect.node_id
        });
        state.bump_version();
        node_id
    });
    if let Some(node_id) = node_to_update {
        let current_beat = ctx.transport.beat_at(Instant::now()).to_float();
        let _ = ctx.osc_sender.n_set(
            OscTiming::Now,
            NodeId::new(node_id),
            &[(param.as_str(), value)],
            current_beat,
        );
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
    use crate::state::{EffectState, StateManager};
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
    fn test_handle_remove_effect() {
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

        // Create an effect first
        shared.with_state_write(|state| {
            let effect = EffectState {
                id: "reverb_1".to_string(),
                synthdef_name: "reverb".to_string(),
                group_path: "main".to_string(),
                params: std::collections::HashMap::new(),
                node_id: Some(1000),
                bus_in: 16,
                bus_out: 0,
                position: 0,
                vst_plugin: None,
                source_location: crate::state::SourceLocation::default(),
                generation: 0,
            };
            state.effects.insert("reverb_1".to_string(), effect);
        });

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

        assert!(shared.with_state_read(|s| s.effects.contains_key("reverb_1")));

        handle_remove_effect(&mut ctx, "reverb_1".to_string());

        assert!(!shared.with_state_read(|s| s.effects.contains_key("reverb_1")));
    }

    #[test]
    fn test_handle_set_effect_param() {
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

        // Create an effect first
        shared.with_state_write(|state| {
            let effect = EffectState {
                id: "reverb_1".to_string(),
                synthdef_name: "reverb".to_string(),
                group_path: "main".to_string(),
                params: std::collections::HashMap::new(),
                node_id: Some(1000),
                bus_in: 16,
                bus_out: 0,
                position: 0,
                vst_plugin: None,
                source_location: crate::state::SourceLocation::default(),
                generation: 0,
            };
            state.effects.insert("reverb_1".to_string(), effect);
        });

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

        handle_set_effect_param(&mut ctx, "reverb_1", "mix".to_string(), 0.6);

        let mix = shared.with_state_read(|s| {
            s.effects.get("reverb_1").and_then(|e| e.params.get("mix").copied())
        });
        assert_eq!(mix, Some(0.6));
    }
}
