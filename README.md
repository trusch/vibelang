<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="landing-page/assets/logo.svg">
    <source media="(prefers-color-scheme: light)" srcset="landing-page/assets/logo_dark.svg">
    <img src="landing-page/assets/logo_dark.svg" alt="VibeLang" width="340" height="80" />
  </picture>
</p>

<h3 align="center">Make music with code.</h3>

<p align="center">
  <a href="https://crates.io/crates/vibelang-cli"><img src="https://img.shields.io/crates/v/vibelang-cli?style=flat-square&logo=rust&logoColor=white&label=crates.io&color=%23f74c00" alt="Crates.io"></a>
  <a href="https://docs.rs/vibelang-core"><img src="https://img.shields.io/docsrs/vibelang-core?style=flat-square&logo=docs.rs&logoColor=white&label=docs.rs" alt="docs.rs"></a>
  <a href="https://crates.io/crates/vibelang-cli"><img src="https://img.shields.io/crates/l/vibelang-cli?style=flat-square" alt="License"></a>
  <a href="https://github.com/trusch/vibelang/stargazers"><img src="https://img.shields.io/github/stars/trusch/vibelang?style=flat-square&logo=github&color=%23181717" alt="GitHub Stars"></a>
</p>

<p align="center">
  <a href="https://vibelang.org">Website</a> •
  <a href="https://vibelang.org/#docs">Documentation</a> •
  <a href="https://vibelang.org/#demo">Examples</a> •
  <a href="https://github.com/trusch/vibelang/issues">Issues</a>
</p>

---

VibeLang is a programming language for making music. Write beats, melodies, and full tracks in code — then edit, save, and **hear it change instantly**.

```ts
set_tempo(120);

import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/bass/sub/sub_deep.vibe";

let kick = voice("kick").synth("kick_808").gain(db(-6));
let bass = voice("bass").synth("sub_deep").gain(db(-12));

pattern("groove").on(kick).step("x... x... x..x ....").start();
melody("line").on(bass).notes("C3 - - - | C3 - G2 -").start();
```

That's a whole beat. Just run it and edit while it plays.

<br>

## ✨ Features

| | |
|---|---|
| **580+ Built-in Sounds** | Drums, bass, leads, pads, keys, world instruments, effects — all as editable `.vibe` files |
| **~1ms Hot Reload** | Edit your code, save, hear it change. No restart needed. Errors don't kill the audio. |
| **Git-Friendly** | Your music is plain text. Diff it, branch it, collaborate on it. |
| **SuperCollider Powered** | Professional-grade audio engine under the hood |
| **Zero Config** | `cargo install vibelang-cli` and you're ready to make music |

<br>

## 🚀 Quick Start

### Prerequisites

- [SuperCollider](https://supercollider.github.io/) — the audio engine
- [JACK Audio](https://jackaudio.org/) (Linux/Mac) or your system audio

### Install

```bash
cargo install vibelang-cli
```

### Your First Beat

Create `hello.vibe`:

```ts
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

Edit the pattern. Save. Hear it change. **That's the vibe.**

<br>

## 📖 Language Overview

### Patterns — Step Sequencing

```ts
// x = hit, . = rest, 0-9 = velocity levels
pattern("drums")
    .on(kick_voice)
    .step("x...x...x..x....")
    .start();

// Euclidean rhythms
pattern("afro").on(perc).euclid(5, 8).start();
```

### Melodies — Note Sequences

```ts
melody("bassline")
    .on(bass_voice)
    .notes("C2 - - . | E2 - G2 . | A2 - - - | G2 . E2 .")
    .start();

// C4, A#3, Bb2 = pitches  |  - = hold  |  . = rest
```

### Voices & Synths

```ts
let lead = voice("lead")
    .synth("lead_bright")
    .gain(db(-6))
    .poly(4)
    .set_param("cutoff", 2000.0);
```

### Groups & Effects

```ts
let drums = define_group("Drums", || {
    let kick = voice("kick").synth("kick_808");
    let snare = voice("snare").synth("snare_808");

    pattern("kick").on(kick).step("x...x...").start();
    pattern("snare").on(snare).step("....x...").start();

    fx("verb").synth("reverb").param("room", 0.3).apply();
});
```

### Custom Sound Design

```ts
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

<br>

## 🎹 Standard Library

VibeLang comes with **580+ ready-to-use sounds**:

| Category | Examples |
|----------|----------|
| **Drums** (125) | kick_808, snare_909, hihat_closed, clap, toms, percussion |
| **Bass** (75) | sub_deep, acid_303, reese, moog, upright |
| **Leads** (50) | supersaw, pluck, brass, strings |
| **Pads** (41) | warm, shimmer, analog, cinematic |
| **Keys** (19) | grand_piano, rhodes, wurlitzer, hammond |
| **World** (24) | sitar, tabla, kalimba, koto, erhu |
| **Effects** (66) | reverb, delay, chorus, distortion, compressor |

All sounds are plain `.vibe` files — read them, tweak them, learn from them.

<br>

## 📚 Learn More

- **[vibelang.org](https://vibelang.org)** — Full documentation, tutorials, and examples
- **[API Reference](https://docs.rs/vibelang-core)** — Rust API documentation
- **[Examples](https://github.com/trusch/vibelang/tree/main/examples)** — Sample projects and tracks

<br>

## 🛠️ Development Status

VibeLang is in **alpha**. Core features work well, but expect changes.

**Working great:** Patterns, melodies, sequences, hot reload, synthdefs, groups, effects, SFZ instruments

**Experimental:** VST plugins, MIDI input, complex automation

Found a bug? Have an idea? [Open an issue](https://github.com/trusch/vibelang/issues).

<br>

## 💡 Why VibeLang?

- **Text is powerful.** Copy, paste, diff, git, grep. Your music is code.
- **Instant feedback.** Edit-save-hear in milliseconds.
- **Transparent.** Every sound is a readable file. No black boxes.
- **Deep when you need it.** From 4-line beats to full productions.

<br>

## 📄 License

MIT

---

<p align="center">
  <i>Made with 🎵 and loud bass.</i>
</p>
