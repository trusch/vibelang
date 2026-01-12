import React, { useEffect, useRef } from 'react';
import './Autocomplete.css';

// Icons for different item types
const TypeIcons = {
  function: 'fn',
  method: '.m',
  synthdef: 'S',
  ugen: 'U',
  keyword: 'kw',
  sample: 'Sa'
};

const TypeColors = {
  function: 'var(--ac-function, #89b4fa)',
  method: 'var(--ac-method, #f9e2af)',
  synthdef: 'var(--ac-synthdef, #a6e3a1)',
  ugen: 'var(--ac-ugen, #f5c2e7)',
  keyword: 'var(--ac-keyword, #cba6f7)',
  sample: 'var(--ac-sample, #fab387)'
};

function Autocomplete({ items, selectedIndex, position, onSelect, onClose }) {
  const listRef = useRef(null);
  const selectedRef = useRef(null);

  // Scroll selected item into view
  useEffect(() => {
    if (selectedRef.current && listRef.current) {
      const list = listRef.current;
      const item = selectedRef.current;
      const listRect = list.getBoundingClientRect();
      const itemRect = item.getBoundingClientRect();

      if (itemRect.bottom > listRect.bottom) {
        item.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      } else if (itemRect.top < listRect.top) {
        item.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      }
    }
  }, [selectedIndex]);

  // Handle click outside
  useEffect(() => {
    const handleClickOutside = (e) => {
      if (listRef.current && !listRef.current.contains(e.target)) {
        onClose();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [onClose]);

  if (items.length === 0) return null;

  return (
    <div
      className="autocomplete-popup"
      style={{
        top: position.top,
        left: position.left
      }}
    >
      <ul ref={listRef} className="autocomplete-list">
        {items.map((item, index) => (
          <li
            key={`${item.type}-${item.name}`}
            ref={index === selectedIndex ? selectedRef : null}
            className={`autocomplete-item ${index === selectedIndex ? 'selected' : ''}`}
            onClick={() => onSelect(item)}
            onMouseEnter={() => {}}
          >
            <span
              className="autocomplete-icon"
              style={{ color: TypeColors[item.type] }}
              title={item.type}
            >
              {TypeIcons[item.type] || '?'}
            </span>
            <span className="autocomplete-name">{item.name}</span>
            {item.signature && (
              <span className="autocomplete-signature">{item.signature}</span>
            )}
            {item.detail && (
              <span className="autocomplete-detail">{item.detail}</span>
            )}
          </li>
        ))}
      </ul>

      {/* Detail panel for selected item */}
      {items[selectedIndex] && (
        <div className="autocomplete-detail-panel">
          <div className="detail-header">
            <span
              className="detail-type"
              style={{ color: TypeColors[items[selectedIndex].type] }}
            >
              {items[selectedIndex].type}
            </span>
            <span className="detail-name">{items[selectedIndex].name}</span>
          </div>
          {items[selectedIndex].signature && (
            <code className="detail-signature">{items[selectedIndex].signature}</code>
          )}
          {items[selectedIndex].description && (
            <p className="detail-description">{items[selectedIndex].description}</p>
          )}
        </div>
      )}
    </div>
  );
}

export default Autocomplete;
