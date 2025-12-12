# Getting Started with VibeLang

Welcome to VibeLang! This guide will take you from zero to creating your first complete beat with drums, bass, melodies, sequences, and effects. Each chapter builds on the previous one, so follow along in order.

## Table of Contents

1. [Installation](#1-installation)
2. [Your First Sound](#2-your-first-sound)
3. [Building a Drum Pattern](#3-building-a-drum-pattern)
4. [Adding a Bass Line](#4-adding-a-bass-line)
5. [Creating Melodies](#5-creating-melodies)
6. [Organizing with Groups](#6-organizing-with-groups)
7. [Adding Effects](#7-adding-effects)
8. [Arranging with Sequences](#8-arranging-with-sequences)
9. [Parameter Automation](#9-parameter-automation)
10. [Custom Synthesis](#10-custom-synthesis)
11. [Complete Example](#11-complete-example)

---

## 1. Installation

### Prerequisites

VibeLang requires the following to be installed:

- **Rust toolchain** - Install from [rustup.rs](https://rustup.rs)
- **SuperCollider** - The synthesis engine. Install from [supercollider.github.io](https://supercollider.github.io)
- **JACK Audio** (Linux/Mac) or your system's audio driver

### Installing VibeLang

```bash
cargo install vibelang-cli
```

This installs the `vibe` command globally.

### Verify Installation

```bash
vibe --help
```

---

## 2. Your First Sound

Let's start with the simplest possible VibeLang program. Create a new file called `first.vibe`:

```rhai
// first.vibe - Your first VibeLang program

// Import the kick drum from the standard library
import "stdlib/drums/kicks/kick_808.vibe";

// Set the tempo (beats per minute)
set_tempo(120);

// Create a voice using the imported kick drum
let kick = voice("kick")
    .synth("kick_808")
    .gain(db(-6));

// Create a simple pattern that triggers on every beat
pattern("basic")
    .on(kick)
    .step("x... x... x... x...")
    .start();
```

Run it with:

```bash
vibe first.vibe
```

You should hear a kick drum on every beat! Press `Ctrl+C` to stop.

### Understanding the Code

- `import "stdlib/..."` - Imports a synthdef from the standard library
- `set_tempo(120)` - Sets the tempo to 120 BPM
- `voice("kick")` - Creates a named voice (instrument instance)
- `.synth("kick_808")` - Uses the imported 808 kick sound
- `.gain(db(-6))` - Sets the volume to -6 decibels
- `pattern("basic")` - Creates a named pattern (step sequencer)
- `.on(kick)` - Connects the pattern to our kick voice
- `.step("x...")` - Defines the rhythm (`x` = hit, `.` = rest)
- `.start()` - Starts the pattern playing immediately

### Pattern Notation

The step pattern uses a simple notation:
- `x` - Trigger the voice (full velocity)
- `.` - Rest (silence)
- `1-9` - Trigger with specific velocity (1 = quiet, 9 = loud)
- Spaces are optional (use them for readability)

### Pattern Length

You can use different pattern lengths. The number of tokens determines how the pattern maps to bars:

```rhai
// 16 steps = 16th notes (one bar in 4/4)
pattern("16th").on(kick).step("x... x... x... x...").start();

// 8 steps = 8th notes (one bar in 4/4)
pattern("8th").on(kick).step("x. x. x. x.").start();

// 4 steps = quarter notes (one bar in 4/4)
pattern("quarter").on(kick).step("x x x x").start();

// 32 steps = 32nd notes (one bar in 4/4)
pattern("32nd").on(kick).step("x....... x....... x....... x.......").start();
```

---

## 3. Building a Drum Pattern

Now let's build a more complete drum kit. Create `drums.vibe`:

```rhai
// drums.vibe - A basic drum pattern

// Import drum sounds from stdlib
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";

set_tempo(120);
set_time_signature(4, 4);

// Create our drum voices
let kick = voice("kick")
    .synth("kick_808")
    .gain(db(-6));

let snare = voice("snare")
    .synth("snare_808")
    .gain(db(-8));

let hihat = voice("hihat")
    .synth("hihat_808_closed")
    .gain(db(-12));

// Four-on-the-floor kick pattern (16 steps = 16th notes)
pattern("kick_pattern")
    .on(kick)
    .step("x... x... x... x...")
    .start();

// Snare on beats 2 and 4
pattern("snare_pattern")
    .on(snare)
    .step(".... x... .... x...")
    .start();

// Offbeat hi-hats (8th notes using 8-step pattern)
pattern("hihat_pattern")
    .on(hihat)
    .step(".x .x .x .x")
    .start();
```

### Adding Ghost Notes

Ghost notes add groove and feel. Use numbers 1-9 for velocity:

```rhai
// Replace the snare pattern with ghost notes
pattern("snare_pattern")
    .on(snare)
    .step("..3. x.3. ..3. x...")
    .start();
```

The `3` plays the snare quietly (velocity 3 out of 9), creating subtle "ghost" hits.

### Euclidean Rhythms

For more interesting rhythms, try Euclidean patterns:

```rhai
// Add to your imports
import "stdlib/drums/percussion/clave.vibe";

let clave = voice("clave")
    .synth("clave")
    .gain(db(-10));

// Euclidean rhythm: 5 hits spread across 16 steps
pattern("clave_pattern")
    .on(clave)
    .euclid(5, 16)
    .start();
```

---

## 4. Adding a Bass Line

Let's add bass to our drums. Create `drums_and_bass.vibe`:

```rhai
// drums_and_bass.vibe - Drums with a bass line

// Import sounds
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";
import "stdlib/bass/sub/sub_deep.vibe";

set_tempo(120);
set_time_signature(4, 4);

// === DRUMS ===
let kick = voice("kick").synth("kick_808").gain(db(-6));
let snare = voice("snare").synth("snare_808").gain(db(-8));
let hihat = voice("hihat").synth("hihat_808_closed").gain(db(-12));

pattern("kick").on(kick).step("x... x... x... x...").start();
pattern("snare").on(snare).step(".... x... .... x...").start();
pattern("hihat").on(hihat).step(".x .x .x .x").start();

// === BASS ===
let bass = voice("bass")
    .synth("sub_deep")      // Deep sub bass
    .gain(db(-10))
    .poly(1);               // Monophonic (one note at a time)

// Create a bass line melody
melody("bassline")
    .on(bass)
    .notes("C2 . . . | C2 . E2 . | F2 . . . | G2 . F2 E2")
    .start();
```

### Melody Notation

The melody notation is simple:
- `C2`, `D#3`, `Bb4` - Note names with octave (sharps `#` and flats `b`)
- `.` - Rest
- `-` - Hold the previous note
- `|` - Bar separator (visual only, ignored by parser)
- Spaces are optional (use them for readability)

### Alternative Bass Sounds

Try different bass sounds from the standard library:

```rhai
// Acid bass
import "stdlib/bass/acid/acid_303_classic.vibe";
let bass = voice("bass").synth("acid_303_classic").gain(db(-10)).poly(1);

// Reese bass (thick, detuned)
import "stdlib/bass/reese/reese_classic.vibe";
let bass = voice("bass").synth("reese_classic").gain(db(-10)).poly(1);

// Plucky bass
import "stdlib/bass/pluck/pluck_funky.vibe";
let bass = voice("bass").synth("pluck_funky").gain(db(-10)).poly(1);
```

---

## 5. Creating Melodies

Now let's add a melodic element. Create `with_melody.vibe`:

```rhai
// with_melody.vibe - Adding a melody to our beat

// Import sounds
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";
import "stdlib/bass/sub/sub_deep.vibe";
import "stdlib/leads/synth/lead_saw.vibe";

set_tempo(120);
set_time_signature(4, 4);

// === DRUMS ===
let kick = voice("kick").synth("kick_808").gain(db(-6));
let snare = voice("snare").synth("snare_808").gain(db(-8));
let hihat = voice("hihat").synth("hihat_808_closed").gain(db(-12));

pattern("kick").on(kick).step("x... x... x... x...").start();
pattern("snare").on(snare).step(".... x... .... x...").start();
pattern("hihat").on(hihat).step(".x .x .x .x").start();

// === BASS ===
let bass = voice("bass").synth("sub_deep").gain(db(-10)).poly(1);

melody("bassline")
    .on(bass)
    .notes("C2 . . . | C2 . E2 . | F2 . . . | G2 . F2 E2")
    .start();

// === LEAD MELODY ===
let lead = voice("lead")
    .synth("lead_saw")
    .gain(db(-14))
    .poly(4);               // Polyphonic (can play chords)

melody("lead_melody")
    .on(lead)
    .notes("C4 . E4 . | G4 . E4 . | C4 . E4 G4 | A4 - - . | G4 . E4 . | C4 . D4 . | E4 - - - | - . . .")
    .start();
```

### Holding Notes

Use `-` to hold notes across steps:

```rhai
melody("sustained")
    .on(lead)
    .notes("C4 - - - | E4 - - - | G4 - - - | C5 - - -")
    .start();
```

### Chords

For chords, create multiple melodies on a polyphonic voice:

```rhai
import "stdlib/pads/ambient/pad_warm.vibe";

let pad = voice("pad")
    .synth("pad_warm")
    .gain(db(-16))
    .poly(8);               // 8-voice polyphony for big chords

// Stack notes in parallel for chords (use multiple melodies)
melody("chord_root").on(pad).notes("C3 - - - - - - - | F3 - - - - - - -").start();
melody("chord_third").on(pad).notes("E3 - - - - - - - | A3 - - - - - - -").start();
melody("chord_fifth").on(pad).notes("G3 - - - - - - - | C4 - - - - - - -").start();
```

---

## 6. Organizing with Groups

As your track grows, organize instruments into groups. Create `with_groups.vibe`:

```rhai
// with_groups.vibe - Organized with groups

// Import all sounds
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";
import "stdlib/drums/claps/clap_808.vibe";
import "stdlib/bass/sub/sub_deep.vibe";
import "stdlib/leads/synth/lead_saw.vibe";

set_tempo(120);
set_time_signature(4, 4);

// === DRUM GROUP ===
define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));
    let hihat = voice("hihat").synth("hihat_808_closed").gain(db(-12));
    let clap = voice("clap").synth("clap_808").gain(db(-10));

    pattern("kick").on(kick).step("x... x... x... x...").start();
    pattern("snare").on(snare).step(".... x... .... x...").start();
    pattern("hihat").on(hihat).step(".x .x .x .x").start();
    pattern("clap").on(clap).step(".... x... .... x...").start();
});

// === BASS GROUP ===
define_group("Bass", || {
    let bass = voice("bass").synth("sub_deep").gain(db(-10)).poly(1);

    melody("bassline")
        .on(bass)
        .notes("C2 . . . | C2 . E2 . | F2 . . . | G2 . F2 E2")
        .start();
});

// === SYNTH GROUP ===
define_group("Synth", || {
    let lead = voice("lead").synth("lead_saw").gain(db(-14)).poly(4);

    melody("lead_melody")
        .on(lead)
        .notes("C4 . E4 . | G4 . E4 . | C4 . E4 G4 | A4 - - .")
        .start();
});
```

### Group Controls

Groups can be controlled together:

```rhai
// After defining groups, you can control them
let drums = get_group("Drums");
drums.gain(db(-3));         // Boost drums by 3dB
drums.mute();               // Mute the entire group
drums.unmute();             // Unmute
drums.solo(true);           // Solo this group
```

---

## 7. Adding Effects

Let's add some effects to make it sound professional. Create `with_effects.vibe`:

```rhai
// with_effects.vibe - Adding effects

// Import sounds
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";
import "stdlib/drums/claps/clap_808.vibe";
import "stdlib/bass/sub/sub_deep.vibe";
import "stdlib/leads/synth/lead_saw.vibe";

// Import effects
import "stdlib/effects/reverbs/reverb.vibe";
import "stdlib/effects/reverbs/hall_reverb.vibe";
import "stdlib/effects/delays/ping_pong_delay.vibe";
import "stdlib/effects/distortion/overdrive.vibe";

set_tempo(120);
set_time_signature(4, 4);

// === DRUM GROUP ===
define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));
    let hihat = voice("hihat").synth("hihat_808_closed").gain(db(-12));
    let clap = voice("clap").synth("clap_808").gain(db(-10));

    pattern("kick").on(kick).step("x... x... x... x...").start();
    pattern("snare").on(snare).step(".... x... .... x...").start();
    pattern("hihat").on(hihat).step(".x .x .x .x").start();
    pattern("clap").on(clap).step(".... x... .... x...").start();

    // Add reverb to the drum group (light room reverb)
    fx("drum_reverb")
        .synth("reverb")
        .param("room", 0.2)
        .param("mix", 0.15)
        .apply();
});

// === BASS GROUP ===
define_group("Bass", || {
    let bass = voice("bass").synth("sub_deep").gain(db(-10)).poly(1);

    melody("bassline")
        .on(bass)
        .notes("C2 . . . | C2 . E2 . | F2 . . . | G2 . F2 E2")
        .start();

    // Add saturation to bass for warmth
    fx("bass_drive")
        .synth("overdrive")
        .param("drive", 0.3)
        .param("mix", 0.4)
        .apply();
});

// === SYNTH GROUP ===
define_group("Synth", || {
    let lead = voice("lead").synth("lead_saw").gain(db(-14)).poly(4);

    melody("lead_melody")
        .on(lead)
        .notes("C4 . E4 . | G4 . E4 . | C4 . E4 G4 | A4 - - .")
        .start();

    // Add delay and reverb to synths
    fx("lead_delay")
        .synth("ping_pong_delay")
        .param("time", 0.375)       // Dotted 8th note at 120 BPM
        .param("feedback", 0.4)
        .param("mix", 0.3)
        .apply();

    fx("lead_reverb")
        .synth("hall_reverb")
        .param("room", 0.6)
        .param("mix", 0.25)
        .apply();
});
```

### Common Effects

Here are some effects from the standard library:

```rhai
// Reverbs
import "stdlib/effects/reverbs/reverb.vibe";
import "stdlib/effects/reverbs/hall_reverb.vibe";
import "stdlib/effects/reverbs/plate_reverb.vibe";

fx("verb").synth("reverb").param("room", 0.5).param("mix", 0.3).apply();
fx("hall").synth("hall_reverb").param("room", 0.8).param("mix", 0.4).apply();
fx("plate").synth("plate_reverb").param("decay", 0.6).param("mix", 0.3).apply();

// Delays
import "stdlib/effects/delays/delay.vibe";
import "stdlib/effects/delays/ping_pong_delay.vibe";
import "stdlib/effects/delays/dub_delay.vibe";

fx("delay").synth("delay").param("time", 0.25).param("feedback", 0.5).param("mix", 0.3).apply();
fx("ping").synth("ping_pong_delay").param("time", 0.375).param("feedback", 0.4).param("mix", 0.25).apply();
fx("dub").synth("dub_delay").param("time", 0.5).param("feedback", 0.6).param("mix", 0.35).apply();

// Filters
import "stdlib/effects/filters/lowpass.vibe";
import "stdlib/effects/filters/moog_filter.vibe";

fx("lowpass").synth("lowpass").param("cutoff", 800.0).param("resonance", 0.3).apply();
fx("moog").synth("moog_filter").param("cutoff", 1200.0).param("resonance", 0.5).apply();

// Dynamics
import "stdlib/effects/dynamics/compressor.vibe";
import "stdlib/effects/dynamics/limiter.vibe";

fx("comp").synth("compressor").param("threshold", db(-12)).param("ratio", 4.0).apply();
fx("limit").synth("limiter").param("threshold", db(-3)).apply();

// Modulation
import "stdlib/effects/modulation/chorus.vibe";
import "stdlib/effects/modulation/phaser.vibe";

fx("chorus").synth("chorus").param("rate", 0.5).param("depth", 0.3).param("mix", 0.4).apply();
fx("phaser").synth("phaser").param("rate", 0.25).param("depth", 0.6).param("mix", 0.5).apply();

// Distortion
import "stdlib/effects/distortion/distortion.vibe";
import "stdlib/effects/distortion/bitcrush.vibe";

fx("dist").synth("distortion").param("drive", 0.5).param("mix", 0.6).apply();
fx("crush").synth("bitcrush").param("bits", 8.0).param("mix", 0.3).apply();
```

---

## 8. Arranging with Sequences

Now let's arrange our track into sections. Create `with_arrangement.vibe`:

```rhai
// with_arrangement.vibe - Full arrangement

// Import sounds
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";
import "stdlib/bass/sub/sub_deep.vibe";
import "stdlib/leads/synth/lead_saw.vibe";

// Import effects
import "stdlib/effects/reverbs/reverb.vibe";
import "stdlib/effects/reverbs/hall_reverb.vibe";
import "stdlib/effects/delays/ping_pong_delay.vibe";
import "stdlib/effects/distortion/overdrive.vibe";

set_tempo(120);
set_time_signature(4, 4);

// === DRUMS ===
define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));
    let hihat = voice("hihat").synth("hihat_808_closed").gain(db(-12));

    // Define multiple pattern variations (don't use .start() - they'll be triggered by the sequence)
    let kick_basic = pattern("kick_basic").on(kick).step("x... x... x... x...");
    let kick_busy = pattern("kick_busy").on(kick).step("x... x... x.x. x...");

    let snare_basic = pattern("snare_basic").on(snare).step(".... x... .... x...");
    let snare_fill = pattern("snare_fill").on(snare).step(".... x... ..x. x.x.");

    let hihat_basic = pattern("hihat_basic").on(hihat).step(".x .x .x .x");
    let hihat_busy = pattern("hihat_busy").on(hihat).step("xx xx xx xx");

    // Arrange drums with a sequence
    sequence("drum_arrangement")
        .loop_bars(32)
        // Intro: just hats (bars 0-8)
        .clip(0..bars(8), hihat_basic)
        // Build: add kick (bars 8-16)
        .clip(bars(8)..bars(16), kick_basic)
        .clip(bars(8)..bars(16), hihat_basic)
        // Drop: full drums (bars 16-24)
        .clip(bars(16)..bars(24), kick_busy)
        .clip(bars(16)..bars(24), snare_basic)
        .clip(bars(16)..bars(24), hihat_busy)
        // Outro: strip back (bars 24-32)
        .clip(bars(24)..bars(32), kick_basic)
        .clip(bars(24)..bars(32), hihat_basic)
        .start();

    fx("drum_reverb").synth("reverb").param("room", 0.2).param("mix", 0.15).apply();
});

// === BASS ===
define_group("Bass", || {
    let bass = voice("bass").synth("sub_deep").gain(db(-10)).poly(1);

    let bass_main = melody("bass_main").on(bass)
        .notes("C2 . . . | C2 . E2 . | F2 . . . | G2 . F2 E2");

    let bass_var = melody("bass_var").on(bass)
        .notes("C2 . C3 . | C2 . E2 . | F2 . F3 . | G2 . . .");

    sequence("bass_arrangement")
        .loop_bars(32)
        // No bass in intro (bars 0-8) - just don't clip anything
        // Simple bass in build (bars 8-16)
        .clip(bars(8)..bars(16), bass_main)
        // Variation in drop (bars 16-24)
        .clip(bars(16)..bars(24), bass_var)
        // Back to main in outro (bars 24-32)
        .clip(bars(24)..bars(32), bass_main)
        .start();

    fx("bass_drive").synth("overdrive").param("drive", 0.3).param("mix", 0.4).apply();
});

// === SYNTH ===
define_group("Synth", || {
    let lead = voice("lead").synth("lead_saw").gain(db(-14)).poly(4);

    let lead_main = melody("lead_main").on(lead)
        .notes("C4 . E4 . | G4 . E4 . | C4 . E4 G4 | A4 - - .");

    let lead_var = melody("lead_var").on(lead)
        .notes("C5 . . . | G4 . E4 . | C4 - - - | . . . .");

    sequence("lead_arrangement")
        .loop_bars(32)
        // No lead in intro or build (bars 0-16) - just don't clip anything
        // Lead comes in at drop (bars 16-24)
        .clip(bars(16)..bars(24), lead_main)
        // Variation in outro (bars 24-32)
        .clip(bars(24)..bars(32), lead_var)
        .start();

    fx("lead_delay").synth("ping_pong_delay").param("time", 0.375).param("feedback", 0.4).param("mix", 0.3).apply();
    fx("lead_reverb").synth("hall_reverb").param("room", 0.6).param("mix", 0.25).apply();
});
```

### Sequence Concepts

- `bars(n)` - Converts bar count to beats (e.g., `bars(4)` = 16 beats in 4/4 time)
- `.loop_bars(n)` - Sets the sequence length and loops
- `.clip(start..end, pattern_or_melody)` - Schedules content in a time range
- Multiple clips can overlap (good for layering)
- If you don't want something playing in a section, just don't add a clip for it

### Important: Patterns/Melodies in Sequences

- Use `.start()` when you want the pattern/melody to play immediately and loop forever
- When using sequences, just define patterns/melodies without `.start()` or `.apply()` - pass them directly to `.clip()` which handles registration internally

---

## 9. Parameter Automation

Add movement with fades and automation. Create `with_automation.vibe`:

```rhai
// with_automation.vibe - Parameter automation

// Import sounds
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";
import "stdlib/bass/acid/acid_303_classic.vibe";
import "stdlib/leads/synth/lead_saw.vibe";

// Import effects
import "stdlib/effects/reverbs/hall_reverb.vibe";
import "stdlib/effects/distortion/overdrive.vibe";

set_tempo(120);
set_time_signature(4, 4);

define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));
    let hihat = voice("hihat").synth("hihat_808_closed").gain(db(-12));

    pattern("kick").on(kick).step("x... x... x... x...").start();
    pattern("snare").on(snare).step(".... x... .... x...").start();
    pattern("hihat").on(hihat).step(".x .x .x .x").start();
});

define_group("Bass", || {
    let bass = voice("bass")
        .synth("acid_303_classic")  // Acid bass has a filter cutoff parameter
        .gain(db(-10))
        .poly(1)
        .set_param("cutoff", 400.0);  // Start with low cutoff

    let bass_main = melody("bassline").on(bass)
        .notes("C2 C2 C2 C2 | C2 C2 E2 E2 | F2 F2 F2 F2 | G2 G2 F2 E2");

    // Filter sweep automation - sweep cutoff over 8 bars
    let filter_sweep = fade("filter_sweep")
        .on_voice("bass")
        .param("cutoff")
        .from(400.0)
        .to(4000.0)
        .over_bars(8)
        .apply();

    // Use sequence to coordinate the melody and fade
    sequence("bass_seq")
        .loop_bars(8)
        .clip(0..bars(8), bass_main)
        .clip(0..bars(8), filter_sweep)
        .start();

    fx("bass_drive").synth("overdrive").param("drive", 0.3).param("mix", 0.4).apply();
});

define_group("Synth", || {
    let lead = voice("lead").synth("lead_saw").gain(db(-14)).poly(4);

    let lead_melody = melody("lead_melody").on(lead)
        .notes("C4 . E4 . | G4 . E4 . | C4 . E4 G4 | A4 - - .");

    fx("lead_reverb").synth("hall_reverb").param("room", 0.6).param("mix", 0.0).apply();

    // Fade in the reverb mix over 4 bars
    let reverb_swell = fade("reverb_swell")
        .on_effect("lead_reverb")
        .param("mix")
        .from(0.0)
        .to(0.5)
        .over_bars(4)
        .apply();

    sequence("lead_seq")
        .loop_bars(4)
        .clip(0..bars(4), lead_melody)
        .clip(0..bars(4), reverb_swell)
        .start();
});
```

### Fade Types

```rhai
// Fade a voice parameter - use in a sequence
let voice_fade = fade("voice_fade")
    .on_voice("bass")
    .param("cutoff")
    .from(200)
    .to(5000)
    .over_bars(8)
    .apply();

// Fade a group's amplitude
let group_fade = fade("group_fade")
    .on_group("Drums")
    .param("amp")
    .from(0.0)
    .to(1.0)
    .over_bars(4)
    .apply();

// Fade an effect parameter
let fx_fade = fade("fx_fade")
    .on_effect("reverb")
    .param("mix")
    .from(0.0)
    .to(0.6)
    .over_bars(16)
    .apply();

// Then use them in sequences
sequence("automation_seq")
    .loop_bars(16)
    .clip(0..bars(8), voice_fade)
    .clip(0..bars(4), group_fade)
    .clip(0..bars(16), fx_fade)
    .start();
```

---

## 10. Custom Synthesis

Create your own sounds with `define_synthdef`. Create `custom_synth.vibe`:

```rhai
// custom_synth.vibe - Creating custom synths

// Import drums and effects
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";
import "stdlib/effects/reverbs/hall_reverb.vibe";

set_tempo(120);
set_time_signature(4, 4);

// Define a custom synth
define_synthdef("my_bass")
    .param("freq", 55.0)        // Default frequency (A1)
    .param("amp", 0.5)          // Default amplitude
    .param("gate", 1.0)         // Gate for envelope
    .param("cutoff", 800.0)     // Filter cutoff
    .body(|freq, amp, gate, cutoff| {
        // Create an envelope
        let env = envelope()
            .adsr("10ms", "100ms", 0.7, "200ms")
            .gate(gate)
            .cleanup_on_finish()
            .build();

        // Two detuned saw oscillators
        let osc1 = saw_ar(freq);
        let osc2 = saw_ar(freq * 1.01);  // Slightly detuned
        let osc = (osc1 + osc2) * 0.5;

        // Low-pass filter
        let filtered = rlpf_ar(osc, cutoff, 0.3);

        // Output with envelope and amplitude
        filtered * env * amp
    });

// Define a custom lead with vibrato
define_synthdef("my_lead")
    .param("freq", 440.0)
    .param("amp", 0.3)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        // Envelope with longer attack for pads
        let env = envelope()
            .adsr("50ms", "200ms", 0.8, "500ms")
            .gate(gate)
            .cleanup_on_finish()
            .build();

        // Vibrato LFO (low frequency oscillator)
        let vibrato = sin_osc_kr(5.0) * 10.0;  // 5 Hz, ±10 Hz range
        let mod_freq = freq + vibrato;

        // Oscillator stack
        let saw = saw_ar(mod_freq);
        let pulse = pulse_ar(mod_freq * 2.0, 0.5) * 0.3;  // Octave up
        let osc = saw + pulse;

        // Filter with envelope modulation
        let filter_env = envelope()
            .adsr("10ms", "300ms", 0.3, "100ms")
            .gate(gate)
            .build();
        let cutoff = 500.0 + (filter_env * 3000.0);
        let filtered = rlpf_ar(osc, cutoff, 0.2);

        filtered * env * amp
    });

// Use our custom synths
define_group("Bass", || {
    let bass = voice("bass")
        .synth("my_bass")
        .gain(db(-6))
        .poly(1)
        .set_param("cutoff", 600.0);

    melody("bassline")
        .on(bass)
        .notes("A1 . . . | A1 . C2 . | D2 . . . | E2 . D2 C2")
        .start();
});

define_group("Lead", || {
    let lead = voice("lead")
        .synth("my_lead")
        .gain(db(-10))
        .poly(4);

    melody("lead_melody")
        .on(lead)
        .notes("A4 . C5 . | E5 . C5 . | A4 . C5 E5 | G5 - - .")
        .start();

    fx("reverb").synth("hall_reverb").param("room", 0.7).param("mix", 0.3).apply();
});

// Add some drums
define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let hihat = voice("hihat").synth("hihat_808_closed").gain(db(-12));

    pattern("kick").on(kick).step("x... x... x... x...").start();
    pattern("hihat").on(hihat).step(".x .x .x .x").start();
});
```

### Available UGens

| Oscillators | Description |
|------------|-------------|
| `sin_osc_ar(freq)` | Sine wave |
| `saw_ar(freq)` | Sawtooth wave |
| `pulse_ar(freq, width)` | Pulse wave with variable width |
| `lf_tri_ar(freq)` | Triangle wave |
| `white_noise_ar()` | White noise |
| `pink_noise_ar()` | Pink noise |

| Filters | Description |
|---------|-------------|
| `lpf_ar(input, freq)` | Low-pass filter |
| `hpf_ar(input, freq)` | High-pass filter |
| `rlpf_ar(input, freq, res)` | Resonant low-pass |
| `rhpf_ar(input, freq, res)` | Resonant high-pass |

| Modulators (Control Rate) | Description |
|---------------------------|-------------|
| `sin_osc_kr(freq)` | LFO sine wave |

### Envelope Types

```rhai
// Attack-Decay-Sustain-Release
let env = envelope().adsr("10ms", "100ms", 0.7, "200ms").gate(gate).cleanup_on_finish().build();

// Attack-Sustain-Release (for held notes)
let env = envelope().asr("15ms", 0.7, "100ms").gate(gate).cleanup_on_finish().build();

// Percussive (attack-release, no sustain)
let env = envelope().perc("1ms", "50ms").gate(gate).cleanup_on_finish().build();

// Simple attack-release
let env = envelope().attack("5ms").release("200ms").gate(gate).cleanup_on_finish().build();
```

---

## 11. Complete Example

Here's a complete track that combines everything we've learned. Create `complete_track.vibe`:

```rhai
// complete_track.vibe - A complete VibeLang track
// Combines drums, bass, melodies, groups, effects, sequences, and automation

// ============================================================================
// IMPORTS
// ============================================================================

// Drums
import "stdlib/drums/kicks/kick_909.vibe";
import "stdlib/drums/snares/snare_909.vibe";
import "stdlib/drums/hihats/hihat_909_closed.vibe";
import "stdlib/drums/hihats/hihat_909_open.vibe";
import "stdlib/drums/claps/clap_808.vibe";

// Bass & Synths
import "stdlib/pads/ambient/pad_warm.vibe";

// Effects
import "stdlib/effects/reverbs/room_reverb.vibe";
import "stdlib/effects/reverbs/hall_reverb.vibe";
import "stdlib/effects/reverbs/shimmer_reverb.vibe";
import "stdlib/effects/delays/ping_pong_delay.vibe";
import "stdlib/effects/distortion/overdrive.vibe";
import "stdlib/effects/dynamics/compressor.vibe";
import "stdlib/effects/modulation/chorus.vibe";

// ============================================================================
// SETUP
// ============================================================================

set_tempo(124);
set_time_signature(4, 4);

// ============================================================================
// CUSTOM SYNTHS
// ============================================================================

define_synthdef("thick_bass")
    .param("freq", 55.0)
    .param("amp", 0.5)
    .param("gate", 1.0)
    .param("cutoff", 600.0)
    .body(|freq, amp, gate, cutoff| {
        let env = envelope().adsr("5ms", "80ms", 0.8, "150ms").gate(gate).cleanup_on_finish().build();
        let sub = sin_osc_ar(freq) * 0.6;
        let mid = saw_ar(freq * 2.0) * 0.3;
        let top = pulse_ar(freq * 4.0, 0.3) * 0.1;
        let osc = sub + mid + top;
        let filtered = rlpf_ar(osc, cutoff, 0.25);
        filtered * env * amp
    });

define_synthdef("pluck_lead")
    .param("freq", 440.0)
    .param("amp", 0.4)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        let env = envelope().perc("1ms", "400ms").gate(gate).cleanup_on_finish().build();
        let filter_env = envelope().perc("1ms", "150ms").gate(gate).build();
        let osc = saw_ar(freq) + saw_ar(freq * 1.005);
        let cutoff = 300.0 + (filter_env * 4000.0);
        let filtered = rlpf_ar(osc, cutoff, 0.2);
        filtered * env * amp * 0.5
    });

// ============================================================================
// DRUMS GROUP
// ============================================================================

define_group("Drums", || {
    let kick = voice("kick").synth("kick_909").gain(db(-4));
    let snare = voice("snare").synth("snare_909").gain(db(-6));
    let hat_c = voice("hat_c").synth("hihat_909_closed").gain(db(-10));
    let hat_o = voice("hat_o").synth("hihat_909_open").gain(db(-12));
    let clap = voice("clap").synth("clap_808").gain(db(-8));

    // Pattern variations (don't use .start() - they'll be triggered by the sequence)
    let kick_main = pattern("kick_main").on(kick).step("x... .... x... ....");
    let kick_four = pattern("kick_four").on(kick).step("x... x... x... x...");

    let snare_main = pattern("snare_main").on(snare).step(".... x... .... x...");

    let hat_basic = pattern("hat_basic").on(hat_c).step(".x .x .x .x");
    let hat_busy = pattern("hat_busy").on(hat_c).step("x.x. x.x. x.x. x.xx");

    let hat_o_accent = pattern("hat_o_accent").on(hat_o).step(".... .... .... ...x");

    let clap_main = pattern("clap_main").on(clap).step(".... x... .... x...");

    // 64-bar arrangement
    sequence("drums_seq")
        .loop_bars(64)
        // INTRO (0-8): Just hats building
        .clip(bars(4)..bars(8), hat_basic)

        // BUILD (8-16): Add kick
        .clip(bars(8)..bars(16), kick_main)
        .clip(bars(8)..bars(16), hat_basic)

        // DROP 1 (16-32): Full drums
        .clip(bars(16)..bars(32), kick_four)
        .clip(bars(16)..bars(32), snare_main)
        .clip(bars(16)..bars(32), hat_busy)
        .clip(bars(16)..bars(32), hat_o_accent)
        .clip(bars(16)..bars(32), clap_main)

        // BREAKDOWN (32-40): Strip back
        .clip(bars(32)..bars(40), kick_main)
        .clip(bars(32)..bars(40), hat_basic)

        // BUILD 2 (40-48): Building again
        .clip(bars(40)..bars(48), kick_four)
        .clip(bars(40)..bars(48), hat_busy)

        // DROP 2 (48-60): Full drums
        .clip(bars(48)..bars(60), kick_four)
        .clip(bars(48)..bars(60), snare_main)
        .clip(bars(48)..bars(60), hat_busy)
        .clip(bars(48)..bars(60), hat_o_accent)
        .clip(bars(48)..bars(60), clap_main)

        // OUTRO (60-64): Fade out
        .clip(bars(60)..bars(64), kick_main)
        .clip(bars(60)..bars(64), hat_basic)

        .start();

    // Drum bus effects
    fx("drum_comp").synth("compressor").param("threshold", db(-10)).param("ratio", 3.0).param("mix", 0.6).apply();
    fx("drum_verb").synth("room_reverb").param("room", 0.15).param("mix", 0.1).apply();
});

// ============================================================================
// BASS GROUP
// ============================================================================

define_group("Bass", || {
    let bass = voice("bass")
        .synth("thick_bass")
        .gain(db(-8))
        .poly(1)
        .set_param("cutoff", 500.0);

    let bass_main = melody("bass_main").on(bass)
        .notes("E1 . . . | E1 . G1 . | A1 . . . | B1 . A1 G1");

    let bass_var = melody("bass_var").on(bass)
        .notes("E1 E2 . . | E1 . G1 G2 | A1 A2 . . | B1 . . .");

    sequence("bass_seq")
        .loop_bars(64)
        // No bass in intro (0-8)
        // Simple bass in build
        .clip(bars(8)..bars(16), bass_main)
        // Variation in drops
        .clip(bars(16)..bars(32), bass_var)
        // Back to main for breakdown/build
        .clip(bars(32)..bars(48), bass_main)
        // Variation again for second drop
        .clip(bars(48)..bars(60), bass_var)
        // Simple for outro
        .clip(bars(60)..bars(64), bass_main)
        .start();

    fx("bass_saturation").synth("overdrive").param("drive", 0.25).param("mix", 0.35).apply();
});

// ============================================================================
// SYNTH GROUP
// ============================================================================

define_group("Synth", || {
    let lead = voice("lead")
        .synth("pluck_lead")
        .gain(db(-12))
        .poly(4);

    let lead_main = melody("lead_main").on(lead)
        .notes("E4 . G4 . | B4 . G4 . | E4 . G4 B4 | D5 - - .");

    let lead_high = melody("lead_high").on(lead)
        .notes("E5 . . . | D5 . B4 . | G4 - - - | . . . .");

    sequence("lead_seq")
        .loop_bars(64)
        // No lead until first drop (0-16)
        // Main melody in drop
        .clip(bars(16)..bars(24), lead_main)
        .clip(bars(24)..bars(32), lead_high)
        // No lead in breakdown (32-48)
        // Both in second drop
        .clip(bars(48)..bars(56), lead_main)
        .clip(bars(56)..bars(60), lead_high)
        // Fade out (60-64) - no lead
        .start();

    fx("lead_delay").synth("ping_pong_delay")
        .param("time", 0.375)     // Dotted 8th
        .param("feedback", 0.45)
        .param("mix", 0.35)
        .apply();

    fx("lead_reverb").synth("hall_reverb")
        .param("room", 0.65)
        .param("mix", 0.3)
        .apply();
});

// ============================================================================
// PAD GROUP
// ============================================================================

define_group("Pad", || {
    let pad = voice("pad")
        .synth("pad_warm")
        .gain(db(-16))
        .poly(8);

    // Chord stacks - Em chord
    let pad_em = melody("pad_em").on(pad).notes("E3 - - - - - - - | - - - - - - - -");
    let pad_em_3 = melody("pad_em_3").on(pad).notes("G3 - - - - - - - | - - - - - - - -");
    let pad_em_5 = melody("pad_em_5").on(pad).notes("B3 - - - - - - - | - - - - - - - -");

    // C chord
    let pad_c = melody("pad_c").on(pad).notes("C3 - - - - - - - | - - - - - - - -");
    let pad_c_3 = melody("pad_c_3").on(pad).notes("E3 - - - - - - - | - - - - - - - -");
    let pad_c_5 = melody("pad_c_5").on(pad).notes("G3 - - - - - - - | - - - - - - - -");

    sequence("pad_seq")
        .loop_bars(64)
        // No pads in intro (0-8)
        // Em chord during build
        .clip(bars(8)..bars(16), pad_em)
        .clip(bars(8)..bars(16), pad_em_3)
        .clip(bars(8)..bars(16), pad_em_5)
        // Alternate chords in drop
        .clip(bars(16)..bars(24), pad_em)
        .clip(bars(16)..bars(24), pad_em_3)
        .clip(bars(16)..bars(24), pad_em_5)
        .clip(bars(24)..bars(32), pad_c)
        .clip(bars(24)..bars(32), pad_c_3)
        .clip(bars(24)..bars(32), pad_c_5)
        // Breakdown pad
        .clip(bars(32)..bars(48), pad_em)
        .clip(bars(32)..bars(48), pad_em_3)
        .clip(bars(32)..bars(48), pad_em_5)
        // Second drop
        .clip(bars(48)..bars(56), pad_c)
        .clip(bars(48)..bars(56), pad_c_3)
        .clip(bars(48)..bars(56), pad_c_5)
        .clip(bars(56)..bars(64), pad_em)
        .clip(bars(56)..bars(64), pad_em_3)
        .clip(bars(56)..bars(64), pad_em_5)
        .start();

    fx("pad_chorus").synth("chorus").param("rate", 0.3).param("depth", 0.4).param("mix", 0.4).apply();
    fx("pad_reverb").synth("shimmer_reverb").param("room", 0.85).param("mix", 0.45).apply();
});
```

Run it with:

```bash
vibe complete_track.vibe
```

---

## What's Next?

Now that you've completed this guide, here are some things to explore:

### Live Coding

VibeLang supports hot-reloading. While your track is playing:
1. Edit the `.vibe` file
2. Save it
3. Changes apply immediately!

### MIDI Integration

Connect MIDI controllers:

```rhai
let midi = midi_open("vibe");
midi_monitor(true);  // See MIDI messages in console
midi.keyboard().to(voice_object);  // Play a voice with MIDI keyboard
```

### SFZ Instruments

Load sampled instruments:

```rhai
let piano = sfz_voice("piano", "path/to/piano.sfz").gain(db(-6));
```

### Standard Library Reference

Explore the full standard library in `crates/vibelang-std/stdlib/`:
- **477+ sounds** organized by category
- Drums, bass, leads, pads, keys, synths, effects, and more

### Tips for Better Productions

1. **Use groups** to organize and apply effects at the bus level
2. **Create pattern variations** and switch between them with sequences
3. **Automate parameters** for movement and energy
4. **Layer sounds** - combine sub bass with midrange for fuller bass
5. **Use effects sparingly** - less is often more
6. **Reference other tracks** - compare your mix to professional productions

Happy music making with VibeLang!
