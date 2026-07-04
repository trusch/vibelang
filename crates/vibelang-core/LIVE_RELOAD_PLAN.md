# Live Reloading Plan for vibelang-core

## Overview

Live reloading allows updating a running VibeLang session when the source `.vibe` script changes. The goal is to minimize audio disruption by only updating what actually changed.

## Design Goals

1. **Minimal disruption** - Don't stop/restart things that haven't changed
2. **No audio glitches** - Preserve audio buses and routing where possible
3. **Fast updates** - Diff and patch, don't rebuild from scratch
4. **State preservation** - Keep transport running, preserve beat position

## Architecture

### State Diffing

```
┌─────────────────┐     ┌─────────────────┐
│  Old State      │     │  New State      │
│  (from script)  │     │  (from script)  │
└────────┬────────┘     └────────┬────────┘
         │                       │
         └───────────┬───────────┘
                     │
                     ▼
            ┌─────────────────┐
            │   State Diff    │
            │   Calculator    │
            └────────┬────────┘
                     │
                     ▼
            ┌─────────────────┐
            │  Patch Actions  │
            │  (ordered list) │
            └────────┬────────┘
                     │
                     ▼
            ┌─────────────────┐
            │  Apply Patches  │
            │  (to runtime)   │
            └─────────────────┘
```

### Script State vs Runtime State

**Script State** (from parsing .vibe file):
- Groups defined
- Voices defined
- Patterns defined
- Melodies defined
- Sequences defined
- Effects defined
- Parameter values

**Runtime State** (in State struct):
- All of the above PLUS:
- Node IDs (allocated by scsynth)
- Buffer IDs (allocated by scsynth)
- Audio bus IDs (allocated by runtime)
- Active synth nodes
- Current beat position
- Playing state

## Diff Types

### Entity Operations

For each entity type (Group, Voice, Pattern, Melody, Sequence, Effect):

| Operation | When | Action |
|-----------|------|--------|
| Create | ID in new, not in old | Create new entity |
| Delete | ID in old, not in new | Free entity and resources |
| Update | ID in both, config differs | Update parameters in-place |
| Unchanged | ID in both, config same | No action |

### Parameter-Level Diffs

For entities that exist in both old and new state, diff their parameters:

```rust
struct ParamDiff {
    added: HashMap<String, f32>,    // New params
    removed: HashSet<String>,        // Deleted params
    changed: HashMap<String, f32>,   // Modified params
}
```

## Bus Handling Strategy

### Problem

Audio buses are a limited resource. When entities are deleted and recreated, we want to:
1. Reuse the same bus if the entity is just being updated
2. Free buses only when entities are truly deleted
3. Avoid bus ID exhaustion

### Solution: Bus Pool with Entity Affinity

```rust
struct BusAllocator {
    // Map entity ID -> reserved bus ID
    group_buses: HashMap<GroupId, BusId>,

    // Pool of freed buses available for reuse
    free_buses: Vec<BusId>,

    // Next bus ID if pool is empty
    next_bus_id: u32,
}

impl BusAllocator {
    /// Get bus for entity, reusing if already assigned
    fn get_or_alloc(&mut self, group_id: GroupId) -> BusId {
        if let Some(bus) = self.group_buses.get(&group_id) {
            return *bus;
        }

        let bus = self.free_buses.pop().unwrap_or_else(|| {
            let id = BusId::new(self.next_bus_id);
            self.next_bus_id += 1;
            id
        });

        self.group_buses.insert(group_id, bus);
        bus
    }

    /// Release bus when entity is deleted
    fn release(&mut self, group_id: GroupId) {
        if let Some(bus) = self.group_buses.remove(&group_id) {
            self.free_buses.push(bus);
        }
    }
}
```

### Node ID Handling

Unlike buses, node IDs don't need pooling - scsynth handles them. But we should:
- Track which entities own which nodes
- Free nodes for deleted entities
- Create new nodes for new entities

## Reload Message

Add a new message type for reload:

```rust
pub enum Message {
    // ... existing messages ...

    /// Reload from new script state
    Reload {
        /// New state parsed from script
        script_state: ScriptState,
    },
}

/// State extracted from a .vibe script (no runtime IDs)
#[derive(Clone, Debug)]
pub struct ScriptState {
    pub tempo: f64,
    pub time_sig: TimeSignature,
    pub groups: HashMap<GroupId, GroupConfig>,
    pub voices: HashMap<VoiceId, VoiceConfig>,
    pub patterns: HashMap<PatternId, PatternConfig>,
    pub melodies: HashMap<MelodyId, MelodyConfig>,
    pub sequences: HashMap<SequenceId, SequenceConfig>,
    pub effects: HashMap<EffectId, EffectConfig>,
}

/// Config for a group (from script, no runtime IDs)
#[derive(Clone, Debug, PartialEq)]
pub struct GroupConfig {
    pub parent: Option<GroupId>,
    pub params: ParamMap,
}
```

## Diff Algorithm

```rust
impl Runtime<B> {
    async fn handle_reload(&mut self, new_state: ScriptState) -> Result<()> {
        // 1. Calculate diffs for each entity type
        let group_diff = self.diff_groups(&new_state.groups);
        let voice_diff = self.diff_voices(&new_state.voices);
        let pattern_diff = self.diff_patterns(&new_state.patterns);
        let melody_diff = self.diff_melodies(&new_state.melodies);
        let sequence_diff = self.diff_sequences(&new_state.sequences);
        let effect_diff = self.diff_effects(&new_state.effects);

        // 2. Order deletions (children before parents)
        // Delete effects first (at tail of groups)
        // Delete voices (synths in groups)
        // Delete groups (last, after children)

        // 3. Apply deletions
        for id in effect_diff.deleted { self.effects.remove(id).await?; }
        for id in voice_diff.deleted { self.voices.delete(id).await?; }
        for id in pattern_diff.deleted { self.patterns.delete(id).await?; }
        for id in melody_diff.deleted { self.melodies.delete(id).await?; }
        for id in sequence_diff.deleted { self.sequences.delete(id).await?; }
        for id in group_diff.deleted { self.groups.delete(id).await?; }

        // 4. Order creations (parents before children)
        // Create groups first
        // Create voices (need groups)
        // Create effects (at tail of groups)

        // 5. Apply creations
        for (id, config) in group_diff.created {
            self.groups.create(id, config.parent).await?;
            for (k, v) in config.params { self.groups.set_param(id, &k, v).await?; }
        }
        for (id, config) in voice_diff.created {
            self.voices.create(id, config).await?;
        }
        // ... etc

        // 6. Apply updates (in-place parameter changes)
        for (id, params) in group_diff.updated {
            for (k, v) in params { self.groups.set_param(id, &k, v).await?; }
        }
        for (id, config) in voice_diff.updated {
            // Voice config changes may require recreating the voice
            self.handle_voice_update(id, config).await?;
        }
        // ... etc

        // 7. Finalize (create link synths for new/changed groups)
        self.groups.finalize().await?;

        Ok(())
    }
}
```

## Entity Diff Structure

```rust
#[derive(Default)]
struct EntityDiff<Id, Config> {
    /// Entities to create (ID -> config)
    created: HashMap<Id, Config>,

    /// Entities to delete
    deleted: Vec<Id>,

    /// Entities with changed config (ID -> new config)
    updated: HashMap<Id, Config>,

    /// Entities unchanged (for reference)
    unchanged: HashSet<Id>,
}
```

## Special Cases

### 1. Group Hierarchy Changes

If a group's parent changes:
- Old: Groups A (parent=None), B (parent=A)
- New: Groups A (parent=None), B (parent=None)

This requires:
1. Free group B's link synth
2. Recreate group B with new parent
3. Recreate link synth with correct routing

### 2. Voice Synthdef Changes

If a voice's synthdef changes, we must:
1. Stop all active synths for that voice
2. Update the config
3. New triggers will use new synthdef

### 3. Playing Patterns/Melodies

If a pattern's steps change while it's playing:
1. Update the config
2. Pattern continues from current position
3. New steps take effect on next loop

### 4. Sample Changes

If a sample path changes:
1. Load new buffer
2. Update sample info
3. Free old buffer
4. Any active playback continues until done

## Implementation Plan

### Phase 1: Diff Infrastructure
1. Create `ScriptState` type
2. Create `EntityDiff<Id, Config>` type
3. Implement diff functions for each entity type

### Phase 2: Bus Pooling
1. Create `BusAllocator`
2. Modify `GroupsHandler` to use allocator
3. Add bus release on group delete

### Phase 3: Reload Handler
1. Add `Message::Reload` variant
2. Implement `Runtime::handle_reload()`
3. Order deletions and creations correctly

### Phase 4: Integration
1. Add reload support to CLI (`--watch` flag)
2. File watcher integration
3. Error recovery (rollback on failure?)

## Files to Create/Modify

### New Files
- `src/reload/mod.rs` - Reload module
- `src/reload/diff.rs` - Diff calculation
- `src/reload/script_state.rs` - Script state type
- `src/reload/bus_pool.rs` - Bus allocator

### Modified Files
- `src/message.rs` - Add Reload message
- `src/runtime.rs` - Add reload handler
- `src/state.rs` - Add bus allocator
- `src/handlers/groups.rs` - Use bus allocator

## Testing Strategy

1. **Unit tests for diff logic** - Test that diffs are calculated correctly
2. **Integration tests** - Create state, reload with changes, verify result
3. **Bus pool tests** - Test allocation, reuse, and release
4. **Ordering tests** - Ensure correct delete/create order

## Edge Cases to Handle

1. **Circular dependencies** - Detect and reject
2. **Missing parent** - Error if group references non-existent parent
3. **ID reuse** - If ID deleted then recreated, handle correctly
4. **Empty reload** - No changes, do nothing
5. **Complete replacement** - All entities changed
6. **Partial failure** - What happens if creation fails mid-reload?
