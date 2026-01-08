//! Group management handlers.
//!
//! Handles group-related messages:
//! - Group registration/unregistration
//! - Group parameter changes
//! - Group mute/unmute/solo
//! - Group finalization (link synth creation)

use super::RuntimeContext;
use crate::osc_sender::OscTiming;
use crate::scsynth::{AddAction, NodeId, Target};
use crate::state::{GroupState, SourceLocation};
use std::time::Instant;

/// Handle UnregisterGroup message - remove a group from state.
pub fn handle_unregister_group(ctx: &mut RuntimeContext<'_>, path: String) {
    ctx.shared.with_state_write(|state| {
        state.groups.remove(&path);
        state.bump_version();
    });
}

/// Handle SetGroupParam message - set a parameter on a group.
pub fn handle_set_group_param(ctx: &mut RuntimeContext<'_>, path_or_name: &str, param: &str, value: f32) {
    // Update state - try to find group by path first, then by name
    let actual_path = ctx.shared.with_state_write(|state| {
        // First try exact path match
        if let Some(group) = state.groups.get_mut(path_or_name) {
            group.params.insert(param.to_string(), value);
            state.bump_version();
            return Some(path_or_name.to_string());
        }

        // If not found, search by name (path ending in .name or just name)
        let found_path = state
            .groups
            .iter()
            .find(|(path, g)| g.name == path_or_name || path.ends_with(&format!(".{}", path_or_name)))
            .map(|(path, _)| path.clone());

        if let Some(ref path) = found_path {
            if let Some(group) = state.groups.get_mut(path) {
                group.params.insert(param.to_string(), value);
                state.bump_version();
            }
        }

        found_path
    });

    match &actual_path {
        Some(path) => {
            log::trace!("[GROUP PARAM] Set {}:{}={}", path, param, value);

            // For amp parameter, also update the link synth in real-time
            // This allows the mixer fader to have immediate effect on audio output
            if param == "amp" {
                let link_node_id = ctx.shared.with_state_read(|state| {
                    state.groups.get(path).and_then(|g| g.link_synth_node_id)
                });

                if let Some(node_id) = link_node_id {
                    // Send n_set to update the link synth's amp parameter
                    let current_beat = ctx.transport.beat_at(Instant::now()).to_float();
                    let _ = ctx.osc_sender.n_set(
                        OscTiming::Now,
                        NodeId::new(node_id),
                        &[("amp", value)],
                        current_beat,
                    );
                    log::trace!("[GROUP PARAM] Updated link synth {} amp={}", node_id, value);
                }
            }

            // Note: For other params, we only update state, not running synths.
            // Group params affect NEW synths - the final amp is calculated as:
            // event_amp × voice_gain × group_amp × voice_amp
        }
        None => {
            log::trace!(
                "[GROUP PARAM] Group '{}' not found when setting {}={}",
                path_or_name,
                param,
                value
            );
        }
    }
}

/// Handle MuteGroup message - mute a group.
pub fn handle_mute_group(ctx: &mut RuntimeContext<'_>, path: &str) {
    set_group_run_state(ctx, path, false);
}

/// Handle UnmuteGroup message - unmute a group.
pub fn handle_unmute_group(ctx: &mut RuntimeContext<'_>, path: &str) {
    set_group_run_state(ctx, path, true);
}

/// Handle SoloGroup message - solo or unsolo a group.
pub fn handle_solo_group(ctx: &mut RuntimeContext<'_>, path: &str, solo: bool) {
    ctx.shared.with_state_write(|state| {
        if let Some(group) = state.groups.get_mut(path) {
            group.soloed = solo;
            state.bump_version();
        }
    });
}

/// Set the run state of a group (muted or unmuted).
fn set_group_run_state(ctx: &mut RuntimeContext<'_>, path: &str, running: bool) {
    let node_to_set = ctx.shared.with_state_write(|state| {
        let node_id = state.groups.get_mut(path).and_then(|group| {
            group.muted = !running;
            group.node_id
        });
        state.bump_version();
        node_id
    });
    if let Some(node_id) = node_to_set {
        let current_beat = ctx.transport.beat_at(Instant::now()).to_float();
        let _ = ctx
            .osc_sender
            .n_run(OscTiming::Now, NodeId::new(node_id), running, current_beat);
    }
}

/// Handle RegisterGroup message - register a new group.
pub fn handle_register_group(
    ctx: &mut RuntimeContext<'_>,
    name: String,
    path: String,
    parent_path: Option<String>,
    mut node_id: i32,
    source_location: SourceLocation,
) {
    let generation = ctx.shared.with_state_read(|s| s.reload_generation);

    // Check if the group already exists in state (was created externally)
    let already_exists = ctx
        .shared
        .with_state_read(|state| state.groups.contains_key(&path));

    if already_exists {
        // Update generation and source_location even if group exists
        ctx.shared.with_state_write(|state| {
            if let Some(group) = state.groups.get_mut(&path) {
                group.generation = generation;
                group.source_location = source_location;
            }
        });
        log::debug!(
            "Group '{}' already exists, updated generation and source_location",
            path
        );
        return;
    }

    // Track if node was pre-created externally (non-zero node_id means SC group already exists)
    let externally_created = node_id != 0;

    // Allocate node ID if not provided (0 means allocate)
    if node_id == 0 {
        node_id = ctx
            .shared
            .with_state_write(|state| state.allocate_group_node());
    }

    // Create the group on SuperCollider (only if not externally created)
    if !externally_created {
        // Get parent's node_id and link_synth_node_id
        let (parent_id, parent_link_synth) = parent_path
            .as_ref()
            .and_then(|pp| {
                ctx.shared.with_state_read(|state| {
                    state
                        .groups
                        .get(pp)
                        .map(|g| (g.node_id, g.link_synth_node_id))
                })
            })
            .unwrap_or((Some(0), None));

        let parent_node_id = parent_id.unwrap_or(0);

        // IMPORTANT: If parent has a link synth, we must place the new group BEFORE it!
        // Otherwise the link synth executes before the child group's audio is written,
        // causing the child's audio to never reach the parent's bus.
        let (add_action, target) = if let Some(link_node) = parent_link_synth {
            log::info!(
                "[GROUP] Creating group '{}' BEFORE parent's link synth (node {})",
                path,
                link_node
            );
            (AddAction::AddBefore, Target::from(link_node))
        } else {
            log::info!(
                "[GROUP] Creating group '{}' at HEAD of parent (node {})",
                path,
                parent_node_id
            );
            (AddAction::AddToHead, Target::from(parent_node_id))
        };

        if let Err(e) = ctx.osc_sender.g_new(NodeId::new(node_id), add_action, target) {
            log::error!("Failed to create group '{}': {}", path, e);
            return;
        }
    }

    // Allocate audio bus (always required, never optional)
    let audio_bus = ctx
        .shared
        .with_state_write(|state| state.allocate_audio_bus());

    // Store in state
    ctx.shared.with_state_write(|state| {
        let mut group = GroupState::new(name, path.clone(), parent_path, audio_bus);
        group.node_id = Some(node_id);
        group.generation = generation;
        group.source_location = source_location;
        state.groups.insert(path, group);
        state.bump_version();
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
    fn test_handle_unregister_group() {
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

        // Add a test group first
        shared.with_state_write(|state| {
            let group = GroupState::new("test".to_string(), "test".to_string(), None, 16);
            state.groups.insert("test".to_string(), group);
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

        // Verify group exists
        assert!(shared.with_state_read(|s| s.groups.contains_key("test")));

        // Unregister the group
        handle_unregister_group(&mut ctx, "test".to_string());

        // Verify group is removed
        assert!(!shared.with_state_read(|s| s.groups.contains_key("test")));
    }

    #[test]
    fn test_handle_solo_group() {
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

        // Add a test group
        shared.with_state_write(|state| {
            let group = GroupState::new("test".to_string(), "test".to_string(), None, 16);
            state.groups.insert("test".to_string(), group);
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

        // Initially not soloed
        assert!(!shared.with_state_read(|s| s.groups.get("test").map(|g| g.soloed).unwrap_or(true)));

        // Solo the group
        handle_solo_group(&mut ctx, "test", true);
        assert!(shared.with_state_read(|s| s.groups.get("test").map(|g| g.soloed).unwrap_or(false)));

        // Unsolo the group
        handle_solo_group(&mut ctx, "test", false);
        assert!(!shared.with_state_read(|s| s.groups.get("test").map(|g| g.soloed).unwrap_or(true)));
    }

    #[test]
    fn test_handle_set_group_param() {
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

        // Add a test group
        shared.with_state_write(|state| {
            let group = GroupState::new("mygroup".to_string(), "main/mygroup".to_string(), Some("main".to_string()), 16);
            state.groups.insert("main/mygroup".to_string(), group);
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

        // Set a parameter using full path
        handle_set_group_param(&mut ctx, "main/mygroup", "amp", 0.5);

        let amp = shared.with_state_read(|s| {
            s.groups.get("main/mygroup").and_then(|g| g.params.get("amp").copied())
        });
        assert_eq!(amp, Some(0.5));

        // Set a parameter using name (should also work)
        handle_set_group_param(&mut ctx, "mygroup", "pan", 0.25);

        let pan = shared.with_state_read(|s| {
            s.groups.get("main/mygroup").and_then(|g| g.params.get("pan").copied())
        });
        assert_eq!(pan, Some(0.25));
    }
}
