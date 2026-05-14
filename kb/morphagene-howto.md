# Morphagene — How-To

User manual for the `morphagene` synthdef family in `vibelang-std`.

Source material for this pass:

* Make Noise Morphagene product page — https://www.makenoisemusic.com/modules/morphagene/
* Make Noise Morphagene manual PDF — https://www.makenoisemusic.com/wp-content/uploads/2024/03/morphagene-manual.pdf
* Local implementation plan — `kb/morphagene-synthdef-plan.md`

The ReSynthesizer research tickets were still in progress when this was
updated, so the implementation keeps to the official Make Noise manual/product
claims and documents approximation boundaries directly here.

## Mental Model

Morphagene is a tape-and-microsound instrument. The hardware organizes sound as:

* **Reel** — the active stereo buffer.
* **Splice** — a region inside the Reel.
* **Gene** — a grain inside the active Splice.

The VibeLang `morphagene` synthdef implements a practical playback/recording
core: multi-splice selection, Vari-Speed, Sound-On-Sound recording,
Gene-Size/Morph granular playback, clocked Gene Shift/Time Stretch behavior,
and a named EOSG CV output.

## Outputs

| Port | Rate | Meaning |
|---|---|---|
| `left` | ar | Left audio output. |
| `right` | ar | Right audio output. |
| `eosg` | kr | 10 V-style End-of-Splice/Gene pulse. |

With no explicit routes, VibeLang routes the first two audio ports to the
voice group and leaves the `eosg` CV port muted. Route `eosg` to a param when
you need patch-style control:

```vibe
let m = voice("morph").synth("morphagene");
let target = voice("target").synth("some_voice");
m.output("eosg").to_param(target, "gate");
```

## Parameters

| Param | Default | Meaning |
|---|---:|---|
| `bufnum` | 0.0 | Active Reel buffer. Use `sample(...).bufnum` or `reel(...).bufnum`. |
| `num_splices` | 1.0 | Number of evenly divided Splices in the Reel. |
| `organize` | 0.0 | Selects the pending Splice. Audible changes commit at EOS. |
| `sos` | 0.0 | Sound-On-Sound amount. Values above `0.001` also enable recording. |
| `clk` | 0.0 | Control-rate clock input for Gene Shift / Time Stretch. |
| `vari_speed` | 0.0 | Bipolar speed/direction. `+1` is +12 ST, `-1` is about -26 ST. |
| `slide` | 0.0 | Start offset inside the active Splice. |
| `gene_size` | 0.0 | Grain duration. `0` uses whole-splice playback. |
| `morph` | 0.0 | Grain overlap, stereo pan spread, pitch scatter, and clock-mode selector. |
| `amp` | 0.5 | Audio output gain. |
| `play_gate` | 1.0 | Click-smoothed audible gate; synth instance stays alive. |
| `gate` | 1.0 | ASR lifecycle gate. |

## Loading A Reel

For file-backed playback, pass the sample buffer directly into `bufnum`.
Do not use `voice.on(sample)`; that path targets generic sample synthdefs and
adds params Morphagene does not use.

```vibe
import "stdlib/instruments/sampler/morphagene.vibe";

let reel = sample("reel", "samples/loop.wav").loop_mode(true);

voice("morph")
    .synth("morphagene")
    .set_param("bufnum", reel.bufnum)
    .set_param("vari_speed", 0.7937);
```

For hot-reload-safe recording or generated Reels, use the helper wrapper around
`allocate_buffer`:

```vibe
let r = reel("blank_reel", 480000, 2);  // 10 seconds at 48 kHz

let v = voice("morph")
    .synth("morphagene")
    .set_param("sos", 0.4);

reel_attach(v, r);
```

`reel_fill_preset(reel, preset)` can seed a Reel with a generated waveform:

| Preset | Source |
|---:|---|
| `0` | Saw |
| `1` | Square |
| `2` | Sine |
| `3` | Pink noise |
| `4+` | Silence |

Guard preset fills during normal work; they rerun on every script reload.

## Vari-Speed

The synthdef uses the Morphagene-style asymmetric range:

| `vari_speed` | Playback behavior |
|---:|---|
| `+1.0` | Forward, about +12 ST. |
| `+0.7937` | Unity forward playback. |
| `0.0` | Halt. |
| `-1.0` | Reverse, about -26 ST. |

The mapping is cubic around center so slow warble and near-halt gestures have
usable resolution.

## Splices And Genes

`num_splices` divides the Reel into equal regions. `organize` selects the
pending region, but playback switches at EOS so splice changes do not click in
the middle of a Splice.

`gene_size` crossfades from whole-splice playback into `grain_buf_ar` granular
playback. `morph` follows a practical staircase:

* low values leave gaps between Genes,
* mid values increase overlap and add stereo pan scatter,
* high values add per-grain pitch scatter.

Clocked behavior follows the hardware control idea:

* with `clk` patched, `gene_size > 0.02`, and `morph < 0.46`, the clock advances
  Gene Shift positions,
* with `clk` patched, `gene_size > 0.02`, and `morph >= 0.46`, the clock drives
  Time Stretch positions while Vari-Speed behaves more like pitch.

## Sound-On-Sound

`sos > 0.001` enables the internal recorder. The recorder writes forward at
unit rate with `recLevel = 1 - sos` and `preLevel = sos`, which gives practical
overdub/decay behavior. A true separate hardware REC gate is not exposed yet.

## Approximation Caveats

* Splices are evenly divided. `.wav` cue chunks and arbitrary marker positions
  are not imported yet.
* Whole-splice playback reads the first Reel channel and duplicates it; the
  granular branch uses true stereo `grain_buf_ar`.
* Clock-rate validation is not enforced in DSP. Very slow clocks stutter; very
  fast clocks can overrun reads.
* SOS recording is inferred from `sos > 0.001`; hardware-style REC-with-SOS-at-0
  overwrite behavior needs a future explicit record gate.
* Reel persistence is script-level via `allocate_buffer`, not SD-card `.wav`
  persistence.

## Examples

* `examples/morphagene_loop.vibe`
* `examples/morphagene_splices.vibe`
* `examples/morphagene_granular.vibe`
* `examples/morphagene_gene_shift.vibe`
* `examples/morphagene_time_stretch.vibe`
* `examples/morphagene_reel_persist.vibe`
