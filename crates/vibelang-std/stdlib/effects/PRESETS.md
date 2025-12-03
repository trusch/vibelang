# Effect Presets and Combinations

Common effect settings and chains for different musical contexts.

## Reverb Presets

### Small Room
```rhai
fx("room").synth("reverb")
    .param("room", 0.3)
    .param("damp", 0.6)
    .param("mix", 0.15)
    .apply();
```

### Medium Hall
```rhai
fx("hall").synth("reverb")
    .param("room", 0.7)
    .param("damp", 0.4)
    .param("mix", 0.3)
    .apply();
```

### Large Cathedral
```rhai
fx("cathedral").synth("gverb")
    .param("roomsize", 100.0)
    .param("revtime", 5.0)
    .param("damping", 0.3)
    .param("spread", 25.0)
    .param("drylevel", 0.5)
    .param("earlylevel", 0.7)
    .param("taillevel", 0.6)
    .apply();
```

### Plate Reverb (Vintage)
```rhai
fx("plate").synth("plate_reverb")
    .param("time", 2.5)
    .param("damping", 0.5)
    .param("mix", 0.35)
    .apply();
```

### Ambient Space
```rhai
fx("ambient").synth("gverb")
    .param("roomsize", 150.0)
    .param("revtime", 8.0)
    .param("damping", 0.2)
    .param("spread", 30.0)
    .param("drylevel", 0.3)
    .param("earlylevel", 0.5)
    .param("taillevel", 0.8)
    .apply();
```

## Delay Presets

### Slapback Echo (Rockabilly)
```rhai
fx("slapback").synth("delay")
    .param("time", 0.12)
    .param("feedback", 0.2)
    .param("mix", 0.3)
    .apply();
```

### Quarter Note Delay
```rhai
fx("quarter").synth("delay")
    .param("time", 0.5)  // 120 BPM
    .param("feedback", 0.4)
    .param("mix", 0.25)
    .apply();
```

### Dotted Eighth (The Edge Style)
```rhai
fx("dotted").synth("delay")
    .param("time", 0.375)  // 120 BPM
    .param("feedback", 0.5)
    .param("mix", 0.4)
    .apply();
```

### Ping Pong Rhythmic
```rhai
fx("ping").synth("ping_pong_delay")
    .param("time", 0.375)
    .param("feedback", 0.65)
    .param("mix", 0.35)
    .apply();
```

### Tape Echo (Dub Style)
```rhai
fx("dub").synth("tape_delay")
    .param("time", 0.5)
    .param("feedback", 0.7)
    .param("wow", 0.5)
    .param("flutter", 0.3)
    .param("mix", 0.6)
    .apply();
```

## Filter Presets

### Acid Bass Sweep
```rhai
fx("acid").synth("moog_filter")
    .param("cutoff", 200.0)
    .param("resonance", 3.5)
    .param("mix", 1.0)
    .apply();
// Then: filter.fade("cutoff").to(3000.0).over("4bar").apply();
```

### Telephone Effect
```rhai
fx("phone").synth("bandpass")
    .param("freq", 1000.0)
    .param("bandwidth", 0.3)
    .param("mix", 1.0)
    .apply();
```

### Radio Effect
```rhai
fx("radio").synth("bandpass")
    .param("freq", 2500.0)
    .param("bandwidth", 0.5)
    .param("mix", 1.0)
    .apply();
```

### Low Pass Sweep
```rhai
fx("sweep").synth("lowpass")
    .param("cutoff", 400.0)
    .param("resonance", 0.3)
    .param("mix", 1.0)
    .apply();
```

## Modulation Presets

### Subtle Chorus
```rhai
fx("subtle_chorus").synth("chorus")
    .param("rate", 0.3)
    .param("depth", 0.005)
    .param("mix", 0.25)
    .apply();
```

### Lush Chorus
```rhai
fx("lush").synth("chorus")
    .param("rate", 0.7)
    .param("depth", 0.015)
    .param("mix", 0.5)
    .apply();
```

### Jet Flanger
```rhai
fx("jet").synth("flanger")
    .param("rate", 0.2)
    .param("depth", 0.008)
    .param("feedback", 0.7)
    .param("mix", 0.6)
    .apply();
```

### Phaser Sweep
```rhai
fx("phase").synth("phaser")
    .param("rate", 0.4)
    .param("depth", 1500.0)
    .param("freq", 800.0)
    .param("feedback", 0.5)
    .param("mix", 0.5)
    .apply();
```

### Rotary Speaker (Slow)
```rhai
fx("leslie_slow").synth("rotary")
    .param("rate", 1.0)
    .param("depth", 0.003)
    .param("mix", 1.0)
    .apply();
```

### Rotary Speaker (Fast)
```rhai
fx("leslie_fast").synth("rotary")
    .param("rate", 6.0)
    .param("depth", 0.005)
    .param("mix", 1.0)
    .apply();
```

### Tremolo (Surf)
```rhai
fx("surf").synth("tremolo")
    .param("rate", 6.0)
    .param("depth", 0.8)
    .param("mix", 1.0)
    .apply();
```

### Auto Pan (Slow)
```rhai
fx("slow_pan").synth("auto_pan")
    .param("rate", 0.1)
    .param("depth", 0.9)
    .param("mix", 1.0)
    .apply();
```

## Distortion Presets

### Light Overdrive
```rhai
fx("light_od").synth("overdrive")
    .param("drive", 3.0)
    .param("tone", 5000.0)
    .param("mix", 0.6)
    .apply();
```

### Heavy Distortion
```rhai
fx("heavy").synth("distortion")
    .param("drive", 20.0)
    .param("mix", 0.8)
    .apply();
```

### Fuzz
```rhai
fx("fuzz").synth("distortion")
    .param("drive", 40.0)
    .param("mix", 0.9)
    .apply();
```

### Warm Saturation
```rhai
fx("warm").synth("overdrive")
    .param("drive", 2.5)
    .param("tone", 4000.0)
    .param("mix", 0.5)
    .apply();
```

## Lo-Fi / Character Presets

### 8-Bit Console
```rhai
fx("8bit").synth("bitcrush")
    .param("bits", 4.0)
    .param("sample_rate", 8000.0)
    .param("mix", 1.0)
    .apply();
```

### Lo-Fi Hip-Hop
```rhai
fx("lofi_hh").synth("lo_fi")
    .param("bit_depth", 12.0)
    .param("sample_rate", 22050.0)
    .param("noise", 0.02)
    .param("filter_freq", 4500.0)
    .param("mix", 0.7)
    .apply();
```

### Vinyl Warmth
```rhai
fx("vinyl_warm").synth("vinyl")
    .param("crackle", 0.02)
    .param("dust", 0.01)
    .param("wow", 0.2)
    .param("flutter", 0.1)
    .param("wear", 5000.0)
    .param("mix", 0.5)
    .apply();
```

### Worn Vinyl
```rhai
fx("vinyl_worn").synth("vinyl")
    .param("crackle", 0.08)
    .param("dust", 0.04)
    .param("wow", 0.5)
    .param("flutter", 0.3)
    .param("wear", 3000.0)
    .param("mix", 0.8)
    .apply();
```

## Complete Effect Chains

### Ambient Pad Chain
```rhai
// Pad sound with atmospheric processing
fx("pad_chorus").synth("chorus")
    .param("rate", 0.3)
    .param("depth", 0.01)
    .param("mix", 0.4)
    .apply();

fx("pad_delay").synth("tape_delay")
    .param("time", 0.75)
    .param("feedback", 0.5)
    .param("wow", 0.3)
    .param("flutter", 0.2)
    .param("mix", 0.3)
    .apply();

fx("pad_reverb").synth("gverb")
    .param("roomsize", 120.0)
    .param("revtime", 6.0)
    .param("damping", 0.3)
    .param("spread", 25.0)
    .param("drylevel", 0.4)
    .param("earlylevel", 0.6)
    .param("taillevel", 0.7)
    .apply();

fx("pad_width").synth("stereo_width")
    .param("width", 1.8)
    .apply();
```

### Synth Lead Chain
```rhai
// Cutting lead sound
fx("lead_filter").synth("lowpass")
    .param("cutoff", 3000.0)
    .param("resonance", 0.4)
    .param("mix", 1.0)
    .apply();

fx("lead_chorus").synth("chorus")
    .param("rate", 0.5)
    .param("depth", 0.008)
    .param("mix", 0.3)
    .apply();

fx("lead_delay").synth("ping_pong_delay")
    .param("time", 0.375)
    .param("feedback", 0.5)
    .param("mix", 0.25)
    .apply();

fx("lead_reverb").synth("plate_reverb")
    .param("time", 2.0)
    .param("damping", 0.4)
    .param("mix", 0.2)
    .apply();
```

### Bass Chain
```rhai
// Punchy bass with character
fx("bass_filter").synth("highpass")
    .param("cutoff", 40.0)
    .param("resonance", 0.7)
    .param("mix", 1.0)
    .apply();

fx("bass_drive").synth("overdrive")
    .param("drive", 4.0)
    .param("tone", 2500.0)
    .param("mix", 0.4)
    .apply();

fx("bass_comp").synth("compressor")
    .param("threshold", 0.5)
    .param("ratio", 4.0)
    .param("attack", 0.01)
    .param("release", 0.1)
    .apply();
```

### Drum Bus Chain
```rhai
// Cohesive drum sound
fx("drum_eq").synth("eq_three_band")
    .param("low_gain", 2.0)
    .param("mid_gain", 0.0)
    .param("high_gain", 1.5)
    .param("low_freq", 100.0)
    .param("high_freq", 5000.0)
    .apply();

fx("drum_comp").synth("compressor")
    .param("threshold", 0.6)
    .param("ratio", 3.0)
    .param("attack", 0.005)
    .param("release", 0.05)
    .apply();

fx("drum_reverb").synth("reverb")
    .param("room", 0.5)
    .param("damp", 0.6)
    .param("mix", 0.15)
    .apply();
```

### Vocal Chain
```rhai
// Clear vocal presence
fx("vocal_eq").synth("eq_three_band")
    .param("low_gain", -2.0)
    .param("mid_gain", 2.0)
    .param("high_gain", 1.0)
    .param("low_freq", 200.0)
    .param("high_freq", 4000.0)
    .apply();

fx("vocal_comp").synth("compressor")
    .param("threshold", 0.5)
    .param("ratio", 4.0)
    .param("attack", 0.01)
    .param("release", 0.15)
    .apply();

fx("vocal_chorus").synth("chorus")
    .param("rate", 0.3)
    .param("depth", 0.005)
    .param("mix", 0.15)
    .apply();

fx("vocal_delay").synth("delay")
    .param("time", 0.375)
    .param("feedback", 0.3)
    .param("mix", 0.2)
    .apply();

fx("vocal_reverb").synth("plate_reverb")
    .param("time", 2.5)
    .param("damping", 0.4)
    .param("mix", 0.25)
    .apply();
```

### Master Chain
```rhai
// Final processing
fx("master_eq").synth("eq_three_band")
    .param("low_gain", 0.5)
    .param("mid_gain", 0.0)
    .param("high_gain", 1.0)
    .param("low_freq", 250.0)
    .param("high_freq", 4000.0)
    .apply();

fx("master_comp").synth("compressor")
    .param("threshold", 0.7)
    .param("ratio", 2.5)
    .param("attack", 0.01)
    .param("release", 0.1)
    .apply();

fx("master_width").synth("stereo_width")
    .param("width", 1.2)
    .apply();

fx("master_limit").synth("limiter")
    .param("level", 0.95)
    .param("lookahead", 0.01)
    .apply();
```

## Genre-Specific Chains

### Dub / Reggae
```rhai
fx("dub_delay").synth("tape_delay")
    .param("time", 0.5)
    .param("feedback", 0.75)
    .param("wow", 0.6)
    .param("flutter", 0.4)
    .param("mix", 0.7)
    .apply();

fx("dub_filter").synth("lowpass")
    .param("cutoff", 1500.0)
    .param("resonance", 0.5)
    .param("mix", 0.8)
    .apply();

fx("dub_reverb").synth("gverb")
    .param("roomsize", 80.0)
    .param("revtime", 4.0)
    .param("mix", 0.5)
    .apply();
```

### Shoegaze / Dream Pop
```rhai
fx("gaze_reverb").synth("plate_reverb")
    .param("time", 6.0)
    .param("damping", 0.2)
    .param("mix", 0.6)
    .apply();

fx("gaze_chorus").synth("chorus")
    .param("rate", 0.4)
    .param("depth", 0.02)
    .param("mix", 0.7)
    .apply();

fx("gaze_delay").synth("tape_delay")
    .param("time", 0.75)
    .param("feedback", 0.6)
    .param("wow", 0.4)
    .param("flutter", 0.3)
    .param("mix", 0.4)
    .apply();
```

### Lo-Fi Hip-Hop
```rhai
fx("lofi_crush").synth("lo_fi")
    .param("bit_depth", 12.0)
    .param("sample_rate", 20000.0)
    .param("noise", 0.025)
    .param("filter_freq", 4000.0)
    .param("mix", 0.8)
    .apply();

fx("lofi_vinyl").synth("vinyl")
    .param("crackle", 0.04)
    .param("dust", 0.02)
    .param("wow", 0.3)
    .param("flutter", 0.2)
    .param("wear", 4500.0)
    .param("mix", 0.6)
    .apply();

fx("lofi_verb").synth("reverb")
    .param("room", 0.4)
    .param("damp", 0.7)
    .param("mix", 0.25)
    .apply();
```

### Techno / Electronic
```rhai
fx("techno_filter").synth("moog_filter")
    .param("cutoff", 800.0)
    .param("resonance", 2.8)
    .param("mix", 1.0)
    .apply();

fx("techno_delay").synth("ping_pong_delay")
    .param("time", 0.375)
    .param("feedback", 0.5)
    .param("mix", 0.3)
    .apply();

fx("techno_reverb").synth("reverb")
    .param("room", 0.6)
    .param("damp", 0.5)
    .param("mix", 0.2)
    .apply();
```

## Tips for Creating Your Own Presets

1. **Start Minimal**: Add one effect at a time
2. **Listen Critically**: A/B with bypass to hear the difference
3. **Automate**: Use `.fade()` for movement and interest
4. **Context Matters**: Mix affects appropriate amounts
5. **CPU Budget**: Monitor performance with complex chains
6. **Save Combinations**: Document what works for your style

## See Also
- [EFFECTS_INDEX.md](EFFECTS_INDEX.md) - Complete effect listing
- [README.md](README.md) - Full documentation

