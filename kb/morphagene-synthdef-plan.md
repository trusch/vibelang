# Morphagene Synthdef Recreation — Deep Analysis & Plan

> Research-only plan for recreating the Make Noise / Soundhack Morphagene as a vibelang synthdef family.
> No code in this document — pure analysis. Implementation tickets are listed in §6.

## 1. Module overview

The **Make Noise / Soundhack Morphagene** (released 2017, DSP by Tom Erbe of Soundhack, hardware by Tony Rolando) is a **20 HP Eurorack tape-and-microsound music module** built around a stereo digital audio buffer ("Reel") that can be recorded into, sliced, granulated, varispeed-played, time-stretched, and reorganized — all under voltage control. The signal-processing posture is purely **time-domain granular + varispeed** (no FFT/spectral path), descended from musique concrète tape splicing and from microsound granular techniques codified by Curtis Roads. Reels are stored as **48 kHz / 32-bit float `.wav` files on a FAT32 microSD card**, with splice markers persisted as standard `.wav` cue chunks. Each Reel is up to **~2.9 minutes** long and may contain up to **300 Splices** (i.e. up to 299 splice markers); the number of Reels per card is limited only by SD capacity. All audio I/O is AC-coupled stereo (line- or modular-level, leveled by a one-shot Auto-Level routine); CV inputs are DC-coupled. Power: 165 mA @ +12 V, 20 mA @ −12 V.

```
                                 ┌──────────────────────────────────────────┐
 In-L,R ──▶┌─────────┐           │              REEL                        │
           │  INPUT  │──┐        │  stereo .wav buffer in RAM (≤ 2.9 min)   │
           │  GAIN   │  │        │  loaded from microSD on Reel-mount       │
           └─────────┘  │        │  recorded forward-only, constant rate    │
              ▲ Auto-   │        │  ┌───────┬────────┬──────┬────────────┐  │
              │ level   │ SOS    │  │Splice0│Splice1 │...   │Splice N-1  │  │
              │         └─▶─────▶│  └───────┴────────┴──────┴────────────┘  │
              │           rec    │              ▲                           │
              │                  │              │ Splice markers (cues)     │
              │      ┌───────────┴──────────────┴────────────┐              │
              │      │  Splice select: Organize (kr panel)    │              │
              │      │   + Shift (increment) + Splice CV-in   │              │
              │      └───────────┬───────────────────────────┘              │
              │                  │                                          │
              │                  ▼                                          │
              │       Active splice region [start_f, end_f]                 │
              │                  │                                          │
              │       ┌──────────┴───────────┐                              │
              │       │ Gene Window =        │                              │
              │       │   gene_size seconds  │ ◀── Slide CV (start offset)  │
              │       │   inside splice      │                              │
              │       └──────────┬───────────┘                              │
              │                  │                                          │
              │   ┌──────────────┴──────────────────────┐                   │
              │   │ Granular Engine                     │ ◀── Vari-Speed    │
              │   │  spawn_rate = f(Morph, gene_size)   │     (rate, dir)   │
              │   │  per-grain dur = gene_size (TIME,   │                   │
              │   │    not samples — independent of     │ ◀── CLK   ┐       │
              │   │    Vari-Speed)                      │           │ Gene  │
              │   │  Morph 12:00+ → overlap + pan       │           │ Shift │
              │   │  Morph 2:30+  → +random pitch-up    │           │  /    │
              │   └──────────────┬──────────────────────┘           │ Time  │
              │                  │                                  │ Strtch│
              │                  ▼                                  │       │
              │           Out-L, Out-R  ────▶ envelope-follower ────┴─▶ CV  │
              │                                                              OUT
              │                                                              EOSG
              │                                            (per Gene/Splice end)
              │
              └──────── Play-gate (loop / stop / retrigger), Rec-gate, Shift-gate
```

The signal that emerges at Out-L/Out-R is a granulated, time-warped resampling of the recorded Reel (plus optionally the live input, mixed by Sound-On-Sound). The Morphagene is **not** a synthesizer — it has no oscillators of its own; every output sample comes from buffer reads through grain windows.

---

## 2. Frontpanel inventory

The 20 HP layout is single-channel (one Reel processed at a time). Controls cluster as: stereo I/O on top, Microsound Tools on the upper face (Gene-Size, Vari-Speed, Slide, Morph), Tape Tools on the lower face (REC, Splice, Shift, Organize, SOS), CLK + CV Out + microSD on the periphery. Numbering matches the manual's Panel Controls pages (10–12).

### 2a. Audio I/O

| Jack | Direction | Type | Function | Notes |
|---|---|---|---|---|
| **Audio In L (Mono)** | input | audio | Stereo channel L; doubles as mono input when only L is patched | Line-to-modular range, AC-coupled, no analog input gain — leveled digitally via Auto-Level (REC + Shift) |
| **Audio In R** | input | audio | Stereo channel R | AC-coupled |
| **Audio Out L (Mono)** | output | audio | Stereo channel L; mono sum when only L is patched | Typically 10 Vpp, AC-coupled |
| **Audio Out R** | output | audio | Stereo channel R | 10 Vpp, AC-coupled |

### 2b. Microsound Tools (upper face)

| Control | Type | Range | Function | CV input | CV scaling |
|---|---|---|---|---|---|
| **Gene-Size** | knob (unipolar) | full splice (CCW) → "extremely short, potentially inaudible" (CW) | Non-destructive shrink of the splice's playback window into Genes (grains). Scale is **time**, not samples — i.e. Gene-Size at a fixed setting yields a constant gene duration regardless of Vari-Speed. | Gene-Size CV IN | 0…+8 V uni-polar (panel labels it "bi-polar" but the documented range is 0 … +8 V); attenuverter is bipolar |
| **Slide** | knob (unipolar) | 0 (CCW) … 1 (CW) | Offsets the start point of the first Gene within the active splice; with Gene-Size full CCW it offsets the Play Reset / Start Point and EOSG location. Continuous. Scrubs when modulated with smooth CV; produces hard timbral steps when modulated with stepped CV. | Slide CV IN | 0…+8 V uni-polar; bipolar attenuverter |
| **Vari-Speed** | knob (bipolar) | 12:00 = halted; CW = forward, max ≈ +12 semitones; CCW = reverse, max ≈ −26 semitones; resolution finer near center | Continuous speed + direction. Affects only **playback** (record speed is constant). | Vari-Speed CV IN | ±4 V bipolar; bipolar attenuverter |
| **Morph** | knob (unipolar) | 0 (CCW): gap-of-silence between Genes (pointillist); ~8:30: 1:1 seamless loop; 12:00: 2:1 overlap; ~1:00: 3:1 overlap + per-grain panning; ~2:30: 4:1 overlap + random pitch-up; CW: "???*" (manual's own label) | Sets spacing between successive Gene instances — gap → seamless → 2× overlap → 3× + pan → 4× + random pitch-up. Independent of Gene-Size. | Morph CV IN | 0…+5 V uni-polar (unity, no attenuverter on panel) |

### 2c. Tape Tools (lower face)

| Control | Type | Function | CV input | Notes |
|---|---|---|---|---|
| **Sound-On-Sound (S.O.S.) Combo Pot** | knob (unipolar) | Mix of Live signal + previously-recorded Loop. Full CCW = live only; full CW = loop only at unity feedback (turning down attenuates loop each cycle → delay-style decay). When SOS CV is patched, this pot becomes the attenuverter for SOS CV. | SOS CV IN | 0…+8 V uni-polar, linear, normalized to +8 V. |
| **REC Button (illuminated)** | momentary | Toggles record on / off; lit while recording, strobes while waiting for synced clock pulse. Many button combos. | — | Single-press action fires on **release** so combos can cancel it (except Record Stop). |
| **Splice Button (illuminated)** | momentary | Drops a splice marker at current playback location. Lights at end of currently-playing splice/gene. | — | — |
| **Shift Button (illuminated)** | momentary | Increments selected splice. Doubles as microSD mount, Auto-Level, Reel mode, deletes — see button-combos table. Lights to indicate microSD is mounted; flashes when SD is busy (do not remove during flash). | — | — |
| **Organize** | knob (unipolar) | Selects which splice is "next" (with EOS hold semantics — see §3e). In Reel Mode it selects the Reel. **Always overrides** Shift button/gate. | Organize CV IN | 0…+5 V uni-polar; designed for sequencer-style 5 Vpp range. Unpatch when entering Reel Mode. |

### 2d. Synchronization jacks

| Jack | Direction | Type | Function | Notes |
|---|---|---|---|---|
| **CLK** | input | clock/gate | Mode-dependent: during recording, syncs REC start/stop to clock edges. During playback with Gene-Size < full splice: Morph below 11:00 → **Gene Shift** (clock advances chronologically through Genes); Morph at/above 11:00 → **Time Stretch / Time Compression** (clock drives time, Vari-Speed becomes pitch). | ≥ 2.5 V; min usable rate ≈ 1 / 3.5 s for Time-Stretch; Time-Compression caps at ≈ 18 Hz (faster clocks no longer compress further). |
| **Play** | input | gate | Gate HIGH at end-of-Gene/Splice → continue playing; LOW → stop at EOS. Rising edge = retrigger from Slide-defined start. **Normalled HIGH** (so unpatched = always loop). | Play and Vari-Speed respect each other: with Vari-Speed at 12:00 (halted), a Play gate cannot retrigger (rate ≈ 0). |
| **REC** | input | gate | Toggles record on/off at clock or gate. | ≥ 2.5 V |
| **Shift** | input | gate | Increments splice on rising edge. | ≥ 2.5 V |
| **Splice** | input | gate | Drops a splice marker at the current playback location on rising edge. | ≥ 2.5 V |
| **EOSG** (End of Gene/Splice Gate) | output | gate | Gate fires at the end of every Gene; with Gene-Size full CCW it fires at end of every Splice. **Steady relative to Gene-Size, not Vari-Speed** — usable as a stable clock under speed modulation. | 0…10 Vpp |
| **CV Output** | output | CV | Envelope follower of the audio outs (Out-L + Out-R). | 0…+8 V DC; useful as input to Echophon, Erbe-Verb, etc., or back into one of the Morphagene's own CV inputs. |

### 2e. Activity windows / LEDs

| LED group | Function | Color code |
|---|---|---|
| **Reel Activity Window** | Selected Reel indicator. Cycle when modulating Reel select. Flashes at CLK in. Strobes at high Gene-Size and Morph values. | Reels paint as: blue → green → light green → yellow → orange → red → pink → white, then repeat. |
| **Splice Activity Window** | Currently-playing splice indicator. Flashes during Erase Splice / Erase All / Erase Audio. | Same color sequence. |
| **Vari-Speed Activity Windows** (left + right pair) | Speed/direction. Right LED = forward; Left LED = reverse. Off at 12:00. | Green = 1:1; Baby Blue = 1 octave up; Peach = 1 octave down; Red = stopped (12:00). |
| **Morph Activity Window** (the *opposite* Vari-Speed LED!) | Morph state, sharing the LED with Vari-Speed direction. | Red = gap-or-overlap (non-integer); Amber = integer overlap (1:1, 2:1, 3:1, 4:1); Blue = Time-Stretch active (CLK patched + Morph ≥ 11:00); Red w/ CLK patched + Morph < 11:00 = Gene Shift. |
| **CV Output Activity Window** | Visual indication of envelope-follower output. | — |
| **REC Button LED** | On = recording. Strobes = waiting for clock. | — |
| **Splice Button LED** | On = end of currently-playing splice/gene. | — |
| **Shift Button LED** | On = microSD mounted. Flashes = SD busy. | — |

### 2f. microSD slot

* FAT32 only. Manual unmount via Shift button when removing card live; mounting is automatic on insert (or on Shift press if not auto-mounted). Reels stored as `.wav` files; splice markers as standard `.wav` cue chunks readable by Reaper, Audacity, etc.

### 2g. Button combinations (real-time)

The full button-combos table (manual page 9). Single-press of REC, Splice, or Shift fires on **release** so combos work without accidental side effects — except Record-Stop, which fires immediately.

| Combo | Action |
|---|---|
| Hold REC + Press Shift | Auto-Level (sets digital input gain so output ≈ 10 Vpp). Holding longer = "Rolling Listen" preview. |
| Press Shift | Mount inserted microSD when not auto-mounted. Also: increment splice. |
| Hold Splice + Press REC | Enter Reel Mode (Organize selects reel; last reel pulses pink/white = "create new on exit"). |
| Hold Splice + Press REC (in Reel Mode) | Exit Reel Mode. |
| Hold Shift + Press REC (in Reel Mode) | Delete current Reel including all Splices and audio. |
| Press REC | Record into current splice (toggle on/off). |
| Hold REC + Press Splice | Record into new splice at end of reel. |
| Press Splice | Drop splice marker at current playback location. |
| Hold Shift + Press Splice | Delete next splice marker (merges current splice with next). |
| Hold Shift + Hold Splice for 3 s | Delete all splice markers (entire reel becomes one splice). |
| Hold Shift + Press REC | Delete splice audio of current splice. |
| Hold Shift + Hold REC for 3 s | Clear reel (deletes all markers and all audio). |

---

## 3. DSP architecture (what's actually inside)

Morphagene is a **time-domain granular sampler over a tape-style buffer**, not a spectral processor. The combination of "Reel + 300 Splices per Reel + Genes inside the active Splice" forms a three-level time hierarchy directly cited in the manual (§14, after Curtis Roads' nine timescales):

* **Reel** — the *meso* timescale (minutes-to-seconds of phrase/form),
* **Splice** — *meso/sound-object* (seconds),
* **Gene** — *micro* (milliseconds-to-microseconds, "particle" granular).

### 3a. Reel buffer

* **Format:** stereo, 48 kHz, 32-bit float (manual: "constant 48 kHz with 32-bit dynamic range"). Length up to 2.9 min ⇒ 8 352 000 frames per channel ⇒ 16 704 000 stereo samples ⇒ ~67 MB per Reel at 32-bit. Stored on microSD as a standard `.wav`.
* **Recording is constant-speed, forward-only**, regardless of Vari-Speed or Morph. The audible playback modulations *are* baked into Sound-On-Sound and Record-Into-New-Splice recordings (because what gets recorded is the live audio at the input + the modulated playback through the SOS mixer), but the sample-clock of the recorder itself never varies.
* **Recording offset**: tracked in frames; recording auto-stops at the 2.9-minute boundary.

### 3b. Splices

* **A splice is a [start_frame, end_frame] sub-region** of the Reel. Up to 299 markers ⇒ 300 splices. Splices are **always evenly spaced through the Organize knob's range**, regardless of their relative audio length: 4 splices ⇒ each occupies a 90° arc; 100 splices ⇒ each occupies 3.6° of knob travel.
* **Marker creation:** Press Splice (or Splice gate rising edge) drops a marker at the current playback location.
* **Marker deletion:** Hold Shift + Press Splice deletes the *next* marker (i.e. merges current splice with the next).
* **Splice selection (Organize):** turning the knob, modulating Organize CV, or pressing/gating Shift causes the Splice Activity Window color to *change immediately*, but **playback does not switch until the currently-playing splice/gene reaches its end** (i.e. EOS-hold semantics; the EOSG output fires the moment the actual switch happens). This is critical for rhythmic micromontage.
* **Organize priority:** Organize panel control + CV always overrides Shift button/gate.
* **microSD persistence:** markers are written as `.wav` cue chunks; they survive across power cycles and can be edited externally in Reaper / Audacity (the manual specifically calls out Reaper at 48 kHz / stereo / 32-bit float).

### 3c. Genes (granular layer)

* A **Gene** is a windowed grain taken from inside the active splice region.
* **Gene-Size** controls *temporal length* — explicitly **time, not samples**. The manual is unambiguous: "the temporal length of the Gene allows EOSG to be used as a steady clock even if Vari-Speed is being modulated." So `gene_dur_seconds = f(gene_size_knob)` with `f(0) = splice_length_seconds` and `f(1) → ε` (sub-millisecond / "potentially inaudible").
* **Slide** offsets the *grain start position* (or, when Gene-Size is full CCW, the splice's Play Reset / Start Point), expressed as a fraction of the splice. Continuous; no quantization.
* **Vari-Speed** applies a per-grain playback rate (= ratio of buffer-read advance vs. the constant audio output sample-clock). Independent of the grain spawn cadence.
* **Granulation type:** at very small Gene-Size the Genes become clicks; the timbral interest comes from many clicks at varied source positions (Slide), pitches (Vari-Speed), and overlap (Morph). This is classical pulsar / dust-grain granular synthesis in the Roads sense.

### 3d. Morph (grain-spawn cadence)

The Morph control is the most idiosyncratic part of the engine. It governs *how* successive Gene instances overlap, layer, and randomize. From the manual (pages 19–21, 24) the staircase is:

| Morph knob | Behavior | Activity-window |
|---|---|---|
| Full CCW (≈7:00) | New Gene starts after current Gene ends + small **gap of silence** (pointillist) | Red |
| ≈ 8:30 | Gap closes; next Gene starts the moment current ends → **1:1 seamless loop** | Amber |
| 8:30 → 12:00 | Next Gene starts before current ends; up to **2× overlap** (current + next) | Red (non-integer) → Amber at 2:1 |
| ≈ 12:00 | A **third Gene instance** is added (overlap > 0.5 of Gene length) | Amber at 3:1 integer points |
| ≈ 1:00 | All three instances are equally spaced; **per-grain panning is introduced** | Amber/Red |
| ≈ 2:30 | A **fourth Gene instance** is added; **random upward pitch-up** of grains kicks in | Amber/Red |
| Beyond 2:30 (CW) | "???" — manual literally annotates `???*` and footnotes "Not for laboratory use." | Red |

Critical constraints:
* The cadence is **independent of Gene-Size** ("the Morph control works the same regardless of the size of the Gene, all the way up to a full Splice of any length including the full Reel, up to 2.9 minutes in length").
* Smoothing: "The Morphagene uses **Dynamic Enveloping** to achieve smoothing of the audible glitches that result from performing these particle physics experiments with audio signals." (Manual, page 24.) The manual does not specify the envelope shape or its time-constants; reasonable bet is per-grain Hann/Tukey + a global LP smoothing on the grain-rate parameter.

### 3e. Synchronization (CLK input)

The CLK input is multi-purpose and mode-switches based on context:

* **During recording:** CLK syncs REC button start/stop to clock edges. REC LED strobes while waiting; lights solid at clock-aligned start.
* **During playback, no CLK patched:** Morph is the autonomous spacer described above.
* **During playback, CLK patched, Gene-Size < full splice, Morph ≥ 11:00 (Blue):** **Time Stretch / Time Compression**. The internal time advances one Gene per clock pulse. Vari-Speed is repurposed as **pitch** (changes playback transposition without affecting time). Min clock rate for stretch: 1 / 3.5 s. Max compression rate: ~18 Hz (above which no further compression occurs).
* **During playback, CLK patched, Gene-Size < full splice, Morph < 11:00 (Red):** **Gene Shift**. Each clock pulse advances the playhead by one Gene's worth of frames in the Vari-Speed-determined direction. Useful for chronological synchronous granulation.

### 3f. Sound-On-Sound (SOS)

Classic tape SOS: the live input is summed with the previously-recorded loop, and the sum is rerecorded into the buffer.

* Algorithmically: on each frame inside the active splice, `new[i] = recLevel · input[i] + preLevel(SOS) · old[i]`. With `preLevel = 1` (full CW) the loop persists at unity feedback; with `preLevel < 1` each pass attenuates the loop, producing tape-style decay.
* SOS only operates while REC is active.
* Recording is forward-only at unit speed, but **playback during SOS recording can run in reverse** (Vari-Speed CCW). That means you can rerecord the reversed loop layered with new live input, but the new layer's audio itself goes onto the buffer forward-time. This is the source of Morphagene's "two sounds in opposite directions" patch.
* When SOS CV is patched, the SOS panel pot becomes a bipolar-ish attenuverter for that CV (the manual's panel-table label).

### 3g. Vari-Speed

* Bipolar continuous: `vari_speed_knob_signed ∈ [-1, +1]` plus CV `[-4, +4] V`.
* Resolution increases toward center → suggests a cubic / odd-power interpolation around 0 rather than linear, so light "wow & flutter" modulation is musically usable.
* Maximum range: about +12 semitones up (rate factor ≈ 2.0) and −26 semitones down (rate factor ≈ 0.222 forward, or ≈ −0.222 with reverse playback). At 12:00, rate = 0 and playback halts.
* Vari-Speed only affects **playback**, never recording.
* Important interactions: with Vari-Speed = 0, a Play-gate cannot retrigger (effective rate is zero). With Vari-Speed > +1 (forward, faster than 1×) plus Time-Lag-Accumulation (TLA) recording, sounds get pitch-shifted into ultrasonic; the Morphagene loses signal eventually.

### 3h. Auto-Level

Hold REC + Press Shift normalizes input gain so that buffer playback peaks ≈ 10 Vpp. The manual: "analyzes the sound and adjusts its gain to the correct amplitude for use in the modular system. Holding this button combination for a few seconds produces a good snapshot of the signal dynamics and ideal level settings." Algorithm not disclosed; reasonable model is peak-tracking + RMS over a few-second window → input-gain set so peak hits a ceiling (e.g. -1 dBFS).

### 3i. CV Output

Envelope follower on the audio outs (sum of L+R). Range 0…+8 V DC. "Higher Morph values often stabilize the CV Out to some degree" — i.e. with more grain overlap the envelope is smoother.

### 3j. EOSG (End of Gene/Splice Gate)

* Fires a gate (0…10 Vpp) at the end of every Gene playback window.
* When Gene-Size is full CCW (entire splice plays), EOSG fires at end-of-splice.
* **Stays steady under Vari-Speed modulation** because Gene length is in time-domain, not sample-domain.
* As Gene-Size grows (CW) or Morph grows (more grains), EOSG fires more frequently.

---

## 4. UGen mapping per behavior

Vibelang's UGen toolbox maps cleanly onto Morphagene's architecture: a single audio buffer (the Reel) plus a granular reader. The cleanest implementation path uses **JoshUGens `grain_buf_j_ar`** (multi-channel buffer granulator with per-grain pan, configurable maxGrains, optional envelope buffer) driven by an **`Impulse`-style spawn trigger**, with `record_buf_ar` for SOS-style writing back into the same buffer.

### 4a. Per-control implementation table

* **Morphagene control:** Audio In L / R (recording path)
  * **Function:** Live signal into the Reel during REC.
  * **Vibelang impl:** `record_buf_ar(input_array=[in_l, in_r], bufnum=reel_buf, offset=rec_pos_frame, recLevel=rec_active * input_gain, preLevel=sos, run=rec_active, loop=0, trigger=rec_start)`. `rec_pos_frame` is held in a `phasor`-backed kr accumulator that advances by 1 frame per audio frame while `rec_active`. `loop=0` because the recorder hard-stops at end-of-Reel.
  * **Param mapping:** `input_gain` from Auto-Level routine (see 4b); `sos` from the SOS combo pot + CV (0…1).

* **Morphagene control:** Vari-Speed (knob + CV)
  * **Function:** Per-grain playback rate (sample-advance ratio); 12:00 = 0; CW up to ≈ 2.0; CCW down to ≈ −0.222 (reverse).
  * **Vibelang impl:** Compose `vari_speed = knob_signed * 0.7 + cv * 0.3 * voct_atten`; map through `pow(2, x * 12.0/12.0)` for CW (forward) and `-pow(2, |x| * 26.0/12.0 * -1.0)` for CCW (reverse), with a dead-zone around ±1% of zero so 12:00 is exactly halt. Feed as `rate` argument to `grain_buf_j_ar`.
  * **Param mapping:** Knob ∈ [-1, +1] (panel), CV ∈ [-4, +4] V at the jack → divide by 4 → [-1, +1]; sum and clip; cubic-shape near zero for the documented finer-near-center resolution.

* **Morphagene control:** Gene-Size (knob + CV)
  * **Function:** Per-grain duration in *seconds*, independent of Vari-Speed.
  * **Vibelang impl:** `gene_dur_sec = mix( splice_length_sec, gene_min_sec, gene_size_normalized^k )` with `gene_min_sec ≈ 50 µs` (sub-audible click region) and `k ≈ 3.0` (exponential CW shrink). Feed as `dur` to `grain_buf_j_ar` — note this UGen polls `dur` at grain creation, so changes apply per-grain.
  * **Param mapping:** Knob ∈ [0, 1], CV ∈ [0, 8] V → divide by 8 → [0, 1]; sum + clamp.

* **Morphagene control:** Slide (knob + CV)
  * **Function:** Offset of grain-start within the active splice (or Play-Reset point when Gene-Size CCW); fraction in [0, 1] of splice length.
  * **Vibelang impl:** `pos_frac_in_splice = clamp(slide_knob + slide_cv/8 * slide_atten, 0, 1)` then `grain_pos_frac_in_reel = (splice_start + pos_frac_in_splice * splice_length) / reel_length`. Pass as `pos` to `grain_buf_j_ar`.
  * **Param mapping:** Knob 0…1; CV 0…+8 V → 0…1; bipolar attenuverter on CV.

* **Morphagene control:** Morph (knob + CV)
  * **Function:** Grain spawn cadence + behaviors (gap → seamless → 2× → 3×+pan → 4×+pitch-up).
  * **Vibelang impl:** Most of the Morphagene's character lives in the spawn trigger:
    * `morph_norm = clamp(morph_knob + morph_cv / 5, 0, 1)` (CV is 0…+5 V, unity).
    * Build `spawn_rate_hz = spawn_rate_curve(morph_norm) / gene_dur_sec`, where `spawn_rate_curve` is piecewise:
      ```
      0.00..0.25 :   1 / (1 + gap_frac)   (gap_frac: 0.5 → 0)   ⇒ rate_factor 0.67 → 1.0
      0.25..0.50 :   1.0 → 2.0            (overlap)
      0.50..0.55 :   2.0 → 3.0            (third instance enters)
      0.55..0.70 :   3.0                  (panning kicks in)
      0.70..0.85 :   3.0 → 4.0            (fourth instance enters)
      0.85..1.00 :   4.0 + pitch-up randomization
      ```
    * Feed `Impulse(freq=spawn_rate_hz)` (or `Dust` for extra randomness when Morph approaches 1.0) as `trigger` into `grain_buf_j_ar`.
    * Per-grain pan: `pan = white_noise_kr() * pan_amount_from_morph` sampled-and-held per grain via `Latch` triggered by the spawn impulse. `pan_amount_from_morph = smoothstep(0.55, 0.85, morph_norm)`.
    * Per-grain pitch-up: derive a per-grain rate multiplier `pitch_up_factor = 1 + uniform_rand(0, pitch_amount)` where `pitch_amount = smoothstep(0.85, 1.0, morph_norm) * 0.8`, latched per spawn-trigger and multiplied into `rate`.
    * Smoothing: lag the spawn-rate parameter by ~5 ms (`Lag` UGen) — vibelang's stand-in for the manual's "Dynamic Enveloping".
  * **Param mapping:** Morph knob ∈ [0, 1]; CV unity (no atten) ∈ [0, +5] V → /5 → [0, 1].

* **Morphagene control:** Organize (knob + CV) — splice select
  * **Function:** Which splice plays next; switch deferred until end-of-current-splice/gene.
  * **Vibelang impl:**
    * `pending_splice_idx = floor( (organize_knob + organize_cv/5) * num_splices )`, clipped.
    * `committed_splice_idx = Latch( pending_splice_idx, trigger = end_of_splice_kr )` — latches the new selection only at EOS, implementing the EOS-hold semantic.
    * Read splice metadata buffer at `committed_splice_idx` to get `splice_start_frame`, `splice_end_frame`. Feed through to the granular reader.
  * **Param mapping:** Knob 0…1, CV 0…+5 V → 0…1.

* **Morphagene control:** Shift (button + gate)
  * **Function:** Increment splice on rising edge; deferred to EOS.
  * **Vibelang impl:** `shift_increment = Stepper( trig=shift_gate_rising_edge, min=0, max=num_splices-1, step=1 )`. Combine with Organize: when Organize is being modulated (`Changed(organize_input, threshold=0.001) > 0`), Organize wins; otherwise Shift's stepper drives the pending splice. Implementation: `pending_splice_idx = select(organize_active, organize_idx, shift_idx)` where `organize_active = Sweep(Changed(organize_in), 0.5)` (recently-changed → active for 500 ms).
  * **Param mapping:** Rising-edge detector on the gate; Schmidt for clean edges.

* **Morphagene control:** Play (gate)
  * **Function:** HIGH at EOS = continue, LOW at EOS = stop, rising edge = retrigger from start.
  * **Vibelang impl:**
    * `play_gate = play_in or 1` (normalled HIGH).
    * Rising-edge detector → `retrigger = Trig1(diff(play_gate) > 0, dur=1/sr)`.
    * On retrigger: reset grain-spawner phase to splice_start + slide.
    * On EOS with play_gate LOW: gate the spawn-trigger off until next rising edge.
  * **Param mapping:** Gate ≥ 2.5 V → boolean.

* **Morphagene control:** REC (button + gate)
  * **Function:** Toggle record state; with CLK patched, sync to clock.
  * **Vibelang impl:** `rec_active = ToggleFF(trig = rec_press_or_gate_rising_edge)`. With CLK patched, ANDed against `Latch(1, clock_in)` so the toggle takes effect at the next clock edge.
  * **Param mapping:** Schmidt-cleaned gate input.

* **Morphagene control:** Splice (button + gate)
  * **Function:** Drop splice marker at current playback location.
  * **Vibelang impl:** On rising edge, write the current `playback_frame` value into the splice-metadata buffer at `num_splices++`. This is a control-rate write to a `LocalBuf` of splice positions, executed once per rising edge — implement as a SuperCollider-style `BufWr` driven by a one-shot trigger and `pulse_count_kr` for the index.
  * **Param mapping:** Schmidt-cleaned gate input.

* **Morphagene control:** SOS (knob + CV)
  * **Function:** Mix of live + recorded loop while recording; doubles as the loop-feedback level.
  * **Vibelang impl:** `sos_norm = clamp(sos_knob + sos_cv/8 * sos_atten, 0, 1)`. `record_buf_ar(..., recLevel = (1 - sos_norm) * input_gain * rec_active, preLevel = sos_norm * rec_active)`. Note the manual's behavior: full CCW = live only, full CW = unity loop feedback. The recLevel-counterbalance is one valid model; alternatively recLevel=1 always and preLevel=sos_norm — both produce a usable SOS, but the (1−sos)/sos pairing matches the panel's "balance" labeling more literally.
  * **Param mapping:** Knob 0…1; CV 0…+8 V → /8 → 0…1; bipolar attenuverter (panel pot acts as attenuverter when CV is patched).

* **Morphagene control:** CLK input
  * **Function:** Multi-mode synchronization (record sync; Gene Shift; Time Stretch).
  * **Vibelang impl:**
    * `clk_rising = Schmidt(clk_in, 1.0, 2.5) > 0` rising-edge detector.
    * Mode select: `clk_mode = case(rec_arming → record_sync; gene_size_full_ccw → none; morph_norm < 0.46 → gene_shift; else → time_stretch)`. (0.46 ≈ 11:00 on a 7:00–5:00 panel arc — calibrate empirically.)
    * **Gene Shift:** `gene_advance_frames = clk_rising_count * gene_dur_frames * sign(vari_speed)`. Add this to the grain-pos accumulator instead of the autonomous Morph spawner.
    * **Time Stretch:** the autonomous spawner is replaced with an Impulse triggered by clk_rising; vari_speed is rerouted to grain `rate` (pitch) but the cross-grain advance becomes `splice_length / num_clocks_to_traverse`. Vari-Speed = 1 in this mode means pitch unchanged.
  * **Param mapping:** Schmidt 1.0 → 2.5 V hysteresis.

* **Morphagene control:** EOSG output
  * **Function:** Gate at end of every Gene (or Splice when Gene-Size CCW).
  * **Vibelang impl:** Easiest: trigger at every grain spawn delayed by `gene_dur_sec` — `eosg_pulse = TDelay(spawn_trigger, dur=gene_dur_sec)`. With Gene-Size CCW the spawn-trigger fires once per splice cycle, so EOSG naturally falls back to end-of-splice. Gate length: `Trig1(eosg_pulse, dur=0.005)` for a 5 ms gate at 10 V.
  * **Param mapping:** N/A — output node.

* **Morphagene control:** CV Output (envelope follower)
  * **Function:** Envelope follower of audio output sum; 0…+8 V.
  * **Vibelang impl:** `cv_out = amplitude_kr(out_l + out_r, attack=0.01, release=0.05) * 8.0` (or use `Amplitude` from the dynamics manifest). Range-clip to [0, 8].
  * **Param mapping:** N/A — output node.

* **Morphagene control:** Auto-Level (REC + Shift combo)
  * **Function:** One-shot gain calibration: holds for ~few seconds, peak-tracks, sets input gain so output ≈ 10 Vpp.
  * **Vibelang impl:** During the combo-hold, run `peak_kr(in)` over a 3 s integration window (reset trigger at combo-start). On combo-release, set `input_gain = target_pp / measured_peak` (e.g. `target_pp = 10 V / typical_buffer_scale`). Persist `input_gain` as a synthdef parameter (latched).
  * **Param mapping:** Synthdef param, write-once-on-combo.

* **Morphagene control:** Reel selection (Reel Mode)
  * **Function:** Switch which reel buffer is active.
  * **Vibelang impl:** Pre-load a registry of `reel_bufnum[N]` (one server-side buffer per reel). In Reel Mode, `active_reel_idx = floor(organize_knob * num_reels)`; on Reel-Mode exit, swap the synthdef's `bufnum` parameter to the new reel. Reel-Mode itself is a top-level state outside the per-block synthdef; expose `active_reel_idx` as a synth param and have the runtime swap the buffer reference.
  * **Param mapping:** Synthdef param `bufnum`.

### 4b. Toolkit choice rationale

| Morphagene stage | Best vibelang toolkit | Reason |
|---|---|---|
| Reel buffer (RAM) | persistent server-side buffer (one-per-reel), addressed via synthdef `bufnum` parameter | A `LocalBuf` is per-synth and won't survive synthdef recompile / reload; we need persistence across hot-reload. Use the same pattern vibelang already uses for SFZ samples (see `vibelang-sfz`). |
| Reel buffer (initialize empty) | `clear_buf_ir(reel_bufnum)` once at boot | Avoids stale data when a reel is freshly created. |
| Recording (SOS-style) | `record_buf_ar(loop=0, preLevel=sos, recLevel=1-sos)` | Native SOS semantics. Loop=0 because recording stops at end-of-reel; the playback granular reader handles looping separately. |
| Granular playback | `grain_buf_j_ar` (Josh variant) with `Impulse`/`Dust` spawn trigger | `grain_buf_j_ar` exposes per-grain `pan`, configurable `maxGrains`, optional envelope buffer, and a `loop` flag — exactly the four extras Morphagene's Morph wants. `t_grains_ar` is also viable but locks to default Hann and stereo. `grain_buf_ar` lacks `pan`. |
| Whole-splice playback (Gene-Size CCW) | `play_buf_ar` with rate=vari_speed, trigger=play_retrigger | Significantly cheaper than running a granular path that just happens to span the full splice. Use a mode switch: when `gene_size_norm < 0.02`, route the audio through `play_buf_ar` instead of `grain_buf_j_ar`. EOSG in this branch comes from a phase-comparator hitting `splice_end`. |
| Vari-Speed (with Vari-Speed at 0 → halt) | scale + threshold at zero | Built-in to feeding `rate` parameter directly. |
| Per-grain panning | `Latch(white_noise_kr(), spawn_trigger) * pan_amount` → `pan` arg of `grain_buf_j_ar` | Native parameter. |
| Per-grain pitch-up randomization | `Latch(uniform_rand_kr(0, pitch_amount), spawn_trigger)` multiplied into `rate` | Latch UGen + the rate input of `grain_buf_j_ar`. |
| Spawn-rate smoothing ("Dynamic Enveloping") | `Lag(spawn_rate_hz, 0.005)` + an envelope buffer for grain shape (Tukey or Hann with 5–10% taper) | Two-level smoothing matches manual's wording. |
| Splice metadata storage | `LocalBuf(num_channels=1, num_frames=300)` storing splice marker frame indices, with a separate single-frame buffer for `num_splices` | Direct 1-D index lookup with `buf_rd_kr`. |
| Splice marker create/delete | `BufWr` at the marker buffer, indexed by `pulse_count_kr(splice_button_rising_edge)` | One-shot writes; no audio-rate cost. |
| EOS hold (Organize debounce) | `latch_kr(pending_idx, trig=eos_trigger)` | Native sample-and-hold. |
| Shift increment | `stepper_kr(trig=shift_rising, min=0, max=num_splices-1)` with wrap | Built-in. |
| EOSG output | `t_delay(spawn_trig, gene_dur_sec)` then `trig1(... , 0.005)` | TDelay + Trig1 pair. |
| CV Output envelope follower | `Amplitude` UGen (in `dynamics.json`) on `(out_l + out_r) * 0.5` | Direct match. |
| CLK rising-edge | `Schmidt` (1.0, 2.5) + `Changed` for derivative | Two-stage debounce. |
| Auto-Level peak tracker | `Peak` (Triggers manifest) over a 3 s window with reset at combo-start | One-shot operation. |
| Reel persistence (.wav + cues) | `disk_in_ar` for streaming load on reel-mount; `disk_out_ar` for save (wired to a separate "save" synthdef triggered on reel-exit) | `.wav` cue chunks for splice markers must be written/parsed by host code, not in the synthdef itself; expose hooks. |

### 4c. Mode-switching topology

The synthdef has two playback branches that share the same Reel buffer:

```
                    ┌────────────────────────────────────────────────┐
                    │                                                │
   reel_buf ────▶───┤  (A) play_buf_ar (whole-splice path)           │──┐
                    │       used when gene_size_norm < 0.02          │  │
                    │       OR gene_dur > splice_length               │  │
                    │                                                │  │
                    │  (B) grain_buf_j_ar (granular path)            │  ├──▶ out_l, out_r
                    │       used otherwise                           │  │
                    │       trigger = autonomous Impulse OR clk      │  │
                    │       (mode-dependent)                         │  │
                    └────────────────────────────────────────────────┘──┘
                              ▲ (C) record_buf_ar (always present
                              │      during rec_active; reads input,
                              │      writes back into reel_buf)
                          [in_l, in_r]
```

The whole-splice branch (A) is a CPU optimization — at Gene-Size CCW the granular branch (B) would spawn a single grain per splice cycle and the result is bit-identical to a varispeed PlayBuf, but PlayBuf is cheaper. Mode switch is not glitch-free by default; cross-fade between branches over ~5 ms when crossing the gene_size_norm = 0.02 threshold.

---

## 5. Behaviors & gotchas

* **Gene length is in *time*, not samples.** This is the manual's most emphatic structural point: "the temporal length of the Gene is independent of Vari-Speed." It's why EOSG can be used as a clock under speed modulation, and why you can't recreate Morphagene's character by simply scaling sample-counts. In vibelang implementation, store `gene_dur_sec`, not `gene_dur_samples` — convert to samples at grain-spawn time using the *server's* sample rate (not the Reel's, in the rare case they differ).

* **Recording is forward-only at 1×.** Vari-Speed never affects recording, only playback. So during SOS recording with reverse playback, the audio that gets *committed to the buffer* is forward-time (the input + the reversed-loop sum, written forward). This is critical for modeling: do not feed `vari_speed` into `record_buf_ar`'s offset-advance rate.

* **EOS-hold semantics on Organize/Shift.** Switching splices is *announced* (Splice LED color changes, EOSG fires) only when the playhead actually crosses into the new splice — at the next end-of-splice (or end-of-gene if gene_size < splice_length). User-facing: a knob turn does *not* immediately retrigger; you must wait. Implementation: latch the pending splice on EOS.

* **Organize always overrides Shift.** When both are active, the manual is unambiguous: turning Organize *wins*. Implement as a recently-touched flag on Organize (e.g. 500 ms after `Changed(organize_in) > 0`, Organize controls; otherwise Shift).

* **Play input is normalled HIGH.** Unpatched = always loop. With nothing patched, the splice loops indefinitely. Treat the synthdef parameter `play_gate` as defaulting to `1`.

* **Vari-Speed at 12:00 = halt.** A Play-gate rising edge does not retrigger if Vari-Speed is at 0 — nothing advances. The manual: "Play and Vari-Speed respect one another."

* **Morph's behavior past 2:30 is officially undocumented ("???*").** Manual literally annotates the panel section with `???*` and the footnote "Not for laboratory use." Treat this region as best-effort: random pitch-up, 4× overlap, and likely additional ad-hoc randomizations (manual's "panning/pitch/???" labels). Reasonable extension: add per-grain random reverse, per-grain random rate jitter, additional pan spread.

* **CV input scaling table** (consolidated; manual is inconsistent on bipolarity for some inputs):
  * Slide CV: 0…+8 V uni-polar; bipolar attenuverter.
  * Gene-Size CV: 0…+8 V (panel labels "bi-polar" but range table says 0…+8 V); bipolar attenuverter.
  * Vari-Speed CV: ±4 V bipolar; bipolar attenuverter.
  * Morph CV: 0…+5 V uni-polar; **no attenuverter on panel** (unity).
  * Organize CV: 0…+5 V uni-polar (5 Vpp typical of analog sequencer output).
  * SOS CV: 0…+8 V uni-polar; combo pot becomes attenuverter when CV is patched.
  * All gate inputs: ≥ 2.5 V threshold.
  * EOSG out: 0…10 Vpp.
  * CV Output: 0…+8 V DC.

* **Splices are evenly distributed over Organize range, regardless of audio length.** A 10 s splice and a 0.1 s splice each occupy the same arc on the knob. This is a feature, not a bug — it makes hard-cut micromontage easy.

* **microSD persistence is destructive.** "Morphagene is always storing your latest recordings and splices to the microSD Card. If you do not want to write over a card, remove it once you have loaded the desired Reel." For vibelang we likely model this as opt-in (`save_to_disk = false` by default), with manual export — Reels live in RAM unless explicitly committed.

* **Auto-Level applies only to recording.** Output is fixed at ~10 Vpp. Auto-Level just trims input gain so peak playback hits that ceiling.

* **Time-Stretch clock-rate limits** are explicit: min 1 / 3.5 s ≈ 0.29 Hz; max compression rate ≈ 18 Hz (above which no further compression occurs). Enforce in implementation.

* **Default reel state is empty.** The microSD ships blank. Replicate: a fresh vibelang Morphagene synthdef should produce silence until something is recorded into the Reel.

* **Sub-sample Gene-Size at full CW.** The manual: "extremely short (potentially inaudible) at full clockwise". This is *useful* for clicky-pulsar timbres; expose `gene_min_sec` as a synthdef parameter (default 50 µs). At 48 kHz that's ~2.4 samples — already in the click region.

* **"Dynamic Enveloping" is not specified.** Manual's only mention. Best guess from the Morph staircase behavior: per-grain Hann/Tukey window plus a global Lag (~5 ms) on the spawn-rate parameter to prevent zipper artifacts when Morph CV jumps. Document the assumption.

* **Reel color sequence** is fixed: blue → green → light-green → yellow → orange → red → pink → white, then repeats. Useful for UI/visualization parity.

* **Tom Erbe's published designs** lean on classical granular formulations (pulsar synthesis, varispeed buffer playback with windowed reads); his SoundHack +grainSampler and +pitchSampler (free desktop tools from soundhack.com) are direct ancestors. Worth listening to those for sonic-target reference.

---

## 6. Implementation plan (epic breakdown)

Proposed epic: **`epic-morphagene-synthdef`** (~36 SP across six stories).

### Story A — Reel + basic varispeed playback
**Goal:** A single-splice synthdef that loops a stereo buffer with continuous Vari-Speed control.
* **A1** (2 SP) — Define `morphagene_v0.vibe` with parameters: `bufnum` (the active Reel), `splice_start_frame`, `splice_end_frame`, `vari_speed`, `play_gate`, `slide`. Document parameter ranges.
* **A2** (2 SP) — Vari-Speed mapping: bipolar knob+CV → `rate` ∈ [reverse 26 ST, forward 12 ST] with 12:00 halt and finer-near-center cubic curve. Verify against manual's "12 up / 26 down" specification.
* **A3** (2 SP) — Whole-splice playback via `play_buf_ar` with `rate=rate`, `startPos=splice_start + slide*splice_len`, `loop=1`, `trigger=play_retrigger`.
* **A4** (2 SP) — Play-gate logic: rising edge → retrigger; gate-low at EOS → stop; gate-high or unpatched → loop. Use `Schmidt` + diff for edge detection.
* **A5** (1 SP) — CV Output envelope follower (`Amplitude` on out_l+out_r, scaled to 0…+8 V).
* **A6** (1 SP) — `examples/morphagene_loop.vibe` — patches a sample.wav in as a one-splice reel, modulates Vari-Speed and Slide.

### Story B — Splice metadata + edits + Organize/Shift + SOS
**Goal:** Multi-splice reel with Organize/Shift selection, splice marker create/delete, and Sound-On-Sound recording.
* **B1** (3 SP) — Splice metadata: `LocalBuf(1, 300)` for marker frames + `LocalBuf(1, 1)` for `num_splices`. Pre-load from synthdef parameters. `splice_start/end` derived from `committed_splice_idx` via `buf_rd_kr`.
* **B2** (2 SP) — Organize: `pending_idx = floor(organize_norm * num_splices)`; `committed_idx = Latch(pending_idx, trig=eos)`. Verify EOS-hold semantic by ear.
* **B3** (2 SP) — Shift gate: `Stepper(trig=shift_rising, min=0, max=num_splices-1, step=1)`. Combine with Organize via "recently-touched Organize wins" logic.
* **B4** (3 SP) — Splice button/gate: rising-edge → write current playback frame to marker buffer at `num_splices`, increment `num_splices`. Implement marker-delete (Shift+Splice) and clear-all (Shift+Splice 3 s).
* **B5** (3 SP) — SOS RecordBuf: `record_buf_ar(loop=0, preLevel=sos_norm, recLevel=(1-sos_norm)*input_gain*rec_active, run=rec_active)`. Wire SOS combo pot + CV with attenuverter behavior.
* **B6** (2 SP) — Auto-Level: REC+Shift combo → `Peak` over 3 s → `input_gain` parameter latched.
* **B7** (1 SP) — `examples/morphagene_splices.vibe` — preset reel with 4 splices, sequencer driving Organize.

### Story C — Granular path: Gene-Size, Slide, Morph
**Goal:** The microsound layer — `grain_buf_j_ar`-based granular playback with full Morph staircase.
* **C1** (2 SP) — Gene-Size mapping: `gene_dur_sec = mix(splice_len, 50µs, gene_size_norm^3)`. CV+atten merge.
* **C2** (3 SP) — Granular branch: `grain_buf_j_ar(numChannels=2, sndbuf=reel_buf, dur=gene_dur_sec, rate=rate, pos=slide_norm_in_reel, pan=grain_pan, trigger=spawn_trig, envbufnum=tukey_buf)`. Wire splice_start/end into pos calculation.
* **C3** (3 SP) — Morph staircase: piecewise spawn-rate curve gap→1:1→2:1→3:1+pan→4:1+pitch-up. Emit `spawn_trig = Impulse(spawn_rate_hz)`; lag spawn_rate by 5 ms.
* **C4** (2 SP) — Per-grain pan: `Latch(white_noise_kr, spawn_trig) * pan_amount(morph_norm)`.
* **C5** (2 SP) — Per-grain pitch-up: `Latch(uniform_rand_kr(0, pitch_amount(morph_norm)), spawn_trig)` multiplied into `rate`.
* **C6** (2 SP) — Branch switching: when `gene_size_norm < 0.02` route to whole-splice (A3) path; cross-fade 5 ms across the threshold.
* **C7** (1 SP) — `examples/morphagene_granular.vibe` — small Gene-Size + Morph sweep.

### Story D — Synchronization: EOSG, CLK modes (Gene Shift, Time Stretch)
**Goal:** Clocked playback modes and the EOSG output.
* **D1** (2 SP) — EOSG: `t_delay(spawn_trig, gene_dur_sec)` → `trig1(0.005s)` → scale to 10 V. Verify steady under Vari-Speed modulation.
* **D2** (3 SP) — Gene Shift mode: detect (CLK patched) ∧ (Morph < 0.46) ∧ (gene_size_norm > 0.02). Replace autonomous spawner with `Impulse` driven by clk_rising. Advance grain pos by `gene_dur_sec * sign(rate)` per clock.
* **D3** (3 SP) — Time Stretch mode: detect (CLK patched) ∧ (Morph ≥ 0.46) ∧ (gene_size_norm > 0.02). Reroute Vari-Speed → grain rate (pitch). Advance grain pos linearly across splice on each clock; total clocks-to-traverse = splice_len / gene_dur_sec.
* **D4** (1 SP) — Clock-rate clamping: warn / clip outside [1/3.5 s, 18 Hz] for Time Stretch.
* **D5** (1 SP) — `examples/morphagene_time_stretch.vibe` and `morphagene_gene_shift.vibe`.

### Story E — Reel registry + persistence (microSD model)
**Goal:** Multiple Reels, hot-swap, .wav persistence with cue-marker support.
* **E1** (3 SP) — Reel registry: per-reel persistent server buffer; runtime side. Synthdef param `bufnum` + matching splice-metadata buffer pair.
* **E2** (3 SP) — `.wav` reader: parse `cue ` and `LIST/adtl/labl` chunks at load time → populate splice marker buffer. Use existing vibelang SFZ-style buffer-load path (`vibelang-sfz` / disk_io).
* **E3** (3 SP) — `.wav` writer: on reel-save, render the Reel buffer to disk via `disk_out_ar` (or off-line), embedding splice markers as cue chunks. Off-DSP path; runtime side.
* **E4** (1 SP) — Reel color cycling for UI parity (blue → green → ... → white, repeat).
* **E5** (1 SP) — Empty-on-create semantics: new Reel = `clear_buf_ir`.

### Story F — Examples + KB documentation + Spectranoise-style extras
**Goal:** User-facing guidance and audible reference.
* **F1** (2 SP) — `kb/morphagene-howto.md` — user manual for the synthdef family, mirroring `kb/spectraphon-howto.md` style.
* **F2** (1 SP) — `examples/morphagene_micromontage.vibe` — Splice + Organize sweep demonstrating EOS-hold scheduling.
* **F3** (1 SP) — `examples/morphagene_sos_decay.vibe` — Sound-On-Sound feedback decay.
* **F4** (1 SP) — `examples/morphagene_pitched_grain.vibe` — Vari-Speed-as-pitch under Time Stretch.
* **F5** (2 SP) — Audio reference renders: drone, granular cloud, SOS decay, time-stretch, gene-shift. Compare to public Morphagene demo videos for sonic target.
* **F6** (2 SP) — Optional: "Morphagene-???*" mode — exploit the manual's undocumented Morph > 2:30 region. Add per-grain random reverse and rate jitter as separate parameters; document and ship as an extension.

**Total:** ~62 SP. Recommended sequencing: A → B → C → D → E → F. Story A alone (~10 SP) gives a usable single-splice Vari-Speed sampler, which is already a meaningfully novel synthdef on its own. The full character only emerges after Story C (granular + Morph).

---

## 7. Risks & open questions

| Risk / open question | Impact | Mitigation / substitute |
|---|---|---|
| **"Dynamic Enveloping" is undocumented.** The manual mentions it as the smoothing technique that prevents grain-glitches but does not specify the algorithm. | Medium — affects perceived smoothness of Morph sweeps. | Default to per-grain Tukey window (5–10% taper) plus a 5 ms `Lag` on spawn-rate; expose taper width and lag time as synthdef parameters; tune against a hardware demo render. |
| **Tom Erbe's exact varispeed interpolator** (sinc? cubic?) is undisclosed. Manual emphasizes "high fidelity playback even when under heavy modulation." | Subtle — affects timbre at extreme speeds. | Use cubic interpolation in `grain_buf_j_ar` (`interp=4`). For premium quality optionally upsample to 96 kHz internally and downsample (significant CPU cost). |
| **Auto-Level algorithm** is not disclosed. | Low — only affects gain calibration UX. | Use `Peak` over a 3 s window; set `input_gain = -1 dBFS / measured_peak`. Document choice; expose as parameter. |
| **Morph > 2:30 ("???*") region** is officially undocumented. | Medium — affects the most extreme/expressive end of the Morph control. | Implement a documented best-effort: 4× overlap, random pitch-up to +5 ST, panning spread, plus an *opt-in* extension mode adding random reverse + rate jitter. Mark clearly in the synthdef as "experimental, hardware behavior undocumented." |
| **EOS-hold semantic timing** depends on whether "EOS" means end-of-Gene or end-of-Splice. From manual: "the currently selected Splice or Gene plays to the end before the next Splice is selected" — so it depends on whether Gene-Size is full CCW. | Low — well-defined once you split the cases. | Latch on `eos = max(end_of_gene_trig, end_of_splice_trig)` — the larger window dominates when Gene-Size is CCW (Gene = Splice). |
| **microSD persistence model** for vibelang. Hardware constantly writes; vibelang likely shouldn't. | Low — a UX policy question. | Default `save_to_disk = false`; explicit save command on reel-eject. Document divergence. |
| **WAV cue-marker parsing/writing.** Standard format but vibelang's `disk_in_ar` may not currently parse cue chunks. | Medium — required for editing reels in Reaper. | Add cue-chunk parser to `vibelang-sfz` or a new `vibelang-wav` helper crate. Off-DSP, runtime-side. |
| **Reel-buffer size** (~67 MB stereo 32-bit at 2.9 min). With multiple Reels held in RAM this scales fast. | Medium — memory pressure in long sessions. | Stream from disk via `disk_in_ar` for inactive reels; promote to RAM (`buffer.read`) only on Reel select. Document the trade-off. |
| **Hot-reload safety** when a Reel buffer is mid-record. | High — could corrupt the Reel on synthdef recompile. | Detach reel buffers from the synthdef (top-level / shared); recompile only swaps the read/write nodes, never reallocates buffers. Mirror the spectraphon plan's Story C3 approach. |
| **Sample-rate divergence** between server (e.g. 44.1 kHz) and Morphagene's native 48 kHz. | Low — `BufRateScale` handles it. | Always rate-scale buffer reads via `buf_rate_scale_kr(reel_bufnum)`; gene-dur stays in seconds (server-rate-independent). |
| **Polyphonic Morph (4× overlap)** can spawn many grains. With 4 grains per Gene at small Gene-Size (e.g. 5 ms) and high Vari-Speed, total active grains stay bounded — but at very long Gene-Size (e.g. 5 s) under 4× overlap there are 4 grains at any moment, fine. The risk is `maxGrains` being too low. | Low. | Set `maxGrains = 64` in `grain_buf_j_ar`; expose as parameter. |
| **Reverse playback during SOS** — recording is forward-only but playback (the loop side of SOS) can run reversed. Modeling this requires the granular reader to keep advancing through Vari-Speed-modulated positions while the recorder advances at +1× simultaneously. | Medium — easy to accidentally couple them. | Keep the recorder's frame-advance separate from the granular reader's; do not derive one from the other. |

---

## Sources

* Make Noise Morphagene product page — https://www.makenoisemusic.com/modules/morphagene/
* Morphagene manual (PDF, hosted by Make Noise; 33 pages, dated 2025-04-09) — https://www.makenoisemusic.com/wp-content/uploads/2024/03/morphagene-manual.pdf
* Mirror manual — https://www.makenoise-manuals.com/morphagene/morphagene-manual.pdf
* ManualsLib mirror (33 pages) — https://www.manualslib.com/manual/1285963/Make-Noise-Soundhack-Morphagene.html
* Signal Flux Morphagene technical guide — https://signalflux.org/guides/morphagene
* Morphagene front-panel cheat sheet (Jeremy Soza, gearspace) — https://gearspace.com/board/attachments/modular-mania-all-things-eurorack-and-modular-synths-effects/811416d1554934432-make-noise-morphagene-thread-morphagene-front-panel-cheat-sheet.pdf
* SoundHack — Tom Erbe's eurorack page — https://www.soundhack.com/echophon/
* Perfect Circuit interview with Tom Erbe (Mimeophon, related design philosophy) — https://www.perfectcircuit.com/signal/tom-erbe-mimeophon-interview
* Synthtopia / DivKid NAMM 2017 coverage — https://divkidvideo.com/make-noise-morphagene-at-namm-2017/
* Curtis Roads, *Microsound* (MIT Press, 2001) — origin of the nine-timescale framework cited in the manual.
* vibelang UGen manifests — `crates/vibelang-dsp/ugen_manifests/{buffers,granular,sc3_josh_granular,disk_io,triggers,dynamics,oscillators,pitchtime}.json`
* Existing reference: `kb/spectraphon-synthdef-plan.md` — same author lineage (Erbe/Make Noise), same structural conventions.
