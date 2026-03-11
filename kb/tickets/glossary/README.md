---
title: Glossary
id: glossary
status: open
tags:
- epic
labels:
  area: glossary
priority: high
created: 2026-03-11T08:35:38.270270873+01:00
updated: 2026-03-11T08:35:38.270270873+01:00
---

# Glossary

Canonical vocabulary for the VibeLang knowledge base. All tickets use these terms consistently.

Canonical terms used across this knowledge base. All tickets use this vocabulary consistently.

## Core Concepts

| Term | Definition |
|------|-----------|
| **voice** | Named instance of a synthdef (an instrument you can play) |
| **synthdef** | Reusable sound definition (blueprint) |
| **pattern** | Step sequencer for rhythmic triggers |
| **melody** | Note sequence with pitch and duration |
| **sequence** | Timeline arrangement of patterns/melodies/fades |
| **group** | Submix bus bundling voices and effects |
| **effect (fx)** | Audio processor on a group bus |
| **fade** | One-shot parameter ramp (automation) |
| **modulator** | Cyclic control signal (LFO) |

## Audio Terms

| Term | Definition |
|------|-----------|
| **UGen** | Unit Generator — signal building block (oscillator, filter, etc.) |
| **envelope** | Amplitude shape over time (ADSR) |
| **gate** | Control signal: 1.0 = note on, 0.0 = note off |
| **ADSR** | Attack, Decay, Sustain, Release — envelope phases |
| **oscillator** | Sound generator (sine, saw, pulse, triangle, noise) |
| **filter** | Sound shaper (low-pass, high-pass, resonant) |
| **cutoff** | Filter cutoff frequency (Hz) |
| **resonance** | Boost at filter cutoff frequency |
| **LFO** | Low Frequency Oscillator — slow oscillator for modulation |
| **bus** | Internal audio channel for routing |
| **gain** | Volume level (always specify via `db()`) |
| **polyphony** | Number of simultaneous notes a voice can play |
| **detuning** | Slight pitch offset for width/richness |
| **mix** | Wet/dry ratio (0.0 = dry, 1.0 = fully wet) |
| **feedback** | Signal recirculation (in delays) |
| **BPM** | Beats per minute (tempo) |
| **velocity** | Strike intensity (1–9 in step notation, x = max) |

## Technical Terms

| Term | Definition |
|------|-----------|
| **Rhai** | Embedded scripting language (interprets .vibe files) |
| **SuperCollider** | Open-source audio engine |
| **JACK** | Low-latency audio driver |
| **OSC** | Open Sound Control — communication protocol |
| **reconciliation** | State diffing on hot reload |
| **hot reload** | Instant code change application (~1ms) |
| **stdlib** | Standard library (187 bundled synthdefs) |
| **registry** | Global directory of registered synthdefs |
| **SFZ** | Open sample instrument format |
| **MIDI** | Musical Instrument Digital Interface |

## Notation

| Term | Definition |
|------|-----------|
| **step notation** | String for rhythms: `x` (trigger), `.` (rest), `1-9` (velocity) |
| **note notation** | String for melodies: `C4` (note), `-` (hold), `.` (rest), `\|` (bar) |
| **hold (-)** | Extend previous note |
| **rest (.)** | Silence |

## Naming Conventions

| Thing | Convention | Examples |
|-------|-----------|----------|
| Voice names | lowercase | `"kick"`, `"bass"`, `"lead"` |
| Pattern names | descriptive | `"kick_basic"`, `"snare_fill"` |
| Group names | capitalized | `"Drums"`, `"Bass"`, `"Synth"` |
| Synthdef names | snake_case | `"kick_808"`, `"acid_303_classic"` |
| Effect names | descriptive | `"drum_reverb"`, `"lead_delay"` |
