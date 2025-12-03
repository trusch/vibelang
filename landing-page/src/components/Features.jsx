import React from 'react';
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
            <span className="features__highlight-label">Music Theory Library</span>
            <h3>30+ scales • Complete chord library • 20+ progressions</h3>
            <p>
              Built-in music theory functions for scales, chords, and progressions.
              Generate bass lines, melodies, and voice leading automatically.
              From pentatonic to Phrygian dominant—theory meets code.
            </p>
          </div>
          <div className="features__highlight-code">
            <pre><code>{`// Use music theory helpers
let scale = theory.scale("C", "minor");
let chord = theory.chord("Am7");
let prog = theory.progression("jazz_251");

// Generate a bass line
bass.melody(theory.bass_pattern(prog), 120);`}</code></pre>
          </div>
        </div>
      </div>
    </section>
  );
}

export default Features;
