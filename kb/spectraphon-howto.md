# Spectraphon — How-To

User manual for the `spectraphon_side` and `spectraphon_dual` synthdefs in
`vibelang-std`.

Source material for this pass:

* Make Noise Spectraphon product page — https://www.makenoisemusic.com/modules/spectraphon/
* Make Noise Spectraphon manual PDF — https://www.makenoisemusic.com/wp-content/uploads/2024/03/spectraphon-manual.pdf
* Local implementation plan — `kb/spectraphon-synthdef-plan.md`

The ReSynthesizer research tickets were still in progress when this was
updated, so the implementation keeps to the official Make Noise manual/product
claims and documents approximation boundaries directly here.

## Mental Model

The hardware Spectraphon is a dual spectral oscillator. Each side either:

* analyzes incoming audio in SAM mode and uses that spectrum to drive a
  harmonic oscillator bank, or
* plays stored spectra in SAO mode from Arrays captured during SAM.

VibeLang approximates that with additive banks of 64 sine partials split into
Odd and Even outputs. The output is newly generated oscillator audio; it is
not the analyzed input passed through an FFT resynthesizer.

## `spectraphon_side`

`spectraphon_side` is the full single-side surface. It exposes four mono
audio-rate output ports:

| Port | Meaning |
|---|---|
| `sine` | Clean oscillator fundamental. Always active. |
| `sub` | SAO: sub-octave saw. SAM: input envelope follower. Silent in Chaos/Noise. |
| `odd` | Odd partial bank, or Chaos/Noise first voice. |
| `even` | Even partial bank, or Chaos/Noise second voice. |

Named inputs:

| Input | Width | Meaning |
|---|---:|---|
| `analyze` | 1 | SAM analyzer source. Unpatched inputs are silent. |

Parameters:

| Param | Default | Meaning |
|---|---:|---|
| `freq` | 220.0 | Oscillator fundamental in Hz. Partial `k` runs at `k * freq`. |
| `partials` | 1.0 | Progressive harmonic reveal. In Chaos it is AM depth; in Noise it is unused. |
| `slide` | 0.5 | SAM analyzer f0, SAO Array X, Chaos feedback, or Noise LP cutoff. |
| `focus` | 0.5 | SAO Array Y, Chaos second-sine ratio, or Noise HP cutoff. |
| `amp` | 0.3 | Post-envelope output gain. |
| `gate` | 1.0 | ASR lifecycle gate. |
| `sam_capture` | 1.0 | Mode selector: `0` SAO, `1` SAM, `2` Chaos, `3` Noise, `4` SAM+capture. |
| `array_idx` | 0.0 | Array slot `0..15` for SAO read or SAM capture write. |
| `bufnum` | 0.0 | Optional script-allocated 65,536-frame Array buffer. |

SAO can run without a wired Array buffer: when `bufnum` is left at `0`, the
synthdef falls back to a built-in `1/k` saw-like spectrum. Capture is disabled
until a script buffer is wired.

For persistent Arrays:

```vibe
import "stdlib/instruments/spectral/spectraphon_side.vibe";

let arr = allocate_buffer("spec_arrays", 65536, 1);

let v = voice("spec")
    .synth("spectraphon_side")
    .set_param("bufnum", arr.bufnum)
    .set_param("sam_capture", 0.0);
```

Route per port when you want hardware-style patching:

```vibe
v.output("sine").to_main();
v.output("sub").mute();
v.output("odd").to(group("dry"));
v.output("even").to(group("wash"));
```

Patch an analyzer source before using SAM modes:

```vibe
v.input("analyze").from(source_voice);
```

## `spectraphon_dual`

`spectraphon_dual` focuses on the two-side oscillator relationship: Follow
ratios and the internal FM bus. It omits Array capture/playback and the
Spectranoise modes to keep the generated synthdef practical.

Output ports:

| Port | Meaning |
|---|---|
| `odd_a` | Side A odd partial bank. |
| `even_a` | Side A even partial bank. |
| `odd_b` | Side B odd partial bank. |
| `even_b` | Side B even partial bank. |

Named inputs:

| Input | Width | Meaning |
|---|---:|---|
| `analyze_a` | 1 | Side A SAM analyzer source. Unpatched inputs are silent. |
| `analyze_b` | 1 | Side B SAM analyzer source. Unpatched inputs are silent. |

The default VibeLang route for more than two ports sends only the first two
ports to the voice group. Route all four explicitly when you want both sides:

```vibe
let v = voice("dual").synth("spectraphon_dual");
v.outputs(["odd_a", "even_a", "odd_b", "even_b"]).to_current_group();
v.input("analyze_a").from(source_a);
v.input("analyze_b").from(source_b);
```

Parameters:

| Param | Default | Meaning |
|---|---:|---|
| `freq_a` | 220.0 | Side A pitch. |
| `freq_b` | 220.0 | Side B pitch when `routing_mode = 0`. |
| `amp` | 0.3 | Shared output gain. |
| `gate` | 1.0 | ASR lifecycle gate. |
| `a_fm_index` | 0.0 | A sine into B FM depth, scaled by B pitch. |
| `b_fm_index` | 0.0 | B sine into A FM depth, scaled by A pitch. |
| `routing_mode` | 0.0 | `0` independent, `1` unison, `2` +7 ST, `3` +12 ST, `4` +19 ST. |
| `partials_a`, `partials_b` | 1.0 | Per-side harmonic reveal. |
| `slide_a`, `slide_b` | 0.5 | Per-side SAM analyzer f0 and odd/even tilt in SAO. |
| `focus_a`, `focus_b` | 0.5 | Per-side reveal-density multiplier. |
| `mode_a`, `mode_b` | 0.0 | `0` SAO built-in saw spectrum, `1` SAM from the named analyzer inputs. |
| `array_idx_a`, `array_idx_b` | 0.0 | Reserved; accepted for future Array-backed dual mode. |

## Approximation Caveats

* SAM analysis reads named input buses. Legacy scripts that relied on implicit
  hardware input 0 must now patch the analyzer input explicitly.
* SAM uses one FFT magnitude bin per partial. The hardware Focus behavior is
  broader and more interactive; this VibeLang version keeps Focus available
  for Array addressing and mode-specific timbre controls.
* SAO Arrays use a script-allocated buffer in `spectraphon_side`; `spectraphon_dual`
  currently uses a built-in saw-like `1/k` spectrum.
* Sync mode is not implemented in `spectraphon_dual`.
* Chaos/Noise are practical Spectranoise approximations, not Tom Erbe's exact
  firmware algorithms.

## Examples

* `examples/spectraphon_drone.vibe`
* `examples/spectraphon_multiout.vibe`
* `examples/spectraphon_chord.vibe`
* `examples/spectraphon_array_capture.vibe`
