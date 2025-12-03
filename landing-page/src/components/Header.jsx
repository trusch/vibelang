import React, { useState, useEffect } from 'react';
import './Header.css';

function Header({ theme, onToggleTheme }) {
  const [scrolled, setScrolled] = useState(false);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  useEffect(() => {
    const handleScroll = () => {
      setScrolled(window.scrollY > 50);
    };
    window.addEventListener('scroll', handleScroll);
    return () => window.removeEventListener('scroll', handleScroll);
  }, []);

  const themeIcon = theme === 'dark' ? '>' : theme === 'light' ? '<' : '=';
  const themeLabel = theme === 'dark' ? 'dark' : theme === 'light' ? 'light' : 'auto';

  return (
    <header className={`header ${scrolled ? 'header--scrolled' : ''}`}>
      <div className="container header__container">
        <a href="#" className="header__logo">
          <span className="header__logo-bracket">{'{'}</span>
          <span className="header__logo-text">v</span>
          <span className="header__logo-bracket">{'}'}</span>
        </a>

        <nav className={`header__nav ${mobileMenuOpen ? 'header__nav--open' : ''}`}>
          <a href="#features" onClick={() => setMobileMenuOpen(false)}>Features</a>
          <a href="#demo" onClick={() => setMobileMenuOpen(false)}>Demo</a>
          <a href="#sounds" onClick={() => setMobileMenuOpen(false)}>Sounds</a>
          <a href="#start" onClick={() => setMobileMenuOpen(false)}>Get Started</a>
        </nav>

        <div className="header__actions">
          <button
            className="header__theme-toggle"
            onClick={onToggleTheme}
            aria-label={`Current theme: ${themeLabel}. Click to change.`}
          >
            <span className="header__theme-icon">{themeIcon}</span>
            <span className="header__theme-label">{themeLabel}</span>
          </button>

          <a
            href="https://github.com/anthropics/vibelang"
            className="btn btn-secondary header__github"
            target="_blank"
            rel="noopener noreferrer"
          >
            GitHub
          </a>

          <button
            className="header__mobile-toggle"
            onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
            aria-label="Toggle menu"
          >
            <span></span>
            <span></span>
            <span></span>
          </button>
        </div>
      </div>
    </header>
  );
}

export default Header;
