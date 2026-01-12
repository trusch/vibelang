import { useState, useEffect, useCallback, useRef } from 'react';

/**
 * Hook to measure actual font metrics for the code editor.
 *
 * This solves the cursor/click alignment problem by measuring real rendered
 * character widths instead of using hard-coded estimates that vary by:
 * - Browser font rendering engine
 * - System DPI and scaling
 * - Font availability (fallback fonts have different metrics)
 * - Responsive breakpoints that change font-size
 */
export function useEditorMetrics(containerRef) {
  const [metrics, setMetrics] = useState({
    charWidth: 8.4,      // Default fallback
    lineHeight: 21,      // Default fallback
    paddingTop: 8,       // Default fallback
    paddingLeft: 12,     // Default fallback
    lineNumbersWidth: 50 // Default fallback
  });

  const measureRef = useRef(null);
  const resizeObserverRef = useRef(null);

  const measureMetrics = useCallback(() => {
    const container = containerRef?.current;
    if (!container) return;

    // Find the textarea or highlight layer to get computed styles
    const editorLayer = container.querySelector('.code-editor-layer');
    const lineNumbers = container.closest('.code-editor-main')?.querySelector('.code-editor-line-numbers');

    if (!editorLayer) return;

    const computedStyle = window.getComputedStyle(editorLayer);

    // Get actual computed values from CSS
    const fontSize = parseFloat(computedStyle.fontSize) || 14;
    const fontFamily = computedStyle.fontFamily;
    const lineHeight = parseFloat(computedStyle.lineHeight) || 21;
    const paddingTop = parseFloat(computedStyle.paddingTop) || 8;
    const paddingLeft = parseFloat(computedStyle.paddingLeft) || 12;

    // Measure line numbers width
    let lineNumbersWidth = 50;
    if (lineNumbers) {
      lineNumbersWidth = lineNumbers.getBoundingClientRect().width;
    }

    // Measure actual character width using canvas (most accurate method)
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    ctx.font = `${fontSize}px ${fontFamily}`;

    // Measure a representative set of characters for monospace font
    // Using multiple characters and averaging handles any sub-pixel variations
    const testString = 'MMMMMMMMMM';  // M is typically the widest
    const charWidth = ctx.measureText(testString).width / testString.length;

    setMetrics({
      charWidth,
      lineHeight,
      paddingTop,
      paddingLeft,
      lineNumbersWidth
    });
  }, [containerRef]);

  // Measure on mount and when window resizes (responsive breakpoints)
  useEffect(() => {
    measureMetrics();

    // Re-measure on resize (handles responsive breakpoints)
    const handleResize = () => {
      // Debounce slightly to avoid excessive measurements
      requestAnimationFrame(measureMetrics);
    };

    window.addEventListener('resize', handleResize);

    // Also observe the container for size changes
    const container = containerRef?.current;
    if (container && typeof ResizeObserver !== 'undefined') {
      resizeObserverRef.current = new ResizeObserver(handleResize);
      resizeObserverRef.current.observe(container);
    }

    return () => {
      window.removeEventListener('resize', handleResize);
      if (resizeObserverRef.current) {
        resizeObserverRef.current.disconnect();
      }
    };
  }, [measureMetrics, containerRef]);

  // Re-measure when fonts load (font-face loading can change metrics)
  useEffect(() => {
    if (typeof document.fonts !== 'undefined') {
      document.fonts.ready.then(measureMetrics);
    }
  }, [measureMetrics]);

  return metrics;
}

/**
 * Utility to calculate text position from pixel coordinates.
 *
 * @param {number} x - X coordinate relative to editor container
 * @param {number} y - Y coordinate relative to editor container
 * @param {number} scrollTop - Current scroll top of textarea
 * @param {number} scrollLeft - Current scroll left of textarea
 * @param {object} metrics - The metrics from useEditorMetrics
 * @returns {{ line: number, column: number }} 0-indexed line and column
 */
export function pixelToTextPosition(x, y, scrollTop, scrollLeft, metrics) {
  const { charWidth, lineHeight, paddingTop, paddingLeft } = metrics;

  // Account for padding and scroll
  const adjustedY = y + scrollTop - paddingTop;
  const adjustedX = x + scrollLeft - paddingLeft;

  // Calculate line and column (0-indexed)
  const line = Math.floor(adjustedY / lineHeight);
  const column = Math.round(adjustedX / charWidth); // round instead of floor for better click accuracy

  return {
    line: Math.max(0, line),
    column: Math.max(0, column)
  };
}

/**
 * Utility to calculate pixel position from text position.
 *
 * @param {number} line - 0-indexed line number
 * @param {number} column - 0-indexed column number
 * @param {number} scrollTop - Current scroll top of textarea
 * @param {number} scrollLeft - Current scroll left of textarea
 * @param {object} metrics - The metrics from useEditorMetrics
 * @returns {{ top: number, left: number }} Pixel position relative to container
 */
export function textPositionToPixel(line, column, scrollTop, scrollLeft, metrics) {
  const { charWidth, lineHeight, paddingTop, paddingLeft, lineNumbersWidth } = metrics;

  return {
    top: (line * lineHeight) + paddingTop - scrollTop + lineHeight, // Position below the line
    left: (column * charWidth) + paddingLeft - scrollLeft + lineNumbersWidth
  };
}
