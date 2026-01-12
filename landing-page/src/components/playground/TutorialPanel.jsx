import React, { useState, useMemo, useCallback } from 'react';
import { tutorialCategories, getTutorial, getTutorialsByCategory } from '../../data/tutorials';
import './TutorialPanel.css';

function TutorialPanel({ onLoadTutorial, currentTutorial }) {
  const [expandedCategories, setExpandedCategories] = useState(new Set(['Getting Started']));
  const [selectedTutorial, setSelectedTutorial] = useState(null);

  // Toggle category expansion
  const toggleCategory = useCallback((categoryName) => {
    setExpandedCategories(prev => {
      const next = new Set(prev);
      if (next.has(categoryName)) {
        next.delete(categoryName);
      } else {
        next.add(categoryName);
      }
      return next;
    });
  }, []);

  // Handle tutorial selection
  const handleSelect = useCallback((tutorialId) => {
    const tutorial = getTutorial(tutorialId);
    setSelectedTutorial(tutorial);
  }, []);

  // Handle load button
  const handleLoad = useCallback(() => {
    if (selectedTutorial && onLoadTutorial) {
      onLoadTutorial(selectedTutorial);
    }
  }, [selectedTutorial, onLoadTutorial]);

  // Get difficulty color
  const getDifficultyColor = (difficulty) => {
    switch (difficulty) {
      case 'beginner': return 'var(--difficulty-beginner, #a6e3a1)';
      case 'intermediate': return 'var(--difficulty-intermediate, #f9e2af)';
      case 'advanced': return 'var(--difficulty-advanced, #fab387)';
      case 'expert': return 'var(--difficulty-expert, #f38ba8)';
      default: return 'var(--muted-color, #6c7086)';
    }
  };

  return (
    <div className="tutorial-panel">
      {/* Header */}
      <div className="tutorial-header">
        <h2 className="tutorial-title">Tutorials</h2>
        <p className="tutorial-subtitle">Learn VibeLang step by step</p>
      </div>

      {/* Category tree */}
      <div className="tutorial-tree">
        {tutorialCategories.map(category => {
          const isExpanded = expandedCategories.has(category.name);
          const tutorials = getTutorialsByCategory(category.name);

          return (
            <div key={category.name} className="tutorial-category">
              <div
                className={`tutorial-category-header ${isExpanded ? 'expanded' : ''}`}
                onClick={() => toggleCategory(category.name)}
              >
                <span className="tutorial-chevron">{isExpanded ? '>' : '>'}</span>
                <span className="tutorial-category-name">{category.name}</span>
                <span className="tutorial-category-count">{tutorials.length}</span>
              </div>

              {isExpanded && (
                <div className="tutorial-list">
                  {tutorials.map(tutorial => (
                    <div
                      key={tutorial.id}
                      className={`tutorial-item ${selectedTutorial?.id === tutorial.id ? 'selected' : ''} ${currentTutorial === tutorial.id ? 'loaded' : ''}`}
                      onClick={() => handleSelect(tutorial.id)}
                    >
                      <div className="tutorial-item-main">
                        <span className="tutorial-item-title">{tutorial.title}</span>
                        {currentTutorial === tutorial.id && (
                          <span className="tutorial-item-loaded">Active</span>
                        )}
                      </div>
                      <div className="tutorial-item-meta">
                        <span
                          className="tutorial-difficulty"
                          style={{ color: getDifficultyColor(tutorial.difficulty) }}
                        >
                          {tutorial.difficulty}
                        </span>
                        <span className="tutorial-duration">{tutorial.duration}</span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* Detail panel */}
      {selectedTutorial && (
        <div className="tutorial-detail">
          <div className="tutorial-detail-header">
            <h3 className="tutorial-detail-title">{selectedTutorial.title}</h3>
            <span
              className="tutorial-detail-difficulty"
              style={{ backgroundColor: getDifficultyColor(selectedTutorial.difficulty) + '33', color: getDifficultyColor(selectedTutorial.difficulty) }}
            >
              {selectedTutorial.difficulty}
            </span>
          </div>

          <p className="tutorial-detail-desc">{selectedTutorial.description}</p>

          <div className="tutorial-detail-meta">
            <span className="tutorial-detail-duration">
              <span className="meta-icon">~</span> {selectedTutorial.duration}
            </span>
          </div>

          <button
            className="tutorial-load-btn"
            onClick={handleLoad}
            disabled={currentTutorial === selectedTutorial.id}
          >
            {currentTutorial === selectedTutorial.id ? 'Currently Loaded' : 'Load Tutorial'}
          </button>
        </div>
      )}
    </div>
  );
}

export default TutorialPanel;
