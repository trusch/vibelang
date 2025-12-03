# Vibelang Effects Library

A comprehensive collection of audio effects for the Vibelang standard library. All effects properly use SuperCollider UGens and follow the effects system conventions.

## Effect Categories

### Filters
- **lowpass** - Resonant low pass filter
- **highpass** - Resonant high pass filter  
- **bandpass** - Band pass filter
- **moog_filter** - Classic Moog 24dB/oct ladder filter with self-oscillation
- **comb_filter** - Comb filter for metallic/robotic sounds
- **formant_filter** - Vowel-like formant filtering

### Modulation Effects
- **chorus** - Thick, shimmering chorus with pitch variation
- **flanger** - Sweeping jet-like flanging effect
- **phaser** - All-pass filter phasing with LFO
- **tremolo** - Amplitude modulation
- **vibrato** - Pitch modulation
- **auto_pan** - Automatic stereo panning
- **rotary** - Rotating speaker (Leslie) simulation
- **ring_mod** - Ring modulator for metallic tones

### Delay Effects
- **delay** - Simple stereo delay with feedback
- **ping_pong_delay** - Stereo delay with left-right bouncing
- **tape_delay** - Analog-style tape delay with wow/flutter

### Reverb Effects
- **reverb** - FreeVerb-based simple reverb
- **gverb** - High quality reverb with early reflections
- **plate_reverb** - Plate reverb simulation using allpass/comb filters

### Dynamics
- **compressor** - Dynamics compressor
- **limiter** - Peak limiter
- **gate** - Noise gate
- **ducking** - Sidechain compression/ducking
- **amp_follower** - Amplitude envelope follower

### Distortion/Saturation
- **distortion** - Hard clipping distortion
- **overdrive** - Soft tube-like overdrive with tone control
- **bitcrush** - Lo-fi bit depth and sample rate reduction

### Spatial Effects
- **stereo_width** - Mid-side stereo width control
- **haas** - Haas effect (precedence effect) for stereo widening

### Character/Texture
- **lo_fi** - Complete lo-fi degradation (bit crushing, noise, filtering)
- **vinyl** - Vinyl record simulation with crackle, dust, wow/flutter

### Utility
- **eq_three_band** - Simple 3-band equalizer
- **dc_blocker** - DC offset removal
- **pitch_shift** - Time-domain pitch shifting

## Usage Examples

### Basic Effect Usage

```rhai
// Define a group with effects
define_group("Synths", ||{
    // ... voices and patterns ...
    
    // Add reverb
    fx("synth_reverb")
        .synth("reverb")
        .param("room", 0.8)
        .param("damp", 0.5)
        .param("mix", 0.3)
        .apply();
    
    // Add delay
    fx("synth_delay")
        .synth("delay")
        .param("time", 0.375)
        .param("feedback", 0.5)
        .param("mix", 0.2)
        .apply();
});
```

### Filter Effect

```rhai
// Moog filter with modulation
let moog = group.add_effect("moog", "moog_filter", #{
    cutoff: 500.0,
    resonance: 3.0,
    mix: 1.0
});

// Sweep the filter over time
moog.fade("cutoff").to(3000.0).over("8bar").apply();
```

### Modulation Chain

```rhai
// Create a modulation effect chain
fx("chorus_fx")
    .synth("chorus")
    .param("rate", 0.5)
    .param("depth", 0.01)
    .param("mix", 0.4)
    .apply();

fx("flanger_fx")
    .synth("flanger")
    .param("rate", 0.3)
    .param("depth", 0.005)
    .param("feedback", 0.5)
    .param("mix", 0.3)
    .apply();
```

### Lo-Fi Character

```rhai
// Add lo-fi character to drums
fx("drums_lofi")
    .synth("lo_fi")
    .param("bit_depth", 10.0)
    .param("sample_rate", 16000.0)
    .param("noise", 0.03)
    .param("filter_freq", 4000.0)
    .param("mix", 0.6)
    .apply();
```

### Vinyl Effect

```rhai
// Simulate vinyl record
fx("vinyl_fx")
    .synth("vinyl")
    .param("crackle", 0.05)
    .param("dust", 0.02)
    .param("wow", 0.3)
    .param("flutter", 0.2)
    .param("wear", 4000.0)
    .param("mix", 0.8)
    .apply();
```

### Spatial Processing

```rhai
// Widen stereo field
fx("width")
    .synth("stereo_width")
    .param("width", 1.5)
    .apply();

// Add Haas effect for width
fx("haas")
    .synth("haas")
    .param("delay_time", 0.015)
    .param("mix", 0.5)
    .apply();
```

### Master Chain

```rhai
// Typical mastering chain
define_group("Master", ||{
    // EQ
    fx("master_eq")
        .synth("eq_three_band")
        .param("low_gain", 1.0)
        .param("mid_gain", 0.0)
        .param("high_gain", 2.0)
        .param("low_freq", 250.0)
        .param("high_freq", 4000.0)
        .apply();
    
    // Compression
    fx("master_comp")
        .synth("compressor")
        .param("threshold", 0.6)
        .param("ratio", 3.0)
        .param("attack", 0.005)
        .param("release", 0.1)
        .apply();
    
    // Limiter
    fx("master_limit")
        .synth("limiter")
        .param("level", 0.95)
        .param("lookahead", 0.01)
        .apply();
});
```

## Effect Parameters

All effects support these common patterns:

### Setting Parameters
```rhai
effect.set("parameter_name", value);
```

### Fading Parameters
```rhai
effect.fade("parameter_name").to(target_value).over("4bar").apply();
```

### Bypassing
```rhai
effect.bypass(true);  // disable effect
effect.bypass(false); // enable effect
```

## Tips for Using Effects

1. **Order Matters**: Effects are processed in the order they're added
   - Typical order: EQ → Compression → Modulation → Delay → Reverb

2. **CPU Considerations**: 
   - Reverbs are CPU-intensive
   - Pitch shifters are expensive
   - Simple filters are very efficient

3. **Mix Levels**:
   - Start with lower mix values and increase
   - Reverb: 0.2-0.4 mix typical
   - Delay: 0.2-0.5 mix typical
   - Modulation: 0.3-0.6 mix typical

4. **Resonance/Feedback**:
   - Be careful with high resonance values
   - High feedback can cause runaway gain
   - Monitor output levels

5. **Stereo Width**:
   - Values > 2.0 can cause phase issues
   - Check mono compatibility

6. **Bit Crushing**:
   - Lower bit depths (4-8) are very aggressive
   - Sample rate < 8kHz is quite degraded

## Technical Notes

### Bus Routing
- Effects receive an `input` array (one NodeRef per channel) from the runtime
- Return an array of processed channels and the system writes it back to the group bus
- No manual calls to `in_ar`/`replace_out_ar` or bus bookkeeping are required

### UGen Usage
All effects use proper SuperCollider UGens:
- Filters: `RLPF`, `RHPF`, `BPF`, `MoogFF`, etc.
- Delays: `DelayN`, `DelayL`, `DelayC`, `CombL`, `AllpassN`, etc.
- Reverbs: `FreeVerb`, `GVerb`
- Dynamics: `Compander`, `Limiter`
- Modulation: `SinOsc`, `LFNoise`, oscillators for LFOs
- Panning: `Pan2`, `Balance2`

### Stereo Processing
Most effects process stereo signals (2 channels):
- Left and right are processed separately or together
- Some effects (reverbs) mix to mono internally
- Modulation effects often use phase-offset LFOs for stereo width

## Performance Guidelines

### Low CPU
- Simple filters (LPF, HPF, BPF)
- Tremolo
- DC blocker
- Gate

### Medium CPU
- Chorus
- Flanger
- Phaser
- Delay effects
- Compressor

### High CPU
- Reverbs (especially GVerb)
- Pitch shifter
- Complex multi-stage effects (plate reverb, vinyl)
- Bitcrush at very low sample rates

## Contributing

When adding new effects:
1. Use proper SuperCollider UGens
2. Follow the `define_fx()` convention
3. Include comprehensive documentation
4. Test with stereo signals
5. Provide sensible default parameters
6. Include usage examples

## See Also
- [Effects System Quick Reference](../EFFECTS_QUICK_REF.md)
- [Object Handles API](../OBJECT_HANDLES_API.md)
- [UGen Manifests](../../ugen_manifests/)
