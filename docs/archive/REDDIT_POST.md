# r/rust post

**Title:** VibeLang v0.4 — a programming language for making music, written in Rust

---

Hey r/rust! VibeLang is a language for making music with code — write beats, melodies, and full tracks, then edit and hear changes instantly via hot-reload.

```rust
set_tempo(120);

let kick = voice("kick").synth("kick_808").gain(db(-6));
let bass = voice("bass").synth("sub_deep").gain(db(-12));

pattern("groove").on(kick).step("x... x... x..x ....").start();
melody("line").on(bass).notes("C3 - - - | C3 - G2 -").start();
```

**v0.4** is a stability release after a thorough code review:

- SFZ sample instruments now participate in hot-reload diffing (previously they were outside the system entirely)
- Fixed a bug where sequences kept playing forever when you removed `.start()` during live coding
- Entity ID hash collision detection (FNV-1a 32-bit was silently overwriting on collision)
- Input validation across the API (tempo, time signature, fade durations, pattern lengths)
- Eliminated all `unwrap()` in the core crate, replaced with proper error handling
- MIDI channels now use 1-16 (musician convention) instead of 0-15

The architecture: Rust workspace with 10 crates, Rhai scripting engine for the `.vibe` files, SuperCollider-style synthesis with JACK audio backend. The hot-reload works via diff-based state reconciliation — it compares old vs new script state and only updates what changed, quantized to musical boundaries.

- GitHub: https://github.com/trusch/vibelang
- Website: https://vibelang.org
- crates.io: https://crates.io/crates/vibelang-cli

`cargo install vibelang-cli` to try it. Needs JACK or PipeWire with JACK support for audio output.

Happy to answer questions about the architecture or the DSP side of things.

---

# r/musicprogramming post

**Title:** VibeLang v0.4 — write music in code with instant hot-reload

---

VibeLang is a programming language for making music. You write `.vibe` scripts that define instruments, patterns, and melodies — then edit them while the music plays and hear changes instantly.

```
set_tempo(140);

import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/bass/sub/sub_deep.vibe";

let kick = voice("kick").synth("kick_808").gain(db(-6));
let bass = voice("bass").synth("sub_deep").gain(db(-12));

pattern("beat").on(kick).step("x... x... x..x ....").start();
melody("bassline").on(bass).notes("C2 - - Eb2 | C2 - G1 -").start();
```

v0.4 focuses on reliability — SFZ instruments now hot-reload properly, sequences actually stop when you remove them, and there's proper input validation across the API so you get warnings instead of silent failures.

It also supports SFZ sample libraries, MIDI output, an LSP for editor integration (VS Code + Emacs), and a built-in standard library of synths and drum sounds.

- Website with demos: https://vibelang.org
- GitHub: https://github.com/trusch/vibelang
- Install: `cargo install vibelang-cli` (needs Rust + JACK/PipeWire)
