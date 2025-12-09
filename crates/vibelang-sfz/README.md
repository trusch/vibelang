# vibelang-sfz

SFZ (Sound Font Zip) instrument support for VibeLang.

## Overview

`vibelang-sfz` provides complete SFZ instrument loading, parsing, and playback for VibeLang. It enables using sampled instruments defined in the industry-standard SFZ format.

Key features:

- **Full SFZ Parsing** - Comprehensive opcode support via `nom` parser
- **Region Matching** - Note/velocity-based sample selection
- **Round Robin** - Voice cycling and group-based voice stealing
- **SynthDef Generation** - Automatic PlayBuf-based synth creation
- **Pitch Calculation** - Correct pitch transposition using `pitch_keycenter`

## Architecture

```
┌─────────────────────────────────────────┐
│           load_sfz("path")              │
│         (Rhai API entry)                │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│              Parser                     │
│   (nom-based SFZ file parsing)          │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│           SfzInstrument                 │
│    (regions, opcodes, samples)          │
└────────────────┬────────────────────────┘
                 │
         ┌───────┴───────┐
         │               │
         ▼               ▼
┌─────────────┐  ┌─────────────────┐
│   Region    │  │    SynthDef     │
│  Matching   │  │   Generation    │
└─────────────┘  └─────────────────┘
```

## Key Modules

### `parser/` - SFZ File Parsing

Full SFZ format parsing with `nom`:

```rust
use vibelang_sfz::parser::parse_sfz;

let sfz_content = std::fs::read_to_string("instrument.sfz")?;
let parsed = parse_sfz(&sfz_content)?;
```

Supported sections:
- `<control>` - Global settings
- `<global>` - Default values for all regions
- `<group>` - Group-level defaults
- `<region>` - Individual sample regions

### `types/` - Core Data Structures

```rust
use vibelang_sfz::types::{SfzInstrument, SfzRegion, SfzRegionOpcodes};

pub struct SfzInstrument {
    pub name: String,
    pub regions: Vec<SfzRegion>,
}

pub struct SfzRegion {
    pub sample: String,
    pub opcodes: SfzRegionOpcodes,
    pub buf_num: Option<i32>,      // Allocated buffer
    pub buf_frames: Option<u32>,   // Sample length
    pub buf_sample_rate: Option<f32>,
}
```

Supported opcodes:
- `lokey`, `hikey`, `pitch_keycenter` - Pitch mapping
- `lovel`, `hivel` - Velocity layers
- `loop_mode` - `no_loop`, `one_shot`, `loop_continuous`
- `trigger` - `attack`, `release`, `first`, `legato`
- `group`, `off_by` - Voice stealing
- `seq_length`, `seq_position` - Round robin
- `tune`, `transpose` - Fine tuning
- `volume`, `pan` - Level control
- `ampeg_*` - Envelope parameters

### `loader/` - Instrument Loading

Backend-agnostic loading with callback for buffer allocation:

```rust
use vibelang_sfz::loader::load_sfz_instrument;

let instrument = load_sfz_instrument(
    "piano.sfz",
    |sample_path| {
        // Allocate buffer and return buf_num
        Ok(allocate_buffer(sample_path)?)
    }
)?;
```

### `region_matcher/` - Sample Selection

```rust
use vibelang_sfz::region_matcher::{find_matching_regions, RoundRobinState};

let mut rr_state = RoundRobinState::new();

// Find regions for note 60, velocity 100, attack trigger
let regions = find_matching_regions(
    &instrument,
    60,                      // MIDI note
    100,                     // Velocity
    TriggerMode::Attack,
    &mut rr_state,
);
```

### `synthdef/` - SynthDef Generation

Creates PlayBuf-based synthdefs for SFZ regions:

```rust
use vibelang_sfz::synthdef::create_sfz_synthdefs;

let synthdefs = create_sfz_synthdefs(&instrument)?;
// Returns one synthdef per region with correct pitch handling
```

Pitch calculation:
```
rate = target_freq / sample_root_freq
     = midi_to_freq(note) / midi_to_freq(pitch_keycenter)
```

## Usage in VibeLang

```rhai
// Load an SFZ instrument
let piano = load_sfz("piano.sfz");

// Create a voice using it
let keys = voice("keys").sfz(piano).gain(db(-6));

// Use in melodies
melody("chords")
    .on(keys)
    .notes("C4 E4 G4 - | - - - - |")
    .start();
```

## Known Limitations

1. **Missing `pitch_keycenter`** - Some SFZ files don't specify `pitch_keycenter`, causing pitch issues. Workaround: Edit the SFZ file or use filename inference.

2. **NOTE_OFF handling** - Melodies don't auto-schedule NOTE_OFF, which can cause SFZ voices to hang. See `CLAUDE.md` for workarounds.

3. **Opcode coverage** - Not all SFZ opcodes are implemented. Focus is on the most common ones.

## Dependencies

- **vibelang-dsp** - SynthDef generation
- **nom** - Parser combinators
- **rhai** - API types

## License

MIT OR Apache-2.0
