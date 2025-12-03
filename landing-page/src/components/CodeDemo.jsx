import React, { useState } from 'react';
import './CodeDemo.css';

const demos = [
  {
    id: 'beat',
    name: 'Simple Beat',
    description: 'A classic four-on-the-floor pattern with hi-hats',
    code: `// Simple 4/4 beat - the foundation of electronic music
set_tempo(120);

import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";
import "stdlib/drums/snares/snare_808.vibe";

let kick = voice("kick").synth("kick_808").gain(db(-6));
let snare = voice("snare").synth("snare_808").gain(db(-8));
let hat = voice("hat").synth("hihat_808_closed").gain(db(-12));

// x = hit, . = rest
pattern("kick").on(kick).step("x...x...x...x...").start();
pattern("snare").on(snare).step("....x.......x...").start();
pattern("hat").on(hat).step(".x.x.x.x.x.x.x.x").start();`
  },
  {
    id: 'melody',
    name: 'Bass Line',
    description: 'A funky bass line with the 303',
    code: `// Funky bass line with acid sound
set_tempo(110);

import "stdlib/bass/acid/acid_303_classic.vibe";

let bass = voice("bass").synth("acid_303_classic").gain(db(-10)).poly(1);

// Notes: pitch, - = sustain, . = rest, | = visual bar
melody("bassline")
    .on(bass)
    .notes("C2 - - . | E2 - . . | G2 - . . | Bb2 - . .")
    .start();`
  },
  {
    id: 'house',
    name: 'Deep House',
    description: 'A complete deep house groove with arrangement',
    code: `// Deep House groove with pad and bass
set_tempo(122);

import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/hihats/hihat_808_open.vibe";
import "stdlib/bass/sub/sub_deep.vibe";
import "stdlib/effects/reverb.vibe";

let drums = define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let hat = voice("hat").synth("hihat_808_open").gain(db(-14));

    pattern("kick").on(kick).step("x...x...x...x...").start();
    pattern("hat").on(hat).step(".x.x.x.x.x.x.x.x").start();
});

let bass = define_group("Bass", || {
    let sub = voice("sub").synth("sub_deep").gain(db(-10)).poly(1);

    melody("bassline")
        .on(sub)
        .notes("C2 - - - | C2 - - - | A1 - - - | F1 - - -")
        .start();

    fx("bass_verb").synth("reverb").param("room", 0.3).param("mix", 0.1).apply();
});`
  },
  {
    id: 'synthdef',
    name: 'Custom Synth',
    description: 'Design your own synthesizer',
    code: `// Create a custom detuned supersaw
define_synthdef("supersaw")
    .param("freq", 440.0)
    .param("amp", 0.3)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        // Detuned sawtooth oscillators
        let osc1 = saw_ar(freq * 0.99);
        let osc2 = saw_ar(freq);
        let osc3 = saw_ar(freq * 1.01);
        let mix = (osc1 + osc2 + osc3) * 0.3;

        // Filter and envelope
        let filtered = rlpf_ar(mix, 2000.0, 0.3);
        let env = env_adsr(0.01, 0.1, 0.7, 0.3);
        let env = NewEnvGenBuilder(env, gate).with_done_action(2.0).build();

        filtered * env * amp
    });

let lead = voice("lead").synth("supersaw").gain(db(-6)).poly(4);
melody("lead").on(lead).notes("C4 - E4 - | G4 - C5 - | B4 - G4 - | E4 - C4 -").start();`
  }
];

function CodeDemo() {
  const [activeDemo, setActiveDemo] = useState(demos[0]);

  return (
    <section id="demo" className="code-demo">
      <div className="container">
        <div className="code-demo__header">
          <span className="code-demo__label">// see it in action</span>
          <h2>Code that sounds good</h2>
          <p className="code-demo__subtitle">
            From simple beats to complex compositions—see how VibeLang makes music production feel like programming should.
          </p>
        </div>

        <div className="code-demo__tabs">
          {demos.map((demo) => (
            <button
              key={demo.id}
              className={`code-demo__tab ${activeDemo.id === demo.id ? 'code-demo__tab--active' : ''}`}
              onClick={() => setActiveDemo(demo)}
            >
              {demo.name}
            </button>
          ))}
        </div>

        <div className="code-demo__content">
          <div className="code-demo__info">
            <h3>{activeDemo.name}</h3>
            <p>{activeDemo.description}</p>
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
              <code>{activeDemo.code}</code>
            </pre>
          </div>
        </div>

        <div className="code-demo__hint">
          <span className="code-demo__hint-icon">*</span>
          <span>Save the file and hear changes instantly in watch mode</span>
        </div>
      </div>
    </section>
  );
}

export default CodeDemo;
