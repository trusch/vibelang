# Spectraphon — How-To

User manual for the split Spectraphon helpers in `vibelang-std`.

Source material for this pass:

* Make Noise Spectraphon product page — https://www.makenoisemusic.com/modules/spectraphon/
* Make Noise Spectraphon manual PDF — https://www.makenoisemusic.com/wp-content/uploads/2024/03/spectraphon-manual.pdf
* Local implementation plan — `kb/spectraphon-synthdef-plan.md`

## Mental Model

Spectraphon is now split into cooperating synthdefs instead of one oversized
server graph:

| Layer | File | Role |
|---|---|---|
| Analyzer | `spectraphon_analyzer.vibe` | Reads the SAM input with `fft_kr` from a helper-allocated FFT chain buffer and one `bin_data_kr` per harmonic, then writes 64 magnitudes to a control buffer. |
| SAM oscillator | `spectraphon_sam_oscillator.vibe` | Reads the live magnitude buffer, optionally captures it into the Array buffer, and renders sine/sub/odd/even. |
| SAO oscillator | `spectraphon_sao_oscillator.vibe` | Reads the SAO Array buffer with slide/focus interpolation and renders sine/sub/odd/even. |
| Helper | `spectraphon_side.vibe`, `spectraphon_dual.vibe` | Allocates buffers, creates child voices, and proxies common methods in pure Rhai. |

The split is structural: rack authors should call
`spectraphon_side(name, mode)` or `spectraphon_dual(name, mode_a, mode_b)`,
not `voice(name).synth("spectraphon_side")`. Use `"sam"` or `"sao"` when
the mode is known. Numeric `sam_capture`/`mode_a`/`mode_b` helper calls remain as
setup-time compatibility shims, but scheduled mode morphing is now a voice
swap rather than one synthdef crossfade.

## Helper Surface

`spectraphon_side(name, mode)` returns a Rhai object map:

| Field | Meaning |
|---|---|
| `analyzer` | The concrete `VoiceHandle` named `<name>__analyzer`. |
| `oscillator` | The concrete `VoiceHandle` named `<name>__oscillator`. |
| `mag_buf` | 64-frame live SAM magnitude buffer. |
| `fft_buf` | 2,048-frame SAM FFT chain buffer allocated before the analyzer node starts. |
| `array_buf` | 65,536-frame SAO Array buffer. |

Use the prefixed helper functions for ordinary rack code. They are deliberately
not named `set_param`/`input`/`output`, because those names collide with Rhai's
synthdef builder methods during imports.

```vibe
import "stdlib/instruments/spectral/spectraphon_side.vibe" as spectraphon;

let spec = spectraphon::spectraphon_side("spec", "sam");
spec = spectraphon::spectraphon_set_param(spec, "freq", 110.0);
spec = spectraphon::spectraphon_set_param(spec, "partials", 0.85);
spec = spectraphon::spectraphon_gain(spec, db(-9));
spec = spectraphon::spectraphon_run(spec);

spectraphon::spectraphon_input(spec, "analyze").from(source);
spectraphon::spectraphon_output(spec, "odd").to(group("dry"));
spectraphon::spectraphon_output(spec, "even").to(group("wash"));
```

`spectraphon_set_param(spec, "slide", value)` fans out to the oscillator `slide` parameter and
to analyzer `f0_analyze` using the Spectraphon-style `50..800 Hz` range.
`spectraphon_set_param(spec, "focus", value)` fans out to both the analyzer bin width and the
oscillator Array/Chaos/Noise focus parameter. `spectraphon_set_param(spec, "bufnum", value)` is
accepted as a compatibility alias for the oscillator `array_buf` parameter.

When a VibeLang API requires a concrete voice handle, use the exposed child:

```vibe
melody("hold").on(spec.oscillator).notes("A2").apply();
fade("scan").on_voice("spec__oscillator").param("slide").from(0.0).to(1.0);
fade("analyze_grid").on_voice("spec__analyzer").param("f0_analyze").from(50.0).to(800.0);
```

## Ports And Parameters

Single-side outputs are `sine`, `sub`, `odd`, and `even`. The analyzer input
is `analyze`.

Important parameters:

| Param | Default | Meaning |
|---|---:|---|
| `freq` | 220.0 | Oscillator fundamental in Hz. |
| `partials` | 1.0 | Progressive harmonic reveal. |
| `slide` | 0.5 | Analyzer f0 fanout and SAO Array X. |
| `focus` | 0.5 | Analyzer bin width fanout and SAO Array Y. |
| `amp` | 0.3 | Post-envelope output gain. |
| `gate` | 1.0 | ASR lifecycle gate. |
| constructor mode | required | `"sao"`, `"sam"`, or `"sam_capture"` selects the dedicated oscillator helper creates. |
| `sam_capture` | compatibility | Setup-time numeric compatibility: `0` SAO, `1` SAM, `4` SAM+capture. Do not automate it as a runtime param. Chaos/Noise are deferred to `spectraphon-chaos-noise-dedicated-oscillator`. |
| `array_idx` | 0.0 | SAO/SAM Array slot `0..15`. |
| `bufnum` | helper allocated | Optional override for the SAO Array buffer. |

The helper allocates a persistent Array buffer by default. You can still pass a
named buffer if you want explicit sharing across helpers or examples:

```vibe
let arr = allocate_buffer("spec_arrays", 65536, 1);
spec = spectraphon::spectraphon_set_param(spec, "bufnum", arr.bufnum);
```

SAM helpers also allocate a dedicated 2,048-frame FFT chain buffer and pass it
to `spectraphon_analyzer` as `fft_buf`. Keeping that chain buffer in script
buffer allocation, rather than `LocalBuf` inside the analyzer graph, preserves
the real FFT/BinData analyzer while avoiding FFT-chain memory initialization
during the analyzer `/s_new`.

## Dual Helper

`spectraphon_dual(name)` creates two side helpers and returns a map with
`side_a`, `side_b`, `analyzer_a`, `analyzer_b`, `oscillator_a`, and
`oscillator_b`.

```vibe
import "stdlib/instruments/spectral/spectraphon_dual.vibe" as spectraphon;

let dual = spectraphon::spectraphon_dual("dual", "sao", "sao");
dual = spectraphon::spectraphon_set_param(dual, "freq_a", 110.0);
dual = spectraphon::spectraphon_set_param(dual, "routing_mode", 2.0);
dual = spectraphon::spectraphon_set_param(dual, "partials_a", 0.85);
dual = spectraphon::spectraphon_set_param(dual, "partials_b", 0.85);
dual = spectraphon::spectraphon_run(dual);

spectraphon::spectraphon_outputs_to_current_group(dual, ["odd_a", "even_a", "odd_b", "even_b"]);
spectraphon::spectraphon_input(dual, "analyze_a").from(source);
spectraphon::spectraphon_input(dual, "analyze_b").from(source);
```

The pure-Rhai dual helper supports follow ratios by setting side B's oscillator
frequency from `freq_a`, `freq_b`, and `routing_mode`. It accepts
`a_fm_index`, `b_fm_index`, and sync-style parameters for source compatibility,
but true audio-rate cross-FM and hard sync need a scoped runtime/synthdef
follow-up because the current split side oscillator has no audio-rate FM input.

## Approximation Caveats

* SAM analysis is real FFT/BinData per harmonic, not a BPF or envelope-only
  approximation. It currently uses a 2,048-point FFT chain for the same bin
  spacing as the original real-FFT implementation.
* The oscillator generates new additive-bank audio; the analyzed input is not
  passed through.
* `spectraphon_set_param(...)` can fan out immediate parameter calls, but
  scheduled fades and CV routes still target concrete child voices. Duplicate
  a scheduled route to `name__analyzer` when you need analyzer f0/focus to move
  with oscillator slide/focus.
* SAM and SAO are separate oscillator synthdefs to keep `/s_new` UGen count
  under control. Changing between them after a voice is running should be
  modeled as stopping one helper voice and starting another.
* The dual helper currently models follow intervals with two split sides.
  Audio-rate cross-FM, hard sync, and composite-target CV routing remain
  runtime gaps rather than hidden Rust changes.

## Examples

* `examples/spectraphon_drone.vibe`
* `examples/spectraphon_multiout.vibe`
* `examples/spectraphon_chord.vibe`
* `examples/spectraphon_array_capture.vibe`
