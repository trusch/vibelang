import React, { useState } from 'react';
import './ExtensionShowcase.css';

// Import screenshots
import arrangementImg from 'url:../../assets/arrangement_view.png';
import melodyEditorImg from 'url:../../assets/melody_editor.png';
import patternEditorImg from 'url:../../assets/pattern_editor.png';
import sampleBrowserImg from 'url:../../assets/sample_browser.png';
import soundDesignerImg from 'url:../../assets/sound_designer.png';

const features = [
  {
    id: 'arrangement',
    name: 'Arrangement Timeline',
    tag: 'Production',
    icon: '|||',
    image: arrangementImg,
    headline: 'Arrange your track like a pro',
    description: 'Full DAW-style timeline right inside VS Code. Drag clips, arrange sections, and see your entire song structure at a glance. Color-coded patterns and melodies make navigation instant.',
    highlights: [
      'Visual clip arrangement with drag & drop',
      'Multiple track lanes with color coding',
      'Beat-synced playhead and looping',
      'Zoom and fit-to-view controls'
    ]
  },
  {
    id: 'melody',
    name: 'Piano Roll',
    tag: 'Composition',
    icon: '♪♫',
    image: melodyEditorImg,
    headline: 'Compose melodies visually',
    description: 'A full-featured piano roll editor with MIDI-style note editing. Draw notes, adjust velocities, and hear changes in real-time. The generated code stays perfectly in sync.',
    highlights: [
      'Click-to-place note editing',
      'Numpad recording for fast input',
      'Scale & octave auto-assignment',
      'Live code generation as you edit'
    ]
  },
  {
    id: 'pattern',
    name: 'Pattern Sequencer',
    tag: 'Beats',
    icon: '▪▪▪',
    image: patternEditorImg,
    headline: 'Build beats with a drum machine',
    description: 'The classic step sequencer experience. Click cells to place hits, use euclidean rhythms for instant groove, and trigger patterns with your numpad. Perfect for drum programming.',
    highlights: [
      '16-step sequencer grid',
      'Euclidean rhythm generator',
      'Numpad pad triggers (1-9)',
      'Swing and offset controls'
    ]
  },
  {
    id: 'samples',
    name: 'Sample Browser',
    tag: 'Library',
    icon: '♦♦♦',
    image: sampleBrowserImg,
    headline: 'Browse. Preview. Drop in.',
    description: 'Explore the built-in sound library with smart search and tags. Filter by genre, character, or instrument type. One click generates the import code—instant integration.',
    highlights: [
      'Searchable preset library',
      'Tag-based filtering system',
      'One-click code generation',
      'Favorites and collections'
    ]
  },
  {
    id: 'sound-designer',
    name: 'Sound Designer',
    tag: 'Synthesis',
    icon: '◈◈◈',
    image: soundDesignerImg,
    headline: 'Build synths visually',
    description: 'Node-based synthesizer design powered by SuperCollider. Connect oscillators, filters, and envelopes with visual cables. Preview your sound with the built-in keyboard.',
    highlights: [
      '100+ UGen building blocks',
      'Visual cable connections',
      'Real-time envelope editor',
      'Built-in keyboard preview'
    ]
  }
];

const shortcuts = [
  { keys: 'Ctrl+Shift+Space', action: 'Play / Stop' },
  { keys: 'Ctrl+Shift+A', action: 'Open Arrangement' },
  { keys: 'Ctrl+Shift+M', action: 'Open Mixer' },
  { keys: 'Ctrl+Shift+D', action: 'Sound Designer' },
  { keys: 'Ctrl+Shift+E', action: 'Pattern Editor' },
  { keys: 'Ctrl+Shift+R', action: 'Melody Editor' },
  { keys: 'Ctrl+Shift+B', action: 'Sample Browser' },
];

function ExtensionShowcase() {
  const [activeFeature, setActiveFeature] = useState(features[0]);
  const [imageLoaded, setImageLoaded] = useState(false);

  const handleFeatureChange = (feature) => {
    setImageLoaded(false);
    setActiveFeature(feature);
  };

  return (
    <section id="extension" className="extension-showcase">
      <div className="container">
        <div className="extension-showcase__header">
          <span className="extension-showcase__label">// vs code extension</span>
          <h2>Your DAW inside your editor</h2>
          <p className="extension-showcase__subtitle">
            The VibeLang extension transforms VS Code into a complete music production environment.
            Visual editors, real-time preview, and seamless code sync—all without leaving your IDE.
          </p>
        </div>

        {/* Feature tabs */}
        <div className="extension-showcase__tabs">
          {features.map((feature) => (
            <button
              key={feature.id}
              className={`extension-showcase__tab ${activeFeature.id === feature.id ? 'extension-showcase__tab--active' : ''}`}
              onClick={() => handleFeatureChange(feature)}
            >
              <span className="extension-showcase__tab-icon">{feature.icon}</span>
              <span className="extension-showcase__tab-name">{feature.name}</span>
            </button>
          ))}
        </div>

        {/* Main content */}
        <div className="extension-showcase__content">
          <div className="extension-showcase__info">
            <div className="extension-showcase__tag">{activeFeature.tag}</div>
            <h3>{activeFeature.headline}</h3>
            <p>{activeFeature.description}</p>
            <ul className="extension-showcase__highlights">
              {activeFeature.highlights.map((highlight, index) => (
                <li key={index}>
                  <span className="extension-showcase__check">+</span>
                  {highlight}
                </li>
              ))}
            </ul>
          </div>

          <div className="extension-showcase__preview">
            <div className="extension-showcase__preview-header">
              <span className="extension-showcase__dot"></span>
              <span className="extension-showcase__dot"></span>
              <span className="extension-showcase__dot"></span>
              <span className="extension-showcase__preview-title">{activeFeature.name}</span>
              <span className="extension-showcase__preview-badge">VS Code</span>
            </div>
            <div className="extension-showcase__preview-content">
              <img
                src={activeFeature.image}
                alt={activeFeature.name}
                className={`extension-showcase__image ${imageLoaded ? 'extension-showcase__image--loaded' : ''}`}
                onLoad={() => setImageLoaded(true)}
              />
              {!imageLoaded && (
                <div className="extension-showcase__loading">
                  <span>Loading...</span>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Keyboard shortcuts section */}
        <div className="extension-showcase__shortcuts">
          <div className="extension-showcase__shortcuts-header">
            <span className="extension-showcase__shortcuts-icon">~</span>
            <h4>Keyboard-driven workflow</h4>
          </div>
          <div className="extension-showcase__shortcuts-grid">
            {shortcuts.map((shortcut, index) => (
              <div key={index} className="extension-showcase__shortcut">
                <kbd>{shortcut.keys}</kbd>
                <span>{shortcut.action}</span>
              </div>
            ))}
          </div>
        </div>

        {/* Bi-directional sync highlight */}
        <div className="extension-showcase__sync">
          <div className="extension-showcase__sync-visual">
            <div className="extension-showcase__sync-box">
              <span className="extension-showcase__sync-label">Code</span>
              <code>melody("lead").notes("C4 E4 G4")</code>
            </div>
            <div className="extension-showcase__sync-arrows">
              <span>↔</span>
            </div>
            <div className="extension-showcase__sync-box">
              <span className="extension-showcase__sync-label">Visual</span>
              <div className="extension-showcase__sync-notes">
                <span></span><span></span><span></span>
              </div>
            </div>
          </div>
          <div className="extension-showcase__sync-text">
            <h4>Bi-directional sync</h4>
            <p>
              Edit in the visual editor—code updates automatically. Edit the code—visuals reflect instantly.
              Your source file is always the single source of truth.
            </p>
          </div>
        </div>

        {/* Install CTA */}
        <div className="extension-showcase__cta">
          <a
            href="https://marketplace.visualstudio.com/items?itemName=vibelang.vibelang"
            className="extension-showcase__cta-button"
            target="_blank"
            rel="noopener noreferrer"
          >
            <span className="extension-showcase__cta-icon">
              <svg viewBox="0 0 24 24" fill="currentColor">
                <path d="M17.583 7.208L12 2.625 2.333 10.125l2.875 2.167L12 7.042l7.208 5.458L12 17.958l-6.792-5.25-2.875 2.167L12 21.375l9.667-7.5-4.084-6.667z"/>
              </svg>
            </span>
            Install from VS Code Marketplace
          </a>
          <p className="extension-showcase__cta-hint">
            Or search "VibeLang" in VS Code extensions
          </p>
        </div>
      </div>
    </section>
  );
}

export default ExtensionShowcase;
