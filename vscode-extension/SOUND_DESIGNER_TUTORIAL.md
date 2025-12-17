# VibeLang Sound Designer Tutorial

A comprehensive guide to building sophisticated synthesizers using the visual Sound Designer.

## Table of Contents
1. [Getting Started](#getting-started)
2. [Interface Overview](#interface-overview)
3. [Tutorial: Building a Classic Subtractive Synth](#tutorial-building-a-classic-subtractive-synth)
4. [Advanced Techniques](#advanced-techniques)
5. [Connecting to VibeLang Runtime](#connecting-to-vibelang-runtime)

---

## Getting Started

### Opening the Sound Designer

1. Open VS Code with the VibeLang extension installed
2. Press `Ctrl+Shift+D` (or `Cmd+Shift+D` on Mac)
3. Or use the Command Palette: `VibeLang: Open Sound Designer`

### Interface Overview

The Sound Designer has four main areas:

```
┌─────────────────────────────────────────────────────────────┐
│ Toolbar: New | Load | Save | [synth_name] | Status | Generate │
├────────┬──────────────────────────────────┬─────────────────┤
│        │                                  │ Inspector       │
│ UGen   │         Node Canvas              │ Envelope Editor │
│ Palette│                                  │ LFO Editor      │
│        │                                  │ Piano Preview   │
├────────┴──────────────────────────────────┴─────────────────┤
│                    Generated Code Preview                    │
└─────────────────────────────────────────────────────────────┘
```

**Left Sidebar - UGen Palette**
- Categories of Unit Generators (oscillators, filters, etc.)
- Utility nodes: Add, Mul, Envelope, LFO, etc.
- Drag items onto the canvas to create nodes

**Center - Node Canvas**
- Visual representation of your synth's signal flow
- Drag nodes to position them
- Connect ports by dragging from outputs (green) to inputs (blue)
- Click on cables to delete connections

**Right Sidebar - Inspector & Editors**
- Inspector: Shows properties of selected node
- Envelope Editor: Appears when an Envelope node is selected
- LFO Editor: Appears when an LFO node is selected
- Piano: Preview your synth (requires runtime connection)

**Bottom - Code Preview**
- Real-time generated VibeLang code
- Copy button for quick export

---

## Tutorial: Building a Classic Subtractive Synth

Let's build a professional-sounding subtractive synthesizer step by step.

### Step 1: Understanding the Default Nodes

When you open Sound Designer, you'll see:
- **freq** (440): Frequency parameter - controls pitch
- **amp** (0.5): Amplitude parameter - controls volume
- **gate** (1): Gate parameter - triggers note on/off
- **Output**: Final signal destination

### Step 2: Add an Oscillator

1. In the UGen Palette, expand the **Oscillators** category
2. Drag **Saw** onto the canvas (position it between freq and Output)
3. Connect **freq** output → **Saw** freq input
4. Connect **Saw** output → **Output** signal input

**Result**: You now have a raw sawtooth oscillator. The code shows:
```vibe
let sig0 = saw_ar(freq);
sig0
```

### Step 3: Add an Envelope for Amplitude

1. Expand the **Utility** category
2. Drag **Envelope** onto the canvas
3. Click on the Envelope node to select it
4. The **Envelope Editor** appears in the right sidebar
5. Configure the envelope:
   - Type: **ADSR**
   - Attack: **10ms** (quick attack)
   - Decay: **200ms**
   - Sustain: **60%**
   - Release: **300ms**

### Step 4: Multiply Oscillator by Envelope

1. Drag **Mul** from Utility onto the canvas
2. Connect **Saw** output → **Mul** input `a`
3. Connect **Envelope** output → **Mul** input `b`
4. Connect **Mul** output → **Output** signal input
5. Delete the old direct connection from Saw to Output (click the cable)

**Result**: The oscillator is now shaped by the envelope.

### Step 5: Add a Filter

1. Expand the **Filters** category
2. Drag **RLPF** (Resonant Low Pass Filter) onto the canvas
3. Insert it between Mul and Output:
   - Connect **Mul** output → **RLPF** input
   - Connect **RLPF** output → **Output**
4. Set RLPF parameters:
   - freq: **2000** (cutoff frequency)
   - rq: **0.3** (resonance, lower = more resonant)

### Step 6: Add Filter Envelope (Filter Modulation)

1. Drag another **Envelope** onto the canvas
2. Select it and configure:
   - Type: **Perc** (percussive)
   - Attack: **5ms**
   - Release: **400ms**

3. We need to scale this envelope to modulate the filter cutoff:
   - Drag **Scale** from Utility
   - Connect the second Envelope → Scale input
   - Set Scale parameters:
     - mul: **4000** (modulation depth)
     - add: **200** (base cutoff)
   - Connect Scale output → RLPF freq input

### Step 7: Add an LFO for Vibrato

1. Drag **LFO** from Utility
2. Select it and configure:
   - Waveform: **∿** (sine)
   - Rate: **5 Hz**
   - Depth: **0.3** (30%)

3. We'll use this for subtle pitch modulation:
   - Drag **Scale** onto canvas
   - Connect LFO → Scale input
   - Set Scale: mul: **10**, add: **0**
   - Drag **Add** onto canvas
   - Connect **freq** → Add input `a`
   - Connect LFO Scale → Add input `b`
   - Connect Add output → Saw freq input

### Step 8: Apply Amplitude

1. Drag another **Mul** node
2. Connect the signal (after filter) → new Mul input `a`
3. Connect **amp** parameter → Mul input `b`
4. Route this to Output

### Final Signal Flow

```
freq ──┬──> Add ──> Saw ──> Mul ──> RLPF ──> Mul ──> Output
       │            (osc)    │      (filter)   │
       │                     │                 │
LFO ───┴─> Scale            Env1              amp
           (vibrato)        (amp env)

                            Env2 ──> Scale
                            (filter env)  ↓
                                         RLPF freq
```

### Step 9: Name and Generate

1. Click the synth name field in the toolbar
2. Type: `subtractive_synth`
3. Click **Generate** to open the code in a new file
4. Or click **Copy** to copy to clipboard

### Generated Code Preview

```vibe
// VibeLang Synthesizer
// Generated by Sound Designer

define_synthdef("subtractive_synth", |builder| {
    builder
        .param("freq", 440)
        .param("amp", 0.5)
        .param("gate", 1)
        .body(|freq, amp, gate| {

            let env_shape = env_adsr(0.01, 0.2, 0.6, 0.3);
            let env = NewEnvGenBuilder(env_shape, gate)
                .with_done_action(2.0)
                .build();

            let env1_shape = env_perc(0.005, 0.4);
            let env1 = NewEnvGenBuilder(env1_shape, gate)
                .with_done_action(2.0)
                .build();

            let lfo = sin_osc_kr(5) * 0.3;

            let sig0 = (lfo * 10 + 0);
            let sig1 = (freq + sig0);
            let sig2 = saw_ar(sig1);
            let sig3 = (sig2 * env);
            let sig4 = (env1 * 4000 + 200);
            let sig5 = rlpf_ar(sig3, sig4, 0.3);
            let sig6 = (sig5 * amp);

            sig6
        })
});
```

---

## Advanced Techniques

### Using Multiple Oscillators

Create a richer sound by combining oscillators:

1. Add a second oscillator (e.g., **Pulse**)
2. Detune it slightly using Scale on freq: mul=1.01, add=0
3. Use **Mix** to blend both oscillators

### Creating Pads with Long Envelopes

1. Set envelope Attack to 1-2 seconds
2. Use ASR envelope type
3. Set high Sustain (80-100%)
4. Add slow LFO to filter cutoff

### FM Synthesis

1. Create a modulator: SinOsc at a higher frequency
2. Use Scale to set modulation index
3. Add to carrier oscillator's frequency input

### Percussive Sounds

1. Use **Perc** envelope type
2. Short attack (1-5ms)
3. Short to medium release (50-300ms)
4. No sustain phase

---

## Connecting to VibeLang Runtime

### For Live Preview

1. Start VibeLang with the API enabled:
   ```bash
   vibelang run your_song.vibe --api
   ```

2. The Sound Designer will automatically detect the connection
   - Status indicator turns **green** when connected
   - Piano keys become playable

3. **Important**: Your synth must be defined in the running session
   - Generate the code
   - Add it to your .vibe file
   - The preview will trigger the voice

### Workflow Tips

1. **Design in Sound Designer** → Create your synth visually
2. **Generate Code** → Export to your .vibe file
3. **Run with --api** → Enable live preview
4. **Iterate** → Tweak parameters and hear changes in real-time

### Troubleshooting Preview

- **"Disconnected" status**: Ensure VibeLang is running with `--api` flag
- **No sound on preview**: Make sure your synth is defined in the running session
- **Connection drops**: VibeLang may have crashed - restart and re-run

---

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| Open Sound Designer | `Ctrl+Shift+D` |
| Delete selected node | `Delete` or `Backspace` |
| Delete connection | Click on cable |

---

## Node Reference

### Utility Nodes
- **Add**: Sum two signals
- **Mul**: Multiply two signals
- **Scale**: (input × mul) + add
- **Mix**: Crossfade between two signals
- **Const**: Constant value
- **Envelope**: ADSR/ASR/Perc envelope generator
- **LFO**: Low frequency oscillator (sine, saw, tri, square)

### Common Oscillators
- **SinOsc**: Pure sine wave
- **Saw**: Sawtooth wave (rich harmonics)
- **Pulse**: Pulse/square wave with width control
- **Tri**: Triangle wave

### Common Filters
- **LPF**: Low pass filter (simple)
- **RLPF**: Resonant low pass filter
- **HPF**: High pass filter
- **BPF**: Band pass filter

---

## Tips for Great Sounds

1. **Start simple**: Get a basic sound working before adding complexity
2. **Use envelopes**: They bring sounds to life
3. **Filter is key**: Subtractive synthesis relies on filtering
4. **Subtle modulation**: LFOs add movement without being distracting
5. **Watch your levels**: Use amp parameter to control final volume
6. **Save presets**: Save your work frequently using the Save button

---

Happy sound designing!
