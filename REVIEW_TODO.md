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

- [ ] **CR-8** Duplicate `parse_note_name` — helpers.rs vs midi/callbacks.rs
- [x] **CR-9** Voice with empty synthdef — now logs warning
- [x] **CR-10** Pattern `.on()` doesn't verify voice exists — now warns
- [x] **CR-11** MIDI channel convention — standardized to 1-16 (musician convention)
- [ ] **CR-12** ControlBusAllocator never reclaims buses
- [ ] **CR-13** node_id and buffer_id allocators never reclaim
- [x] **CR-14** Fade duration not validated — minimum 1/64th note
- [x] **CR-15** Pattern swing — already clamped (false positive)

## Low (deferred)

- [ ] **CR-16** Voice auto-syncs before configuration
- [ ] **CR-17** Missing documentation on helpers
- [ ] **CR-18** Magic numbers in epsilon calculation
- [ ] **CR-19** Lossy f64→f32 undocumented
- [ ] **CR-20** Static atomic counter in melodies handler
