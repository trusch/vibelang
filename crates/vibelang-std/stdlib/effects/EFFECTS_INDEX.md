# Effects Index

Complete alphabetical index of all effects in the standard library.

## All Effects (Alphabetical)

| Effect | Category | Description | CPU Cost |
|--------|----------|-------------|----------|
| amp_follower | Dynamics | Amplitude envelope follower | Low |
| auto_pan | Modulation | Automatic stereo panning | Low |
| bandpass | Filter | Band pass filter | Low |
| bitcrush | Distortion | Bit depth and sample rate reduction | Medium |
| chorus | Modulation | Thick chorus with pitch variation | Medium |
| comb_filter | Filter | Comb filter for resonances | Low |
| compressor | Dynamics | Dynamics compressor | Medium |
| dc_blocker | Utility | DC offset removal | Low |
| delay | Delay | Simple stereo delay | Low |
| distortion | Distortion | Hard clipping distortion | Low |
| ducking | Dynamics | Sidechain compression | Medium |
| eq_three_band | Utility | 3-band equalizer | Low |
| flanger | Modulation | Sweeping flanger effect | Medium |
| formant_filter | Filter | Vowel-like formant filtering | Low |
| gate | Dynamics | Noise gate | Medium |
| gverb | Reverb | High quality reverb with early reflections | High |
| haas | Spatial | Haas effect for stereo widening | Low |
| highpass | Filter | Resonant high pass filter | Low |
| limiter | Dynamics | Peak limiter | Medium |
| lo_fi | Character | Complete lo-fi degradation | Medium |
| lowpass | Filter | Resonant low pass filter | Low |
| moog_filter | Filter | Moog ladder filter | Low |
| overdrive | Distortion | Soft tube-like overdrive | Low |
| phaser | Modulation | All-pass phasing effect | Medium |
| ping_pong_delay | Delay | Stereo bouncing delay | Low |
| pitch_shift | Utility | Time-domain pitch shifting | High |
| plate_reverb | Reverb | Plate reverb simulation | High |
| reverb | Reverb | Simple FreeVerb reverb | Medium |
| ring_mod | Modulation | Ring modulator | Low |
| rotary | Modulation | Rotating speaker simulation | Medium |
| stereo_width | Spatial | Mid-side stereo width control | Low |
| tape_delay | Delay | Analog tape delay with wow/flutter | Medium |
| tremolo | Modulation | Amplitude modulation | Low |
| vibrato | Modulation | Pitch modulation | Low |
| vinyl | Character | Vinyl record simulation | Medium |

## By Category

### Filters (7 effects)
```
bandpass          - Band pass filter
comb_filter       - Comb filter for metallic sounds
formant_filter    - Vowel-like formant filtering
highpass          - Resonant high pass filter
lowpass           - Resonant low pass filter
moog_filter       - Classic Moog 24dB/oct ladder filter
```

### Modulation (8 effects)
```
auto_pan          - Automatic stereo panning
chorus            - Thick shimmering chorus
flanger           - Sweeping jet-like flanger
phaser            - All-pass filter phasing
ring_mod          - Ring modulator
rotary            - Rotating speaker (Leslie)
tremolo           - Amplitude modulation
vibrato           - Pitch modulation
```

### Delay (3 effects)
```
delay             - Simple stereo delay
ping_pong_delay   - Stereo bouncing delay
tape_delay        - Analog tape delay with wow/flutter
```

### Reverb (3 effects)
```
gverb             - High quality reverb
plate_reverb      - Plate reverb simulation
reverb            - Simple FreeVerb reverb
```

### Dynamics (5 effects)
```
amp_follower      - Amplitude envelope follower
compressor        - Dynamics compressor
ducking           - Sidechain compression
gate              - Noise gate
limiter           - Peak limiter
```

### Distortion (3 effects)
```
bitcrush          - Bit depth and sample rate reduction
distortion        - Hard clipping distortion
overdrive         - Soft tube-like overdrive
```

### Spatial (2 effects)
```
haas              - Haas effect for widening
stereo_width      - Mid-side stereo width control
```

### Character (2 effects)
```
lo_fi             - Complete lo-fi degradation
vinyl             - Vinyl record simulation
```

### Utility (3 effects)
```
dc_blocker        - DC offset removal
eq_three_band     - 3-band equalizer
pitch_shift       - Time-domain pitch shifting
```

## Quick Parameter Reference

### Common Parameters

**Mix/Wet-Dry**
- Most effects have a `mix` parameter (0.0 = dry, 1.0 = wet)
- Typical values: 0.2-0.5 for subtle, 0.6-1.0 for obvious

**Filter Cutoff**
- Range: 20 Hz to 20,000 Hz
- Typical values: 500-5000 Hz depending on material

**Resonance/Q**
- Range: 0.0 to 1.0 (or higher for some)
- Low values = more resonance (paradoxical!)
- Be careful with very low values (< 0.1)

**Delay Time**
- Range: 0.01 to 2.0 seconds
- Tempo-synced: 0.125 (32nd), 0.25 (16th), 0.375 (dotted 16th), 0.5 (8th)

**Feedback**
- Range: 0.0 to 0.95
- Values > 0.95 can cause runaway feedback

**LFO Rate**
- Range: 0.05 to 20.0 Hz
- Typical: 0.1-1.0 Hz for slow, 2-10 Hz for fast

**Attack/Release**
- Range: 0.001 to 1.0 seconds
- Fast: 0.001-0.01s, Medium: 0.01-0.1s, Slow: 0.1-1.0s

## Effect Chain Examples

### Synth Lead Chain
```
lowpass → chorus → delay → reverb
```

### Bass Chain
```
overdrive → eq_three_band → compressor
```

### Vocal Chain
```
eq_three_band → compressor → chorus → reverb
```

### Drum Bus Chain
```
eq_three_band → compressor → tape_delay → reverb
```

### Lo-Fi Hip-Hop Chain
```
bitcrush → lo_fi → vinyl → reverb
```

### Ambient Pad Chain
```
chorus → rotary → plate_reverb → stereo_width
```

### Master Chain
```
eq_three_band → compressor → limiter
```

## CPU Optimization Tips

1. **Use effects on groups, not individual voices**
   - One reverb on a drum group is better than reverb on each drum

2. **Order by CPU cost**
   - Put cheap effects first, expensive ones last
   - Example: filter → chorus → delay → reverb

3. **Bypass unused effects**
   - Use `effect.bypass(true)` instead of removing

4. **Choose wisely**
   - Simple reverb vs. GVerb vs. Plate
   - DelayN vs. DelayL vs. DelayC (N is cheapest)

5. **Limit reverb usage**
   - Maximum 2-3 reverbs in a project
   - Use sends (group effects) rather than inserts

## See Also
- [README.md](README.md) - Full documentation
- [all_effects.vibe](all_effects.vibe) - Load all effects
- [effects_demo.vibe](../examples/effects_demo.vibe) - Demo file

