---
title: "Glossar (Einheitliches Vokabular)"
id: glossar
status: open
tags: [referenz, vokabular]
labels:
  kategorie: referenz
  bereich: vokabular
priority: critical
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Glossar — Einheitliches Vokabular

Dieses Glossar definiert die kanonischen Begriffe für die VibeLang Knowledge Base. Alle Einträge verwenden diese Begriffe konsistent.

## Kern-Konzepte

| Begriff | Definition | Englisch |
|---------|-----------|----------|
| **Voice** | Benannte Instanz eines Synthesizers (Instrument) | Voice |
| **Synthdef** | Wiederverwendbare Klangdefinition (Bauplan) | Synth Definition |
| **Pattern** | Step-Sequencer für rhythmische Trigger | Pattern |
| **Melody** | Notensequenz mit Tonhöhe und Dauer | Melody |
| **Sequence** | Zeitliche Anordnung (Arrangement) | Sequence |
| **Group** | Mischgruppe / Submix-Bus | Group |
| **Effect (fx)** | Audioeffekt auf einem Gruppen-Bus | Effect |
| **Fade** | Einmalige Parameter-Rampe (Automation) | Fade |
| **Modulator** | Zyklisches Steuersignal (LFO) | Modulator |

## Audio-Begriffe

| Begriff | Definition |
|---------|-----------|
| **UGen** | Unit Generator — Signalbaustein (Oszillator, Filter, etc.) |
| **Hüllkurve (Envelope)** | Lautstärkeverlauf über Zeit (ADSR) |
| **Gate** | Steuersignal: 1.0 = Note an, 0.0 = Note aus |
| **ADSR** | Attack, Decay, Sustain, Release — Hüllkurven-Phasen |
| **Oszillator** | Klangerzeuger (Sinus, Sägezahn, Puls, Dreieck, Rauschen) |
| **Filter** | Klangformer (Tiefpass, Hochpass, resonant) |
| **Cutoff** | Grenzfrequenz eines Filters (Hz) |
| **Resonanz** | Verstärkung an der Filter-Grenzfrequenz |
| **LFO** | Low Frequency Oscillator — langsamer Oszillator für Modulation |
| **Bus** | Audio-Kanal für internes Routing |
| **Gain** | Lautstärke (immer über `db()` angeben) |
| **Polyphonie** | Anzahl gleichzeitig spielbarer Noten |
| **Detuning** | Leichte Verstimmung für Klangbreite |
| **Mix (Wet/Dry)** | Mischverhältnis Original/Effekt (0.0-1.0) |
| **Feedback** | Rückkopplung (bei Delays) |
| **BPM** | Beats Per Minute — Tempo |

## Notations-Begriffe

| Begriff | Definition |
|---------|-----------|
| **Step-Notation** | Zeichenkette für Rhythmen: `x` (Trigger), `.` (Pause), `1-9` (Velocity) |
| **Noten-Notation** | Zeichenkette für Melodien: `C4` (Note), `-` (Hold), `.` (Pause), `\|` (Taktstrich) |
| **Velocity** | Anschlagstärke (1-9 in Step-Notation, x = Maximum) |
| **Hold (-)** | Vorherige Note verlängern |
| **Rest (.)** | Pause / Stille |

## Technische Begriffe

| Begriff | Definition |
|---------|-----------|
| **Rhai** | Eingebettete Skriptsprache (interpretiert .vibe-Dateien) |
| **SuperCollider** | Open-Source Audio-Engine |
| **JACK** | Low-Latency Audio-Treiber |
| **OSC** | Open Sound Control — Kommunikationsprotokoll |
| **Reconciliation** | Zustandsabgleich beim Hot-Reload |
| **Hot-Reload** | Sofortige Übernahme von Codeänderungen (~1ms) |
| **stdlib** | Standard-Bibliothek (187 mitgelieferte Synthdefs) |
| **Registry** | Globales Verzeichnis registrierter Synthdefs |
| **SFZ** | Offenes Sample-Instrument-Format |
| **MIDI** | Musical Instrument Digital Interface |
| **LSP** | Language Server Protocol (Editor-Unterstützung) |

## Namenskonventionen

- **Voice-Namen**: Kleinbuchstaben, beschreibend: `"kick"`, `"bass"`, `"lead"`
- **Pattern-Namen**: Beschreibend mit Kontext: `"kick_basic"`, `"snare_fill"`
- **Group-Namen**: Großbuchstabe am Anfang: `"Drums"`, `"Bass"`, `"Synth"`
- **Synthdef-Namen**: snake_case: `"kick_808"`, `"acid_303_classic"`, `"my_bass"`
- **Effect-Namen**: Beschreibend: `"drum_reverb"`, `"lead_delay"`
