# Stdlib CV Spec

The contract every CV (control voltage) synthdef in
`crates/vibelang-std/stdlib/cv/` must follow.

CV synthdefs are normal vibelang voices whose output is intended to drive an
external DC-coupled audio interface (e.g. Expert Sleepers ES-3) so that the
sample-domain signal becomes a hardware control voltage at a Eurorack jack.

## 1. Voltage convention

All CV synthdefs assume a calibrated DC-coupled DAC where audio sample
amplitude maps linearly to jack voltage. The reference scaling is:

| Quantity              | Audio value | Jack voltage   |
|-----------------------|-------------|----------------|
| Full positive scale   | `+1.0`      | `+10 V`        |
| Full negative scale   | `-1.0`      | `-10 V`        |
| Zero / V/oct origin   | `0.0`       | `0 V` = MIDI C4 (note 60) |
| 1V/oct coefficient    | `0.083333`  | `1 V` per octave |
| Trigger pulse         | `0.5`       | `+5 V`, 5 ms wide |
| Sustained gate        | `0.5`       | `+5 V`, held while gated |

`1V/oct = 0.083333` is the per-octave audio coefficient used when generating
pitch CVs from MIDI / Hz. The 0V reference sits at MIDI 60 (C4); each
additional octave adds `0.083333` to the audio sample, each semitone adds
`0.083333 / 12 ≈ 0.006944`.

Calibration is verified at the jack with the `cv_test_swept_dc` and
`cv_voct_calib` synthdefs (see §5). If your interface's voltage scale differs
(some DC-coupled DACs reach ±5V or ±12V instead of ±10V), use
`cv_test_swept_dc` to measure the actual peak voltage and apply a global gain
trim at the routing stage rather than re-scaling each synthdef.

## 2. The contract

### 2.1 Header metadata

The first line of every CV synthdef file is a single-line metadata comment:

```
// kind=cv | category=<…> | flavor=<…> | range=<vrange> | params=<…>
```

Fields (pipe-separated `key=value`):

| key        | meaning                                                         |
|------------|-----------------------------------------------------------------|
| `kind`     | always `cv` for stdlib CV synthdefs                             |
| `category` | top-level group: `calibration`, `pitch`, `gate`, `trigger`, `envelope`, `lfo`, `clock`, `random`, `sequencer` |
| `flavor`   | the specific algorithm variant — `swept_dc`, `voct_calib`, `v5_steady`, `ar_envelope`, ... |
| `range`    | output voltage range as `low..high V`, e.g. `-10..+10 V`, `0..+5 V`, `0..+10 V` |
| `params`   | comma-separated list of public params, in declaration order     |

Tooling (LSP, completion, future CV index) reads this header. Keep it on
line 1, no leading blank lines, and keep the field set exact.

A free-form description block follows the header. Document the intended jack
behaviour and any rate / latency assumptions there.

### 2.2 Mono, single channel

A CV synthdef returns a single audio channel. The body must return
`[signal]` (a one-element array of NodeRef), **not** a bare `signal` and
**not** `[signal, signal]`.

```rhai
.body(|...| {
    // ... compute cv ...
    [cv]
})
```

Returning a bare NodeRef triggers automatic Pan2 wrapping
(`crates/vibelang-dsp/src/builder.rs:561`), which scales the signal by ~0.707
across two channels — fatal for CV. Returning `[L, R]` writes two output
channels and clobbers the next jack.

The wrapping `[…]` array form bypasses Pan2 and writes one channel at full
amplitude to the synthdef's `out` bus. The downstream group routing (§4)
decides which hardware channel that bus lands on.

### 2.3 Hard clip to ±1.0

The final returned signal is hard-clipped to ±1.0:

```rhai
let clipped = clip_ar(cv, -1.0, 1.0);
[clipped]
```

This protects the DAC and the downstream Eurorack module from overvoltage
events caused by modulation, sum, or feedback. Clip is the **last** stage
before returning.

### 2.4 Forbidden

CV chains MUST NOT use:

| UGen / param      | Why                                                                  |
|-------------------|----------------------------------------------------------------------|
| `leak_dc_ar`      | Removes the DC content that **is** the signal — destroys offsets.    |
| `hpf_ar` / `hpf_kr` | Same — high-pass filtering kills the DC component of pitch / gate. |
| `softclip_ar` / `tanh_ar` | Asymmetric saturation introduces DC offset and bends the V/oct linearity. Use `clip_ar` (hard clip) only. |
| `mix` parameter   | Dry/wet blending makes no sense for a CV; the signal is the signal. Use a static gain trim downstream if you need attenuation. |

These appear in audio effect contracts (`stdlib-effects-spec.md` §3, §5) for
good reason — they are equally forbidden here for the opposite reason.

### 2.5 No envelope on the carrier

CV synthdefs typically do **not** wrap their output in an amplitude envelope
that fades the DC away. If the synthdef needs to be gated (start / stop on
note events), use a `gate` parameter that drives the CV directly (e.g. a
gate output stays at `0.5` while `gate >= 1`, drops to `0.0` otherwise) —
not a separate `env_gen_ar` multiplied across the signal.

The exceptions are envelope synthdefs (`cv/envelope/*.vibe`) where the
envelope **is** the output.

### 2.6 Self-contained

Like effects, CV bodies only read from declared params and never touch
specific bus IDs. The hardware routing happens in user-space via the
`.output([…])` group call (§4).

## 3. Param order in `.body(...)`

Standard order: signal-shape params first, then `gate` if applicable.
Conventional names:

| param     | meaning                                                           |
|-----------|-------------------------------------------------------------------|
| `freq`    | rate for LFOs / clocks (Hz)                                       |
| `voltage` | static DC level, in audio units (e.g. `0.5` for `+5V`)            |
| `amp`     | scalar multiplier on the output, default `1.0`                    |
| `gate`    | `1.0` = active, `0.0` = silent. Default `1.0` — a CV that's never gated still outputs continuously |

There is no `mix` and no `level` — see §2.4. If the user needs to attenuate
the CV, they apply a group-level gain or a downstream attenuverter.

## 4. Channel routing — sending CV to ES-3 channel N

The Expert Sleepers ES-3 (or any DC-coupled audio interface) presents its
hardware outputs as JACK output channels 1..8 (vibelang bus indices `0..7`).
Vibelang routes voices via groups; the `.output([L, R])` builder pins a
group to a stereo pair of hardware buses
(`crates/vibelang-rhai/src/api/group.rs:155`).

To send a CV synthdef to ES-3 jack N (1-indexed), put the voice in a group
that's been routed to bus pair `[N-1, N]`:

```rhai
import "stdlib/cv/calibration/cv_v5_steady.vibe";

// Route group 'cv1' to ES-3 jack 1 (JACK bus 0 = the 'L' of the [0, 1] pair).
group("cv1").output([0, 1]);

// CV synthdef writes one channel to bus 0 (the L of [0, 1]).
// Bus 1 (the R) gets silence — wasted but reserved.
voice("gate")
    .synth("cv_v5_steady")
    .group("cv1")
    .start();
```

To address ES-3 jack 3, use `group("cv3").output([2, 3])`. The `R` bus of
each pair is wasted with the current routing; future routing work
(`kb/voice-multi-output-cv-routing-plan.md`) will let one synthdef target a
single mono bus directly.

`scsynth` must be started with enough output channels — pass `--output-channels 8`
to vibelang for an 8-jack ES-3 setup.

## 5. Calibration synthdefs

Three calibration helpers ship in `crates/vibelang-std/stdlib/cv/calibration/`:

| synthdef            | output                                              | use                                              |
|---------------------|-----------------------------------------------------|--------------------------------------------------|
| `cv_test_swept_dc`  | Slow saw ramp `-1.0 → +1.0` over 10s, repeats       | Verify the jack reaches ±10V at the rails        |
| `cv_voct_calib`     | Stepped: 0V (3s) → +1V (3s) → +2V (3s) → +3V (3s)   | Calibrate quantizers; confirm 1V/oct response on a downstream VCO |
| `cv_v5_steady`      | Constant `+0.5` (= +5V)                             | Gate-high reference; verify the +5V level used by `gate` and `trigger` signals |

See `examples/cv_calibration.vibe` for a runnable patch that routes
`cv_test_swept_dc` to ES-3 jack 1.

## Naming

CV synthdefs follow `cv_<category>_<flavor>` form when the category is
needed for disambiguation, or just `cv_<flavor>` for the calibration
helpers and other obvious cases:

- `cv_test_swept_dc`, `cv_voct_calib`, `cv_v5_steady` (calibration)
- `cv_env_ar`, `cv_env_adsr` (envelopes)
- `cv_lfo_sine`, `cv_lfo_random` (LFOs)
- `cv_clock_div`, `cv_trig_burst` (clock / trigger)

## Reference implementation

[`crates/vibelang-std/stdlib/cv/calibration/cv_v5_steady.vibe`](../crates/vibelang-std/stdlib/cv/calibration/cv_v5_steady.vibe)
is the canonical minimal example: one constant, one clip, single-channel
return. Copy its skeleton when adding new CV synthdefs.
