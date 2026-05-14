# Unreleased

## Named-Input Processors

- **Rhai synthdef input builders** (`9b67409`) - `define_synthdef(...)`
  now supports `.input("name")` and `.input("name", channels)` for declared
  audio-rate named inputs. Inputs are mono/stereo only and read from
  `p.inputs.<name>` in `body_map` processors.
- **Script-side named input routing** - scripts can patch declared inputs with
  `target.input("name").from(source)`, including voice, group, current-group,
  and silent sources.
- **Parent-group autofeed for inline processors** (`e4c8352`) - a declared
  stereo audio-rate input named exactly `in` defaults to the parent group's
  pre-fader bus when no explicit script route overrides it.
- **Hot-reload input-port reconciliation** (`ea1156c`) - renaming, removing,
  or changing the width of an input drops dependent routes with warnings and
  frees materialized `input_link_*` resources.
- **First-batch stdlib processors** (`4c1fa21`) - `stdlib/processors/` now
  includes mono/stereo passthrough, lowpass, and ring-mod processors plus
  `crossfade_stereo` and `mixer4_stereo`, matching the
  `design-named-input-stdlib-primitive-catalogue` contract.

Deferred: control-rate named input jacks are not public yet; input routes still
cannot select a non-default source output; fan-in verbs such as `from_all` and
`add_from` are not implemented; stereo-to-mono downmixing remains explicit
rather than implicit.

## Fixed

- **Named input route stabilization** - unpatched declared inputs receive valid
  shared silent buses, route materialization is atomic and retryable after
  backend failures, and group/stereo sources routed to mono inputs are rejected
  as width mismatches instead of being implicitly downmixed.

# VibeLang v0.4.0

A stability and correctness release. v0.4 focuses on fixing bugs found during a thorough code review, improving hot-reload reliability, and bringing SFZ instruments into the diff/reload system.

## Highlights

- **SFZ instruments now participate in hot-reload** — previously, SFZ voices were outside the diff system entirely. Changing an SFZ instrument in your script now correctly updates during live coding without restarting.
- **Stateful fades with reload diffing** — fades are now tracked as first-class entities in the runtime state, enabling proper diff-based updates during hot-reload.
- **MIDI channel convention standardized to 1-16** — matches musician expectations instead of the internal 0-15 representation.

## Bug Fixes

- **Sequences now actually stop on reload** — removing `.start()` from a sequence during live coding previously left it playing forever. Fixed.
- **Group diff no longer reports phantom updates** — the runtime now tracks group names, eliminating unnecessary updates on every reload.
- **Entity ID hash collision detection** — FNV-1a 32-bit hashes now detect collisions with linear probing instead of silently overwriting.
- **Input validation across the API:**
  - `set_tempo()` clamped to 1.0–999.0
  - `set_time_signature()` validates numerator (1–32) and snaps denominator to power-of-2
  - Pattern/melody lengths enforce minimum 1/64th note
  - Fade durations enforce minimum 1/64th note
- **Voices with empty synthdefs** now log a warning instead of silently failing.
- **Pattern `.on()` warns** when referencing a nonexistent voice.
- **Group finalization** now runs on updates too, not just initial creation.
- **MIDI Start no longer re-sent** on hot-reload; fades restart correctly.
- **Eliminated all `unwrap()` calls** in vibelang-core, replaced with proper error handling.

## Code Quality

- Fixed all pre-existing test failures
- Added `clippy::unwrap_used` warning to vibelang-core
- Comprehensive code review completed and tracked (see `REVIEW_TODO.md`)

## Install / Upgrade

```bash
cargo install vibelang-cli
```

**Full changelog:** https://github.com/trusch/vibelang/compare/v0.3.0...v0.4.0
