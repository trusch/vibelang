# VibeLang Review TODO

Source: CODE_REVIEW.md (2026-03-11)

## Critical

- [ ] **CR-1** Sequences not stopped in reload Phase 6 — `sequences_to_stop` loop logs but never calls `stop()` (runtime.rs ~L1548)
- [ ] **CR-2** Group diff always reports updates — runtime reconstructs GroupConfig with `name: String::new()`, causing name mismatch every reload (reload/mod.rs ~L158)

## High

- [ ] **CR-3** SFZ instruments outside diff/reload system — never cleaned up (reload/mod.rs)
- [ ] **CR-4** FNV-1a 32-bit hash collision risk — no collision detection in `get_or_create_*_id` (context.rs)
- [ ] **CR-5** `set_time_signature(0, 0)` accepted — div-by-zero in quantization (global.rs)
- [ ] **CR-6** `set_tempo` no API-layer validation — unclamped in script state (global.rs)
- [ ] **CR-7** Near-zero pattern/melody length causes runaway triggering (handlers/patterns.rs, handlers/melodies.rs)

## Medium

- [ ] **CR-8** Duplicate `parse_note_name` — helpers.rs vs midi/callbacks.rs
- [ ] **CR-9** Voice with empty synthdef silently accepted (voice.rs)
- [ ] **CR-10** Pattern `.on()` doesn't verify voice exists (pattern.rs)
- [ ] **CR-11** MIDI channel convention inconsistency — 0-based vs 1-based (midi/device.rs)
- [ ] **CR-12** ControlBusAllocator never reclaims buses (state.rs)
- [ ] **CR-13** node_id and buffer_id allocators never reclaim (state.rs)
- [ ] **CR-14** Fade duration not validated — div-by-zero risk (sequence.rs)
- [ ] **CR-15** Pattern swing not validated (pattern.rs)

## Low

- [ ] **CR-16** Voice auto-syncs before configuration complete (voice.rs)
- [ ] **CR-17** Missing documentation on helper functions (helpers.rs)
- [ ] **CR-18** Magic numbers in position epsilon (runtime.rs)
- [ ] **CR-19** Lossy f64→f32 conversions undocumented (multiple)
- [ ] **CR-20** Static atomic counter for debug logging (melodies.rs)
