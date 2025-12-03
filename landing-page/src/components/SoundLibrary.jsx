import React from 'react';
import './SoundLibrary.css';

const categories = [
  {
    name: 'Drums',
    count: 54,
    icon: 'X',
    items: ['Kicks (12)', 'Snares (12)', 'Hi-hats (10)', 'Claps (10)', 'Percussion (10)'],
    color: '#ff6b35'
  },
  {
    name: 'Bass',
    count: 30,
    icon: '~',
    items: ['Sub Bass (10)', 'Acid 303 (10)', 'Pluck Bass (10)'],
    color: '#7c3aed'
  },
  {
    name: 'Leads',
    count: 20,
    icon: '^',
    items: ['Synth Leads (10)', 'Pluck Leads (10)'],
    color: '#10b981'
  },
  {
    name: 'Pads',
    count: 20,
    icon: '=',
    items: ['Ambient Pads (10)', 'Lush Pads (10)'],
    color: '#3b82f6'
  },
  {
    name: 'FX',
    count: 15,
    icon: '*',
    items: ['Risers', 'Impacts', 'Sweeps', 'Textures', 'Drones'],
    color: '#f59e0b'
  },
  {
    name: 'Theory',
    count: '80+',
    icon: '#',
    items: ['30+ Scales', 'Chords', '20+ Progressions', 'Bass Patterns'],
    color: '#ec4899'
  }
];

function SoundLibrary() {
  return (
    <section id="sounds" className="sound-library">
      <div className="container">
        <div className="sound-library__header">
          <span className="sound-library__label">// sound palette</span>
          <h2>149+ production-ready sounds</h2>
          <p className="sound-library__subtitle">
            Every sound is a readable .vibe file. Learn synthesis by exploring,
            or use them as-is for instant results.
          </p>
        </div>

        <div className="sound-library__grid">
          {categories.map((category, index) => (
            <div key={index} className="sound-card" style={{ '--card-color': category.color }}>
              <div className="sound-card__header">
                <span className="sound-card__icon">{category.icon}</span>
                <span className="sound-card__count">{category.count}</span>
              </div>
              <h3 className="sound-card__title">{category.name}</h3>
              <ul className="sound-card__list">
                {category.items.map((item, i) => (
                  <li key={i}>{item}</li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        <div className="sound-library__example">
          <div className="sound-library__example-header">
            <span className="sound-library__example-label">Example: Using the standard library</span>
          </div>
          <div className="sound-library__example-content">
            <pre><code>{`// Import entire category
import "stdlib/drums" as drums;

// Or import specific sounds
import "stdlib/bass/acid" as acid;

// Use them directly
let kick = voice("kick", drums.kicks.kick_808);
let bass = voice("bass", acid.acid_303);

// Each sound file is pure VibeLang code
// Open it, learn from it, modify it`}</code></pre>
          </div>
        </div>

        <div className="sound-library__cta">
          <p>All sounds are open source and customizable.</p>
          <a href="#" className="btn btn-secondary">
            Browse the full library
          </a>
        </div>
      </div>
    </section>
  );
}

export default SoundLibrary;
