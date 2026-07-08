# VibeLang Review TODO

Source: CODE_REVIEW.md (2026-03-11)

## Rack Audio Audit Close-Out (2026-05-16)

Harness: `scripts/rack_smoke_audio_audit.py`

Seven rack examples are in the final musical-band target after the rack audio
v2 sweep:

| Rack | Peak dBFS | RMS dBFS | Status |
|------|----------:|---------:|--------|
| `4ms_ambient` | -3.79 | -17.81 | in band |
| `dnb` | -3.97 | -16.39 | in band |
| `intellijel_perform` | -9.05 | -12.06 | in band |
| `lofi` | -6.27 | -17.10 | in band |
| `minimal_techno` | -6.49 | -15.89 | in band |
| `resynthesizer` | -4.07 | -17.45 | in band |
| `verbos_westcoast` | -6.89 | -11.86 | in band |

Known limitations:

- `mutable_rack` remains blocked on
  `mutable-rack-output-limiter-soft-clipper-to-tame-mi-ugen-transients`;
  Mi UGen transients still need a rack-local output limiter or soft clipper.
- `erica_techno` remains blocked on
  `investigate-erica-techno-gain-bypass-21-dbfs-despite-all-vibe-trims`;
  the remaining fix is a narrow Rust gain-bypass investigation rather than
  broad rack trimming.

## Critical

- [x] **CR-1** Sequences not stopped in reload Phase 6 — restored `stop()` call
- [x] **CR-2** Group diff always reports updates — now uses stored group name in diff

## High

- [x] **CR-3** SFZ instruments outside diff/reload — added SFZ diffing + cleanup
- [x] **CR-4** FNV-1a 32-bit hash collision risk — added collision detection with linear probing
- [x] **CR-5** `set_time_signature(0, 0)` — clamp numerator 1-32, snap denominator to power-of-2
- [x] **CR-6** `set_tempo` no validation — clamped to 1.0-999.0 at API layer
- [x] **CR-7** Near-zero pattern/melody length — enforce minimum 1/64th note

## Medium

- [x] **CR-8** Duplicate `parse_note_name` — canonical impl now lives in
  `vibelang_dsp::notes` (`parse_note_name` + unclamped `parse_note_name_raw`);
  core re-exports it via `vibelang_core::midi::parse_note_name`, and the
  wasm/http/lsp copies were replaced with thin delegating wrappers
- [x] **CR-9** Voice with empty synthdef — now logs warning
- [x] **CR-10** Pattern `.on()` doesn't verify voice exists — now warns
- [x] **CR-11** MIDI channel convention — standardized to 1-16 (musician convention)
- [x] **CR-12** ControlBusAllocator never reclaims buses — free-list reclamation wired
  through voice/effect/route/summer/adapter/trigger teardown and reload port diffs;
  exhaustion now returns `Error::IdsExhausted` instead of panicking the runtime task
- [x] **CR-13** node_id and buffer_id allocators never reclaim — buffer IDs freed on
  sample/SFZ unload and recording cancel; node IDs recycled on group/effect/route
  teardown and on voice delete/stop, polyphony eviction, choke, and same-pitch
  retrigger (all explicit `/n_free` paths); all four `expect()` exhaustion panics
  converted to `Error::IdsExhausted` results. Still leaked (ambiguous lifetime,
  skipped deliberately): node IDs of gate-released nodes (`note_off`,
  `graceful_delete`) and self-freeing one-shot synths (doneAction=2, incl. the
  MIDI-fallback packed-event synths) — reclaiming those safely needs `/n_end`
  bookkeeping that dedupes against explicit-free recycling
- [x] **CR-14** Fade duration not validated — minimum 1/64th note
- [x] **CR-15** Pattern swing — already clamped (false positive)

## SFZ Subsystem (2026-07-06)

- [x] **SFZ-1** `#include` / `#define` preprocessor — implemented in
  `vibelang-sfz/src/parser/preprocess.rs` (recursive includes relative to the
  including file, cycle detection, depth limit 32, textual `$VAR` substitution
  with later-definition override); wired in before tokenization in
  `parse_sfz_file` / `parse_sfz_str`
- [x] **SFZ-2** Unknown opcodes silently ignored — now collected during parse
  (`SfzFile::unknown_opcodes`, numbered `*occN` forms normalized) and surfaced
  as ONE summarized warning per file; `<curve>`/`<effect>` sections exempt
- [x] **SFZ-3** Dropped regions untracked — loader now reports
  `N regions parsed, M loaded, K dropped (reason: count, ...)` once per
  instrument (`SfzInstrument::diagnostics`), re-surfaced by
  `handlers/sfz.rs::load` via tracing
- [x] **SFZ-4** (CRITICAL) SFZ region matching never ran at note-on — fixed:
  `note_on_audio_at` now calls `handlers::sfz::sfz_note_spawn_params` when the
  voice has an SFZ instrument (correct region/buffer, repitching, ampeg
  envelope, note-off release); notes with no matching region are skipped
  instead of playing buffer 0

## Audio Path / Hot-Reload Hotlist Close-Out (2026-07-06)

All ten hotlist items landed (reload staging off the tick task, gate-release
teardown, route-mixer placement, effect chain order, pattern anchoring +
content swaps, MIDI dispatch threads, param diff snapshots, synthdef body
hashing + shared-port reconcile, de-click package, sample/SFZ buffer-swap
safety). Known limitations deliberately left open:

- [ ] **AP-1** Buffers displaced by a sample/SFZ content reload are freed
  after a fixed 500 ms grace; a loop-mode note still reading past that goes
  silent (no glitch). Proper fix needs per-buffer node refcounting —
  adjacent to the `/n_end` bookkeeping already noted in CR-13
- [ ] **AP-2** Direct `SampleMessage::Load`/`SfzMessage::Load` (non-reload
  path) still load inline on the runtime task; only reload rides the
  off-task staging plan
- [ ] **AP-3** SFZ change detection is path-only: editing a .sfz's
  *referenced sample files* in place isn't detected (the buffer-swap
  discipline keeps it safe once detected)
- [ ] **AP-4** Route-diff mixer frees (re-routing a live voice) are still
  immediate, not grace-deferred — re-routing mid-note can truncate the
  routed tail
- [ ] **AP-5** Rapid reloads queued during staging apply every intermediate
  state in order rather than latest-wins

## Performance Sweep 2 (2026-07-08)

Second perf/quality pass over the audio path, reload system, and startup.
All ten landed; verified by `cargo test` (1027 core/dsp/sfz tests green) and
the resynthesizer smoke (`examples/resynthesizer/smoke.sh` PASS).

- [x] **PS2-1** 50 ms `thread::sleep` per synthdef deploy removed
  (`vibelang-dsp/src/api.rs` `deploy_synthdef_ir`/`deploy_fx_ir`) — was
  ~98% of script-eval time on every reload. Deploy ordering already
  guaranteed by the backend's `/done d_recv` await + the CLI's post-eval
  `sync_and_wait` barrier.
- [x] **PS2-2** Byte-identical synthdefs are hash-skipped on reload (compare
  `get_synthdef_hash` before send; register hash only after a successful
  `deploy_bytes`). Hash registry is now cleared on scsynth (re)connect
  (`backends/scsynth.rs::connect`) so a fresh server always gets a full
  re-send — required for correctness once deploys are skipped.
- [x] **PS2-3** Built-in synthdef load does ONE barrier `/sync` after the
  batch instead of a per-def sync (`handlers/synthdefs.rs`) — each def's
  load is already confirmed by its own `/done`. Cuts ~N startup round trips.
  Also: `/tmp/<name>.scsyndef` debug dump gated behind `VIBELANG_DUMP_SYNTHDEFS`.
- [x] **PS2-4** Per-synthdef param defaults memoized as `Arc<HashMap>` in
  vibelang-dsp (`get_synthdef_param_defaults_arc`), invalidated at every
  registry write + on reconnect — was rebuilt 3-5×/note under the State
  write lock just to check for a `gate` param.
- [x] **PS2-5** SFZ note-on region matching: per-note String/Vec allocation
  churn eliminated (`&'static str` params, no per-RR `format!` key, trigger
  info built only for the selected region), plus a 128-slot note→region
  bucket index (`SfzInstrumentState::note_index`) so the note-on scan is
  O(regions-covering-the-note) with a full-scan fallback.
- [x] **PS2-6** MIDI output threads are event-driven (`crossbeam select!`
  with a 1 ms spin window near the deadline + 50 ms shutdown cap) instead
  of a 10 kHz poll — was 10k idle wakeups/sec per device.
- [x] **PS2-7** Pattern-content fades ride a `PatternState::fade_overlay`
  (one float write/tick, merged at trigger time) instead of cloning all
  steps + rebuilding the `Arc<PatternContent>` every 2 ms tick; final value
  flushed into content once on completion/cancel. `Patterns::set_param` and
  the sequence fade-start share the one-shot `write_param_to_all_steps`.
- [x] **PS2-8** File watchers filtered to `.vibe` scripts (both `-w` and TUI
  watchers); TUI watcher flipped to Recursive so subdir imports are seen —
  stops WAV renders / editor swaps / git ops from triggering full reloads.
- [x] **PS2-9** Reload diff no longer deep-clones every entity config to test
  equality (`reload/diff.rs` `diff_entities` takes an in-place
  `matches_config` probe; state types compare against their `Arc<Content>`
  without cloning). `snapshot_script_config` moves the script param maps out
  of the owned `new_state` instead of cloning (incl. the no-change path).
- [x] **PS2-10** SFZ sample buffers load concurrently (`buffer_unordered(8)`)
  with buffer IDs pre-allocated in deterministic order, instead of one
  serial `/b_allocRead` round trip per sample.

## Low (deferred)

- [ ] **CR-16** Voice auto-syncs before configuration
- [ ] **CR-17** Missing documentation on helpers
- [ ] **CR-18** Magic numbers in epsilon calculation
- [ ] **CR-19** Lossy f64→f32 undocumented
- [ ] **CR-20** Static atomic counter in melodies handler
