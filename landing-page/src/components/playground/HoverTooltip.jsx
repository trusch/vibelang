import React from 'react';
import './HoverTooltip.css';

function HoverTooltip({ info, position }) {
  if (!info) return null;

  // Determine display color based on type
  const typeColors = {
    function: '#89b4fa',
    method: '#f9e2af',
    synthdef: '#a6e3a1',
    ugen: '#f5c2e7',
    keyword: '#cba6f7'
  };

  const color = typeColors[info.type] || '#cdd6f4';

  // Calculate position to avoid going off screen
  const tooltipStyle = {
    position: 'fixed',
    left: Math.min(position.x + 10, window.innerWidth - 320),
    top: position.y + 20
  };

  // If tooltip would go below viewport, show above cursor
  if (position.y + 200 > window.innerHeight) {
    tooltipStyle.top = position.y - 120;
  }

  return (
    <div className="hover-tooltip" style={tooltipStyle}>
      <div className="hover-header">
        <span className="hover-type" style={{ color }}>
          {info.type}
        </span>
        <span className="hover-name">{info.name}</span>
      </div>

      {info.signature && (
        <code className="hover-signature">{info.signature}</code>
      )}

      {info.description && (
        <p className="hover-description">{info.description}</p>
      )}

      {/* Synthdef-specific info */}
      {info.type === 'synthdef' && info.params && info.params.length > 0 && (
        <div className="hover-params">
          <span className="hover-params-label">Parameters:</span>
          <ul>
            {info.params.slice(0, 5).map(p => (
              <li key={p.name}>
                <span className="param-name">{p.name}</span>
                <span className="param-default">= {p.default}</span>
              </li>
            ))}
            {info.params.length > 5 && (
              <li className="param-more">+{info.params.length - 5} more</li>
            )}
          </ul>
        </div>
      )}

      {/* Category info */}
      {info.category && (
        <span className="hover-category">{info.category}</span>
      )}

      {/* Genre for synthdefs */}
      {info.genre && (
        <span className="hover-genre">{info.genre}</span>
      )}

      {/* Context for methods */}
      {info.context && Array.isArray(info.context) && (
        <span className="hover-context">
          Available on: {info.context.join(', ')}
        </span>
      )}
    </div>
  );
}

export default HoverTooltip;
