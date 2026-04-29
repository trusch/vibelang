# Morphagene — How-To

> User manual for the `morphagene` synthdef family in `vibelang-std`. Pairs
> with the deeper analysis in `kb/morphagene-synthdef-plan.md`.
>
> **Status snapshot:** Story A (`Reel + basic varispeed playback`) is merged.
> Stories B–F are documented in the plan but not yet implemented; the
> §"Roadmap" table at the end calls out which capabilities are coming in
> which story so the howto stays honest as the synthdef grows.

The `morphagene` synthdef recreates the playback-side of the Make Noise /
SoundHack Morphagene as a vibelang sampler instrument. Today (Story A) it is
a single-splice, varispeed-controllable stereo buffer player. Once Stories
B–E land it grows into a multi-splice, granular, Sound-On-Sound-capable
microsound engine. This document only covers what is wired up *right now*,
plus a "what's coming" appendix so users can plan their patches.

---

## 1. Mental model

Hardware Morphagene wraps three nested time-scales:

* **Reel** — the whole stereo buffer (up to 2.9 min on hardware).
* **Splice** — a `[start_frame, end_frame]` sub-region of the Reel.
* **Gene** — a microsound grain inside the active splice.

Story A exposes the **Reel** (as a `bufnum` parameter) and a **Splice**
(as `splice_start_frame`, `splice_end_frame`). The whole splice plays back
through a single `play_buf_ar` reader at a Vari-Speed-controlled rate.
Genes are **not** yet implemented — they arrive with the granular path in
Story C.

```
              ┌─────────────── Reel buffer (bufnum) ────────────────┐
              │                                                     │
              │   ┌── splice ──┐                                    │
              │   │  start..   │                                    │
              │   │  ..end     │                                    │
              │   └────────────┘                                    │
              │       ▲                                             │
              │       │ slide (0..1) offsets the play start         │
              │       │ inside the splice                           │
              │       │                                             │
              └───────┼─────────────────────────────────────────────┘
                      │
                      ▼
              vari_speed (-1..+1) → rate (cubic, finer near 0)
                      │
                      ▼
                 stereo audio out (L = R, see §5)
```

---

## 2. Parameters (Story A)

All parameters of `morphagene` as currently implemented in
`crates/vibelang-std/stdlib/instruments/sampler/morphagene.vibe`:

| Param | Default | Range | Meaning |
|---|---|---|---|
| `bufnum` | 0.0 | server bufnum | The active Reel. Loaded via `sample(...)` at the script level — the synthdef does not own the buffer. |
| `splice_start_frame` | 0.0 | 0 ≤ frame < BufFrames(bufnum) | First frame of the playback splice. |
| `splice_end_frame` | 0.0 | 0 = whole buffer; otherwise > splice_start | Last frame of the playback splice. **0 is a sentinel meaning "use BufFrames(bufnum)"** — leave it at 0 and the synthdef reads the entire Reel as one splice. |
| `vari_speed` | 0.0 | bipolar `[-1, +1]` | Continuous speed + direction. `0` halts. `+1` → rate 2.0 (forward, +12 ST). `-1` → rate −4.4898 (reverse, −26 ST). Asymmetric cubic curve gives finer resolution near zero (musically-usable wow/flutter under modulation). |
| `slide` | 0.0 | `[0, 1]` | Start offset inside the splice, expressed as a fraction of splice length. `slide=0` starts at `splice_start_frame`; `slide=0.5` starts halfway in. |
| `play_gate` | 1.0 | 0 / 1 | `1` = audible. `0` = output silenced (synth instance stays alive). 5 ms slew prevents click on edges. |
| `play_retrigger` | 0.0 | rising-edge | Rising edge restarts playback from the slide-defined start position. |
| `gate` | 1.0 | 0 / 1 | Synth-instance lifecycle (note on / note off). 5 ms attack, 20 ms release ASR; `cleanup_on_finish()` frees the synth on note-off. |
| `amp` | 0.5 | `[0, 1]` (typical) | Output amplitude. |

DC-blocker (`leak_dc`) is applied to both output channels — important when
modulating Vari-Speed across zero.

---

## 3. Loading a Reel

The active Reel is just a server-side buffer. Use the standard sampler
loader; the synthdef reads the buffer the runtime hands it via `bufnum`.

```vibe
import "stdlib/instruments/sampler/morphagene.vibe";

let reel = sample("reel", "samples/loop.wav").loop_mode(true);

let v = voice("morph")
    .synth("morphagene")
    .set_param("bufnum", reel.bufnum);   // wire Reel buffer explicitly
```

Pass `sample.bufnum` (or `allocate_buffer(...).bufnum` for a
script-allocated Reel — see `kb/morphagene-reel-howto.md`) directly
into the `bufnum` param. Don't use `voice.on(reel)` here: that path is
shaped for `sample_voice`/`warp_voice` and silently injects
envelope/rate/loop params that morphagene doesn't accept.

Stereo `.wav` files at 48 kHz are the native Morphagene format. Other
sample rates are fine — the synthdef applies `BufRateScale` so playback
speed stays calibrated when the Reel SR ≠ the server SR.

---

## 4. Vari-Speed control

The `vari_speed` parameter is the primary musical control. Mapping:

| `vari_speed` | rate | semitones from unity | notes |
|---|---|---|---|
| `+1.0` | `+2.000` | +12 ST | top of the forward range |
| `+0.7937` | `+1.000` |  0 ST | unity playback (cube root of 0.5) |
| `+0.5` | `+0.250` | −24 ST | slow-mo forward |
| `0.0` | `0.000` | halted | no advance — Play retrigger has no effect here |
| `−0.5` | `−0.561` | −10 ST reverse | reverse, mid-range |
| `−1.0` | `−4.490` | −26 ST reverse | bottom of the reverse range |

Two takeaways:

1. **12:00 = halt.** With `vari_speed = 0` the buffer pointer does not
   advance. A `play_retrigger` rising edge resets the read position but
   produces no audio until Vari-Speed leaves zero. This matches the
   hardware's "Play and Vari-Speed respect each other" behavior.
2. **Resolution is finest near zero.** Both halves of the curve are
   independent cubics, so small Vari-Speed values produce small
   wow/flutter rather than abrupt rate changes. Modulate with a
   slow LFO across `[-0.3, +0.5]` for tape-warble.

Modulator examples:

```vibe
// Slow wobble across zero (forward + a touch of reverse). The LFO is a
// plain voice on a kr-output synthdef; wire it into the morphagene
// voice's vari_speed param via .modulate_by (target-first BEND).
let vs_wobble = voice("vs_wobble")
    .synth("lfo_sine")              // kr port "out"
    .set_param("rate", 0.12)
    .set_param("lo", -0.3)
    .set_param("hi", 0.5);

// then on the morphagene voice:
//   morph.param("vari_speed").modulate_by(vs_wobble, "out");
```

---

## 5. Splice bounds

Two parameters carve the playback region out of the Reel:

* `splice_start_frame` — first frame to play. `0` = top of the Reel.
* `splice_end_frame` — last frame to play. `0` is a **sentinel**: the
  synthdef treats `0` as "play to `BufFrames(bufnum)`". Any value `≥ 1`
  is taken literally.

For a "play the whole Reel" patch leave both parameters at their defaults.
For a tape-cut you can pin specific frames:

```vibe
voice("v")
    .synth("morphagene")
    .set_param("bufnum", reel.bufnum)
    .set_param("splice_start_frame", 48000.0)        // 1.0 s in @ 48 kHz
    .set_param("splice_end_frame", 96000.0)          // 2.0 s in
    .set_param("slide", 0.0);                         // start at splice top
```

`slide` is a fraction of the splice (`[0, 1]`), so `slide=0.5` always
means "halfway through whatever splice you set", regardless of how long
the splice is.

> **Coming in Story B:** an internal splice-marker buffer (up to 300
> markers) plus **Organize** / **Shift** controls that select among the
> markers with the hardware's EOS-hold semantics — the splice change is
> *announced* immediately but the audio doesn't switch until the current
> splice/gene reaches its end. Until B lands, you set the splice bounds
> directly with `splice_start_frame` / `splice_end_frame` per voice.

---

## 6. Reel-buffer loading idiom

The synthdef does **not** allocate the Reel — the runtime does, exactly
once, and hands the bufnum down via the sample loader. This matches how
`vibelang-sfz` handles sample buffers and is what makes hot-reload safe:
recompiling the synthdef rewires the read nodes but leaves the buffer
intact.

Two patterns to know:

```vibe
// 1. One-shot Reel: pulls the file from disk on script start.
let reel = sample("reel", "samples/loop.wav").loop_mode(true);

// 2. Multiple voices reading the same Reel at different splices.
let r = sample("r", "samples/loop.wav").loop_mode(true);

voice("a").synth("morphagene")
    .set_param("bufnum", r.bufnum)
    .set_param("splice_start_frame", 0.0)
    .set_param("splice_end_frame", 24000.0);

voice("b").synth("morphagene")
    .set_param("bufnum", r.bufnum)
    .set_param("splice_start_frame", 48000.0)
    .set_param("splice_end_frame", 96000.0);
```

The same Reel buffer is shared across voices; only the splice bounds and
playback parameters differ. This is how you build micromontage patches
today, before the splice-marker buffer of Story B exists.

---

## 7. Stereo limitation (current)

The vibelang DSL surfaces only the first channel of `play_buf_ar`, so a
stereo Reel is read as **its left channel duplicated to L and R**. On the
hardware the "L (Mono)" jack normalises identically when only L is
patched, so this matches one supported routing — but it is not yet a true
dual-channel read.

True stereo reads land alongside the granular path in Story C
(`grain_buf_j_ar` exposes both channels natively).

---

## 8. Common patches (today)

### 8a. Tape-warble loop

```vibe
import "stdlib/instruments/sampler/morphagene.vibe";
import "stdlib/effects/reverbs/hall_reverb.vibe";
import "stdlib/modulators/lfo/lfo_sine.vibe";

let reel = sample("reel", "samples/loop.wav").loop_mode(true);

let vs = voice("vs")
    .synth("lfo_sine")                  // kr port "out"
    .set_param("rate", 0.12)
    .set_param("lo", -0.05)
    .set_param("hi", 0.15);

let v = voice("warble")
    .synth("morphagene")
    .set_param("bufnum", reel.bufnum)
    .set_param("slide", 0.5)
    .gain(db(-9));
v.param("vari_speed").modulate_by(vs, "out");
melody("hold").on(v).notes("C3").apply();
```

A slow LFO across `[-0.05, +0.15]` keeps Vari-Speed close to zero where
the cubic curve is densest, giving subtle pitch wow.

### 8b. Reverse drone

```vibe
voice("rev")
    .synth("morphagene")
    .set_param("bufnum", reel.bufnum)
    .set_param("vari_speed", -0.55)    // ~1× reverse-ish
    .set_param("slide", 0.0);
```

`vari_speed = -0.55` gives `rate ≈ -0.748` — a moderate reverse pace
without dropping below the audible range.

### 8c. Vari-Speed as pitch

A short splice + several voices at different `vari_speed` values produces
pitched playback. See `examples/morphagene_pitched_grain.vibe`.

> **Coming in Story D:** Time Stretch mode under CLK + Morph ≥ 11:00 makes
> Vari-Speed *purely* a pitch control, decoupled from grain advance. Until
> then, Vari-Speed-as-pitch and Vari-Speed-as-time are the same control.

---

## 9. Things that don't work yet

When you want them, watch for these stories to land:

| Capability | Story | What it adds |
|---|---|---|
| Multi-splice metadata buffer | B | Up to 300 splice markers per Reel, persisted alongside the buffer. |
| **Organize** knob (splice select with EOS hold) | B | Smooth-or-stepped selection across the splice list; switch defers to next end-of-splice/gene, matching hardware. |
| **Shift** gate (next splice) | B | Rising-edge increment of the splice index; deferred to EOS. |
| Splice button / gate (drop marker) | B | Live splice marker creation, deletion, clear-all. |
| **Sound-On-Sound** recording | B | `record_buf_ar`-based loop overdubbing with `sos` mix knob. |
| Auto-Level | B | One-shot input-gain calibration. |
| **Gene-Size** (per-grain duration in seconds) | C | Microsound grain length; sub-millisecond at full CW. |
| **Morph** (grain spawn cadence + per-grain pan / pitch-up) | C | Gap → 1:1 → 2:1 → 3:1+pan → 4:1+pitch-up staircase. |
| Granular `grain_buf_j_ar` playback path | C | Replaces `play_buf_ar` when Gene-Size leaves full-CCW; true stereo reads. |
| **EOSG** output | D | End-of-Gene/Splice gate, steady under Vari-Speed modulation. |
| **CLK** input — Gene Shift mode | D | Clock advances grain position by one Gene per pulse. |
| **CLK** input — Time Stretch / Compression | D | Clock drives time, Vari-Speed becomes pitch. |
| **CV Output** (envelope follower) | A5 (post-A in the plan) / B | 0…+8 V envelope of out_l + out_r. |
| Reel registry + `.wav` cue chunk persistence | E | Multiple Reels with hot-swap and Reaper-compatible splice markers. |
| "Morphagene-???*" experimental Morph extension | F | Documented best-effort for the manual's `???*` region. |

The plan-of-record is `kb/morphagene-synthdef-plan.md` §6 — that is the
canonical breakdown of which Story owns which behavior.

---

## 10. References

* `crates/vibelang-std/stdlib/instruments/sampler/morphagene.vibe` —
  current synthdef source, with inline comments on the cubic Vari-Speed
  curve and the splice-end sentinel.
* `kb/morphagene-synthdef-plan.md` — deep analysis of the hardware module,
  UGen mapping, behaviors, and the full implementation plan.
* `examples/morphagene_loop.vibe` — Story A hello-world. Loads a stereo
  loop, modulates Vari-Speed and Slide, sends through a hall reverb.
* `examples/morphagene_micromontage.vibe` — multi-splice tape-cut
  pattern using Story A's per-voice splice bounds (a stand-in for the
  Organize / EOS-hold flow that arrives with Story B).
* `examples/morphagene_sos_decay.vibe` — Sound-On-Sound feedback decay
  pattern. Today this is simulated via a `dub_delay` feedback path on the
  morphagene output; true SOS recording arrives with Story B.
* `examples/morphagene_pitched_grain.vibe` — Vari-Speed-as-pitch using a
  short splice and multiple voices at calibrated `vari_speed` values.
  Genuine grain-rate decoupling lands in Stories C + D.
* Make Noise Morphagene manual — https://www.makenoisemusic.com/wp-content/uploads/2024/03/morphagene-manual.pdf
