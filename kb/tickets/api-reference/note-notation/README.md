---
title: Note Notation
id: note-notation
status: open
tags:
- reference
labels:
  area: api
  topic: notation
created: 2026-03-11T08:36:06.144336999+01:00
updated: 2026-03-11T08:36:06.144336999+01:00
---

# Note Notation

Note notation is a string format for defining melodic sequences.

## Characters

| Token | Meaning |
|-------|---------|
| `C4`, `D#3`, `Bb2` | Note with octave (# = sharp, b = flat) |
| `.` | Rest (silence) |
| `-` | Hold previous note (extend duration) |
| `\|` | Bar separator (visual only, ignored) |

## Examples

```
"C2 - - . | E2 - G2 . | A2 - - - | G2 . E2 ."
"C4 . E4 . | G4 . E4 ."
"C3 - - - - - - -"    — whole note held for 8 steps
```

## Note Names

- Natural: C, D, E, F, G, A, B
- Sharp: C#, D#, F#, G#, A#
- Flat: Db, Eb, Gb, Ab, Bb
- Octave: 0–8 (C4 = middle C, A4 = 440 Hz)
