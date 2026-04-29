import React, { useState, useMemo } from 'react';
import { UGenDocs, ugenCategories, ugenData } from '../generated/UGenDocs';
import { StdlibDocs, stdlibCategories, stdlibData, getSubcategories, stdlibAllItems } from '../generated/StdlibDocs';
import { highlightCode } from '../utils/syntaxHighlight';
import Header from './Header';
import './Documentation.css';

// API Primitives data - manually curated for best documentation
const apiPrimitives = {
  'Global': [
    {
      name: 'set_tempo',
      signature: 'set_tempo(bpm)',
      description: 'Set the tempo in beats per minute.',
      params: [{ name: 'bpm', type: 'f64 | i64', description: 'Beats per minute (e.g., 120)' }],
      example: 'set_tempo(128);'
    },
    {
      name: 'get_tempo',
      signature: 'get_tempo() -> f64',
      description: 'Get the current tempo in BPM.',
      params: [],
      example: 'let bpm = get_tempo();'
    },
    {
      name: 'set_time_signature',
      signature: 'set_time_signature(numerator, denominator)',
      description: 'Set the time signature (e.g., 4/4, 3/4, 6/8).',
      params: [
        { name: 'numerator', type: 'i64', description: 'Beats per bar' },
        { name: 'denominator', type: 'i64', description: 'Note value that gets one beat' }
      ],
      example: 'set_time_signature(4, 4);'
    },
    {
      name: 'get_current_beat',
      signature: 'get_current_beat() -> f64',
      description: 'Get the current beat position in the transport.',
      params: [],
      example: 'let beat = get_current_beat();'
    },
    {
      name: 'get_current_bar',
      signature: 'get_current_bar() -> i64',
      description: 'Get the current bar number (1-indexed).',
      params: [],
      example: 'let bar = get_current_bar();'
    }
  ],
  'Voice': [
    {
      name: 'voice',
      signature: 'voice(name) -> Voice',
      description: 'Create a new voice - the fundamental building block for making sound. A voice connects a sound source (synth, sample, or SFZ instrument) to the audio output.',
      params: [{ name: 'name', type: 'String', description: 'Unique identifier for this voice' }],
      example: `let kick = voice("kick")
    .synth("kick_808")
    .gain(db(-6));`
    },
    {
      name: '.synth / .on',
      signature: '.synth(name) / .on(source)',
      description: 'Set the sound source for this voice. Can be a synthdef name, SFZ instrument handle, or sample handle.',
      params: [{ name: 'source', type: 'String | SfzHandle | SampleHandle', description: 'The sound source' }],
      example: `// Using a synthdef
voice("bass").synth("acid_303_classic");

// Using an SFZ instrument
let piano = load_sfz("piano", "piano.sfz");
voice("keys").on(piano);`
    },
    {
      name: '.group',
      signature: '.group(path)',
      description: 'Assign this voice to a group for collective mixing and effects.',
      params: [{ name: 'path', type: 'String', description: 'Group path (e.g., "drums", "drums/kicks")' }],
      example: `voice("kick").synth("kick_808").group("drums");
voice("snare").synth("snare_909").group("drums");`
    },
    {
      name: '.poly',
      signature: '.poly(count)',
      description: 'Set the polyphony (number of simultaneous notes) for this voice.',
      params: [{ name: 'count', type: 'i64', description: 'Maximum simultaneous voices (default: 1)' }],
      example: 'voice("pad").synth("pad_warm").poly(8);'
    },
    {
      name: '.gain',
      signature: '.gain(value)',
      description: 'Set the gain (volume) for this voice. Use db() helper for decibels.',
      params: [{ name: 'value', type: 'f64', description: 'Linear amplitude (0.0 to 1.0+) or use db()' }],
      example: 'voice("lead").synth("lead_saw").gain(db(-12));'
    },
    {
      name: '.trigger',
      signature: '.trigger() / .trigger(params)',
      description: 'Trigger the voice immediately with optional parameters.',
      params: [{ name: 'params', type: 'Map', description: 'Optional parameter overrides' }],
      example: `kick.trigger();
kick.trigger(#{ freq: 55.0 });`
    },
    {
      name: '.note_on / .note_off',
      signature: '.note_on(note, velocity) / .note_off(note)',
      description: 'Send MIDI-style note on/off messages. Use for SFZ instruments or sustained sounds.',
      params: [
        { name: 'note', type: 'String | i64', description: 'MIDI note number or name (e.g., "C4", 60)' },
        { name: 'velocity', type: 'f64', description: 'Note velocity (0.0 to 1.0)' }
      ],
      example: `piano.note_on("C4", 0.8);
sleep(500);
piano.note_off("C4");`
    },
    {
      name: '.on (MIDI)',
      signature: '.on(midi_device)',
      description: 'Route this voice to an external MIDI device. Notes and parameter changes will be sent as MIDI messages.',
      params: [{ name: 'midi_device', type: 'MidiDevice', description: 'MIDI output device' }],
      example: `let midi_out = midi_device("synth").output("My Synth");
voice("hw_synth").on(midi_out).channel(0);`
    },
    {
      name: '.channel',
      signature: '.channel(ch)',
      description: 'Set the MIDI channel (0-15) for a MIDI output voice.',
      params: [{ name: 'ch', type: 'i64', description: 'MIDI channel (0-15)' }],
      example: 'voice("hw_synth").on(midi_out).channel(0);'
    },
    {
      name: '.cc_map',
      signature: '.cc_map(param, cc)',
      description: 'Map a voice parameter to a MIDI CC number. When the parameter changes, the value is sent as MIDI CC to the output device. Use with .on(midi_device) for hardware synth control.',
      params: [
        { name: 'param', type: 'String', description: 'Parameter name (e.g., "cutoff")' },
        { name: 'cc', type: 'i64', description: 'MIDI CC number (0-127)' }
      ],
      example: `// Hardware synth with CC mappings
let hw = voice("hw_synth")
    .on(midi_out)
    .channel(0)
    .cc_map("cutoff", 74)
    .cc_map("resonance", 71)
    .apply();`
    }
  ],
  'Pattern': [
    {
      name: 'pattern',
      signature: 'pattern(name) -> Pattern',
      description: 'Create a rhythmic pattern that triggers a voice at specified steps.',
      params: [{ name: 'name', type: 'String', description: 'Unique identifier for this pattern' }],
      example: `pattern("beat")
    .on(kick)
    .step("x...x...x...x...")
    .start();`
    },
    {
      name: '.step',
      signature: '.step(steps)',
      description: 'Define the pattern using a step string. "x" triggers, "." rests, "|" is a visual separator.',
      params: [{ name: 'steps', type: 'String', description: 'Step pattern (e.g., "x..x..x.")' }],
      example: `// 4-on-the-floor kick
pattern("kick").on(kick).step("x...x...x...x...");

// Offbeat hi-hat
pattern("hat").on(hihat).step("..x...x...x...x.");`
    },
    {
      name: '.euclid',
      signature: '.euclid(hits, steps)',
      description: 'Generate a Euclidean rhythm - evenly distributed hits across steps.',
      params: [
        { name: 'hits', type: 'i64', description: 'Number of hits' },
        { name: 'steps', type: 'i64', description: 'Total number of steps' }
      ],
      example: `// Classic 3-over-8 pattern
pattern("perc").on(clave).euclid(3, 8).start();`
    },
    {
      name: '.len',
      signature: '.len(beats)',
      description: 'Set the loop length in beats.',
      params: [{ name: 'beats', type: 'f64', description: 'Length in beats' }],
      example: 'pattern("fill").on(snare).step("..x.x.xx").len(2.0);'
    },
    {
      name: '.swing',
      signature: '.swing(amount)',
      description: 'Add swing feel by delaying off-beat notes.',
      params: [{ name: 'amount', type: 'f64', description: 'Swing amount (0.0 to 1.0, where 0.5 is triplet feel)' }],
      example: 'pattern("groove").on(hat).step("x.x.x.x.").swing(0.3);'
    },
    {
      name: '.start / .stop',
      signature: '.start() / .stop()',
      description: 'Start or stop the pattern playback.',
      params: [],
      example: `let p = pattern("beat").on(kick).step("x...").start();
// Later...
p.stop();`
    }
  ],
  'Melody': [
    {
      name: 'melody',
      signature: 'melody(name) -> Melody',
      description: 'Create a melodic sequence with pitched notes.',
      params: [{ name: 'name', type: 'String', description: 'Unique identifier for this melody' }],
      example: `melody("bassline")
    .on(bass)
    .notes("C2 - - - | G2 - - - | A2 - - - | F2 - - -")
    .start();`
    },
    {
      name: '.notes',
      signature: '.notes(pattern)',
      description: 'Define notes using a string pattern. Note names with octave, "-" for hold, "." for rest.',
      params: [{ name: 'pattern', type: 'String', description: 'Note pattern (e.g., "C4 - E4 G4")' }],
      example: `// Simple melody
melody("lead").on(synth)
    .notes("C4 D4 E4 F4 | G4 - - - | E4 - C4 -")
    .start();`
    },
    {
      name: '.scale / .root',
      signature: '.scale(name) / .root(note)',
      description: 'Set a scale and root note for scale-degree notation.',
      params: [
        { name: 'name', type: 'String', description: 'Scale name (e.g., "minor", "major", "dorian")' },
        { name: 'note', type: 'String', description: 'Root note (e.g., "C", "F#")' }
      ],
      example: `melody("riff")
    .on(lead)
    .scale("minor")
    .root("A")
    .notes("1 3 5 8 | 5 3 1 -")
    .start();`
    },
    {
      name: '.gate',
      signature: '.gate(duration)',
      description: 'Set the default note duration as a fraction of the step length.',
      params: [{ name: 'duration', type: 'f64', description: 'Gate time (0.0 to 1.0+, default: 0.9)' }],
      example: 'melody("staccato").on(lead).gate(0.3).notes("C4 E4 G4 C5");'
    },
    {
      name: '.transpose',
      signature: '.transpose(semitones)',
      description: 'Transpose all notes by a number of semitones.',
      params: [{ name: 'semitones', type: 'i64', description: 'Semitones to transpose (positive or negative)' }],
      example: 'melody("high").on(lead).notes("C4 E4 G4").transpose(12);'
    }
  ],
  'Group': [
    {
      name: 'group',
      signature: 'group(path) -> GroupHandle',
      description: 'Get a handle to a group for mixing and effects.',
      params: [{ name: 'path', type: 'String', description: 'Group path (e.g., "drums", "synths/bass")' }],
      example: `group("drums").gain(db(-3));
group("drums").add_effect("verb", "reverb", #{ mix: 0.2 });`
    },
    {
      name: 'define_group',
      signature: 'define_group(name, closure)',
      description: 'Define a group with a closure that sets up voices and effects.',
      params: [
        { name: 'name', type: 'String', description: 'Group name' },
        { name: 'closure', type: 'FnPtr', description: 'Setup function' }
      ],
      example: `define_group("drums", || {
    voice("kick").synth("kick_808");
    voice("snare").synth("snare_909");
    fx("comp").synth("compressor").param("ratio", 4.0).apply();
});`
    },
    {
      name: '.add_effect',
      signature: '.add_effect(id, synthdef, params)',
      description: 'Add an effect to the group\'s signal chain.',
      params: [
        { name: 'id', type: 'String', description: 'Unique effect ID within this group' },
        { name: 'synthdef', type: 'String', description: 'Effect synthdef name' },
        { name: 'params', type: 'Map', description: 'Effect parameters' }
      ],
      example: `group("synths")
    .add_effect("delay", "ping_pong_delay", #{ time: 0.375, feedback: 0.4 })
    .add_effect("reverb", "hall_reverb", #{ mix: 0.3 });`
    },
    {
      name: '.fade_gain_to',
      signature: '.fade_gain_to(target, duration)',
      description: 'Smoothly fade the group gain to a target value.',
      params: [
        { name: 'target', type: 'f64', description: 'Target gain value' },
        { name: 'duration', type: 'f64', description: 'Fade duration in seconds' }
      ],
      example: `// Fade out over 4 beats
group("drums").fade_gain_to(0.0, bars(1));`
    }
  ],
  'Sample': [
    {
      name: 'sample',
      signature: 'sample(id, path) -> SampleHandle',
      description: 'Load an audio sample from file. Returns a handle that can be used with voices for playback.',
      params: [
        { name: 'id', type: 'String', description: 'Unique identifier for this sample' },
        { name: 'path', type: 'String', description: 'Path to audio file (wav, flac, ogg, mp3, etc.)' }
      ],
      example: `let kick = sample("kick", "samples/kick.wav");
let loop = sample("break", "samples/amen_break.wav");

// Use with a voice
voice("drums").on(kick).gain(db(-3));
voice("drums").trigger();`
    },
    {
      name: '.bpm',
      signature: '.bpm() -> f64',
      description: 'Get the detected BPM of the sample. VibeLang automatically analyzes samples to detect their tempo.',
      params: [],
      example: `let loop = sample("break", "samples/amen_break.wav");
let detected_bpm = loop.bpm();
print("Loop BPM: " + detected_bpm);`
    },
    {
      name: '.warp',
      signature: '.warp(enabled) -> SampleHandle',
      description: 'Enable or disable time-stretching (warping) to match the current tempo. When enabled, the sample automatically stretches or compresses to sync with your project BPM.',
      params: [{ name: 'enabled', type: 'bool', description: 'Whether to enable warping (default: false)' }],
      example: `// Load a loop and sync it to project tempo
let loop = sample("break", "samples/amen_170bpm.wav")
    .warp(true);

// The loop will now play at the project tempo
voice("drums").on(loop);`
    },
    {
      name: '.slice',
      signature: '.slice(start, end) -> SampleHandle',
      description: 'Extract a portion of the sample by specifying start and end positions in seconds. Returns a new sample handle for just that slice.',
      params: [
        { name: 'start', type: 'f64', description: 'Start position in seconds' },
        { name: 'end', type: 'f64', description: 'End position in seconds' }
      ],
      example: `// Get the first second of a sample
let intro = sample("break", "samples/amen_break.wav")
    .slice(0.0, 1.0);

// Extract a specific section
let loop = sample("break", "samples/amen_break.wav");
let first_half = loop.slice(0.0, loop.duration / 2.0);
let second_half = loop.slice(loop.duration / 2.0, loop.duration);

// Use slices with voices
voice("drums").on(first_half).trigger();`
    },
    {
      name: '.loop',
      signature: '.loop(enabled) -> SampleHandle',
      description: 'Enable or disable looping for the sample. When enabled, the sample repeats continuously.',
      params: [{ name: 'enabled', type: 'bool', description: 'Whether to enable looping' }],
      example: `// Create a looping pad
let pad = sample("texture", "samples/ambient_texture.wav")
    .loop(true);

voice("ambience").on(pad).gain(db(-12));
voice("ambience").trigger();`
    },
    {
      name: '.reverse',
      signature: '.reverse() -> SampleHandle',
      description: 'Create a reversed version of the sample.',
      params: [],
      example: `let cymbal = sample("crash", "samples/crash.wav");
let rev_cymbal = cymbal.reverse();

// Use for reverse cymbal swells
voice("fx").on(rev_cymbal).trigger();`
    },
    {
      name: '.start / .end',
      signature: '.start(position) / .end(position) -> SampleHandle',
      description: 'Set the start or end position for playback. Positions are in seconds or as a ratio (0.0 to 1.0).',
      params: [{ name: 'position', type: 'f64', description: 'Position in seconds, or ratio if < 1.0' }],
      example: `// Play only a portion of the sample
let vocal = sample("vox", "samples/vocal.wav")
    .start(0.5)   // Start at 0.5 seconds
    .end(2.0);    // End at 2.0 seconds

// Or use ratios
let tail = sample("hit", "samples/snare.wav")
    .start(0.3);  // Start at 30% of the sample`
    }
  ],
  'Sequence': [
    {
      name: 'sequence',
      signature: 'sequence(name) -> Sequence',
      description: 'Create a timeline arrangement of patterns, melodies, and fades.',
      params: [{ name: 'name', type: 'String', description: 'Unique identifier for this sequence' }],
      example: `sequence("verse")
    .loop_bars(8)
    .clip(0.0..bars(4), kick_pattern)
    .clip(bars(4)..bars(8), snare_fill)
    .start();`
    },
    {
      name: '.loop_bars / .loop_beats',
      signature: '.loop_bars(bars) / .loop_beats(beats)',
      description: 'Set the loop length of the sequence.',
      params: [
        { name: 'bars', type: 'f64 | i64', description: 'Loop length in bars' },
        { name: 'beats', type: 'f64 | i64', description: 'Loop length in beats' }
      ],
      example: `sequence("intro")
    .loop_bars(4)
    .clip(0.0..bars(4), drums)
    .start();`
    },
    {
      name: '.clip',
      signature: '.clip(range, source)',
      description: 'Add a clip (pattern, melody, fade, or another sequence) at a time range. Range is in beats - use bars() helper to specify in bars.',
      params: [
        { name: 'range', type: 'Range<f64>', description: 'Start and end time in beats (use bars() for bars)' },
        { name: 'source', type: 'Pattern | Melody | Fade | Sequence', description: 'The clip source' }
      ],
      example: `// Using beats directly
sequence("song")
    .clip(0.0..16.0, intro_drums)   // beats 0-16
    .clip(16.0..48.0, verse_drums)  // beats 16-48
    .start();

// Using bars() helper (recommended)
sequence("song")
    .clip(0.0..bars(4), intro_drums)   // bars 0-4
    .clip(bars(4)..bars(12), verse)    // bars 4-12
    .start();`
    },
    {
      name: '.start / .stop / .launch',
      signature: '.start() / .stop() / .launch()',
      description: 'Control sequence playback. Launch starts if not already playing.',
      params: [],
      example: `let seq = sequence("main").loop_bars(8).clip(0.0..bars(8), drums);
seq.start();  // Start playing
// Later...
seq.stop();   // Stop playback`
    }
  ],
  'Fade': [
    {
      name: 'fade',
      signature: 'fade(name) -> Fade',
      description: 'Create an automation fade for smoothly changing parameters over time. Can target voices, groups, or effects.',
      params: [{ name: 'name', type: 'String', description: 'Unique identifier for this fade' }],
      example: `fade("filter_sweep")
    .on_group("synths")
    .param("cutoff")
    .from(200.0)
    .to(8000.0)
    .over_bars(4)
    .start();`
    },
    {
      name: '.on_group / .on_voice / .on_effect',
      signature: '.on_group(name) / .on_voice(name) / .on_effect(name)',
      description: 'Set the target for the fade - a group, voice, or effect.',
      params: [{ name: 'name', type: 'String', description: 'Name of the target' }],
      example: `// Fade a group
fade("drums_out").on_group("drums").param("gain").to(0.0).over_bars(2);

// Fade a voice
fade("bass_filter").on_voice("bass").param("cutoff").from(500.0).to(2000.0);

// Fade an effect
fade("reverb_in").on_effect("hall").param("mix").from(0.0).to(0.5);`
    },
    {
      name: '.param',
      signature: '.param(name)',
      description: 'Set the parameter to fade.',
      params: [{ name: 'name', type: 'String', description: 'Parameter name (e.g., "gain", "cutoff", "mix")' }],
      example: `fade("volume").on_group("master").param("gain").to(db(-6));`
    },
    {
      name: '.from / .to',
      signature: '.from(value) / .to(value)',
      description: 'Set the start and end values for the fade. If .from() is omitted, uses current value.',
      params: [{ name: 'value', type: 'f64', description: 'Parameter value' }],
      example: `// Explicit start and end
fade("sweep").param("cutoff").from(200.0).to(8000.0);

// Only set target (starts from current value)
fade("fadeout").param("gain").to(0.0);`
    },
    {
      name: '.over / .over_bars',
      signature: '.over(beats) / .over_bars(bars)',
      description: 'Set the duration of the fade.',
      params: [
        { name: 'beats', type: 'f64', description: 'Duration in beats' },
        { name: 'bars', type: 'i64', description: 'Duration in bars' }
      ],
      example: `// Fade over 2 bars
fade("intro").param("gain").from(0.0).to(1.0).over_bars(2);

// Fade over 8 beats
fade("quick").param("cutoff").to(1000.0).over(8.0);`
    },
    {
      name: '.start / .apply',
      signature: '.start() / .apply()',
      description: 'Start the fade immediately, or apply it for use in a sequence.',
      params: [],
      example: `// Start immediately
fade("now").on_group("drums").param("gain").to(0.0).over_bars(1).start();

// For use in a sequence
let my_fade = fade("later").on_group("synths").param("cutoff").to(8000.0).over_bars(4).apply();
sequence("song").clip(bars(2)..bars(6), my_fade).start();`
    }
  ],
  'Modulator': [
    {
      name: 'voice (kr source)',
      signature: 'voice(name).synth(kr_synthdef) -> Voice',
      description: 'A "modulator" is just a regular voice whose synthdef declares a control-rate output port via .output_kr("out"). Create the voice the same way as any other voice; the kr port is what makes it usable as a modulation source.',
      params: [
        { name: 'name', type: 'String', description: 'Unique identifier for this voice' },
        { name: 'kr_synthdef', type: 'String', description: 'Synthdef name with a kr output port (e.g. lfo_sine, lfo_tri, lfo_saw, lfo_square, lfo_random, envelope_follower)' }
      ],
      example: `// Create a filter sweep LFO as a voice on a kr-output synthdef.
let filter_lfo = voice("filter_lfo")
    .synth("lfo_tri")            // kr port "out"
    .set_param("rate", 0.25)     // 4-second cycle
    .set_param("lo", 100.0)      // Min cutoff
    .set_param("hi", 4000.0);    // Max cutoff`
    },
    {
      name: '.param',
      signature: '.param(name) -> ParamHandle',
      description: 'Open a target-first parameter handle on a voice. Chain .modulate_by(source, port) to wire a kr source voice into this param. Multiple modulate_by calls on the same param sum the sources via an internal kr summer.',
      params: [
        { name: 'name', type: 'String', description: 'Parameter name on the target voice' }
      ],
      example: `let bass = voice("bass").synth("moog_bass").gain(db(-6));
bass.param("cutoff").modulate_by(filter_lfo, "out");`
    },
    {
      name: '.modulate_by',
      signature: '.modulate_by(source, port) -> ParamHandle',
      description: 'Target-first BEND: route a kr port from a source voice into the target voice param. Chain multiple .modulate_by calls on the same param to sum sources (env + lfo → cutoff).',
      params: [
        { name: 'source', type: 'Voice', description: 'Source voice running a kr-output synthdef' },
        { name: 'port', type: 'String', description: 'Name of the kr output port on the source synthdef' }
      ],
      example: `// LFO and envelope summed into a single cutoff param.
let env = voice("env").synth("lfo_sine").set_param("rate", 0.3);
let lfo = voice("vib").synth("lfo_sine").set_param("rate", 5.0);

bass.param("cutoff")
    .modulate_by(env, "out")
    .modulate_by(lfo, "out");`
    },
    {
      name: '.output(...).to_param',
      signature: '.output(port).to_param(target, param) -> RouteHandle',
      description: 'Source-first SET: wire a source voice\'s kr output port into a target voice\'s param via scsynth /n_map. Equivalent to target.param(param).modulate_by(source, port) but written from the source side. Single-source per param; for multi-source fan-in use .modulate_by.',
      params: [
        { name: 'port', type: 'String', description: 'kr output port name on the source synthdef' },
        { name: 'target', type: 'Voice', description: 'Target voice to drive' },
        { name: 'param', type: 'String', description: 'Parameter on the target voice' }
      ],
      example: `let env = voice("env").synth("maths").group("ctrl");
let bass = voice("bass").synth("tb303_bass").group("bass");

env.output("ch1").to_param(bass, "cutoff");`
    },
    {
      name: 'define_synthdef + .output_kr',
      signature: 'define_synthdef(name).output_kr(port).body(...) ',
      description: 'Define a custom kr-rate ("modulator") synthdef. Declare a control-rate output port via .output_kr; the body returns a kr signal. The synthdef is then usable as a source voice for .modulate_by / .to_param.',
      params: [{ name: 'name', type: 'String', description: 'Unique synthdef name' }],
      example: `// Custom triangle LFO synthdef with a kr output port.
define_synthdef("my_lfo")
    .output_kr("out")
    .param("rate", 1.0)
    .param("lo", 0.0)
    .param("hi", 1.0)
    .body(|rate, lo, hi| {
        let osc = lf_tri_kr(rate);
        let normalized = osc * 0.5 + 0.5;
        normalized * (hi - lo) + lo
    });`
    }
  ],
  'SynthDef': [
    {
      name: 'define_synthdef',
      signature: 'define_synthdef(name) -> SynthDefBuilder',
      description: 'Define a new synthesizer using the DSP builder API.',
      params: [{ name: 'name', type: 'String', description: 'Unique synthdef name' }],
      example: `define_synthdef("my_bass")
    .param("freq", 110.0)
    .param("amp", 0.5)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        let osc = saw_ar(freq);
        let filt = lpf_ar(osc, 800.0);
        // Use the fluent envelope builder
        let env = envelope()
            .perc(0.01, 0.3)
            .gate(gate)
            .cleanup_on_finish()
            .build();
        filt * env * amp
    });`
    },
    {
      name: 'envelope',
      signature: 'envelope() -> EnvelopeBuilder',
      description: 'Create an envelope using the fluent builder API. Supports perc (percussive), asr (attack-sustain-release), and adsr shapes. Defaults to a constant gate if none specified.',
      params: [],
      example: `// Percussive envelope (drums, plucks, piano)
let env = envelope()
    .perc(0.01, 0.5)        // attack, release
    .cleanup_on_finish()
    .build();

// ASR envelope (pads, brass, sustained sounds)
let env = envelope()
    .asr(0.1, 0.8, 0.3)     // attack, sustain_level, release
    .gate(gate)
    .cleanup_on_finish()
    .build();

// Full ADSR envelope
let env = envelope()
    .adsr(0.01, 0.1, 0.7, 0.2)  // attack, decay, sustain, release
    .gate(gate)
    .cleanup_on_finish()
    .build();

// With time_scale for pitch-dependent decay
let decay_time = 2.0 + (1.0 / (freq / 100.0));
let env = envelope()
    .perc(0.001, 1.0)
    .gate(gate)
    .time_scale(decay_time)  // lower notes decay longer
    .cleanup_on_finish()
    .build();`
    },
    {
      name: 'define_fx',
      signature: 'define_fx(name) -> FxDefBuilder',
      description: 'Define an audio effect that processes input signal.',
      params: [{ name: 'name', type: 'String', description: 'Unique effect name' }],
      example: `define_fx("my_delay")
    .param("time", 0.25)
    .param("feedback", 0.5)
    .param("mix", 0.3)
    .body(|input, time, feedback, mix| {
        let delayed = comb_l_ar(input, 2.0, time, feedback * 4.0);
        (input * (1.0 - mix)) + (delayed * mix)
    });`
    }
  ],
  'Helpers': [
    {
      name: 'db',
      signature: 'db(decibels) -> f64',
      description: 'Convert decibels to linear amplitude.',
      params: [{ name: 'decibels', type: 'f64', description: 'Value in decibels (negative for quieter)' }],
      example: `voice("kick").gain(db(-6));  // Half amplitude
voice("hat").gain(db(-12)); // Quarter amplitude`
    },
    {
      name: 'note',
      signature: 'note(name) -> i64',
      description: 'Parse a note name to MIDI number.',
      params: [{ name: 'name', type: 'String', description: 'Note name (e.g., "C4", "F#3", "Bb5")' }],
      example: `let midi = note("C4");  // Returns 60
let freq = 440.0 * 2.0.pow((midi - 69) / 12.0);`
    },
    {
      name: 'bars',
      signature: 'bars(count) -> f64',
      description: 'Convert bars to beats based on current time signature.',
      params: [{ name: 'count', type: 'f64 | i64', description: 'Number of bars' }],
      example: `pattern("fill").on(snare).step("xxxx").len(bars(0.5));
fade("intro").over(bars(4));`
    },
    {
      name: 'load_sfz',
      signature: 'load_sfz(id, path) -> SfzHandle',
      description: 'Load an SFZ instrument from file.',
      params: [
        { name: 'id', type: 'String', description: 'Unique identifier for this instrument' },
        { name: 'path', type: 'String', description: 'Path to .sfz file' }
      ],
      example: `let piano = load_sfz("piano", "instruments/piano.sfz");
voice("keys").on(piano).poly(8);`
    },
    {
      name: 'sleep',
      signature: 'sleep(ms) / sleep_secs(secs)',
      description: 'Pause script execution for a duration.',
      params: [
        { name: 'ms', type: 'i64', description: 'Milliseconds to sleep' },
        { name: 'secs', type: 'f64', description: 'Seconds to sleep' }
      ],
      example: `kick.trigger();
sleep(500);  // Wait 500ms
snare.trigger();`
    }
  ],
  'Extensions': [
    {
      name: 'Filesystem (ext-fs)',
      signature: 'Feature: ext-fs',
      description: 'File system access extension. Provides functions for reading, writing, and manipulating files and directories. Must be enabled with --ext-fs flag.',
      params: [],
      example: `// Enable with: vibelang run --ext-fs script.vibe

// Read and write files
let content = read_file("notes.txt");
write_file("output.txt", "Hello!");

// Check files
if file_exists("config.json") {
    let size = file_size("config.json");
}

// Find files with glob patterns
let samples = glob("**/*.wav");`
    },
    {
      name: 'read_file',
      signature: 'read_file(path) -> String',
      description: 'Read entire file contents as a string.',
      params: [{ name: 'path', type: 'String', description: 'Path to the file to read' }],
      example: `let lyrics = read_file("song_lyrics.txt");
print(lyrics);`
    },
    {
      name: 'read_lines',
      signature: 'read_lines(path) -> Array',
      description: 'Read file contents as an array of lines.',
      params: [{ name: 'path', type: 'String', description: 'Path to the file to read' }],
      example: `let songs = read_lines("playlist.txt");
for song in songs {
    print("Track: " + song);
}`
    },
    {
      name: 'write_file',
      signature: 'write_file(path, content)',
      description: 'Write content to a file (creates or overwrites).',
      params: [
        { name: 'path', type: 'String', description: 'Path to the file' },
        { name: 'content', type: 'String', description: 'Content to write' }
      ],
      example: `write_file("session.json", json_stringify(session_data));`
    },
    {
      name: 'append_file',
      signature: 'append_file(path, content)',
      description: 'Append content to a file.',
      params: [
        { name: 'path', type: 'String', description: 'Path to the file' },
        { name: 'content', type: 'String', description: 'Content to append' }
      ],
      example: `append_file("log.txt", "Session started\\n");`
    },
    {
      name: 'file_exists',
      signature: 'file_exists(path) -> bool',
      description: 'Check if a file or directory exists.',
      params: [{ name: 'path', type: 'String', description: 'Path to check' }],
      example: `if file_exists("backup.wav") {
    print("Backup found!");
}`
    },
    {
      name: 'is_dir / is_file',
      signature: 'is_dir(path) -> bool / is_file(path) -> bool',
      description: 'Check if path is a directory or regular file.',
      params: [{ name: 'path', type: 'String', description: 'Path to check' }],
      example: `if is_dir("samples") {
    let files = list_dir("samples");
}`
    },
    {
      name: 'file_size',
      signature: 'file_size(path) -> i64',
      description: 'Get file size in bytes.',
      params: [{ name: 'path', type: 'String', description: 'Path to the file' }],
      example: `let size = file_size("audio.wav");
print("File is " + size + " bytes");`
    },
    {
      name: 'list_dir',
      signature: 'list_dir(path) -> Array',
      description: 'List contents of a directory.',
      params: [{ name: 'path', type: 'String', description: 'Directory path' }],
      example: `let files = list_dir("samples/kicks");
for f in files {
    print(f);
}`
    },
    {
      name: 'create_dir / create_dir_all',
      signature: 'create_dir(path) / create_dir_all(path)',
      description: 'Create a directory. create_dir_all creates parent directories too.',
      params: [{ name: 'path', type: 'String', description: 'Directory path to create' }],
      example: `create_dir_all("output/renders/2024");`
    },
    {
      name: 'copy_file',
      signature: 'copy_file(src, dst) -> i64',
      description: 'Copy a file. Returns bytes copied.',
      params: [
        { name: 'src', type: 'String', description: 'Source file path' },
        { name: 'dst', type: 'String', description: 'Destination file path' }
      ],
      example: `let bytes = copy_file("original.wav", "backup.wav");`
    },
    {
      name: 'rename_file',
      signature: 'rename_file(src, dst)',
      description: 'Rename or move a file.',
      params: [
        { name: 'src', type: 'String', description: 'Current path' },
        { name: 'dst', type: 'String', description: 'New path' }
      ],
      example: `rename_file("temp.wav", "final_mix.wav");`
    },
    {
      name: 'remove_file / remove_dir',
      signature: 'remove_file(path) / remove_dir(path)',
      description: 'Remove a file or empty directory.',
      params: [{ name: 'path', type: 'String', description: 'Path to remove' }],
      example: `remove_file("temp.txt");
remove_dir("empty_folder");`
    },
    {
      name: 'glob',
      signature: 'glob(pattern) -> Array',
      description: 'Find files matching a glob pattern. Supports * (any chars), ** (recursive), ? (single char).',
      params: [{ name: 'pattern', type: 'String', description: 'Glob pattern (e.g., "*.wav", "**/*.vibe")' }],
      example: `// Find all wav files
let samples = glob("*.wav");

// Find recursively
let all_vibes = glob("**/*.vibe");

// Match pattern
let tracks = glob("track_??.wav");`
    },
    {
      name: 'path_join',
      signature: 'path_join(base, path) -> String',
      description: 'Join path components.',
      params: [
        { name: 'base', type: 'String', description: 'Base path' },
        { name: 'path', type: 'String', description: 'Path to append' }
      ],
      example: `let full = path_join("samples", "kicks/kick_01.wav");`
    },
    {
      name: 'path_parent / path_filename',
      signature: 'path_parent(path) / path_filename(path) -> String',
      description: 'Get parent directory or filename from a path.',
      params: [{ name: 'path', type: 'String', description: 'Path to parse' }],
      example: `let dir = path_parent("/home/user/song.vibe");  // /home/user
let name = path_filename("/home/user/song.vibe"); // song.vibe`
    },
    {
      name: 'path_extension / path_stem',
      signature: 'path_extension(path) / path_stem(path) -> String',
      description: 'Get file extension or filename without extension.',
      params: [{ name: 'path', type: 'String', description: 'Path to parse' }],
      example: `let ext = path_extension("song.vibe");  // vibe
let stem = path_stem("song.vibe");      // song`
    },
    {
      name: 'Shell Exec (ext-exec)',
      signature: 'Feature: ext-exec',
      description: 'Shell command execution extension. Allows running external commands and accessing environment variables. Use with caution - only in trusted environments.',
      params: [],
      example: `// Enable with: vibelang run --ext-exec script.vibe

// Run commands
let files = exec("ls -la");
let status = exec_status("make build");

// Use shell features (pipes, etc.)
let count = shell("ls *.wav | wc -l");

// Environment variables
let home = env_var("HOME");`
    },
    {
      name: 'exec',
      signature: 'exec(command) -> String',
      description: 'Execute a command and return stdout as string.',
      params: [{ name: 'command', type: 'String', description: 'Command to execute (space-separated)' }],
      example: `let output = exec("ls -la");
print(output);`
    },
    {
      name: 'exec_status',
      signature: 'exec_status(command) -> i64',
      description: 'Execute a command and return exit status code.',
      params: [{ name: 'command', type: 'String', description: 'Command to execute' }],
      example: `let status = exec_status("which ffmpeg");
if status == 0 {
    print("ffmpeg is installed");
}`
    },
    {
      name: 'exec_lines',
      signature: 'exec_lines(command) -> Array',
      description: 'Execute a command and return stdout as array of lines.',
      params: [{ name: 'command', type: 'String', description: 'Command to execute' }],
      example: `let files = exec_lines("ls");
for f in files {
    print(f);
}`
    },
    {
      name: 'exec_with_args',
      signature: 'exec_with_args(program, args) -> String',
      description: 'Execute program with explicit arguments array (safer for special characters).',
      params: [
        { name: 'program', type: 'String', description: 'Program to run' },
        { name: 'args', type: 'Array', description: 'Array of arguments' }
      ],
      example: `// Safe with spaces and special chars
let result = exec_with_args("echo", ["Hello", "World!"]);
exec_with_args("touch", ["my file.txt"]);`
    },
    {
      name: 'exec_full',
      signature: 'exec_full(command) -> Map',
      description: 'Execute command and return full result with stdout, stderr, status, and success.',
      params: [{ name: 'command', type: 'String', description: 'Command to execute' }],
      example: `let result = exec_full("make build");
print("stdout: " + result.stdout);
print("stderr: " + result.stderr);
print("status: " + result.status);
print("success: " + result.success);`
    },
    {
      name: 'shell',
      signature: 'shell(script) -> String',
      description: 'Execute script through system shell. Supports pipes, redirects, and shell features.',
      params: [{ name: 'script', type: 'String', description: 'Shell script to execute' }],
      example: `// Use pipes
let count = shell("ls *.wav | wc -l");

// Chain commands
shell("echo hello && echo world");`
    },
    {
      name: 'env_var / env_var_or',
      signature: 'env_var(name) / env_var_or(name, default) -> String',
      description: 'Get environment variable value, optionally with a default.',
      params: [
        { name: 'name', type: 'String', description: 'Variable name' },
        { name: 'default', type: 'String', description: 'Default value if not set' }
      ],
      example: `let home = env_var("HOME");
let editor = env_var_or("EDITOR", "nano");`
    },
    {
      name: 'set_env_var',
      signature: 'set_env_var(name, value)',
      description: 'Set an environment variable.',
      params: [
        { name: 'name', type: 'String', description: 'Variable name' },
        { name: 'value', type: 'String', description: 'Value to set' }
      ],
      example: `set_env_var("MY_CONFIG", "production");`
    },
    {
      name: 'env_vars',
      signature: 'env_vars() -> Map',
      description: 'Get all environment variables as a map.',
      params: [],
      example: `let vars = env_vars();
print("PATH: " + vars["PATH"]);`
    },
    {
      name: 'cwd / set_cwd',
      signature: 'cwd() -> String / set_cwd(path)',
      description: 'Get or set current working directory.',
      params: [{ name: 'path', type: 'String', description: 'Directory to change to' }],
      example: `let current = cwd();
set_cwd("/tmp");
// ... do work ...
set_cwd(current);  // restore`
    },
    {
      name: 'pid',
      signature: 'pid() -> i64',
      description: 'Get current process ID.',
      params: [],
      example: `let process_id = pid();
print("Running as PID: " + process_id);`
    },
    {
      name: 'Networking (ext-net)',
      signature: 'Feature: ext-net',
      description: 'HTTP client and networking extension. Provides HTTP requests, URL utilities, and JSON parsing. Basic version supports HTTP only.',
      params: [],
      example: `// Enable with: vibelang run --ext-net script.vibe

// HTTP requests
let data = http_get_json("http://api.example.com/data");

// POST JSON data
http_post_json("http://api.example.com/save", #{
    tempo: 120,
    key: "C minor"
});

// URL and JSON utilities
let encoded = url_encode("hello world");
let parsed = json_parse(json_string);`
    },
    {
      name: 'http_get',
      signature: 'http_get(url) -> String',
      description: 'Perform HTTP GET request and return body as string.',
      params: [{ name: 'url', type: 'String', description: 'URL to fetch (http:// only in basic version)' }],
      example: `let html = http_get("http://example.com");`
    },
    {
      name: 'http_get_lines',
      signature: 'http_get_lines(url) -> Array',
      description: 'Perform HTTP GET and return body as array of lines.',
      params: [{ name: 'url', type: 'String', description: 'URL to fetch' }],
      example: `let lines = http_get_lines("http://example.com/data.txt");
for line in lines { print(line); }`
    },
    {
      name: 'http_get_json',
      signature: 'http_get_json(url) -> Dynamic',
      description: 'Perform HTTP GET and parse JSON response.',
      params: [{ name: 'url', type: 'String', description: 'URL to fetch' }],
      example: `let data = http_get_json("http://api.example.com/config");
print("Setting: " + data.setting);`
    },
    {
      name: 'http_post',
      signature: 'http_post(url, body) -> String',
      description: 'Perform HTTP POST with form data.',
      params: [
        { name: 'url', type: 'String', description: 'URL to post to' },
        { name: 'body', type: 'String', description: 'Form data body' }
      ],
      example: `let result = http_post("http://api.example.com/submit", "name=test&value=123");`
    },
    {
      name: 'http_post_json',
      signature: 'http_post_json(url, data) -> Dynamic',
      description: 'POST JSON data and parse JSON response.',
      params: [
        { name: 'url', type: 'String', description: 'URL to post to' },
        { name: 'data', type: 'Map', description: 'Data to send as JSON' }
      ],
      example: `let response = http_post_json("http://api.example.com/save", #{
    title: "My Song",
    bpm: 120
});`
    },
    {
      name: 'url_encode / url_decode',
      signature: 'url_encode(s) / url_decode(s) -> String',
      description: 'URL encode or decode a string.',
      params: [{ name: 's', type: 'String', description: 'String to encode/decode' }],
      example: `let encoded = url_encode("hello world");  // hello+world
let decoded = url_decode("hello+world");  // hello world`
    },
    {
      name: 'parse_url',
      signature: 'parse_url(url) -> Map',
      description: 'Parse URL into components (scheme, host, port, path, query).',
      params: [{ name: 'url', type: 'String', description: 'URL to parse' }],
      example: `let parts = parse_url("http://example.com:8080/path?q=1");
print(parts.host);   // example.com
print(parts.port);   // 8080
print(parts.path);   // /path
print(parts.query);  // q=1`
    },
    {
      name: 'build_query_string',
      signature: 'build_query_string(params) -> String',
      description: 'Build URL query string from a map.',
      params: [{ name: 'params', type: 'Map', description: 'Map of parameters' }],
      example: `let query = build_query_string(#{
    search: "drum samples",
    format: "wav"
});
// search=drum+samples&format=wav`
    },
    {
      name: 'json_parse',
      signature: 'json_parse(json) -> Dynamic',
      description: 'Parse JSON string into Rhai value.',
      params: [{ name: 'json', type: 'String', description: 'JSON string to parse' }],
      example: `let data = json_parse(\`{"name": "test", "values": [1, 2, 3]}\`);
print(data.name);        // test
print(data.values[0]);   // 1`
    },
    {
      name: 'json_stringify',
      signature: 'json_stringify(value) -> String',
      description: 'Convert Rhai value to JSON string.',
      params: [{ name: 'value', type: 'Dynamic', description: 'Value to convert' }],
      example: `let json = json_stringify(#{
    tempo: 128,
    tracks: ["drums", "bass"]
});
write_file("config.json", json);`
    }
  ]
};

const apiCategories = Object.keys(apiPrimitives);

// Category descriptions - top-level explanations for each API topic
const categoryDescriptions = {
  'Global': `Global functions control the fundamental properties of your VibeLang session. Set the tempo and time signature to establish the rhythmic foundation, and query the current transport position for dynamic behaviors. These settings affect all patterns, melodies, and sequences.`,

  'Voice': `Voices are the fundamental building blocks for making sound in VibeLang. A voice connects a sound source (synthesizer, sample, or SFZ instrument) to the audio output. Voices can be assigned to groups for collective mixing, set to polyphonic mode for chords, and triggered either directly or from patterns and melodies. The fluent builder API makes it easy to configure voices in a readable, chainable style.`,

  'Pattern': `Patterns are rhythmic sequencers that trigger voices at specified steps. They're perfect for drums, percussion, and any repetitive rhythmic elements. Define patterns using intuitive step strings where "x" triggers and "." rests, or generate Euclidean rhythms automatically. Patterns loop continuously and can be started, stopped, and modified on the fly.`,

  'Melody': `Melodies are pitched sequencers for playing musical phrases. Unlike patterns which just trigger, melodies specify notes with pitch and duration. Use note names like "C4" and "F#3", or work with scale degrees for easier transposition. Melodies support holds ("-") and rests (".") for expressive timing, and can be constrained to scales for harmonically correct output.`,

  'Group': `Groups are mixing channels that organize voices for collective processing. All voices assigned to a group share the same effects chain and can have their parameters automated together. Groups support hierarchical paths (e.g., "drums/kicks") for complex routing. Use groups to apply reverb to all synths, compress your drums, or fade entire sections in and out.`,

  'Sample': `Samples are audio files that can be loaded, manipulated, and played back. VibeLang provides powerful sample processing including automatic BPM detection, time-stretching (warping) to match your project tempo, slicing for beat chopping and resequencing, and flexible playback control. Load any audio format (wav, flac, ogg, mp3) and use samples with voices just like synths.`,

  'Sequence': `Sequences are timeline arrangers for structuring your music. They combine patterns, melodies, fades, and even other sequences into time-based arrangements. Use clips to place elements at specific bar positions, creating verse/chorus structures or complex arrangements. Sequences can loop or play once, making them perfect for both live performance and composition.`,

  'Fade': `Fades provide smooth parameter automation over time. Use them to create filter sweeps, volume swells, effect wet/dry transitions, or any gradual change. Fades can target voices, groups, or effects, and can be started immediately or placed in sequences for precise timing. The builder API lets you specify start value, end value, and duration in beats or bars.`,

  'Modulator': `Modulators provide real-time continuous parameter control using LFOs, envelope followers, and other control-rate signals. A modulator is just a regular voice whose synthdef declares a kr output port (.output_kr("out")); wire it into a target voice's parameter using .param(name).modulate_by(source, port) (target-first) or source.output(port).to_param(target, name) (source-first). Multiple sources can be summed into a single param via chained .modulate_by calls. The standard library includes sine, triangle, saw, square, and random LFOs, plus an envelope follower for dynamics-based modulation.`,

  'SynthDef': `SynthDefs let you create custom synthesizers and effects at the DSP level. Using VibeLang's UGen library (oscillators, filters, envelopes, and more), you can build anything from simple sine waves to complex FM synthesizers. Effects process input signals and can be added to groups. The standard library includes many ready-to-use synthdefs, but defining your own unlocks unlimited sonic possibilities.`,

  'Helpers': `Helper functions provide convenient utilities for common tasks. Convert decibels to linear amplitude, parse note names to MIDI numbers, calculate bar durations, load SFZ instruments, and control script timing. These functions make your code more readable and handle the math and conversions that would otherwise clutter your musical logic.`,

  'Extensions': `VibeLang provides optional extensions that add powerful capabilities beyond music creation. Extensions must be explicitly enabled with feature flags for security reasons. The **Filesystem extension (ext-fs)** provides file I/O operations, directory manipulation, and glob pattern matching. The **Shell Execution extension (ext-exec)** allows running external commands, accessing environment variables, and interacting with the system. The **Networking extension (ext-net)** provides HTTP client functionality, URL utilities, and JSON parsing for web API integration. Enable extensions with the --ext-fs, --ext-exec, or --ext-net flags when running scripts.`
};

function Documentation({ theme, onToggleTheme }) {
  const [activeTab, setActiveTab] = useState('api');
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedCategory, setSelectedCategory] = useState('all');
  const [expandedCategories, setExpandedCategories] = useState({});
  const [expandedSubcategories, setExpandedSubcategories] = useState({});
  const [selectedItem, setSelectedItem] = useState(null);
  const [expandedItem, setExpandedItem] = useState(null);

  // Get categories based on active tab
  const categories = useMemo(() => {
    switch (activeTab) {
      case 'api': return apiCategories;
      case 'stdlib': return stdlibCategories;
      case 'ugens': return ugenCategories;
      default: return [];
    }
  }, [activeTab]);

  // Toggle category expansion
  const toggleCategoryExpanded = (cat) => {
    setExpandedCategories(prev => ({
      ...prev,
      [cat]: !prev[cat]
    }));
  };

  // Toggle subcategory expansion
  const toggleSubcategoryExpanded = (key) => {
    setExpandedSubcategories(prev => ({
      ...prev,
      [key]: !prev[key]
    }));
  };

  // Get items for a category/subcategory based on active tab
  const getItemsForCategory = (cat, sub = null) => {
    if (activeTab === 'stdlib') {
      const catData = stdlibData[cat];
      if (!catData) return [];
      if (sub) {
        return catData.subcategories[sub] || [];
      }
      return catData.items || [];
    } else if (activeTab === 'api') {
      // API has no subcategories, return primitives directly
      return apiPrimitives[cat] || [];
    } else if (activeTab === 'ugens') {
      // UGens have no subcategories, return ugens directly
      return ugenData[cat] || [];
    }
    return [];
  };

  // Get subcategories for a category (only stdlib has subcategories)
  const getSubcategoriesForCategory = (cat) => {
    if (activeTab === 'stdlib') {
      return getSubcategories(cat);
    }
    return [];
  };

  // Check if an item matches the search term
  const itemMatchesSearch = (item, term) => {
    if (!term) return true;
    const t = term.toLowerCase();
    return item.name.toLowerCase().includes(t) ||
           (item.description && item.description.toLowerCase().includes(t)) ||
           (item.signature && item.signature.toLowerCase().includes(t)) ||
           (item.genre && item.genre.toLowerCase().includes(t));
  };

  // Calculate which categories have matches (memoized)
  const searchMatches = useMemo(() => {
    if (!searchTerm) return null;

    const term = searchTerm.toLowerCase();
    const matches = {
      categories: {},
      subcategories: {},
      items: new Set()
    };

    for (const cat of categories) {
      let catMatchCount = 0;
      const subcats = getSubcategoriesForCategory(cat);

      // Check direct items
      const directItems = getItemsForCategory(cat, null);
      for (const item of directItems) {
        if (itemMatchesSearch(item, term)) {
          matches.items.add(item.name);
          catMatchCount++;
        }
      }

      // Check subcategory items
      for (const sub of subcats) {
        let subMatchCount = 0;
        const items = getItemsForCategory(cat, sub);
        for (const item of items) {
          if (itemMatchesSearch(item, term)) {
            matches.items.add(item.name);
            subMatchCount++;
            catMatchCount++;
          }
        }
        matches.subcategories[`${cat}/${sub}`] = subMatchCount;
      }

      matches.categories[cat] = catMatchCount;
    }

    return matches;
  }, [searchTerm, activeTab, categories]);

  // Handle item selection - expand it and scroll within content area only
  const handleItemSelect = (itemName) => {
    setSelectedItem(itemName);
    setExpandedItem(itemName);
    // Scroll to the item card after a short delay
    setTimeout(() => {
      const element = document.querySelector(`[data-item-name="${itemName}"]`);
      const contentArea = document.querySelector('.docs-content');
      if (element && contentArea) {
        // Calculate scroll position within the content area, accounting for some padding
        const elementRect = element.getBoundingClientRect();
        const contentRect = contentArea.getBoundingClientRect();
        const targetScrollTop = contentArea.scrollTop + (elementRect.top - contentRect.top) - 20; // 20px padding from top
        contentArea.scrollTo({ top: Math.max(0, targetScrollTop), behavior: 'smooth' });
        element.classList.add('docs-card--highlighted');
        setTimeout(() => element.classList.remove('docs-card--highlighted'), 2000);
      }
    }, 100);
  };

  // Handle card toggle - used by child components
  const handleCardToggle = (itemName) => {
    setExpandedItem(prev => prev === itemName ? null : itemName);
  };

  return (
    <div className="documentation">
      <Header theme={theme} onToggleTheme={onToggleTheme} page="docs" />

      <div className="docs-layout">
        <aside className="docs-sidebar">
          <div className="docs-sidebar__tabs">
            <button
              className={activeTab === 'api' ? 'active' : ''}
              onClick={() => { setActiveTab('api'); setSelectedCategory('all'); setExpandedItem(null); setExpandedCategories({}); }}
            >
              API
            </button>
            <button
              className={activeTab === 'stdlib' ? 'active' : ''}
              onClick={() => { setActiveTab('stdlib'); setSelectedCategory('all'); setExpandedItem(null); setExpandedCategories({}); }}
            >
              Stdlib
            </button>
            <button
              className={activeTab === 'ugens' ? 'active' : ''}
              onClick={() => { setActiveTab('ugens'); setSelectedCategory('all'); setExpandedItem(null); setExpandedCategories({}); }}
            >
              UGens
            </button>
          </div>

          <div className="docs-sidebar__search">
            <input
              type="text"
              placeholder="Search docs..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
            />
          </div>

          <div className="docs-sidebar__categories">
            <button
              className={selectedCategory === 'all' ? 'active' : ''}
              onClick={() => setSelectedCategory('all')}
            >
              All
              {searchMatches && (
                <span className="docs-sidebar__item-count">{searchMatches.items.size}</span>
              )}
            </button>
            {/* Show no results message when search has no matches */}
            {searchMatches && searchMatches.items.size === 0 && (
              <div className="docs-sidebar__no-results">
                No matches for "{searchTerm}"
              </div>
            )}
            {categories.map(cat => {
              const subcats = getSubcategoriesForCategory(cat);
              const directItems = getItemsForCategory(cat, null);
              const hasSubcats = subcats.length > 0;
              const hasDirectItems = directItems.length > 0;
              const hasChildren = hasSubcats || hasDirectItems;

              // When searching, auto-expand categories with matches
              const hasMatches = searchMatches ? searchMatches.categories[cat] > 0 : true;
              const isExpanded = searchMatches ? hasMatches : expandedCategories[cat];

              const itemCount = hasSubcats
                ? subcats.reduce((acc, sub) => acc + getItemsForCategory(cat, sub).length, 0) + directItems.length
                : directItems.length;
              const matchCount = searchMatches ? searchMatches.categories[cat] : itemCount;

              // Hide categories with no matches when searching
              if (searchMatches && !hasMatches) {
                return null;
              }

              return (
                <div key={cat} className="docs-sidebar__category-group">
                  <button
                    className={`docs-sidebar__category ${selectedCategory === cat ? 'active' : ''} ${hasChildren ? 'has-children' : ''}`}
                    onClick={() => {
                      if (hasChildren && !searchMatches) {
                        toggleCategoryExpanded(cat);
                      }
                      setSelectedCategory(cat);
                    }}
                  >
                    {hasChildren && (
                      <span className="docs-sidebar__expand-icon">
                        {isExpanded ? '▾' : '▸'}
                      </span>
                    )}
                    {cat}
                    <span className="docs-sidebar__item-count">
                      {searchMatches ? matchCount : itemCount}
                    </span>
                  </button>
                  {hasChildren && isExpanded && (
                    <div className="docs-sidebar__subcategories">
                      {/* Direct items at category level (for API/UGens, or stdlib items without subcategory) */}
                      {hasDirectItems && !hasSubcats && (
                        <div className="docs-sidebar__items docs-sidebar__items--direct">
                          {directItems
                            .filter(item => !searchMatches || searchMatches.items.has(item.name))
                            .map(item => (
                            <button
                              key={item.name}
                              className={`docs-sidebar__item ${selectedItem === item.name ? 'active' : ''} ${searchMatches && searchMatches.items.has(item.name) ? 'match' : ''}`}
                              onClick={() => {
                                setSelectedCategory(cat);
                                handleItemSelect(item.name);
                              }}
                            >
                              {item.name}
                            </button>
                          ))}
                        </div>
                      )}
                      {/* For stdlib: show direct items first, then subcategories */}
                      {hasDirectItems && hasSubcats && (
                        <div className="docs-sidebar__items docs-sidebar__items--direct">
                          {directItems
                            .filter(item => !searchMatches || searchMatches.items.has(item.name))
                            .map(item => (
                            <button
                              key={item.name}
                              className={`docs-sidebar__item ${selectedItem === item.name ? 'active' : ''} ${searchMatches && searchMatches.items.has(item.name) ? 'match' : ''}`}
                              onClick={() => {
                                setSelectedCategory(cat);
                                handleItemSelect(item.name);
                              }}
                            >
                              {item.name}
                            </button>
                          ))}
                        </div>
                      )}
                      {/* Subcategories (stdlib only) */}
                      {subcats.map(sub => {
                        const subKey = `${cat}/${sub}`;
                        const items = getItemsForCategory(cat, sub);
                        const hasItems = items.length > 0;

                        // When searching, check subcategory matches
                        const subMatchCount = searchMatches ? searchMatches.subcategories[subKey] || 0 : items.length;
                        const hasSubMatches = searchMatches ? subMatchCount > 0 : true;

                        // Hide subcategories with no matches when searching
                        if (searchMatches && !hasSubMatches) {
                          return null;
                        }

                        // Auto-expand subcategories with matches when searching
                        const isSubExpanded = searchMatches ? hasSubMatches : expandedSubcategories[subKey];

                        return (
                          <div key={sub} className="docs-sidebar__subcategory-group">
                            <button
                              className={`docs-sidebar__subcategory ${selectedCategory === subKey ? 'active' : ''} ${hasItems ? 'has-children' : ''}`}
                              onClick={() => {
                                if (hasItems && !searchMatches) {
                                  toggleSubcategoryExpanded(subKey);
                                }
                                setSelectedCategory(subKey);
                              }}
                            >
                              {hasItems && (
                                <span className="docs-sidebar__expand-icon">
                                  {isSubExpanded ? '▾' : '▸'}
                                </span>
                              )}
                              {sub}
                              <span className="docs-sidebar__item-count">
                                {searchMatches ? subMatchCount : items.length}
                              </span>
                            </button>
                            {hasItems && isSubExpanded && (
                              <div className="docs-sidebar__items">
                                {items
                                  .filter(item => !searchMatches || searchMatches.items.has(item.name))
                                  .map(item => (
                                  <button
                                    key={item.name}
                                    className={`docs-sidebar__item ${selectedItem === item.name ? 'active' : ''} ${searchMatches && searchMatches.items.has(item.name) ? 'match' : ''}`}
                                    onClick={() => {
                                      setSelectedCategory(subKey);
                                      handleItemSelect(item.name);
                                    }}
                                  >
                                    {item.name}
                                  </button>
                                ))}
                              </div>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </aside>

        <main className="docs-content">
          {activeTab === 'api' && (
            <APIDocs
              searchTerm={searchTerm}
              selectedCategory={selectedCategory}
              expandedItem={expandedItem}
              onCardToggle={handleCardToggle}
            />
          )}
          {activeTab === 'stdlib' && (
            <StdlibDocs
              searchTerm={searchTerm}
              selectedCategory={selectedCategory}
              expandedItem={expandedItem}
              onCardToggle={handleCardToggle}
            />
          )}
          {activeTab === 'ugens' && (
            <UGenDocs
              searchTerm={searchTerm}
              selectedCategory={selectedCategory}
              expandedItem={expandedItem}
              onCardToggle={handleCardToggle}
            />
          )}
        </main>
      </div>
    </div>
  );
}

function APIDocs({ searchTerm, selectedCategory, expandedItem, onCardToggle }) {
  // When searching, always search all categories
  const filteredCategories = (searchTerm || selectedCategory === 'all')
    ? apiCategories
    : apiCategories.filter(c => c === selectedCategory);

  const isSingleCategory = selectedCategory !== 'all' && !searchTerm;

  return (
    <div className="api-docs">
      {/* Show general intro when viewing all categories */}
      {!isSingleCategory && (
        <div className="docs-intro">
          <h1>VibeLang API Reference</h1>
          <p>
            VibeLang uses a fluent builder API for creating sounds, patterns, and arrangements.
            Everything chains together naturally. Select a category from the sidebar to dive deeper.
          </p>
        </div>
      )}

      {/* Show category-specific intro when a single category is selected */}
      {isSingleCategory && categoryDescriptions[selectedCategory] && (
        <div className="docs-intro docs-intro--category">
          <h1>{selectedCategory}</h1>
          <p>{categoryDescriptions[selectedCategory]}</p>
        </div>
      )}

      {filteredCategories.map(category => {
        const primitives = apiPrimitives[category].filter(p => {
          if (!searchTerm) return true;
          const term = searchTerm.toLowerCase();
          return p.name.toLowerCase().includes(term) ||
                 p.description.toLowerCase().includes(term);
        });

        if (primitives.length === 0) return null;

        return (
          <div key={category} className="api-category">
            {/* Only show category title when viewing all categories */}
            {!isSingleCategory && (
              <h2 className="api-category__title">{category}</h2>
            )}
            <div className="api-category__list">
              {primitives.map(primitive => (
                <APICard
                  key={primitive.name}
                  primitive={primitive}
                  isExpanded={expandedItem === primitive.name}
                  onToggle={() => onCardToggle(primitive.name)}
                />
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function APICard({ primitive, isExpanded, onToggle }) {
  return (
    <div className={`api-card ${isExpanded ? 'api-card--expanded' : ''}`} data-item-name={primitive.name}>
      <div className="api-card__header" onClick={onToggle}>
        <div className="api-card__title-row">
          <code className="api-card__signature">{primitive.signature}</code>
        </div>
        <p className="api-card__description">{primitive.description}</p>
        <span className="api-card__expand-icon">{isExpanded ? '−' : '+'}</span>
      </div>

      {isExpanded && (
        <div className="api-card__details">
          {primitive.params && primitive.params.length > 0 && (
            <div className="api-card__params-section">
              <h4>Parameters</h4>
              <table className="api-card__params">
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Type</th>
                    <th>Description</th>
                  </tr>
                </thead>
                <tbody>
                  {primitive.params.map(param => (
                    <tr key={param.name}>
                      <td className="api-card__param-name">{param.name}</td>
                      <td className="api-card__param-type">{param.type}</td>
                      <td className="api-card__param-desc">{param.description}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {primitive.example && (
            <div className="api-card__example">
              <h4>Example</h4>
              <pre><code>{highlightCode(primitive.example)}</code></pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default Documentation;
