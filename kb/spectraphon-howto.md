# Spectraphon — How-To

> User manual for the `spectraphon_side` and `spectraphon_dual` synthdefs in
> `vibelang-std`. Pairs with the deeper analysis in
> `kb/spectraphon-synthdef-plan.md`.
>
> **Status snapshot:** Stories A through E are merged. Story F (this doc +
> the matching examples) wraps the v1 surface. Sync mode (Story D), a true
> dual-side SAM analyzer, and a script-allocated persistent Array buffer
> are documented as future work below.
>
> **Multi-output revision:** `spectraphon_side` now exposes four named
> output ports — `sine`, `sub`, `odd`, `even` — wired per-port via the
> `voice.output("name").to(group)` API. The §2 parameter table below
> still references the legacy `merge_even` / `right_mode` knobs from the
> stereo-output revision; consult the synthdef source for the current
> shape and see `examples/spectraphon_multiout.vibe` for a worked routing
> patch. Full routing surface is documented in
> `kb/voice-multioutput-howto.md`.

The `spectraphon_*` synthdefs recreate the playback engine of the Make
Noise / Soundhack Spectraphon as a vibelang additive instrument. Two
synthdefs ship today:

* **`spectraphon_side`** — one full Spectraphon side, including the SAM
  real-time analyzer, 16-Array memory + capture/recall, and the
  Spectranoise Chaos and Noise modes. Stereo output mirrors the hardware's
  Odd / Even (or Sine / Sub-CV) jacks.
* **`spectraphon_dual`** — both sides wired through the audio-rate FM bus
  with a Follow-mode pitch ratio menu. SAO playback with the default
  saw-spectrum only (the SAM analyzer + Array memory live in
  `spectraphon_side`).

Both synthdefs are built from unrolled additive banks of 32 odd + 32 even
`sin_osc_ar` voices per side (`klang_ar` would exceed Rhai's 16-arity
limit; see `kb/spectraphon-synthdef-plan.md` §7 for the fallback
rationale).

---

## 1. Mental model

```
                 ┌────────────────── Spectraphon side ───────────────────┐
                 │                                                       │
  In ───SAM────▶ │  Analyze ───▶ mag[1..64] ─┐                          │
                 │                            ├──▶ Harmonic bank ──┬──▶ Odd  (k=1,3,…) → L
                 │  Array ──SAO─▶ mag[1..64] ─┘   freq = f0          ├──▶ Even (k=2,4,…) → R
                 │   (16 stored, addressed by Slide × Focus)         │
                 │                                                    └──▶ Sine / Sub-CV
                 │  Chaos / Noise (Spectranoise) → bypass the bank          (right_mode 1/2)
                 └───────────────────────────────────────────────────────┘
```

Three things to keep separate in your head:

1. **Pitch** (`freq`) is the side's own oscillator fundamental. The
   harmonic bank's partial frequencies are integer multiples of it.
2. **Spectrum** comes from a separate source — either real-time SAM
   analysis of the audio input, an SAO Array lookup, or a Spectranoise
   mode synthesises it directly.
3. **Partials** (`partials`) is a global gate that progressively reveals
   harmonics low-to-high as it sweeps CCW → CW. CCW = silent on the bank;
   CW = full spectrum at stored amplitudes. Sine and Sub-CV bypass it.

---

## 2. `spectraphon_side` parameters

Source: `crates/vibelang-std/stdlib/instruments/spectral/spectraphon_side.vibe`.

| Param | Default | Range | Meaning |
|---|---|---|---|
| `freq` | 220.0 | Hz | Side's oscillator fundamental. Harmonic bank emits at `k · freq` for k=1..64. |
| `partials` | 1.0 | `[0, 1]` | Progressive harmonic gate. `0` = bank silent (sine/sub still audible if `right_mode ≥ 1`); `1` = all 64 partials at stored amplitudes. Doubles as the Chaos AM depth and as a global VCA on the Even/Odd outputs. |
| `slide` | 0.5 | `[0, 1]` | **SAM** (mode 1/4): analyzer fundamental, log-mapped to `[50, 800]` Hz — picks which harmonic grid is tracked in the input (independent of `freq`). **SAO** (mode 0): X-axis position into the Array. **Chaos**: chaos-feedback amount. **Noise**: LP cutoff log-mapped to `[100, 16000]` Hz. |
| `focus` | 0.5 | `[0, 1]` | **SAM**: addresses the Array Y-axis on capture (the analyzer itself uses a 1-bin centre — see deviation in §9). **SAO**: Y-axis position into the Array. **Chaos**: second-sine pitch ratio mapped to `[1, 4]`. **Noise**: HP cutoff log-mapped to `[20, 2000]` Hz. |
| `amp` | 0.3 | `[0, ~1]` | Output amplitude (post-envelope, post-DC-block). |
| `gate` | 1.0 | 0/1 | Synth-instance lifecycle (note on/off). 10 ms attack, 300 ms release; `cleanup_on_finish()` frees on note-off. |
| `merge_even` | 1.0 | 0/1 | `1` (default) folds Even partials into the L (Odd) jack — matches the hardware's Even-into-Odd normalization when the Even jack is unpatched. `0` keeps L = Odd, R = Even. |
| `right_mode` | 0.0 | 0/1/2 | Selects the R-channel jack equivalent in SAM/SAO modes. `0` = Even jack (or Odd-summed when `merge_even=1`), `1` = clean Sine (k=1, ungated), `2` = Sub-CV (saw at `freq · 0.5` in SAO; envelope follower on the analyzer input in SAM). Ignored in Chaos/Noise modes. |
| `sam_capture` | 1.0 | 0/1/2/3/4 | Mode select, folded with capture flag (10-arg cap). See §3. |
| `array_idx` | 0.0 | 0..15 | Which of the 16 stored Arrays to address (read in SAO; write in mode 4). |

Output: stereo `[L, R]`. DC-blocked per channel.

> **Limit:** the SAM analyzer is hardwired to **hardware audio input
> channel 0** via `sound_in_ar(0.0)` — see §9 for the rationale and the
> one-line workaround for analyzing an internal bus.

---

## 3. Modes (`sam_capture`)

`sam_capture` is the mode selector (folded with the capture flag because
the synthdef body is at the 10-arg Rhai cap). Truth table:

| `sam_capture` | Mode | What drives the bank | Notes |
|---|---|---|---|
| `0` | **SAO** playback | Array lookup at `(slide, focus)` × `array_idx` | Bilinear interpolation across 4 cells. |
| `1` | **SAM** (no capture) | Real-time FFT of audio input ch 0 | Default mode. `slide` picks the analyzer fundamental; `focus` is unused in the analyzer (see §9 deviation). |
| `2` | **Chaos** (Spectranoise) | Paired sines + chaos feedback | Bypasses the bank. `partials` becomes audio-rate AM depth. |
| `3` | **Noise** (Spectranoise) | Filtered pink noise AM-modulating sine carriers | Bypasses the bank. `partials` is unused. |
| `4` | **SAM with capture** | Real-time FFT, *and* writes the current frame's `mag[1..64]` into the Array cell at `(slide_lo, focus_lo)` | Cell coordinates floor to the 8×8 grid; sweep the knobs while in this mode to fill the Array. |

> **Renumbering note:** in earlier revisions of this synthdef
> `sam_capture = 2` engaged capture. With Story E (Chaos/Noise) the
> capture flag moved to `sam_capture = 4` and values `2`/`3` were claimed
> for the Spectranoise modes. Update old patches accordingly.

---

## 4. Array memory (Story C)

Each side of `spectraphon_side` carries its own 16-Array bank:

```
local_buf_ir(1, 65536)
└─ Array 0..15
   └─ 8 × 8 grid (slide_lo, focus_lo cells)
      └─ 64 partial magnitudes
```

### 4a. Capturing into an Array

1. Set `sam_capture = 4` (SAM + capture) and pick a target Array via
   `array_idx`.
2. Drive audio at JACK input channel 0. The FFT (2048-frame, Hann, 50%
   hop) tracks harmonic magnitudes referenced to the analyzer
   fundamental that `slide` selects (`50..800` Hz log).
3. Sweep `slide` and `focus` over a few seconds. Each frame writes into
   the cell at `floor(slide·8)` × `floor(focus·8)`. Repeated visits to
   the same cell overwrite — the most recent frame wins.
4. Drop `sam_capture` back to `0` (SAO). The captured cells now feed the
   bank with bilinear interpolation between the 4 cells around `(slide,
   focus)`.

### 4b. Persistence

The Array buffer is `local_buf_ir`, so it lives **inside the synth
instance**. A few consequences:

* Captures **survive** parameter changes and held notes.
* Captures **do not survive** voice cleanup (note-off + envelope
  release) or synthdef hot-reload.
* The 16 Arrays are **not** shared across voices — two voices on the
  same `spectraphon_side` synthdef have independent banks.

A future story (`§6.C3` of the plan) adds a script-allocated buffer
parameter so Arrays survive voice lifecycle and are sharable.

### 4c. The default spectrum

Until you capture, the Array buffer reads back zero — SAO playback in an
empty Array will be silent. To audition the synthdef without capturing,
either:

* Stay in SAM mode (`sam_capture = 1`) with audio at JACK ch 0, or
* Use one of the Spectranoise modes (`2` or `3`), or
* Use `spectraphon_dual` instead — it bakes a `1/k` saw-like default
  spectrum directly into both sides' banks (Story A1).

---

## 5. Spectranoise — Chaos and Noise

### 5a. Chaos mode (`sam_capture = 2`)

Two sine voices per side:

* `carrier_a` at `freq` (reusing the side's clean sine).
* `carrier_b` at `freq · (1 + focus·3)` — `focus` picks the second-sine
  ratio in `[1, 4]`.
* Both carriers are AM-modulated by `fb_sine_l_ar` (linear-interpolating
  chaotic sine) whose modulation index and feedback both scale with
  `slide`.
* `partials` is the AM depth. At `partials=0` the pair is a clean
  detuned dyad; at `partials=1` each carrier is fully ring-modulated by
  the chaos voice.

L = `carrier_a + carrier_b · merge_even` × AM. R = `carrier_b ·
(1-merge_even)` × AM. `right_mode` is ignored.

### 5b. Noise mode (`sam_capture = 3`)

Pink noise → LPF (`slide` → cutoff in `[100, 16000]` Hz log) → HPF
(`focus` → cutoff in `[20, 2000]` Hz log) → AM with sine carriers at
`freq` (L) and `2·freq` (R). With `slide < focus` the bandpass collapses
to silence — useful musical edge.

`partials` and `right_mode` are unused in Noise.

---

## 6. `spectraphon_dual` parameters

Source: `crates/vibelang-std/stdlib/instruments/spectral/spectraphon_dual.vibe`.

| Param | Default | Range | Meaning |
|---|---|---|---|
| `freq_a` | 220.0 | Hz | Side A pitch. |
| `freq_b` | 220.0 | Hz | Side B pitch (used only when `routing_mode = 0`). |
| `partials` | 1.0 | `[0, 1]` | Shared per-partial reveal mask + global VCA across both sides. |
| `slide` | 0.5 | `[0, 1]` | Shared odd/even bank balance. `slide=0` favours even (×0.5 on odd), `slide=1` favours odd (×1.0 on odd, ×0.5 on even). Stand-in for the per-side SAO Array X-axis. |
| `focus` | 0.5 | `[0, 1]` | Shared per-partial reveal-cursor multiplier. `focus=0` → 0.5× scale (sparse); `focus=1` → 2.0× scale (dense). Stand-in for the SAO Array Y-axis. |
| `amp` | 0.3 | `[0, ~1]` | Output amplitude. |
| `a_fm_index` | 0.0 | `[0, 1]` | Amount of A's clean sine into B's FM input. Depth scales linearly with `freq_b`. |
| `b_fm_index` | 0.0 | `[0, 1]` | Amount of B's clean sine into A's FM input. Depth scales linearly with `freq_a`. |
| `routing_mode` | 0.0 | 0..4 | Pitch-tracking menu (see §7). |
| `gate` | 1.0 | 0/1 | Voice lifecycle. |

Output: 4-channel array `[odd_a, even_a, odd_b, even_b]`. Sum or pan
downstream for stereo.

> **Folded into one synthdef** because each side is already 64 sine
> voices and the param surface hits the 10-arg Rhai cap. Compared to
> `spectraphon_side`, the dual omits SAM, Array memory, Chaos, Noise,
> independent Slide/Focus per side, and `right_mode` / `merge_even`.
> Bring those back per-side by stacking two `spectraphon_side` voices
> instead — but you lose the shared FM bus.

---

## 7. Dual-channel routing (`routing_mode`)

Pitch-tracking menu, quantised to nearest integer in `[0, 4]`:

| `routing_mode` | Side B effective freq | Use |
|---|---|---|
| `0` | `freq_b` (independent) | Two independently pitched sides. |
| `1` | `freq_a` | Unison. |
| `2` | `freq_a · 1.4983` | Perfect fifth (≈ +7 ST). |
| `3` | `freq_a · 2.0` | Octave. |
| `4` | `freq_a · 2.9966` | Octave + fifth (≈ +19 ST). |

Modes `1..4` correspond to Spectraphon's hardware Follow position with
preset intervals (the hardware Follow uses a continuous offset; the
discrete menu here folds the offset into `routing_mode` to fit the
10-arg cap).

> **Sync mode is not implemented** in the v1 dual synthdef. The
> hardware's hard-sync of B's bank to A's f0 zero-crossings would
> require a custom phase-reset bank — see `kb/spectraphon-synthdef-plan.md`
> §7 risk row for the implementation options.

### 7a. FM bus

Each side's reference sine (clean, un-modulated) is the FM modulator for
the other side's f0:

```
freq_a_fm = freq_a    + b_fm_index · sin(freq_b_eff) · freq_a
freq_b_fm = freq_b_eff + a_fm_index · sin(freq_a)    · freq_b_eff
```

Depth is linear in the modulator's amplitude, scaled by the *carrier's*
fundamental — so the perceived FM character stays consistent across
pitch.

> **Naming convention** (matches the dual synthdef's parameter names,
> *inverts* the panel-name convention in the kb plan §2c): `a_fm_index`
> is the amount of A's sine into B; `b_fm_index` is the amount of B's
> sine into A. Read each as "amount of <named-side>'s sine *into* the
> opposite side".

---

## 8. Common patches

### 8a. Static SAO drone (single side, default Array)

```vibe
import "stdlib/instruments/spectral/spectraphon_side.vibe";
import "stdlib/effects/reverbs/reverb_jpverb.vibe";

let pad = voice("pad")
    .synth("spectraphon_side")
    .param("freq", 110.0)
    .param("partials", 0.7)
    .param("amp", 0.4)
    .apply();

voice("pad").fx("reverb_jpverb", #{ time: 4.0, mix: 0.5 });
voice("pad").start();
```

Same as `examples/spectraphon_drone.vibe`. Note: with `sam_capture = 1`
(default) and no audio at JACK ch 0, the SAM analyzer reads silence and
the bank stays quiet. For an audible drone with no input, switch to
Chaos: `.param("sam_capture", 2.0)`.

### 8b. SAM live spectral tracking

```vibe
let v = voice("track")
    .synth("spectraphon_side")
    .param("freq", 220.0)         // playback pitch (independent of input)
    .param("slide", 0.4)          // analyzer f0 ≈ 175 Hz (50·16^0.4)
    .param("partials", 0.9)
    .param("sam_capture", 1.0);   // SAM, no capture
```

Plays back whatever spectrum is currently being analyzed at JACK input
ch 0, but at 220 Hz fundamental — so a sung A2 input becomes a
synthesised A3 with the input's harmonic envelope.

### 8c. Chord via Follow + FM

See `examples/spectraphon_chord.vibe` for a complete walkthrough. Core
idea:

```vibe
let v = voice("chord")
    .synth("spectraphon_dual")
    .param("freq_a", 110.0)
    .param("routing_mode", 3.0)   // B = octave above A
    .param("a_fm_index", 0.15)    // light cross-FM
    .param("b_fm_index", 0.05)
    .param("partials", 0.8)
    .param("amp", 0.25);
```

`routing_mode = 3` locks B to A + 12 ST; the FM bus adds spectral motion
that varies with the unison interval.

### 8d. Array capture + recall

See `examples/spectraphon_array_capture.vibe`. Workflow: SAM → capture
sweeps → switch to SAO and play the captured spectrum back.

### 8e. Per-port multi-output routing

See `examples/spectraphon_multiout.vibe` for the full multi-output
surface in action: `sine` and `sub` go straight to main, `odd` runs dry
through the `leads` group, `even` is sent to a dedicated reverb bus.
This is the canonical reference for `.output("name").to(...)` on a
`spectraphon_side` voice — see `kb/voice-multioutput-howto.md` for the
full routing API.

---

## 9. Caveats and deviations

The deviations below are documented inline in the synthdef source; this
section collects them so users know what to expect.

* **SAM input is hardware ch 0 only.** `spectraphon_side` calls
  `sound_in_ar(0.0)`. To analyze an internal vibelang bus, edit the
  synthdef line `let analyze_in = sound_in_ar(0.0);` to
  `let analyze_in = in_ar(<bus>, 1);`. A future story should expose the
  source as an additional bus parameter once the 10-arg cap relaxes.
* **Focus does not modulate analyzer bandwidth.** The plan §3.a calls for
  Focus to set bucket-sum bandwidth (`bandwidth_bins ∈ [1, 16]`). Story
  C's bilinear lookup already consumes 256 `buf_rd` UGens per side, and
  a wider analyzer kernel would push the synthdef definition past the
  64 KB UDP `/d_load` limit. Focus instead exclusively addresses the
  Array Y-axis. A wider bucket-sum is feasible once vibelang switches
  to TCP `/d_recv`.
* **Captures are per-voice and reset on hot-reload.** `local_buf_ir`
  scopes the Array buffer to the synth instance. The plan §6.C3 calls
  for a script-allocated shared buffer; that ticket is open.
* **No default-spectrum bake in `spectraphon_side`.** Empty Arrays read
  zero. `spectraphon_dual` carries a `1/k` saw default and is the right
  starting point for "make some sound without input".
* **Sync mode is unimplemented** in `spectraphon_dual` (Story D5
  risk row).
* **Tuning Beacon** (the LED-color hint for simple ratios on the
  hardware) is not exposed.
* **`spectraphon_dual` ignores SAM, Arrays, Chaos, Noise, `right_mode`,
  and `merge_even`.** Both sides play SAO with the saw default. Layer
  two `spectraphon_side` voices instead if you need full per-side
  control — at the cost of losing the audio-rate FM bus.
* **Analyzer latency.** 2048-frame FFT at 50% overlap = ~24 ms hop at
  44.1 kHz, plus a 20 ms one-pole lag on `mag[k]` updates. SAM has
  noticeable latency relative to the hardware.

---

## 10. Roadmap

| Capability | Story | What it adds |
|---|---|---|
| Persistent script-allocated Array buffers | C3 (open) | Captures survive hot-reload; sharable across voices. |
| Default Array bake (Array 0 = saw, 1 = square, …) | C4 (open) | Audible SAO without prior capture. |
| Sync mode in `spectraphon_dual` | D4 / §7 risk | Hard-sync of B's bank to A's f0. |
| Per-side SAM + Arrays in the dual synthdef | future | Requires lifting the 10-arg cap. |
| Tuning Beacon meta-output | D5 | Visual hint for consonant ratios. |
| Configurable SAM input bus | future | Replace the hardcoded `sound_in_ar(0.0)`. |
| Wider analyzer bucket-sum (Focus → bandwidth) | future | Restores the plan §3.a bandwidth role of Focus. Blocked on `/d_recv` over TCP. |

---

## 11. References

* `crates/vibelang-std/stdlib/instruments/spectral/spectraphon_side.vibe`
  — single-side synthdef (Stories A + B + C + E).
* `crates/vibelang-std/stdlib/instruments/spectral/spectraphon_dual.vibe`
  — dual-side synthdef (Story D).
* `kb/spectraphon-synthdef-plan.md` — deep analysis of the hardware
  module, UGen mapping, behaviors, and the full implementation plan.
* `examples/spectraphon_drone.vibe` — Story A5 hello-world: low static
  spectrum + reverb tail.
* `examples/spectraphon_chord.vibe` — Story F1: dual-side chord patch
  with the FM bus and Follow-mode pitch tracking.
* `examples/spectraphon_array_capture.vibe` — Story F2: capture an
  Array from the hardware audio input, then play it back as SAO.
* `examples/spectraphon_multiout.vibe` — multi-output Story 12 demo:
  per-port routing of `sine`/`sub`/`odd`/`even` into different groups
  and main bus.
* `kb/voice-multioutput-howto.md` — full multi-output routing API
  (named ports, default routes, terminal verbs, reload semantics).
* Make Noise Spectraphon manual —
  https://www.makenoisemusic.com/wp-content/uploads/2024/03/spectraphon-manual.pdf
* Spectraphon Cheat Sheet —
  https://www.makenoisemusic.com/wp-content/uploads/2024/03/Spectraphon-Cheat-Sheet.pdf
