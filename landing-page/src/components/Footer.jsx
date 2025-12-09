import React from 'react';
import './Footer.css';

function Footer() {
  const currentYear = new Date().getFullYear();

  return (
    <footer className="footer">
      <div className="container">
        <div className="footer__content">
          <div className="footer__brand">
            <a href="#" className="footer__logo">
              <span className="footer__bracket">{'{'}</span>
              <span className="footer__v">v</span>
              <span className="footer__text">ibelang</span>
              <span className="footer__bracket">{'}'}</span>
            </a>
            <p className="footer__tagline">Make music with code.</p>
          </div>

          <div className="footer__links">
            <div className="footer__column">
              <h4>Product</h4>
              <a href="#features">Features</a>
              <a href="#sounds">Sound Library</a>
              <a href="#demo">Demo</a>
              <a href="#start">Get Started</a>
            </div>

            <div className="footer__column">
              <h4>Resources</h4>
              <a href="#/docs">Documentation</a>
              <a href="https://github.com/trusch/vibelang/tree/main/examples" target="_blank" rel="noopener noreferrer">Examples</a>
              <a href="#/docs">API Reference</a>
              <a href="https://github.com/trusch/vibelang/releases" target="_blank" rel="noopener noreferrer">Changelog</a>
            </div>

            <div className="footer__column">
              <h4>Community</h4>
              <a href="https://github.com/trusch/vibelang" target="_blank" rel="noopener noreferrer">GitHub</a>
              <a href="https://github.com/trusch/vibelang/discussions" target="_blank" rel="noopener noreferrer">Discussions</a>
              <a href="https://github.com/trusch/vibelang/issues" target="_blank" rel="noopener noreferrer">Issues</a>
            </div>
          </div>
        </div>

        <div className="footer__bottom">
          <div className="footer__copyright">
            <span>&copy; {currentYear} VibeLang</span>
            <span className="footer__separator">|</span>
            <span>Open Source</span>
            <span className="footer__separator">|</span>
            <span>MIT License</span>
          </div>

          <div className="footer__made-with">
            <span>Made with</span>
            <span className="footer__heart">~</span>
            <span>and SuperCollider</span>
          </div>
        </div>
      </div>
    </footer>
  );
}

export default Footer;
