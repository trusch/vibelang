# vibelang-keys

Terminal MIDI keyboard for VibeLang - play MIDI from your computer keyboard.

## Overview

`vibelang-keys` is a terminal application that turns your computer keyboard into a MIDI controller. It features:

- **Visual Piano** - See which keys are pressed in real-time
- **JACK MIDI Output** - Connect to VibeLang, SuperCollider, or any JACK-compatible software
- **Multiple Layouts** - QWERTY, QWERTZ, and custom layouts
- **Reliable Key Detection** - OS-level input for proper key release handling
- **Configurable** - TOML configuration file

## Installation

```bash
# Build from source
cargo build --release -p vibelang-keys

# Binary at target/release/vibe-keys
```

## Quick Start

```bash
# Start with default settings (German QWERTZ layout)
vibe-keys

# Use US QWERTY layout
vibe-keys --us-layout

# Start at a different octave (0-9)
vibe-keys --octave 4

# Use custom JACK client name
vibe-keys --client-name keyboard

# Create a default config file
vibe-keys init

# Show config file path
vibe-keys config-path

# List available JACK MIDI ports
vibe-keys list-ports
```

## Keyboard Layout

The keyboard maps your computer keys to piano keys:

**Lower octave (bottom row + home row):**
```
     S     F G     J K L        (black keys - home row)
     A#2   C#D#    F#G#A#
    Y   X   C   V   B   N   M   ,   .   -   (white keys - bottom row)
    A2  B2  C3  D3  E3  F3  G3  A3  B3  C4
```

**Upper octave (QWERTY row + number row):**
```
     1   2     4   5   6        (black keys - number row)
     C#4 D#4   F#4 G#4 A#4
    Q   W   E   R   T   Z   U   (white keys - QWERTY row)
    D4  E4  F4  G4  A4  B4  C5
```

### Controls

- `<` / `>` or Arrow keys - Octave down/up
- `Esc` or `Ctrl+C` - Quit

## Visual Display

```
┌────────────────────────────────────────────────────────────┐
│                      vibe-keys                              │
│                    Octave: 4  Vel: 100                      │
├────────────────────────────────────────────────────────────┤
│ ┌─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┐        │
│ │ │█│ │█│ │ │█│ │█│ │█│ │ │█│ │█│ │ │█│ │█│ │█│ │ │        │
│ │ │█│ │█│ │ │█│ │█│ │█│ │ │█│ │█│ │ │█│ │█│ │█│ │ │        │
│ │ └┬┘ └┬┘ │ └┬┘ └┬┘ └┬┘ │ └┬┘ └┬┘ │ └┬┘ └┬┘ └┬┘ │ │        │
│ │  │   │  │  │   │   │  │  │   │  │  │   │   │  │ │        │
│ │ ▓│  ▓│  │  │  ▓│   │  │  │   │  │  │   │   │  │ │        │
│ └──┴───┴──┴──┴───┴───┴──┴──┴───┴──┴──┴───┴───┴──┴─┘        │
│   C4      D4      E4  F4      G4      A4      B4  C5       │
└────────────────────────────────────────────────────────────┘

▓ = Currently pressed keys
```

## Configuration

Config file location:
- **Linux**: `~/.config/vibe-keys/config.toml`
- **macOS**: `~/Library/Application Support/vibe-keys/config.toml`
- **Windows**: `C:\Users\<User>\AppData\Roaming\vibe-keys\config.toml`

Create a config file with `vibe-keys init`, then edit it:

```toml
[keyboard]
layout = "german"  # or "us" or "custom"
base_note = 48     # C3
velocity = 100
channel = 0
note_release_ms = 400

[midi]
client_name = "vibe-keys"
port_name = "midi_out"
# auto_connect = ["a2j:Hydrogen"]

[theme]
white_key_color = "white"
black_key_color = "dark_gray"
pressed_key_color = "cyan"
border_color = "cyan"
show_note_names = true
show_help = true
```

## Use with VibeLang

```bash
# In one terminal, start the keyboard
vibe-keys --client-name keyboard

# In another terminal, run VibeLang with MIDI input
vibe --midi-input keyboard my_song.vibe
```

## Library Usage

```rust
use vibelang_keys::{VirtualKeyboard, KeyboardConfig, MidiOutput};

// Create a keyboard
let mut keyboard = VirtualKeyboard::new(KeyboardConfig::german_layout());

// Handle key events
if let Some((note, velocity)) = keyboard.key_down('c') {
    println!("Note on: {} velocity {}", note, velocity);
}

if let Some(note) = keyboard.key_up('c') {
    println!("Note off: {}", note);
}

// Use with MIDI output
use vibelang_keys::midi::JackMidiOutput;

let midi = JackMidiOutput::new("my-app", "midi_out").unwrap();
midi.note_on(0, 60, 100);  // Play middle C
midi.note_off(0, 60);
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                     Main Loop                        │
└───────────────┬─────────────────────────────────────┘
                │
    ┌───────────┼───────────┐
    │           │           │
    ▼           ▼           ▼
┌────────┐ ┌─────────┐ ┌─────────┐
│Terminal│ │   OS    │ │  JACK   │
│ Input  │ │Keyboard │ │  MIDI   │
│        │ │Listener │ │ Output  │
└───┬────┘ └────┬────┘ └────┬────┘
    │           │           │
    └─────┬─────┘           │
          │                 │
          ▼                 │
    ┌───────────┐           │
    │  Virtual  │───────────┘
    │ Keyboard  │
    │  State    │
    └─────┬─────┘
          │
          ▼
    ┌───────────┐
    │    TUI    │
    │  Render   │
    └───────────┘
```

### Key Modules

- `keyboard/` - Virtual keyboard state and key mapping
- `midi/` - JACK MIDI output
- `ui/` - Terminal visualization
- `os_keyboard/` - OS-level key detection (for reliable release events)
- `config/` - Configuration loading and saving

## Dual Input System

Terminal input alone has issues with key release detection. `vibe-keys` uses a dual approach:

1. **Terminal events** (crossterm) - Primary input, fast
2. **OS keyboard listener** (rdev) - Background thread for reliable key releases

This ensures notes are released properly even when terminal input is unreliable.

## JACK Connection

Connect `vibe-keys` to your MIDI-capable software:

```bash
# Using JACK CLI
jack_connect vibe-keys:midi_out system:midi_playback_1

# Or use QjackCtl/Carla for graphical patching
```

## Requirements

- JACK audio server (for MIDI output)
- X11 or Wayland (for OS-level keyboard input on Linux)

## Dependencies

- **ratatui** / **crossterm** - TUI framework
- **jack** - JACK MIDI
- **rdev** - OS keyboard events
- **serde** / **toml** - Configuration
- **directories** - Config file paths
- **clap** - CLI parsing

## License

MIT OR Apache-2.0
