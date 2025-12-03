import React, { useState } from 'react';
import './CodeDemo.css';

const demos = [
  {
    id: 'beat',
    name: 'Simple Beat',
    description: 'A classic four-on-the-floor pattern with hi-hats',
    code: `// Simple 4/4 beat - the foundation of electronic music
import "stdlib/drums/kicks" as kicks;
import "stdlib/drums/hihats" as hihats;
import "stdlib/drums/snares" as snares;

let kick = voice("kick", kicks.kick_808);
let snare = voice("snare", snares.snare_tight);
let hat = voice("hat", hihats.hihat_closed);

// X = hit, . = rest
kick.pattern ("X...X...X...X...", 120);
snare.pattern("....X.......X...", 120);
hat.pattern  ("..X...X...X...X.", 120);`
  },
  {
    id: 'melody',
    name: 'Bass Line',
    description: 'A funky bass line with swing',
    code: `// Funky bass line with swing timing
import "stdlib/bass/acid" as bass;

let synth = voice("bass", bass.acid_303);
synth.gain(0.8);

// Notes: pitch, - = sustain, . = rest, | = visual bar
synth.melody(\`
  C2 - - . | E2 - . . | G2 - . . | Bb2 - . .
  C2 - - . | E2 - . . | G2 . F2 . | E2 - . .
\`, 110, #{ swing: 0.2 });`
  },
  {
    id: 'house',
    name: 'Deep House',
    description: 'A complete deep house groove',
    code: `// Deep House groove with pad and bass
import "stdlib/drums" as drums;
import "stdlib/bass/sub" as sub;
import "stdlib/pads/lush" as pads;

let kick = voice("kick", drums.kicks.kick_deep);
let hat = voice("hat", drums.hihats.hihat_open);
let bass = voice("bass", sub.sub_warm);
let pad = voice("pad", pads.pad_warm);

// Drums with swing
kick.pattern("X...X...X...X...", 122, #{ swing: 0.15 });
hat.pattern ("..X...X...X...X.", 122, #{ swing: 0.15 });

// Bass follows the chord root
bass.melody("C2 - - - | C2 - - - | Am2 - - - | F2 - - -", 122);

// Lush pad chord progression
pad.melody("C4 - - - | E4 - - - | A3 - - - | F3 - - -", 122);
pad.gain(0.4);`
  },
  {
    id: 'synthdef',
    name: 'Custom Synth',
    description: 'Design your own synthesizer',
    code: `// Create a custom detuned supersaw
define_synthdef("supersaw")
  .param("freq", 440.0)
  .param("detune", 0.1)
  .param("cutoff", 2000.0)
  .body(|freq, detune, cutoff| {
    // 7 detuned sawtooth oscillators
    let saws = (0..7).map(|i| {
      let ratio = 1.0 + (i - 3) * detune * 0.01;
      saw_ar(freq * ratio)
    }).sum() / 7.0;

    // Filter and envelope
    let filtered = rlpf_ar(saws, cutoff, 0.3);
    let env = env_adsr_ar(0.01, 0.1, 0.7, 0.3);

    filtered * env
  });

let lead = voice("lead", "supersaw");
lead.melody("C4 E4 G4 C5 | B4 G4 E4 C4", 128);`
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
