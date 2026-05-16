# Spectraphon Synthdef Recreation — Deep Analysis & Plan

> Research-only plan for recreating the Make Noise / Soundhack Spectraphon as a vibelang synthdef family.
> No code in this document — pure analysis. Implementation tickets are listed in §6.

## 1. Module overview

The **Make Noise / Soundhack Spectraphon** (released 2023, designed by Tom Erbe of Soundhack with Make Noise's hardware team) is a **34 HP dual spectral oscillator** built on Make Noise's new DSP platform (5 V CODEC, fully DC-coupled, 2 audio in / 8 audio out). Its identifying idea: rather than oscillating a static waveform, each side either continuously *analyzes* an incoming audio signal into a set of harmonic amplitudes (SAM — Spectral Amplitude Modulation) or *plays back* sets of stored harmonic amplitude arrays (SAO — Spectral Array Oscillation), driving an internal harmonic oscillator whose **odd** and **even** partials appear at separate outputs. The two sides A and B are nearly identical, share an FM bus, and interact via Follow/Sync modes. A free 2023 firmware ("Spectranoise") added two additional per-side modes: **Chaos** (paired sines with chaotic feedback) and **Noise** (sine carriers modulated by filtered noise sidebands). Heritage: Buchla 296 (16-band programmable spectral processor), Buchla 259 (complex VCO), Touché. Available outputs are *additive* in nature (Sine, Sub/CV, Odd, Even per side), not phase-vocoder resynthesis — the spectrum is read out by an oscillator bank, not IFFT.

```
                       ┌─────────────────────────── SIDE A ──────────────────────────────┐
 In-A ──▶┌────────────┐                              ┌──────────────────────┐
         │  ANALYZE   │ mag[k]                       │  HARMONIC OSC BANK   │──▶ Sine   (core freq, clean)
         │  (SAM:     │──┐                           │  freq = f0           │──▶ Sub/CV (env.foll. SAM | sub SAO)
         │   real-time│  │                           │  partials k=1..N     │──▶ Odd    (k=1,3,5,…  · partials gate)
         │   spectral │  │   ┌──────────────────┐    │  amps  = mag[k]      │──▶ Even   (k=2,4,6,…  · partials gate)
         │   tracker) │  └─▶│ ARRAY MEMORY     │───▶│  shape via Slide,    │
         └────────────┘     │ 16 slots × M     │    │  Focus, Partials     │
                  ▲ Shift+ARRAY (capture)      │    └──────────▲───────────┘
                  │         │ (Slide/Focus pick│               │
 SAM/SAO ─────────┘         │  spectrum in     │  CV: Slide / Focus / Partials (with attenuverters)
                            │  SAO mode)       │
                            └──────────────────┘
                                   ▲
                                   │ Clock advances Array index in SAO

                    FM BUS  (between A and B)              Follow / Sync (shared switch)
                    A-FM-Index   B-FM-Index                B's pitch becomes offset of A in Follow;
                       │             │                     phase-syncs B to A in Sync
                       └─────────────┘
                       (mirrored side B identical)
```

The output you patch is *not* the analyzed input passed through — it is a freshly-generated tone built from harmonic partials whose **amplitudes** come from the analyzed/stored spectrum. Pitch is set by side A/B's own oscillator; the input only contributes its spectral envelope.

---

## 2. Frontpanel inventory

The module has two **mirror-symmetric Sides** (A on the left, B on the right). The center column houses the shared **FM Bus** and **Follow/Sync** controls.

### 2a. Per-side controls (replicated for A and B)

| Control | Type | Range | Function | CV input | Notes |
|---|---|---|---|---|---|
| **Frequency** | knob | wide (sub-audio to ≈ Nyquist) | Coarse pitch of the side's oscillator (V/oct summed) | 1V/oct in (Pitch) | Sets the **fundamental** for the harmonic bank |
| **Fine-Tune** | knob | ± a few semitones | Fine pitch trim, summed with Frequency + V/oct | — | — |
| **A-In / B-In Attenuverter** | knob | −1 … 0 … +1 | Bipolar gain on the audio input (drives ANALYZE in SAM, FM index in SAO) | — | Attenuverters are bipolar across the whole panel |
| **Slide** | knob | 0…full | **SAM**: governs how input modulates harmonics (selects the fundamental ratio of the analysis grid). **SAO**: scans Slide axis of the Array (modulates which spectrum is read). | Slide CV in (with attenuverter) | Erbe-described as "balance between odd and even harmonics" |
| **Focus** | knob | 0…full | **SAM**: width of the harmonic-bandwidth window the input drives. **SAO**: scans Focus axis of the Array. | Focus CV in (with attenuverter) | Erbe-described as "density of partials" |
| **Partials** | knob | 0…full | Combined amplitude+timbre gate on the Odd/Even outputs. CCW = silent; turning CW progressively unmutes harmonics from low → high (alternating odd/even). At full CW all harmonics ring at their array amplitudes. | Partials CV in (with attenuverter) | Acts like a low-pass-gate / VCA hybrid |
| **SAM/SAO** | latching button | 2-state | Selects this side's mode. LED lit = SAO. | — | (Spectranoise firmware: cycles SAM → SAO → Chaos → Noise) |
| **Shift** | momentary button | — | (a) Manual clock pulse for this side's Array stepping; (b) gold-labeled secondary functions when held: Shift+ARRAY = begin/end Array capture (SAM); Shift+CV = Sub/CV mode select; Shift+other-side-Shift = Array selection |  — | Acts as a chord-key for combos |

### 2b. Per-side jacks

| Jack | Direction | Type | Function | Normalization |
|---|---|---|---|---|
| **In-A / In-B** | input | audio | SAM: signal to analyze. SAO: FM modulator into A-FM-Index path. | Required for SAM analysis |
| **1V/oct** | input | CV | V/oct pitch tracking | DC-coupled |
| **Slide CV** | input | CV (attenuverted) | Modulates Slide | bipolar |
| **Focus CV** | input | CV (attenuverted) | Modulates Focus | bipolar |
| **Partials CV** | input | CV (attenuverted) | Modulates Partials | bipolar |
| **Sine** | output | audio | Pure sine at the side's core frequency (untouched by Partials/Slide/Focus) | Useful for tuning |
| **Sub/CV** | output | audio/CV | SAM: envelope follower on input. SAO: sub-oscillator (Sawtooth on A, Saturated Sine on B). Switchable to clockable LFO via Shift+CV. | Fully DC-coupled |
| **Odd** | output | audio | Odd-numbered harmonics (k = 1, 3, 5, …) gated by Partials | Even harmonics sum into Odd if Even is unpatched |
| **Even** | output | audio | Even-numbered harmonics (k = 2, 4, 6, …) gated by Partials | When unpatched, normalized into Odd |

### 2c. Shared / center-column controls

| Control | Type | Function | Notes |
|---|---|---|---|
| **A-FM-Index** | knob | Sets FM depth from B → A (audio-rate, internal; bypasses jacks) | High-definition internal bus |
| **B-FM-Index** | knob | Sets FM depth from A → B | — |
| **Array Binary display** | 4 colored LEDs | Shows current Slide/Focus operating point as 4-bit binary; during Array selection shows Array index (0..15) | Same display, dual function |
| **Clock** | input | trigger/CV | Steps Arrays in SAO mode; rhythmic source for Sub/CV-as-clock |
| **Follow / Sync** | shared button | Cycles: off → Follow → Sync. Follow: B's Frequency knob and 1V/oct become offsets of A (centered at 12:00). Sync: hard-sync of one side to the other. | Follow makes Side B a tracking interval from Side A |

### 2d. Hidden / firmware modes

* **Array creation:** `Shift + Shift+ARRAY` (or Shift+ARRAY single-press) begins record. Subsequent capture frames into selected slot. End with same combo.
* **Array selection:** Hold this side's Shift, press the *other* side's Shift; binary display shows index (0–15).
* **Default Array restore:** Delete files from the microSD card.
* **Sub/CV mode:** Shift+CV cycles between envelope-follower / sub-osc / clockable LFO.
* **Spectranoise firmware:** Free Soundhack firmware released late 2023; replaces stock firmware via SD card. Adds **Chaos** mode (paired sines, ratio set by Focus, audio-rate Partials modulation, Slide adds chaotic feedback paths) and **Noise** mode (sine carriers modulated by noise sidebands; Slide = LP, Focus = HP). Also improves low-frequency Partials compensation in SAO.
* **Tuning Beacon (LED on Frequency knob):** Green = simple integer ratios between A and B (2:1, 3:2, 4:3); Red = next-simplest (5:4, 6:5).

---

## 3. DSP architecture (what's actually inside)

Spectraphon is **not** a phase-vocoder/IFFT resynthesizer. The Soundhack design and the panel taxonomy ("harmonics", "Odd/Even outputs", "partials") strongly indicate an **analysis-driven additive bank**: a real-time partial-tracking analyzer extracts harmonic amplitudes referenced to a fundamental, and those amplitudes drive a bank of harmonic sine oscillators tuned to the side's pitch. This matches Tom Erbe's prior Soundhack tools (PvocEx, partial trackers) and Buchla 296 lineage.

### 3a. Analyze stage (SAM)

* **What:** Real-time spectral analysis of the audio at In-A/B. Produces a vector of harmonic magnitudes referenced to a *user-controlled fundamental*. Slide chooses the fundamental within the analyzed signal (e.g. tracks every 100 Hz vs every 200 Hz harmonic grid); Focus sets the bandwidth around each harmonic (narrow = pure tracking, wide = scrape neighboring spectral energy in).
* **Likely implementation:** Short-time FFT (likely 512–2048 sample windows at 48 kHz, ~10–40 Hz bin spacing) followed by **harmonic-product / partial-bin summation**: for each integer multiple k of the chosen fundamental, sum magnitudes within ± Focus·bins around bin index `k·f0/SR·N`. Output: vector `mag[1..N]` of harmonic amplitudes.
* **Alternative:** Sinusoidal partial tracker (peak-pick + per-frame matching). Tom Erbe's published work uses both.
* **Update rate:** every FFT hop (~10 ms hops typical), smoothed.

### 3b. Spectral memory (Arrays)

* **Structure:** Up to **16 Arrays per side**. Each Array is a *2-D table* indexed by (Slide, Focus) — i.e. the SAM session is recorded as a continuous trajectory through Slide/Focus, and the Array stores `mag[1..N]` at each grid cell.
* **Capture:** Initiated/ended by Shift+ARRAY. While capturing, every analysis frame is written to the Array at the current (Slide, Focus) cursor.
* **Playback (SAO):** Slide/Focus knobs become *position* into the 2-D Array, returning an interpolated `mag[1..N]` for the current location. Clock input can step linearly through stored frames.
* **Persistence:** microSD card.

### 3c. Oscillator / playback engine

* **Harmonic oscillator bank:** N sine oscillators, frequencies `k · f0` for k=1..N, amplitudes from `mag[k]`. f0 = Frequency + Fine-Tune + 1V/oct.
* **N (partial count):** unpublished, but consistent with panel and CPU budget: likely **64–128 partials** (8 LEDs in the binary display × 16 stored Arrays suggests internal grid resolution; partial count is independent and likely a power of 2). Treat **64 partials** as default for vibelang recreation.
* **Odd / Even split:** the bank is *physically* split — odd-k partials feed the Odd output, even-k partials feed the Even output. This is direct (not a post-FFT filter).
* **Sine output:** k=1 partial only, full amplitude, never gated by Partials.
* **Sub/CV output:**
  * SAM: envelope-follower on the input (rectified + lowpass).
  * SAO: dedicated sub-oscillator tuned to f0 / 2 (sawtooth on A, saturated sine on B). Can be retasked as a clockable LFO via Shift+CV.

### 3d. Per-control mapping (DSP semantics)

| Control | Internal effect |
|---|---|
| **Frequency / Fine-Tune / V-oct** | Sets `f0` for the harmonic bank. Does **not** retune the analyzer. |
| **Slide (SAM)** | Sets the fundamental of the analysis grid (independent of `f0`). Effect: changes which harmonic ratios are tracked. |
| **Slide (SAO)** | Selects X-axis position into the Array's stored spectra. |
| **Focus (SAM)** | Width of analysis sum-window around each harmonic. CCW: narrow tracking; CW: gather neighboring noise energy. |
| **Focus (SAO)** | Selects Y-axis position into the Array. Combined with Slide, picks one stored spectrum out of an interpolated 2-D field. |
| **Partials** | Progressive amplitude/timbre gate: a non-linear envelope across `mag[k]` — at low values, only `mag[1..2]` survive at reduced level; sweeping CW unmutes harmonics in alternating odd/even order, brightening and loudening. Equivalent to a tilted lowpass over partial-index combined with a VCA. |
| **A-FM-Index / B-FM-Index** | Audio-rate FM of one side's `f0` by the other side's Sine output, with depth scaled by the knob. |
| **Follow** | B's pitch becomes `pitch_A + offset_B` where offset is centered at 12 o'clock (i.e. B's knob/V-oct become an interval). |
| **Sync** | Hard-sync of one oscillator's phase to the other (likely Side B's bank phase reset by Side A's f0 zero-crossing). |

### 3e. Chord / scale modes

Spectraphon has **no explicit pitch-quantization mode**. "Chord" effects come from:
1. The harmonic ratios (always strict integer multiples of f0 in SAM/SAO),
2. Tuning Beacon visually guides Frequency knobs to consonant ratios between A and B (it does not lock them),
3. FM and Sync between sides yield spectral chord families.

There is no diatonic/chromatic quantizer. Replicating "chord" character is therefore a *gestural* problem — get the partial-bank ratios and FM bus right, and chordal sonorities emerge naturally.

### 3f. Dual-channel routing

* Sides are **independent** by default — different modes, different pitches, independent Arrays.
* **FM Bus** is symmetric and audio-rate: A's f0 is FM-modulated by B's Sine output (A-FM-Index depth) and vice versa.
* **Follow** removes B's independence: B becomes a tracked interval of A.
* **Sync** keeps both phases coupled.
* **Even/Odd normalization** is per-side only; A's Even ≠ B's Odd.

---

## 4. UGen mapping per behavior

Vibelang's UGen toolbox provides three plausible implementation paths. We propose a **hybrid** in which the harmonic bank is built directly with `klang_ar`-style additive synthesis (because Spectraphon is fundamentally additive, not IFFT), while the analyzer uses FFT + custom harmonic-summation. JoshUGens (`pv_record_buf_kr`, `pv_bin_play_buf_kr`, `pv_partial_synth_f_kr`) provide the Array storage primitives.

### 4a. Per-control implementation table

* **Spectraphon control:** Frequency / Fine-Tune / 1V/oct
  * **Function:** Sets `f0` of the per-side harmonic oscillator bank.
  * **Vibelang impl:** Sum of `freq` + `finetune` + `voct_to_hz(voct_in)` → drive `klang_ar` frequency array as `[f0, 2*f0, 3*f0, …, N*f0]`.
  * **Param mapping:** `freq` ∈ [20 Hz, 8 kHz] (knob) + `voct` ∈ [-5, +5 V] → multiply by `2^voct`.

* **Spectraphon control:** Slide (SAM)
  * **Function:** Selects analysis fundamental (which harmonic grid to track in input).
  * **Vibelang impl:** Drives the analyzer's `f0_analyze` parameter — used to compute, for each k, the bin index `k * f0_analyze / SR * N` in the FFT chain.
  * **Param mapping:** `slide` ∈ [0, 1] → `f0_analyze` ∈ [50 Hz, 800 Hz] (log).

* **Spectraphon control:** Slide (SAO)
  * **Function:** Picks X-axis position in the Array.
  * **Vibelang impl:** Use `pv_bin_buf_rd_kr` or `pv_buf_rd_kr` from JoshUGens with `point` driven by Slide; or, if storing Arrays as plain magnitude-arrays in audio buffers, `buf_rd_ar` with 2-D indexing (slide → row index).
  * **Param mapping:** `slide` ∈ [0, 1] → `point` ∈ [0, 1].

* **Spectraphon control:** Focus (SAM)
  * **Function:** Width of harmonic bandwidth window in analyzer.
  * **Vibelang impl:** Controls the number of FFT bins summed around each harmonic bin. Implemented as parameter to a custom `harmonic_extract` UGen, OR by feeding FFT chain through `pv_mag_smear_kr` (smear bin amplitudes) before harmonic summation.
  * **Param mapping:** `focus` ∈ [0, 1] → `bandwidth_bins` ∈ [1, 16] linear.

* **Spectraphon control:** Focus (SAO)
  * **Function:** Y-axis position in the Array (orthogonal to Slide).
  * **Vibelang impl:** Second axis of the 2-D Array buffer lookup. If Array stored as `[16 slots × N_grid_cells × N_partials]`, focus picks the cell along Y; combined with Slide for X.
  * **Param mapping:** `focus` ∈ [0, 1] → cell row.

* **Spectraphon control:** Partials
  * **Function:** Progressive non-linear gate over partial amplitudes; alternates odd/even.
  * **Vibelang impl:** Apply a per-partial gain `g[k] = clamp(partials*N*2 - k, 0, 1)` after splitting into odd/even sub-banks. Optionally squash with `pow(g, 0.5)` for the curve. Implement as a two-stage VCA: per-partial tilt mask, then global VCA.
  * **Param mapping:** `partials` ∈ [0, 1] → harmonics 1..N revealed sequentially; CW also boosts overall level.

* **Spectraphon control:** Odd output
  * **Function:** Sum of partials k=1, 3, 5, …
  * **Vibelang impl:** Build a separate `klang_ar` (or sub-summed `sin_osc_ar` array) using only odd-k frequencies and amplitudes `mag[1], mag[3], …`. Or use `pv_odd_bin_kr` if working in the FFT domain (but this gates *bins*, not *partials* — only equivalent when `f0` aligns with FFT bin spacing). The additive-bank path is preferred for Spectraphon's character.
  * **Param mapping:** N/A — output node.

* **Spectraphon control:** Even output
  * **Function:** Sum of partials k=2, 4, 6, …
  * **Vibelang impl:** Mirror of Odd — second `klang_ar` for even k. Apply normalization rule: when Even output is unpatched, sum into Odd bus.
  * **Param mapping:** N/A.

* **Spectraphon control:** Sine output
  * **Function:** Clean k=1 sine.
  * **Vibelang impl:** Standalone `sin_osc_ar(f0)`; never multiplied by Partials gate.

* **Spectraphon control:** Sub/CV (SAM)
  * **Function:** Envelope follower on input.
  * **Vibelang impl:** `amplitude_kr(in_signal, attack=0.01, release=0.1)` (use `Amplitude` UGen) → smooth → CV-rate output. Or rectify+lowpass.

* **Spectraphon control:** Sub/CV (SAO, side A)
  * **Function:** Sub-oscillator, sawtooth.
  * **Vibelang impl:** `saw_ar(f0 / 2)`.

* **Spectraphon control:** Sub/CV (SAO, side B)
  * **Function:** Sub-oscillator, saturated sine.
  * **Vibelang impl:** `tanh(drive * sin_osc_ar(f0 / 2))` with drive ≈ 2.0–3.0.

* **Spectraphon control:** A-FM-Index / B-FM-Index
  * **Function:** Internal audio-rate FM between sides.
  * **Vibelang impl:** Add `fm_index_a * sin_b` to A's `f0` parameter (and symmetrically). Implement using vibelang's existing FM idiom (modulating freq input to klang).
  * **Param mapping:** `fm_index` ∈ [0, 1] → modulation depth ∈ [0, f0] Hz (linear) — exponential mapping yields clearer character.

* **Spectraphon control:** Follow mode
  * **Function:** B tracks A with offset.
  * **Vibelang impl:** When Follow flag set, replace `f0_b = f0_b_independent` with `f0_b = f0_a * 2^(b_voct_offset)` where `b_voct_offset` is `freq_b_knob - 0.5` mapped to ±5V/oct.

* **Spectraphon control:** Sync mode
  * **Function:** Hard-sync B to A.
  * **Vibelang impl:** Use phase-reset trigger: detect zero-crossing of A's master sine via `coyote_ar` or trigger UGen, send as reset to B's klang phase. May require a custom UGen if klang doesn't expose phase reset; fallback is to drive B's sub-bank as `sin_osc_ar` instances with phase-modulating reset.

* **Spectraphon control:** Clock input
  * **Function:** Steps Array index (SAO) or generates rhythmic Sub/CV.
  * **Vibelang impl:** `pulse_divider_kr` / counter UGen, increments an `array_index` accumulator on each rising edge.

* **Spectraphon control:** Tuning Beacon LED color
  * **Function:** Indicates A:B frequency ratio simplicity (informational only).
  * **Vibelang impl:** Optional — compute `ratio = f0_a / f0_b`, find nearest small-integer ratio, expose as a meta-output for visualization. Not synthesis-critical.

* **Spectraphon control:** Spectranoise — Chaos mode
  * **Function:** Pair of sines per side, second sine ratio set by Focus, Slide adds chaotic feedback.
  * **Vibelang impl:** `sin_osc_ar(f0)` summed with `sin_osc_ar(f0 * focus_ratio)`; feedback loop via `fb_sine_l_ar` or `latoocarfian_n_ar` driving a phase or amplitude modulation. Partials becomes audio-rate AM depth.

* **Spectraphon control:** Spectranoise — Noise mode
  * **Function:** Sine carriers modulated by filtered noise sidebands; Slide=LP, Focus=HP.
  * **Vibelang impl:** `pink_noise_ar` → `lpf_ar(slide_cutoff)` → `hpf_ar(focus_cutoff)` → multiply with `sin_osc_ar(f0)` (and `sin_osc_ar(2*f0)` for even side). Output as Odd/Even.

### 4b. Toolkit choice rationale

| Spectraphon stage | Best vibelang toolkit | Reason |
|---|---|---|
| Real-time analyzer (SAM) | `fft_kr` + custom harmonic summation (or `pv_partial_synth_f_kr` for residual suppression) | We need *harmonic-relative* magnitudes, not bin magnitudes. Build a UGen wrapper that buckets bins by harmonic index of a given f0_analyze. |
| Array storage | `local_buf_kr` per Array, recorded via `pv_record_buf_kr` (frame-level) or via a custom audio-buffer write that stores `mag[1..N]` | Need 16 Arrays × side, addressable by Slide/Focus; FFT-frame storage works if N is small enough. Otherwise plain audio buffers with 1 sample per partial per frame are simpler. |
| Harmonic oscillator bank | `klang_ar` (sine bank, fixed freq/amp arrays) — or N parallel `sin_osc_ar` for amp-modulation | Spectraphon is additive; klang is the direct match. Use one klang for odd, one for even. |
| Odd/Even split | Two separate klang banks with disjoint freq arrays | Cleaner than `pv_odd_bin_kr` because it tracks exact harmonic ratios rather than FFT bins. |
| Partials gate | Custom per-partial mask × global VCA | Tilt + amplitude in one node. |
| FM bus | Standard frequency-modulation idiom (sum `index * sin_b` into A's freq) | No special UGen needed. |
| Sub/CV | `amplitude_kr` (SAM) / `saw_ar`+`tanh` (SAO) | Direct primitive matches. |
| Chaos mode | `fb_sine_l_ar` or pair of `sin_osc_ar` + chaos UGen | Spectranoise behavior is well within `chaos.json`. |
| Noise mode | `pink_noise_ar` + `lpf_ar`/`hpf_ar` + AM with `sin_osc_ar` | Trivial. |
| FFT-based fallback (alternative, simpler) | `fft_kr` → magnitude-only manipulation → `ifft_ar` | Less faithful to Spectraphon's odd/even-split character but a viable lo-fi recreation. |

---

## 5. Behaviors & gotchas

* **ANALYZE is continuous, not triggered.** SAM is always-on real-time analysis while in SAM mode; there is no "freeze a frame" button outside of stopping playback. Capturing into an Array is what creates a persistent snapshot — and that capture itself records a *trajectory* of frames over time, not a single frame.
* **Switching SAM → SAO is destructive to your patch:** when you flip to SAO, the input no longer drives anything (it instead becomes an FM modulator). The cheat sheet specifically warns: "When switching from SAM to SAO, you will usually want to unpatch your sound source from the input."
* **Even output normalizes into Odd.** If a patch only takes the Odd output, all harmonics sum to it. Replicate this in vibelang: declare Odd as the default summing bus, and have Even subtract from Odd when Even is patched (i.e. expose the patched-state as a parameter).
* **CV scaling:**
  * 1V/oct is standard.
  * Slide/Focus/Partials CV inputs are bipolar with attenuverters — assume **±5 V** full-scale at the jack, attenuverter scales [-1, +1]; final knob+CV result clamps to [0, 1] internally.
  * All jacks DC-coupled (per Make Noise platform spec).
* **Tuning is per-side and analyzer-independent.** The analyzer's f0 (Slide in SAM) is *not* the oscillator's f0 (Frequency knob). You can tune the oscillator to a melodic pitch while keeping the analyzer locked to the input's actual fundamental — this is core to the instrument's sound.
* **Hidden mode entry:** Spectranoise firmware adds modes, accessed by cycling SAM/SAO multiple times. Treat the two extra modes as 3rd/4th positions of the mode parameter, not as a separate engine.
* **Array creation requires no input.** While SAM is the natural source, you can also capture in SAO → re-storing what's currently being played back (with new Slide/Focus trajectory). Make Noise documentation explicitly notes "no prescribed best practice" for Array creation.
* **Sub/CV mode is sticky per side.** It survives mode changes; user-set via Shift+CV.
* **Default patch (no input, no CV, SAO mode):** the factory Arrays produce a recognizable harmonic-rich tone. Replicate this with a default Array containing a saw-like spectrum (`mag[k] = 1/k`).
* **Partial count is undocumented.** Use **64 partials** as a working default for the vibelang recreation; expose as a synthdef parameter so users can scale up to 128.
* **Partials CCW = silence, not "fundamental only".** The Sine output is the only path that survives at Partials=0; Odd/Even are fully muted.
* **Tuning Beacon is informational only.** It does *not* quantize.

---

## 5a. Current split implementation

The implemented stdlib surface uses a split graph rather than the original
monolithic `spectraphon_side`/`spectraphon_dual` synthdefs:

| File | Role |
|---|---|
| `spectraphon_analyzer.vibe` | Real SAM analyzer: one `fft_kr` chain, 64 `bin_data_kr` harmonic reads, and 64 `buf_wr_kr` writes to a shared magnitude buffer. |
| `spectraphon_sam_oscillator.vibe`, `spectraphon_sao_oscillator.vibe` | Dedicated additive oscillator banks: SAM reads live magnitudes with `buf_rd_kr` plus capture writes, while SAO reads Arrays with bilinear buffer lookup; both output `sine`, `sub`, `odd`, `even`. |
| `spectraphon_side.vibe` | Pure-Rhai helper: allocates buffers, creates `<name>__analyzer` and `<name>__oscillator`, and proxies common methods. |
| `spectraphon_dual.vibe` | Pure-Rhai helper: creates two split sides, maps `analyze_a`/`analyze_b` and `odd_a`/`even_a`/`odd_b`/`even_b`, and implements follow ratios by retuning side B. |

Rack authors call `spectraphon_side("name")` or `spectraphon_dual("name")` and
then configure/route them with prefixed helpers such as
`spectraphon::spectraphon_set_param(...)`,
`spectraphon::spectraphon_input(...)`, and
`spectraphon::spectraphon_output(...)` from an aliased import.
The old `voice("name").synth("spectraphon_side")` shape is intentionally not
used, because that would require registering another monolithic synthdef under
the legacy name. The helpers return Rhai object maps with concrete child voice
handles exposed for APIs that cannot target a composite map, such as scheduled
fades and CV-to-param routing.

Known runtime gaps surfaced by the pure-Rhai approach:

| Gap | Current handling |
|---|---|
| Composite target for `to_param` / scheduled fades | Use `helper.oscillator`, `helper.analyzer`, or the deterministic child voice names. |
| Dual audio-rate cross-FM | Accepted params are stored for source compatibility, but true FM needs a scoped oscillator input or runtime helper. |
| Dual hard sync | Deferred; current helper does not alter oscillator phase. |

---

## 6. Implementation plan (epic breakdown)

Proposed epic: **`epic-spectraphon-synthdef`** (~30 SP across six stories).

### Story A — Core spectral oscillator (single side, SAO-only)
**Goal:** A single-side synthdef that plays back a static stored spectrum as a harmonic bank with Odd/Even splits.
* **A1** (2 SP) — Define `spectraphon_side_v0.vibe` with parameters: `freq`, `partials`, `slide`, `focus`, harmonic count `N=64`. Hardcode a default `mag[1..N] = 1/k` saw-like spectrum.
* **A2** (3 SP) — Build the additive bank using two `klang_ar` instances, one for odd k, one for even k. Drive with f0 = freq.
* **A3** (2 SP) — Implement Partials as a per-partial tilt mask × global VCA. Verify CCW = silence, CW = full spectrum.
* **A4** (2 SP) — Add Sine output and Sub/CV-as-saw output. Implement Even-into-Odd normalization via a `merge_even` synthdef parameter.
* **A5** (1 SP) — KB doc + hello-world example: `examples/spectraphon_drone.vibe`.

### Story B — Slide / Focus / SAM analyzer
**Goal:** Add the SAM mode with a real-time harmonic analyzer.
* **B1** (3 SP) — Build a `harmonic_extract_kr` UGen wrapper around `fft_kr`: takes a buffer and `f0_analyze`, outputs `mag[1..N]` averaged over Focus-bandwidth bins.
* **B2** (2 SP) — Wire SAM mode: input → FFT → `harmonic_extract_kr` → drive klang amplitude arrays. Slide controls `f0_analyze`; Focus controls bandwidth.
* **B3** (2 SP) — Implement the envelope-follower Sub/CV variant for SAM mode (use `Amplitude`).
* **B4** (1 SP) — Mode parameter `mode = sam | sao` switches the amplitude source.
* **B5** (1 SP) — Smoothing on `mag[k]` updates (one-pole at ~50 Hz) to avoid zipper.

### Story C — Array memory + capture/recall
**Goal:** Up to 16 stored Arrays per side with Slide/Focus 2-D addressing.
* **C1** (3 SP) — Allocate per-side buffer of shape `[16 slots × Y_grid × X_grid × N_partials]`. Provide capture-trigger parameter that streams `mag[1..N]` into the buffer at the current Slide/Focus cell.
* **C2** (3 SP) — Implement 2-D buffer lookup (bilinear) in SAO mode; drive klang amps from interpolated cell.
* **C3** (2 SP) — Array selection parameter `array_idx ∈ [0, 15]`. Hot-reload-safe: arrays survive synthdef recompile (use a top-level shared buffer outside the per-voice scope).
* **C4** (1 SP) — Default-array preset loader: bake a saw-like and a square-like default into Array 0/1.

### Story D — Dual-channel cross-modulation
**Goal:** Wire two Spectraphon sides with FM bus, Follow, Sync.
* **D1** (2 SP) — `spectraphon_dual.vibe` synthdef instantiating two sides (A, B). Independent params per side.
* **D2** (2 SP) — FM bus: route A's Sine into B's f0 (scaled by `b_fm_index`) and vice versa.
* **D3** (2 SP) — Follow mode: when set, B's f0 = A's f0 * 2^(b_voct_offset).
* **D4** (3 SP) — Sync mode: hard-sync B's klang phase to A's f0 zero-crossings. May require extending klang or building a custom phase-reset bank from individual `sin_osc_ar` voices.
* **D5** (1 SP) — Tuning-beacon meta-output (debug only).

### Story E — Spectranoise modes (Chaos, Noise)
**Goal:** Replicate the 2023 firmware's two extra per-side modes.
* **E1** (2 SP) — Chaos mode: paired sine + Focus-ratio sine + chaos-feedback (`fb_sine_l_ar` or feedback loop on phase).
* **E2** (2 SP) — Noise mode: noise → LPF (Slide) → HPF (Focus) → AM with sine carriers, split into Odd/Even.
* **E3** (1 SP) — Mode parameter extends to `sam | sao | chaos | noise`; verify mode-switching is glitch-free.

### Story F — Examples + KB documentation
**Goal:** User-facing guidance.
* **F1** (1 SP) — `examples/spectraphon_chord.vibe` — dual-side patch with FM and Follow.
* **F2** (1 SP) — `examples/spectraphon_array_capture.vibe` — demonstrates capturing an Array from a sample.
* **F3** (2 SP) — `kb/spectraphon-howto.md` — user-facing manual for the synthdef.
* **F4** (1 SP) — Audio reference renders (so reviewers have a sonic target).

**Total:** ~46 SP. Recommend sequencing A → B → C → D → E → F. Story A alone (~10 SP) gives a usable single-side spectral oscillator and is the highest-leverage starting point.

---

## 7. Risks & open questions

| Risk / open question | Impact | Mitigation / substitute |
|---|---|---|
| **Tom Erbe's exact partial-tracker is proprietary.** SAM analyzer may use sinusoidal partial tracking (peak-pick + matching) which is more robust than harmonic-bin summation but vastly more complex. | Subtle — affects how cleanly noisy/inharmonic input becomes harmonic. | Start with FFT + harmonic-bin summation (Story B1). If results are too noisy, swap in a `pv_partial_synth_f_kr` pre-stage to discard bins below a threshold. |
| **Partial count N is undocumented.** | Affects CPU and timbre. | Default N=64; expose as parameter; benchmark. |
| **Array storage format is undocumented.** Hardware uses microSD; in vibelang we're choosing a 2-D buffer layout that may not match Spectraphon's actual recall behavior (could be linear-time-indexed instead of 2-D-cell-indexed). | High — affects how Slide/Focus *feel* in SAO. | Prototype both: (a) 2-D Slide-X-Focus-Y bilinear, (b) linear time index with Slide=position and Focus=playback-rate. Listen test against a hardware demo video. |
| **Sync mode** requires phase-reset of klang, which the standard `klang_ar` UGen doesn't expose. | Medium — Sync is a popular mode. | Either (a) build a custom UGen, (b) decompose klang into N parallel `sin_osc_ar` instances we can phase-reset individually (CPU cost but flexible), or (c) skip Sync in v1. |
| **Tuning Beacon ratio detection** is auxiliary but visually expressive. | Low. | Implement as a meta-output once core works, or skip. |
| **Spectranoise firmware behavior** (Chaos/Noise) is described in interviews but not in the manual. | Low — Story E only. | Treat as best-effort; the descriptions in §3/4 above are reasonable approximations. |
| **DC-coupling and CV scaling differences** between hardware and vibelang. | Low — affects external CV interop only. | Document expected ranges in `kb/spectraphon-howto.md`. |
| **Real-time analyzer latency.** Hardware feels instantaneous; FFT at hop=512/sr=48k is ~10 ms, audible as a slight lag in modulation response. | Medium. | Use small FFT size (512–1024) with 50% overlap; smooth amplitude updates to mask boundary jitter. |

---

## Sources

* Make Noise Spectraphon product page — https://www.makenoisemusic.com/modules/spectraphon
* Spectraphon manual (PDF, hosted by Make Noise) — https://www.makenoisemusic.com/wp-content/uploads/2024/03/spectraphon-manual.pdf
* Spectraphon Cheat Sheet (PDF) — https://www.makenoisemusic.com/wp-content/uploads/2024/03/Spectraphon-Cheat-Sheet.pdf
* ManualsLib mirror (page 27, Selecting Arrays) — https://www.manualslib.com/manual/3085350/Make-Noise-Soundhack-Spectraphon.html
* Make Noise manuals tips page — https://makenoise-manuals.com/spectraphon/spectraphon-manual-tips.html
* Synthtopia intro coverage — https://www.synthtopia.com/content/2023/05/11/make-noise-soundhack-intro-spectraphon-dual-spectral-oscillator/
* Synthanatomy — Spectranoise firmware coverage — https://synthanatomy.com/2023/12/spectranoise-make-noise-with-soundhack-new-firmware-for-the-spectraphon.html
* vibelang UGen manifests — `crates/vibelang-dsp/ugen_manifests/{pv_spectral,sc3_josh_spectral,oscillators,granular,mi_ugens,fft,buffers,chaos,noise}.json`
