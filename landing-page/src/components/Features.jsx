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
    icon: '||',
    title: 'Bus Mixing',
    description: 'Group instruments, apply effects, route audio. Professional mixing workflow with reverb, delay, compression, and more.',
    tag: 'Effects'
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
        let env = env_adsr(0.01, 0.1, 0.7, 0.3);
        let envGen = NewEnvGenBuilder(env, gate)
            .with_done_action(2.0)
            .build();
        rlpf_ar(osc, 800.0, 0.2) * envGen * amp
    });`)}</code></pre>
          </div>
        </div>
      </div>
    </section>
  );
}

export default Features;
