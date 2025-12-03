# Vibelang Melody Library

A comprehensive collection of 140+ well-known melodies from various genres and eras, ready to use in your vibelang compositions.

## Overview

This library provides instant access to iconic melodies from:
- Classical music (public domain)
- Traditional folk songs
- Nursery rhymes and children's songs
- Holiday music
- National anthems
- Video game themes
- TV and movie themes
- Jazz standards and ragtime
- Pop and rock hooks
- World music traditions

All melodies are provided as functions that return note strings (space-separated) compatible with vibelang's `.step()` notation.

## Quick Start

```rhai
import "stdlib/theory/melodies/classical.vibe" as classical;
import "stdlib/theory/melodies/video_games.vibe" as vg;

// Get a melody (returns a string like "E4 E4 F4 G4 ...")
let beethoven = classical::ode_to_joy();
let tetris = vg::tetris_theme();

// Use it directly in your composition
melody("lead")
    .on(lead_voice)
    .step(beethoven)
    .len(8.0)
    .start();
```

## Melody Syntax

Melodies use a simple string notation that supports various rhythmic features:

### Basic Notation

- **Notes**: `"C4 E4 G4"` - Space-separated note names
- **Rests**: `"C4 . E4"` - Dot (`.`) for silence
- **Holds**: `"C4 - -"` - Dash (`-`) extends the previous note
- **Bar separator**: `"C4 E4 | G4 C5"` - Pipe (`|`) for visual organization (optional)

### Tuplets (Triplets, Quintuplets, etc.)

Some melodies require tuplets - groups of notes that divide the beat in unusual ways. The most common is the triplet (3 notes in the time of 2).

**Syntax**: `(ratio notes...)`

- **Explicit ratio**: `(3:2 C4 D4 E4)` - 3 notes in space of 2 beats
- **Implicit triplet**: `(3 C4 D4 E4)` - Shorthand for `(3:2 ...)`

**Common Tuplets:**

```rhai
// Triplet (most common)
"(3:2 C4 D4 E4)"  // 3 notes in 2 beats

// Quintuplet  
"(5:4 C4 D4 E4 F4 G4)"  // 5 notes in 4 beats

// Sextuplet
"(6:4 C4 D4 E4 F4 G4 A4)"  // 6 notes in 4 beats
```

**Example with Tetris theme:**

```rhai
// Tetris uses triplets for the iconic bouncy rhythm
"E5 - (3:2 B4 C5 D5) - | C5 - (3:2 B4 A4 G4) ."
```

## Categories

### 1. Classical Music (`classical.vibe`)

23 iconic classical compositions from the public domain:

```rhai
import "stdlib/theory/melodies/classical.vibe" as classical;

// Beethoven
let ode = classical::ode_to_joy();
let elise = classical::fur_elise();
let moonlight = classical::moonlight_sonata();

// Mozart
let nacht = classical::eine_kleine_nachtmusik();
let turkish = classical::turkish_march();
let symphony = classical::mozart_symphony_40();

// Tchaikovsky
let swan = classical::swan_lake();
let sugar_plum = classical::sugar_plum_fairy();

// Other composers
let carmen = classical::carmen_habanera();
let william_tell = classical::william_tell_overture();
let blue_danube = classical::blue_danube();
let valkyries = classical::ride_of_valkyries();
let spring = classical::spring_vivaldi();
let winter = classical::winter_vivaldi();
let canon = classical::canon_in_d();
let toccata = classical::toccata_fugue();
let hall = classical::hall_mountain_king();
let hungarian = classical::hungarian_dance_5();
let clair = classical::clair_de_lune();
let gymno = classical::gymnopedie_1();
let hallelujah = classical::hallelujah_chorus();
let nocturne = classical::nocturne_op9_no2();
```

### 2. Folk Songs (`folk_songs.vibe`)

15 traditional folk melodies from various cultures:

```rhai
import "stdlib/theory/melodies/folk_songs.vibe" as folk;

// English traditional
let greensleeves = folk::greensleeves();
let scarborough = folk::scarborough_fair();

// American folk
let susanna = folk::oh_susanna();
let yankee = folk::yankee_doodle();
let home = folk::home_on_the_range();
let mountain = folk::coming_round_the_mountain();
let old_man = folk::this_old_man();
let skip = folk::skip_to_my_lou();
let camptown = folk::camptown_races();
let clementine = folk::clementine();
let smokey = folk::old_smokey();
let riverside = folk::down_by_riverside();

// Traditional hymns
let amazing = folk::amazing_grace();
let auld = folk::auld_lang_syne();
let danny = folk::danny_boy();
```

### 3. Nursery Rhymes (`nursery_rhymes.vibe`)

10 classic children's songs:

```rhai
import "stdlib/theory/melodies/nursery_rhymes.vibe" as nursery;

let twinkle = nursery::twinkle_twinkle();
let mary = nursery::mary_had_little_lamb();
let baa_baa = nursery::baa_baa_black_sheep();
let row_boat = nursery::row_row_row_boat();
let london = nursery::london_bridge();
let weasel = nursery::pop_goes_weasel();
let humpty = nursery::humpty_dumpty();
let jack_jill = nursery::jack_and_jill();
let macdonald = nursery::old_macdonald();
let wheels = nursery::wheels_on_bus();
```

### 4. Holiday Songs (`holiday_songs.vibe`)

10 traditional holiday melodies:

```rhai
import "stdlib/theory/melodies/holiday_songs.vibe" as holiday;

let jingle = holiday::jingle_bells();
let silent = holiday::silent_night();
let deck = holiday::deck_the_halls();
let merry = holiday::merry_christmas();
let tree = holiday::o_christmas_tree();
let joy = holiday::joy_to_world();
let noel = holiday::first_noel();
let hark = holiday::hark_herald_angels();
let birthday = holiday::happy_birthday();
let auld = holiday::auld_lang_syne_holiday();
```

### 5. National Anthems (`national_anthems.vibe`)

10 national anthems from around the world:

```rhai
import "stdlib/theory/melodies/national_anthems.vibe" as anthems;

let usa = anthems::star_spangled_banner();
let uk = anthems::god_save_queen();
let france = anthems::la_marseillaise();
let canada = anthems::o_canada();
let australia = anthems::advance_australia_fair();
let germany = anthems::deutschlandlied();
let japan = anthems::kimigayo();
let belgium = anthems::la_brabanconne();
let italy = anthems::fratelli_italia();
let mexico = anthems::mexican_anthem();
```

### 6. Video Game Music (`video_games.vibe`)

20 iconic video game themes:

```rhai
import "stdlib/theory/melodies/video_games.vibe" as vg;

let tetris = vg::tetris_theme();
let mario = vg::mario_theme();
let zelda = vg::zelda_theme();
let sonic = vg::sonic_green_hill();
let megaman = vg::megaman_theme();
let pokemon = vg::pokemon_theme();
let ff_victory = vg::ff_victory_fanfare();
let pacman = vg::pacman_theme();
let dk = vg::donkey_kong_theme();
let kirby = vg::kirby_green_greens();
let sf2 = vg::street_fighter_ryu();
let castlevania = vg::castlevania_vampire_killer();
let metroid = vg::metroid_theme();
let contra = vg::contra_jungle();
let frogger = vg::frogger_theme();
let space_inv = vg::space_invaders();
let galaga = vg::galaga_theme();
let duck_hunt = vg::duck_hunt_theme();
let punch_out = vg::punch_out_theme();
let chrono = vg::chrono_trigger_theme();
```

### 7. TV Themes (`tv_themes.vibe`)

15 memorable television theme songs:

```rhai
import "stdlib/theory/melodies/tv_themes.vibe" as tv;

let simpsons = tv::simpsons_theme();
let star_trek = tv::star_trek_theme();
let doctor_who = tv::doctor_who_theme();
let flintstones = tv::flintstones_theme();
let pink_panther = tv::pink_panther_theme();
let mission = tv::mission_impossible_theme();
let x_files = tv::x_files_theme();
let addams = tv::addams_family_theme();
let sesame = tv::sesame_street_theme();
let twilight = tv::twilight_zone_theme();
let lucy = tv::i_love_lucy_theme();
let cheers = tv::cheers_theme();
let brady = tv::brady_bunch_theme();
let scooby = tv::scooby_doo_theme();
let gadget = tv::inspector_gadget_theme();
```

### 8. Movie Themes (`movie_themes.vibe`)

15 iconic film themes:

```rhai
import "stdlib/theory/melodies/movie_themes.vibe" as movie;

let space_odyssey = movie::also_sprach_zarathustra();
let jaws = movie::jaws_theme();
let good_bad = movie::good_bad_ugly_theme();
let entertainer = movie::the_entertainer_movie();
let chariots = movie::chariots_of_fire();
let imperial = movie::imperial_march();
let bond = movie::james_bond_theme();
let axel = movie::axel_f();
let halloween = movie::halloween_theme();
let ghostbusters = movie::ghostbusters_theme();
let bttf = movie::back_to_future_theme();
let rocky = movie::gonna_fly_now();
let close_enc = movie::close_encounters();
let superman = movie::superman_theme();
let indy = movie::indiana_jones_theme();
```

### 9. Jazz Standards (`jazz_standards.vibe`)

12 jazz and ragtime classics:

```rhai
import "stdlib/theory/melodies/jazz_standards.vibe" as jazz;

let saints = jazz::when_saints_go_marching();
let take_five = jazz::take_five();
let caravan = jazz::caravan();
let entertainer = jazz::the_entertainer();
let maple = jazz::maple_leaf_rag();
let stlouis = jazz::st_louis_blues();
let in_mood = jazz::in_the_mood();
let sing3 = jazz::sing_sing_sing();
let summer = jazz::summertime();
let blue_monk = jazz::blue_monk();
let swing_low = jazz::swing_low_sweet_chariot();
let black_bottom = jazz::black_bottom_stomp();
```

### 10. Pop/Rock (`pop_rock.vibe`)

15 famous pop and rock riffs:

```rhai
import "stdlib/theory/melodies/pop_rock.vibe" as rock;

let house = rock::house_rising_sun();
let louie = rock::louie_louie();
let bamba = rock::la_bamba();
let smoke = rock::smoke_on_water();
let seven = rock::seven_nation_army();
let sweet = rock::sweet_child_opening();
let iron = rock::iron_man_riff();
let satisfaction = rock::satisfaction_riff();
let sunshine = rock::sunshine_love_riff();
let superstition = rock::superstition_riff();
let bites = rock::another_one_bites_dust();
let billie = rock::billie_jean_bass();
let come = rock::come_together_riff();
let day = rock::day_tripper_riff();
let pretty = rock::pretty_woman_riff();
```

### 11. World Music (`world_music.vibe`)

14 traditional melodies from around the world:

```rhai
import "stdlib/theory/melodies/world_music.vibe" as world;

let hava = world::hava_nagila();           // Jewish
let cucaracha = world::la_cucaracha();     // Mexican
let sakura = world::sakura_sakura();       // Japanese
let kalinka = world::kalinka();            // Russian
let bella = world::bella_ciao();           // Italian
let matilda = world::waltzing_matilda();   // Australian
let frere = world::frere_jacques();        // French
let malaguena = world::malaguena();        // Spanish
let siya = world::siyahamba();             // South African
let guanta = world::guantanamera();        // Cuban
let irish_w = world::irish_washerwoman();  // Irish
let scotland = world::scotland_brave();     // Scottish
let hatikvah = world::hatikvah();          // Hebrew
let zorba = world::zorbas_dance();         // Greek
```

## Usage Examples

### Example 1: Simple Melody

```rhai
import "stdlib/theory/melodies/classical.vibe" as classical;

set_tempo(120);

let lead = voice("lead").synth("lead_bright").gain(db(-6));

melody("beethoven")
    .on(lead)
    .step(classical::ode_to_joy())
    .len(8.0)
    .start();
```

### Example 2: Layered Melodies

```rhai
import "stdlib/theory/melodies/folk_songs.vibe" as folk;
import "stdlib/theory/melodies/nursery_rhymes.vibe" as nursery;

let voice1 = voice("lead1").synth("sine_pad").gain(db(-10));
let voice2 = voice("lead2").synth("saw_lead").gain(db(-12));

// Play two melodies in harmony
melody("folk")
    .on(voice1)
    .step(folk::greensleeves())
    .len(16.0)
    .start();

melody("nursery")
    .on(voice2)
    .step(nursery::twinkle_twinkle())
    .len(8.0)
    .start();
```

### Example 3: Transposed Melody

```rhai
import "stdlib/theory/melodies/video_games.vibe" as vg;
import "stdlib/theory/core.vibe" as core;

let tetris = vg::tetris_theme();

// Split into array, transpose each note
let notes = tetris.split(" ");
let transposed_notes = [];
for note in notes {
    if note == "." {
        transposed_notes.push(".");
    } else {
        transposed_notes.push(core::shift_octave(note, 1));
    }
}

// Join back into string
let tetris_high = "";
for i in 0..transposed_notes.len() {
    tetris_high += transposed_notes[i];
    if i < transposed_notes.len() - 1 {
        tetris_high += " ";
    }
}

melody("tetris_high")
    .on(lead_voice)
    .step(tetris_high)
    .len(8.0)
    .start();
```

### Example 4: Melody Mashup

```rhai
import "stdlib/theory/melodies/jazz_standards.vibe" as jazz;
import "stdlib/theory/melodies/classical.vibe" as classical;

// Combine melodies (strings concatenate with space)
let mashup = jazz::take_five() + " " + classical::fur_elise();

melody("mashup")
    .on(lead_voice)
    .step(mashup)
    .len(16.0)
    .start();
```

### Example 5: Melody with Effects

```rhai
import "stdlib/theory/melodies/movie_themes.vibe" as movie;

let lead = voice("lead")
    .synth("synth_bright")
    .gain(db(-8));

// Add reverb to the group
let melody_group = define_group("Melody", || {
    melody("theme")
        .on(lead)
        .step(movie::james_bond_theme())
        .len(8.0)
        .gate(0.8)
        .start();
});

group_add_fx(melody_group, "reverb", #{
    room_size: 0.8,
    damping: 0.5,
    wet: 0.3
});
```

### Example 6: Rhythmic Variation

```rhai
import "stdlib/theory/melodies/holiday_songs.vibe" as holiday;

let jingle = holiday::jingle_bells();

melody("jingle_bells")
    .on(lead_voice)
    .step(jingle)
    .len(8.0)
    .swing(0.3)          // Add swing feel
    .humanize(0.1)       // Slight timing variation
    .start();
```

## Advanced Techniques

### Melody Transformation

```rhai
import "stdlib/theory/melodies/classical.vibe" as classical;
import "stdlib/theory/melody_gen.vibe" as mel;

// Get melody as string and convert to array for transformation
let ode_string = classical::ode_to_joy();
let ode = ode_string.split(" ");

// Apply transformations
let retrograde = mel::retrograde(ode);
let inverted = mel::inversion(ode);
let augmented = mel::augmentation(ode);

// Convert back to strings for use (join array with spaces)
let retro_str = retrograde.join(" ");
let invert_str = inverted.join(" ");

// Use transformed melodies
melody("retro").on(voice1).step(retro_str).len(8.0).start();
melody("invert").on(voice2).step(invert_str).len(8.0).start();
```

### Creating Variations

```rhai
import "stdlib/theory/melodies/folk_songs.vibe" as folk;
import "stdlib/theory/melody_gen.vibe" as mel;

// Convert string to array for transformation
let base = folk::amazing_grace().split(" ");

// Add embellishments
let with_passing = mel::add_passing_tones(base);
let with_neighbor = mel::add_neighbor_tones(base, true);

// Convert back to string
let passing_str = with_passing.join(" ");

melody("var1").on(lead).step(passing_str).len(8.0).start();
```

### Building Sequences

```rhai
import "stdlib/theory/melodies/nursery_rhymes.vibe" as nursery;
import "stdlib/theory/melody_gen.vibe" as mel;

// Create a simple motif
let motif = ["C4", "E4", "G4", "C5"];

// Create ascending sequence
let sequence = mel::ascending_sequence(motif, 4, 2);

// Convert to string
let seq_str = sequence.join(" ");

melody("seq").on(lead).step(seq_str).len(8.0).start();
```

## Tips and Best Practices

### 1. Tempo Matching

Different melodies work best at different tempos:
- Classical: 80-140 BPM
- Jazz: 120-180 BPM
- Rock: 100-140 BPM
- Video game themes: 120-160 BPM

### 2. Melody Length

Most melodies in this library are 8-16 notes. Use `.len()` to control how long the pattern loops:
- `.len(4.0)` - plays in 4 beats
- `.len(8.0)` - plays in 8 beats (common)
- `.len(16.0)` - plays in 16 beats

### 3. Gate Settings

Control note duration with `.gate()`:
- `.gate(0.5)` - staccato (50% of duration)
- `.gate(0.8)` - normal (80% of duration)
- `.gate(0.95)` - legato (95% of duration)

### 4. Layering Melodies

When layering, use different octaves and timbres:

```rhai
// Bass melody (low)
let bass_voice = voice("bass").synth("sub_bass");
melody("b").on(bass_voice).step(melody1).len(8.0).start();

// Lead melody (high)
let lead_voice = voice("lead").synth("bright_lead");
melody("l").on(lead_voice).step(melody2).len(8.0).start();
```

### 5. Gain Staging

Balance melody volumes:
- Lead melodies: -6 to -3 dB
- Supporting melodies: -12 to -8 dB
- Background melodies: -18 to -12 dB

## Copyright Notice

All melodies in this library are either:
- **Public domain** (published before 1928 or traditional folk songs)
- **Short melodic fragments** used for educational/reference purposes

For copyrighted material, only brief, recognizable motifs (typically 8-16 notes) are included, which constitute factual information about the music rather than substantial reproductions.

## Contributing

To add new melodies:
1. Ensure the melody is public domain or a brief educational reference
2. Follow the function naming convention: lowercase with underscores
3. Return an array of note strings (e.g., `["C4", "E4", "G4"]`)
4. Use `.` for rests
5. Add appropriate documentation

## License

Part of the vibelang project. See main project license.

---

**Total Melodies**: 144 across 11 categories
**Last Updated**: November 2025

