# VibeLang

**Make music with code. Make code with vibes.**

VibeLang is a musical programming language that turns your text editor into a synthesizer. Write patterns, melodies, and entire arrangements in a clean scripting syntax—then hit save and hear your changes instantly.

```rhai
set_tempo(120);

import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/bass/sub/sub_deep.vibe";

let kick = voice("kick").synth("kick_808").gain(db(-6));
let bass = voice("bass").synth("sub_deep").gain(db(-12));

pattern("groove").on(kick).step("x... x... x..x ....").start();
melody("line").on(bass).notes("C3 - - - | C3 - G2 -").start();
```

That's a whole beat. Just run it and edit while it plays—watching is on by default.

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

# Run your first beat (watching is on by default)
./target/release/vibe examples/minimal_techno/main.vibe
```

### First Song

Create `hello.vibe`:

```rhai
set_tempo(110);

import "stdlib/drums/kicks/kick_808.vibe";

let kick = voice("kick").synth("kick_808").gain(db(-6));

pattern("four_on_floor")
    .on(kick)
    .step("x...x...x...x...")
    .start();
```

Run it:
```bash
vibe hello.vibe
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
    .synth("lead_bright")           // Use a synthdef
    .gain(db(-6))                   // Output level in dB
    .poly(4)                        // 4-voice polyphony
    .set_param("cutoff", 2000.0);   // Synth parameters
```

### Patterns (Rhythm)

Step sequencer for drums and rhythmic parts:

```rhai
// x = hit, . = rest
pattern("hihat")
    .on(hat_voice)
    .step("x.x.x.x.x.x.x.x.")
    .start();

// Velocity levels: 0-9 (9 = loudest)
pattern("ghost_snare")
    .on(snare)
    .step("..3.x.3...3.x...")
    .start();

// Euclidean rhythms
pattern("afro").on(perc).euclid(5, 8).start();
```

### Melodies (Pitch)

Note sequences with sustain and rests:

```rhai
melody("bassline")
    .on(bass_voice)
    .notes("C2 - - . | E2 - G2 . | A2 - - - | G2 . E2 .")
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
let verse = melody("verse").on(lead).notes("E3 - - . | G3 - - . | A3 - - . | G3 - - .");
let chorus = melody("chorus").on(lead).notes("C4 - - . | E4 - - . | G4 - - . | E4 - - .");

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
    .on_voice("bass")              // Target a voice by name
    .param("cutoff")
    .from(200.0)
    .to(5000.0)
    .over_bars(8);

// Fades can also target groups or effects:
fade("group_swell")
    .on_group("Synth")
    .param("amp")
    .from(db(-20.0))
    .to(db(0.0))
    .over_bars(4);
```

### Groups (Mixing)

Organize voices with shared effects:

```rhai
let drums = define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));

    pattern("kick").on(kick).step("x...x...x...x...").start();
    pattern("snare").on(snare).step("....x.......x...").start();

    // Add effects to the group
    fx("drum_verb")
        .synth("reverb")
        .param("room", 0.3)
        .param("mix", 0.15)
        .apply();
});
```

### Effects

Add processing inside groups:

```rhai
fx("room")
    .synth("reverb")
    .param("room", 0.6)
    .param("mix", 0.3)
    .apply();

fx("tape_delay")
    .synth("delay")
    .param("delay_time", 0.375)
    .param("feedback", 0.4)
    .param("mix", 0.25)
    .apply();
```

### SynthDefs (Sound Design)

Create custom synths using SuperCollider UGens:

```rhai
define_synthdef("my_bass")
    .param("freq", 110.0)
    .param("amp", 0.5)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        let osc = saw_ar(freq) + saw_ar(freq * 1.01);
        let filt = rlpf_ar(osc, 800.0, 0.3);

        let env = env_adsr(0.01, 0.1, 0.5, 0.2);
        let env = NewEnvGenBuilder(env, gate).with_done_action(2.0).build();

        filt * env * amp
    });
```

### Samples

Load audio samples:

```rhai
let hit = sample("hit", "samples/hit.wav");
let hit_voice = voice("hit").on(hit).gain(db(-6));

pattern("hit_pattern").on(hit_voice).step("x...x...").start();
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

Watching is on by default—just run your file:

```bash
vibe my_song.vibe
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
import "stdlib/effects/reverb.vibe";

// Define a custom lead synth
define_synthdef("lead_bright")
    .param("freq", 440.0)
    .param("amp", 0.3)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        let env = env_adsr(0.01, 0.2, 0.5, 0.3);
        let env = NewEnvGenBuilder(env, gate).with_done_action(2.0).build();
        let osc = saw_ar(freq) + pulse_ar(freq * 2.0, 0.3) * 0.5;
        let filt = rlpf_ar(osc, 2000.0, 0.4);
        filt * env * amp
    });

let drums = define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));

    pattern("kick").on(kick).step("x...x...x..x....").start();
    pattern("snare").on(snare).step("....x.......x...").start();
});

let bass = define_group("Bass", || {
    let sub = voice("sub").synth("sub_deep").gain(db(-10)).poly(1);

    melody("bassline")
        .on(sub)
        .notes("D2 - - . | D2 - . . | F2 - - . | D2 . A1 .")
        .start();
});

let lead = define_group("Lead", || {
    let synth = voice("lead").synth("lead_bright").gain(db(-12)).poly(4);

    melody("melody")
        .on(synth)
        .notes("D4 - - . | . . F4 - | . . D4 - | . . . .")
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
