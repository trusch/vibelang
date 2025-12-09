import React from 'react';
import { highlightCode } from '../utils/syntaxHighlight';
import './SoundLibrary.css';

const categories = [
  {
    name: 'Drums',
    count: 125,
    icon: 'X',
    items: ['Kicks (20)', 'Snares (16)', 'Hi-hats (10)', 'Latin Percussion (20)', 'Drum Machines (16)', 'Breakbeats (2)', 'Foley (5)'],
    color: '#ff6b35'
  },
  {
    name: 'Bass',
    count: 75,
    icon: '~',
    items: ['Sub Bass (10)', 'Acoustic (8)', 'Synth Bass (8)', 'Genre-Specific (12)', 'Acid 303', 'Reese', 'Wobble'],
    color: '#7c3aed'
  },
  {
    name: 'Leads & Pads',
    count: 91,
    icon: '▣',
    items: ['Classic Leads (8)', 'Modern Leads (7)', 'Organic Leads (7)', 'Analog Pads (5)', 'Cinematic Pads (5)', 'Textures (10)'],
    color: '#3b82f6'
  },
  {
    name: 'Classic Synths',
    count: 16,
    icon: '♦',
    items: ['Minimoog', 'Juno', 'Jupiter', 'TB-303', 'MS-20', 'CS-80', 'Prophet', 'Polysix'],
    color: '#ec4899'
  },
  {
    name: 'Keys & Piano',
    count: 19,
    icon: '♪',
    items: ['Grand Piano', 'Rhodes (2)', 'Wurlitzer', 'Hammond', 'Harpsichord', 'Pipe Organ', 'Mellotron (3)'],
    color: '#f59e0b'
  },
  {
    name: 'World',
    count: 24,
    icon: '◇',
    items: ['Sitar', 'Tabla', 'Kalimba', 'Koto', 'Oud', 'Erhu', 'Didgeridoo', 'Hang Drum', 'Mbira'],
    color: '#10b981'
  },
  {
    name: 'Orchestral',
    count: 28,
    icon: '♫',
    items: ['Strings (9)', 'Brass (7)', 'Woodwinds (6)', 'Bells (7)', 'Timpani', 'Wind Chimes'],
    color: '#06b6d4'
  },
  {
    name: 'Cinematic & Retro',
    count: 12,
    icon: '◎',
    items: ['Braams', 'Impacts', 'Drones', 'Whooshes', 'Chip Sounds (4)', 'Game SFX (4)'],
    color: '#84cc16'
  },
  {
    name: 'Effects',
    count: 66,
    icon: '*',
    items: ['Delays (11)', 'Reverbs (8)', 'Filters (9)', 'Modulation (12)', 'Dynamics (10)', 'Distortion (10)'],
    color: '#8b5cf6'
  },
  {
    name: 'FX & Utility',
    count: 68,
    icon: '^',
    items: ['Risers', 'Impacts', 'Sweeps', 'Test Tones', 'Tuners', 'Click Tracks', 'Noise Generators'],
    color: '#6b7280'
  }
];

function SoundLibrary() {
  return (
    <section id="sounds" className="sound-library">
      <div className="container">
        <div className="sound-library__header">
          <span className="sound-library__label">// sound palette</span>
          <h2>580+ production-ready sounds</h2>
          <p className="sound-library__subtitle">
            Every sound is a readable .vibe file. Peek under the hood to learn synthesis,
            or just use them and start making music.
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
            <pre><code>{highlightCode(`// Import the sounds you need
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/bass/acid/acid_303_classic.vibe";

// Create voices and start playing
let kick = voice("kick")
    .synth("kick_808")
    .gain(db(-6));

let bass = voice("bass")
    .synth("acid_303_classic")
    .gain(db(-10));

// Each .vibe file is readable code
// Open it, learn from it, make it yours`)}</code></pre>
          </div>
        </div>

        <div className="sound-library__cta">
          <p>All sounds are open source and customizable. Tweak them, learn from them, make them yours.</p>
          <a href="https://github.com/trusch/vibelang/tree/main/crates/vibelang-std/stdlib" className="btn btn-secondary" target="_blank" rel="noopener noreferrer">
            Browse the full library
          </a>
        </div>
      </div>
    </section>
  );
}

export default SoundLibrary;
