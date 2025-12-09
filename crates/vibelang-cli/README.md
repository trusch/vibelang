# vibelang-cli

Command-line interface for VibeLang - the `vibe` command.

## Overview

`vibelang-cli` is the main entry point for VibeLang. It provides:

- **Live Coding** - Run `.vibe` files with automatic hot-reload on save
- **TUI Mode** - Full terminal UI with editor, transport, and visualization
- **Offline Rendering** - Convert `.vibescore` archives to audio files
- **Virtual Keyboard** - Play MIDI notes from your computer keyboard

## Installation

```bash
# Build from source
cargo build --release

# The binary is at target/release/vibe
```

## Commands

### Run (default)

```bash
# Run a file with watch mode (default)
vibe my_song.vibe

# Same as above, explicit
vibe run my_song.vibe

# Run without watching
vibe run --no-watch my_song.vibe

# Run with TUI
vibe run --tui my_song.vibe

# Record to .vibescore archive
vibe run --record my_song.vibe

# Record to specific path
vibe run --record=output.vibescore my_song.vibe

# Exit after a sequence completes
vibe run --exit-after-sequence song my_song.vibe
```

### Render

Convert `.vibescore` archives to audio:

```bash
# Render to WAV (default)
vibe render recording.vibescore output.wav

# Specify format
vibe render recording.vibescore output.mp3
vibe render recording.vibescore output.flac
vibe render recording.vibescore output.ogg

# Custom sample rate and bit depth
vibe render --sample-rate 96000 --bit-depth 32 recording.vibescore output.wav

# Add tail time for reverb decay
vibe render --tail-time 3.0 recording.vibescore output.wav
```

## TUI Features

When running with `--tui`:

```
┌─────────────────────────────────────────────────────────────┐
│                        VibeLang TUI                         │
├─────────────────────────────────────────────────────────────┤
│ ▶ 120 BPM | 4/4 | Beat: 16.5 | Bar: 5                      │
├────────────────────────────┬────────────────────────────────┤
│                            │ Console                        │
│   Code Editor              │ > Script loaded                │
│                            │ > Pattern "kick" started       │
│   set_tempo(120);          │ > Melody "bass" started        │
│   ...                      │                                │
│                            │                                │
├────────────────────────────┴────────────────────────────────┤
│ Voices: kick[♪] snare[♪] bass[♪]                           │
├─────────────────────────────────────────────────────────────┤
│ ┌─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┐          │
│ │ │█│ │█│ │ │█│ │█│ │█│ │ │█│ │█│ │ │█│ │█│ │█│ │          │
│ └─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┘          │
│   C4      D4      E4      F4      G4      A4      B4       │
└─────────────────────────────────────────────────────────────┘
```

Features:
- Live code display with syntax highlighting
- Console output and log messages
- Transport controls (play/pause/stop)
- Voice activity indicators
- Virtual piano keyboard for MIDI input
- Real-time state visualization

### TUI Keyboard Shortcuts

- `q` / `Ctrl+C` - Quit
- `Space` - Play/Pause
- `r` - Reload script
- `z`/`x` - Octave down/up
- Piano keys mapped to computer keyboard

## Architecture

```
main.rs
├── run command
│   ├── Runtime initialization
│   ├── Script execution
│   ├── Watch mode (file change detection)
│   └── TUI mode (optional)
└── render command
    ├── Score archive extraction
    ├── Offline SuperCollider rendering
    └── Audio file encoding
```

### Key Modules

- `main.rs` - CLI parsing and command routing
- `tui/mod.rs` - TUI orchestration
- `tui/app.rs` - Application state machine
- `tui/ui.rs` - Ratatui rendering
- `tui/keyboard.rs` - Virtual keyboard handling
- `tui/layout.rs` - Panel layout
- `render.rs` - Offline rendering pipeline

## Offline Rendering

The render command:

1. Extracts `.vibescore` archive (contains OSC events, synthdefs, samples)
2. Starts scsynth in non-realtime mode
3. Replays OSC events with accurate timing
4. Records output to audio file

Supported formats:
- WAV (default, highest quality)
- FLAC (lossless compression)
- MP3 (requires ffmpeg)
- OGG (Vorbis)

## Dependencies

- **vibelang-core** - Runtime and API
- **vibelang-dsp** - SynthDef generation
- **vibelang-std** - Standard library
- **rhai** - Scripting engine
- **clap** - CLI parsing
- **ratatui** / **crossterm** - TUI framework
- **rdev** - OS keyboard input
- **rosc** - OSC protocol
- **tar** - Score archives

## Examples

```bash
# Quick start
vibe examples/minimal_techno/main.vibe

# Full TUI experience
vibe run --tui examples/full_track.vibe

# Record a performance
vibe run --record --exit-after-sequence main track.vibe

# Render to high-quality audio
vibe render track.vibescore track.wav
```

## Environment Variables

- `RUST_LOG=debug` - Enable debug logging
- `RUST_LOG=info` - Enable info logging

## License

MIT OR Apache-2.0
