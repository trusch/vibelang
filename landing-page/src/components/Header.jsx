import React, { useState, useEffect } from 'react';
import './Header.css';

function Header({ theme, onToggleTheme, page = 'home' }) {
  const [scrolled, setScrolled] = useState(false);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const [typedChars, setTypedChars] = useState(0);
  const [animationStarted, setAnimationStarted] = useState(false);
  const fullText = 'ibelang';

  useEffect(() => {
    const handleScroll = () => {
      const isScrolled = window.scrollY > 50;
      setScrolled(isScrolled);

      // On home page, start animation when user scrolls down
      if (page === 'home' && isScrolled && !animationStarted) {
        setAnimationStarted(true);
      }
    };
    window.addEventListener('scroll', handleScroll);
    return () => window.removeEventListener('scroll', handleScroll);
  }, [page, animationStarted]);

  // Typing animation - starts immediately on docs, or when scrolled on home
  useEffect(() => {
    // On docs page, start immediately
    // On home page, start when scrolled
    const shouldAnimate = page === 'docs' || (page === 'home' && animationStarted);
    if (!shouldAnimate) return;

    const startDelay = setTimeout(() => {
      let charIndex = 0;
      const typeInterval = setInterval(() => {
        charIndex++;
        setTypedChars(charIndex);
        if (charIndex >= fullText.length) {
          clearInterval(typeInterval);
        }
      }, 80 + Math.random() * 60);

      return () => clearInterval(typeInterval);
    }, 300);

    return () => clearTimeout(startDelay);
  }, [page, animationStarted]);

  const themeIcon = theme === 'dark' ? '◐' : theme === 'light' ? '○' : '◑';

  // Docs page always has solid background
  const isScrolled = page === 'docs' || scrolled;

  // Show full logo on docs always, on home only when scrolled
  const showFullLogo = page === 'docs' || (page === 'home' && scrolled);

  const navigateTo = (hash) => {
    setMobileMenuOpen(false);
    if (page === 'docs' && !hash.startsWith('#/docs')) {
      // From docs, go to home page section
      window.location.hash = '';
      if (hash !== '#') {
        setTimeout(() => { window.location.hash = hash; }, 50);
      }
    }
  };

  return (
    <header className={`header ${isScrolled ? 'header--scrolled' : ''}`}>
      <div className="container header__container">
        <a
          href="#"
          onClick={(e) => { e.preventDefault(); navigateTo('#'); }}
          className="header__logo"
        >
          <span className="header__logo-bracket">{'{'}</span>
          <span className="header__logo-v">v</span>
          {showFullLogo && (
            <>
              <span className="header__logo-rest">
                {fullText.slice(0, typedChars)}
              </span>
              <span className={`header__logo-cursor ${typedChars >= fullText.length ? 'header__logo-cursor--done' : ''}`}></span>
            </>
          )}
          <span className="header__logo-bracket">{'}'}</span>
        </a>

        <nav className={`header__nav ${mobileMenuOpen ? 'header__nav--open' : ''}`}>
          <a href="#start" onClick={() => navigateTo('#start')}>Get Started</a>
          <a href="#features" onClick={() => navigateTo('#features')}>Features</a>
          <a href="#demo" onClick={() => navigateTo('#demo')}>Demo</a>
          <a href="#sounds" onClick={() => navigateTo('#sounds')}>Sounds</a>
          <a
            href="#/docs"
            className={page === 'docs' ? 'active' : ''}
            onClick={() => setMobileMenuOpen(false)}
          >
            Docs
          </a>
        </nav>

        <div className="header__actions">
          <button
            className="header__btn"
            onClick={onToggleTheme}
            aria-label={`Toggle theme`}
          >
            <span className="header__btn-icon">{themeIcon}</span>
          </button>

          <a
            href="https://github.com/trusch/vibelang"
            className="header__btn"
            target="_blank"
            rel="noopener noreferrer"
            aria-label="GitHub"
          >
            <svg className="header__btn-icon" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
            </svg>
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
