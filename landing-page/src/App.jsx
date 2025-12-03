import React, { useState, useEffect } from 'react';
import Header from './components/Header';
import Hero from './components/Hero';
import Features from './components/Features';
import CodeDemo from './components/CodeDemo';
import SoundLibrary from './components/SoundLibrary';
import Workflow from './components/Workflow';
import GetStarted from './components/GetStarted';
import Footer from './components/Footer';

function App() {
  const [theme, setTheme] = useState(() => {
    if (typeof window !== 'undefined') {
      return localStorage.getItem('theme') || 'system';
    }
    return 'system';
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

  const toggleTheme = () => {
    setTheme(current => {
      if (current === 'system') return 'dark';
      if (current === 'dark') return 'light';
      return 'system';
    });
  };

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
