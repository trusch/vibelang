# VibeLang Features and Test Coverage

This document catalogs all features in VibeLang (vibelang-core2) and tracks test coverage for each feature.

## Table of Contents

1. [Feature Areas](#feature-areas)
2. [Unit Test Coverage](#unit-test-coverage)
3. [Integration Tests](#integration-tests)
4. [Edge Cases and Known Issues](#edge-cases-and-known-issues)
5. [Test Runner Usage](#test-runner-usage)

---

## Feature Areas

### 1. Transport

**Description**: Controls global tempo, time signature, and playback state.

**API Functions**:
- `set_tempo(bpm)` - Set tempo in beats per minute (1.0-999.0)
- `get_tempo()` - Get current tempo
- `set_time_signature(numerator, denominator)` - Set time signature (e.g., 4/4, 3/4)
- `get_current_bar()` - Get current bar number
- `set_quantization(beats)` - Set quantization grid
- `get_quantization()` - Get quantization grid
- `start()` - Start transport
- `stop()` - Stop transport

**Unit Tests**: `crates/vibelang-core2/src/handlers/transport.rs`
- Tempo validation (min/max limits)
- Time signature changes
- Bar calculation

**Integration Tests**: `tests/integration/test_01_transport.vibe`
- Tempo set/get
- Time signature set
- Quantization set/get
- Transport state

---

### 2. Groups

**Description**: Hierarchical audio bus groups for mixing and routing.

**API Functions**:
- `define_group(name, body)` - Define a new group with contents
- `group(name)` - Get existing group by name
- `group.mute()` / `group.unmute()` - Mute/unmute group
- `group.solo(bool)` - Solo/unsolo group
- `group.gain(value)` - Set group gain
- `group.set_param(name, value)` - Set custom parameter

**Unit Tests**: `crates/vibelang-core2/src/handlers/groups.rs`
- Group creation
- Group hierarchy
- Audio bus allocation

**Integration Tests**: `tests/integration/test_05_groups_effects.vibe`
- Group creation within define_group
- Nested groups (planned)
- Group mute/solo

---

### 3. Voices

**Description**: Sound-producing synth voices with polyphony.

**API Functions**:
- `voice(name)` - Create voice builder
- `.synth(synthdef)` - Set synth definition
- `.gain(value)` - Set gain (use `db()` helper)
- `.poly(count)` - Set polyphony (1-128)
- `.param(name, value)` - Set parameter
- `.mute()` / `.unmute()` - Mute/unmute voice
- `.solo()` / `.unsolo()` - Solo/unsolo voice
- `.choke_group(name)` - Set choke group (exclusive triggering)
- `.round_robin(count)` - Set round-robin count for samples
- `.apply()` - Apply configuration (required)

**Unit Tests**: `crates/vibelang-core2/src/handlers/voices.rs`
- Voice creation
- Polyphony limits
- Choke group behavior
- Round-robin cycling

**Integration Tests**: `tests/integration/test_02_voices_patterns.vibe`
- Voice creation with polyphony
- Multiple voices in group
- Mute/solo states

---

### 4. Patterns

**Description**: Rhythmic step sequencers for triggering voices.

**API Functions**:
- `pattern(name)` - Create pattern builder
- `.on(voice)` - Set target voice
- `.step(notation)` - Set step notation (e.g., "x... x...")
- `.euclid(hits, steps)` - Generate Euclidean rhythm
- `.apply()` - Apply configuration without starting
- `.start()` - Start pattern
- `.stop()` - Stop pattern
- `.launch()` - Quantized start

**Step Notation**:
- `x` - Trigger
- `.` - Rest
- `|` - Bar separator
- `x[param=value]` - Trigger with parameters
- `-` - Sustain/tie (in melodies)

**Key Implementation Details** (`handlers/patterns.rs:72-100`):
- Normal case: Steps at `last_pos <= step < new_pos` are triggered
- Wrap case: Steps at `step >= last_pos || step <= new_pos` are triggered
- First tick: Steps at `step == new_pos` are triggered

**Unit Tests**: `crates/vibelang-rhai/src/api/pattern.rs`
- Step notation parsing
- Euclidean rhythm generation
- Pattern length calculation

**Integration Tests**: `tests/integration/test_02_voices_patterns.vibe`
- Basic step patterns
- Pattern with parameters
- Multi-bar patterns

---

### 5. Melodies

**Description**: Pitched note sequences with duration and velocity.

**API Functions**:
- `melody(name)` - Create melody builder
- `.on(voice)` - Set target voice
- `.notes(notation)` - Set notes (string or array)
- `.add_note(beat, note, velocity, duration)` - Add single note
- `.add_chord(beat, notes, velocity, duration)` - Add chord
- `.transpose(semitones)` - Transpose melody
- `.apply()` - Apply configuration without starting
- `.start()` - Start melody
- `.stop()` - Stop melody
- `.launch()` - Quantized start

**Note Notation**:
- `C4 D4 E4` - Notes by name (with octave)
- `C#4` / `Db4` - Sharps and flats
- `-` - Sustain previous note
- `.` - Rest
- `|` - Bar separator
- `C3:maj` - Chord notation

**Key Implementation Details** (`handlers/melodies.rs:97-125`):
- Normal case: Notes at `last_pos < note <= new_pos` are triggered
- Wrap case: Notes at `note >= last_pos || note <= new_pos` are triggered
- First tick: Notes at `note == new_pos` are triggered
- **IMPORTANT**: Pattern and Melody use **different boundary conditions**!

**Unit Tests**: `crates/vibelang-rhai/src/api/melody.rs`
- Note parsing
- Duration calculation
- Transposition

**Integration Tests**: `tests/integration/test_03_melodies.vibe`
- Note name parsing
- Chord notation
- Array input

---

### 6. Sequences

**Description**: Arrangements of clips (patterns, melodies, fades).

**API Functions**:
- `sequence(name)` - Create sequence builder
- `.loop_bars(bars)` - Set loop length in bars
- `.add_clip(item, start, end)` - Add pattern/melody clip
- `.add_fade(config, start)` - Add automation clip
- `.apply()` - Apply configuration
- `.start()` / `.stop()` / `.pause()` / `.resume()` - Transport control
- `.launch()` - Quantized start

**Unit Tests**: `crates/vibelang-core2/src/handlers/sequences.rs`
- Clip scheduling
- Loop wrapping
- Fade triggering

**Integration Tests**: `tests/integration/test_04_sequences.vibe`
- Sequence creation
- Clip management
- Loop behavior

---

### 7. Effects

**Description**: Audio effects processing on groups.

**API Functions**:
- `fx(name)` - Create effect builder
- `.synth(synthdef)` - Set effect synthdef
- `.param(name, value)` - Set parameter
- `.apply()` - Apply effect to current group

**Unit Tests**: `crates/vibelang-core2/src/handlers/effects.rs`
- Effect creation
- Parameter routing

**Integration Tests**: `tests/integration/test_05_groups_effects.vibe`
- Effect addition
- Parameter setting

---

### 8. Fades (Automation)

**Description**: Parameter automation over time.

**API Functions**:
- `fade()` - Create fade builder
- `.target_voice(id)` / `.target_group(id)` / `.target_effect(id)` - Set target
- `.param(name)` - Set parameter to automate
- `.from(value)` - Set start value (optional)
- `.to(value)` - Set end value
- `.duration(beats)` - Set duration
- `.curve(type)` - Set curve type (linear, exponential, sine)
- `.start()` - Start fade

**Unit Tests**: `crates/vibelang-core2/src/handlers/fades.rs`
- Fade calculation
- Curve interpolation

**Integration Tests**: `tests/integration/test_05_groups_effects.vibe`
- Basic fades
- Curve types

---

### 9. Samples

**Description**: Audio file loading and playback configuration.

**API Functions**:
- `sample(name, path)` - Load sample
- `.attack(seconds)` / `.sustain(level)` / `.release(seconds)` - Envelope
- `.amp(level)` - Set amplitude
- `.rate(ratio)` - Set playback rate
- `.loop_mode(bool)` - Enable looping
- `.offset(seconds)` - Set start offset
- `.length(seconds)` - Set length
- `.warp(bool)` - Enable time stretching
- `.speed(ratio)` - Set speed (with warp)
- `.pitch(ratio)` - Set pitch (with warp)
- `.semitones(semitones)` - Pitch shift
- `.warp_to_bpm(bpm)` - Match tempo
- `.slice(start, end)` - Extract slice

**Unit Tests**: `crates/vibelang-rhai/src/api/sample.rs`
- Sample configuration
- Warp calculations

**Integration Tests**: `tests/integration/test_07_samples.vibe`
- Sample loading
- Playback configuration

---

### 10. SFZ Instruments

**Description**: SFZ instrument file loading and playback.

**API Functions**:
- `sfz(name, path)` - Load SFZ instrument
- Associated voice methods work with SFZ voices

**Known Issues** (from CLAUDE.md):
- `pitch_keycenter` must be specified in SFZ file for correct pitch
- NOTE_OFF timing issue with melodies (gate never releases)

**Unit Tests**: `crates/vibelang-core2/src/handlers/sfz.rs`
- SFZ parsing
- Region matching

**Integration Tests**: `tests/integration/test_09_sfz.vibe`
- SFZ loading
- Note playback

---

### 11. SynthDefs

**Description**: SuperCollider synthdef management via DSP DSL.

**API Functions**:
- `define_synthdef(name)` - Create synthdef builder
- `.param(name, default)` - Add parameter
- `.body(closure)` - Define signal graph

**UGen Functions** (via vibelang-dsp):
- Oscillators: `sin_osc_ar`, `saw_ar`, `pulse_ar`, `noise_ar`, etc.
- Filters: `lpf_ar`, `hpf_ar`, `bpf_ar`, `rlpf_ar`, etc.
- Envelopes: `envelope()` builder with `.adsr()`, `.perc()`, etc.
- Effects: `reverb_ar`, `delay_ar`, `chorus_ar`, etc.

**Unit Tests**: `crates/vibelang-dsp/src/` (various)
- Synthdef generation
- UGen compilation

**Integration Tests**: `tests/integration/test_06_custom_synthdefs.vibe`
- Synthdef creation
- Parameter definition

---

### 12. MIDI (Feature-Gated)

**Description**: MIDI device I/O and routing.

**API Functions**:
- `midi_device(index)` - Get MIDI device
- `midi_devices()` - List available devices
- `.route_to(voice)` - Route keyboard to voice
- `.route_cc(cc, voice, param, min, max)` - Route CC to parameter
- `.open_input()` / `.close_input()` - Open/close MIDI input
- `midi_keyboard(device, voice)` - Shortcut for keyboard routing
- `midi_learn_cc(device, voice, param)` - Auto-map CC

**Unit Tests**: `crates/vibelang-core2/src/midi/` (various modules)
- MIDI message parsing
- CC routing
- MPE support
- NRPN encoding

**Integration Tests**: `tests/integration/test_08_midi.vibe`
- Device enumeration
- Keyboard routing
- CC mapping

---

### 13. Recordings (Native Only)

**Description**: Audio recording to buffers and files.

**API Functions**:
- `record(name)` - Create recording builder
- `.to_file(path)` / `.to_buffer()` - Set destination
- `.channels(count)` - Set channel count
- `.duration(seconds)` - Set max duration
- `.start()` / `.stop()` - Control recording

**Unit Tests**: `crates/vibelang-core2/src/handlers/recordings.rs`
- Recording lifecycle
- Buffer management

**Integration Tests**: (Planned)

---

### 14. Helper Functions

**Description**: Utility functions for common operations.

**API Functions**:
- `db(decibels)` - Convert dB to linear amplitude
- `note(name)` - Convert note name to MIDI number
- `chord(name, octave?)` - Get chord notes
- `scale(root, mode, octave?)` - Get scale notes
- `scale_degree(root, mode, degree)` - Get scale degree note
- `bars(count)` - Convert bars to beats
- `beats(count)` - Return beats (identity function)
- `rand()` / `rand_int(min, max)` / `rand_float(min, max)` - Random values
- `shuffle(array)` / `pick(array)` - Array utilities
- `seq(min, max)` / `rev(array)` - Sequence utilities

**Unit Tests**: `crates/vibelang-rhai/src/api/helpers.rs`
- dB conversion
- Note parsing
- Chord generation
- Scale generation
- Random functions

**Integration Tests**: `tests/integration/test_02_voices_patterns.vibe`
- Note/chord/scale helpers

---

## Unit Test Coverage

### vibelang-core2/src/

| Module | File | Tests | Coverage |
|--------|------|-------|----------|
| types/time | time.rs | 20 | High |
| types/ids | ids.rs | 10 | High |
| handlers/transport | transport.rs | 5 | Medium |
| handlers/groups | groups.rs | 5 | Medium |
| handlers/voices | voices.rs | 8 | Medium |
| handlers/patterns | patterns.rs | 0 | **LOW** |
| handlers/melodies | melodies.rs | 0 | **LOW** |
| handlers/sequences | sequences.rs | 0 | **LOW** |
| handlers/effects | effects.rs | 3 | Low |
| handlers/fades | fades.rs | 5 | Medium |
| handlers/samples | samples.rs | 3 | Low |
| handlers/sfz | sfz.rs | 10 | Medium |
| handlers/recordings | recordings.rs | 5 | Medium |
| handlers/synthdefs | synthdefs.rs | 3 | Low |
| midi/* | (various) | 45+ | High |
| clock | clock.rs | 5 | Medium |
| validation | validation.rs | 20+ | High |
| reload | reload/ | 10 | Medium |

### vibelang-rhai/src/api/

| Module | File | Tests | Coverage |
|--------|------|-------|----------|
| helpers | helpers.rs | 50+ | High |
| pattern | pattern.rs | 40+ | High |
| melody | melody.rs | 30+ | High |
| voice | voice.rs | 25+ | High |
| sequence | sequence.rs | 15+ | Medium |
| sample | sample.rs | 20+ | High |
| assert | assert.rs | 10+ | High |
| global | global.rs | 5+ | Medium |
| group | group.rs | 10+ | Medium |

---

## Integration Tests

Location: `tests/integration/`

| Test File | Features Tested | Status |
|-----------|-----------------|--------|
| test_01_transport.vibe | Tempo, time signature, quantization | Active |
| test_02_voices_patterns.vibe | Voices, patterns, helpers | Active |
| test_03_melodies.vibe | Melodies, note parsing, chords | Active |
| test_04_sequences.vibe | Sequences, clips | Active |
| test_05_groups_effects.vibe | Groups, effects, fades | Active |
| test_06_custom_synthdefs.vibe | SynthDef DSL | Active |
| test_07_samples.vibe | Sample loading | Conditional |
| test_08_midi.vibe | MIDI routing | Feature-gated |
| test_09_sfz.vibe | SFZ instruments | Conditional |

### New Integration Tests Added

| Test File | Features Tested | Status |
|-----------|-----------------|--------|
| test_10_loop_boundaries.vibe | Pattern/melody loop wrap-around, note helpers, chords, scales | **NEW** |
| test_11_edge_cases.vibe | Extreme tempo/polyphony, zero-length, boundary conditions | **NEW** |
| test_12_timing.vibe | Beat calculations, note timing, triplets, swing | **NEW** |
| test_13_polyphony.vibe | Voice polyphony, choke groups, mute/solo, round-robin | **NEW** |

---

## Edge Cases and Known Issues

### Loop Boundary Consistency (FIXED)

**Status**: ✅ FIXED

**Issue**: Notes at loop boundaries were handled inconsistently between patterns and melodies.

**Location**:
- `handlers/patterns.rs:72-100`
- `handlers/melodies.rs:97-125`

**Fix Applied**:
Both patterns and melodies now use consistent boundary semantics:
- `(last_pos, new_pos]` - exclusive start, inclusive end
- `last_pos` is where we were (already triggered on previous tick)
- `new_pos` is where we are now (should trigger)

This ensures:
- Notes/steps at the current position (`new_pos`) trigger correctly
- Notes/steps that were just triggered (`last_pos`) don't double-trigger
- Consistent behavior between patterns and melodies

**Edge Cases Covered by Unit Tests**:
1. Note exactly at beat 0 when pattern loops
2. Note at last beat of pattern (e.g., beat 3.99 in 4-beat pattern)
3. Pattern/melody starting at non-zero beat
4. Very short patterns (< 1 beat)
5. Pattern length that doesn't divide evenly into current beat

### Other Edge Cases

| Category | Edge Case | Expected Behavior |
|----------|-----------|-------------------|
| Tempo | tempo = 1.0 (minimum) | Should work, very slow |
| Tempo | tempo = 999.0 (maximum) | Should work, very fast |
| Polyphony | poly = 1 | Monophonic, voice stealing |
| Polyphony | poly = 128 (max) | Many simultaneous notes |
| Pattern | length = 0 | Should skip processing |
| Melody | empty notes | Should skip processing |
| Beat | Negative beat position | Should wrap correctly |
| Time Sig | 1/4 (minimum) | Should work |
| Time Sig | 32/32 (unusual) | Should work |

---

## Test Runner Usage

### Running All Integration Tests

```bash
cd tests/integration
./run_tests.sh
```

### Running Specific Tests

```bash
./run_tests.sh transport    # Run test_01_transport.vibe
./run_tests.sh pattern      # Run tests matching "pattern"
```

### Options

```bash
./run_tests.sh --quick      # 2-second timeout
./run_tests.sh --verbose    # Show output
./run_tests.sh --api        # Enable HTTP API verification
```

### Writing Integration Tests

```rhai
// Start a test suite
test_start("test_name");

// Assertions
assert_eq(actual, expected, "message");
assert_true(condition, "message");
assert_approx(actual, expected, tolerance, "message");
assert_len(array, length, "message");
assert_contains(array, value, "message");

// End test suite (prints summary)
test_end();

// Or end and exit with appropriate exit code
test_end_and_exit();

// Manual exit
exit(0);  // Success
exit(1);  // Failure
```

---

## Test Coverage Improvements Needed

### High Priority

1. **Loop Boundary Tests**: Add unit tests for pattern/melody tick logic at boundaries
2. **Timing Accuracy Tests**: Verify notes trigger at correct beats
3. **Handler Integration Tests**: Test actual audio triggering behavior

### Medium Priority

4. **Error Recovery Tests**: Test invalid inputs, missing resources
5. **Stress Tests**: Many patterns/voices simultaneously
6. **Hot Reload Tests**: State changes during playback

### Low Priority

7. **Performance Benchmarks**: Measure tick processing time
8. **Memory Tests**: Long-running playback memory usage
9. **Cross-Platform Tests**: WASM backend compatibility
