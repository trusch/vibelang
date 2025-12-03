# VibeLang

**Make music with code. Make code with vibes.**

VibeLang is a musical programming language that turns your text editor into a synthesizer. Write patterns, melodies, and entire arrangements in a clean scripting syntax—then hit save and hear your changes instantly.

```rhai
set_tempo(120);

import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/bass/sub/sub_deep.vibe";

let kick = voice("kick").synth("kick_808").gain(db(-6));
let bass = voice("bass").synth("sub_deep").gain(db(-12));

pattern("groove").on(kick).step("x...x...x..x....").len(4.0).start();
melody("line").on(bass).notes("C2 C2 G1 G1").len(4.0).start();
```

That's a whole beat. Run it with `--watch` and edit while it plays.

## Quick Start

### Prerequisites

- [SuperCollider](https://supercollider.github.io/) (the audio engine, scsynth)
- [JACK Audio](https://jackaudio.org/) (Linux/Mac) or your system audio

### Install

```bash
# Clone and build
git clone https://github.com/yourusername/vibelang.git
cd vibelang
cargo build --release

# Run your first beat
./target/release/vibelang examples/project/main.vibe --watch
```

### First Song

Create `hello.vibe`:

```rhai
set_tempo(110);

import "stdlib/drums/kicks/kick_808.vibe";

let kick = voice("kick").synth("kick_808");

pattern("four_on_floor")
    .on(kick)
    .step("x...x...x...x...")
    .len(4.0)
    .start();
```

Run it:
```bash
vibelang hello.vibe --watch
```

Edit the pattern. Save. Hear it change. That's the vibe.

---

## Language Primitives

### Tempo & Time

```rhai
set_tempo(128);                    // BPM
set_time_signature(4, 4);          // 4/4 time

db(-6)                             // Convert dB to amplitude
note(1, 16)                        // Note duration (1/16th = 0.25 beats)
bars(4)                            // 4 bars = 16 beats
```

### Voices

Voices are your instruments. Assign a synth, set gain, control polyphony:

```rhai
let lead = voice("lead")
    .synth("lead_bright")          // Use a synthdef
    .gain(db(-6))                  // Output level
    .poly(4)                       // 4-voice polyphony
    .set_param("cutoff", 2000.0);  // Synth parameters
```

### Patterns (Rhythm)

Step sequencer for drums and rhythmic parts:

```rhai
pattern("hihat")
    .on(hat_voice)
    .step("x.x.x.x.x.x.x.x.")       // x = hit, . = rest
    .len(2.0)                       // Length in beats
    .swing(0.1)                     // Shuffle
    .start();

// Velocity levels: 0-9 (9 = loudest)
pattern("ghost_snare")
    .on(snare)
    .step("..3.x.3...3.x...")
    .len(4.0)
    .start();

// Euclidean rhythms
pattern("afro").on(perc).euclid(5, 8).len(2.0).start();
```

### Melodies (Pitch)

Note sequences with sustain and rests:

```rhai
melody("bassline")
    .on(bass_voice)
    .notes("C2 - - . | E2 - G2 . | A2 - - - | G2 . E2 .")
    .len(8.0)
    .gate(0.8)                      // Note length (0-1)
    .start();

// Note syntax:
// C4, A#3, Bb2 = pitches
// - = hold previous note
// . = rest
// | = visual separator (ignored)
```

### Sequences (Arrangement)

Clip-based timeline for song structure:

```rhai
let verse = melody("verse").on(lead).notes("...").len(16.0);
let chorus = melody("chorus").on(lead).notes("...").len(16.0);

sequence("song")
    .loop_bars(64)
    .clip(0..bars(16), verse)
    .clip(bars(16)..bars(32), chorus)
    .clip(bars(32)..bars(48), verse)
    .clip(bars(48)..bars(64), chorus)
    .start();
```

### Fades (Automation)

Parameter changes over time:

```rhai
fade("filter_sweep")
    .on(bass_voice)
    .param("cutoff")
    .from(200.0)
    .to(5000.0)
    .over_bars(8)
    .start();
```

### Groups (Mixing)

Organize voices with shared effects:

```rhai
let drums = define_group("Drums", ||{
    let kick = voice("kick").synth("kick_808");
    let snare = voice("snare").synth("snare_808");

    pattern("kick").on(kick).step("x...x...").len(2.0).start();
    pattern("snare").on(snare).step("....x...").len(2.0).start();
});

drums.gain(db(-3));
drums.add_effect("comp", "compressor", #{threshold: 0.5});
```

### Effects

Add processing to groups:

```rhai
fx("room")
    .synth("reverb")
    .param("room", 0.6)
    .param("mix", 0.3)
    .apply();
```

### SynthDefs (Sound Design)

Create custom synths using SuperCollider UGens:

```rhai
define_synthdef("my_bass")
    .param("freq", 110.0)
    .param("amp", 0.5)
    .body(|freq, amp| {
        let osc = saw_ar(freq) + saw_ar(freq * 1.01);
        let filt = rlpf_ar(osc, 800.0, 0.3);
        let env = env_perc_ar(0.01, 0.3, amp, -4.0);
        filt * env
    });
```

### SFZ Instruments

Load sampled instruments:

```rhai
let piano = load_sfz("piano", "samples/piano.sfz");

let piano_voice = voice("piano").on(piano).poly(16);

melody("keys").on(piano_voice).notes("C4 E4 G4 C5").len(4.0).start();
```

---

## Standard Library

VibeLang includes 180+ ready-to-use sounds:

| Category | Sounds |
|----------|--------|
| **Drums** | kick_808, kick_909, snare_808, hihat_closed, clap_reverb, ... |
| **Bass** | sub_deep, acid_303, reese_classic, wobble_deep, pluck_funky, ... |
| **Leads** | lead_saw, lead_supersaw, pluck_bell, stab_brass, ... |
| **Pads** | pad_warm, pad_shimmer, pad_dark, pad_evolving, ... |
| **Effects** | reverb, delay, distortion, chorus, compressor, ... |

```rhai
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/bass/acid/acid_303_classic.vibe";
import "stdlib/effects/reverb.vibe";
```

All sounds are plain `.vibe` files—read them, tweak them, learn from them.

---

## Watch Mode (Live Coding)

The magic is in `--watch`:

```bash
vibelang my_song.vibe --watch
```

Edit your file. Save. Changes apply instantly. No restart needed. Script errors don't kill the audio—you'll see the error and keep jamming.

---

## Project Structure

```
my_project/
├── main.vibe              # Your song
├── synths/                # Custom synthdefs
│   └── my_bass.vibe
├── patterns/              # Pattern definitions
│   └── drums.vibe
└── samples/               # Audio files
    └── vocal.wav
```

Use imports to organize:

```rhai
import "synths/my_bass.vibe";
import "patterns/drums.vibe";
```

---

## Example: Full Track

```rhai
set_tempo(122);

import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/bass/sub/sub_deep.vibe";
import "stdlib/leads/synth/lead_bright.vibe";
import "stdlib/effects/reverb.vibe";

let drums = define_group("Drums", ||{
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));

    pattern("kick").on(kick).step("x...x...x..x....").len(4.0).start();
    pattern("snare").on(snare).step("....x.......x...").len(4.0).start();
});

let bass = define_group("Bass", ||{
    let sub = voice("sub").synth("sub_deep").gain(db(-10)).poly(1);

    melody("bassline")
        .on(sub)
        .notes("D2 - - . | D2 - . . | F2 - - . | D2 . A1 .")
        .len(16.0)
        .start();
});

let lead = define_group("Lead", ||{
    let synth = voice("lead").synth("lead_bright").gain(db(-12)).poly(4);

    melody("melody")
        .on(synth)
        .notes("D4 - - . | . . F4 - | . . D4 - | . . . .")
        .len(4.0)
        .start();

    fx("space").synth("reverb").param("room", 0.5).param("mix", 0.3).apply();
});
```

---

## Alpha Status

This is an alpha release. Things work, things break, things will change.

**Working well:**
- Patterns, melodies, sequences
- Watch mode with hot reload
- SynthDef creation and caching
- Group-based mixing with effects
- SFZ instrument loading
- Standard library sounds

**Experimental:**
- VST plugin support
- MIDI input mapping
- Complex automation curves

**Known quirks:**
- SFZ instruments need manual note-off in some cases
- Some stdlib synths may need tuning

Found a bug? Have an idea? [Open an issue](https://github.com/yourusername/vibelang/issues).

---

## Why VibeLang?

- **Text is powerful.** Copy, paste, diff, git, grep. Your music is code.
- **Instant feedback.** Edit-save-hear in milliseconds.
- **Transparent.** Every sound is a readable `.vibe` file. No black boxes.
- **Deep when you need it.** From 4-line beats to full album productions.

---

## License

MIT

---

*Made with love and loud bass.*
