//! Diff calculation for live reloading.
//!
//! This module provides types and functions for calculating the difference
//! between the current runtime state and a new script state.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

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
/// * `get_current_config` - Function to get current config for comparison
///
/// # Returns
/// An EntityDiff with created, deleted, updated, and unchanged entities.
pub fn diff_entities<Id, Config, F>(
    current_ids: &HashSet<Id>,
    new_configs: &HashMap<Id, Config>,
    get_current_config: F,
) -> EntityDiff<Id, Config>
where
    Id: Clone + Eq + Hash,
    Config: PartialEq + Clone,
    F: Fn(&Id) -> Option<Config>,
{
    let mut diff = EntityDiff::new();

    // Check each new entity
    for (id, new_config) in new_configs {
        if current_ids.contains(id) {
            // Entity exists - check if config changed
            if let Some(current_config) = get_current_config(id) {
                if current_config == *new_config {
                    diff.unchanged.insert(id.clone());
                } else {
                    diff.updated.insert(id.clone(), new_config.clone());
                }
            } else {
                // Shouldn't happen, but treat as create
                diff.created.insert(id.clone(), new_config.clone());
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
        }
    }
}

impl ReloadDiff {
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

        let diff = diff_entities(&current_ids, &new_configs, |_| None::<String>);

        assert_eq!(diff.created.len(), 1);
        assert!(diff.deleted.is_empty());
        assert!(diff.updated.is_empty());
    }

    #[test]
    fn test_diff_entities_delete() {
        let mut current_ids: HashSet<u32> = HashSet::new();
        current_ids.insert(1);

        let new_configs: HashMap<u32, String> = HashMap::new();

        let diff = diff_entities(&current_ids, &new_configs, |_| Some("old".to_string()));

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

        let diff = diff_entities(&current_ids, &new_configs, |_| Some("old_config".to_string()));

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

        let diff = diff_entities(&current_ids, &new_configs, |_| Some("same_config".to_string()));

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
}
