import React, { useState } from 'react';
import './GetStarted.css';

function GetStarted() {
  const [copied, setCopied] = useState(false);

  const installCommand = 'cargo install vibelang-cli';

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(installCommand);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  return (
    <section id="start" className="get-started">
      <div className="container">
        <div className="get-started__header">
          <span className="get-started__label">// get started</span>
          <h2>Ready to make some noise?</h2>
          <p className="get-started__subtitle">
            VibeLang runs on SuperCollider's synthesis engine. Install both and start making music in minutes.
          </p>
        </div>

        <div className="get-started__grid">
          <div className="get-started__step">
            <div className="get-started__step-number">1</div>
            <h3>Install SuperCollider</h3>
            <p>VibeLang uses SuperCollider's audio engine. Download it from the official site.</p>
            <a
              href="https://supercollider.github.io/downloads"
              className="btn btn-secondary"
              target="_blank"
              rel="noopener noreferrer"
            >
              Download SuperCollider
            </a>
          </div>

          <div className="get-started__step">
            <div className="get-started__step-number">2</div>
            <h3>Install VibeLang</h3>
            <p>Use Cargo (Rust's package manager) to install VibeLang.</p>
            <div className="get-started__install">
              <code>$ {installCommand}</code>
              <button
                className="get-started__copy"
                onClick={handleCopy}
                aria-label="Copy to clipboard"
              >
                {copied ? 'copied!' : 'copy'}
              </button>
            </div>
          </div>

          <div className="get-started__step">
            <div className="get-started__step-number">3</div>
            <h3>Create Your First Beat</h3>
            <p>Create a .vibe file and run it. Watching is on by default!</p>
            <div className="get-started__commands">
              <code>$ vibe beat.vibe</code>
            </div>
          </div>
        </div>

        <div className="get-started__example">
          <div className="get-started__example-header">
            <span>Your first beat: beat.vibe</span>
          </div>
          <pre className="get-started__example-code"><code>{`// Your first VibeLang beat
set_tempo(120);

import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";

let kick = voice("kick").synth("kick_808").gain(db(-6));
let hat = voice("hat").synth("hihat_808_closed").gain(db(-12));

pattern("kick").on(kick).step("x...x...x...x...").start();
pattern("hat").on(hat).step(".x.x.x.x.x.x.x.x").start();

// Save the file and hear it play!
// Try changing the pattern and saving again.`}</code></pre>
        </div>

        <div className="get-started__resources">
          <h3>Resources</h3>
          <div className="get-started__links">
            <a href="#" className="get-started__link">
              <span className="get-started__link-icon">{'>'}</span>
              <span>Documentation</span>
            </a>
            <a href="#" className="get-started__link">
              <span className="get-started__link-icon">{'>'}</span>
              <span>Example Projects</span>
            </a>
            <a href="#" className="get-started__link">
              <span className="get-started__link-icon">{'>'}</span>
              <span>Standard Library Reference</span>
            </a>
            <a href="#" className="get-started__link">
              <span className="get-started__link-icon">{'>'}</span>
              <span>Community Discord</span>
            </a>
          </div>
        </div>
      </div>
    </section>
  );
}

export default GetStarted;
