# vibelang-core

Core types, state management, and runtime orchestration for VibeLang.

## Overview

`vibelang-core` is the central crate that powers VibeLang's runtime. It provides:

- **State Management** - Message-passing architecture for managing voices, patterns, melodies, groups, and effects
- **Event Scheduling** - Beat-accurate event scheduling with lookahead for sample-perfect timing
- **Rhai API** - Comprehensive scripting API exposed to `.vibe` files
- **SuperCollider Integration** - OSC client for communicating with scsynth
- **MIDI Support** - MIDI input routing and callback system
- **Transport & Timing** - Tempo, time signatures, and beat-based timing primitives

## Architecture

```
┌─────────────────────────────────────────┐
│            RuntimeHandle                │
│   (Thread-safe API entry point)         │
└────────────────┬────────────────────────┘
                 │
    ┌────────────┼────────────┐
    │            │            │
    ▼            ▼            ▼
┌────────┐ ┌──────────┐ ┌─────────┐
│ State  │ │ Scheduler│ │   OSC   │
│Manager │ │          │ │ Client  │
└────────┘ └──────────┘ └─────────┘
    │            │            │
    └────────────┼────────────┘
                 │
                 ▼
         ┌───────────────┐
         │   scsynth     │
         │(SuperCollider)│
         └───────────────┘
```

## Key Modules

### `api/` - Rhai Scripting API

Exposes functions to `.vibe` scripts:

- `global.rs` - `set_tempo()`, `set_time_signature()`, `quantize()`
- `voice.rs` - `voice()`, `.synth()`, `.gain()`, `.poly()`
- `pattern.rs` - `pattern()`, `.step()`, `.euclid()`, `.start()`
- `melody.rs` - `melody()`, `.notes()`, `.gate()`, `.start()`
- `sequence.rs` - `sequence()`, `.clip()`, `.loop_bars()`
- `group.rs` - `define_group()`, `fx()`
- `synthdef.rs` - SynthDef registration
- `sample.rs` - `sample()` loading and playback
- `sfz.rs` - SFZ instrument loading
- `midi.rs` - MIDI input routing and callbacks
- `helpers.rs` - `db()`, `note()`, `bars()`, note parsing

### `state/` - State Management

Message-passing state system:

```rust
use vibelang_core::state::{StateManager, StateMessage};

// All mutations go through messages
let msg = StateMessage::SetTempo(120.0);
state_manager.send(msg)?;
```

### `timing/` - Beat & Time Primitives

```rust
use vibelang_core::timing::{BeatTime, TimeSignature};

let beat = BeatTime::from_beats(4.5);  // 4 and a half beats
let sig = TimeSignature::new(4, 4);    // 4/4 time
```

### `scheduler/` - Event Scheduling

Beat-accurate scheduling with pattern/melody/sequence support:

```rust
use vibelang_core::scheduler::EventScheduler;

let scheduler = EventScheduler::new();
scheduler.schedule_pattern(&pattern, state);
let due_events = scheduler.collect_due(current_beat, lookahead);
```

### `runtime/` - Main Runtime

Spawns and manages the runtime thread:

```rust
use vibelang_core::runtime::{Runtime, RuntimeHandle};

let runtime = Runtime::new(config)?;
let handle = runtime.handle();
// Use handle in API functions
```

## Usage

This crate is typically used through `vibelang-cli`, but can be embedded:

```rust
use vibelang_core::{Runtime, RuntimeConfig};

let config = RuntimeConfig::default();
let runtime = Runtime::new(config)?;

// Execute a script
runtime.execute_script("set_tempo(120);")?;
```

## Dependencies

- **vibelang-dsp** - SynthDef generation
- **vibelang-sfz** - SFZ instrument support
- **rhai** - Scripting engine
- **rosc** - OSC protocol
- **jack**, **midir** - MIDI I/O
- **aubio-rs** - Audio analysis (BPM detection)

## License

MIT OR Apache-2.0
