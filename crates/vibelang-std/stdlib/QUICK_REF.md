# Vibelang Standard Library - Quick Reference

## Total: 187 Synthdefs ⭐

### Drums (62) ⭐
- **Kicks**: 12 sounds (808, 909, techno deep, techno hard, dnb, trap, acoustic, sub, pitched, fm, distorted, soft)
- **Snares**: 12 sounds (808, 909, acoustic, clap-snare, piccolo, rimshot, tight electronic, loose acoustic, layered, reverb, filtered, lofi)
- **Hihats**: 10 sounds (808 closed/open, 909 closed/open, metallic, short, long, filtered, dusty, splash)
- **Claps**: 10 sounds (808, short, reverb, layered, crowd, tight, loose, filtered, lofi, handclap)
- **Percussion**: 10 sounds (tom high/mid/low, rim, cowbell, clave, woodblock, shaker, tambourine, snap)
- **Cymbals**: 8 sounds (crash bright/dark, ride, splash, china, reverse, swell, bell ping) ⭐ NEW

### Bass (47) ⭐
- **Sub**: 10 sounds (pure sine, triangle, filtered, octave, modulated, warm, deep, mono, stereo, harmonic)
- **Acid**: 10 sounds (303 classic, squelchy, distorted, bubbly, minimal, aggressive, modulated, detuned, filtered square, resonant sweep)
- **Pluck**: 10 sounds (short, long, bright, dark, resonant, muted, funky, elastic, bell, percussive)
- **Reese**: 6 sounds (classic, deep, aggressive, smooth, evolving, distorted) ⭐ NEW
- **Wobble**: 6 sounds (classic, aggressive, smooth, squelch, deep, fm) ⭐ NEW
- **FM**: 5 sounds (classic, deep, metallic, evolving, aggressive) ⭐ NEW

### Leads (28) ⭐
- **Synth**: 10 sounds (saw, square, supersaw, detuned, pwm, filtered, bright, dark, aggressive, smooth)
- **Pluck**: 10 sounds (bright, bell, marimba, kalimba, harp, piano, short, long, resonant, muted)
- **Stabs**: 8 sounds (bright, dark, brass, super, short, distorted, piano, orchestral) ⭐ NEW

### Pads (20)
- **Ambient**: 10 sounds (dark, bright, evolving, sparse, dense, filtered, shimmer, warm, cold, space)
- **Lush**: 10 sounds (detuned, chorus, wide, narrow, string, voice, organ, soft, aggressive, morphing)

### FX (20) ⭐
- **Risers**: 5 sounds (white noise, pink noise, filtered, pitch, reverse)
- **Impacts**: 5 sounds (hard, soft, metallic, sub, noise)
- **Sweeps**: 5 sounds (filter up, filter down, pitch, whoosh, reverse)
- **Sub Drops**: 5 sounds (classic, deep, distorted, click, long) ⭐ NEW

### Textures (10)
- **Ambient**: 5 sounds (wind, rain, space, granular, field)
- **Drone**: 5 sounds (dark, harmonic, noise, evolving, resonant)

## Usage Examples

### Basic Drum Pattern
```rhai
// Load kick
let kick = voice("kick").synth("kick_808").gain(db(0));

pattern("beat")
    .on(kick)
    .step("x...x...x...x...")
    .len(4.0)
    .start();
```

### Bass Line
```rhai
// Load acid bass
let bass = voice("bass").synth("acid_303_classic").gain(db(-6));

melody("bassline")
    .on(bass)
    .step("C2 . E2 . G2 . F2 .")
    .len(4.0)
    .start();
```

### Pad Background
```rhai
// Load ambient pad
let pad = voice("pad").synth("pad_space").gain(db(-20));

melody("pad_chord")
    .on(pad)
    .step("C3")
    .len(8.0)
    .gate(0.9)
    .start();
```

### FX Transitions
```rhai
// Riser into drop
let riser = voice("riser").synth("riser_white_noise").gain(db(-3));

melody("build")
    .on(riser)
    .step("C4")
    .len(4.0)
    .start();
```

## File Organization

Each sound is in its own `.vibe` file:
```
stdlib/
├── drums/kicks/kick_808.vibe
├── bass/acid/acid_303_classic.vibe
├── leads/synth/lead_saw.vibe
└── ...
```

## Coverage by Genre

- **Hip-hop/Trap**: 808/909 drums, trap kick, lofi sounds, sub drops
- **Techno/House**: 909 drums, acid bass, synth leads, cymbals
- **Drum & Bass**: DNB kick, Reese bass, tight drums, fast elements ⭐
- **Dubstep/Bass**: Sub kicks, wobble bass, sub drops, impacts ⭐
- **Brostep**: Aggressive wobbles, distorted sounds, heavy stabs ⭐
- **Ambient**: Pads, textures, drones, soft sounds
- **Pop/EDM**: Layered sounds, bright leads, stabs, FX ⭐
- **Trance**: Supersaw leads, bright stabs, pitch risers
- **Experimental**: FM sounds, noise textures, resonant elements

## Sound Design Features

✓ Classic drum machine sounds (808, 909)
✓ Complete drum kit with cymbals ⭐
✓ Multiple bass synthesis techniques (sub, acid, pluck, Reese, wobble, FM) ⭐
✓ Rich lead sounds with movement and stabs ⭐
✓ Evolving pads and textures  
✓ Professional FX including sub drops ⭐
✓ Modern bass music essentials (Reese, wobble) ⭐
✓ Dubstep/DnB production tools ⭐
✓ Ambient and experimental sounds
✓ Genre-specific variations
✓ Multi-genre coverage (9+ genres)

## Getting Started

1. Browse the `stdlib/` directory
2. Pick sounds that fit your genre
3. Include them in your `.vibe` file
4. Start making music!

The library is designed to be comprehensive enough for complete productions while remaining organized and easy to navigate.

