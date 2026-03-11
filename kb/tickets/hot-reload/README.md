---
title: "Hot-Reload (Live-Coding)"
id: hot-reload
status: open
tags: [konzept, workflow]
labels:
  kategorie: kern
  bereich: workflow
priority: high
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Hot-Reload (Live-Coding)

VibeLang unterstützt **~1ms Hot-Reload**: Änderungen an `.vibe`-Dateien werden sofort hörbar, ohne den Audio-Stream zu unterbrechen.

## Workflow

1. `vibe my_song.vibe` starten
2. `.vibe`-Datei im Editor bearbeiten
3. Speichern → Änderungen sind sofort hörbar
4. Fehler im Code unterbrechen das Audio nicht

## Watch-Modus

```bash
vibe my_song.vibe           # Standard: Watch-Modus aktiv
vibe run my_song.vibe -w    # Explizit Watch-Modus
```

## State Reconciliation

Beim Reload wird der Audio-Graph verglichen (Reconciliation):

- Neue Voices/Patterns werden gestartet
- Entfernte Voices/Patterns werden gestoppt
- Geänderte Parameter werden aktualisiert
- Laufende Patterns bleiben im Takt

## Fehlertoleranz

- Syntaxfehler → Fehlermeldung in Konsole, Audio läuft weiter
- Laufzeitfehler → gleich, letzter gültiger Zustand bleibt aktiv

## Vokabular

- **Hot-Reload** = Sofortige Übernahme von Codeänderungen (~1ms)
- **Reconciliation** = Vergleich und Anpassung des Audio-Graphs
- **Watch-Modus** = Automatische Überwachung der Datei auf Änderungen
- **Live-Coding** = Musik durch Programmieren in Echtzeit
