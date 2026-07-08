//! Diff calculation for live reloading.
//!
//! This module provides types and functions for calculating the difference
//! between the current runtime state and a new script state.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::handlers::{
    InputRouteDiff, ParamRoute, ParamRouteDiff, ParamRouteMap, ParamRouteTarget, Route, RouteDiff,
    RouteMap,
};
use crate::types::VoiceId;

/// Result of diffing an entity type.
///
/// Contains lists of entities to create, delete, and update.
#[derive(Clone, Debug, Default)]
pub struct EntityDiff<Id, Config> {
    /// Entities to create (new in script, not in runtime).
    pub created: HashMap<Id, Config>,

    /// Entities to delete (in runtime, not in script).
    pub deleted: Vec<Id>,

    /// Entities with changed config (in both, but config differs).
    pub updated: HashMap<Id, Config>,

    /// Entities unchanged (in both, config same).
    pub unchanged: HashSet<Id>,
}

impl<Id: Clone + Eq + Hash, Config: PartialEq + Clone> EntityDiff<Id, Config> {
    /// Create a new empty diff.
    pub fn new() -> Self {
        Self {
            created: HashMap::new(),
            deleted: Vec::new(),
            updated: HashMap::new(),
            unchanged: HashSet::new(),
        }
    }

    /// Check if there are any changes.
    pub fn has_changes(&self) -> bool {
        !self.created.is_empty() || !self.deleted.is_empty() || !self.updated.is_empty()
    }

    /// Total number of changes (creates + deletes + updates).
    pub fn change_count(&self) -> usize {
        self.created.len() + self.deleted.len() + self.updated.len()
    }
}

/// Calculate diff between current and new entity maps.
///
/// # Arguments
/// * `current_ids` - Set of IDs currently in the runtime
/// * `new_configs` - Map of ID -> Config from the new script state
/// * `probe_current` - In-place equality probe against the current runtime
///   entity. Given `(id, new_config)`, returns `Some(true)` when an existing
///   current entity **equals** the proposed config, `Some(false)` when it
///   exists but **differs**, and `None` when it does not exist (treated as a
///   create). Unlike a clone-and-compare, this must decide equality by
///   *borrowing* current state — no owned `Config` is materialized just to be
///   thrown away, which on a hot-reload save avoids deep-cloning every
///   pattern/melody/voice config that did not change.
///
/// # Returns
/// An EntityDiff with created, deleted, updated, and unchanged entities.
pub fn diff_entities<Id, Config, F>(
    current_ids: &HashSet<Id>,
    new_configs: &HashMap<Id, Config>,
    probe_current: F,
) -> EntityDiff<Id, Config>
where
    Id: Clone + Eq + Hash,
    Config: PartialEq + Clone,
    F: Fn(&Id, &Config) -> Option<bool>,
{
    let mut diff = EntityDiff::new();

    // Check each new entity
    for (id, new_config) in new_configs {
        if current_ids.contains(id) {
            // Entity exists - probe whether its live config matches in place.
            match probe_current(id, new_config) {
                Some(true) => {
                    diff.unchanged.insert(id.clone());
                }
                Some(false) => {
                    diff.updated.insert(id.clone(), new_config.clone());
                }
                None => {
                    // Shouldn't happen (id in current_ids but no entity), but
                    // treat as create to match the previous behaviour.
                    diff.created.insert(id.clone(), new_config.clone());
                }
            }
        } else {
            // New entity
            diff.created.insert(id.clone(), new_config.clone());
        }
    }

    // Check for deleted entities
    for id in current_ids {
        if !new_configs.contains_key(id) {
            diff.deleted.push(id.clone());
        }
    }

    diff
}

/// Compute the additions and removals between two route maps at
/// `(voice, port, dest)`-edge granularity.
pub fn diff_routes(old: &RouteMap, new: &RouteMap) -> RouteDiff {
    let mut diff = RouteDiff::default();

    for ((voice_id, port_name), new_dests) in new {
        let old_dests = old.get(&(*voice_id, port_name.clone()));
        for d in new_dests {
            let in_old = old_dests.map(|v| v.iter().any(|x| x == d)).unwrap_or(false);
            if !in_old {
                diff.additions.push(Route {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    dest: d.clone(),
                });
            }
        }
    }

    for ((voice_id, port_name), old_dests) in old {
        let new_dests = new.get(&(*voice_id, port_name.clone()));
        for d in old_dests {
            let in_new = new_dests.map(|v| v.iter().any(|x| x == d)).unwrap_or(false);
            if !in_new {
                diff.removals.push(Route {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    dest: d.clone(),
                });
            }
        }
    }

    diff
}

/// Compute the additions and removals between two Param-route maps.
pub fn diff_param_routes(old: &ParamRouteMap, new: &ParamRouteMap) -> ParamRouteDiff {
    let mut diff = ParamRouteDiff::default();

    for ((src_voice, src_port), new_targets) in new {
        let old_targets = old.get(&(*src_voice, src_port.clone()));
        for (tgt, tgt_param) in new_targets {
            let in_old = old_targets
                .map(|v| v.iter().any(|t| t.0 == *tgt && t.1 == *tgt_param))
                .unwrap_or(false);
            if !in_old {
                diff.additions.push(ParamRoute {
                    source_voice: *src_voice,
                    source_port: src_port.clone(),
                    target: *tgt,
                    target_param: tgt_param.clone(),
                });
            }
        }
    }

    for ((src_voice, src_port), old_targets) in old {
        let new_targets = new.get(&(*src_voice, src_port.clone()));
        for (tgt, tgt_param) in old_targets {
            let in_new = new_targets
                .map(|v| v.iter().any(|t| t.0 == *tgt && t.1 == *tgt_param))
                .unwrap_or(false);
            if !in_new {
                diff.removals.push(ParamRoute {
                    source_voice: *src_voice,
                    source_port: src_port.clone(),
                    target: *tgt,
                    target_param: tgt_param.clone(),
                });
            }
        }
    }

    diff
}

/// Compute param-route changes, including per-source scale/offset edits
/// for routes whose source/target edge did not otherwise change.
pub fn diff_param_routes_with_shaping(
    old: &ParamRouteMap,
    new: &ParamRouteMap,
    old_shaping: &HashMap<(VoiceId, String, ParamRouteTarget, String), (f32, f32)>,
    new_shaping: &HashMap<(VoiceId, String, ParamRouteTarget, String), (f32, f32)>,
) -> ParamRouteDiff {
    let mut diff = diff_param_routes(old, new);

    for ((src_voice, src_port), new_targets) in new {
        let old_targets = old.get(&(*src_voice, src_port.clone()));
        for (target, target_param) in new_targets {
            let in_old = old_targets
                .map(|targets| {
                    targets.iter().any(|(old_target, old_param)| {
                        *old_target == *target && *old_param == *target_param
                    })
                })
                .unwrap_or(false);
            if !in_old {
                continue;
            }

            let key = (*src_voice, src_port.clone(), *target, target_param.clone());
            let old_shape = old_shaping.get(&key).copied().unwrap_or((1.0, 0.0));
            let new_shape = new_shaping.get(&key).copied().unwrap_or((1.0, 0.0));
            if old_shape != new_shape {
                diff.updates.push(ParamRoute {
                    source_voice: *src_voice,
                    source_port: src_port.clone(),
                    target: *target,
                    target_param: target_param.clone(),
                });
            }
        }
    }

    diff
}

/// Parameter-level diff for in-place updates.
#[derive(Clone, Debug, Default)]
pub struct ParamDiff {
    /// New parameters (not in old).
    pub added: HashMap<String, f32>,

    /// Removed parameters (in old, not in new).
    pub removed: HashSet<String>,

    /// Changed parameters (in both, value differs).
    pub changed: HashMap<String, f32>,
}

impl ParamDiff {
    /// Check if there are any parameter changes.
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.changed.is_empty()
    }

    /// Calculate diff between two parameter maps.
    pub fn diff(old: &HashMap<String, f32>, new: &HashMap<String, f32>) -> Self {
        let mut diff = ParamDiff::default();

        // Check new params
        for (key, &new_value) in new {
            if let Some(&old_value) = old.get(key) {
                if (old_value - new_value).abs() > f32::EPSILON {
                    diff.changed.insert(key.clone(), new_value);
                }
            } else {
                diff.added.insert(key.clone(), new_value);
            }
        }

        // Check removed params
        for key in old.keys() {
            if !new.contains_key(key) {
                diff.removed.insert(key.clone());
            }
        }

        diff
    }
}

/// Complete diff result for a reload operation.
#[derive(Clone, Debug)]
pub struct ReloadDiff {
    /// Whether tempo changed.
    pub tempo_changed: Option<f64>,

    /// Whether time signature changed.
    pub time_sig_changed: Option<crate::types::TimeSignature>,

    /// Group changes.
    pub groups: EntityDiff<crate::types::GroupId, super::script_state::GroupConfig>,

    /// Voice changes.
    pub voices: EntityDiff<crate::types::VoiceId, crate::traits::VoiceConfig>,

    /// Pattern changes.
    pub patterns: EntityDiff<crate::types::PatternId, crate::traits::PatternConfig>,

    /// Melody changes.
    pub melodies: EntityDiff<crate::types::MelodyId, crate::traits::MelodyConfig>,

    /// Sequence changes.
    pub sequences: EntityDiff<crate::types::SequenceId, crate::traits::SequenceConfig>,

    /// Effect changes.
    pub effects: EntityDiff<crate::types::EffectId, super::script_state::EffectConfig>,

    /// Sample changes.
    pub samples: EntityDiff<crate::types::SampleId, crate::traits::SampleConfig>,

    /// Script-allocated buffer changes.
    pub buffers: EntityDiff<crate::types::BufferId, crate::traits::BufferConfig>,

    /// SFZ instrument changes.
    pub sfz: EntityDiff<crate::types::SfzId, crate::traits::SfzConfig>,

    /// Fade changes.
    pub fades: EntityDiff<crate::types::FadeId, crate::traits::FadeConfig>,

    /// Output route changes.
    pub output_routes: RouteDiff,

    /// Effective output route map computed during diffing.
    pub effective_output_routes: crate::handlers::RouteMap,

    /// Desired voice roles computed during diffing.
    pub voice_roles: HashMap<crate::types::VoiceId, crate::state::VoiceRole>,

    /// Existing voices whose derived role changed.
    pub voice_role_changes: usize,

    /// SET parameter route changes.
    pub param_routes_set: ParamRouteDiff,

    /// BEND parameter route changes.
    pub param_routes_bend: ParamRouteDiff,

    /// TRIGGER parameter route changes.
    pub param_routes_trigger: ParamRouteDiff,

    /// Named input route changes.
    pub input_routes: InputRouteDiff,

    /// Pending voice output-port reconciles.
    pub voice_port_reconciles: usize,

    /// MIDI route, clock-output, or looper config changes.
    #[cfg(feature = "midi")]
    pub midi_routes_changed: bool,
}

impl Default for ReloadDiff {
    fn default() -> Self {
        Self {
            tempo_changed: None,
            time_sig_changed: None,
            groups: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            voices: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            patterns: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            melodies: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            sequences: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            effects: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            samples: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            buffers: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            sfz: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            fades: EntityDiff {
                created: HashMap::new(),
                deleted: Vec::new(),
                updated: HashMap::new(),
                unchanged: HashSet::new(),
            },
            output_routes: RouteDiff::default(),
            effective_output_routes: crate::handlers::RouteMap::new(),
            voice_roles: HashMap::new(),
            voice_role_changes: 0,
            param_routes_set: ParamRouteDiff::default(),
            param_routes_bend: ParamRouteDiff::default(),
            param_routes_trigger: ParamRouteDiff::default(),
            input_routes: InputRouteDiff::default(),
            voice_port_reconciles: 0,
            #[cfg(feature = "midi")]
            midi_routes_changed: false,
        }
    }
}

impl ReloadDiff {
    /// Check if there are any parameter-route changes.
    pub fn param_routes_have_changes(&self) -> bool {
        !self.param_routes_set.is_empty()
            || !self.param_routes_bend.is_empty()
            || !self.param_routes_trigger.is_empty()
    }

    /// Check if there are any changes.
    pub fn has_changes(&self) -> bool {
        self.tempo_changed.is_some()
            || self.time_sig_changed.is_some()
            || self.groups.has_changes()
            || self.voices.has_changes()
            || self.patterns.has_changes()
            || self.melodies.has_changes()
            || self.sequences.has_changes()
            || self.effects.has_changes()
            || self.samples.has_changes()
            || self.buffers.has_changes()
            || self.sfz.has_changes()
            || self.fades.has_changes()
            || !self.output_routes.is_empty()
            || self.voice_role_changes > 0
            || self.param_routes_have_changes()
            || !self.input_routes.is_empty()
            || self.voice_port_reconciles > 0
            || {
                #[cfg(feature = "midi")]
                {
                    self.midi_routes_changed
                }
                #[cfg(not(feature = "midi"))]
                {
                    false
                }
            }
    }

    /// Get a summary of changes for logging.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if self.tempo_changed.is_some() {
            parts.push("tempo".to_string());
        }
        if self.time_sig_changed.is_some() {
            parts.push("time_sig".to_string());
        }
        if self.groups.has_changes() {
            parts.push(format!(
                "groups(+{} -{} ~{})",
                self.groups.created.len(),
                self.groups.deleted.len(),
                self.groups.updated.len()
            ));
        }
        if self.voices.has_changes() {
            parts.push(format!(
                "voices(+{} -{} ~{})",
                self.voices.created.len(),
                self.voices.deleted.len(),
                self.voices.updated.len()
            ));
        }
        if self.patterns.has_changes() {
            parts.push(format!(
                "patterns(+{} -{} ~{})",
                self.patterns.created.len(),
                self.patterns.deleted.len(),
                self.patterns.updated.len()
            ));
        }
        if self.melodies.has_changes() {
            parts.push(format!(
                "melodies(+{} -{} ~{})",
                self.melodies.created.len(),
                self.melodies.deleted.len(),
                self.melodies.updated.len()
            ));
        }
        if self.sequences.has_changes() {
            parts.push(format!(
                "sequences(+{} -{} ~{})",
                self.sequences.created.len(),
                self.sequences.deleted.len(),
                self.sequences.updated.len()
            ));
        }
        if self.effects.has_changes() {
            parts.push(format!(
                "effects(+{} -{} ~{})",
                self.effects.created.len(),
                self.effects.deleted.len(),
                self.effects.updated.len()
            ));
        }
        if self.samples.has_changes() {
            parts.push(format!(
                "samples(+{} -{} ~{})",
                self.samples.created.len(),
                self.samples.deleted.len(),
                self.samples.updated.len()
            ));
        }
        if self.buffers.has_changes() {
            parts.push(format!(
                "buffers(+{} -{} ~{})",
                self.buffers.created.len(),
                self.buffers.deleted.len(),
                self.buffers.updated.len()
            ));
        }
        if self.sfz.has_changes() {
            parts.push(format!(
                "sfz(+{} -{} ~{})",
                self.sfz.created.len(),
                self.sfz.deleted.len(),
                self.sfz.updated.len()
            ));
        }
        if self.fades.has_changes() {
            parts.push(format!(
                "fades(+{} -{} ~{})",
                self.fades.created.len(),
                self.fades.deleted.len(),
                self.fades.updated.len()
            ));
        }
        if !self.output_routes.is_empty() {
            parts.push(format!(
                "output_routes(+{} -{})",
                self.output_routes.additions.len(),
                self.output_routes.removals.len()
            ));
        }
        if self.voice_role_changes > 0 {
            parts.push(format!("voice_roles(~{})", self.voice_role_changes));
        }
        if !self.param_routes_set.is_empty() {
            parts.push(format!(
                "param_routes_set(+{} -{} ~{})",
                self.param_routes_set.additions.len(),
                self.param_routes_set.removals.len(),
                self.param_routes_set.updates.len()
            ));
        }
        if !self.param_routes_bend.is_empty() {
            parts.push(format!(
                "param_routes_bend(+{} -{} ~{})",
                self.param_routes_bend.additions.len(),
                self.param_routes_bend.removals.len(),
                self.param_routes_bend.updates.len()
            ));
        }
        if !self.param_routes_trigger.is_empty() {
            parts.push(format!(
                "param_routes_trigger(+{} -{} ~{})",
                self.param_routes_trigger.additions.len(),
                self.param_routes_trigger.removals.len(),
                self.param_routes_trigger.updates.len()
            ));
        }
        if !self.input_routes.is_empty() {
            parts.push(format!(
                "input_routes(+{} -{})",
                self.input_routes.additions.len(),
                self.input_routes.removals.len()
            ));
        }
        if self.voice_port_reconciles > 0 {
            parts.push(format!(
                "voice_port_reconciles({})",
                self.voice_port_reconciles
            ));
        }
        #[cfg(feature = "midi")]
        if self.midi_routes_changed {
            parts.push("midi_routes".to_string());
        }

        if parts.is_empty() {
            "no changes".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::{InputRoute, InputRouteSrc, ParamRouteTarget, RouteDest};
    use crate::types::{GroupId, VoiceId};

    #[test]
    fn test_entity_diff_empty() {
        let diff: EntityDiff<u32, String> = EntityDiff::new();
        assert!(!diff.has_changes());
        assert_eq!(diff.change_count(), 0);
    }

    #[test]
    fn test_entity_diff_has_changes() {
        let mut diff: EntityDiff<u32, String> = EntityDiff::new();
        diff.created.insert(1, "test".to_string());
        assert!(diff.has_changes());
        assert_eq!(diff.change_count(), 1);
    }

    #[test]
    fn test_diff_entities_create() {
        let current_ids: HashSet<u32> = HashSet::new();
        let mut new_configs = HashMap::new();
        new_configs.insert(1u32, "config1".to_string());

        let diff = diff_entities(&current_ids, &new_configs, |_, _: &String| None);

        assert_eq!(diff.created.len(), 1);
        assert!(diff.deleted.is_empty());
        assert!(diff.updated.is_empty());
    }

    #[test]
    fn test_diff_entities_delete() {
        let mut current_ids: HashSet<u32> = HashSet::new();
        current_ids.insert(1);

        let new_configs: HashMap<u32, String> = HashMap::new();

        // New map is empty, so the probe is never invoked; deletion is decided
        // purely from `current_ids`.
        let diff = diff_entities(&current_ids, &new_configs, |_, _: &String| Some(true));

        assert!(diff.created.is_empty());
        assert_eq!(diff.deleted.len(), 1);
        assert!(diff.updated.is_empty());
    }

    #[test]
    fn test_diff_entities_update() {
        let mut current_ids: HashSet<u32> = HashSet::new();
        current_ids.insert(1);

        let mut new_configs = HashMap::new();
        new_configs.insert(1u32, "new_config".to_string());

        // Current "old_config" differs from new "new_config": probe reports false.
        let diff = diff_entities(&current_ids, &new_configs, |_, new: &String| {
            Some(*new == "old_config")
        });

        assert!(diff.created.is_empty());
        assert!(diff.deleted.is_empty());
        assert_eq!(diff.updated.len(), 1);
    }

    #[test]
    fn test_diff_entities_unchanged() {
        let mut current_ids: HashSet<u32> = HashSet::new();
        current_ids.insert(1);

        let mut new_configs = HashMap::new();
        new_configs.insert(1u32, "same_config".to_string());

        // Current equals new "same_config": probe reports true (unchanged).
        let diff = diff_entities(&current_ids, &new_configs, |_, new: &String| {
            Some(*new == "same_config")
        });

        assert!(diff.created.is_empty());
        assert!(diff.deleted.is_empty());
        assert!(diff.updated.is_empty());
        assert_eq!(diff.unchanged.len(), 1);
    }

    #[test]
    fn test_param_diff_no_changes() {
        let mut old = HashMap::new();
        old.insert("freq".to_string(), 440.0f32);

        let mut new = HashMap::new();
        new.insert("freq".to_string(), 440.0f32);

        let diff = ParamDiff::diff(&old, &new);
        assert!(!diff.has_changes());
    }

    #[test]
    fn test_param_diff_added() {
        let old = HashMap::new();

        let mut new = HashMap::new();
        new.insert("freq".to_string(), 440.0f32);

        let diff = ParamDiff::diff(&old, &new);
        assert!(diff.has_changes());
        assert_eq!(diff.added.len(), 1);
    }

    #[test]
    fn test_param_diff_removed() {
        let mut old = HashMap::new();
        old.insert("freq".to_string(), 440.0f32);

        let new = HashMap::new();

        let diff = ParamDiff::diff(&old, &new);
        assert!(diff.has_changes());
        assert_eq!(diff.removed.len(), 1);
    }

    #[test]
    fn test_param_diff_changed() {
        let mut old = HashMap::new();
        old.insert("freq".to_string(), 440.0f32);

        let mut new = HashMap::new();
        new.insert("freq".to_string(), 880.0f32);

        let diff = ParamDiff::diff(&old, &new);
        assert!(diff.has_changes());
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed.get("freq"), Some(&880.0f32));
    }

    #[test]
    fn test_reload_diff_summary() {
        let diff = ReloadDiff::default();
        assert_eq!(diff.summary(), "no changes");
    }

    #[test]
    fn reload_diff_has_changes_for_output_route_only_change() {
        let mut diff = ReloadDiff::default();
        diff.output_routes.additions.push(Route {
            voice_id: VoiceId::new(1),
            port_name: "out".to_string(),
            dest: RouteDest::Group(GroupId::new(2)),
        });

        assert!(diff.has_changes());
    }

    #[test]
    fn reload_diff_has_changes_for_param_route_only_change() {
        let mut diff = ReloadDiff::default();
        diff.param_routes_set.additions.push(ParamRoute {
            source_voice: VoiceId::new(1),
            source_port: "env".to_string(),
            target: ParamRouteTarget::Voice(VoiceId::new(2)),
            target_param: "cutoff".to_string(),
        });

        assert!(diff.has_changes());
    }

    #[test]
    fn reload_diff_has_changes_for_input_route_only_change() {
        let mut diff = ReloadDiff::default();
        diff.input_routes.additions.push(InputRoute {
            voice_id: VoiceId::new(2),
            port_name: "in".to_string(),
            src: InputRouteSrc::Voice(VoiceId::new(1), "out".to_string()),
        });

        assert!(diff.has_changes());
    }

    #[test]
    fn reload_diff_has_changes_for_port_reconcile_only_change() {
        let mut diff = ReloadDiff::default();
        diff.voice_port_reconciles = 1;

        assert!(diff.has_changes());
    }

    #[test]
    #[cfg(feature = "midi")]
    fn reload_diff_has_changes_for_midi_route_only_change() {
        let mut diff = ReloadDiff::default();
        diff.midi_routes_changed = true;

        assert!(diff.has_changes());
        assert_eq!(diff.summary(), "midi_routes");
    }

    #[test]
    fn reload_diff_has_no_changes_for_empty_diff() {
        assert!(!ReloadDiff::default().has_changes());
    }
}
