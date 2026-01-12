import React from 'react';
import './KeyboardShortcuts.css';

const shortcuts = [
  {
    category: 'Playback',
    items: [
      { keys: ['Ctrl', 'B'], description: 'Compile (check syntax)' },
      { keys: ['Ctrl', 'Enter'], description: 'Run / Reload script' },
      { keys: ['Ctrl', '.'], description: 'Stop playback' },
    ]
  },
  {
    category: 'Editor',
    items: [
      { keys: ['Ctrl', 'Z'], description: 'Undo' },
      { keys: ['Ctrl', 'Y'], description: 'Redo' },
      { keys: ['Ctrl', 'Space'], description: 'Open autocomplete' },
      { keys: ['Tab'], description: 'Indent selected lines / Accept suggestion' },
      { keys: ['Shift', 'Tab'], description: 'Unindent selected lines' },
      { keys: ['Esc'], description: 'Close autocomplete / tooltips' },
    ]
  },
  {
    category: 'Navigation',
    items: [
      { keys: ['Ctrl', 'G'], description: 'Go to line' },
      { keys: ['Ctrl', 'F'], description: 'Find in editor' },
      { keys: ['↑', '↓'], description: 'Navigate autocomplete' },
      { keys: ['Enter'], description: 'Select autocomplete item' },
    ]
  },
  {
    category: 'General',
    items: [
      { keys: ['Ctrl', 'S'], description: 'Save to browser storage' },
      { keys: ['Ctrl', '?'], description: 'Show this dialog' },
    ]
  }
];

function KeyboardShortcuts({ isOpen, onClose }) {
  if (!isOpen) return null;

  // Handle backdrop click
  const handleBackdropClick = (e) => {
    if (e.target === e.currentTarget) {
      onClose();
    }
  };

  // Handle escape key
  React.useEffect(() => {
    const handleKeyDown = (e) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown);
    }

    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [isOpen, onClose]);

  return (
    <div className="shortcuts-backdrop" onClick={handleBackdropClick}>
      <div className="shortcuts-modal">
        <div className="shortcuts-header">
          <h2 className="shortcuts-title">Keyboard Shortcuts</h2>
          <button className="shortcuts-close" onClick={onClose} aria-label="Close">
            x
          </button>
        </div>

        <div className="shortcuts-content">
          {shortcuts.map(category => (
            <div key={category.category} className="shortcuts-category">
              <h3 className="shortcuts-category-title">{category.category}</h3>
              <div className="shortcuts-list">
                {category.items.map((item, index) => (
                  <div key={index} className="shortcuts-item">
                    <div className="shortcuts-keys">
                      {item.keys.map((key, keyIndex) => (
                        <React.Fragment key={keyIndex}>
                          <kbd className="shortcuts-key">{key}</kbd>
                          {keyIndex < item.keys.length - 1 && (
                            <span className="shortcuts-plus">+</span>
                          )}
                        </React.Fragment>
                      ))}
                    </div>
                    <span className="shortcuts-desc">{item.description}</span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>

        <div className="shortcuts-footer">
          <span className="shortcuts-hint">Press Esc to close</span>
        </div>
      </div>
    </div>
  );
}

export default KeyboardShortcuts;
