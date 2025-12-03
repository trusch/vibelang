# Vibelang Music Theory Library

A comprehensive music theory toolkit for vibelang, providing functions for scales, chords, progressions, melodies, bass lines, voice leading, harmony analysis, and counterpoint.

## Overview

This library enables you to:
- Generate scales in any key and mode
- Build chords with any quality and voicing
- Create common chord progressions from multiple genres
- Generate melodic and bass patterns
- Apply music theory rules automatically
- Analyze harmony and voice leading
- Create species counterpoint

All functions are written in pure Rhai and return arrays compatible with vibelang's `.step()` notation.

## How to Use

Import the modules you need at the top of your `.vibe` file:

```rhai
import "stdlib/theory/scales.vibe" as scales;
import "stdlib/theory/chords.vibe" as chords;
import "stdlib/theory/progressions.vibe" as prog;

// Now use the functions
let melody = scales::major_scale("C", 5, 8);
let chord = chords::major7_chord("C", 4);
```

**Note:** The theory modules import each other internally as needed. Rhai's module system handles caching automatically, so you don't need to worry about duplicate imports - each module is only loaded once by the engine.

## Modules

### 1. Core (`core.vibe`)

**Foundation utilities for all music theory operations**

```rhai
import "stdlib/theory/core.vibe" as core;

// Note conversions
let midi = core::note_to_midi("C4");           // 60
let note = core::midi_to_note(60);             // "C4"
let freq = core::note_to_freq("A4");           // 440.0

// Transposition
let up = core::transpose_note("C4", 7);        // "G4" (up a fifth)
let down = core::transpose_note("G4", -7);     // "C4"

// Intervals
let interval = core::interval_semitones("C4", "G4");  // 7
let name = core::interval_name(7);             // "Perfect 5th"

// Octave operations
let shifted = core::shift_octave("C4", 1);     // "C5"
let set = core::set_octave("C5", 3);           // "C3"
```

### 2. Scales (`scales.vibe`)

**Generate scales in any key with various modes and types**

```rhai
import "stdlib/theory/scales.vibe" as scales;

// Basic scales
let major = scales::major_scale("C", 4, 8);         // C major, 8 notes
let minor = scales::minor_scale("A", 4, 8);         // A natural minor
let harmonic = scales::harmonic_minor_scale("A", 4, 8);
let melodic = scales::melodic_minor_scale("A", 4, 8);

// Modes
let dorian = scales::dorian_scale("D", 4, 8);
let phrygian = scales::phrygian_scale("E", 4, 8);
let lydian = scales::lydian_scale("F", 4, 8);
let mixolydian = scales::mixolydian_scale("G", 4, 8);

// Pentatonic
let maj_pent = scales::major_pentatonic_scale("C", 4, 8);
let min_pent = scales::minor_pentatonic_scale("A", 4, 8);

// Blues
let blues = scales::blues_scale("C", 4, 8);

// Exotic
let double_harmonic = scales::double_harmonic_scale("C", 4, 8);
let phrygian_dom = scales::phrygian_dominant_scale("E", 4, 8);
let japanese = scales::japanese_scale("D", 4, 8);

// Get scale by name
let scale = scales::scale("C", "major", 4, 8);
```

**Use with melodies:**

```rhai
let melody_notes = scales::major_scale("C", 5, 16);

melody("lead")
    .on(lead_voice)
    .step(melody_notes)
    .len(8.0)
    .start();
```

### 3. Chords (`chords.vibe`)

**Generate chords with various qualities, extensions, and voicings**

```rhai
import "stdlib/theory/chords.vibe" as chords;

// Triads
let major = chords::major_triad("C", 4);        // ["C4", "E4", "G4"]
let minor = chords::minor_triad("A", 4);        // ["A4", "C5", "E5"]
let dim = chords::diminished_triad("B", 4);
let aug = chords::augmented_triad("C", 4);

// 7th chords
let maj7 = chords::major7_chord("C", 4);
let min7 = chords::minor7_chord("D", 4);
let dom7 = chords::dominant7_chord("G", 4);
let half_dim = chords::half_diminished7_chord("B", 4);

// Extended chords
let maj9 = chords::major9_chord("C", 4);
let dom13 = chords::dominant13_chord("G", 3);

// By name
let chord = chords::chord("C", "major7", 4);
let chord2 = chords::chord("D", "m7", 4);

// Inversions
let first_inv = chords::chord_inversion("C", "major", 1, 4);
let second_inv = chords::chord_inversion("C", "major", 2, 4);

// Voicings
let open = chords::open_voicing(major);
let drop2 = chords::drop2_voicing(maj7);
let drop3 = chords::drop3_voicing(maj7);
```

**Use with patterns:**

```rhai
let chord = chords::major7_chord("C", 4);

melody("pad")
    .on(pad_voice)
    .step(chord)                    // Play chord as arpeggio
    .len(4.0)
    .start();
```

### 4. Progressions (`progressions.vibe`)

**Common chord progressions from various musical styles**

```rhai
import "stdlib/theory/progressions.vibe" as prog;

// Pop/Rock
let pop1 = prog::pop_progression_1("C", 3);        // I-V-vi-IV
let pop2 = prog::pop_progression_2("C", 3);        // vi-IV-I-V
let axis = prog::axis_progression("C", 3);         // I-IV-vi-V

// Jazz
let ii_v_i = prog::jazz_ii_v_i("C", 3);
let turnaround = prog::jazz_turnaround("C", 3);

// Blues
let blues12 = prog::blues_12bar("C", 3);

// Modal
let dorian = prog::dorian_vamp("D", 3);
let mixolydian = prog::mixolydian_groove("G", 3);

// EDM
let house = prog::house_progression("C", 3);
let trance = prog::trance_progression("Am", 3);

// By name
let progression = prog::progression("pop_1", "C", 3);

// Utility functions
let roots = prog::progression_roots(progression);  // Just the root notes
let flattened = prog::flatten_progression(progression, 1);
```

**Complete example:**

```rhai
import "stdlib/theory/progressions.vibe" as prog;
import "stdlib/theory/bass_patterns.vibe" as bass;

// Create progression
let chords = prog::pop_progression_1("C", 3);

// Generate bass line from progression
let bass_line = bass::root_fifth_pattern(chords, 4);

// Use in composition
melody("bassline")
    .on(bass_voice)
    .step(bass_line)
    .len(16.0)
    .start();
```

### 5. Bass Patterns (`bass_patterns.vibe`)

**Generate bass lines from chord progressions**

```rhai
import "stdlib/theory/bass_patterns.vibe" as bass;

let progression = /* your chord progression */;

// Simple patterns
let root = bass::root_pattern(progression, 4);
let root_fifth = bass::root_fifth_pattern(progression, 4);
let root_octave = bass::root_octave_pattern(progression, 4);

// Arpeggio patterns
let arp_up = bass::arpeggio_ascending(progression, 4);
let arp_down = bass::arpeggio_descending(progression, 4);

// Jazz walking bass
let walking = bass::walking_bass_simple(progression, 4);
let bebop = bass::walking_bass_bebop(progression, 4);

// Genre-specific patterns
let house = bass::house_bass(progression, 4);
let techno = bass::techno_bass(progression, 4);
let funk = bass::funk_bass(progression, 4);
let reggae = bass::reggae_bass(progression, 4);
let dnb = bass::dnb_bass(progression, 4);
let disco = bass::disco_bass(progression, 4);

// By name
let pattern = bass::bass_pattern("walking", progression, 4);
```

### 6. Melody Generation (`melody_gen.vibe`)

**Generate melodies using various techniques**

```rhai
import "stdlib/theory/melody_gen.vibe" as mel;

// Scale-based generation
let random_walk = mel::random_walk_melody("C", "major", 16, 5);
let stepwise = mel::stepwise_melody("C", "major", 16, 5);

// Chord-based generation
let chord_tones = mel::chord_tone_melody(progression, 4);
let arpeggiated = mel::arpeggiated_melody(progression, "up");

// Contour-based
let ascending = mel::ascending_melody("C", "major", 8, 5);
let descending = mel::descending_melody("C", "major", 8, 5);
let arch = mel::arch_melody("C", "major", 16, 5);
let wave = mel::wave_melody("C", "major", 16, 5);

// Embellishments
let with_passing = mel::add_passing_tones(melody);
let with_neighbor = mel::add_neighbor_tones(melody, true);

// Motivic development
let retrograde = mel::retrograde(motif);
let inversion = mel::inversion(motif);
let augmentation = mel::augmentation(motif);

// Sequences
let asc_seq = mel::ascending_sequence(motif, 4, 2);
let desc_seq = mel::descending_sequence(motif, 4, 2);

// Call and response
let call_response = mel::call_and_response(call_motif, answer_motif);

// Jazz
let bebop_run = mel::bebop_run("C", 4, "up");
```

### 7. Arpeggios (`arpeggios.vibe`)

**Generate arpeggio patterns from chords**

```rhai
import "stdlib/theory/arpeggios.vibe" as arp;

let chord = chords::major7_chord("C", 4);

// Basic patterns
let up = arp::arpeggio_up(chord);
let down = arp::arpeggio_down(chord);
let up_down = arp::arpeggio_up_down(chord);

// Extended (multiple octaves)
let extended = arp::arpeggio_up_extended(chord, 2);

// Broken chord patterns
let alberti = arp::alberti_bass(chord);
let broken = arp::broken_1_3_2_4(chord);

// Rhythmic variations
let with_rests = arp::arpeggio_with_rests(chord, "up");
let accented = arp::arpeggio_accented(chord, "up");

// Speed variations
let triplet = arp::arpeggio_triplet(chord, 2);

// Style-specific
let rolling = arp::rolling_arpeggio(chord);
let tremolo = arp::tremolo_arpeggio(chord, 0, 1, 4);
let cascading = arp::cascading_arpeggio(chord, 2);

// Apply to progression
let arp_prog = arp::apply_to_progression(progression, "up_down");
```

### 8. Rhythm (`rhythm.vibe`)

**Generate rhythmic patterns**

```rhai
import "stdlib/theory/rhythm.vibe" as rhythm;

// Basic patterns
let four_floor = rhythm::four_on_floor(2);         // "x...x...x...x..."
let backbeat = rhythm::backbeat(2);                 // Beats 2 and 4
let offbeat = rhythm::offbeat(2);

// Euclidean rhythms
let euclidean = rhythm::euclidean_rhythm(5, 8);    // 5 hits in 8 steps
let tresillo = rhythm::euclidean_pattern("tresillo");
let bossa = rhythm::euclidean_pattern("bossa");

// Clave patterns
let son_clave = rhythm::son_clave_3_2();
let rumba_clave = rhythm::rumba_clave_3_2();
let bossa_clave = rhythm::bossa_nova_clave();

// Genre-specific
let house_kick = rhythm::house_kick();
let techno_kick = rhythm::techno_kick();
let trap_hh = rhythm::trap_hihat();
let dnb = rhythm::dnb_break();

// Transformations
let reversed = rhythm::reverse_pattern(pattern);
let rotated = rhythm::rotate_pattern(pattern, 2);
let inverted = rhythm::invert_pattern(pattern);
let doubled = rhythm::double_time(pattern);
let halved = rhythm::half_time(pattern);

// Add accents
let accented = rhythm::add_accents(pattern, [0, 4, 8]);
let every_n = rhythm::accent_every_n(pattern, 4);

// Apply rhythm to melody
let rhythmic_melody = rhythm::apply_rhythm(notes, pattern);
```

### 9. Voice Leading (`voice_leading.vibe`)

**Smooth voice leading between chords**

```rhai
import "stdlib/theory/voice_leading.vibe" as vl;

// Voice lead from one chord to another
let chord2_voiced = vl::voice_lead(chord1, chord2);

// Voice lead entire progression
let voiced_prog = vl::voice_lead_progression(progression);

// Common tone retention
let common_tones = vl::find_common_tones(chord1, chord2);
let with_common = vl::voice_lead_with_common_tones(chord1, chord2);

// Motion analysis
let motion = vl::analyze_motion("C4", "E4", "E4", "G4");  // "parallel"

// Check for errors
let parallel_errors = vl::check_parallel_perfect(chord1, chord2);
let has_crossing = vl::has_voice_crossing(chord);
let fixed = vl::fix_voice_crossing(chord);

// SATB voice leading
let ranges = vl::satb_ranges();
let satb_voiced = vl::satb_voice_lead(chord1, chord2);
let range_check = vl::check_range(chord, ranges);

// Assess quality
let quality_score = vl::assess_voice_leading(chord1, chord2);
```

### 10. Harmony (`harmony.vibe`)

**Harmonic analysis tools**

```rhai
import "stdlib/theory/harmony.vibe" as harm;

// Chord identification
let quality = harm::identify_chord(notes);

// Degree analysis
let degree = harm::chord_degree_in_key("D", "C", "major");  // 2
let numeral = harm::roman_numeral("D", "minor", "C", "major");  // "ii"

// Harmonic function
let function = harm::harmonic_function(2, "major");  // "subdominant"
let is_functional = harm::is_functional_progression([1, 4, 5, 1]);

// Available tensions
let tensions = harm::available_tensions("major7", "jazz");
let with_tension = harm::add_tension(chord, 14);  // Add 9th

// Scale degree analysis
let note_degree = harm::scale_degree("E", "C", "major");  // 3
let is_chord_tone = harm::is_chord_tone("E", chord);
let is_scale_tone = harm::is_scale_tone("E", "C", "major");

// Classification
let classification = harm::classify_melodic_note(note, prev, next, chord, "C");

// Substitutions
let tritone_sub = harm::tritone_sub("G");
let relative = harm::relative_key("C", "major");  // "A" (minor)
let substitutes = harm::suggest_substitutes("G", "7");

// Consonance/dissonance
let is_consonant = harm::is_consonant(7);  // true (P5)
let dissonance = harm::dissonance_level(1);  // 5 (high)
let chord_cons = harm::chord_consonance(chord);
```

### 11. Counterpoint (`counterpoint.vibe`)

**Species counterpoint and two-voice writing**

```rhai
import "stdlib/theory/counterpoint.vibe" as cp;

// Generate counterpoint
let cantus_firmus = ["C4", "D4", "E4", "F4", "E4", "D4", "C4"];
let cp_above = cp::first_species_above(cantus_firmus, "C", "major");
let cp_below = cp::first_species_below(cantus_firmus, "C", "major");

// Validate counterpoint
let validation = cp::validate_counterpoint(voice1, voice2);
// Returns: { valid: true/false, errors: [...], error_count: N }

// Check specific rules
let parallels = cp::check_parallel_perfects_counterpoint(v1, v2);
let hidden = cp::check_hidden_perfects(v1, v2);
let leap_errors = cp::check_leap_treatment(melody);

// Cadence check
let is_proper = cp::is_proper_cadence(v1_final, v2_final, v1_penult, v2_penult);

// Generate cantus firmus
let cantus = cp::generate_cantus_firmus("C", "major", 8);

// Score quality
let score = cp::score_counterpoint(voice1, voice2);  // 0-100
```

## Complete Examples

### Example 1: Generate a Complete Song

```rhai
import "stdlib/theory/scales.vibe" as scales;
import "stdlib/theory/progressions.vibe" as prog;
import "stdlib/theory/bass_patterns.vibe" as bass;
import "stdlib/theory/melody_gen.vibe" as mel;

set_tempo(120);

// Define voices
let bass_voice = voice("bass").synth("sub_deep").gain(db(-3));
let lead_voice = voice("lead").synth("lead_saw").gain(db(-6));

// Create chord progression
let chords = prog::pop_progression_1("C", 2);  // I-V-vi-IV in C

// Generate bass line
let bass_line = bass::walking_bass_simple(chords, 4);

// Generate melody
let scale = scales::major_scale("C", 5, 16);
let lead_melody = mel::arch_melody("C", "major", 16, 5);

// Apply to music
melody("bass")
    .on(bass_voice)
    .step(bass_line)
    .len(16.0)
    .start();

melody("lead")
    .on(lead_voice)
    .step(lead_melody)
    .len(16.0)
    .start();
```

### Example 2: Jazz Progression with Voice Leading

```rhai
import "stdlib/theory/progressions.vibe" as prog;
import "stdlib/theory/voice_leading.vibe" as vl;
import "stdlib/theory/bass_patterns.vibe" as bass;

// Jazz ii-V-I progression
let progression = prog::jazz_ii_v_i("C", 3);

// Apply smooth voice leading
let voiced_progression = vl::voice_lead_progression(progression);

// Walking bass
let bass_line = bass::walking_bass_bebop(voiced_progression, 4);

// Use in composition...
```

### Example 3: Generative Melody with Embellishments

```rhai
import "stdlib/theory/scales.vibe" as scales;
import "stdlib/theory/melody_gen.vibe" as mel;
import "stdlib/theory/rhythm.vibe" as rhythm;

// Generate base melody
let motif = scales::major_pentatonic_scale("C", 5, 4);

// Develop motif
let sequence = mel::ascending_sequence(motif, 3, 2);
let with_passing = mel::add_passing_tones(sequence);

// Apply rhythm
let pattern = rhythm::euclidean_rhythm(7, 16);
let final_melody = rhythm::apply_rhythm(with_passing, pattern);

// Use in composition...
```

### Example 4: Counterpoint Composition

```rhai
import "stdlib/theory/counterpoint.vibe" as cp;
import "stdlib/theory/scales.vibe" as scales;

// Generate cantus firmus
let cantus = cp::generate_cantus_firmus("C", "major", 8);

// Generate counterpoint above it
let counterpoint = cp::first_species_above(cantus, "C", "major");

// Validate the result
let validation = cp::validate_counterpoint(cantus, counterpoint);

if validation["valid"] {
    print("Perfect counterpoint!");
} else {
    print(`Found ${validation["error_count"]} errors`);
}

// Use both voices in composition...
```

## Quick Reference

### Common Tasks

**Generate a scale:**
```rhai
let scale = scales::major_scale("C", 4, 8);
```

**Create a chord:**
```rhai
let chord = chords::chord("C", "major7", 4);
```

**Get a progression:**
```rhai
let progression = prog::progression("pop_1", "C", 3);
```

**Generate bass line:**
```rhai
let bass = bass::bass_pattern("walking", progression, 4);
```

**Create melody:**
```rhai
let melody = mel::arch_melody("C", "major", 16, 5);
```

**Generate rhythm:**
```rhai
let pattern = rhythm::euclidean_rhythm(5, 8);
```

**Voice lead chords:**
```rhai
let voiced = vl::voice_lead_progression(progression);
```

## Tips and Best Practices

1. **Import only what you need** - Each module can be imported separately
2. **Combine functions** - Chain operations for complex results
3. **Validate progressions** - Use harmony analysis to check your progressions
4. **Experiment with parameters** - Most functions have optional parameters
5. **Use voice leading** - Always apply voice leading for smooth progressions
6. **Start simple** - Begin with basic patterns and add complexity

## Music Theory Concepts

### Scales
- **Major/Minor**: Foundation of Western music
- **Modes**: Different flavors (Dorian for jazzy minor, Mixolydian for bluesy major)
- **Pentatonic**: 5-note scales, great for melodies
- **Blues**: Pentatonic + blue note

### Chords
- **Triads**: 3 notes (root, third, fifth)
- **7th chords**: Add the 7th for richer harmony
- **Extensions**: 9th, 11th, 13th for jazz and complexity
- **Voicings**: Different arrangements of the same chord

### Progressions
- **I-IV-V**: The most common progression
- **ii-V-I**: The jazz standard
- **I-V-vi-IV**: Modern pop progression

### Voice Leading
- **Common tones**: Keep notes that are in both chords
- **Stepwise motion**: Move voices by small intervals
- **Contrary motion**: Voices move in opposite directions

## License

Part of the vibelang project.

## Contributing

Contributions welcome! Add new patterns, scales, or functions following the existing conventions.

