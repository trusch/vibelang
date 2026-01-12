// Autocomplete data processed from rhai-api.json, StdlibDocs, and UGenDocs
// This file provides all completions for the playground editor

import { stdlibData, stdlibCategories } from '../generated/StdlibDocs';
import { ugenData } from '../generated/UGenDocs';

// Top-level functions (not methods)
export const functions = [
  // Global/Transport
  { name: 'set_tempo', signature: 'set_tempo(bpm: float)', description: 'Set the global tempo in BPM', category: 'global', insertText: 'set_tempo(${1:120})' },
  { name: 'get_tempo', signature: 'get_tempo() -> float', description: 'Get the current tempo', category: 'global', insertText: 'get_tempo()' },
  { name: 'set_time_signature', signature: 'set_time_signature(num: int, denom: int)', description: 'Set the time signature', category: 'global', insertText: 'set_time_signature(${1:4}, ${2:4})' },
  { name: 'set_quantization', signature: 'set_quantization(grid: string)', description: 'Set quantization grid', category: 'global', insertText: 'set_quantization("${1:bar}")' },
  { name: 'get_current_beat', signature: 'get_current_beat() -> float', description: 'Get current beat position', category: 'global', insertText: 'get_current_beat()' },
  { name: 'get_current_bar', signature: 'get_current_bar() -> int', description: 'Get current bar number', category: 'global', insertText: 'get_current_bar()' },
  { name: 'jump_to_start', signature: 'jump_to_start()', description: 'Jump transport to beat 0', category: 'global', insertText: 'jump_to_start()' },
  { name: 'nudge_transport', signature: 'nudge_transport(beats: float)', description: 'Move transport by beats', category: 'global', insertText: 'nudge_transport(${1:4.0})' },

  // Definition
  { name: 'define_synthdef', signature: 'define_synthdef(name: string)', description: 'Define a new synthesizer', category: 'definition', insertText: 'define_synthdef("${1:name}")\n    .param("freq", 440.0)\n    .param("amp", 0.5)\n    .body(|freq, amp| {\n        ${2:// DSP code}\n    })' },
  { name: 'define_fx', signature: 'define_fx(name: string)', description: 'Define an effect processor', category: 'definition', insertText: 'define_fx("${1:name}")\n    .param("mix", 0.5)\n    .body(|input, mix| {\n        ${2:// Effect code}\n    })' },
  { name: 'define_group', signature: 'define_group(name: string, body: fn)', description: 'Define a mixer group', category: 'definition', insertText: 'define_group("${1:name}", || {\n    ${2:// Group contents}\n})' },
  { name: 'define_macro', signature: 'define_macro(name: string, body: fn)', description: 'Define a triggerable macro', category: 'definition', insertText: 'define_macro("${1:name}", || {\n    ${2:// Macro code}\n})' },
  { name: 'define_send', signature: 'define_send(name: string, fx: string)', description: 'Define an aux send bus', category: 'definition', insertText: 'define_send("${1:reverb_bus}", "${2:reverb}")' },

  // Builders
  { name: 'voice', signature: 'voice(name: string) -> Voice', description: 'Create a voice builder', category: 'builder', insertText: 'voice("${1:name}").synth("${2:synthdef}").gain(db(-6))' },
  { name: 'pattern', signature: 'pattern(name: string) -> Pattern', description: 'Create a rhythmic pattern', category: 'builder', insertText: 'pattern("${1:name}")\n    .on(${2:voice})\n    .step("${3:x... x... x... x...}")\n    .start()' },
  { name: 'melody', signature: 'melody(name: string) -> Melody', description: 'Create a melodic sequence', category: 'builder', insertText: 'melody("${1:name}")\n    .on(${2:voice})\n    .notes("${3:C4 E4 G4 C5}")\n    .start()' },
  { name: 'sequence', signature: 'sequence(name: string) -> Sequence', description: 'Create an arrangement sequence', category: 'builder', insertText: 'sequence("${1:name}")\n    .loop_bars(${2:8})\n    .clip(0..bars(${3:4}), ${4:pattern})\n    .start()' },
  { name: 'fade', signature: 'fade(name: string) -> FadeBuilder', description: 'Create a parameter fade', category: 'builder', insertText: 'fade("${1:name}")\n    .on_group("${2:group}")\n    .param("amp")\n    .from(${3:0})\n    .to(${4:1})\n    .over_bars(${5:4})\n    .start()' },
  { name: 'fx', signature: 'fx(name: string) -> Fx', description: 'Create an effect in current group', category: 'builder', insertText: 'fx("${1:name}")\n    .synth("${2:effect}")\n    .param("${3:mix}", ${4:0.5})\n    .apply()' },
  { name: 'automation', signature: 'automation(name: string) -> Automation', description: 'Create parameter automation', category: 'builder', insertText: 'automation("${1:name}")\n    .to_param("${2:param}")\n    .curve([(0.0, ${3:0}), (1.0, ${4:1})])\n    .start()' },

  // Loading
  { name: 'sample', signature: 'sample(name: string, path: string)', description: 'Load an audio sample', category: 'loading', insertText: 'sample("${1:name}", "${2:path/to/sample.wav}")' },
  { name: 'load_sample', signature: 'load_sample(name: string, path: string)', description: 'Load an audio sample', category: 'loading', insertText: 'load_sample("${1:name}", "${2:path}")' },
  { name: 'load_sfz', signature: 'load_sfz(name: string, path: string)', description: 'Load an SFZ instrument', category: 'loading', insertText: 'load_sfz("${1:name}", "${2:path/to/instrument.sfz}")' },
  { name: 'load_vst_instrument', signature: 'load_vst_instrument(name: string, plugin: string)', description: 'Load a VST instrument', category: 'loading', insertText: 'load_vst_instrument("${1:name}", "${2:plugin_key}")' },
  { name: 'load_vst_effect', signature: 'load_vst_effect(name: string, plugin: string)', description: 'Load a VST effect', category: 'loading', insertText: 'load_vst_effect("${1:name}", "${2:plugin_key}")' },

  // Getters
  { name: 'group', signature: 'group(name: string) -> GroupHandle', description: 'Get a group handle by name', category: 'getter', insertText: 'group("${1:name}")' },
  { name: 'get_voice', signature: 'get_voice(name: string) -> Voice', description: 'Get a voice by name', category: 'getter', insertText: 'get_voice("${1:name}")' },
  { name: 'get_pattern', signature: 'get_pattern(name: string) -> Pattern', description: 'Get a pattern by name', category: 'getter', insertText: 'get_pattern("${1:name}")' },
  { name: 'get_melody', signature: 'get_melody(name: string) -> Melody', description: 'Get a melody by name', category: 'getter', insertText: 'get_melody("${1:name}")' },
  { name: 'get_effect', signature: 'get_effect(name: string) -> Effect', description: 'Get an effect by name', category: 'getter', insertText: 'get_effect("${1:name}")' },

  // Lists
  { name: 'all_group_names', signature: 'all_group_names() -> Array', description: 'Get all group names', category: 'getter', insertText: 'all_group_names()' },
  { name: 'all_voice_names', signature: 'all_voice_names() -> Array', description: 'Get all voice names', category: 'getter', insertText: 'all_voice_names()' },
  { name: 'all_pattern_names', signature: 'all_pattern_names() -> Array', description: 'Get all pattern names', category: 'getter', insertText: 'all_pattern_names()' },
  { name: 'all_melody_names', signature: 'all_melody_names() -> Array', description: 'Get all melody names', category: 'getter', insertText: 'all_melody_names()' },
  { name: 'all_effect_names', signature: 'all_effect_names() -> Array', description: 'Get all effect names', category: 'getter', insertText: 'all_effect_names()' },
  { name: 'active_synth_count', signature: 'active_synth_count() -> int', description: 'Get active synth count', category: 'getter', insertText: 'active_synth_count()' },

  // Recording
  { name: 'record', signature: 'record(id: string) -> RecordHandle', description: 'Create a recording', category: 'recording', insertText: 'record("${1:take1}")\n    .from_group("${2:main/Group}")\n    .bars(${3:4})\n    .apply()' },
  { name: 'stop_recording', signature: 'stop_recording(id: string)', description: 'Stop a recording', category: 'recording', insertText: 'stop_recording("${1:take1}")' },
  { name: 'cancel_recording', signature: 'cancel_recording(id: string)', description: 'Cancel a recording', category: 'recording', insertText: 'cancel_recording("${1:take1}")' },

  // MIDI
  { name: 'midi_device', signature: 'midi_device(name: string) -> MidiDevice', description: 'Create MIDI device connection', category: 'midi', insertText: 'midi_device("${1:controller}")\n    .input("${2:MIDI Device}")\n    .output("${3:MIDI Device}")' },
  { name: 'midi_map', signature: 'midi_map(source: MidiSource) -> MidiMap', description: 'Create MIDI mapping', category: 'midi', insertText: 'midi_map(${1:source})\n    .to_param("${2:param}")\n    .range(${3:0}, ${4:1})' },

  // Utility
  { name: 'db', signature: 'db(value: float) -> float', description: 'Convert decibels to amplitude', category: 'utility', insertText: 'db(${1:-6})' },
  { name: 'note', signature: 'note(num: int, denom: int) -> float', description: 'Calculate note duration', category: 'utility', insertText: 'note(${1:1}, ${2:4})' },
  { name: 'bars', signature: 'bars(count: float) -> int', description: 'Convert bars to beats', category: 'utility', insertText: 'bars(${1:4})' },
  { name: 'sleep', signature: 'sleep(duration: string)', description: 'Pause execution', category: 'utility', insertText: 'sleep("${1:1s}")' },
  { name: 'sleep_secs', signature: 'sleep_secs(seconds: float)', description: 'Pause for seconds', category: 'utility', insertText: 'sleep_secs(${1:1.0})' },
  { name: 'exit', signature: 'exit()', description: 'Exit the program', category: 'utility', insertText: 'exit()' },
  { name: 'trigger_macro', signature: 'trigger_macro(name: string)', description: 'Trigger a macro', category: 'utility', insertText: 'trigger_macro("${1:name}")' },
  { name: 'envelope', signature: 'envelope() -> EnvelopeBuilder', description: 'Create an envelope builder', category: 'dsp', insertText: 'envelope()\n    .${1:perc}(${2:0.01}, ${3:0.3})\n    .cleanup_on_finish()\n    .build()' },
  { name: 'melody_gen', signature: 'melody_gen(steps: int) -> MelodyGen', description: 'Create generative melody', category: 'builder', insertText: 'melody_gen(${1:16})\n    .tonality("${2:E}", "${3:minor}")\n    .density(${4:0.6})\n    .render()' },

  // DSP Helpers
  { name: 'mix', signature: 'mix(signals: Array) -> NodeRef', description: 'Mix/sum signals', category: 'dsp', insertText: 'mix([${1:sig1}, ${2:sig2}])' },
  { name: 'sum', signature: 'sum(signals: Array) -> NodeRef', description: 'Sum signals', category: 'dsp', insertText: 'sum([${1:sig1}, ${2:sig2}])' },
  { name: 'dup', signature: 'dup(signal: NodeRef, count: int) -> Array', description: 'Duplicate signal', category: 'dsp', insertText: 'dup(${1:signal}, ${2:2})' },
  { name: 'sound_in', signature: 'sound_in(channels: int) -> Array', description: 'Read from audio input', category: 'dsp', insertText: 'sound_in(${1:2})' },
  { name: 'env_perc', signature: 'env_perc(attack: float, release: float)', description: 'Percussive envelope spec', category: 'dsp', insertText: 'env_perc(${1:0.01}, ${2:0.3})' },
  { name: 'env_adsr', signature: 'env_adsr(a: float, d: float, s: float, r: float)', description: 'ADSR envelope spec', category: 'dsp', insertText: 'env_adsr(${1:0.01}, ${2:0.1}, ${3:0.7}, ${4:0.5})' },
  { name: 'env_asr', signature: 'env_asr(attack: float, sustain: float, release: float)', description: 'ASR envelope spec', category: 'dsp', insertText: 'env_asr(${1:0.1}, ${2:1.0}, ${3:0.5})' },
  { name: 'detune_spread', signature: 'detune_spread(voices: int, amount: float) -> Array', description: 'Generate detune values for unison', category: 'dsp', insertText: 'detune_spread(${1:5}, ${2:0.01})' },

  // Signal math
  { name: 'tanh', signature: 'tanh(signal) -> NodeRef', description: 'Soft saturation', category: 'math', insertText: 'tanh(${1:signal})' },
  { name: 'abs', signature: 'abs(signal) -> NodeRef', description: 'Absolute value', category: 'math', insertText: 'abs(${1:signal})' },
  { name: 'clip', signature: 'clip(signal, lo, hi) -> NodeRef', description: 'Hard clip signal', category: 'math', insertText: 'clip(${1:signal}, ${2:-1}, ${3:1})' },
  { name: 'fold', signature: 'fold(signal, lo, hi) -> NodeRef', description: 'Wave fold signal', category: 'math', insertText: 'fold(${1:signal}, ${2:-1}, ${3:1})' },
  { name: 'wrap', signature: 'wrap(signal, lo, hi) -> NodeRef', description: 'Wrap signal in range', category: 'math', insertText: 'wrap(${1:signal}, ${2:0}, ${3:1})' },
  { name: 'lerp', signature: 'lerp(a, b, t) -> NodeRef', description: 'Linear interpolation', category: 'math', insertText: 'lerp(${1:a}, ${2:b}, ${3:t})' },
  { name: 'min', signature: 'min(a, b) -> NodeRef', description: 'Minimum of two values', category: 'math', insertText: 'min(${1:a}, ${2:b})' },
  { name: 'max', signature: 'max(a, b) -> NodeRef', description: 'Maximum of two values', category: 'math', insertText: 'max(${1:a}, ${2:b})' },
  { name: 'pow', signature: 'pow(signal, exp) -> NodeRef', description: 'Raise to power', category: 'math', insertText: 'pow(${1:signal}, ${2:2})' },
];

// Methods (after dot)
export const methods = [
  // Voice methods
  { name: 'synth', signature: '.synth(name: string) -> Self', description: 'Set the synthdef to use', context: ['Voice', 'Fx'], insertText: 'synth("${1:name}")' },
  { name: 'on', signature: '.on(source) -> Self', description: 'Set the voice/sample/SFZ to use', context: ['Voice', 'Pattern', 'Melody'], insertText: 'on(${1:source})' },
  { name: 'poly', signature: '.poly(count: int) -> Voice', description: 'Set polyphony', context: ['Voice'], insertText: 'poly(${1:4})' },
  { name: 'gain', signature: '.gain(value: float) -> Self', description: 'Set volume/gain', context: ['Voice', 'GroupHandle'], insertText: 'gain(db(${1:-6}))' },
  { name: 'set_param', signature: '.set_param(name: string, value: float) -> Self', description: 'Set a parameter value', context: ['Voice', 'Pattern', 'Melody'], insertText: 'set_param("${1:param}", ${2:value})' },
  { name: 'mute', signature: '.mute() -> Self', description: 'Mute the voice/group', context: ['Voice', 'GroupHandle'], insertText: 'mute()' },
  { name: 'unmute', signature: '.unmute() -> UnmuteBuilder', description: 'Unmute the group', context: ['GroupHandle'], insertText: 'unmute()' },
  { name: 'solo', signature: '.solo(enabled: bool) -> Self', description: 'Solo the voice/group', context: ['Voice', 'GroupHandle'], insertText: 'solo(${1:true})' },
  { name: 'trigger', signature: '.trigger(params?: Map) -> Voice', description: 'Trigger the voice', context: ['Voice'], insertText: 'trigger()' },
  { name: 'note_on', signature: '.note_on(note: int, velocity?: float) -> Voice', description: 'Send note-on', context: ['Voice'], insertText: 'note_on(${1:60}, ${2:100})' },
  { name: 'note_off', signature: '.note_off(note: int) -> Voice', description: 'Send note-off', context: ['Voice'], insertText: 'note_off(${1:60})' },
  { name: 'stop', signature: '.stop() -> Self', description: 'Stop playback', context: ['Voice', 'Pattern', 'Melody', 'Sequence'], insertText: 'stop()' },
  { name: 'stop_all', signature: '.stop_all() -> Voice', description: 'Stop all playing synths', context: ['Voice'], insertText: 'stop_all()' },
  { name: 'run', signature: '.run() -> Voice', description: 'Run voice continuously', context: ['Voice'], insertText: 'run()' },
  { name: 'group', signature: '.group(name: string) -> Voice', description: 'Set voice mixer group', context: ['Voice'], insertText: 'group("${1:name}")' },

  // Pattern/Melody methods
  { name: 'step', signature: '.step(pattern: string) -> Self', description: 'Set step pattern string', context: ['Pattern', 'Melody'], insertText: 'step("${1:x... x... x... x...}")' },
  { name: 'euclid', signature: '.euclid(hits: int, steps: int) -> Pattern', description: 'Generate Euclidean rhythm', context: ['Pattern'], insertText: 'euclid(${1:5}, ${2:8})' },
  { name: 'len', signature: '.len(beats: float) -> Self', description: 'Set loop length in beats', context: ['Pattern', 'Melody'], insertText: 'len(${1:4.0})' },
  { name: 'swing', signature: '.swing(amount: float) -> Self', description: 'Add swing timing', context: ['Pattern', 'Melody'], insertText: 'swing(${1:0.3})' },
  { name: 'quantize', signature: '.quantize(grid: string) -> Self', description: 'Set quantization grid', context: ['Pattern', 'Melody', 'Sequence'], insertText: 'quantize("${1:1/16}")' },
  { name: 'lane', signature: '.lane(param: string) -> LaneBuilder', description: 'Create parameter lane', context: ['Pattern', 'Melody'], insertText: 'lane("${1:amp}")' },
  { name: 'start', signature: '.start() -> Self', description: 'Start playback', context: ['Pattern', 'Melody', 'Sequence', 'FadeBuilder', 'Automation'], insertText: 'start()' },
  { name: 'apply', signature: '.apply() -> Self', description: 'Register without starting', context: ['Pattern', 'Melody', 'Sequence', 'Fx', 'FadeBuilder', 'Voice', 'RecordHandle'], insertText: 'apply()' },
  { name: 'launch', signature: '.launch()', description: 'Ensure playing (idempotent)', context: ['Pattern', 'Melody'], insertText: 'launch()' },
  { name: 'is_playing', signature: '.is_playing() -> bool', description: 'Check if playing', context: ['Pattern', 'Melody', 'Sequence'], insertText: 'is_playing()' },

  // Melody-specific
  { name: 'notes', signature: '.notes(notes: string | Array) -> Melody', description: 'Set the notes to play', context: ['Melody'], insertText: 'notes("${1:C4 E4 G4 C5}")' },
  { name: 'scale', signature: '.scale(name: string) -> Melody', description: 'Set the scale', context: ['Melody'], insertText: 'scale("${1:minor}")' },
  { name: 'root', signature: '.root(note: string) -> Melody', description: 'Set root note', context: ['Melody'], insertText: 'root("${1:E}")' },
  { name: 'gate', signature: '.gate(duration: float) -> Melody', description: 'Set gate duration', context: ['Melody', 'EnvelopeBuilder'], insertText: 'gate(${1:0.5})' },
  { name: 'transpose', signature: '.transpose(semitones: int) -> Melody', description: 'Transpose by semitones', context: ['Melody'], insertText: 'transpose(${1:12})' },

  // Sequence methods
  { name: 'loop_bars', signature: '.loop_bars(bars: float) -> Sequence', description: 'Set loop length in bars', context: ['Sequence'], insertText: 'loop_bars(${1:8})' },
  { name: 'loop_beats', signature: '.loop_beats(beats: float) -> Sequence', description: 'Set loop length in beats', context: ['Sequence'], insertText: 'loop_beats(${1:16})' },
  { name: 'clip', signature: '.clip(range: Range, source) -> Sequence', description: 'Add a looping clip', context: ['Sequence'], insertText: 'clip(${1:0}..bars(${2:4}), ${3:pattern})' },
  { name: 'clip_once', signature: '.clip_once(range: Range, source) -> Sequence', description: 'Add a one-shot clip', context: ['Sequence'], insertText: 'clip_once(${1:0}..bars(${2:4}), ${3:pattern})' },
  { name: 'start_once', signature: '.start_once() -> Sequence', description: 'Start without looping', context: ['Sequence'], insertText: 'start_once()' },
  { name: 'pause', signature: '.pause() -> Sequence', description: 'Pause sequence', context: ['Sequence'], insertText: 'pause()' },
  { name: 'resume', signature: '.resume() -> Sequence', description: 'Resume paused sequence', context: ['Sequence'], insertText: 'resume()' },

  // Fade methods
  { name: 'on_group', signature: '.on_group(name: string) -> FadeBuilder', description: 'Target a group', context: ['FadeBuilder'], insertText: 'on_group("${1:name}")' },
  { name: 'on_voice', signature: '.on_voice(name: string) -> FadeBuilder', description: 'Target a voice', context: ['FadeBuilder'], insertText: 'on_voice("${1:name}")' },
  { name: 'on_effect', signature: '.on_effect(name: string) -> FadeBuilder', description: 'Target an effect', context: ['FadeBuilder'], insertText: 'on_effect("${1:name}")' },
  { name: 'param', signature: '.param(name: string, value?: float) -> Self', description: 'Set parameter to fade', context: ['FadeBuilder', 'Fx', 'SynthdefBuilder'], insertText: 'param("${1:amp}")' },
  { name: 'from', signature: '.from(value: float) -> FadeBuilder', description: 'Set start value', context: ['FadeBuilder'], insertText: 'from(${1:0})' },
  { name: 'to', signature: '.to(value: float) -> Self', description: 'Set target value', context: ['FadeBuilder', 'ParamFadeBuilder'], insertText: 'to(${1:1})' },
  { name: 'over', signature: '.over(duration) -> Self', description: 'Set fade duration', context: ['FadeBuilder', 'ParamFadeBuilder'], insertText: 'over(${1:4.0})' },
  { name: 'over_bars', signature: '.over_bars(bars: float) -> FadeBuilder', description: 'Set duration in bars', context: ['FadeBuilder'], insertText: 'over_bars(${1:4})' },

  // Synthdef/Fx builder
  { name: 'body', signature: '.body(closure: fn) -> ()', description: 'Set DSP processing body', context: ['SynthdefBuilder', 'FxBuilder'], insertText: 'body(|${1:freq}, ${2:amp}| {\n    ${3:// DSP code}\n})' },

  // Envelope builder
  { name: 'perc', signature: '.perc(attack: float, release: float)', description: 'Percussive envelope', context: ['EnvelopeBuilder'], insertText: 'perc(${1:0.01}, ${2:0.3})' },
  { name: 'adsr', signature: '.adsr(a, d, s, r)', description: 'ADSR envelope', context: ['EnvelopeBuilder'], insertText: 'adsr(${1:0.01}, ${2:0.1}, ${3:0.7}, ${4:0.5})' },
  { name: 'asr', signature: '.asr(attack, sustain, release)', description: 'ASR envelope', context: ['EnvelopeBuilder'], insertText: 'asr(${1:0.1}, ${2:1.0}, ${3:0.5})' },
  { name: 'attack', signature: '.attack(time: float) -> EnvelopeBuilder', description: 'Set attack time', context: ['EnvelopeBuilder'], insertText: 'attack(${1:0.01})' },
  { name: 'decay', signature: '.decay(time: float) -> EnvelopeBuilder', description: 'Set decay time', context: ['EnvelopeBuilder'], insertText: 'decay(${1:0.1})' },
  { name: 'sustain', signature: '.sustain(level: float) -> EnvelopeBuilder', description: 'Set sustain level', context: ['EnvelopeBuilder'], insertText: 'sustain(${1:0.7})' },
  { name: 'release', signature: '.release(time: float) -> EnvelopeBuilder', description: 'Set release time', context: ['EnvelopeBuilder'], insertText: 'release(${1:0.5})' },
  { name: 'cleanup_on_finish', signature: '.cleanup_on_finish() -> EnvelopeBuilder', description: 'Free synth when done', context: ['EnvelopeBuilder'], insertText: 'cleanup_on_finish()' },
  { name: 'build', signature: '.build() -> NodeRef', description: 'Build the envelope', context: ['EnvelopeBuilder'], insertText: 'build()' },

  // Group methods
  { name: 'route_to', signature: '.route_to(group: string) -> GroupHandle', description: 'Route to another group', context: ['GroupHandle'], insertText: 'route_to("${1:MainBus}")' },
  { name: 'send', signature: '.send(bus: string, amount: float) -> GroupHandle', description: 'Send to aux bus', context: ['GroupHandle'], insertText: 'send("${1:reverb_bus}", ${2:0.3})' },
  { name: 'add_effect', signature: '.add_effect(id, synthdef, params)', description: 'Add effect to chain', context: ['GroupHandle'], insertText: 'add_effect("${1:comp}", "${2:compressor}", #{})' },
  { name: 'remove_effect', signature: '.remove_effect(id: string)', description: 'Remove effect from chain', context: ['GroupHandle'], insertText: 'remove_effect("${1:id}")' },
  { name: 'fade_gain_to', signature: '.fade_gain_to(target, duration)', description: 'Fade gain over time', context: ['GroupHandle'], insertText: 'fade_gain_to(db(${1:-6}), ${2:4.0})' },
  { name: 'fade_param', signature: '.fade_param(param: string)', description: 'Start parameter fade', context: ['Voice', 'GroupHandle'], insertText: 'fade_param("${1:amp}")' },

  // Scheduling
  { name: 'now', signature: '.now()', description: 'Execute immediately', context: ['MuteBuilder', 'UnmuteBuilder'], insertText: 'now()' },
  { name: 'after', signature: '.after(time: string)', description: 'Execute after delay', context: ['MuteBuilder', 'UnmuteBuilder', 'ParamFadeBuilder'], insertText: 'after("${1:4bar}")' },

  // Effect
  { name: 'bypass', signature: '.bypass(enabled: bool) -> Self', description: 'Bypass effect', context: ['Fx', 'Effect'], insertText: 'bypass(${1:true})' },
  { name: 'set', signature: '.set(param: string, value: float)', description: 'Set effect parameter', context: ['Effect'], insertText: 'set("${1:param}", ${2:value})' },

  // Sample
  { name: 'warp_to_bpm', signature: '.warp_to_bpm(bpm: float)', description: 'Time-stretch to BPM', context: ['SampleHandle'], insertText: 'warp_to_bpm(${1:128})' },
  { name: 'slice', signature: '.slice(positions: Array)', description: 'Create sample slices', context: ['SampleHandle'], insertText: 'slice([${1:0.0}, ${2:0.25}, ${3:0.5}, ${4:0.75}])' },

  // Lane
  { name: 'values', signature: '.values(arr: Array) -> Pattern | Melody', description: 'Set lane values', context: ['LaneBuilder'], insertText: 'values([${1:1.0}, ${2:0.5}, ${3:0.8}, ${4:0.5}])' },

  // Recording
  { name: 'from_group', signature: '.from_group(path: string)', description: 'Set group to record', context: ['RecordHandle'], insertText: 'from_group("${1:main/Group}")' },
  { name: 'bars', signature: '.bars(count: float)', description: 'Set recording length', context: ['RecordHandle'], insertText: 'bars(${1:4})' },
  { name: 'count_in', signature: '.count_in(bars: float)', description: 'Add count-in bars', context: ['RecordHandle'], insertText: 'count_in(${1:1})' },
  { name: 'metronome', signature: '.metronome(enabled: bool)', description: 'Enable metronome', context: ['RecordHandle'], insertText: 'metronome(${1:true})' },
  { name: 'to_file', signature: '.to_file(path: string)', description: 'Save to WAV file', context: ['RecordHandle'], insertText: 'to_file("${1:takes/recording.wav}")' },

  // MelodyGen
  { name: 'tonality', signature: '.tonality(root, scale)', description: 'Set key and scale', context: ['MelodyGen'], insertText: 'tonality("${1:E}", "${2:minor}")' },
  { name: 'density', signature: '.density(value: float)', description: 'Set note density', context: ['MelodyGen'], insertText: 'density(${1:0.6})' },
  { name: 'seed', signature: '.seed(value: int)', description: 'Set random seed', context: ['MelodyGen'], insertText: 'seed(${1:42})' },
  { name: 'render', signature: '.render() -> Array', description: 'Render to note array', context: ['MelodyGen'], insertText: 'render()' },
  { name: 'motif', signature: '.motif(notes: Array)', description: 'Set melodic motif', context: ['MelodyGen'], insertText: 'motif([${1:"C4"}, ${2:"E4"}, ${3:"G4"}])' },
  { name: 'contour', signature: '.contour(shape: string)', description: 'Set melodic contour', context: ['MelodyGen'], insertText: 'contour("${1:arch}")' },
];

// Keywords
export const keywords = [
  'let', 'if', 'else', 'for', 'while', 'loop', 'fn', 'return', 'true', 'false',
  'import', 'in', 'continue', 'break', 'throw', 'try', 'catch'
];

// Extract synthdefs from StdlibDocs for autocomplete
export function getSynthdefs() {
  const synthdefs = [];

  for (const category of stdlibCategories) {
    const categoryData = stdlibData[category];
    if (!categoryData?.subcategories) continue;

    for (const [subcategory, items] of Object.entries(categoryData.subcategories)) {
      for (const item of items) {
        if (item.type === 'synthdef') {
          synthdefs.push({
            name: item.name,
            description: item.character || item.description,
            category: `${category}/${subcategory}`,
            genre: item.genre,
            params: item.params,
            path: item.path
          });
        }
      }
    }
  }

  return synthdefs;
}

// Extract UGen functions for autocomplete
export function getUGens() {
  const ugens = [];

  for (const [category, items] of Object.entries(ugenData)) {
    for (const ugen of items) {
      // Add both ar and kr versions
      for (const func of ugen.functions || []) {
        ugens.push({
          name: func,
          ugenName: ugen.name,
          description: ugen.description,
          category: category,
          inputs: ugen.inputs,
          signature: formatUGenSignature(func, ugen.inputs)
        });
      }
    }
  }

  return ugens;
}

function formatUGenSignature(funcName, inputs) {
  if (!inputs || inputs.length === 0) return `${funcName}()`;

  const params = inputs.map(i => {
    const defaultStr = i.default !== undefined ? ` = ${i.default}` : '';
    return `${i.name}${defaultStr}`;
  }).join(', ');

  return `${funcName}(${params})`;
}

// Cached data
let _synthdefs = null;
let _ugens = null;

export function getAllSynthdefs() {
  if (!_synthdefs) _synthdefs = getSynthdefs();
  return _synthdefs;
}

export function getAllUGens() {
  if (!_ugens) _ugens = getUGens();
  return _ugens;
}

// Fuzzy match function
export function fuzzyMatch(text, query) {
  if (!query) return true;
  const lowerText = text.toLowerCase();
  const lowerQuery = query.toLowerCase();

  // Exact prefix match gets highest score
  if (lowerText.startsWith(lowerQuery)) return 100;

  // Contains match
  if (lowerText.includes(lowerQuery)) return 50;

  // Fuzzy match (all chars in order)
  let qi = 0;
  for (let i = 0; i < lowerText.length && qi < lowerQuery.length; i++) {
    if (lowerText[i] === lowerQuery[qi]) qi++;
  }

  return qi === lowerQuery.length ? 25 : 0;
}
