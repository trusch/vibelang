import React, { useState, useEffect } from 'react';
import Header from './components/Header';
import Hero from './components/Hero';
import Features from './components/Features';
import CodeDemo from './components/CodeDemo';
import ExtensionShowcase from './components/ExtensionShowcase';
import SoundLibrary from './components/SoundLibrary';
import Workflow from './components/Workflow';
import GetStarted from './components/GetStarted';
import Footer from './components/Footer';
import Documentation from './components/Documentation';
import Playground from './components/Playground';

function App() {
  const [theme, setTheme] = useState(() => {
    if (typeof window !== 'undefined') {
      return localStorage.getItem('theme') || 'system';
    }
    return 'system';
  });

  const [currentPage, setCurrentPage] = useState(() => {
    if (typeof window !== 'undefined') {
      const hash = window.location.hash;
      if (hash === '#/docs') return 'docs';
      if (hash === '#/playground') return 'playground';
      return 'home';
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
      const hash = window.location.hash;
      if (hash === '#/docs') {
        setCurrentPage('docs');
      } else if (hash === '#/playground') {
        setCurrentPage('playground');
      } else {
        setCurrentPage('home');
      }
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

  // Render playground page
  if (currentPage === 'playground') {
    return <Playground theme={theme} onToggleTheme={toggleTheme} />;
  }

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
        <ExtensionShowcase />
        <SoundLibrary />
        <Workflow />
        <GetStarted />
      </main>
      <Footer />
    </>
  );
}

export default App;
