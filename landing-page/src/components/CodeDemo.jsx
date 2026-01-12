import React, { useState, useRef } from 'react';
import { highlightCode } from '../utils/syntaxHighlight';
import './CodeDemo.css';
import soundDesignAudio from 'url:../assets/sound-design-demo.mp3';

const demos = [
  {
    id: 'sound-design',
    name: 'Sound Design',
    description: 'Deep DSP control. Layer oscillators, modulate filters, shape envelopes. Build your own signature sound.',
    audio: soundDesignAudio,
    code: `// Complex sound design with FM, filtering, and modulation
set_tempo(120);
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";

define_synthdef("space-wobble")
    .param("freq", 440.0).param("amp", 1.).param("gate", 1.0)
    .body(|freq, amp, gate| {
        let env = envelope().adsr(0.5, 0.2, 0.7, 0.1)
            .gate(gate).cleanup_on_finish().build();
        let modifiers = [0, 1.01, 2.02, 3.03, 4.04];
        let freq = sin_osc_kr(0.5) * freq + (freq - (freq / 100.));
        let out = modifiers.map(|i| {
            let base_freq = freq * (i + 1.);
            let p1 = sin_osc_ar(base_freq);
            let p2 = saw_ar(base_freq);
            let p3 = pulse_ar(base_freq, 0.5);
            (p1 + p2 + p3) 
        }).sum() / (0. + modifiers.len());
        let sub_lfo = sin_osc_kr(0.8) * 4.0 + 3.;
        let lfo = sin_osc_kr(sub_lfo) * 800.0 + 900.0;
        rhpf_ar(out, lfo, 0.3) * env * amp
    });

let wobble = voice("space-wobble").synth("space-wobble").gain(db(-25));
let kick = voice("kick").synth("kick_808").gain(db(-6));
let snare = voice("snare").synth("snare_808").gain(db(-8));
let hat = voice("hat").synth("hihat_808_closed").gain(db(-14));

melody("test").on(wobble).notes("A2 - C2 - | A3 A3 A3 - | ----").start();
pattern("kick").on(kick).step("x...x...x...x...").start();
pattern("snare").on(snare).step("....x.......x...").start();
pattern("hat").on(hat).step("x.x.x.x.x.x.x.x.").start();
`
  },
  {
    id: 'sandstorm',
    name: 'Sandstorm',
    description: 'Darude\'s 1999 anthem. The drop that defined a generation.',
    code: `// Dududududu
set_tempo(136);
import "stdlib/synths/leads/lead_saw.vibe";
import "stdlib/drums/kicks/kick_909.vibe";

let lead = voice("lead").synth("lead_saw").poly(1);
let kick = voice("kick").synth("kick_909");

pattern("kick").on(kick).step("x...x...x...x...").start();
melody("storm").on(lead)
    .notes("B4 B4 B4 B4 E5 E5 E5 E5 | D5 D5 D5 D5 A4 A4 B4 B4")
    .start();`
  },
  {
    id: 'da-funk',
    name: 'Da Funk',
    description: 'Daft Punk\'s gritty filter bass. French house in 8 lines.',
    code: `// The funk. You can feel it.
set_tempo(110);
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/bass/acid/acid_303_classic.vibe";

let kick = voice("kick").synth("kick_808");
let bass = voice("bass").synth("acid_303_classic").poly(1);

pattern("kick").on(kick).step("x...x...x...x...").start();
melody("funk").on(bass)
    .notes("G2 . . . Bb2 . . . | C3 . . . G2 . . .")
    .start();`
  },
  {
    id: 'ambient-pad',
    name: 'Ambient Pad',
    description: 'Lush pads with reverb and delay. Effects make the vibe.',
    code: `// Space and atmosphere
set_tempo(90);
import "stdlib/pads/ambient/pad_warm.vibe";
import "stdlib/effects/reverb.vibe";
import "stdlib/effects/delay.vibe";

let pad = voice("pad").synth("pad_warm").poly(4);

melody("drift").on(pad)
    .notes("C4 - - - - - - - E4 - - - - - - - | G4 - - - - - - - C5 - - - - - - -")
    .start();

fx("verb").synth("reverb").param("room", 0.8).param("mix", 0.5).apply();
fx("echo").synth("delay").param("time", 0.375).param("feedback", 0.4).apply();`
  },
  {
    id: 'custom-synth',
    name: 'Custom Synth',
    description: 'Build your own sounds from scratch. Full DSP control.',
    code: `// Fat supersaw from oscillators
set_tempo(128);
define_synthdef("supersaw")
    .param("freq", 440.0).param("amp", 0.3).param("gate", 1.0)
    .body(|freq, amp, gate| {
        let osc = saw_ar(freq*0.99) + saw_ar(freq) + saw_ar(freq*1.01);
        let filt = rlpf_ar(osc * 0.3, 2000.0, 0.3);
        let env = envelope().adsr(0.01, 0.1, 0.7, 0.3)
            .gate(gate).cleanup_on_finish().build();
        filt * env * amp
    });

let lead = voice("lead").synth("supersaw").poly(4);
melody("hook").on(lead)
    .notes("E4 - - - G4 - - - | B4 - - - E5 - - - | D5 - - - B4 - - - | G4 - - - E4 - - -")
    .start();`
  },
  {
    id: 'arrangement',
    name: 'Arrangement',
    description: 'Compose full tracks with timeline-based clip arrangement.',
    code: `// Arrange a full track with sequences
set_tempo(120);
import "stdlib/drums/kicks/kick_909.vibe";
import "stdlib/drums/snares/snare_808.vibe";
import "stdlib/synths/leads/lead_saw.vibe";

let kick = voice("kick").synth("kick_909");
let snare = voice("snare").synth("snare_808");
let lead = voice("lead").synth("lead_saw").poly(4);

// Define your building blocks
pattern("verse_kick").on(kick).step("x...x...x...x...");
pattern("verse_snare").on(snare).step("....x.......x...");
melody("verse_lead").on(lead).notes("C4 - E4 - G4 - - - | A4 - G4 - E4 - - -");

// Arrange them on a timeline
sequence("song")
    .clip(pattern("verse_kick"), beat(0), beat(32))
    .clip(pattern("verse_snare"), beat(8), beat(32))
    .clip(melody("verse_lead"), beat(16), beat(32))
    .start();`
  },
  {
    id: 'midi-out',
    name: 'MIDI Out',
    description: 'Control external synths, drum machines, and DAWs.',
    code: `// Send MIDI to external hardware
set_tempo(128);

// Open a MIDI output device
let hw = midi_out("USB MIDI Device");

// Create a MIDI voice on channel 1
let bass = voice("hw_bass")
    .midi(hw, 1)
    .poly(4);

// Play melodies on external gear
melody("bass_line").on(bass)
    .notes("C2 - - E2 | G2 - - C2")
    .velocity(100)
    .start();

// Send CC messages for filter sweeps
pattern("filter").on(bass)
    .step("x.x.x.x.x.x.x.x.")
    .cc(74, [0, 20, 40, 60, 80, 100, 120, 100])
    .start();`
  },
];

function CodeDemo() {
  const [activeDemo, setActiveDemo] = useState(demos[0]);
  const [isPlaying, setIsPlaying] = useState(false);
  const audioRef = useRef(null);

  const handlePlayPause = () => {
    if (!activeDemo.audio) return;

    if (isPlaying) {
      audioRef.current?.pause();
      setIsPlaying(false);
    } else {
      if (!audioRef.current) {
        audioRef.current = new Audio(activeDemo.audio);
        audioRef.current.onended = () => setIsPlaying(false);
      }
      audioRef.current.play();
      setIsPlaying(true);
    }
  };

  const handleDemoChange = (demo) => {
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current = null;
    }
    setIsPlaying(false);
    setActiveDemo(demo);
  };

  return (
    <section id="demo" className="code-demo">
      <div className="container">
        <div className="code-demo__header">
          <span className="code-demo__label">// see it in action</span>
          <h2>Code that sounds good</h2>
          <p className="code-demo__subtitle">
            From a simple beat to a full track—VibeLang grows with you. Start simple, go deep when you're ready.
          </p>
        </div>

        <div className="code-demo__tabs">
          {demos.map((demo) => (
            <button
              key={demo.id}
              className={`code-demo__tab ${activeDemo.id === demo.id ? 'code-demo__tab--active' : ''}`}
              onClick={() => handleDemoChange(demo)}
            >
              {demo.name}
            </button>
          ))}
        </div>

        <div className="code-demo__content">
          <div className="code-demo__info">
            <h3>{activeDemo.name}</h3>
            <p>{activeDemo.description}</p>
            {activeDemo.audio && (
              <button
                className={`code-demo__play-btn ${isPlaying ? 'code-demo__play-btn--playing' : ''}`}
                onClick={handlePlayPause}
              >
                <span className="code-demo__play-icon">{isPlaying ? '||' : '>'}</span>
                <span>{isPlaying ? 'Pause' : 'Listen'}</span>
              </button>
            )}
          </div>

          <div className="code-demo__editor">
            <div className="code-demo__editor-header">
              <span className="code-demo__dot"></span>
              <span className="code-demo__dot"></span>
              <span className="code-demo__dot"></span>
              <span className="code-demo__filename">{activeDemo.id}.vibe</span>
              <span className="code-demo__status">
                <span className="code-demo__status-dot"></span>
                watching
              </span>
            </div>
            <pre className="code-demo__code">
              <code>{highlightCode(activeDemo.code)}</code>
            </pre>
          </div>
        </div>

        <div className="code-demo__hint">
          <span className="code-demo__hint-icon">✨</span>
          <span>Edit. Save. Hear it change. That's the whole workflow.</span>
        </div>
      </div>
    </section>
  );
}

export default CodeDemo;
