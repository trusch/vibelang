import React, { useState, useEffect } from 'react';
import Header from './components/Header';
import Hero from './components/Hero';
import Features from './components/Features';
import CodeDemo from './components/CodeDemo';
import SoundLibrary from './components/SoundLibrary';
import Workflow from './components/Workflow';
import GetStarted from './components/GetStarted';
import Footer from './components/Footer';
import Documentation from './components/Documentation';

function App() {
  const [theme, setTheme] = useState(() => {
    if (typeof window !== 'undefined') {
      return localStorage.getItem('theme') || 'system';
    }
    return 'system';
  });

  const [currentPage, setCurrentPage] = useState(() => {
    if (typeof window !== 'undefined') {
      return window.location.hash === '#/docs' ? 'docs' : 'home';
    }
    return 'home';
  });

  useEffect(() => {
    const root = document.documentElement;
    if (theme === 'system') {
      root.removeAttribute('data-theme');
      localStorage.removeItem('theme');
    } else {
      root.setAttribute('data-theme', theme);
      localStorage.setItem('theme', theme);
    }
  }, [theme]);

  // Handle browser navigation (hash-based routing)
  useEffect(() => {
    const handleHashChange = () => {
      setCurrentPage(window.location.hash === '#/docs' ? 'docs' : 'home');
    };
    window.addEventListener('hashchange', handleHashChange);
    return () => window.removeEventListener('hashchange', handleHashChange);
  }, []);

  const toggleTheme = () => {
    setTheme(current => {
      if (current === 'system') return 'dark';
      if (current === 'dark') return 'light';
      return 'system';
    });
  };

  // Render docs page
  if (currentPage === 'docs') {
    return <Documentation theme={theme} onToggleTheme={toggleTheme} />;
  }

  // Render landing page
  return (
    <>
      <Header theme={theme} onToggleTheme={toggleTheme} />
      <main>
        <Hero />
        <Features />
        <CodeDemo />
        <SoundLibrary />
        <Workflow />
        <GetStarted />
      </main>
      <Footer />
    </>
  );
}

export default App;
