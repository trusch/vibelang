//! Pattern scheduling handlers.
//!
//! Handles pattern-related messages:
//! - Pattern creation
//! - Pattern deletion
//! - Pattern parameter changes

use super::RuntimeContext;
use crate::events::Pattern;
use crate::state::{PatternState, SourceLocation};

/// Handle CreatePattern message - create or update a pattern.
pub fn handle_create_pattern(
    ctx: &mut RuntimeContext<'_>,
    name: String,
    group_path: String,
    voice_name: Option<String>,
    pattern: Pattern,
    source_location: SourceLocation,
    step_pattern: Option<String>,
) {
    let generation = ctx.shared.with_state_read(|s| s.reload_generation);
    ctx.shared.with_state_write(|state| {
        let ps = state
            .patterns
            .entry(name.clone())
            .or_insert_with(|| PatternState::new(name.clone(), group_path.clone(), voice_name.clone()));
        ps.loop_pattern = Some(pattern);
        ps.generation = generation;
        ps.group_path = group_path;
        ps.voice_name = voice_name;
        ps.source_location = source_location;
        ps.step_pattern = step_pattern;
        state.bump_version();
    });
}

/// Handle DeletePattern message - remove a pattern.
///
/// Note: This also resets the scheduler tracking to prevent ghost events
/// when the pattern is recreated. The scheduler reset must be done by the caller
/// since we don't have access to the scheduler through RuntimeContext for this operation.
pub fn handle_delete_pattern(ctx: &mut RuntimeContext<'_>, name: &str) {
    // Reset scheduler tracking to prevent ghost events when pattern is recreated
    ctx.scheduler.reset_loop(name);
    ctx.shared.with_state_write(|state| {
        state.patterns.remove(name);
        state.bump_version();
    });
}

/// Handle SetPatternParam message - set a parameter on a pattern.
pub fn handle_set_pattern_param(ctx: &mut RuntimeContext<'_>, name: &str, param: String, value: f32) {
    ctx.shared.with_state_write(|state| {
        if let Some(p) = state.patterns.get_mut(name) {
            p.params.insert(param, value);
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
    fn test_handle_create_pattern() {
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

        let pattern = Pattern::new("kick_pattern", 4.0);
        handle_create_pattern(
            &mut ctx,
            "kick_pattern".to_string(),
            "main/drums".to_string(),
            Some("kick".to_string()),
            pattern,
            SourceLocation::default(),
            Some("x...x...".to_string()),
        );

        let pattern_exists = shared.with_state_read(|s| s.patterns.contains_key("kick_pattern"));
        assert!(pattern_exists);

        let step_pattern = shared.with_state_read(|s| {
            s.patterns.get("kick_pattern").and_then(|p| p.step_pattern.clone())
        });
        assert_eq!(step_pattern, Some("x...x...".to_string()));
    }

    #[test]
    fn test_handle_delete_pattern() {
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

        // Create a pattern first
        shared.with_state_write(|state| {
            let pattern = PatternState::new(
                "test_pattern".to_string(),
                "main".to_string(),
                Some("kick".to_string()),
            );
            state.patterns.insert("test_pattern".to_string(), pattern);
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

        assert!(shared.with_state_read(|s| s.patterns.contains_key("test_pattern")));

        handle_delete_pattern(&mut ctx, "test_pattern");

        assert!(!shared.with_state_read(|s| s.patterns.contains_key("test_pattern")));
    }

    #[test]
    fn test_handle_set_pattern_param() {
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

        // Create a pattern first
        shared.with_state_write(|state| {
            let pattern = PatternState::new(
                "test_pattern".to_string(),
                "main".to_string(),
                Some("kick".to_string()),
            );
            state.patterns.insert("test_pattern".to_string(), pattern);
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

        handle_set_pattern_param(&mut ctx, "test_pattern", "velocity".to_string(), 0.9);

        let velocity = shared.with_state_read(|s| {
            s.patterns.get("test_pattern").and_then(|p| p.params.get("velocity").copied())
        });
        assert_eq!(velocity, Some(0.9));
    }
}
