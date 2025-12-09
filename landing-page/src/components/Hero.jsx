import React, { useEffect, useState } from 'react';
import TerminalCode from './TerminalCode';
import './Hero.css';

function Hero() {
  const [typedChars, setTypedChars] = useState(0);
  const fullText = 'ibelang';

  useEffect(() => {
    // Start typing after initial delay
    const startDelay = setTimeout(() => {
      let charIndex = 0;
      const typeInterval = setInterval(() => {
        charIndex++;
        setTypedChars(charIndex);
        if (charIndex >= fullText.length) {
          clearInterval(typeInterval);
        }
      }, 80 + Math.random() * 60); // Random delay for natural feel

      return () => clearInterval(typeInterval);
    }, 800);

    return () => clearTimeout(startDelay);
  }, []);

  const heroCode = `// Your first beat — a few lines
import "stdlib/drums/kicks/kick_808.vibe";

let kick = voice("kick")
    .synth("kick_808");

pattern("beat")
    .on(kick)
    .step("x...x...x...x...")
    .start();`;

  return (
    <section className="hero">
      <div className="hero__bg-pattern"></div>
      <div className="container hero__container">
        <div className="hero__content">
          <div className="hero__logo">
            <span className="hero__bracket">{'{'}</span>
            <span className="hero__v">v</span>
            <span className="hero__rest">
              {fullText.slice(0, typedChars)}
            </span>
            <span className={`hero__cursor ${typedChars >= fullText.length ? 'hero__cursor--done' : ''}`}></span>
            <span className="hero__bracket">{'}'}</span>
          </div>

          <p className="hero__tagline fade-in stagger-1">
            Make music with code.
          </p>

          <p className="hero__description fade-in stagger-2">
            VibeLang is a programming language for making music.
            Write beats, melodies, and full tracks in code.
            <span className="hero__highlight"> Edit. Save. Hear it change.</span>
          </p>

          <div className="hero__cta fade-in stagger-3">
            <a href="#start" className="btn btn-primary">
              <span>$</span> cargo install vibelang-cli
            </a>
            <a href="#demo" className="btn btn-secondary">
              See examples
            </a>
          </div>

          <div className="hero__stats fade-in stagger-4">
            <div className="hero__stat">
              <span className="hero__stat-number">580+</span>
              <span className="hero__stat-label">Sounds</span>
            </div>
            <div className="hero__stat">
              <span className="hero__stat-number">~1ms</span>
              <span className="hero__stat-label">Hot Reload</span>
            </div>
            <div className="hero__stat">
              <span className="hero__stat-number">100%</span>
              <span className="hero__stat-label">Git-friendly</span>
            </div>
          </div>
        </div>

        <div className="hero__demo fade-in stagger-2">
          <TerminalCode code={heroCode} filename="beat.vibe" />
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
