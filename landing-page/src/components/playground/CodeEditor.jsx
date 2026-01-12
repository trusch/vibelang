import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { highlightCode } from '../../utils/syntaxHighlight';
import Autocomplete from './Autocomplete';
import HoverTooltip from './HoverTooltip';
import { useAutocomplete } from '../../hooks/useAutocomplete';
import { useEditorMetrics, pixelToTextPosition } from '../../hooks/useEditorMetrics';
import './CodeEditor.css';

// Bracket pairs for matching
const BRACKETS = { '(': ')', '[': ']', '{': '}' };

function CodeEditor({
  value,
  onChange,
  onRun,
  onStop,
  error,
  readOnly = false
}) {
  const textareaRef = useRef(null);
  const highlightRef = useRef(null);
  const lineNumbersRef = useRef(null);
  const editorContainerRef = useRef(null);

  // Get dynamically measured font metrics (fixes cursor alignment issues)
  const metrics = useEditorMetrics(editorContainerRef);

  const [cursorPosition, setCursorPosition] = useState(0);
  const [cursorLine, setCursorLine] = useState(1);
  const [cursorColumn, setCursorColumn] = useState(1);
  const [selection, setSelection] = useState({ start: 0, end: 0 });
  const [hoverInfo, setHoverInfo] = useState(null);
  const [hoverPosition, setHoverPosition] = useState({ x: 0, y: 0 });

  // Undo/redo stack
  const undoStackRef = useRef([]);
  const redoStackRef = useRef([]);
  const lastValueRef = useRef(value);

  // Helper to push to undo stack and apply change
  const applyChange = useCallback((newValue, newCursor = null) => {
    // Push current state to undo stack
    undoStackRef.current.push({
      value: lastValueRef.current,
      cursor: textareaRef.current?.selectionStart || 0
    });
    // Clear redo stack on new change
    redoStackRef.current = [];
    // Limit undo stack size
    if (undoStackRef.current.length > 100) {
      undoStackRef.current.shift();
    }
    lastValueRef.current = newValue;
    onChange(newValue);

    if (newCursor !== null) {
      setTimeout(() => {
        if (textareaRef.current) {
          textareaRef.current.selectionStart = newCursor;
          textareaRef.current.selectionEnd = newCursor;
        }
      }, 0);
    }
  }, [onChange]);

  // Undo function
  const undo = useCallback(() => {
    if (undoStackRef.current.length === 0) return;

    const current = {
      value: lastValueRef.current,
      cursor: textareaRef.current?.selectionStart || 0
    };
    redoStackRef.current.push(current);

    const prev = undoStackRef.current.pop();
    lastValueRef.current = prev.value;
    onChange(prev.value);

    setTimeout(() => {
      if (textareaRef.current) {
        textareaRef.current.selectionStart = prev.cursor;
        textareaRef.current.selectionEnd = prev.cursor;
      }
    }, 0);
  }, [onChange]);

  // Redo function
  const redo = useCallback(() => {
    if (redoStackRef.current.length === 0) return;

    const current = {
      value: lastValueRef.current,
      cursor: textareaRef.current?.selectionStart || 0
    };
    undoStackRef.current.push(current);

    const next = redoStackRef.current.pop();
    lastValueRef.current = next.value;
    onChange(next.value);

    setTimeout(() => {
      if (textareaRef.current) {
        textareaRef.current.selectionStart = next.cursor;
        textareaRef.current.selectionEnd = next.cursor;
      }
    }, 0);
  }, [onChange]);

  // Autocomplete hook
  const {
    showAutocomplete,
    autocompleteItems,
    selectedIndex,
    autocompletePosition,
    triggerAutocomplete,
    handleAutocompleteSelect,
    closeAutocomplete,
    navigateAutocomplete,
    getHoverInfo
  } = useAutocomplete(value, cursorPosition, textareaRef, metrics);

  // Sync lastValueRef when value changes from external sources (like loading a tutorial)
  useEffect(() => {
    // Only update if value changed externally (not from our own edits)
    if (value !== lastValueRef.current) {
      // Clear undo/redo stacks when loading new content externally
      undoStackRef.current = [];
      redoStackRef.current = [];
      lastValueRef.current = value;
    }
  }, [value]);

  // Calculate line count
  const lineCount = useMemo(() => value.split('\n').length, [value]);

  // Update cursor position info
  const updateCursorInfo = useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    const pos = textarea.selectionStart;
    setCursorPosition(pos);
    setSelection({ start: textarea.selectionStart, end: textarea.selectionEnd });

    // Calculate line and column
    const textBefore = value.substring(0, pos);
    const lines = textBefore.split('\n');
    setCursorLine(lines.length);
    setCursorColumn(lines[lines.length - 1].length + 1);
  }, [value]);

  // Sync scroll between all elements
  const syncScroll = useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    if (highlightRef.current) {
      highlightRef.current.scrollTop = textarea.scrollTop;
      highlightRef.current.scrollLeft = textarea.scrollLeft;
    }
    if (lineNumbersRef.current) {
      lineNumbersRef.current.scrollTop = textarea.scrollTop;
    }
  }, []);

  // Handle text change
  const handleChange = useCallback((e) => {
    const newValue = e.target.value;

    // Push to undo stack
    undoStackRef.current.push({
      value: lastValueRef.current,
      cursor: textareaRef.current?.selectionStart || 0
    });
    redoStackRef.current = [];
    if (undoStackRef.current.length > 100) {
      undoStackRef.current.shift();
    }
    lastValueRef.current = newValue;
    onChange(newValue);

    // Trigger autocomplete after change
    setTimeout(() => {
      triggerAutocomplete(newValue, e.target.selectionStart);
    }, 0);
  }, [onChange, triggerAutocomplete]);

  // Handle key down
  const handleKeyDown = useCallback((e) => {
    // Autocomplete navigation
    if (showAutocomplete) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        navigateAutocomplete(1);
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        navigateAutocomplete(-1);
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        const item = autocompleteItems[selectedIndex];
        if (item) {
          const newValue = handleAutocompleteSelect(item, value, cursorPosition);
          if (newValue !== null) {
            onChange(newValue.text);
            setTimeout(() => {
              if (textareaRef.current) {
                textareaRef.current.selectionStart = newValue.cursor;
                textareaRef.current.selectionEnd = newValue.cursor;
                updateCursorInfo();
              }
            }, 0);
          }
        }
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        closeAutocomplete();
        return;
      }
    }

    // Run script: Ctrl/Cmd + Enter
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      onRun?.();
      return;
    }

    // Stop: Ctrl/Cmd + .
    if ((e.ctrlKey || e.metaKey) && e.key === '.') {
      e.preventDefault();
      onStop?.();
      return;
    }

    // Force autocomplete: Ctrl + Space
    if (e.ctrlKey && e.key === ' ') {
      e.preventDefault();
      triggerAutocomplete(value, cursorPosition, true);
      return;
    }

    // Undo: Ctrl+Z
    if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
      e.preventDefault();
      undo();
      return;
    }

    // Redo: Ctrl+Y or Ctrl+Shift+Z
    if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || (e.key === 'z' && e.shiftKey))) {
      e.preventDefault();
      redo();
      return;
    }

    // Tab: indent selected lines or insert 4 spaces
    if (e.key === 'Tab' && !e.shiftKey) {
      e.preventDefault();
      const textarea = textareaRef.current;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;

      // Check if multiple lines are selected
      const selectedText = value.substring(start, end);
      if (selectedText.includes('\n')) {
        // Multi-line: indent each line
        const lineStart = value.lastIndexOf('\n', start - 1) + 1;
        const lineEnd = value.indexOf('\n', end - 1);
        const actualLineEnd = lineEnd === -1 ? value.length : lineEnd;

        const linesSection = value.substring(lineStart, actualLineEnd);
        const indentedLines = linesSection.split('\n').map(line => '    ' + line).join('\n');
        const newValue = value.substring(0, lineStart) + indentedLines + value.substring(actualLineEnd);

        applyChange(newValue);
        setTimeout(() => {
          // Adjust selection to cover all indented lines
          textarea.selectionStart = lineStart;
          textarea.selectionEnd = lineStart + indentedLines.length;
          updateCursorInfo();
        }, 0);
      } else {
        // Single cursor or single-line selection: insert 4 spaces
        const newValue = value.substring(0, start) + '    ' + value.substring(end);
        applyChange(newValue, start + 4);
        setTimeout(() => updateCursorInfo(), 0);
      }
      return;
    }

    // Shift+Tab: unindent selected lines
    if (e.key === 'Tab' && e.shiftKey) {
      e.preventDefault();
      const textarea = textareaRef.current;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;

      // Find the lines to unindent
      const lineStart = value.lastIndexOf('\n', start - 1) + 1;
      const lineEnd = value.indexOf('\n', end - 1);
      const actualLineEnd = lineEnd === -1 ? value.length : lineEnd;

      const linesSection = value.substring(lineStart, actualLineEnd);
      let totalRemoved = 0;
      let firstLineRemoved = 0;

      const unindentedLines = linesSection.split('\n').map((line, index) => {
        // Remove up to 4 leading spaces or 1 tab
        let removed = 0;
        if (line.startsWith('    ')) {
          removed = 4;
        } else if (line.startsWith('\t')) {
          removed = 1;
        } else {
          // Remove as many leading spaces as possible (up to 4)
          const match = line.match(/^( {1,4})/);
          if (match) {
            removed = match[1].length;
          }
        }
        if (index === 0) firstLineRemoved = removed;
        totalRemoved += removed;
        return line.substring(removed);
      }).join('\n');

      if (totalRemoved > 0) {
        const newValue = value.substring(0, lineStart) + unindentedLines + value.substring(actualLineEnd);
        applyChange(newValue);
        setTimeout(() => {
          // Adjust selection
          const newStart = Math.max(lineStart, start - firstLineRemoved);
          textarea.selectionStart = newStart;
          textarea.selectionEnd = lineStart + unindentedLines.length;
          updateCursorInfo();
        }, 0);
      }
      return;
    }

    // Toggle comment: Ctrl/Cmd + /
    if ((e.ctrlKey || e.metaKey) && e.key === '/') {
      e.preventDefault();
      const textarea = textareaRef.current;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const lineStart = value.lastIndexOf('\n', start - 1) + 1;
      const lineEnd = value.indexOf('\n', end);
      const actualLineEnd = lineEnd === -1 ? value.length : lineEnd;
      const line = value.substring(lineStart, actualLineEnd);

      let newLine;
      if (line.trimStart().startsWith('//')) {
        // Uncomment
        newLine = line.replace(/^(\s*)\/\/\s?/, '$1');
      } else {
        // Comment
        newLine = line.replace(/^(\s*)/, '$1// ');
      }

      const newValue = value.substring(0, lineStart) + newLine + value.substring(actualLineEnd);
      applyChange(newValue);
      return;
    }

    // Auto-close brackets
    if (BRACKETS[e.key]) {
      const textarea = textareaRef.current;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;

      if (start !== end) {
        // Wrap selection
        e.preventDefault();
        const selected = value.substring(start, end);
        const newValue = value.substring(0, start) + e.key + selected + BRACKETS[e.key] + value.substring(end);
        applyChange(newValue);
        setTimeout(() => {
          textarea.selectionStart = start + 1;
          textarea.selectionEnd = end + 1;
        }, 0);
        return;
      }
    }

    // Auto-close quotes
    if (e.key === '"' || e.key === "'") {
      const textarea = textareaRef.current;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const nextChar = value[end];

      if (nextChar === e.key) {
        // Skip over existing quote
        e.preventDefault();
        textarea.selectionStart = textarea.selectionEnd = end + 1;
        updateCursorInfo();
        return;
      }

      if (start !== end) {
        // Wrap selection
        e.preventDefault();
        const selected = value.substring(start, end);
        const newValue = value.substring(0, start) + e.key + selected + e.key + value.substring(end);
        applyChange(newValue);
        setTimeout(() => {
          textarea.selectionStart = start + 1;
          textarea.selectionEnd = end + 1;
        }, 0);
        return;
      }
    }
  }, [value, onChange, onRun, onStop, showAutocomplete, autocompleteItems, selectedIndex,
      cursorPosition, triggerAutocomplete, closeAutocomplete, navigateAutocomplete,
      handleAutocompleteSelect, updateCursorInfo, undo, redo, applyChange]);

  // Handle mouse move for hover tooltips
  const handleMouseMove = useCallback((e) => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    const rect = textarea.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    const scrollTop = textarea.scrollTop;
    const scrollLeft = textarea.scrollLeft;

    // Use dynamically measured metrics instead of hardcoded values
    const { line, column: col } = pixelToTextPosition(x, y, scrollTop, scrollLeft, metrics);

    const lines = value.split('\n');
    if (line < 0 || line >= lines.length) {
      setHoverInfo(null);
      return;
    }

    const lineText = lines[line];
    if (col < 0 || col >= lineText.length) {
      setHoverInfo(null);
      return;
    }

    // Find word at position
    let wordStart = col;
    let wordEnd = col;
    while (wordStart > 0 && /[a-zA-Z0-9_]/.test(lineText[wordStart - 1])) wordStart--;
    while (wordEnd < lineText.length && /[a-zA-Z0-9_]/.test(lineText[wordEnd])) wordEnd++;

    const word = lineText.substring(wordStart, wordEnd);
    if (word.length < 2) {
      setHoverInfo(null);
      return;
    }

    // Check for method context (preceded by dot)
    const isMethod = wordStart > 0 && lineText[wordStart - 1] === '.';

    const info = getHoverInfo(word, { afterDot: isMethod });
    if (info) {
      setHoverInfo(info);
      setHoverPosition({ x: e.clientX, y: e.clientY });
    } else {
      setHoverInfo(null);
    }
  }, [value, getHoverInfo, metrics]);

  const handleMouseLeave = useCallback(() => {
    setHoverInfo(null);
  }, []);

  // Update cursor info on selection change
  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    const handleSelect = () => updateCursorInfo();
    textarea.addEventListener('select', handleSelect);
    textarea.addEventListener('click', handleSelect);
    textarea.addEventListener('keyup', handleSelect);

    return () => {
      textarea.removeEventListener('select', handleSelect);
      textarea.removeEventListener('click', handleSelect);
      textarea.removeEventListener('keyup', handleSelect);
    };
  }, [updateCursorInfo]);

  // Highlighted code
  const highlightedCode = useMemo(() => highlightCode(value), [value]);

  // Parse error line number if available
  const errorLine = useMemo(() => {
    if (!error) return null;
    const match = error.match(/line\s+(\d+)/i);
    return match ? parseInt(match[1], 10) : null;
  }, [error]);

  return (
    <div className="code-editor-wrapper">
      <div className="code-editor-main">
        {/* Line numbers */}
        <div className="code-editor-line-numbers" ref={lineNumbersRef}>
          {Array.from({ length: lineCount }, (_, i) => (
            <div
              key={i + 1}
              className={`line-number ${errorLine === i + 1 ? 'error' : ''} ${cursorLine === i + 1 ? 'active' : ''}`}
            >
              {i + 1}
            </div>
          ))}
        </div>

        {/* Editor container */}
        <div className="code-editor-container" ref={editorContainerRef}>
          {/* Syntax highlighted background - using div instead of pre/code for better alignment */}
          <div ref={highlightRef} className="code-editor-layer code-editor-highlight" aria-hidden="true">
            {highlightedCode}
          </div>

          {/* Editable textarea */}
          <textarea
            ref={textareaRef}
            className="code-editor-layer code-editor-textarea"
            value={value}
            onChange={handleChange}
            onKeyDown={handleKeyDown}
            onScroll={syncScroll}
            onMouseMove={handleMouseMove}
            onMouseLeave={handleMouseLeave}
            spellCheck={false}
            autoCapitalize="off"
            autoComplete="off"
            autoCorrect="off"
            readOnly={readOnly}
            placeholder="Write your VibeLang code here..."
          />

          {/* Autocomplete popup */}
          {showAutocomplete && autocompleteItems.length > 0 && (
            <Autocomplete
              items={autocompleteItems}
              selectedIndex={selectedIndex}
              position={autocompletePosition}
              onSelect={(item) => {
                const newValue = handleAutocompleteSelect(item, value, cursorPosition);
                if (newValue !== null) {
                  onChange(newValue.text);
                  setTimeout(() => {
                    if (textareaRef.current) {
                      textareaRef.current.selectionStart = newValue.cursor;
                      textareaRef.current.selectionEnd = newValue.cursor;
                      textareaRef.current.focus();
                      updateCursorInfo();
                    }
                  }, 0);
                }
              }}
              onClose={closeAutocomplete}
            />
          )}

          {/* Hover tooltip */}
          {hoverInfo && (
            <HoverTooltip
              info={hoverInfo}
              position={hoverPosition}
            />
          )}
        </div>
      </div>

      {/* Status bar */}
      <div className="code-editor-status-bar">
        <span className="status-item">
          Ln {cursorLine}, Col {cursorColumn}
        </span>
        {selection.start !== selection.end && (
          <span className="status-item">
            ({selection.end - selection.start} selected)
          </span>
        )}
        <span className="status-item status-hint">
          Ctrl+Enter: Run | Ctrl+Space: Autocomplete
        </span>
      </div>
    </div>
  );
}

export default CodeEditor;
