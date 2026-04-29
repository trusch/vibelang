# Morphagene Reel persistence — pure .vibe pattern

> Story E of the morphagene synthdef plan. See
> `kb/morphagene-synthdef-plan.md` §3 + §6.E for the original
> hardware-targeted spec, and the file header in
> `crates/vibelang-std/stdlib/instruments/sampler/morphagene.vibe` for
> the as-shipped DSP details.

The Story E "Reel persistence" goal — Reels that survive hot-reload —
lives entirely at the script tier. There is **no** Rust-side Reel
registry, **no** parallel `Reel` core type, and **no** new buffer
allocator: the existing top-level `allocate_buffer(name, frames,
channels)` Rhai API (commit `7a6eebe`) already gives us a
script-allocated SC buffer whose contents survive synthdef recompiles
as long as the call stays in the script.

The morphagene stdlib file ships a thin set of helpers that wrap that
API into a "Reel" abstraction: a Rhai object map that also carries a
splices array and a few attach / fill helpers.

## API surface

All helpers live in `crates/vibelang-std/stdlib/instruments/sampler/morphagene.vibe`
— `import` that file to use them.

### `reel(name, frames, channels) -> #{...}`

Allocates a persistent Reel buffer via `allocate_buffer`. Returns an
object map with:

| Key        | Type   | Meaning                                                     |
|------------|--------|-------------------------------------------------------------|
| `name`     | string | Script-side name (also the `allocate_buffer` key).          |
| `bufnum`   | float  | SC bufnum — feed to `voice.set_param("bufnum", r.bufnum)`.  |
| `frames`   | float  | Buffer length in frames.                                    |
| `channels` | int    | 1 or 2.                                                     |
| `splices`  | array  | Splice positions (0..1). Initially empty.                   |

Hot-reload semantics are inherited from `allocate_buffer`: same name +
same `(frames, channels)` ⇒ same SC bufnum ⇒ buffer is not freed and
contents persist.

### `reel_attach(voice, reel) -> voice`

Wires `bufnum` and `num_splices` onto a morphagene voice. Returns the
voice for chaining. `num_splices` is `reel.splices.len()` clamped to
≥ 1 — an empty splices array means "single-splice whole-Reel reader".

### `reel_fill_preset(reel, preset) -> voice`

Spawns a one-shot `morphagene_reel_fill` voice that writes a generated
waveform into the Reel via `record_buf_ar` with `loop=0` +
`doneAction=2` — the synth frees itself when the buffer is full.
`preset` selects the waveform: 0 = saw, 1 = square, 2 = sine, 3 = pink
noise, ≥4 = silence.

**Re-runs on every script execution that calls it.** To keep SOS
overdubs safe across hot-reload, gate the call behind a flag:

```rhai
let bake_default = true;        // first run
// let bake_default = false;     // subsequent runs
if bake_default { reel_fill_preset(r, 0); }
```

## Worked example

See `examples/morphagene_reel_persist.vibe` for a full script that
allocates a 10-second Reel, bakes a saw default, sets up 4 evenly-spaced
splices, and sweeps Organize via an LFO.

The minimal pattern is:

```rhai
import "stdlib/instruments/sampler/morphagene.vibe";

let r = reel("my_reel", 480000, 1);
r.splices = [0.0, 0.25, 0.5, 0.75];      // metadata; synthdef divides evenly

if false { reel_fill_preset(r, 0); }      // optional one-shot bake

let v = voice("morph")
    .synth("morphagene")
    .set_param("amp", 0.5);
reel_attach(v, r);                        // sets bufnum + num_splices

melody("morph_hold").on(v).notes("C3").apply();
```

## What's deferred

| Feature                             | Status     | Where it lives                                      |
|-------------------------------------|------------|-----------------------------------------------------|
| Custom (uneven) splice positions    | Deferred   | Needs a synthdef-level read of a positions array.   |
| `.wav` cue-marker → splice import   | Deferred   | Open a follow-up `vibelang-sfz` ticket — the wav loader needs to populate cue markers on `SampleHandle` first. |
| Reel save (`.wav` writer)           | Deferred   | Off-DSP runtime path; not script-tier.              |
| Multi-Reel hot-swap (registry)      | Trivial    | Allocate multiple Reels with distinct names; pick which one to attach via Rhai conditionals. |
| Reel color cycling (UI parity)      | Out of scope | Display tier, not the synthdef.                  |

## .wav-backed Reels (alternative path)

When you want a Reel seeded from an existing `.wav` and don't care about
SOS-overdub persistence, the existing `sample()` API still works:

```rhai
let reel_buf = sample("reel", "samples/loop.wav").loop_mode(true);
let v = voice("morph").on(reel_buf).synth("morphagene") /* ... */;
```

`voice.on(sample_handle)` wires the SC bufnum from the loaded `.wav`.
The cost is that hot-reload re-loads the `.wav`, so any SOS-overdubbed
content during the previous session is overwritten by the on-disk
source on every edit. Use the `reel(...)` + `allocate_buffer` path
when SOS overdubs must survive hot-reload.
