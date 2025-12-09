import React, { useState } from 'react';
import { highlightCode } from '../utils/syntaxHighlight';
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

  const exampleCode = `// Your first beat — copy this into beat.vibe
set_tempo(120);

import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/drums/hihats/hihat_808_closed.vibe";

let kick = voice("kick")
    .synth("kick_808")
    .gain(db(-6));

let hat = voice("hat")
    .synth("hihat_808_closed")
    .gain(db(-12));

pattern("kick").on(kick).step("x...x...x...x...").start();
pattern("hat").on(hat).step(".x.x.x.x.x.x.x.x").start();

// Change the patterns and save. Instant feedback!`;

  return (
    <section id="start" className="get-started">
      <div className="container">
        <div className="get-started__header">
          <span className="get-started__label">// get started</span>
          <h2>Ready to jam?</h2>
          <p className="get-started__subtitle">
            Three steps. Five minutes. Then you're making music with code.
          </p>
        </div>

        <div className="get-started__grid">
          <div className="get-started__step">
            <div className="get-started__step-number">1</div>
            <h3>Get SuperCollider</h3>
            <p>The audio engine that powers VibeLang. It's free and open source.</p>
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
            <p>One command if you have Rust. No Rust? Get it at rustup.rs first.</p>
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
            <h3>Start Playing</h3>
            <p>Create a .vibe file, run it, and start experimenting. Watching is on by default!</p>
            <div className="get-started__commands">
              <code>$ vibe beat.vibe</code>
            </div>
          </div>
        </div>

        <div className="get-started__example">
          <div className="get-started__example-header">
            <span>beat.vibe</span>
            <span className="get-started__example-status">
              <span className="get-started__example-dot"></span>
              ready to run
            </span>
          </div>
          <pre className="get-started__example-code">
            <code>{highlightCode(exampleCode)}</code>
          </pre>
        </div>

        <div className="get-started__resources">
          <h3>Keep exploring</h3>
          <div className="get-started__links">
            <a href="https://github.com/trusch/vibelang" className="get-started__link" target="_blank" rel="noopener noreferrer">
              <span className="get-started__link-icon">→</span>
              <span>GitHub & Source</span>
            </a>
            <a href="https://github.com/trusch/vibelang/tree/main/examples" className="get-started__link" target="_blank" rel="noopener noreferrer">
              <span className="get-started__link-icon">→</span>
              <span>Example Projects</span>
            </a>
            <a href="https://github.com/trusch/vibelang/tree/main/crates/vibelang-std/stdlib" className="get-started__link" target="_blank" rel="noopener noreferrer">
              <span className="get-started__link-icon">→</span>
              <span>Standard Library</span>
            </a>
            <a href="https://github.com/trusch/vibelang/issues" className="get-started__link" target="_blank" rel="noopener noreferrer">
              <span className="get-started__link-icon">→</span>
              <span>Report Issues</span>
            </a>
          </div>
        </div>
      </div>
    </section>
  );
}

export default GetStarted;
