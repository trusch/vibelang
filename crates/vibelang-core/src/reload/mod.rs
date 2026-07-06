//! Live reloading support for vibelang-core.
//!
//! This module provides infrastructure for hot-reloading VibeLang scripts
//! without stopping playback. It calculates the minimal set of changes needed
//! and applies them incrementally.
//!
//! # Overview
//!
//! ```text
//! ┌─────────────────┐     ┌─────────────────┐
//! │  Old State      │     │  New State      │
//! │  (runtime)      │     │  (from script)  │
//! └────────┬────────┘     └────────┬────────┘
//!          │                       │
//!          └───────────┬───────────┘
//!                      │
//!                      ▼
//!             ┌─────────────────┐
//!             │   Diff Engine   │
//!             └────────┬────────┘
//!                      │
//!                      ▼
//!             ┌─────────────────┐
//!             │  Patch Actions  │
//!             │  (minimal set)  │
//!             └────────┬────────┘
//!                      │
//!                      ▼
//!             ┌─────────────────┐
//!             │  Apply to       │
//!             │  Runtime        │
//!             └─────────────────┘
//! ```
//!
//! # Key Features
//!
//! - **Minimal updates**: Only changes what actually changed
//! - **Bus preservation**: Entities keep their audio buses across updates
//! - **Ordered operations**: Deletes children before parents, creates parents before children
//! - **State preservation**: Transport keeps running, beat position preserved
//!
//! # Usage
//!
//! ```ignore
//! use vibelang_core::reload::{ScriptState, GroupConfig};
//! use vibelang_core::message::{Message, ReloadMessage};
//!
//! // Build new state from script
//! let mut new_state = ScriptState::new()
//!     .with_tempo(140.0);
//!
//! new_state.add_group(GroupId::new(1), GroupConfig::default());
//!
//! // Send reload message
//! runtime.send(Message::Reload(ReloadMessage::Apply { state: new_state })).await?;
//! ```

mod bus_pool;
mod diff;
mod port_diff;
mod script_state;

pub use bus_pool::BusAllocator;
pub use diff::{
    diff_entities, diff_param_routes, diff_param_routes_with_shaping, diff_routes, EntityDiff,
    ParamDiff, ReloadDiff,
};
pub use port_diff::{diff_port_set, reconcile_voice_ports, PortReconcile, PortSetDiff};
pub use script_state::{
    reconcile_voice_input_ports, BodyContribution, EffectConfig, GroupAliasError, GroupAliasTarget,
    GroupConfig, InputPortDropReason, InputPortReconcile, ParamRouteConflict, ParamRouteKind,
    ScriptState,
};
// ChangeQuant is defined in this module and exported directly

#[cfg(feature = "midi")]
pub use script_state::{
    AdvancedMidiBendRoute, AdvancedMidiCcRoute, AdvancedMidiKeyboardRoute, AdvancedMidiNoteRoute,
    LooperConfig, LooperKey, Midi2CcRoute, Midi2KeyboardRoute, Midi2PerNoteControllerType,
    Midi2PerNoteRoute, MidiCallbackConfig, MidiCcRoute, MidiClockOutputRequest, MidiKeyboardRoute,
    MidiOutputMessage, MidiRecordingRequest,
};

// Types available on all platforms (for order_group_creations)
use crate::handlers::{
    compute_input_route_diff, default_routes_for_voice, merge_default_routes, ParamRouteMap,
    RouteDest, RouteMap,
};
use crate::state::VoiceRole;
use crate::types::GroupId;
use std::collections::{HashMap, HashSet};
use vibelang_dsp::{OutputPort, PortRate};

// Imports for calculate_diff and order_group_deletions
use crate::state::State;
use crate::traits::SampleConfig;
use crate::types::{
    Beat, BufferId, EffectId, FadeId, MelodyId, PatternId, SampleId, SequenceId, SfzId,
    TimeSignature, VoiceId,
};

/// Quantization mode for applying hot reload changes.
///
/// Controls when content swaps take effect during playback. Using musical
/// quantization ensures changes happen at musically appropriate moments,
/// preventing audio disruption.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChangeQuant {
    /// Apply change immediately (may cause audio artifacts).
    Immediate,
    /// Apply change at the next beat boundary.
    NextBeat,
    /// Apply change at the next bar boundary (default).
    #[default]
    NextBar,
    /// Apply change at the end of the current pattern/melody loop.
    NextCycle,
}

impl ChangeQuant {
    /// Check if a change should be applied at the current beat position.
    ///
    /// # Arguments
    /// * `current_beat` - The global transport beat position
    /// * `loop_position` - The current position within the pattern/melody loop
    /// * `loop_length` - The length of the pattern/melody in beats
    /// * `time_sig` - The current time signature
    /// * `tolerance` - Beat tolerance for boundary detection (typically ~0.02 beats at 100Hz tick)
    pub fn should_apply(
        &self,
        current_beat: Beat,
        loop_position: Beat,
        loop_length: Beat,
        time_sig: TimeSignature,
        tolerance: f64,
    ) -> bool {
        match self {
            ChangeQuant::Immediate => true,
            ChangeQuant::NextBeat => {
                // Near integer beat boundary
                let beat_frac = current_beat.to_f64().fract();
                beat_frac < tolerance || beat_frac > (1.0 - tolerance)
            }
            ChangeQuant::NextBar => {
                // Near bar boundary (beat 0 of any bar)
                let bar_length = time_sig.numerator as f64;
                let position_in_bar = current_beat.to_f64() % bar_length;
                position_in_bar < tolerance || position_in_bar > (bar_length - tolerance)
            }
            ChangeQuant::NextCycle => {
                // Near the start of the pattern/melody loop
                let pos = loop_position.to_f64();
                let len = loop_length.to_f64();
                pos < tolerance || pos > (len - tolerance)
            }
        }
    }
}

fn route_source_count_for_voice(voice_id: VoiceId, maps: [&ParamRouteMap; 3]) -> usize {
    maps.into_iter()
        .flat_map(|map| map.iter())
        .filter(|((source_voice, _), _)| *source_voice == voice_id)
        .map(|(_, targets)| targets.len())
        .sum()
}

fn output_rate(
    current: &State,
    new: &ScriptState,
    voice_id: VoiceId,
    port_name: &str,
) -> Option<PortRate> {
    let config = new.voices.get(&voice_id)?;
    output_ports_for_script_voice(current, voice_id, config)
        .into_iter()
        .find(|port| port.name == port_name)
        .map(|port| port.rate)
}

fn output_ports_for_script_voice(
    current: &State,
    voice_id: VoiceId,
    config: &crate::traits::VoiceConfig,
) -> Vec<OutputPort> {
    let current_ports = current.synthdef_outputs(&config.synthdef);
    if current
        .voices
        .get(&voice_id)
        .is_some_and(|voice| voice.config.synthdef == config.synthdef)
    {
        vibelang_dsp::get_synthdef_outputs(&config.synthdef).unwrap_or(current_ports)
    } else {
        current_ports
    }
}

fn has_audible_ar_route(current: &State, new: &ScriptState, voice_id: VoiceId) -> bool {
    new.routes.iter().any(|((route_voice, port_name), dests)| {
        *route_voice == voice_id
            && matches!(
                output_rate(current, new, voice_id, port_name),
                Some(PortRate::Ar)
            )
            && dests
                .iter()
                .any(|dest| matches!(dest, RouteDest::Group(_) | RouteDest::Main))
    })
}

fn has_kr_or_tr_user_route(current: &State, new: &ScriptState, voice_id: VoiceId) -> bool {
    new.routes.iter().any(|((route_voice, port_name), dests)| {
        *route_voice == voice_id
            && matches!(
                output_rate(current, new, voice_id, port_name),
                Some(PortRate::Kr | PortRate::Tr)
            )
            && dests
                .iter()
                .any(|dest| matches!(dest, RouteDest::Param { .. }))
    })
}

fn calculate_voice_roles(current: &State, new: &ScriptState) -> HashMap<VoiceId, VoiceRole> {
    let param_maps = [
        &new.param_routes_set,
        &new.param_routes_bend,
        &new.param_routes_trigger,
    ];

    new.voices
        .iter()
        .map(|(voice_id, config)| {
            let has_mod_route = route_source_count_for_voice(*voice_id, param_maps) > 0
                || has_kr_or_tr_user_route(current, new, *voice_id);
            let role = if config.modulator_only
                || (has_mod_route && !has_audible_ar_route(current, new, *voice_id))
            {
                VoiceRole::ModulatorOnly
            } else {
                VoiceRole::Audible
            };
            (*voice_id, role)
        })
        .collect()
}

fn default_output_routes_for_script_state(current: &State, new: &ScriptState) -> RouteMap {
    let mut defaults = RouteMap::new();
    for (voice_id, config) in &new.voices {
        if config.sfz_instrument.is_none()
            && !config.synthdef.is_empty()
            && !current.synthdefs.contains(&config.synthdef)
        {
            continue;
        }
        let ports = output_ports_for_script_voice(current, *voice_id, config);
        for (port_name, dests) in default_routes_for_voice(config.group, &ports) {
            defaults.insert((*voice_id, port_name), dests);
        }
    }
    defaults
}

fn output_routes_for_script_state(current: &State, new: &ScriptState) -> RouteMap {
    new.routes
        .iter()
        .filter(|((voice_id, port_name), _)| {
            let Some(config) = new.voices.get(voice_id) else {
                return true;
            };
            if !current
                .voices
                .get(voice_id)
                .is_some_and(|voice| voice.config.synthdef == config.synthdef)
            {
                return true;
            }
            matches!(
                output_rate(current, new, *voice_id, port_name),
                Some(PortRate::Ar)
            )
        })
        .map(|(key, dests)| (key.clone(), dests.clone()))
        .collect()
}

fn effective_output_routes(
    current: &State,
    new: &ScriptState,
    voice_roles: &HashMap<VoiceId, VoiceRole>,
) -> RouteMap {
    let defaults = default_output_routes_for_script_state(current, new);
    let mut filtered = RouteMap::new();
    let mut suppressed: HashSet<VoiceId> = HashSet::new();
    let param_maps = [
        &new.param_routes_set,
        &new.param_routes_bend,
        &new.param_routes_trigger,
    ];

    for (key, dests) in defaults {
        let (voice_id, _) = key;
        if voice_roles.get(&voice_id) == Some(&VoiceRole::ModulatorOnly) {
            suppressed.insert(voice_id);
        } else {
            filtered.insert(key, dests);
        }
    }

    for voice_id in suppressed {
        let count = route_source_count_for_voice(voice_id, param_maps);
        let name = new
            .voices
            .get(&voice_id)
            .map(|config| config.name.clone())
            .or_else(|| {
                current
                    .voices
                    .get(&voice_id)
                    .map(|voice| voice.config.name.clone())
            })
            .unwrap_or_else(|| format!("{:?}", voice_id));
        let reason = if new
            .voices
            .get(&voice_id)
            .map(|config| config.modulator_only)
            .unwrap_or(false)
        {
            "explicit modulator_only() flag"
        } else {
            "VoiceRole::ModulatorOnly"
        };
        tracing::info!(
            "Voice '{}': skipping default audio routing — {} ({} outgoing param routes)",
            name,
            reason,
            count
        );
    }

    let user_routes = output_routes_for_script_state(current, new);
    merge_default_routes(&user_routes, &filtered)
}

/// Calculate the diff between current runtime state and new script state.
pub fn calculate_diff(current: &State, new: &ScriptState, current_routes: &RouteMap) -> ReloadDiff {
    let mut diff = ReloadDiff::default();

    // Tempo
    if (current.tempo - new.tempo).abs() > f64::EPSILON {
        diff.tempo_changed = Some(new.tempo);
    }

    // Time signature
    if current.time_sig != new.time_sig {
        diff.time_sig_changed = Some(new.time_sig);
    }

    // Groups
    //
    // Params come from the script-config snapshot (falling back to the live
    // params map for state assembled outside the reload path) so live
    // `set_param` tweaks neither dirty the diff nor mask script edits, and
    // the effect chain is reconstructed from runtime state so equality with
    // the script-side `GroupConfig::effects` is meaningful — with the old
    // `Vec::new()` placeholder every effect-bearing group diffed as
    // "updated" on every reload.
    let current_group_ids: HashSet<GroupId> = current.groups.keys().copied().collect();
    diff.groups = diff_entities(&current_group_ids, &new.groups, |id| {
        current.groups.get(id).map(|g| GroupConfig {
            name: g.name.clone(),
            parent: g.parent,
            params: current
                .script_group_params
                .get(id)
                .cloned()
                .unwrap_or_else(|| g.params.clone()),
            effects: current
                .script_group_effects
                .get(id)
                .cloned()
                .unwrap_or_else(|| current.effect_ids_in_group(*id)),
            muted: g.muted,
            soloed: g.soloed,
            output_bus: g.output_bus,
            output_channels: g.output_channels,
        })
    });

    // Voices
    //
    // Same script-snapshot substitution for params: `VoiceConfig::params` on
    // the live state absorbs HTTP/MIDI `set_param` writes and runtime-set
    // values, which must not count as config drift.
    let current_voice_ids: HashSet<VoiceId> = current.voices.keys().copied().collect();
    diff.voices = diff_entities(&current_voice_ids, &new.voices, |id| {
        current.voices.get(id).map(|v| {
            let mut config = v.config.clone();
            if let Some(script_params) = current.script_voice_params.get(id) {
                config.params = script_params.clone();
            }
            config
        })
    });

    // Patterns
    let current_pattern_ids: HashSet<PatternId> = current
        .patterns
        .iter()
        .filter_map(|(id, pattern)| {
            (pattern.owner == crate::state::PatternOwner::Script).then_some(*id)
        })
        .collect();
    diff.patterns = diff_entities(&current_pattern_ids, &new.patterns, |id| {
        current.patterns.get(id).map(|p| p.config())
    });

    // Melodies
    let current_melody_ids: HashSet<MelodyId> = current.melodies.keys().copied().collect();
    tracing::debug!(
        "calculate_diff: melodies - current={}, new={}",
        current_melody_ids.len(),
        new.melodies.len()
    );
    for (id, config) in &new.melodies {
        tracing::debug!(
            "  new melody {:?} '{}': voice={:?}, notes={}, length={}",
            id,
            config.name,
            config.voice,
            config.notes.len(),
            config.length.to_f64()
        );
    }
    diff.melodies = diff_entities(&current_melody_ids, &new.melodies, |id| {
        current.melodies.get(id).map(|m| m.config())
    });
    tracing::debug!(
        "calculate_diff: melody diff - created={}, updated={}, deleted={}",
        diff.melodies.created.len(),
        diff.melodies.updated.len(),
        diff.melodies.deleted.len()
    );

    // Sequences
    let current_sequence_ids: HashSet<SequenceId> = current.sequences.keys().copied().collect();
    diff.sequences = diff_entities(&current_sequence_ids, &new.sequences, |id| {
        current.sequences.get(id).map(|s| s.config.clone())
    });

    // Effects — script-snapshot params, same reasoning as groups/voices.
    let current_effect_ids: HashSet<EffectId> = current.effects.keys().copied().collect();
    diff.effects = diff_entities(&current_effect_ids, &new.effects, |id| {
        current.effects.get(id).map(|e| EffectConfig {
            group: e.group,
            synthdef: e.synthdef.clone(),
            params: current
                .script_effect_params
                .get(id)
                .cloned()
                .unwrap_or_else(|| e.params.clone()),
        })
    });

    // Samples
    let current_sample_ids: HashSet<SampleId> = current.samples.keys().copied().collect();
    diff.samples = diff_entities(&current_sample_ids, &new.samples, |id| {
        current
            .samples
            .get(id)
            .map(|s| SampleConfig::new(s.path.clone()))
    });

    // Script-allocated buffers
    let current_buffer_ids: HashSet<BufferId> = current.buffers.keys().copied().collect();
    diff.buffers = diff_entities(&current_buffer_ids, &new.buffers, |id| {
        current.buffers.get(id).cloned()
    });

    // SFZ instruments
    let current_sfz_ids: HashSet<SfzId> = current.sfz_instruments.keys().copied().collect();
    diff.sfz = diff_entities(&current_sfz_ids, &new.sfz_instruments, |id| {
        current
            .sfz_instruments
            .get(id)
            .map(|s| crate::traits::SfzConfig::new(s.path.clone()))
    });

    // Fades
    // We diff against the runtime's tracked fade configs (stored in state.fade_configs).
    // Active fades in state.active_fades are the runtime execution state, not the config.
    let current_fade_ids: HashSet<FadeId> = current.fade_configs.keys().copied().collect();
    diff.fades = diff_entities(&current_fade_ids, &new.fades, |id| {
        current.fade_configs.get(id).cloned()
    });

    // Force-restart fades should be treated as updated even if config is unchanged
    for id in &new.force_restart_fades {
        if diff.fades.unchanged.remove(id) {
            if let Some(config) = new.fades.get(id) {
                diff.fades.updated.insert(*id, config.clone());
            }
        }
    }

    diff.voice_roles = calculate_voice_roles(current, new);
    diff.voice_role_changes = diff
        .voice_roles
        .iter()
        .filter(|(id, role)| {
            current
                .voices
                .get(id)
                .map(|voice| voice.role != **role)
                .unwrap_or(false)
        })
        .count();
    diff.effective_output_routes = effective_output_routes(current, new, &diff.voice_roles);
    diff.output_routes = diff::diff_routes(current_routes, &diff.effective_output_routes);
    diff.param_routes_set = diff::diff_param_routes_with_shaping(
        &current.param_routes_set,
        &new.param_routes_set,
        &current.param_route_set_shaping,
        &new.param_route_set_shaping,
    );
    diff.param_routes_bend = diff::diff_param_routes_with_shaping(
        &current.param_routes_bend,
        &new.param_routes_bend,
        &current.param_route_bend_shaping,
        &new.param_route_bend_shaping,
    );
    diff.param_routes_trigger =
        diff::diff_param_routes(&current.param_routes_trigger, &new.param_routes_trigger);
    diff.input_routes = compute_input_route_diff(&current.input_routes, &new.input_routes);

    diff
}

/// Order group deletions so children are deleted before parents.
///
/// This prevents errors when trying to delete a group that still has children.
pub fn order_group_deletions(
    groups: &std::collections::HashMap<GroupId, crate::state::GroupState>,
    to_delete: &[GroupId],
) -> Vec<GroupId> {
    let delete_set: HashSet<GroupId> = to_delete.iter().copied().collect();

    // Build parent -> children relationships
    let mut children_of: std::collections::HashMap<GroupId, Vec<GroupId>> =
        std::collections::HashMap::new();
    for (&id, group) in groups {
        if let Some(parent) = group.parent {
            children_of.entry(parent).or_default().push(id);
        }
    }

    let mut ordered = Vec::new();
    let mut remaining: HashSet<GroupId> = delete_set.clone();

    // Keep iterating until all are processed
    while !remaining.is_empty() {
        let mut batch = Vec::new();

        for &id in &remaining {
            // Can delete if all children in delete_set have already been deleted
            let children = children_of.get(&id).map(|c| c.as_slice()).unwrap_or(&[]);
            let all_children_deleted = children.iter().all(|child_id| {
                // Either child is not in delete set, or child is not remaining
                !delete_set.contains(child_id) || !remaining.contains(child_id)
            });

            if all_children_deleted {
                batch.push(id);
            }
        }

        // If no progress made, there's a cycle (shouldn't happen)
        if batch.is_empty() && !remaining.is_empty() {
            // Just add remaining in any order
            batch.extend(remaining.iter().copied());
        }

        // Story 4: deterministic intra-batch order. `remaining` is a HashSet so
        // its iteration order varies per process. Sort by `GroupId::raw()` (a
        // stable FNV-1a hash of the group path) so the deletion order — and any
        // bus IDs the BusAllocator hands back to the free pool — are byte
        // identical across reloads.
        batch.sort_by_key(|id| id.raw());

        for id in &batch {
            remaining.remove(id);
        }
        ordered.extend(batch);
    }

    ordered
}

/// Order group creations so parents are created before children.
///
/// This ensures parent groups exist before children try to reference them.
pub fn order_group_creations(
    configs: &std::collections::HashMap<GroupId, GroupConfig>,
) -> Vec<GroupId> {
    let all_ids: HashSet<GroupId> = configs.keys().copied().collect();
    let mut ordered = Vec::new();
    let mut remaining: HashSet<GroupId> = all_ids.clone();

    while !remaining.is_empty() {
        let mut batch = Vec::new();

        for &id in &remaining {
            if let Some(config) = configs.get(&id) {
                // Can create if:
                // - No parent, or
                // - Parent not being created, or
                // - Parent already created
                let can_create = config.parent.is_none_or(|parent_id| {
                    !all_ids.contains(&parent_id) || !remaining.contains(&parent_id)
                });

                if can_create {
                    batch.push(id);
                }
            }
        }

        // If no progress made, there's a cycle
        if batch.is_empty() && !remaining.is_empty() {
            // Cycle detected - add remaining anyway (will error on apply)
            batch.extend(remaining.iter().copied());
        }

        // Story 4: deterministic intra-batch order. Without this, the order of
        // root groups (and any siblings sharing a parent) depends on `HashSet`
        // iteration, which Rust randomises per process. That non-determinism
        // propagates straight into `state.alloc_bus_id()` calls inside
        // `groups.create()` — so two reboots of the same script can flip which
        // root group lands at which audio bus. Sorting by `GroupId::raw()` (a
        // stable FNV-1a hash of the group path) makes bus assignments byte
        // identical across processes, fixing the "drums sometimes worked,
        // sometimes didn't" bus collision in the studio script.
        batch.sort_by_key(|id| id.raw());

        for id in &batch {
            remaining.remove(id);
        }
        ordered.extend(batch);
    }

    ordered
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::state::{GroupState, PatternOwner, PatternState};
    use crate::traits::PatternConfig;
    use crate::types::{BusId, NodeId, ParamMap};

    fn make_group_state(id: GroupId, parent: Option<GroupId>) -> GroupState {
        GroupState {
            name: String::new(),
            id,
            parent,
            node_id: NodeId::new(id.0 + 100),
            audio_bus: BusId::new(id.0 + 16),
            link_synth_node_id: None,
            muted: false,
            soloed: false,
            params: ParamMap::new(),
            output_bus: None,
            output_channels: None,
        }
    }

    #[test]
    fn test_calculate_diff_tempo() {
        let current = State::default(); // tempo = 120
        let new = ScriptState::new().with_tempo(140.0);

        let diff = calculate_diff(&current, &new, &RouteMap::new());

        assert_eq!(diff.tempo_changed, Some(140.0));
    }

    #[test]
    fn test_calculate_diff_no_changes() {
        let current = State::default();
        let new = ScriptState::new(); // Same tempo

        let diff = calculate_diff(&current, &new, &RouteMap::new());

        assert!(!diff.has_changes());
    }

    #[test]
    fn looper_owned_patterns_survive_unrelated_reload_diff() {
        let looper_a = PatternId::new(101);
        let looper_b = PatternId::new(202);
        let script_pattern = PatternId::new(303);
        let mut current = State::default();

        let mut pattern_a = PatternState::with_owner(
            looper_a,
            PatternConfig::with_length("__looper_1_1", VoiceId::new(1), 4.0),
            PatternOwner::Looper,
        );
        pattern_a.playing = true;
        let mut pattern_b = PatternState::with_owner(
            looper_b,
            PatternConfig::with_length("__looper_2_1", VoiceId::new(2), 4.0),
            PatternOwner::Looper,
        );
        pattern_b.playing = true;
        let mut script = PatternState::new(
            script_pattern,
            PatternConfig::with_length("script_pattern", VoiceId::new(3), 4.0),
        );
        script.playing = true;

        current.patterns.insert(looper_a, pattern_a);
        current.patterns.insert(looper_b, pattern_b);
        current.patterns.insert(script_pattern, script);

        let diff = calculate_diff(&current, &ScriptState::new(), &RouteMap::new());

        assert_eq!(diff.patterns.deleted, vec![script_pattern]);
        assert!(current.patterns[&looper_a].playing);
        assert!(current.patterns[&looper_b].playing);
    }

    #[test]
    fn test_order_group_deletions_no_parent() {
        let mut groups = std::collections::HashMap::new();
        groups.insert(GroupId::new(1), make_group_state(GroupId::new(1), None));
        groups.insert(GroupId::new(2), make_group_state(GroupId::new(2), None));

        let to_delete = vec![GroupId::new(1), GroupId::new(2)];
        let ordered = order_group_deletions(&groups, &to_delete);

        assert_eq!(ordered.len(), 2);
    }

    #[test]
    fn test_order_group_deletions_child_first() {
        let mut groups = std::collections::HashMap::new();
        groups.insert(GroupId::new(1), make_group_state(GroupId::new(1), None));
        groups.insert(
            GroupId::new(2),
            make_group_state(GroupId::new(2), Some(GroupId::new(1))),
        );

        let to_delete = vec![GroupId::new(1), GroupId::new(2)];
        let ordered = order_group_deletions(&groups, &to_delete);

        // Child (2) should come before parent (1)
        let pos_1 = ordered
            .iter()
            .position(|&id| id == GroupId::new(1))
            .unwrap();
        let pos_2 = ordered
            .iter()
            .position(|&id| id == GroupId::new(2))
            .unwrap();
        assert!(pos_2 < pos_1);
    }

    #[test]
    fn test_order_group_creations_parent_first() {
        let mut configs = std::collections::HashMap::new();
        configs.insert(GroupId::new(1), GroupConfig::default());
        configs.insert(
            GroupId::new(2),
            GroupConfig {
                name: "child".to_string(),
                parent: Some(GroupId::new(1)),
                params: ParamMap::new(),
                effects: Vec::new(),
                muted: false,
                soloed: false,
                output_bus: None,
                output_channels: None,
            },
        );

        let ordered = order_group_creations(&configs);

        // Parent (1) should come before child (2)
        let pos_1 = ordered
            .iter()
            .position(|&id| id == GroupId::new(1))
            .unwrap();
        let pos_2 = ordered
            .iter()
            .position(|&id| id == GroupId::new(2))
            .unwrap();
        assert!(pos_1 < pos_2);
    }

    #[test]
    fn test_order_group_creations_deep_hierarchy() {
        let mut configs = std::collections::HashMap::new();
        configs.insert(GroupId::new(1), GroupConfig::default());
        configs.insert(
            GroupId::new(2),
            GroupConfig {
                name: "child".to_string(),
                parent: Some(GroupId::new(1)),
                params: ParamMap::new(),
                effects: Vec::new(),
                muted: false,
                soloed: false,
                output_bus: None,
                output_channels: None,
            },
        );
        configs.insert(
            GroupId::new(3),
            GroupConfig {
                name: "grandchild".to_string(),
                parent: Some(GroupId::new(2)),
                params: ParamMap::new(),
                effects: Vec::new(),
                muted: false,
                soloed: false,
                output_bus: None,
                output_channels: None,
            },
        );

        let ordered = order_group_creations(&configs);

        let pos_1 = ordered
            .iter()
            .position(|&id| id == GroupId::new(1))
            .unwrap();
        let pos_2 = ordered
            .iter()
            .position(|&id| id == GroupId::new(2))
            .unwrap();
        let pos_3 = ordered
            .iter()
            .position(|&id| id == GroupId::new(3))
            .unwrap();

        assert!(pos_1 < pos_2);
        assert!(pos_2 < pos_3);
    }
}
