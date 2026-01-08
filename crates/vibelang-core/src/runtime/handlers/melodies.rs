//! Melody scheduling handlers.
//!
//! Handles melody-related messages:
//! - Melody creation
//! - Melody deletion
//! - Melody parameter changes

use super::RuntimeContext;
use crate::events::Pattern;
use crate::state::{MelodyState, SourceLocation};

/// Handle CreateMelody message - create or update a melody.
pub fn handle_create_melody(
    ctx: &mut RuntimeContext<'_>,
    name: String,
    group_path: String,
    voice_name: Option<String>,
    pattern: Pattern,
    source_location: SourceLocation,
    notes_patterns: Vec<String>,
) {
    let generation = ctx.shared.with_state_read(|s| s.reload_generation);
    ctx.shared.with_state_write(|state| {
        let ms = state
            .melodies
            .entry(name.clone())
            .or_insert_with(|| MelodyState::new(name.clone(), group_path.clone(), voice_name.clone()));
        ms.loop_pattern = Some(pattern);
        ms.generation = generation;
        ms.group_path = group_path;
        ms.voice_name = voice_name;
        ms.source_location = source_location;
        ms.notes_patterns = notes_patterns;
        state.bump_version();
    });
}

/// Handle DeleteMelody message - remove a melody.
pub fn handle_delete_melody(ctx: &mut RuntimeContext<'_>, name: &str) {
    ctx.shared.with_state_write(|state| {
        state.melodies.remove(name);
        state.bump_version();
    });
}

/// Handle SetMelodyParam message - set a parameter on a melody.
pub fn handle_set_melody_param(ctx: &mut RuntimeContext<'_>, name: &str, param: String, value: f32) {
    ctx.shared.with_state_write(|state| {
        if let Some(m) = state.melodies.get_mut(name) {
            m.params.insert(param, value);
            state.bump_version();
        }
    });
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
    fn test_handle_create_melody() {
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

        let pattern = Pattern::new("bass_melody", 4.0);
        handle_create_melody(
            &mut ctx,
            "bass_melody".to_string(),
            "main/bass".to_string(),
            Some("bass".to_string()),
            pattern,
            SourceLocation::default(),
            vec!["C3 . E3 . G3 . .".to_string()],
        );

        let melody_exists = shared.with_state_read(|s| s.melodies.contains_key("bass_melody"));
        assert!(melody_exists);

        let notes_patterns = shared.with_state_read(|s| {
            s.melodies.get("bass_melody").map(|m| m.notes_patterns.clone())
        });
        assert_eq!(notes_patterns, Some(vec!["C3 . E3 . G3 . .".to_string()]));
    }

    #[test]
    fn test_handle_delete_melody() {
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

        // Create a melody first
        shared.with_state_write(|state| {
            let melody = MelodyState::new(
                "test_melody".to_string(),
                "main".to_string(),
                Some("lead".to_string()),
            );
            state.melodies.insert("test_melody".to_string(), melody);
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

        assert!(shared.with_state_read(|s| s.melodies.contains_key("test_melody")));

        handle_delete_melody(&mut ctx, "test_melody");

        assert!(!shared.with_state_read(|s| s.melodies.contains_key("test_melody")));
    }

    #[test]
    fn test_handle_set_melody_param() {
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

        // Create a melody first
        shared.with_state_write(|state| {
            let melody = MelodyState::new(
                "test_melody".to_string(),
                "main".to_string(),
                Some("lead".to_string()),
            );
            state.melodies.insert("test_melody".to_string(), melody);
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

        handle_set_melody_param(&mut ctx, "test_melody", "velocity".to_string(), 0.85);

        let velocity = shared.with_state_read(|s| {
            s.melodies.get("test_melody").and_then(|m| m.params.get("velocity").copied())
        });
        assert_eq!(velocity, Some(0.85));
    }
}
