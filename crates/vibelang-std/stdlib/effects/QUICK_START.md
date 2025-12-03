# Effects Quick Start Guide

Get started with Vibelang effects in 5 minutes!

## Install

Effects are already in the standard library. Just import what you need:

```rhai
// Import specific effects
import "stdlib/effects/reverb.vibe";
import "stdlib/effects/delay.vibe";

// Or import everything
import "stdlib/effects/all_effects.vibe";
```

## Basic Usage

### 1. Add Effect to a Group

```rhai
define_group("Drums", ||{
    // ... your voices and patterns ...
    
    // Add reverb
    fx("drums_verb")
        .synth("reverb")
        .param("room", 0.7)
        .param("damp", 0.5)
        .param("mix", 0.3)
        .apply();
});
```

### 2. Control Effect Parameters

```rhai
// Get effect handle
let verb = get_effect("drums_verb");

// Change parameter instantly
verb.set("room", 0.9);

// Fade parameter over time
verb.fade("mix").to(0.5).over("4bar").apply();

// Bypass effect
verb.bypass(true);  // off
verb.bypass(false); // on
```

## Essential Effects

### Reverb
```rhai
fx("verb").synth("reverb")
    .param("room", 0.7)      // 0.0-1.0, room size
    .param("damp", 0.5)      // 0.0-1.0, damping
    .param("mix", 0.3)       // 0.0-1.0, wet amount
    .apply();
```

### Delay
```rhai
fx("delay").synth("delay")
    .param("time", 0.375)    // seconds
    .param("feedback", 0.5)  // 0.0-0.95
    .param("mix", 0.3)       // 0.0-1.0
    .apply();
```

### Low Pass Filter
```rhai
fx("lpf").synth("lowpass")
    .param("cutoff", 2000.0)  // Hz
    .param("resonance", 0.5)  // 0.0-1.0 (lower = more)
    .param("mix", 1.0)        // usually 1.0 for filters
    .apply();
```

### Compressor
```rhai
fx("comp").synth("compressor")
    .param("threshold", 0.6)  // 0.0-1.0
    .param("ratio", 4.0)      // 2.0-20.0
    .param("attack", 0.01)    // seconds
    .param("release", 0.1)    // seconds
    .apply();
```

### Chorus
```rhai
fx("chorus").synth("chorus")
    .param("rate", 0.5)      // Hz
    .param("depth", 0.01)    // seconds
    .param("mix", 0.4)       // 0.0-1.0
    .apply();
```

## Common Patterns

### Effect Chain
```rhai
// Order matters: EQ → Compression → Modulation → Delay → Reverb
fx("eq").synth("eq_three_band")
    .param("low_gain", 1.0)
    .param("mid_gain", 0.0)
    .param("high_gain", 1.5)
    .apply();

fx("comp").synth("compressor")
    .param("threshold", 0.6)
    .param("ratio", 3.0)
    .apply();

fx("chorus").synth("chorus")
    .param("rate", 0.5)
    .param("mix", 0.3)
    .apply();

fx("delay").synth("delay")
    .param("time", 0.375)
    .param("mix", 0.25)
    .apply();

fx("reverb").synth("reverb")
    .param("room", 0.7)
    .param("mix", 0.3)
    .apply();
```

### Filter Sweep
```rhai
// Start with closed filter
fx("sweep").synth("moog_filter")
    .param("cutoff", 200.0)
    .param("resonance", 3.0)
    .param("mix", 1.0)
    .apply();

// Sweep it up over 8 bars
get_effect("sweep").fade("cutoff").to(3000.0).over("8bar").apply();
```

### Master Chain
```rhai
define_group("Master", ||{
    fx("master_comp").synth("compressor")
        .param("threshold", 0.7)
        .param("ratio", 2.5)
        .apply();
    
    fx("master_limit").synth("limiter")
        .param("level", 0.95)
        .apply();
});
```

## Top 10 Effects to Start With

1. **reverb** - Essential for space and depth
2. **delay** - Rhythmic echoes and ambience
3. **lowpass** - Tone shaping and filtering
4. **compressor** - Dynamics control
5. **chorus** - Width and thickness
6. **eq_three_band** - Frequency balance
7. **distortion** - Grit and aggression
8. **limiter** - Prevent clipping (master)
9. **tremolo** - Rhythmic volume changes
10. **stereo_width** - Control stereo field

## Mix Tips

### Reverb Levels
- Drums: 10-20% (mix 0.1-0.2)
- Synths: 20-40% (mix 0.2-0.4)
- Pads: 40-60% (mix 0.4-0.6)

### Delay Levels
- Subtle: 15-25% (mix 0.15-0.25)
- Present: 30-50% (mix 0.3-0.5)
- Obvious: 50-70% (mix 0.5-0.7)

### Compression Ratios
- Light: 2:1 to 3:1
- Medium: 3:1 to 6:1
- Heavy: 6:1 to 12:1
- Limiting: 12:1 to 20:1

## Troubleshooting

### No Sound
- Check effect mix parameter (not 0.0)
- Verify effect isn't bypassed
- Check that effect is defined (`import` the file)

### Distortion/Clipping
- Lower effect wet/dry mix
- Use limiter on master
- Check input levels
- Reduce feedback on delays

### CPU Issues
- Use simpler effects (FreeVerb vs GVerb)
- Apply effects to groups, not individual voices
- Bypass unused effects
- Reduce number of reverbs

### Thin Sound
- Increase stereo width
- Add chorus or flanger
- Check if too much high-pass filtering

## Next Steps

1. **Read** [README.md](README.md) for complete documentation
2. **Browse** [EFFECTS_INDEX.md](EFFECTS_INDEX.md) for all effects
3. **Try** [PRESETS.md](PRESETS.md) for ready-made settings
4. **Run** `vibe examples/effects_demo.vibe` to hear examples
5. **Experiment** with parameter automation using `.fade()`

## Effect Categories

- **Filters** (7): lowpass, highpass, bandpass, moog_filter, comb_filter, formant_filter
- **Modulation** (8): chorus, flanger, phaser, tremolo, vibrato, auto_pan, rotary, ring_mod
- **Delay** (3): delay, ping_pong_delay, tape_delay
- **Reverb** (3): reverb, gverb, plate_reverb
- **Dynamics** (5): compressor, limiter, gate, ducking, amp_follower
- **Distortion** (3): distortion, overdrive, bitcrush
- **Spatial** (2): stereo_width, haas
- **Character** (2): lo_fi, vinyl
- **Utility** (3): eq_three_band, dc_blocker, pitch_shift

**Total: 36 effects**

Happy music making! 🎵

