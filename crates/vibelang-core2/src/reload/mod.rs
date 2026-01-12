//! Live reloading support for vibelang-core2.
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
//! use vibelang_core2::reload::{ScriptState, GroupConfig};
//! use vibelang_core2::message::{Message, ReloadMessage};
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
mod script_state;

pub use bus_pool::BusAllocator;
pub use diff::{diff_entities, EntityDiff, ParamDiff, ReloadDiff};
pub use script_state::{EffectConfig, GroupConfig, ScriptState};

#[cfg(feature = "midi")]
pub use script_state::{
    AdvancedMidiCcRoute, AdvancedMidiKeyboardRoute, AdvancedMidiNoteRoute,
    MidiCallbackConfig, MidiCcRoute, MidiClockOutputRequest, MidiKeyboardRoute,
    MidiOutputMessage, MidiRecordingRequest,
};

// Types available on all platforms (for order_group_creations)
use crate::types::GroupId;
use std::collections::HashSet;

// Imports for calculate_diff and order_group_deletions
use crate::state::State;
use crate::traits::SampleConfig;
use crate::types::{EffectId, MelodyId, ModulatorId, PatternId, SampleId, SequenceId, VoiceId};

/// Calculate the diff between current runtime state and new script state.
pub fn calculate_diff(current: &State, new: &ScriptState) -> ReloadDiff {
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
    let current_group_ids: HashSet<GroupId> = current.groups.keys().copied().collect();
    diff.groups = diff_entities(&current_group_ids, &new.groups, |id| {
        current.groups.get(id).map(|g| GroupConfig {
            name: String::new(), // Runtime doesn't track name
            parent: g.parent,
            params: g.params.clone(),
            effects: Vec::new(), // TODO: Track effects per group in runtime state
            muted: g.muted,
            soloed: g.soloed,
        })
    });

    // Voices
    let current_voice_ids: HashSet<VoiceId> = current.voices.keys().copied().collect();
    diff.voices = diff_entities(&current_voice_ids, &new.voices, |id| {
        current.voices.get(id).map(|v| v.config.clone())
    });

    // Patterns
    let current_pattern_ids: HashSet<PatternId> = current.patterns.keys().copied().collect();
    diff.patterns = diff_entities(&current_pattern_ids, &new.patterns, |id| {
        current.patterns.get(id).map(|p| p.config.clone())
    });

    // Melodies
    let current_melody_ids: HashSet<MelodyId> = current.melodies.keys().copied().collect();
    diff.melodies = diff_entities(&current_melody_ids, &new.melodies, |id| {
        current.melodies.get(id).map(|m| m.config.clone())
    });

    // Sequences
    let current_sequence_ids: HashSet<SequenceId> = current.sequences.keys().copied().collect();
    diff.sequences = diff_entities(&current_sequence_ids, &new.sequences, |id| {
        current.sequences.get(id).map(|s| s.config.clone())
    });

    // Effects
    let current_effect_ids: HashSet<EffectId> = current.effects.keys().copied().collect();
    diff.effects = diff_entities(&current_effect_ids, &new.effects, |id| {
        current.effects.get(id).map(|e| EffectConfig {
            group: e.group,
            synthdef: e.synthdef.clone(),
            params: e.params.clone(),
        })
    });

    // Modulators
    let current_modulator_ids: HashSet<ModulatorId> = current.modulators.keys().copied().collect();
    diff.modulators = diff_entities(&current_modulator_ids, &new.modulators, |id| {
        current.modulators.get(id).map(|m| m.config.clone())
    });

    // Samples
    let current_sample_ids: HashSet<SampleId> = current.samples.keys().copied().collect();
    diff.samples = diff_entities(&current_sample_ids, &new.samples, |id| {
        current.samples.get(id).map(|s| SampleConfig::new(s.path.clone()))
    });

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
pub fn order_group_creations(configs: &std::collections::HashMap<GroupId, GroupConfig>) -> Vec<GroupId> {
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
    use crate::state::GroupState;
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
        }
    }

    #[test]
    fn test_calculate_diff_tempo() {
        let current = State::default(); // tempo = 120
        let new = ScriptState::new().with_tempo(140.0);

        let diff = calculate_diff(&current, &new);

        assert_eq!(diff.tempo_changed, Some(140.0));
    }

    #[test]
    fn test_calculate_diff_no_changes() {
        let current = State::default();
        let new = ScriptState::new(); // Same tempo

        let diff = calculate_diff(&current, &new);

        assert!(!diff.has_changes());
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
        let pos_1 = ordered.iter().position(|&id| id == GroupId::new(1)).unwrap();
        let pos_2 = ordered.iter().position(|&id| id == GroupId::new(2)).unwrap();
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
            },
        );

        let ordered = order_group_creations(&configs);

        // Parent (1) should come before child (2)
        let pos_1 = ordered.iter().position(|&id| id == GroupId::new(1)).unwrap();
        let pos_2 = ordered.iter().position(|&id| id == GroupId::new(2)).unwrap();
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
            },
        );

        let ordered = order_group_creations(&configs);

        let pos_1 = ordered.iter().position(|&id| id == GroupId::new(1)).unwrap();
        let pos_2 = ordered.iter().position(|&id| id == GroupId::new(2)).unwrap();
        let pos_3 = ordered.iter().position(|&id| id == GroupId::new(3)).unwrap();

        assert!(pos_1 < pos_2);
        assert!(pos_2 < pos_3);
    }
}
