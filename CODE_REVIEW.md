# VibeLang Code Review

Date: 2026-03-11

## Executive Summary

VibeLang's codebase is well-structured for a project of this complexity. The separation between the Rhai API layer (builders that collect state) and the core runtime (message-based reconciliation) is clean and enables the impressive hot-reload capability. The diff system, ordered group creation/deletion, and content-swap quantization for patterns/melodies show mature engineering thinking. Test coverage in the core crate is solid for the runtime and reload logic.

The main concerns center on three areas: (1) a critical bug where sequences are not actually stopped during reload Phase 6, (2) the group diff always seeing name changes because the runtime doesn't track group names, causing unnecessary updates on every reload, and (3) SFZ instruments being entirely outside the diff/reload system. The API layer is generally consistent but has some validation gaps — `set_tempo` and `set_time_signature` accept arbitrary values without clamping at the API level (though the transport handler clamps tempo), and there are scattered opportunities for silent failures.

The builder pattern implementation is pragmatic — each builder method calls `sync_to_state()` to write the config to the thread-local script state, meaning the builder state is always eventually consistent even if `.apply()` is forgotten. This is user-friendly but creates some confusion about when `.apply()` is actually necessary.

## Critical Issues

### 1. Sequences not stopped in reload Phase 6

**File:** `crates/vibelang-core/src/runtime.rs`, lines 1548-1550

The `sequences_to_stop` vector is collected but the loop only logs — it never calls `self.sequences.stop(id).await`. Patterns and melodies have their stop calls; sequences do not.

**Impact:** If a user removes `.start()` from a sequence during live coding, the sequence continues playing indefinitely. This directly contradicts the expected hot-reload behavior.

**Severity:** Critical

**Fix:**
```rust
for id in sequences_to_stop {
    tracing::debug!("Reload: stopping sequence {:?}", id);
    let _ = self.sequences.stop(id).await;
}
```

### 2. Group diff always reports updates due to name mismatch

**File:** `crates/vibelang-core/src/reload/mod.rs`, lines 158-167

When diffing groups, the current runtime state reconstructs a `GroupConfig` with `name: String::new()` because "Runtime doesn't track name." But `GroupConfig` derives `PartialEq`, so every group with a non-empty name in the script will be seen as "updated" on every single reload, even when nothing changed.

**Impact:** Every reload triggers unnecessary group param updates and potentially mute/solo re-application for all groups. This wastes backend round-trips and could cause brief audio artifacts.

**Severity:** Critical

**Fix:** Either track the group name in `GroupState` (preferred), or exclude `name` from `PartialEq` comparison by implementing a custom `PartialEq` for `GroupConfig`.

## High Priority

### 3. SFZ instruments not part of the diff/reload system

**File:** `crates/vibelang-core/src/reload/mod.rs`

The `calculate_diff` function diffs groups, voices, patterns, melodies, sequences, effects, modulators, samples, and fades — but not SFZ instruments. The `ScriptState` has `sfz_instruments: HashMap<SfzId, SfzConfig>`, but no corresponding diff is calculated.

**Impact:** SFZ instruments are never cleaned up on reload. If a user removes an SFZ instrument from their script, the buffers remain loaded. Adding/changing SFZ instruments during live coding may not work correctly.

**Severity:** High

**Fix:** Add SFZ diffing to `calculate_diff` and handle SFZ create/delete/update in `apply_reload`.

### 4. Hash collision potential in entity ID generation

**File:** `crates/vibelang-rhai/src/context.rs`, `hash_name_to_id()`

FNV-1a 32-bit hash is used for entity IDs. Two different entity names that hash-collide would silently overwrite each other. With u32 and FNV-1a, the birthday paradox gives ~50% collision probability at ~77k entities. Real scripts have far fewer, but the failure mode is silent data corruption.

**Impact:** Unlikely in practice (scripts rarely have >100 entities), but when it hits, the behavior would be bewildering — one entity silently replaces another.

**Severity:** High (low probability, high impact)

**Fix:** Add collision detection in `get_or_create_*_id` — check if the existing entry at that ID has a different name and log an error/return a different ID.

### 5. `set_time_signature` accepts invalid values

**File:** `crates/vibelang-rhai/src/api/global.rs`, `set_time_signature()`

```rust
pub fn set_time_signature(numerator: i64, denominator: i64) {
    context::with_state(|state| {
        state.time_sig = TimeSignature {
            numerator: numerator as u8,
            denominator: denominator as u8,
        };
    });
}
```

No validation or clamping. `set_time_signature(0, 0)` creates a `0/0` time signature. Negative values wrap via `as u8` (e.g., -1 becomes 255). A denominator of 0 will cause division-by-zero in `ChangeQuant::NextBar` and anywhere bars are calculated.

**Severity:** High

**Fix:** Clamp and validate: `numerator.clamp(1, 32) as u8`, `denominator` should be power-of-2 (1, 2, 4, 8, 16, 32). Return an error or warn on invalid values.

### 6. `set_tempo` at the API layer has no validation

**File:** `crates/vibelang-rhai/src/api/global.rs`, `set_tempo()`

The script-level `set_tempo` stores the value directly in `ScriptState` without validation. The transport handler clamps to 1.0-999.0, but between script execution and runtime application, the unclamped value exists in the diff system. `set_tempo(0.0)` or `set_tempo(-100.0)` would be stored in the script state; if any code reads tempo from script state before it reaches the transport handler, it could cause division-by-zero (tempo is used as a divisor in beat-to-time conversions).

**Severity:** High

**Fix:** Add `bpm.clamp(1.0, 999.0)` in the API-layer `set_tempo` function as well.

### 7. Pattern/melody with zero-length causes tight loop

**File:** `crates/vibelang-core/src/handlers/patterns.rs`, line ~87

```rust
if !pattern.playing || pattern.content.length == Beat::ZERO {
    continue;
}
```

This correctly skips zero-length patterns, but a near-zero length (e.g., `Beat::from_f64(0.0001)`) could cause extremely rapid triggering — potentially hundreds of note-ons per tick.

**Severity:** High

**Fix:** Enforce a minimum pattern/melody length (e.g., 0.0625 beats = 64th note) either at the API level or in the handler.

## Medium Priority

### 8. Duplicate `parse_note_name` implementations

**Files:** `crates/vibelang-rhai/src/api/helpers.rs:578` and `crates/vibelang-core/src/midi/callbacks.rs:563`

Two separate implementations of `parse_note_name` exist. If one is updated (e.g., to support double sharps), the other won't be.

**Severity:** Medium

**Fix:** Export from `vibelang-core` and use in `vibelang-rhai`.

### 9. Voice `on()` method overloaded but no error for empty synthdef

**File:** `crates/vibelang-rhai/src/api/voice.rs`

If a user calls `voice("bass").apply()` without ever calling `.on()` or `.synth()`, the voice is registered with `synthdef: ""`. The runtime will try to create a synth with an empty synthdef name, which will fail silently or produce an unhelpful error.

**Severity:** Medium

**Fix:** Warn or error in `apply()` if `synth_name` is `None`.

### 10. Pattern `.on()` accepts voice name string but doesn't verify voice exists

**File:** `crates/vibelang-rhai/src/api/pattern.rs`

`pattern("kick").on("nonexistent_voice")` silently creates a voice ID for a voice that was never configured. The pattern will be created but triggers will fail at runtime with no clear error trace back to the script.

**Severity:** Medium

**Fix:** At minimum, log a warning. Ideally, validate voice existence at script execution time.

### 11. MIDI `on_note_channel` uses 1-16 range but `channel()` uses 0-15

**File:** `crates/vibelang-rhai/src/api/midi/device.rs`

```rust
// on_note_channel: channel is 1-16, converted to 0-15
channel: Some(channel.clamp(1, 16) as u8 - 1),

// channel(): channel is 0-15
self.channel = ch.clamp(0, 15) as u8;
```

The `on_note_channel` method expects 1-based channels while `channel()` expects 0-based. The `route_to_channel` also uses 0-based. This inconsistency will confuse users.

**Severity:** Medium

**Fix:** Standardize on one convention (0-15 is MIDI standard internally; 1-16 for user-facing API). Document clearly.

### 12. `ControlBusAllocator` never reclaims buses

**File:** `crates/vibelang-core/src/state.rs`

The allocator monotonically increases `next_bus`. Deleted modulators don't free their buses. Over many reload cycles, bus IDs grow unboundedly. SuperCollider has 16384 control buses by default.

**Severity:** Medium (only matters after many thousands of reloads)

**Fix:** Track freed buses and reuse them, or reset the allocator when all modulators are recreated.

### 13. `node_id` and `buffer_id` allocators also never reclaim

**File:** `crates/vibelang-core/src/state.rs`

Same pattern: `next_node_id` and `next_buffer_id` grow monotonically. Node IDs start at 1000 and SuperCollider has a practical limit. After ~2 billion operations this wraps, but more practically, freed node IDs are never reused.

**Severity:** Low-Medium (unlikely to hit in practice)

### 14. `Fade` duration validation missing

**File:** `crates/vibelang-rhai/src/api/sequence.rs`

Fade builders accept any duration value. A zero or negative duration would cause division-by-zero in `ActiveFade::current_value()` and `is_complete()` when calculating `duration_secs`.

**Severity:** Medium

**Fix:** Clamp duration to a positive minimum in the Fade builder.

### 15. Pattern `swing` not validated

**File:** `crates/vibelang-rhai/src/api/pattern.rs`

The `swing()` method accepts any f64 value. Extreme swing values (>1.0 or negative) could cause steps to overlap or trigger in wrong order.

**Severity:** Medium

**Fix:** Clamp to a reasonable range like 0.0-1.0.

## Low Priority

### 16. `voice()` auto-syncs on creation before configuration

**File:** `crates/vibelang-rhai/src/api/voice.rs`, `voice()` function

```rust
pub fn voice(ctx: NativeCallContext, name: String) -> Voice {
    let v = Voice::new(ctx, name);
    v.sync_to_state();
    v
}
```

Every `voice("name")` call immediately registers an incomplete voice config (no synthdef, default params). If the user later configures it, `sync_to_state()` overwrites. This is mostly harmless but creates unnecessary churn in the script state during execution.

**Severity:** Low

### 17. Missing documentation on several API functions

**File:** `crates/vibelang-rhai/src/api/helpers.rs`

Many helper functions lack doc comments. Functions like `bars()`, `db()`, `note()` have brief docs but the musical context could be clearer for users unfamiliar with music theory.

**Severity:** Low

### 18. Magic numbers in position epsilon calculation

**File:** `crates/vibelang-core/src/runtime.rs`, `calculate_position_epsilon()`

```rust
const TICK_WINDOW_MS: f64 = 20.0;
// ...
let max_epsilon = length.to_f64() * 0.10;
```

The 20ms window and 10% cap are reasonable but undocumented rationale. The comment explains the math but not why 20ms was chosen over, say, 10ms or 50ms.

**Severity:** Low

### 19. Lossy `f64 as f32` conversions throughout API layer

**Files:** Multiple API files (voice.rs, group.rs, sequence.rs)

All user-specified parameter values go through `value as f32` conversion. This is expected (audio parameters are f32 in SuperCollider), but there's no documentation about precision loss. For most audio params this is fine, but for timing/position values it could matter.

**Severity:** Low

### 20. `MelodiesHandler` uses a static atomic counter for debug logging

**File:** `crates/vibelang-core/src/handlers/melodies.rs`, line ~107

```rust
static TICK_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
```

A global static for debug logging frequency. Not harmful but slightly surprising in otherwise clean handler code.

**Severity:** Low (style)

## API Consistency Audit

| Method | Voice | Pattern | Melody | Sequence | Modulator | Group |
|--------|-------|---------|--------|----------|-----------|-------|
| `on()` | ✅ (source) | ✅ (voice) | ✅ (voice) | N/A | ✅ (synthdef) | N/A |
| `apply()` | ✅ | ✅ | ✅ | ✅ | ✅ | N/A (implicit) |
| `start()` | N/A | ✅ (chainable) | ✅ (chainable) | ✅ (mut self) | N/A | N/A |
| `stop()` | N/A | ✅ (&mut self) | ✅ (&mut self) | ✅ (&mut self) | N/A | N/A |
| `set_param()` | ✅ (chainable) | N/A | N/A | N/A | ✅ (chainable) | ✅ (via `set()`) |
| `mute()` | ✅ | N/A | N/A | N/A | N/A | ✅ |
| `solo()` | ✅ | N/A | N/A | N/A | N/A | ✅ |
| `gain()` | ✅ | N/A | N/A | N/A | N/A | ✅ (via `amp()` or `set()`) |
| `run()` | ✅ | N/A | N/A | N/A | N/A | N/A |
| `swing()` | N/A | ✅ | ✅ | N/A | N/A | N/A |
| `poly()` | ✅ | N/A | N/A | N/A | N/A | N/A |

**Inconsistency notes:**
- `start()` is chainable (returns `Self`) on Pattern and Melody but takes `&mut self` on Sequence
- `stop()` takes `&mut self` on all types (consistent)
- Group has no `apply()` — group state is managed implicitly through the group-path hierarchy
- Pattern and Melody lack `set_param()` — step parameters are defined inline in the notation

## Detailed Findings by Module

### voice.rs

Overall well-designed builder. Good clamping on `poly()` (1-255) and `channel()` (0-15).

- **Lines ~486-493 (`apply`):** Does `resolve_name()` then `sync_to_state()`. Both are also called in `run()`. The auto-sync in the `voice()` constructor means the voice is registered before the user finishes configuring it. This is intentional for ergonomics but means incomplete configs briefly exist in the script state.
- **Empty synthdef:** No validation that a synthdef was set before `apply()`. See issue #9.
- **`on_sample`:** Accesses `context::with_state` to read sample config. If the sample hasn't been loaded yet (script ordering issue), `config` would be `None` and the sample params wouldn't be set. No warning is logged.

### pattern.rs

Step notation parser is well-implemented with support for velocity modifiers, sub-patterns, etc.

- **`step()` method:** Accepts a string but parsing happens later in `apply()`. If the notation is invalid, errors surface during `apply()` not during `step()`.
- **Euclidean rhythm (`euclidean`):** Properly handles edge cases (k=0, k>n) with clamping. Good.
- **`swing()`:** No range validation. See issue #15.
- **`length()`:** Accepts any f64. Negative lengths are not caught. See issue #7 for near-zero.

### melody.rs

The note notation parser (lines ~650-850) uses a character-by-character state machine. Mostly correct but:

- **Line 661, 685, 761, 817, 830:** Multiple `chars.next().unwrap()` calls. These are safe because of the `while chars.peek().is_some()` guard, but the pattern is fragile. A refactor that changes the loop condition could introduce panics.
- **`notes()` method:** Assumes 4 beats per bar (`let beats_per_bar = 4.0`). This ignores the actual time signature. A 3/4 piece would have incorrect bar lengths.
- **Chord notation parsing:** Well-handled with `split_into_bars` and bar boundary detection.

### sequence.rs

Three builders in one file: `Sequence`, `Fade`, and `Fx`.

- **`Sequence::start()`** is chainable (returns Self) unlike Pattern/Melody's `start()` which also returns Self. Actually consistent — good.
- **`Fade::apply()`:** Missing validation on duration. See issue #14.
- **`Fx::apply()`:** Registers effect with the group correctly. No issues found.

### group.rs

Groups are implicitly created via path strings. The hierarchical group system is clever.

- **`amp()` and `set()`:** Both set parameters on the group config. `amp()` is a convenience wrapper.
- **No explicit `apply()`:** Groups are created during reload when referenced by voices. This is clean but means typos in group names silently create new groups.

### modulator.rs

Clean builder pattern. `set_param()` is chainable.

- **`rate()`, `lo()`, `hi()`:** Convenience methods that map to `set_param`. Good ergonomics.
- **`bars()` for rate:** Converts bar count to frequency. `1.0 / (bars * beats_per_bar * 60.0 / tempo)`. Uses default tempo/time_sig from context.
- **No validation on synthdef name:** Empty string is accepted silently.

### helpers.rs

Comprehensive set of music-theory helpers.

- **`db()`:** Correct formula `10^(dB/20)`.
- **`note()`:** Converts note name to MIDI number. Uses the `parse_note_name` from helpers (not from core). See issue #8 about duplication.
- **`chord()` and `scale()`:** Return arrays of note numbers. Well-implemented with standard music theory.
- **Math functions (`rand`, `sin`, `cos`, etc.):** Thin wrappers. `rand(min, max)` properly handles min > max by swapping.

### global.rs

- **`set_tempo`:** No validation. See issue #6.
- **`set_time_signature`:** No validation. See issue #5.
- **`set_quantization`:** Sets quantization in beats. Accepts any f64.

### sample.rs

Sample loading API. Builder pattern with warp mode support.

- **File path handling:** Uses `resolve_path` to handle relative paths from the script file's directory.
- **`buffer_id` stored as f32 in voice params:** `sample.buffer_id as f32`. Buffer IDs are u32 internally. For buffer IDs > 16 million, the f32 representation loses precision. Unlikely to hit in practice but technically a lossy conversion.

### midi/device.rs

Extensive MIDI API with both MIDI 1.0 and 2.0 support.

- **All values clamped:** Channel (0-15), note (0-127), velocity (0-127 or 0-65535 for hires), CC (0-127). Consistently applied. Good.
- **`on_note_channel` vs `channel()`:** Different conventions. See issue #11.
- **`open_input`/`open_output` deprecated:** Good — auto-opening is cleaner. The deprecated methods still work for backward compatibility.
- **`note()` method:** Falls back to C4 (60) on invalid input with a warning. Reasonable default.

### midi/routing.rs & recording.rs

Not read in full but routing builders follow the same consistent pattern. `CcRoute`, `KeyboardRoute`, `NoteRoute` all use builder patterns that register with the script state.

### sfz.rs

SFZ instrument loading. Simple API wrapping file loading.

- **Not part of diff system:** See issue #3.

### runtime.rs

The heart of the system. Well-structured message dispatch with a clear phase-based reload.

- **Phase ordering is correct** (stop → delete → create → update → finalize → start) with proper attention to parent/child ordering for groups.
- **`sync_with_retry`:** 10ms timeout is aggressive but well-reasoned for MIDI clock. The comment explains the tradeoff.
- **Sequence stop bug:** See issue #1.
- **MIDI 2.0 output messages silently skipped during reload:** Line ~1142: `"Reload: skipping MIDI 2.0 message (not yet implemented)"`. Not an error but users using MIDI 2.0 output messages won't get them applied during reload. Should at minimum be a warning.

### state.rs

Clean state management with well-documented structs.

- **`PatternState`/`MelodyState`:** Content/playback separation is elegant. The `try_apply_pending` method for quantized content swaps is well-implemented.
- **`ActiveFade::current_value`:** Division by `duration_secs` — if tempo is 0 or duration is 0, this will produce NaN or infinity. See issue #14.
- **Allocators:** Monotonic, never reclaim. See issues #12, #13.

### reload/diff.rs

Clean generic diffing implementation. `EntityDiff` with created/deleted/updated/unchanged sets.

- **`ParamDiff`:** Uses `f32::EPSILON` for float comparison. This is appropriate for audio parameters.
- **`ReloadDiff::summary()`:** Nice debugging output.

### reload/mod.rs

- **`ChangeQuant::should_apply`:** Well-implemented quantization boundary detection.
- **`order_group_deletions` / `order_group_creations`:** Correct topological sort with cycle detection. Both handle cycles by adding remaining in arbitrary order (safe fallback).
- **Group name mismatch in diff:** See issue #2.
