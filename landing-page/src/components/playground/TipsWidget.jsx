import React, { useState, useEffect, useCallback } from 'react';
import { getRandomTip, getNextTip } from '../../data/tips';
import './TipsWidget.css';

const ROTATION_INTERVAL = 45000; // 45 seconds

function TipsWidget({ visible = true }) {
  const [currentTip, setCurrentTip] = useState(() => getRandomTip());
  const [isAnimating, setIsAnimating] = useState(false);

  // Rotate tips automatically
  useEffect(() => {
    if (!visible) return;

    const interval = setInterval(() => {
      rotateTip();
    }, ROTATION_INTERVAL);

    return () => clearInterval(interval);
  }, [visible, currentTip]);

  // Get next tip with animation
  const rotateTip = useCallback(() => {
    setIsAnimating(true);

    setTimeout(() => {
      setCurrentTip(prev => getNextTip(prev));
      setIsAnimating(false);
    }, 200);
  }, []);

  if (!visible || !currentTip) return null;

  // Get icon based on tip type
  const getIcon = () => {
    switch (currentTip.icon) {
      case 'bulb': return '💡';
      case 'info': return 'ℹ';
      case 'star': return '✨';
      case 'keyboard': return '⌨';
      default: return '💡';
    }
  };

  // Get label based on tip type
  const getLabel = () => {
    switch (currentTip.type) {
      case 'tip': return 'Pro Tip';
      case 'fact': return 'Did you know?';
      case 'joke': return 'Fun fact';
      case 'shortcut': return 'Keyboard shortcut';
      default: return 'Tip';
    }
  };

  return (
    <div className={`tips-widget ${isAnimating ? 'animating' : ''}`}>
      <div className="tips-icon">{getIcon()}</div>
      <div className="tips-content">
        <span className="tips-label">{getLabel()}</span>
        <span className="tips-text">{currentTip.text}</span>
      </div>
      <button
        className="tips-next"
        onClick={rotateTip}
        title="Next tip"
        aria-label="Next tip"
      >
        <span className="tips-next-icon">→</span>
      </button>
    </div>
  );
}

export default TipsWidget;
