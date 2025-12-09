import React, { useState } from 'react';
import { highlightCode } from '../utils/syntaxHighlight';
import './CodeDemo.css';

const demos = [
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
        let env = NewEnvGenBuilder(env_adsr(0.01, 0.1, 0.7, 0.3), gate)
            .with_done_action(2.0).build();
        filt * env * amp
    });

let lead = voice("lead").synth("supersaw").poly(4);
melody("hook").on(lead)
    .notes("E4 - - - G4 - - - | B4 - - - E5 - - - | D5 - - - B4 - - - | G4 - - - E4 - - -")
    .start();`
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
            From a simple beat to a full track—VibeLang grows with you. Start simple, go deep when you're ready.
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
