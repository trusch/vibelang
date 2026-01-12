import React, { useState, useEffect, useRef, useCallback } from 'react';
import './Playground.css';
import CodeEditor from './playground/CodeEditor';
import StdlibExplorer from './playground/StdlibExplorer';
import TutorialPanel from './playground/TutorialPanel';
import TipsWidget from './playground/TipsWidget';
import KeyboardShortcuts from './playground/KeyboardShortcuts';
import { useSoundPreview } from '../hooks/useSoundPreview';

// Default example script - uses stdlib!
const DEFAULT_SCRIPT = `// Welcome to VibeLang!
// Press Play or Ctrl+Enter to hear your music.

// Import sounds from the standard library
import "stdlib/drums/kicks/kick_909.vibe";
import "stdlib/drums/claps/clap_808.vibe";
import "stdlib/drums/hihats/hihat_909_closed.vibe";
import "stdlib/bass/genre/bass_house.vibe";

set_tempo(124);

define_group("Drums", || {
    let kick = voice("kick").synth("kick_909").gain(db(-6));
    let snare = voice("snare").synth("clap_808").gain(db(-9));
    let hat = voice("hat").synth("hihat_909_closed").gain(db(-15));

    // Pattern notation: x = hit, . = rest
    pattern("kick").on(kick).step("x... x... x... x...").start();
    pattern("snare").on(snare).step(".... x... .... x...").start();
    pattern("hat").on(hat).step("..x. ..x. ..x. ..x.").start();
});

define_group("Bass", || {
    let bass = voice("bass").synth("bass_house").gain(db(-9));

    melody("bassline")
        .on(bass)
        .notes("A2 - . . | A2 . G2 . | A2 - . . | C3 . A2 .")
        .start();
});
`;

// Template scripts for quick start
const TEMPLATES = [
  {
    id: 'house',
    name: 'House Beat',
    icon: '4/4',
    script: DEFAULT_SCRIPT
  },
  {
    id: 'dnb',
    name: 'Drum & Bass',
    icon: 'D&B',
    script: `// Fast Drum & Bass groove
import "stdlib/drums/kicks/kick_dnb.vibe";
import "stdlib/drums/snares/snare_jungle.vibe";
import "stdlib/drums/hihats/hihat_909_closed.vibe";
import "stdlib/bass/synth/bass_moog.vibe";

set_tempo(174);

define_group("Drums", || {
    let kick = voice("kick").synth("kick_dnb").gain(db(-6));
    let snare = voice("snare").synth("snare_jungle").gain(db(-9));
    let hat = voice("hat").synth("hihat_909_closed").gain(db(-15));

    pattern("kick").on(kick).step("x....... ..x.....").start();
    pattern("snare").on(snare).step(".... x... .... x...").start();
    pattern("hat").on(hat).step("x.x.x.x. x.x.x.x.").start();
});

define_group("Bass", || {
    let bass = voice("bass").synth("bass_moog").gain(db(-6));
    melody("bass").on(bass).notes("E2 - - . | E2 . D2 . | E2 - - . | G2 - E2 .").start();
});
`
  },
  {
    id: 'ambient',
    name: 'Ambient',
    icon: 'PAD',
    script: `// Peaceful ambient atmosphere
import "stdlib/pads/ambient/pad_warm.vibe";
import "stdlib/effects/reverbs/reverb.vibe";

set_tempo(70);

define_group("Pads", || {
    let pad = voice("pad").synth("pad_warm").poly(4).gain(db(-9));
    pad.note_on(57, 0.4);  // A3
    pad.note_on(60, 0.4);  // C4
    pad.note_on(64, 0.4);  // E4
    pad.note_on(67, 0.3);  // G4
})
.fx("reverb", #{ room: 0.9, damp: 0.4, mix: 0.6 });
`
  },
  {
    id: 'acid',
    name: 'Acid Techno',
    icon: '303',
    script: `// Classic acid techno
import "stdlib/drums/kicks/kick_909.vibe";
import "stdlib/drums/hihats/hihat_909_closed.vibe";
import "stdlib/bass/acid/acid_303_classic.vibe";
import "stdlib/effects/delays/delay.vibe";

set_tempo(138);

define_group("Drums", || {
    let kick = voice("kick").synth("kick_909").gain(db(-6));
    let hat = voice("hat").synth("hihat_909_closed").gain(db(-15));
    pattern("kick").on(kick).step("x... x... x... x...").start();
    pattern("hat").on(hat).step("..x. ..x. ..x. ..x.").start();
});

define_group("Acid", || {
    let acid = voice("acid").synth("acid_303_classic").gain(db(-9));
    melody("squelch").on(acid).notes("A2 A2 . A3 | A2 . G2 . | A2 A2 . A3 | C3 . A2 .").gate(0.2).start();
})
.fx("delay", #{ time: 0.188, feedback: 0.4, mix: 0.3 });
`
  },
  {
    id: 'lofi',
    name: 'Lo-Fi',
    icon: 'CHILL',
    script: `// Chill lo-fi beat
import "stdlib/drums/kicks/kick_lofi.vibe";
import "stdlib/drums/snares/snare_lofi.vibe";
import "stdlib/drums/hihats/hihat_dusty.vibe";
import "stdlib/keys/rhodes_dark.vibe";
import "stdlib/effects/reverbs/reverb.vibe";

set_tempo(85);

define_group("Drums", || {
    let kick = voice("kick").synth("kick_lofi").gain(db(-6));
    let snare = voice("snare").synth("snare_lofi").gain(db(-9));
    let hat = voice("hat").synth("hihat_dusty").gain(db(-15));
    pattern("kick").on(kick).step("x... .... x.x. ....").start();
    pattern("snare").on(snare).step(".... x... ..x. x...").start();
    pattern("hat").on(hat).step("x.x. x.x. x.x. x.x.").start();
});

define_group("Keys", || {
    let keys = voice("keys").synth("rhodes_dark").poly(4).gain(db(-12));
    keys.note_on(60, 0.4);  // C4
    keys.note_on(64, 0.4);  // E4
    keys.note_on(67, 0.4);  // G4
    keys.note_on(71, 0.3);  // B4
})
.fx("reverb", #{ room: 0.5, damp: 0.6, mix: 0.3 });
`
  }
];

// Side panel tabs
const SIDE_TABS = [
  { id: 'stdlib', label: 'Sounds', icon: 'music' },
  { id: 'tutorials', label: 'Learn', icon: 'book' },
];

function Playground({ theme, onToggleTheme }) {
  const [script, setScript] = useState(DEFAULT_SCRIPT);
  const [isRunning, setIsRunning] = useState(false);
  const [isCompiling, setIsCompiling] = useState(false);
  const [isPlaying, setIsPlaying] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState(null);
  const [stats, setStats] = useState(null);
  const [logs, setLogs] = useState([]);
  const [audioEnabled, setAudioEnabled] = useState(false);
  const [compileStatus, setCompileStatus] = useState(null); // 'success', 'error', null

  // Beat visualization
  const [currentBeat, setCurrentBeat] = useState(0);
  const beatIntervalRef = useRef(null);

  // Side panel state
  const [sidePanel, setSidePanel] = useState('tutorials');
  const [sidePanelCollapsed, setSidePanelCollapsed] = useState(false);

  // Modal state
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [showTemplates, setShowTemplates] = useState(false);

  // Tutorial tracking
  const [currentTutorial, setCurrentTutorial] = useState(null);

  const vibelangRef = useRef(null);

  // Sound preview hook
  const { isPlaying: isPreviewPlaying, currentPreview, previewSynthdef, stopPreview } = useSoundPreview(vibelangRef);

  // Initialize VibeLang on mount
  useEffect(() => {
    let mounted = true;

    async function init() {
      try {
        const vibelang = await import('../audio/vibelang-loader');
        vibelangRef.current = vibelang;

        if (mounted) {
          setIsLoading(false);
          addLog('info', 'Ready! Click anywhere to enable audio.');
        }
      } catch (err) {
        console.error('Failed to load VibeLang:', err);
        if (mounted) {
          setError('Failed to load VibeLang: ' + err.message);
          setIsLoading(false);
        }
      }
    }

    init();

    return () => {
      mounted = false;
      if (beatIntervalRef.current) {
        clearInterval(beatIntervalRef.current);
      }
    };
  }, []);

  // Beat counter when playing
  useEffect(() => {
    if (isPlaying && stats?.tempo) {
      const msPerBeat = (60 / stats.tempo) * 1000;
      beatIntervalRef.current = setInterval(() => {
        setCurrentBeat(prev => (prev + 1) % 4);
      }, msPerBeat);
    } else {
      if (beatIntervalRef.current) {
        clearInterval(beatIntervalRef.current);
        beatIntervalRef.current = null;
      }
      setCurrentBeat(0);
    }

    return () => {
      if (beatIntervalRef.current) {
        clearInterval(beatIntervalRef.current);
      }
    };
  }, [isPlaying, stats?.tempo]);

  // Keyboard shortcut handler
  useEffect(() => {
    const handleGlobalKeyDown = (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === '?') {
        e.preventDefault();
        setShowShortcuts(true);
      }
      if ((e.ctrlKey || e.metaKey) && e.key === '.') {
        e.preventDefault();
        handleStop();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'b') {
        e.preventDefault();
        handleCompile();
      }
      if (e.key === 'Escape') {
        setShowShortcuts(false);
        setShowTemplates(false);
      }
    };

    window.addEventListener('keydown', handleGlobalKeyDown);
    return () => window.removeEventListener('keydown', handleGlobalKeyDown);
  }, [script]);

  const addLog = useCallback((type, message) => {
    setLogs(prev => [...prev.slice(-50), { type, message, time: Date.now() }]);
  }, []);

  const handleCompile = async () => {
    if (!vibelangRef.current) {
      setError('VibeLang not loaded');
      return;
    }

    setError(null);
    setIsCompiling(true);
    setCompileStatus(null);
    addLog('info', 'Compiling...');

    try {
      // Initialize if needed (for parse-only mode)
      if (!vibelangRef.current.isReady()) {
        addLog('info', 'Initializing compiler...');
        await vibelangRef.current.initVibelang({ enableAudio: false });
      }

      const result = await vibelangRef.current.compileScript(script);

      if (result.success) {
        setStats(result.stats);
        setCompileStatus('success');
        addLog('success', `Compiled: ${result.stats.voices} voices, ${result.stats.patterns} patterns @ ${result.stats.tempo} BPM`);
      } else {
        setError(result.error);
        setCompileStatus('error');
        addLog('error', result.error);
      }
    } catch (err) {
      console.error('Compile error:', err);
      setError(err.message);
      setCompileStatus('error');
      addLog('error', err.message);
    } finally {
      setIsCompiling(false);
    }
  };

  const handleRun = async () => {
    if (!vibelangRef.current) {
      setError('VibeLang not loaded');
      return;
    }

    setError(null);
    setIsRunning(true);
    setCompileStatus(null);
    addLog('info', 'Running...');

    try {
      if (!vibelangRef.current.isReady()) {
        addLog('info', 'Starting audio engine...');
        const initResult = await vibelangRef.current.initVibelang({ enableAudio: true });
        setAudioEnabled(initResult.audioEnabled);
      }

      const result = await vibelangRef.current.executeScript(script);

      if (result.success) {
        setStats(result.stats);
        setIsPlaying(true);
        setCompileStatus('success');
        addLog('success', `Playing @ ${result.stats.tempo} BPM`);
      } else {
        setError(result.error);
        setCompileStatus('error');
        addLog('error', result.error);
        setIsPlaying(false);
      }
    } catch (err) {
      console.error('Execution error:', err);
      setError(err.message);
      setCompileStatus('error');
      addLog('error', err.message);
      setIsPlaying(false);
    } finally {
      setIsRunning(false);
    }
  };

  const handleStop = async () => {
    if (!vibelangRef.current) return;

    try {
      await vibelangRef.current.stopAll();
      stopPreview();
      setIsPlaying(false);
      addLog('info', 'Stopped');
    } catch (err) {
      console.error('Stop error:', err);
    }
  };

  const handleInsertCode = useCallback((code) => {
    setScript(prev => code + '\n' + prev);
    addLog('info', 'Code inserted');
  }, []);

  const handlePreview = useCallback((synthdef) => {
    if (isPreviewPlaying && currentPreview === synthdef.name) {
      stopPreview();
    } else {
      previewSynthdef(synthdef);
    }
  }, [isPreviewPlaying, currentPreview, previewSynthdef, stopPreview]);

  const handleLoadTutorial = useCallback(async (tutorial) => {
    // Reset the audio engine completely when switching tutorials
    // This ensures no state leaks between tutorials
    if (vibelangRef.current) {
      try {
        addLog('info', 'Resetting audio engine...');
        await vibelangRef.current.resetRuntime();
        setIsPlaying(false);
        setStats(null);
        setCompileStatus(null);
        setError(null);
        setAudioEnabled(false);
      } catch (err) {
        console.warn('Error resetting runtime:', err);
      }
    }

    setScript(tutorial.code);
    setCurrentTutorial(tutorial.id);
    addLog('info', `Loaded: ${tutorial.title}`);
  }, [addLog]);

  const handleSelectTemplate = useCallback(async (template) => {
    // Reset the audio engine completely when switching templates
    if (vibelangRef.current) {
      try {
        addLog('info', 'Resetting audio engine...');
        await vibelangRef.current.resetRuntime();
        setIsPlaying(false);
        setStats(null);
        setCompileStatus(null);
        setError(null);
        setAudioEnabled(false);
      } catch (err) {
        console.warn('Error resetting runtime:', err);
      }
    }

    setScript(template.script);
    setShowTemplates(false);
    addLog('info', `Template: ${template.name}`);
  }, [addLog]);

  const toggleSidePanel = useCallback((panel) => {
    if (sidePanel === panel && !sidePanelCollapsed) {
      setSidePanelCollapsed(true);
    } else {
      setSidePanel(panel);
      setSidePanelCollapsed(false);
    }
  }, [sidePanel, sidePanelCollapsed]);

  const handleClearConsole = useCallback(() => {
    setLogs([]);
    setError(null);
  }, []);

  return (
    <div className="playground" data-theme={theme}>
      {/* Header */}
      <header className="playground-header">
        <div className="playground-logo">
          <a href="#/" className="back-link">
            <svg className="back-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" width="20" height="20">
              <path d="M19 12H5M12 19l-7-7 7-7"/>
            </svg>
            <span>VibeLang</span>
          </a>
          <span className="playground-title">Playground</span>
        </div>

        <div className="playground-tabs">
          {SIDE_TABS.map(tab => (
            <button
              key={tab.id}
              className={`tab-button ${sidePanel === tab.id && !sidePanelCollapsed ? 'active' : ''}`}
              onClick={() => toggleSidePanel(tab.id)}
              title={tab.label}
            >
              <span className="tab-icon">
                {tab.icon === 'music' && (
                  <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
                    <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
                  </svg>
                )}
                {tab.icon === 'book' && (
                  <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
                    <path d="M5 13.18v4L12 21l7-3.82v-4L12 17l-7-3.82zM12 3L1 9l11 6 9-4.91V17h2V9L12 3z"/>
                  </svg>
                )}
              </span>
              <span className="tab-label">{tab.label}</span>
            </button>
          ))}
        </div>

        <div className="playground-actions">
          <button
            className="action-button template-button"
            onClick={() => setShowTemplates(!showTemplates)}
            title="Load template"
          >
            <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
              <path d="M14 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V8l-6-6zm2 16H8v-2h8v2zm0-4H8v-2h8v2zm-3-5V3.5L18.5 9H13z"/>
            </svg>
          </button>
          <button
            className="action-button shortcuts-button"
            onClick={() => setShowShortcuts(true)}
            title="Keyboard shortcuts"
          >
            <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
              <path d="M20 5H4c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm-9 3h2v2h-2V8zm0 3h2v2h-2v-2zM8 8h2v2H8V8zm0 3h2v2H8v-2zm-1 2H5v-2h2v2zm0-3H5V8h2v2zm9 7H8v-2h8v2zm0-4h-2v-2h2v2zm0-3h-2V8h2v2zm3 3h-2v-2h2v2zm0-3h-2V8h2v2z"/>
            </svg>
          </button>
          <button
            className="action-button theme-toggle"
            onClick={onToggleTheme}
            title={theme === 'dark' ? 'Light mode' : 'Dark mode'}
          >
            {theme === 'dark' ? (
              <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
                <path d="M12 7c-2.76 0-5 2.24-5 5s2.24 5 5 5 5-2.24 5-5-2.24-5-5-5zM2 13h2c.55 0 1-.45 1-1s-.45-1-1-1H2c-.55 0-1 .45-1 1s.45 1 1 1zm18 0h2c.55 0 1-.45 1-1s-.45-1-1-1h-2c-.55 0-1 .45-1 1s.45 1 1 1zM11 2v2c0 .55.45 1 1 1s1-.45 1-1V2c0-.55-.45-1-1-1s-1 .45-1 1zm0 18v2c0 .55.45 1 1 1s1-.45 1-1v-2c0-.55-.45-1-1-1s-1 .45-1 1z"/>
              </svg>
            ) : (
              <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
                <path d="M12 3c-4.97 0-9 4.03-9 9s4.03 9 9 9 9-4.03 9-9c0-.46-.04-.92-.1-1.36-.98 1.37-2.58 2.26-4.4 2.26-2.98 0-5.4-2.42-5.4-5.4 0-1.81.89-3.42 2.26-4.4-.44-.06-.9-.1-1.36-.1z"/>
              </svg>
            )}
          </button>
        </div>
      </header>

      {/* Template dropdown */}
      {showTemplates && (
        <>
          <div className="dropdown-backdrop" onClick={() => setShowTemplates(false)} />
          <div className="template-dropdown">
            <div className="dropdown-header">
              <span>Quick Start Templates</span>
              <button onClick={() => setShowTemplates(false)} className="dropdown-close">&times;</button>
            </div>
            <div className="template-list">
              {TEMPLATES.map(t => (
                <button
                  key={t.id}
                  className="template-item"
                  onClick={() => handleSelectTemplate(t)}
                >
                  <span className="template-icon">{t.icon}</span>
                  <span className="template-name">{t.name}</span>
                </button>
              ))}
            </div>
          </div>
        </>
      )}

      {/* Main content */}
      <div className="playground-main">
        {/* Editor section */}
        <div className="editor-section">
          <div className="editor-panel">
            <div className="editor-toolbar">
              <div className="toolbar-left">
                {/* Compile button */}
                <button
                  className={`toolbar-btn compile-btn ${compileStatus === 'success' ? 'success' : ''} ${compileStatus === 'error' ? 'error' : ''}`}
                  onClick={handleCompile}
                  disabled={isLoading || isCompiling}
                  title="Compile (Ctrl+B)"
                >
                  {isCompiling ? (
                    <div className="spinner small" />
                  ) : (
                    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
                      <path d="M9.4 16.6L4.8 12l4.6-4.6L8 6l-6 6 6 6 1.4-1.4zm5.2 0l4.6-4.6-4.6-4.6L16 6l6 6-6 6-1.4-1.4z"/>
                    </svg>
                  )}
                  <span className="btn-label">Compile</span>
                  {compileStatus === 'success' && (
                    <svg className="status-icon" viewBox="0 0 24 24" fill="currentColor" width="14" height="14">
                      <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z"/>
                    </svg>
                  )}
                  {compileStatus === 'error' && (
                    <svg className="status-icon" viewBox="0 0 24 24" fill="currentColor" width="14" height="14">
                      <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
                    </svg>
                  )}
                </button>

                <div className="toolbar-divider" />
              </div>

              <div className="toolbar-center">
                {/* Transport controls */}
                <div className="transport-controls">
                  <button
                    className={`transport-btn stop-btn ${!isPlaying ? 'disabled' : ''}`}
                    onClick={handleStop}
                    disabled={isLoading || !isPlaying}
                    title="Stop (Ctrl+.)"
                  >
                    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
                      <rect x="6" y="6" width="12" height="12" rx="1"/>
                    </svg>
                  </button>
                  <button
                    className={`transport-btn play-btn ${isPlaying ? 'playing' : ''}`}
                    onClick={isPlaying ? handleStop : handleRun}
                    disabled={isLoading || isRunning}
                    title={isPlaying ? 'Stop (Ctrl+.)' : 'Play (Ctrl+Enter)'}
                  >
                    {isRunning ? (
                      <div className="spinner" />
                    ) : isPlaying ? (
                      <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
                        <rect x="6" y="5" width="4" height="14" rx="1"/>
                        <rect x="14" y="5" width="4" height="14" rx="1"/>
                      </svg>
                    ) : (
                      <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
                        <path d="M8 5v14l11-7z"/>
                      </svg>
                    )}
                  </button>
                  <button
                    className={`transport-btn reload-btn ${!isPlaying ? 'disabled' : ''}`}
                    onClick={handleRun}
                    disabled={isLoading || isRunning || !isPlaying}
                    title="Reload script (Ctrl+Enter)"
                  >
                    {isRunning && isPlaying ? (
                      <div className="spinner small" />
                    ) : (
                      <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
                        <path d="M17.65 6.35A7.958 7.958 0 0012 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08A5.99 5.99 0 0112 18c-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z"/>
                      </svg>
                    )}
                  </button>
                </div>
              </div>

              <div className="toolbar-right">
                {/* Tempo and beat display */}
                <div className={`tempo-display ${isPlaying ? 'active' : ''}`}>
                  <div className="beat-dots">
                    {[0, 1, 2, 3].map(i => (
                      <span key={i} className={`beat-dot ${isPlaying && currentBeat === i ? 'active' : ''}`} />
                    ))}
                  </div>
                  <span className="tempo-value">{stats?.tempo || '--'}</span>
                  <span className="tempo-unit">BPM</span>
                </div>
              </div>
            </div>

            <div className="editor-container">
              <CodeEditor
                value={script}
                onChange={setScript}
                onRun={handleRun}
                onStop={handleStop}
              />
            </div>
          </div>

          <div className="output-panel">
            <div className="output-toolbar">
              <span className="output-label">Console</span>
              <div className="output-actions">
                {stats && (
                  <div className="stats">
                    <span className="stat">{stats.voices} voices</span>
                    <span className="stat-sep">/</span>
                    <span className="stat">{stats.patterns} patterns</span>
                  </div>
                )}
                <button className="clear-btn" onClick={handleClearConsole} title="Clear">
                  <svg viewBox="0 0 24 24" fill="currentColor" width="14" height="14">
                    <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
                  </svg>
                </button>
              </div>
            </div>

            <div className="console">
              {error && (
                <div className="console-error">
                  <span className="error-icon">!</span>
                  <span className="error-message">{error}</span>
                </div>
              )}
              {logs.map((log, i) => (
                <div key={i} className={`console-line console-${log.type}`}>
                  <span className="console-time">{new Date(log.time).toLocaleTimeString()}</span>
                  <span className="console-message">{log.message}</span>
                </div>
              ))}
              {logs.length === 0 && !error && (
                <div className="console-placeholder">Output will appear here...</div>
              )}
            </div>
          </div>
        </div>

        {/* Side panel */}
        {!sidePanelCollapsed ? (
          <div className="side-panel">
            <button
              className="panel-toggle collapse-btn"
              onClick={() => setSidePanelCollapsed(true)}
              title="Collapse"
            >
              <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
                <path d="M15.41 7.41L14 6l-6 6 6 6 1.41-1.41L10.83 12z"/>
              </svg>
            </button>

            {sidePanel === 'stdlib' && (
              <StdlibExplorer
                onInsert={handleInsertCode}
                onPreview={handlePreview}
                isPreviewPlaying={isPreviewPlaying}
                currentPreview={currentPreview}
              />
            )}

            {sidePanel === 'tutorials' && (
              <TutorialPanel
                onLoadTutorial={handleLoadTutorial}
                currentTutorial={currentTutorial}
              />
            )}
          </div>
        ) : (
          <div className="side-panel-collapsed">
            <button
              className="panel-toggle expand-btn"
              onClick={() => setSidePanelCollapsed(false)}
              title="Expand"
            >
              <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
                <path d="M8.59 16.59L10 18l6-6-6-6-1.41 1.41L13.17 12z"/>
              </svg>
            </button>
          </div>
        )}
      </div>

      {/* Footer */}
      <footer className="playground-footer">
        <TipsWidget visible={true} />
        <div className="footer-right">
          <span className="version">VibeLang</span>
          <a href="https://github.com/trusch/vibelang" target="_blank" rel="noopener noreferrer">GitHub</a>
        </div>
      </footer>

      {/* Keyboard shortcuts modal */}
      <KeyboardShortcuts isOpen={showShortcuts} onClose={() => setShowShortcuts(false)} />
    </div>
  );
}

export default Playground;
