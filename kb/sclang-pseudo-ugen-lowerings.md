# sclang pseudo-UGen lowerings for codegen

This note catalogues class-library UGens that must not be emitted as
server-side UGen names unless noted otherwise. Source inspection was done
against SuperCollider 3.14.1 installed under `/usr/share/SuperCollider`.

`/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Changed.sc` does not
exist on this host; `Changed` is defined in
`/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Filter.sc`.

## Summary

| Name | Status | Lowering target |
| --- | --- | --- |
| `Changed` | sclang pseudo-UGen | `HPZ1` + `abs` + `>` |
| `SoundIn` | sclang pseudo-UGen | `NumOutputBuses.ir + bus` into `In.ar` |
| `PMOsc` | sclang pseudo-UGen | nested `SinOsc` |
| `SelectX` | sclang pseudo-UGen | `Select` + `XFade2` |
| `LinSelectX` | sclang pseudo-UGen | `Select` + `LinXFade2` |
| `Mix` | sclang pseudo helper | `Sum4`, `Sum3`, or `BinaryOpUGen` add tree |
| `Silence` / `Silent` | pseudo helper | `DC` zero, repeated per channel |
| `Splay` | sclang pseudo-UGen | `Pan2` + `Mix` |
| `SplayAz` | sclang pseudo-UGen | `PanAz` + per-output `Mix` |
| `BLowPass4` | sclang pseudo-UGen | two cascaded `SOS` filters |
| `BHiPass4` | sclang pseudo-UGen | two cascaded `SOS` filters |
| `EnvGate` | sclang pseudo helper | `NamedControl` + `EnvGen.kr` |
| `JPverb` | sc3-plugins pseudo wrapper | `JPverbRaw.ar` with stereo input expansion |
| `Greyhole` | sc3-plugins pseudo wrapper | `GreyholeRaw.ar` with stereo input expansion |
| `DynKlang` | sclang pseudo-UGen | `SinOsc` bank + sum |
| `DynKlank` | sclang pseudo-UGen | `Ringz` bank + sum |
| `LinXFade2` | server UGen with sclang `level` wrapper | `LinXFade2(inA, inB, pan) * level` |
| `Klang` | real server UGen, class flattens specs | emit `Klang` with flattened fixed spec |
| `Klank` | real server UGen, class flattens specs | emit `Klank` with flattened fixed spec |
| `WAmp` | real sc3-plugins server UGen | emit `WAmp.kr` if plugin is available |
| `RMS` | real sc3-plugins server UGen wrapper | emit `RMS.ar`/`RMS.kr` if plugin is available |

On this Arch install, `nm -D --defined-only /usr/lib/SuperCollider/plugins/*.so`
exports only loader symbols such as `load`, `api_version`, and `server_type`,
not per-UGen names. `strings` finds exact names for `Klang`, `Klank`,
`LinXFade2`, and `WAmp`; `RMS` has a class wrapper and `RMS.so`, but the exact
name did not appear in `strings`.

## Operator and helper notes

Use existing generated helpers where possible:

- `hpz1_ar` / `hpz1_kr`
- `sin_osc_ar` / `sin_osc_kr`
- `ringz_ar` / `ringz_kr`
- `select_ar` / `select_kr`
- `x_fade2_ar` / `x_fade2_kr`
- `lin_x_fade2_ar` / `lin_x_fade2_kr`, but see the `level` caveat below
- `pan2_ar` / `pan2_kr`, `pan_az_ar` / `pan_az_kr`
- `sum3_ar` / `sum3_kr`, `sum4_ar` / `sum4_kr`
- `sos_ar` / `sos_kr`
- `in_ar`, `num_output_buses_ir`, `dc_ar` / `dc_kr`, `k2_a_ar`, `a2_k_kr`

The current `NodeRef` API has arithmetic and `abs`, `sqrt`, `round`,
`round_to`, `floor`, `min`, `max`, `wrap`, and `fold`, but no comparison
operator helper. `Changed` needs `BinaryOpUGen` special index 9 (`>`), output
1.0 when true and 0.0 when false.

## Changed

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Filter.sc`

Original:

```supercollider
Changed : Filter {
	*kr { arg input, threshold = 0;
		^HPZ1.kr(input).abs > threshold
	}
	*ar { arg input, threshold = 0;
		^HPZ1.ar(input).abs > threshold
	}
}
```

Lowering:

```text
changed_kr(input, threshold = 0):
  gt(abs(hpz1_kr(input)), threshold)

changed_ar(input, threshold = 0):
  gt(abs(hpz1_ar(input)), threshold)
```

Implementation detail:

- `gt(a, b)` is `BinaryOpUGen` with special index 9.
- `abs(x)` is `UnaryOpUGen` with special index 5.
- Output is a trigger-like 1.0/0.0 signal, not a one-sample impulse
  normalizer.
- `HPZ1` is the SC two-point difference filter:
  `0.5 * (x[n] - x[n - 1])`. This means the threshold is compared to half the
  sample-to-sample or control-block value delta.

Rate and edge cases:

- `ar` and `kr` are independent and preserve the requested rate through
  `HPZ1`.
- Default `threshold` is `0`.
- Multichannel expansion is inherited from `HPZ1` and binary op expansion:
  arrays of inputs or thresholds expand element-wise.

## SoundIn

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/SoundIn.sc`

Original:

```supercollider
SoundIn  {
	*ar { arg bus = 0, mul=1.0, add=0.0;
		var chanOffset;
		chanOffset = this.channelOffset;
		if(bus.isArray.not,{
			^In.ar(chanOffset + bus, 1).madd(mul,add)
		});

		if(bus.every({arg item, i;
				(i==0) or: {item == (bus.at(i-1)+1)}
			}),{
			^In.ar(chanOffset + bus.first, bus.size).madd(mul,add)
		},{
			^In.ar(chanOffset + bus).madd(mul,add)
		})
	}

	*channelOffset {
		^NumOutputBuses.ir
	}
}
```

Lowering:

```text
sound_in_ar(bus = 0, mul = 1, add = 0):
  In.ar(NumOutputBuses.ir + bus, channels).madd(mul, add)
```

For a scalar bus, `channels = 1`. For a consecutive array `[n, n+1, ...]`,
emit one `In.ar(NumOutputBuses.ir + n, len(array))`. For a non-consecutive
array, let the bus expression multichannel-expand, equivalent to separate
`In.ar` reads per requested channel.

Rate and edge cases:

- Audio-rate only in sclang.
- The channel offset adapts to the server's configured number of hardware
  output buses.
- Vibelang already hand-lowers this in `crates/vibelang-dsp/src/helpers.rs`.

## PMOsc

Source:
`/usr/share/SuperCollider/SCClassLibrary/backwards_compatibility/PMOsc.sc`

Original:

```supercollider
PMOsc  {
	*ar { arg carfreq,modfreq,pmindex=0.0,modphase=0.0,mul=1.0,add=0.0;
		^SinOsc.ar(carfreq, SinOsc.ar(modfreq, modphase, pmindex),mul,add)
	}

	*kr { arg carfreq,modfreq,pmindex=0.0,modphase=0.0,mul=1.0,add=0.0;
		^SinOsc.kr(carfreq, SinOsc.kr(modfreq, modphase, pmindex),mul,add)
	}
}
```

Lowering:

```text
pm_osc_ar(carfreq, modfreq, pmindex = 0, modphase = 0, mul = 1, add = 0):
  sin_osc_ar(carfreq, sin_osc_ar(modfreq, modphase, pmindex), mul, add)

pm_osc_kr(carfreq, modfreq, pmindex = 0, modphase = 0, mul = 1, add = 0):
  sin_osc_kr(carfreq, sin_osc_kr(modfreq, modphase, pmindex), mul, add)
```

Rate and edge cases:

- The modulator uses the same requested rate as the carrier.
- Multichannel expansion follows nested `SinOsc` expansion.
- This is a backward-compatibility pseudo-UGen, not a server UGen.

## SelectX and LinSelectX

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Osc.sc`

Original:

```supercollider
SelectX {
	*new1 { arg rate, which, array;
		var selector = UGen.methodSelectorForRate(rate);
		^this.crossfadeClass.perform(selector,
			Select.perform(selector, which.round(2), array),
			Select.perform(selector, which.trunc(2) + 1, array),
			(which * 2 - 1).fold2(1)
		);
	}
	*ar { arg which, array, wrap=1;
		^this.new1(\audio, which, array, wrap);
	}
	*kr { arg which, array, wrap=1;
		^this.new1(\control, which, array, wrap);
	}
	*crossfadeClass {
		^XFade2
	}
}

LinSelectX : SelectX {
	*crossfadeClass {
		^LinXFade2
	}
}
```

Lowering:

```text
select_x_RATE(which, array):
  XFade2.RATE(
    Select.RATE(round_to(which, 2), array),
    Select.RATE(trunc_to(which, 2) + 1, array),
    fold2(which * 2 - 1, 1)
  )

lin_select_x_RATE(which, array):
  LinXFade2.RATE(
    Select.RATE(round_to(which, 2), array),
    Select.RATE(trunc_to(which, 2) + 1, array),
    fold2(which * 2 - 1, 1)
  )
```

Rate and edge cases:

- `RATE` is `ar` or `kr`.
- The public `wrap` argument is ignored by the sclang implementation.
- `round_to(which, 2)` is SC's `which.round(2)`, i.e. nearest multiple of 2.
  `trunc_to(which, 2)` is truncation to a multiple of 2. Codegen currently has
  `round_to`; it may need `trunc_to` and `fold2` helpers or direct
  `BinaryOpUGen` wrappers.
- Out-of-range behavior is whatever `Select` and the folded pan produce; do
  not add extra clamping.

## LinXFade2

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Pan.sc`

Original:

```supercollider
LinXFade2 : UGen {
	*ar { arg inA, inB = 0.0, pan = 0.0, level = 1.0;
		^this.multiNew('audio', inA, inB, pan) * level
	}
	*kr { arg inA, inB = 0.0, pan = 0.0, level = 1.0;
		^this.multiNew('control', inA, inB, pan) * level
	}
	checkInputs { ^this.checkNInputs(2) }
}
```

Lowering:

```text
lin_x_fade2_ar(inA, inB = 0, pan = 0, level = 1):
  LinXFade2.ar(inA, inB, pan) * level

lin_x_fade2_kr(inA, inB = 0, pan = 0, level = 1):
  LinXFade2.kr(inA, inB, pan) * level
```

Rate and edge cases:

- `LinXFade2` is a real server UGen name, but the server UGen takes three
  inputs. The fourth sclang argument, `level`, is a separate multiply.
- The current manifest lists `level` as an input to `LinXFade2`; codegen should
  not encode four inputs for the server node.

## Mix

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Mix.sc`

Original core:

```supercollider
Mix {
	*new { arg array;
		var reducedArray = array.asArray.clump(4);
		var mixedArray = reducedArray.collect {|a|
			if (a.size == 4) { Sum4(*a) } {
				if (a.size == 3) { Sum3(*a) } { a.sum }
			}
		};

		if (mixedArray.size < 3) { ^mixedArray.sum };
		if (mixedArray.size == 3) { ^Sum3(*mixedArray) } { ^Mix(mixedArray) }
	}
}
```

Lowering:

```text
mix(array):
  clump inputs in groups of 4
  group size 4 -> Sum4
  group size 3 -> Sum3
  group size 1 or 2 -> ordinary sum
  if reduced size < 3 -> ordinary sum
  if reduced size == 3 -> Sum3
  otherwise recurse
```

Rate and edge cases:

- `Mix.ar(array)` converts a control result with `K2A.ar(result)` and a scalar
  result with `DC.ar(result)`.
- `Mix.kr(array)` warns and converts any audio inputs with `A2K.kr`, then
  converts a scalar result with `DC.kr(result)`.
- `Sum3` and `Sum4` are real server UGens and sort inputs by rate internally in
  sclang. For codegen, a left-associated add tree is semantically fine; `Sum3`
  and `Sum4` are only optimization and fidelity to emitted SC graphs.

## Silence / Silent

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Line.sc`

The SC class is named `Silent`, while the Vibelang manifest currently exposes
`Silence`.

Original:

```supercollider
Silent {
	*ar { arg numChannels = 1;
		var sig = DC.ar(0);
		if (numChannels == 1) {
			^sig
		} {
			^(sig ! numChannels)
		}
	}
}
```

Lowering:

```text
silent_ar(numChannels = 1):
  sig = dc_ar(0)
  if numChannels == 1:
    sig
  else:
    repeat sig numChannels times

silence_kr(numChannels = 1), Vibelang extension:
  same shape, but use dc_kr(0)
```

Rate and edge cases:

- sclang only defines audio-rate `Silent.ar`.
- `Silent` itself is not a server UGen name; `DC` is the emitted server UGen.
- `UGen.replaceZeroesWithSilence` uses `Silent.ar` to replace literal `0.0`
  outputs in output UGens.
- For Vibelang's manifest-level `Silence.kr`, `DC.kr(0)` is the faithful
  control-rate analogue, but it is not an sclang method.

## Splay

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Splay.sc`

Original:

```supercollider
Splay : UGen {
	*new1 { arg rate, spread = 1, level = 1, center = 0.0, levelComp = true ... inArray;
		var n = max(2, inArray.size);
		var n1 = n - 1;
		var positions = ((0 .. n1) * (2 / n1) - 1) * spread + center;
		level = level * LevelComp(levelComp, rate, n);

		^Mix(Pan2.perform(this.methodSelectorForRate(rate), inArray, positions)) * level;
	}
}
```

Lowering:

```text
splay_RATE(inArray, spread = 1, level = 1, center = 0, levelComp = true):
  n = max(2, len(inArray))
  positions[i] = ((i * (2 / (n - 1))) - 1) * spread + center
  compensated_level = level * LevelComp(levelComp, RATE, n)
  pan_pairs = Pan2.RATE(inArray, positions)
  Mix(pan_pairs) * compensated_level
```

`LevelComp`:

```text
true at audio rate   -> n ** -0.5
true at control rate -> n ** -1
false                -> 1
number or UGen x     -> n ** -clip(x, 0, 1)
```

Rate and edge cases:

- `ar` returns stereo audio; `kr` returns stereo control.
- With one input, `n` is still 2, so the only position is computed from the
  two-channel spread grid and level compensation uses 2.
- Multichannel expansion happens through `Pan2` followed by `Mix`.

## SplayAz

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Splay.sc`

Original core:

```supercollider
SplayAz : UGen {
	*ar { arg numChans = 4, inArray, spread = 1, level = 1, width = 2,
			center = 0.0, orientation = 0.5, levelComp = true;
		var n = max(1, inArray.size);
		var normSpread = (n - 1 / n) * spread;
		var pos = if(n == 1) { center } { [ center - normSpread, center + normSpread ].resamp1(n) };
		level = level * LevelComp(levelComp, \audio, n);
		^PanAz.ar(numChans, inArray.asArray, pos, level, width, orientation).flop.collect(Mix(_))
	}
}
```

Lowering:

```text
splay_az_RATE(numChans = 4, inArray, spread = 1, level = 1, width = 2,
              center = 0, orientation = 0.5, levelComp = true):
  n = max(1, len(inArray))
  normSpread = ((n - 1) / n) * spread
  pos = center if n == 1 else linspace(center - normSpread, center + normSpread, n)
  compensated_level = level * LevelComp(levelComp, RATE, n)
  pan_matrix = PanAz.RATE(numChans, inArray, pos, compensated_level, width, orientation)
  output[ch] = Mix(pan_matrix column ch)
```

Rate and edge cases:

- Output channel count is `numChans`.
- For one input, position is exactly `center`.
- The SC source text writes `n - 1 / n`; in sclang precedence this behaves as
  `(n - 1) / n`.

## BLowPass4 and BHiPass4

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/BEQSuite.sc`

Original:

```supercollider
BLowPass4 {
	*ar { arg in, freq = 1200.0, rq = 1.0, mul = 1.0, add = 0.0;
		var coefs;
		rq = sqrt(rq);
		coefs = BLowPass.sc(nil, freq, rq);
		^SOS.ar(SOS.ar(in, *coefs), *coefs ++ [mul, add]);
	}
}

BHiPass4 {
	*ar { arg in, freq = 1200.0, rq = 1.0, mul = 1.0, add = 0.0;
		var coefs;
		rq = sqrt(rq);
		coefs = BHiPass.sc(nil, freq, rq);
		^SOS.ar(SOS.ar(in, *coefs), *coefs ++ [mul, add]);
	}
}
```

Lowering:

```text
b_low_pass4_ar(in, freq = 1200, rq = 1, mul = 1, add = 0):
  rq2 = sqrt(rq)
  [a0, a1, a2, b1, b2] = BLowPass.sc(nil, freq, rq2)
  sos_ar(sos_ar(in, a0, a1, a2, b1, b2), a0, a1, a2, b1, b2) * mul + add

b_hi_pass4_ar(in, freq = 1200, rq = 1, mul = 1, add = 0):
  rq2 = sqrt(rq)
  [a0, a1, a2, b1, b2] = BHiPass.sc(nil, freq, rq2)
  sos_ar(sos_ar(in, a0, a1, a2, b1, b2), a0, a1, a2, b1, b2) * mul + add
```

Rate and edge cases:

- sclang only defines `ar`. The current Vibelang manifests list `kr`; that is
  not present in the SC class library.
- Coefficients come from the corresponding second-order BEQ class method and
  must match its formula.
- Multichannel expansion comes from `SOS` and binary ops.

## EnvGate

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Control/GraphBuilder.sc`

Original:

```supercollider
EnvGate {
	*new { arg i_level=1, gate, fadeTime, doneAction=2, curve='sin';
		var synthGate = gate ?? { NamedControl.kr(\gate, 1.0) };
		var synthFadeTime = fadeTime ?? { NamedControl.kr(\fadeTime, 0.02) };
		^EnvGen.kr(
			Env.new([ i_level, 1.0, 0.0], #[1.0, 1.0], curve, 1),
			synthGate, 1.0, 0.0, synthFadeTime, doneAction
		)
	}
}
```

Lowering:

```text
env_gate(i_level = 1, gate = NamedControl.kr("gate", 1),
         fadeTime = NamedControl.kr("fadeTime", 0.02),
         doneAction = 2, curve = "sin"):
  env_gen_kr(
    Env(levels = [i_level, 1, 0], times = [1, 1], curve = curve, releaseNode = 1),
    gate, levelScale = 1, levelBias = 0, timeScale = fadeTime,
    doneAction = doneAction
  )
```

Rate and edge cases:

- Always control-rate output.
- The defaults allocate named controls when arguments are omitted. A faithful
  lowering must either use existing synthdef parameters named `gate` and
  `fadeTime`, or create equivalent controls.
- This is used by SC's graph wrapping and JITLib fade helpers; it is not a
  server UGen.

## JPverb

Source: `/usr/share/SuperCollider/Extensions/SC3plugins/DEINDUGens/JPverb.sc`

The stdlib matrix surfaced `j_pverb_ar` in `erica_black_hole_dsp`.
`JPverb` is a convenience wrapper; the installed server UGen name is
`JPverbRaw`.

Original:

```supercollider
JPverb {
	*ar { | in, t60(1.0), damp(0.0), size(1.0), earlyDiff(0.707),
			modDepth(0.1), modFreq(2.0), low(1.0), mid(1.0), high(1.0),
			lowcut(500.0), highcut(2000.0)|
		in = in.asArray;

		^JPverbRaw.ar(in.first, in.last, damp, earlyDiff, highcut, high,
			lowcut, low, modDepth, modFreq, mid, size, t60)
	}
}

JPverbRaw : MultiOutUGen {
	*ar { | in1, in2, damp(0.0), earlydiff(0.707), highband(2000.0),
			highx(1.0), lowband(500.0), lowx(1.0), mdepth(0.1),
			mfreq(2.0), midx(1.0), size(1.0), t60(1.0) |
		^this.multiNew('audio', in1, in2, damp, earlydiff, highband,
			highx, lowband, lowx, mdepth, mfreq, midx, size, t60)
	}
}
```

Lowering:

```text
j_pverb_ar(in, t60 = 1, damp = 0, size = 1, earlyDiff = 0.707,
           modDepth = 0.1, modFreq = 2, low = 1, mid = 1, high = 1,
           lowcut = 500, highcut = 2000):
  channels = asArray(in)
  jpverb_raw_ar(
    channels.first,
    channels.last,
    damp,
    earlyDiff,
    highcut,
    high,
    lowcut,
    low,
    modDepth,
    modFreq,
    mid,
    size,
    t60
  )
```

Rate and edge cases:

- `JPverb` exposes only `ar`; `JPverbRaw` exposes `ar` and `kr`, but the
  wrapper does not call `kr`.
- `in.asArray.first` and `in.asArray.last` mean a mono input is duplicated into
  both raw inputs. A stereo array uses the first and last element. Arrays with
  more than two channels discard the middle channels.
- `JPverbRaw` is multi-output stereo. The wrapper returns the raw stereo output
  unchanged.
- `strings` finds `JPverbRaw` in installed plugin binaries, not `JPverb`.

## Additional Manifest-Audit Pseudos

The sibling pseudo-UGen audit broadened the manifest-level set beyond the
stdlib-triggering entries. The following are confirmed sclang-side aliases or
wrappers in the installed sources. They are not currently the primary stdlib
failure path, but they should not be emitted as literal server UGen names.

| Name | Source | Lowering |
| --- | --- | --- |
| `Greyhole` | `/usr/share/SuperCollider/Extensions/SC3plugins/DEINDUGens/Greyhole.sc` | `GreyholeRaw.ar(in.asArray.first, in.asArray.last, damp, delayTime, diff, feedback, modDepth, modFreq, size)` |
| `TWChoose` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Osc.sc` | `Select.ar/kr(TWindex.ar/kr(trig, weights, normalize), array)` |
| `LinLin` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Line.sc` | `ar`: `MulAdd(in, scale, offset)`; `kr`: `in * scale + offset`, where `scale = (dsthi - dstlo) / (srchi - srclo)` and `offset = dstlo - scale * srclo` |
| `ExpLin` / `explin` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/UGen.sc` | `log(prune(in)/inMin) / log(inMax/inMin) * (outMax - outMin) + outMin` |
| `ExpExp` / `expexp` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/UGen.sc` | `pow(outMax/outMin, log(prune(in)/inMin) / log(inMax/inMin)) * outMin` |
| `FFTCentroid` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDFFTUGens.sc` | Deprecated alias to `SpecCentroid.kr(buffer)` |
| `PV_DiffMags` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDFFTUGens.sc` | Deprecated alias to `PV_MagSubtract(bufferA, bufferB)` |
| `PulseDPW` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDOscUGens.sc` | `(SawDPW.RATE(freq, 0) - SawDPW.RATE(freq, (width + width).wrap(-1, 1))).madd(mul, add)` |
| `PanX2D` | `/usr/share/SuperCollider/Extensions/SC3plugins/JoshUGens/classes/PanX.sc` | `PanX.RATE(numChansX, PanX.RATE(numChansY, in, posY, level, widthY), posX, level, widthX)` |
| `Rotate` | `/usr/share/SuperCollider/Extensions/SC3plugins/JoshUGens/classes/Ambisonics.sc` | `Rotate2.ar(x, y, rotate * -0.31830988618379)` then return `[w, xout, yout, z]` |
| `Tilt` | `/usr/share/SuperCollider/Extensions/SC3plugins/JoshUGens/classes/Ambisonics.sc` | `Rotate2.ar(x, z, tilt * -0.31830988618379)` then return `[w, xout, y, zout]` |
| `Tumble` | `/usr/share/SuperCollider/Extensions/SC3plugins/JoshUGens/classes/Ambisonics.sc` | `Rotate2.ar(y, z, tilt * -0.31830988618379)` then return `[w, x, yout, zout]` |
| `AMClip`, `AbsDif`, `Atan2`, `Clip2`, `DifSqr`, `Excess`, `FirstArg`, `Fold2`, `Hypot`, `HypotApx`, `Ring1`, `Ring2`, `Ring3`, `Ring4`, `ScaleNeg`, `SqrDif`, `SqrSum`, `SumSqr`, `Thresh`, `Wrap2` | `crates/vibelang-dsp/ugen_manifests/math.json` plus SC binary op methods | Emit `BinaryOpUGen` with the manifest's `special_index`, not the manifest name as a UGen class. |

`OnsetsDS` is also confirmed pseudo in
`/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/OnsetsDS.sc`, but
its source explicitly deprecates it in favor of `Onsets.kr`. It expands to a
larger onset-detection graph involving `FFT`, `PV_Whiten`, one of
`FFTComplexDev`/`FFTPhaseDev`/`FFTMKL`/`FFTPower`, `MedianTriggered`,
comparisons, and `Trig1`; prefer removing or aliasing it rather than adding a
fresh hand-lowering.

## DynKlang and DynKlank

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/FSinOsc.sc`

Original:

```supercollider
DynKlank : UGen {
	*new1 { arg rate, specificationsArrayRef, input, freqscale = 1.0,
			freqoffset = 0.0, decayscale = 1.0;
		var spec = specificationsArrayRef.value;
		var selector = this.methodSelectorForRate(rate);
		^Ringz.perform(selector,
				input,
				spec[0] ? #[440.0] * freqscale + freqoffset,
				spec[2] ? #[1.0] * decayscale,
				spec[1] ? #[1.0]
		).sum
	}
}

DynKlang : UGen {
	*new1 { arg rate, specificationsArrayRef, freqscale = 1.0, freqoffset = 0.0;
		var spec = specificationsArrayRef.value;
		var selector = this.methodSelectorForRate(rate);
		^SinOsc.perform(selector,
				spec[0] ? #[440.0] * freqscale + freqoffset,
				spec[2] ? #[0.0],
				spec[1] ? #[1.0]
		).sum
	}
}
```

Lowering:

```text
dyn_klang_RATE(spec, freqscale = 1, freqoffset = 0):
  freqs  = (spec[0] default [440]) * freqscale + freqoffset
  phases = spec[2] default [0]
  amps   = spec[1] default [1]
  sum(SinOsc.RATE(freqs, phases, amps))

dyn_klank_RATE(spec, input, freqscale = 1, freqoffset = 0, decayscale = 1):
  freqs = (spec[0] default [440]) * freqscale + freqoffset
  times = (spec[2] default [1]) * decayscale
  amps  = spec[1] default [1]
  sum(Ringz.RATE(input, freqs, times, amps))
```

Rate and edge cases:

- Both define `ar` and `kr`.
- These are dynamic because frequency, amplitude, phase/time arrays can contain
  UGens. They lower to multichannel oscillator/resonator banks and sum the
  expanded result.
- The `specificationsArrayRef` is evaluated with `.value`; functions are
  allowed in sclang. Vibelang can restrict this to concrete arrays for now.

## Klang and Klank

Source: `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/FSinOsc.sc`

Original core:

```supercollider
Klang : UGen {
	*new1 { arg rate, freqscale, freqoffset, arrayRef;
		var specs, freqs, amps, phases;
		# freqs, amps, phases = arrayRef.dereference;
		specs = [freqs,
				amps ?? {Array.fill(freqs.size,1.0)},
				phases ?? {Array.fill(freqs.size,0.0)}
				].flop.flat;
		^super.new.rate_(rate).addToSynth.init([freqscale,freqoffset] ++ specs);
	}
}

Klank : UGen {
	*new1 { arg rate, input, freqscale, freqoffset, decayscale, arrayRef;
		var specs, freqs, amps, times;
		# freqs, amps, times = arrayRef.dereference;
		specs = [freqs,
				amps ?? {Array.fill(freqs.size,1.0)},
				times ?? {Array.fill(freqs.size,1.0)}
				].flop.flat;
		^super.new.rate_(rate).addToSynth.init([input,freqscale,freqoffset,decayscale] ++ specs);
	}
}
```

Lowering:

```text
klang_ar([freqs, amps?, phases?], freqscale = 1, freqoffset = 0):
  emit server Klang with inputs:
    [freqscale, freqoffset, freq0, amp0, phase0, freq1, amp1, phase1, ...]

klank_ar([freqs, amps?, times?], input, freqscale = 1, freqoffset = 0,
         decayscale = 1):
  emit server Klank with inputs:
    [input, freqscale, freqoffset, decayscale, freq0, amp0, time0, ...]
```

Rate and edge cases:

- `Klang` and `Klank` are real server UGen names on this host; `strings` finds
  exact names in installed plugin binaries.
- sclang only exposes `ar` for both.
- Missing amps default to all 1.0. Missing phases default to all 0.0. Missing
  Klank ring times default to all 1.0.
- `specificationsArrayRef.multichannelExpandRef(2)` participates in SC's ref
  multichannel expansion before `multiNewList`.
- These are not lowerable to simple binary UGens without changing behavior;
  unlike `DynKlang` and `DynKlank`, they rely on fixed-size server UGens.

## WAmp and RMS

Sources:

- `/usr/share/SuperCollider/Extensions/SC3plugins/BatUGens/BatUGens.sc`
- `/usr/share/SuperCollider/Extensions/SC3plugins/DEINDUGens/RMS.sc`

Original:

```supercollider
WAmp : UGen {
	*kr { arg in = 0.0, winSize = 0.1;
		^this.multiNew('control', in, winSize)
	}
}

RMS : UGen  {
	*ar { arg  in, lpFreq=10;
		^this.multiNew('audio', in, lpFreq);
	}
	*kr { arg in, lpFreq=10;
		^this.multiNew('control', in, lpFreq);
	}
}
```

Lowering:

```text
w_amp_kr(in = 0, winSize = 0.1):
  emit server WAmp.kr(in, winSize)

rms_ar(in, lpFreq = 10):
  emit server RMS.ar(in, lpFreq)

rms_kr(in, lpFreq = 10):
  emit server RMS.kr(in, lpFreq)
```

Rate and edge cases:

- These are not class-library composites.
- `WAmp` is control-rate only.
- `RMS` has audio and control rates.
- They depend on sc3-plugins being installed and loadable by scsynth.
