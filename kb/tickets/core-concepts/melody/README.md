---
title: Melody
id: melody
status: open
tags:
- concept
labels:
  area: core
  topic: melody
created: 2026-03-11T08:35:45.012830018+01:00
updated: 2026-03-11T08:35:45.012830018+01:00
---

# Melody

A **melody** defines a sequence of pitched notes. Unlike a pattern (rhythm only), a melody specifies _which_ notes to play.

## Creation

```rhai
melody("bassline")
    .on(bass_voice)
    .notes("C2 - - . | E2 - G2 . | A2 - - - | G2 . E2 .")
    .start();
```

## Methods

- `melody("name")` — create with unique name
- `.on(voice)` — assign a voice
- `.notes("notation")` — define note sequence using note notation
- `.start()` — start immediately (loops)

## Note Notation

- `C4`, `D#3`, `Bb2` — note name with octave (# = sharp, b = flat)
- `.` — rest (silence)
- `-` — hold previous note
- `|` — bar separator (visual only, ignored)

## Chords

Stack multiple melodies on a polyphonic voice:

```rhai
let pad = voice("pad").synth("pad_warm").poly(8);
melody("root").on(pad).notes("C3 - - - - - - -").start();
melody("third").on(pad).notes("E3 - - - - - - -").start();
melody("fifth").on(pad).notes("G3 - - - - - - -").start();
```

## Without .start()

Melodies without `.start()` are controlled by a **sequence**.
