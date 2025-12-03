import React, { useEffect, useState } from 'react';
import './Hero.css';

function Hero() {
  const [showFull, setShowFull] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setShowFull(true), 1200);
    return () => clearTimeout(timer);
  }, []);

  return (
    <section className="hero">
      <div className="hero__bg-pattern"></div>
      <div className="container hero__container">
        <div className="hero__content">
          <div className="hero__logo">
            <span className="hero__bracket">{'{'}</span>
            <span className="hero__v">v</span>
            <span className={`hero__rest ${showFull ? 'hero__rest--visible' : ''}`}>
              ibelang
            </span>
            <span className="hero__cursor"></span>
            <span className="hero__bracket">{'}'}</span>
          </div>

          <p className="hero__tagline fade-in stagger-1">
            Make music with code.
          </p>

          <p className="hero__description fade-in stagger-2">
            A musical programming language that transforms text into beats.
            Real-time synthesis, live coding, and 149+ production-ready sounds.
            <span className="hero__highlight"> Edit. Save. Hear.</span>
          </p>

          <div className="hero__cta fade-in stagger-3">
            <a href="#start" className="btn btn-primary">
              <span>$</span> cargo install vibelang
            </a>
            <a href="#demo" className="btn btn-secondary">
              See it in action
            </a>
          </div>

          <div className="hero__stats fade-in stagger-4">
            <div className="hero__stat">
              <span className="hero__stat-number">149+</span>
              <span className="hero__stat-label">Sounds</span>
            </div>
            <div className="hero__stat">
              <span className="hero__stat-number">30+</span>
              <span className="hero__stat-label">Scales</span>
            </div>
            <div className="hero__stat">
              <span className="hero__stat-number">~0ms</span>
              <span className="hero__stat-label">Hot Reload</span>
            </div>
            <div className="hero__stat">
              <span className="hero__stat-number">100%</span>
              <span className="hero__stat-label">Git-friendly</span>
            </div>
          </div>
        </div>

        <div className="hero__demo fade-in stagger-2">
          <div className="hero__terminal">
            <div className="hero__terminal-header">
              <span className="hero__terminal-dot"></span>
              <span className="hero__terminal-dot"></span>
              <span className="hero__terminal-dot"></span>
              <span className="hero__terminal-title">beat.vibe</span>
            </div>
            <pre className="hero__terminal-code"><code>{`// Create a beat in 4 lines
let kick = voice("kick", drums.kick_808);
let hat = voice("hat", drums.hihat_closed);

kick.pattern("X...X...X...X...", 120);
hat.pattern("..X...X...X...X.", 120);`}</code></pre>
          </div>
        </div>
      </div>

      <div className="hero__scroll-indicator">
        <span>scroll</span>
        <div className="hero__scroll-line"></div>
      </div>
    </section>
  );
}

export default Hero;
