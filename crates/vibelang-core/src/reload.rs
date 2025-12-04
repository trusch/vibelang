//! Live Reload System for VibeLang
//!
//! This module implements a robust hot-reload system for live coding that:
//! 1. **Diffs state** - Detects what actually changed using content hashing
//! 2. **Quantizes changes** - Applies removals at musical boundaries
//! 3. **Preserves unchanged** - Only touches entities that actually changed
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                      RELOAD LIFECYCLE                           │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  1. BeginReload                                                 │
//! │     └─ Capture snapshot of current state (content hashes)       │
//! │                                                                 │
//! │  2. Script Executes                                             │
//! │     └─ Entities created/updated in state                        │
//! │     └─ Updated content available immediately                    │
//! │                                                                 │
//! │  3. FinalizeReload                                              │
//! │     └─ Diff old snapshot vs new state                           │
//! │     └─ Compute change operations (ADD/UPDATE/REMOVE/KEEP)       │
//! │     └─ Queue removals for next quantization boundary            │
//! │                                                                 │
//! │  4. At Quantization Boundary (triggered by transport)           │
//! │     └─ Apply REMOVE operations (stop loops, free nodes)         │
//! │     └─ UPDATE/ADD already in state, scheduler picks up changes  │
//! │     └─ KEEP entities are untouched                              │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Key Principles
//!
//! - **Content hashing** determines if an entity actually changed
//! - **KEEP** operations mean no action (entity is identical)
//! - **UPDATE** operations mean content changed but scheduler handles naturally
//! - **REMOVE** operations clean up entities no longer in the script
//! - **Root groups** (like "main") are protected from removal

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Duration of crossfades in beats
pub const CROSSFADE_BEATS: f64 = 0.25;

/// How many beats before we apply changes (minimum lookahead)
pub const MIN_APPLY_DELAY_BEATS: f64 = 0.1;

// ============================================================================
// Content Hashing
// ============================================================================

/// Compute a content hash for any hashable value.
/// Used to detect if an entity's content actually changed.
pub fn content_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Trait for entities that can be snapshotted and compared.
pub trait Snapshottable {
    /// Get a unique identifier for this entity.
    fn id(&self) -> &str;

    /// Compute a content hash of this entity's meaningful state.
    /// Two entities with the same hash are considered identical.
    fn content_hash(&self) -> u64;
}

// ============================================================================
// Entity Snapshots
// ============================================================================

/// A snapshot of an entity's identity and content at a point in time.
#[derive(Clone, Debug)]
pub struct EntitySnapshot {
    /// Entity identifier (name/path)
    pub id: String,
    /// Content hash at snapshot time
    pub hash: u64,
    /// Entity kind for logging
    pub kind: EntityKind,
}

/// The kind of entity being tracked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Voice,
    Pattern,
    Melody,
    Sequence,
    Effect,
    Group,
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityKind::Voice => write!(f, "voice"),
            EntityKind::Pattern => write!(f, "pattern"),
            EntityKind::Melody => write!(f, "melody"),
            EntityKind::Sequence => write!(f, "sequence"),
            EntityKind::Effect => write!(f, "effect"),
            EntityKind::Group => write!(f, "group"),
        }
    }
}

// ============================================================================
// Change Operations
// ============================================================================

/// An operation to apply during reload.
#[derive(Clone, Debug)]
pub enum ChangeOp {
    /// Entity unchanged - no action needed
    Keep {
        kind: EntityKind,
        id: String,
    },
    /// New entity added
    Add {
        kind: EntityKind,
        id: String,
    },
    /// Entity content changed - needs transition
    Update {
        kind: EntityKind,
        id: String,
        old_hash: u64,
        new_hash: u64,
    },
    /// Entity removed - needs fade out
    Remove {
        kind: EntityKind,
        id: String,
    },
}

impl ChangeOp {
    pub fn kind(&self) -> EntityKind {
        match self {
            ChangeOp::Keep { kind, .. } => *kind,
            ChangeOp::Add { kind, .. } => *kind,
            ChangeOp::Update { kind, .. } => *kind,
            ChangeOp::Remove { kind, .. } => *kind,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            ChangeOp::Keep { id, .. } => id,
            ChangeOp::Add { id, .. } => id,
            ChangeOp::Update { id, .. } => id,
            ChangeOp::Remove { id, .. } => id,
        }
    }
}

// ============================================================================
// State Snapshot
// ============================================================================

/// Complete snapshot of the script state at a point in time.
#[derive(Clone, Debug, Default)]
pub struct StateSnapshot {
    pub voices: HashMap<String, EntitySnapshot>,
    pub patterns: HashMap<String, EntitySnapshot>,
    pub melodies: HashMap<String, EntitySnapshot>,
    pub sequences: HashMap<String, EntitySnapshot>,
    pub effects: HashMap<String, EntitySnapshot>,
    pub groups: HashMap<String, EntitySnapshot>,
}

impl StateSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a snapshot for an entity.
    pub fn add(&mut self, kind: EntityKind, id: String, hash: u64) {
        let snapshot = EntitySnapshot { id: id.clone(), hash, kind };
        match kind {
            EntityKind::Voice => { self.voices.insert(id, snapshot); }
            EntityKind::Pattern => { self.patterns.insert(id, snapshot); }
            EntityKind::Melody => { self.melodies.insert(id, snapshot); }
            EntityKind::Sequence => { self.sequences.insert(id, snapshot); }
            EntityKind::Effect => { self.effects.insert(id, snapshot); }
            EntityKind::Group => { self.groups.insert(id, snapshot); }
        }
    }

    /// Get total entity count.
    pub fn total_count(&self) -> usize {
        self.voices.len() + self.patterns.len() + self.melodies.len() +
        self.sequences.len() + self.effects.len() + self.groups.len()
    }
}

// ============================================================================
// Pending Reload
// ============================================================================

/// Represents a pending reload waiting to be applied.
#[derive(Clone, Debug)]
pub struct PendingReload {
    /// Beat at which to apply these changes
    pub apply_at_beat: f64,
    /// The changes to apply
    pub changes: Vec<ChangeOp>,
    /// Snapshot taken before the reload
    pub before_snapshot: StateSnapshot,
}

impl PendingReload {
    /// Create a new pending reload.
    pub fn new(apply_at_beat: f64, changes: Vec<ChangeOp>, before_snapshot: StateSnapshot) -> Self {
        Self {
            apply_at_beat,
            changes,
            before_snapshot,
        }
    }

    /// Get counts of each change type.
    pub fn change_counts(&self) -> (usize, usize, usize, usize) {
        let mut keep = 0;
        let mut add = 0;
        let mut update = 0;
        let mut remove = 0;
        for op in &self.changes {
            match op {
                ChangeOp::Keep { .. } => keep += 1,
                ChangeOp::Add { .. } => add += 1,
                ChangeOp::Update { .. } => update += 1,
                ChangeOp::Remove { .. } => remove += 1,
            }
        }
        (keep, add, update, remove)
    }
}

// ============================================================================
// Active Crossfade
// ============================================================================

/// An active crossfade transition.
#[derive(Clone, Debug)]
pub struct ActiveCrossfade {
    /// What kind of entity is being faded
    pub kind: EntityKind,
    /// Entity identifier
    pub id: String,
    /// Beat when fade started
    pub start_beat: f64,
    /// Beat when fade ends
    pub end_beat: f64,
    /// What to do when fade completes
    pub on_complete: CrossfadeAction,
    /// Node ID to fade (for synths/effects)
    pub node_id: Option<i32>,
}

/// What to do when a crossfade completes.
#[derive(Clone, Debug)]
pub enum CrossfadeAction {
    /// Free the node after fade out
    FreeNode,
    /// Just remove from tracking (node handles its own cleanup)
    RemoveTracking,
    /// No action needed
    None,
}

impl ActiveCrossfade {
    /// Calculate the current fade value (0.0 to 1.0).
    /// Returns None if fade is complete.
    pub fn fade_value(&self, current_beat: f64) -> Option<f64> {
        if current_beat >= self.end_beat {
            return None; // Fade complete
        }
        if current_beat <= self.start_beat {
            return Some(1.0); // Not started yet
        }
        let progress = (current_beat - self.start_beat) / (self.end_beat - self.start_beat);
        Some(1.0 - progress.clamp(0.0, 1.0))
    }

    /// Check if this fade is complete.
    pub fn is_complete(&self, current_beat: f64) -> bool {
        current_beat >= self.end_beat
    }
}

// ============================================================================
// Reload Manager
// ============================================================================

/// Manages the reload lifecycle and state transitions.
#[derive(Debug)]
pub struct ReloadManager {
    /// Snapshot taken at BeginReload
    before_snapshot: Option<StateSnapshot>,
    /// Pending reload waiting to be applied
    pending_reload: Option<PendingReload>,
    /// Active crossfades in progress
    active_crossfades: Vec<ActiveCrossfade>,
    /// Quantization for when to apply changes (in beats)
    quantization_beats: f64,
}

impl Default for ReloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ReloadManager {
    /// Create a new reload manager.
    pub fn new() -> Self {
        Self {
            before_snapshot: None,
            pending_reload: None,
            active_crossfades: Vec::new(),
            quantization_beats: 4.0, // Default: apply at bar boundaries
        }
    }

    /// Set the quantization for change application.
    pub fn set_quantization(&mut self, beats: f64) {
        self.quantization_beats = beats;
    }

    /// Begin a reload cycle by taking a snapshot of current state.
    pub fn begin_reload(&mut self, snapshot: StateSnapshot) {
        log::debug!(
            "[RELOAD] Begin reload, captured {} entities in snapshot",
            snapshot.total_count()
        );
        self.before_snapshot = Some(snapshot);
    }

    /// Finalize a reload by computing the diff and queuing changes.
    /// Returns the pending reload for logging purposes.
    pub fn finalize_reload(
        &mut self,
        new_snapshot: StateSnapshot,
        current_beat: f64,
    ) -> Option<&PendingReload> {
        let before = self.before_snapshot.take()?;

        // Compute the diff
        let changes = Self::compute_diff(&before, &new_snapshot);

        // Calculate when to apply (next quantization boundary)
        let apply_at_beat = Self::next_quantization_beat(
            current_beat,
            self.quantization_beats,
        );

        log::info!(
            "[RELOAD] Finalized: {} changes queued for beat {:.2}",
            changes.len(),
            apply_at_beat
        );

        // Store pending reload
        self.pending_reload = Some(PendingReload::new(apply_at_beat, changes, before));
        self.pending_reload.as_ref()
    }

    /// Check if there's a pending reload ready to apply.
    pub fn has_pending_reload(&self) -> bool {
        self.pending_reload.is_some()
    }

    /// Check if pending reload should be applied now.
    pub fn should_apply(&self, current_beat: f64) -> bool {
        self.pending_reload
            .as_ref()
            .map(|p| current_beat >= p.apply_at_beat)
            .unwrap_or(false)
    }

    /// Take the pending reload for application.
    pub fn take_pending_reload(&mut self) -> Option<PendingReload> {
        self.pending_reload.take()
    }

    /// Add an active crossfade.
    pub fn add_crossfade(&mut self, crossfade: ActiveCrossfade) {
        self.active_crossfades.push(crossfade);
    }

    /// Process active crossfades, returning completed ones.
    pub fn process_crossfades(&mut self, current_beat: f64) -> Vec<ActiveCrossfade> {
        let (completed, active): (Vec<_>, Vec<_>) = self
            .active_crossfades
            .drain(..)
            .partition(|cf| cf.is_complete(current_beat));
        self.active_crossfades = active;
        completed
    }

    /// Get mutable access to active crossfades for updating.
    pub fn active_crossfades_mut(&mut self) -> &mut Vec<ActiveCrossfade> {
        &mut self.active_crossfades
    }

    /// Calculate the next quantization boundary.
    fn next_quantization_beat(current_beat: f64, quantization: f64) -> f64 {
        let next = ((current_beat / quantization).ceil() * quantization).max(0.0);
        // Ensure at least MIN_APPLY_DELAY_BEATS in the future
        if next - current_beat < MIN_APPLY_DELAY_BEATS {
            next + quantization
        } else {
            next
        }
    }

    /// Compute the diff between two snapshots.
    fn compute_diff(before: &StateSnapshot, after: &StateSnapshot) -> Vec<ChangeOp> {
        let mut changes = Vec::new();

        // Helper to diff a single entity type
        fn diff_entities(
            before: &HashMap<String, EntitySnapshot>,
            after: &HashMap<String, EntitySnapshot>,
            kind: EntityKind,
            changes: &mut Vec<ChangeOp>,
        ) {
            // Check for updates and keeps
            for (id, new_snap) in after {
                if let Some(old_snap) = before.get(id) {
                    if old_snap.hash == new_snap.hash {
                        changes.push(ChangeOp::Keep {
                            kind,
                            id: id.clone(),
                        });
                    } else {
                        changes.push(ChangeOp::Update {
                            kind,
                            id: id.clone(),
                            old_hash: old_snap.hash,
                            new_hash: new_snap.hash,
                        });
                    }
                } else {
                    changes.push(ChangeOp::Add {
                        kind,
                        id: id.clone(),
                    });
                }
            }

            // Check for removals
            for id in before.keys() {
                if !after.contains_key(id) {
                    changes.push(ChangeOp::Remove {
                        kind,
                        id: id.clone(),
                    });
                }
            }
        }

        diff_entities(&before.voices, &after.voices, EntityKind::Voice, &mut changes);
        diff_entities(&before.patterns, &after.patterns, EntityKind::Pattern, &mut changes);
        diff_entities(&before.melodies, &after.melodies, EntityKind::Melody, &mut changes);
        diff_entities(&before.sequences, &after.sequences, EntityKind::Sequence, &mut changes);
        diff_entities(&before.effects, &after.effects, EntityKind::Effect, &mut changes);
        diff_entities(&before.groups, &after.groups, EntityKind::Group, &mut changes);

        changes
    }
}

// ============================================================================
// Hash implementations for state types
// ============================================================================

/// Macro to implement content hashing for state types.
/// Lists the fields that contribute to the content hash.
#[macro_export]
macro_rules! impl_content_hash {
    ($type:ty, $($field:ident),+) => {
        impl std::hash::Hash for $type {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                $(self.$field.hash(state);)+
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash() {
        let hash1 = content_hash(&"hello");
        let hash2 = content_hash(&"hello");
        let hash3 = content_hash(&"world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_diff_empty() {
        let before = StateSnapshot::new();
        let after = StateSnapshot::new();
        let changes = ReloadManager::compute_diff(&before, &after);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_diff_add() {
        let before = StateSnapshot::new();
        let mut after = StateSnapshot::new();
        after.add(EntityKind::Pattern, "drums".to_string(), 123);

        let changes = ReloadManager::compute_diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], ChangeOp::Add { kind: EntityKind::Pattern, id } if id == "drums"));
    }

    #[test]
    fn test_diff_remove() {
        let mut before = StateSnapshot::new();
        before.add(EntityKind::Pattern, "drums".to_string(), 123);
        let after = StateSnapshot::new();

        let changes = ReloadManager::compute_diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], ChangeOp::Remove { kind: EntityKind::Pattern, id } if id == "drums"));
    }

    #[test]
    fn test_diff_keep() {
        let mut before = StateSnapshot::new();
        before.add(EntityKind::Pattern, "drums".to_string(), 123);
        let mut after = StateSnapshot::new();
        after.add(EntityKind::Pattern, "drums".to_string(), 123);

        let changes = ReloadManager::compute_diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], ChangeOp::Keep { kind: EntityKind::Pattern, id } if id == "drums"));
    }

    #[test]
    fn test_diff_update() {
        let mut before = StateSnapshot::new();
        before.add(EntityKind::Pattern, "drums".to_string(), 123);
        let mut after = StateSnapshot::new();
        after.add(EntityKind::Pattern, "drums".to_string(), 456);

        let changes = ReloadManager::compute_diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], ChangeOp::Update { kind: EntityKind::Pattern, id, .. } if id == "drums"));
    }

    #[test]
    fn test_quantization() {
        assert_eq!(ReloadManager::next_quantization_beat(0.0, 4.0), 4.0);
        assert_eq!(ReloadManager::next_quantization_beat(3.9, 4.0), 4.0);
        assert_eq!(ReloadManager::next_quantization_beat(4.0, 4.0), 8.0); // Too close, go to next
        assert_eq!(ReloadManager::next_quantization_beat(4.5, 4.0), 8.0);
    }
}
