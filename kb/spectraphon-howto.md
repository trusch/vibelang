# Spectraphon How-To

`stdlib/instruments/spectral/spectraphon.vibe` defines one public synthdef named
`spectraphon`. Instantiate it with the normal voice API, patch SAM audio into
`.input("analyze")`, route any of its four outputs, and keep the voice running
as a sustained sound source.

```vibe
import "stdlib/instruments/spectral/spectraphon.vibe";

let arr = allocate_buffer("spec_arrays", 65536, 1);

let spec = voice("spec")
    .synth("spectraphon")
    .set_param("freq", 110.0)
    .set_param("partials", 0.85)
    .set_param("slide", 0.55)
    .set_param("focus", 0.45)
    .set_param("amp", 0.25)
    .set_param("mode", 2.0)
    .set_param("array_buf", arr.bufnum);

spec.output("odd").to_current_group();
spec.output("even").to_current_group();
spec.output("sine").mute();
spec.output("sub").mute();
spec.run();
```

## Modes

| Mode | Meaning | Typical patch |
|---:|---|---|
| `0.0` | SAM live | Analyze `analyze`, then resynthesize the live magnitude bank. |
| `1.0` | SAM capture | Analyze `analyze`, resynthesize it, and write the magnitudes into `array_buf`. |
| `2.0` | SAO playback | Read `array_buf` with `slide`/`focus` interpolation. |

## Inputs, Outputs, And Params

| Surface | Names |
|---|---|
| Input | `analyze` (mono audio) |
| Outputs | `sine`, `sub`, `odd`, `even` |
| Core params | `freq`, `v_oct`, `partials`, `slide`, `focus`, `amp`, `gate`, `mode`, `array_idx` |
| Buffer params | `fft_buf`, `mag_buf`, `array_buf` |
| Analyzer params | `f0_analyze`, `active` |

For SAM and SAM-capture patches, `fft_buf` and `mag_buf` are optional but useful
script-owned buffers. `fft_buf` keeps FFT-chain memory explicit, and `mag_buf`
keeps the live magnitude bank stable across hot reload. `array_buf` stores SAO
Array data.

`freq` is the base tuning in Hz; `v_oct` is a 1V/oct exponential
transposition input (default 0 = no shift). Route a v/oct CV source
(e.g. `rene.cv`, which is `(midi - 60) / 12` clipped to ±5 V) directly
into `v_oct` instead of linearly scaling it into `freq`:

```vibe
spec.set_param("freq", 261.63); // C4 base
rene.output("cv").to_param(spec, "v_oct");
```

```vibe
import "stdlib/utility/test_tones.vibe";

let fft = allocate_buffer("spec_fft", 2048, 1);
let mag = allocate_buffer("spec_mag", 64, 1);
let arr = allocate_buffer("spec_arrays", 65536, 1);

let src = voice("source").synth("test_saw").set_param("freq", 110.0);
let spec = voice("spec")
    .synth("spectraphon")
    .set_param("mode", 1.0)
    .set_param("freq", 110.0)
    .set_param("f0_analyze", 110.0)
    .set_param("fft_buf", fft.bufnum)
    .set_param("mag_buf", mag.bufnum)
    .set_param("array_buf", arr.bufnum);

spec.input("analyze").from(src, "out");
spec.output("odd").to_current_group();
spec.output("even").to_current_group();
src.output("out").mute();
src.run();
spec.run();
```

For dual-side patches, instantiate two `spectraphon` voices and patch/tune each
side explicitly:

```vibe
let freq_a = 110.0;
let freq_b = freq_a * 1.4983;
let arr_a = allocate_buffer("spec_a_arrays", 65536, 1);
let fft_b = allocate_buffer("spec_b_fft", 2048, 1);
let mag_b = allocate_buffer("spec_b_mag", 64, 1);

let a = voice("spec_a").synth("spectraphon")
    .set_param("freq", freq_a)
    .set_param("mode", 2.0)
    .set_param("array_buf", arr_a.bufnum);

let b = voice("spec_b").synth("spectraphon")
    .set_param("freq", freq_b)
    .set_param("mode", 0.0)
    .set_param("fft_buf", fft_b.bufnum)
    .set_param("mag_buf", mag_b.bufnum);

b.input("analyze").from(a, "sine");
a.output("odd").to_current_group();
b.output("even").to_current_group();
a.run();
b.run();
```

## Caveats

SAM analysis uses FFT/BinData per harmonic and the oscillator generates a new
additive-bank signal; the analyzed input is not passed through. Follow, Sync,
audio-rate cross-FM, exact hardware Array formatting, and Spectranoise modes are
not modeled by this synthdef. Use CV routes or direct params to drive sustained
Spectraphon voices; do not layer `melody(...).on(spec)` over a running
Spectraphon voice.

See `examples/spectraphon_drone.vibe`, `examples/spectraphon_multiout.vibe`,
`examples/spectraphon_chord.vibe`, `examples/spectraphon_array_capture.vibe`,
and `examples/resynthesizer/main.vibe` for complete patches.
