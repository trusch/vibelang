import React from 'react';
import { highlightCode } from '../utils/syntaxHighlight';
import './Features.css';

const features = [
  {
    icon: '>>',
    title: 'Live Coding',
    description: 'Edit your .vibe file, hit save, hear the change instantly. Zero restart, zero latency. Your music evolves as fast as your ideas.',
    tag: 'Real-time'
  },
  {
    icon: '[]',
    title: 'Pattern Sequencer',
    description: 'Intuitive X-and-dot notation for drums. Add swing, euclidean rhythms, and ghost notes. From four-on-the-floor to breakbeats.',
    tag: 'Rhythms'
  },
  {
    icon: '~~',
    title: 'Melodic Freedom',
    description: 'Write melodies using standard note names: C4, A#3, Bb2. Automatic pitch handling, sustain, and expressive timing built in.',
    tag: 'Melodies'
  },
  {
    icon: '|||',
    title: 'Sequences & Arrangement',
    description: 'Arrange patterns and melodies on a timeline. Build song structures with sections, loops, and one-shot clips. Compose full tracks.',
    tag: 'Arrangement'
  },
  {
    icon: '()',
    title: 'SynthDef Engine',
    description: 'Design your own synthesizers using SuperCollider UGens directly in code. From simple sine waves to complex FM beasts.',
    tag: 'Synthesis'
  },
  {
    icon: '{}',
    title: 'SFZ Instruments',
    description: 'Load sampled instruments from SFZ files. Realistic pianos, orchestral sounds, vintage synths—all with a single line of code.',
    tag: 'Samples'
  },
  {
    icon: '<>',
    title: 'MIDI (Experimental)',
    description: 'Play voices live from MIDI keyboards and controllers — input is experimental but works today. MIDI output (notes, CCs, clock to external hardware) is in active development.',
    tag: 'Hardware'
  },
  {
    icon: '=|',
    title: 'Eurorack Recreations',
    description: 'Full modular systems rebuilt in code, shipped as example projects: Mutable Instruments, Verbos, 4ms, Erica Synths, Intellijel, and a Make Noise resynthesizer.',
    tag: 'Modular'
  },
  {
    icon: '||',
    title: 'Groups & Mixing',
    description: 'Organize voices into hierarchical groups. Apply effects per-group, control gain and pan. Professional mixing workflow.',
    tag: 'Mixing'
  }
];

function Features() {
  return (
    <section id="features" className="features">
      <div className="container">
        <div className="features__header">
          <span className="features__label">// capabilities</span>
          <h2>Everything you need to make music</h2>
          <p className="features__subtitle">
            From beats to full compositions—VibeLang handles the complexity so you can focus on creativity.
          </p>
        </div>

        <div className="features__grid">
          {features.map((feature, index) => (
            <div key={index} className="feature-card">
              <div className="feature-card__icon">{feature.icon}</div>
              <div className="feature-card__tag">{feature.tag}</div>
              <h3 className="feature-card__title">{feature.title}</h3>
              <p className="feature-card__description">{feature.description}</p>
            </div>
          ))}
        </div>

        <div className="features__highlight">
          <div className="features__highlight-content">
            <span className="features__highlight-label">SuperCollider Powered</span>
            <h3>Real audio synthesis • Zero latency • Professional quality</h3>
            <p>
              VibeLang compiles to SuperCollider, the same engine used in universities
              and professional studios worldwide. Get pristine audio quality
              with sub-millisecond latency and rock-solid timing.
            </p>
          </div>
          <div className="features__highlight-code">
            <pre><code>{highlightCode(`// Build your own synth from scratch
define_synthdef("my_bass")
    .param("freq", 440.0)
    .param("amp", 0.3)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        let osc = saw_ar(freq) + saw_ar(freq * 0.5);
        let env = envelope().adsr(0.01, 0.1, 0.7, 0.3)
            .gate(gate).cleanup_on_finish().build();
        rlpf_ar(osc, 800.0, 0.2) * env * amp
    });`)}</code></pre>
          </div>
        </div>

        <div className="features__highlight features__highlight--reverse">
          <div className="features__highlight-code">
            <pre><code>{highlightCode(`# REST API for external control
curl -X POST localhost:1606/transport/play

# Get all voices
curl localhost:1606/voices

# Update a parameter in real-time
curl -X PUT localhost:1606/voices/bass/params/cutoff \\
  -d '{"value": 800}'

# WebSocket for live updates
wscat -c ws://localhost:1606/ws`)}</code></pre>
          </div>
          <div className="features__highlight-content">
            <span className="features__highlight-label">REST API & WebSocket</span>
            <h3>Programmatic control • Live integration • Build tools</h3>
            <p>
              Full HTTP REST API and WebSocket support. Control playback,
              update parameters, and sync state with external tools.
              Build custom UIs, integrate with OSC controllers, or automate your workflow.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

export default Features;
