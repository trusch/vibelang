# VibeLang Release Notes Draft

Working draft for the next VibeLang release. This file is release-prep
material, not the final tagged release notes.

## Release Scope / Highlights

- **CV-to-param routing is back end-to-end.** `.output_kr(...).to_param(target, param)`
  (source-first SET) and `target.param(param).modulate_by(source, port)`
  (target-first BEND) reach live `/n_map` again, with per-source
  `.scale(x).offset(y)` shaping threaded through the runtime summers.
- **Legacy `modulator()` API removed (BREAKING).** CV sources are now plain
  voices whose synthdefs declare `.output_kr(...)` ports and wire via
  `.to_param(...)` / `.modulate_by(...)`. The `stdlib/modulators/*` namespace
  is gone; LFO/envelope/follower defs live alongside other synthdefs.
- Named-input processors are now part of the public Rhai and runtime surface:
  custom synthdefs can declare mono/stereo audio inputs and scripts can patch
  them from voices, groups, current group audio, or silence.
- The standard library has expanded toward modular/rack workflows, including
  Make Noise ReSynthesizer-inspired modules and patch examples, Erica,
  Mutable, Verbos, and Intellijel-inspired performance patches, and additional
  named-input stereo processors.
- The ReSynthesizer demo now articulates with real stereo motion end-to-end
  (Rene-driven pitch, envelope-gated Spectraphon, mimeophon feedback halo, X-PAN
  stereo spread), not the previous drone.
- Synthdef reliability improved for live scsynth use through preflight
  rejection handling, pseudo-UGen/operator lowering, cleaned UGen manifests,
  automatic `MaxLocalBufs` insertion for `LocalBuf` graphs, and a corrected
  BufWr UGen codegen that finally updates server buffers from running synths.
- ReSynthesizer now has a bounded scsynth smoke harness that checks the patch
  reaches transport startup without known server/runtime regression output.

## Breaking Changes

- **`modulator(...)` removed; migrate to CV-source synthdefs.** The
  `define_modulator`, `modulator`, and `stdlib/modulators/*` surfaces are
  gone. Scripts that used `let lfo = modulator(...)` now write
  `let lfo = voice("sine_lfo")` (or the equivalent CV-source synthdef) and
  route the kr output with `lfo.output("out").to_param(target, "freq")`
  or `target.param("freq").modulate_by(lfo, "out")`. Stdlib LFO/envelope
  defs were migrated to the new namespace; user scripts that import the
  old paths will fail to parse and need to switch to the new synthdef IDs.

## New Features and Improvements

- **CV-to-param scale/offset shaping**: `.to_param(target, param).scale(x).offset(y)`
  and the target-first equivalent on `modulate_by(...)` are honored end-to-end
  by the runtime — the value the target sees is `source * scale + offset`,
  applied inside the `param_kr_modulate_N` summer the runtime spawns.
- **Fan-in to one param**: multiple kr sources can drive the same `(target,
  param)`; the runtime allocates a `param_kr_modulate_N` summer (N up to 8),
  rebuilds on add/remove, and collapses to direct `/n_map` at N=1 or unmaps
  at N=0.
- **Fan-out from one kr port**: a single `.to_param(...)` source fans out
  additively across distinct `(target, param)` pairs.
- **Auto owned-output-buses**: any synthdef declaring `.output(...)` ports
  automatically gets `out=voice.output_buses[0]` wiring; the manual
  `__vibelang_owned_output_buses=1.0` opt-in is gone. User
  `set_param("out", N)` precedence is preserved via merge ordering.
- **Mixed-rate synthdefs**: a single synthdef can declare audio and
  control-rate outputs (e.g. an audio voice that also taps a kr envelope),
  with per-port routing terminal verbs.
- **Group-to-hardware routing**: `group("g").output(N)` pins a group to a
  single mono hardware bus; `group("g").output([L, R])` pins to a stereo
  pair on consecutive buses.
- **Named-input synthdef builders**: `define_synthdef(...)` supports
  `.input("name")` and `.input("name", channels)` for declared audio-rate
  inputs consumed through `p.inputs.<name>` in `body_map` processors.
- **Script-side input patching**: scripts can route audio with
  `target.input("name").from(source)`, including voice, group, current-group,
  and silent sources. A stereo input named `in` can default to the parent group
  pre-fader bus for inline processor workflows.
- **Expanded modular stdlib catalogue**: recent additions include
  ReSynthesizer core modules (`spectraphon`, `morphagene`, MATHS, TEMPI,
  Rene, Wogglebug, PrssPnt, CV Bus, X-PAN, QPAS, DXG, Mimeophon), Erica Techno
  System modules, Mutable/Verbos/Intellijel rack modules, and related
  performance examples under `examples/`.
- **Pseudo-UGen compatibility**: priority/operator pseudo UGens now lower into
  supported graph forms, reducing common "UGen not installed" failures when
  importing sclang-derived synthdefs.
- **Synthdef preflight**: scsynth synthdef load failures are surfaced before
  the runtime commits unusable synthdef state.
- **Release smoke coverage for ReSynthesizer**:
  `examples/resynthesizer/smoke.sh` runs a bounded release-binary check with a
  temporary `XDG_DATA_HOME`, avoiding stale embedded stdlib caches.

## Fixes / Stability Work

- **CV-to-param runtime mapping**: a route-finalization phase regressed during
  a prior revert wave; `.to_param`/`.modulate_by` no longer reached the target
  synth at runtime. The mapping is restored, including scale/offset shaping
  and structural-recreate rebinding of unchanged SET routes.
- **Param-route lifecycle hardening**: active SET/BEND/TRIGGER mappings are
  now applied to newly-spawned voice nodes (note-on, trigger); BEND-routed
  notes spawned mid-reload inherit the user's last `set_param` baseline
  instead of reading `0` until the next set_param; voice teardown refreshes
  param routes so summers do not survive a deleted target.
- **Output port rate flips**: an ar→kr / kr→tr / etc. port rename or rate
  flip frees the old-rate adapter/summer and respawns the right shape for
  the new rate, dropping dependent routes with a warning instead of leaving
  zombie state.
- **BufWr UGen codegen**: BufWr's input ordering and num_outputs encoding
  were wrong; running synths could not write to server buffers. With the
  codegen fix, Spectraphon's inline SAM analyzer populates `mag_buf` for real
  and buffer-bake flows actually capture audio.
- **Spectraphon SAM gain normalization**: Spectraphon's partial-bank sum is
  now unit-spectrum normalized at read time so analyzer-driven partial banks do
  not saturate the output bus.
- **TEMPI gate generator**: replaces a narrow phase-window comparator
  (unreliable at kr sample rate) with wrap-edge detection, so clock pulses
  fire reliably regardless of `width` parameter or kr alignment.
- **Owned output buses**: synthdefs with declared `.output(...)` ports route
  through private source buses instead of leaking to bus 0; the previous
  manual `__vibelang_owned_output_buses` opt-in workaround is gone.
- **Legacy implicit-`out` synths**: route through the owned source bus so
  recirculating stateful UGens (BufWr feedback, comb delays, mimeophon halo)
  no longer collapse to a single tap when wired through named-input
  processors.
- **Voice input ordering**: voices with named-input routes are added after
  their `input_link` anchor in scsynth tree order, avoiding stereo-collapse
  caused by reading from a not-yet-running mixer.
- **MIDI looper hardening** (`60e9bd1`): reliable transport start, paired
  quantization at record/play boundaries, identity-aware note-off
  (generation-counter avoids stale same-pitch off events), and beat capture
  from MIDI arrival timestamps instead of poll time.
- **ReSynthesizer demo**: stereo articulated patch (peak ~−3 dBFS, RMS ~−18
  to −15 dBFS, ~12 dB half-second dynamics, real spectral motion 400–1800 Hz)
  replaces the prior drone. Mimeophon feedback halo tail audible at the wet
  output; per-side delay/pan and split odd/even taps produce L/R correlation
  ~0.0 instead of bit-identical mono.
- Original draft fixes still apply:
  - ReSynthesizer scsynth startup cleaned up to avoid known failure strings
    (missing UGens/synthdefs, `LocalBuf` allocation failures, excessive grain
    counts).
  - Non-binary UGen manifests cleaned up; required SuperCollider plugin
    documentation added in `kb/required-sc-plugins.md`.
  - `LocalBuf` graphs auto-insert `MaxLocalBufs` when needed.
  - Morphagene clocked grains debounced to avoid runaway retriggering.
  - Named input route reconciliation drops dependent routes and frees
    runtime resources when inputs are renamed, removed, or have width
    changes.
  - Width mismatches (e.g. routing stereo/group sources to mono inputs)
    are rejected rather than implicitly downmixed.

## Internal / Architecture

- **Routing subsystem cleanup epic** (commits `946eddd`, `06cb5ce`, `0bc8ad7`,
  `c8651b9`, `66e43e2`, `ec78ce3`): no user-visible change, but a substantial
  internal refactor that prevents the next regression hunt from following the
  trajectory the CV-to-param hunt did:
  - `ReloadDiff::has_changes()` is the single source of truth for
    "did anything change", covering routes / param_routes / input_routes /
    voice-port reconciles plus entity changes.
  - SET / BEND / TRIGGER route registration is now one parameterized code
    path per layer (`ParamRouteKind`) instead of three near-duplicates.
  - `VoiceRole { Audible, ModulatorOnly }` lives on `VoiceState`;
    effective output routes and default-route suppression are computed once
    at diff time and stored on `ReloadDiff`. The old distributed heuristic
    + callback plumbing are gone.
  - `pending_voice_port_reconciles` no longer mutates `new_state` at diff
    time; route-stripping happens entirely in the apply phase against the
    live State.
  - `Runtime::current_routes` was relocated onto `State` so it cannot
    desync from `route_synths` across panic recovery / direct mutation /
    snapshot-restore paths.
  - `Runtime::apply_reload` was 1585 lines of inline `Phase 1`, `Phase 2`,
    `Phase 4.7`, `Phase 6.6` etc.; it is now a 32-line orchestrator that
    calls 14 named `phase_*` methods with single-line doc comments. Numeric
    phase labels are gone.

## Manual Verification Playbook

Run these checks from `/home/trusch/projects/music/vibelang` unless a command
explicitly changes directory. Record exact command output, exit code, audio
device/JACK/PipeWire setup, and any failures in the release ticket.

### Automated Checks

```bash
bash -c "cargo fmt --all -- --check"
```

Expected outcome: exits 0 with no formatting diff.

```bash
bash -c "cargo check --workspace"
```

Expected outcome: exits 0 for all workspace crates.

```bash
bash -c "cargo test --workspace"
```

Expected outcome: exits 0. Any test that requires host audio/MIDI hardware
must be called out separately rather than treated as a generic failure.

```bash
bash -c "cargo clippy --workspace"
```

Expected outcome: exits 0, or only reports warnings already accepted by the
release owner.

```bash
bash -c "cargo build --release -p vibelang-cli"
```

Expected outcome: builds `target/release/vibe`.

```bash
bash -c "cd vscode-extension && npm ci && npm run compile"
```

Expected outcome: TypeScript compile succeeds and stdlib metadata generation
finishes without schema or import errors.

```bash
bash -c "cd tests/integration && ./run_tests.sh --quick --verbose"
```

Expected outcome: integration scripts start, assertion markers show pass
counts, and no unexpected parse/runtime errors are printed. Note: the runner
currently references historical `vibe2` wording in comments; verify the actual
built binary path before treating a missing binary as a product regression.

### CV-to-Param Routing Smoke

A quick repro that a CV source modulates a target param at runtime:

```bash
RUST_LOG=info ./target/release/vibe run --no-watch --no-api --no-jack-connect examples/maths_to_param.vibe
```

Expected outcome: OSC traffic includes `/s_new param_kr_modulate_1 ...
scale_a=... offset_a=...` for the summer and `/n_map [target_node, "freq",
source_bus]` for the param map. Audibly, the target voice's frequency sweeps
across the scale/offset range.

### Live Audio / scsynth Checks

```bash
bash -c "cargo build --release -p vibelang-cli && bash examples/resynthesizer/smoke.sh"
```

Expected outcome: `PASS: resynthesizer smoke reached transport startup with no
known regression output`. The smoke log should contain `Transport started` and
must not contain missing UGen/synthdef, `Message too long`, `LocalBuf`, buffer
allocation, or `Too many grains` errors.

```bash
RUST_LOG=info ./target/release/vibe run --no-watch --no-api --no-jack-connect examples/resynthesizer/main.vibe
```

Expected outcome: bounded manual run reaches transport startup. Stop with
Ctrl-C after confirming startup and checking logs.

```bash
RUST_LOG=info ./target/release/vibe examples/resynthesizer/main.vibe
```

Expected outcome: live run boots scsynth, connects to the normal JACK/PipeWire
output path, starts transport, and remains stable during several edits to the
parameters listed in `examples/resynthesizer/README.md`. Audibly: stereo
articulated patch — peak ≤ −3 dBFS, RMS roughly −18 to −10 dBFS, dynamic
spectral motion, audible mimeophon feedback tail, non-mono L/R image.

ReSynthesizer-specific listening checks:

- Confirm Morphagene dry layer, Spectraphon odd/even partial banks, X-PAN,
  QPAS, DXG, and Mimeophon contribute audible changes when their documented
  parameters are edited.
- While watch mode is running, edit `morphagene_reel.organize`,
  `spectraphon_voice.partials`, `qpas_filter.cutoff`, and
  `mimeophon_space.mix`; expected outcome is hot-reload without killing audio
  and a clear timbral change for each edit.
- Run one full smoke pass with a fresh temporary stdlib cache, then one live
  run using the normal user cache; expected outcome is no difference in missing
  synthdef/UGen behavior.
- Pending fill-in: `measure-resynthesizer-audio-output-energy` should supply
  numeric non-zero output evidence with this release's articulated patch
  (duration, sample rate/channel count, RMS/peak/non-zero sample count,
  capture method, command output).

Additional live examples:

```bash
RUST_LOG=info ./target/release/vibe run --no-watch --no-api --no-jack-connect examples/mutable_rack/main.vibe
RUST_LOG=info ./target/release/vibe run --no-watch --no-api --no-jack-connect examples/verbos_westcoast/main.vibe
RUST_LOG=info ./target/release/vibe run --no-watch --no-api --no-jack-connect examples/erica_techno/main.vibe
RUST_LOG=info ./target/release/vibe run --no-watch --no-api --no-jack-connect examples/intellijel_perform/main.vibe
```

Expected outcome: each example reaches transport startup without missing stdlib
imports, missing UGens, or synthdef load rejections. For release sign-off, at
least one patch should also be listened to through the normal audio output path.

### MIDI / Manual Hardware Checks

```bash
./target/release/vibe devices
```

Expected outcome: available MIDI input and output ports are listed without a
panic. If no MIDI devices are attached, record that as environment coverage,
not as a release blocker.

```bash
RUST_LOG=info ./target/release/vibe run --input-channels 2 --output-channels 2 examples/midi_callback.vibe
```

Expected outcome: with a MIDI controller attached, incoming events reach the
script callback and the runtime continues cleanly while notes/CCs are sent.

For the MIDI looper hardening (`60e9bd1`): with a controller attached, record
a short loop with one held note bridging the quantize boundary, then play it
back. Expected outcome: the recorded note plays back at the correct beat
offset, paired note-on/off ride through the same quantize step, and a stale
same-pitch off event from a prior take does not silence the current take.

For hardware-output routing, run with the actual target port names discovered
from `pw-link -i`, `pw-link -o`, or `jack_lsp`:

```bash
RUST_LOG=info ./target/release/vibe run --output-channels 10 --input-channels 8 --jack-connect-to "PORT_L,PORT_R" examples/resynthesizer/main.vibe
```

Expected outcome: main stereo output reaches the requested hardware ports, and
group/hardware bus allocation does not collide with hardware input or output
buses. Record the real port names used for the release notes.

For CV calibration hardware, use the calibration examples and document measured
voltage ranges:

```bash
RUST_LOG=info ./target/release/vibe examples/cv_calibration.vibe
```

Expected outcome: the target output jack follows the documented calibration
signal. Record peak voltage or tuning error if this check is performed.

### Editor / VS Code / LSP Checks

```bash
bash -c "cargo build --release -p vibelang-cli"
```

Expected outcome: `target/release/vibe lsp` is available for editor testing.

```bash
printf 'Content-Length: 107\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"processId\":null,\"rootUri\":null,\"capabilities\":{}}}' | ./target/release/vibe lsp
```

Expected outcome: the server responds with an LSP initialize result or a
well-formed JSON-RPC response; no panic or immediate process crash.

```bash
bash -c "cd vscode-extension && npm run compile"
```

Expected outcome: generated stdlib metadata and TypeScript compile complete.

Manual VS Code checks:

- Install or run the local extension package from `vscode-extension/`
  (the `.vscodeignore`-driven packaging now defaults to the `vibe` binary
  per `d891dca`).
- Open a `.vibe` file and confirm syntax highlighting, diagnostics,
  completion/hover, and format document still work.
- Start a runtime with `./target/release/vibe run examples/resynthesizer/main.vibe`,
  then use the extension's connection, session explorer, mixer, inspector,
  arrangement, sound designer, pattern editor, melody editor, sample browser,
  and effect rack commands.
- Expected outcome: views open without webview errors, runtime connection state
  is visible, and transport/pattern/melody commands affect the running session.

## Migration Notes

- **Scripts using `modulator()`**: replace with a voice on a CV-source
  synthdef (e.g. `sine_lfo`, `decay2_kr`, `envelope_follower`) and wire the
  kr output:
  ```rhai
  // OLD: let lfo = modulator("sine_lfo").freq(0.5);
  //      lfo.to_param(target, "cutoff");

  let lfo = voice("sine_lfo").set_param("freq", 0.5);
  lfo.output("out").to_param(target, "cutoff");
  // or, target-first:
  target.param("cutoff").modulate_by(lfo, "out");
  ```
- **Scripts using `__vibelang_owned_output_buses`**: drop the param; any
  synthdef declaring `.output(...)` now opts in automatically.
- **Importing stdlib modulators**: the `stdlib/modulators/*` path no longer
  exists; the migrated synthdefs live under the regular instrument/processor
  namespaces. Re-import paths according to the new layout.

## Known Caveats / Follow-Up Tickets

- `measure-resynthesizer-audio-output-energy` is the required release
  follow-up for numeric proof that the new articulated ReSynthesizer patch
  emits non-zero main output audio with the documented level/dynamics. Earlier
  captures (e.g. `examples/resynthesizer/captures/resynth-stereo-full-20260517-135252/`)
  exist but should be redone against the final release build for the record.
- `fix-audit-voice-deletion-param-summer-teardown-timing` is a deferred
  follow-up from the CV-to-param fix; it has no known active symptom but is
  open as a structural audit.
- The working tree at draft time still carries unrelated dirty / untracked
  files (resynthesizer probe scripts and audio captures, VS Code extension
  VSIX artifact, vibelang-rhai source-port input API edits in
  `crates/vibelang-rhai/src/api/route.rs`, modified tests in
  `crates/vibelang-core/tests/owned_output_bus_params.rs` and
  `crates/vibelang-rhai/tests/named_input_routes_runtime.rs`, modified
  `examples/resynthesizer/main.vibe` + `README.md`, plus `tmp-*.txt`).
  Release signing should start from an intentionally reviewed tree; this
  draft did not clean or classify those changes.
- ReSynthesizer is a patch-semantic approximation, not an exact clone of Make
  Noise panel behavior, firmware, storage, analog circuits, Select Bus,
  capacitive touch, or hidden modes.
- Control-rate named input jacks are not public yet; input routes cannot choose
  non-default source outputs; fan-in verbs such as `from_all` / `add_from` are
  still deferred.
- Fan-out of one audio output port to multiple **groups** remains deferred;
  fan-out to multiple `(target, param)` pairs from one kr port is supported.
- Required SuperCollider plugin coverage should be checked against
  `kb/required-sc-plugins.md` on the release host.
