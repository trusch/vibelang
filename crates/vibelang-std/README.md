# vibelang-std

Standard library of VibeLang sound design files.

## Overview

`vibelang-std` provides a comprehensive collection of ready-to-use `.vibe` instrument and effect files. The library is embedded in the binary at compile time and automatically extracted to the user's data directory on first run.

## Sound Library

**580+ sounds** organized by category:

| Category | Count | Description |
|----------|-------|-------------|
| **drums/** | 125 | Kicks, snares, hihats, toms, cymbals, percussion, claps, drum machines, breakbeats, foley, latin |
| **bass/** | 75 | Sub bass, acid, reese, moog, acoustic, pluck, genre-specific |
| **leads/** | 50 | Classic synth leads, modern leads, organic leads, pluck variations |
| **pads/** | 41 | Analog pads, cinematic, textural, movement-based |
| **keys/** | 19 | Grand piano, rhodes, wurlitzer, hammond, harpsichord, mellotron, organ |
| **synths/** | 16 | Classic synths: minimoog, juno, jupiter, tb303, ms20, cs80, prophet |
| **world/** | 24 | Sitar, tabla, kalimba, koto, oud, erhu, didgeridoo, hang drum |
| **orchestral/** | 28 | Strings, brass, woodwinds, timpani, bells |
| **strings/** | 4 | Acoustic guitar, electric guitar, bass, string ensemble |
| **brass/** | 1 | Brass section |
| **woodwinds/** | 1 | Oboe |
| **vocals/** | 8 | Vocal pads and formants |
| **fx/** | 20 | Risers, downers, sweeps, transitions, impacts |
| **cinematic/** | 4 | Braam, impact, drone, whoosh |
| **effects/** | 66 | Delays, reverbs, filters, modulation, dynamics, distortion, spatial |
| **utility/** | 6 | Noise generators, click track, tuner, silence |
| **theory/** | - | Scales, chords, progressions helpers |

## Directory Structure

```
stdlib/
├── index.vibe           # Main import (loads all)
├── drums/
│   ├── index.vibe
│   ├── kicks/
│   ├── snares/
│   ├── hihats/
│   └── ...
├── bass/
│   ├── index.vibe
│   ├── acoustic/
│   ├── synth/
│   └── ...
├── leads/
├── pads/
├── keys/
├── synths/
├── effects/
│   ├── index.vibe
│   ├── delays/
│   ├── reverbs/
│   ├── filters/
│   └── ...
├── fx/
│   ├── risers/
│   ├── downers/
│   ├── sweeps/
│   └── transitions/
└── ...
```

## Usage

```rhai
// Import specific sounds
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/bass/sub/sub_deep.vibe";

// Or import entire categories
import "stdlib/drums/index.vibe";

// Use the synths
let kick = voice("kick").synth("kick_808").gain(db(-6));
```

## Installation Path

The stdlib is automatically extracted to:

- **Linux**: `~/.local/share/vibelang/stdlib/`
- **macOS**: `~/Library/Application Support/vibelang/stdlib/`
- **Windows**: `C:\Users\<User>\AppData\Roaming\vibelang\stdlib\`

## API

```rust
use vibelang_std::{stdlib_path, ensure_stdlib_extracted, embedded_stdlib};

// Get the stdlib path (extracts if needed)
let path = stdlib_path()?;

// Force extraction
ensure_stdlib_extracted()?;

// Access embedded files directly
let dir = embedded_stdlib();
```

## Version Management

The stdlib is re-extracted automatically when:
- The version changes (checked via `.version` file)
- Files are missing
- User deletes the directory

## Customization

All sounds are plain `.vibe` files - you can:
- Copy them to your project and modify
- Use them as templates for your own sounds
- Read them to learn synthesis techniques

Example sound file (`kick_808.vibe`):
```rhai
define_synthdef("kick_808")
    .param("freq", 55.0)
    .param("amp", 0.8)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        let pitch_env = env_perc(0.001, 0.15);
        let pitch_eg = env_gen(pitch_env, gate, 0.0);
        let osc_freq = freq + (freq * 4.0 * pitch_eg);

        let osc = sin_osc_ar(osc_freq);

        let amp_env = env_perc(0.001, 0.4);
        let amp_eg = env_gen(amp_env, gate, 2.0);

        osc * amp_eg * amp
    });
```

## Dependencies

- **include_dir** - Compile-time file embedding
- **dirs** - Cross-platform directory paths

## License

MIT OR Apache-2.0
