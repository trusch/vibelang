import { useState, useCallback, useMemo } from 'react';
import {
  functions,
  methods,
  keywords,
  getAllSynthdefs,
  getAllUGens,
  fuzzyMatch
} from '../data/autocompleteData';

// Context types
const CONTEXT = {
  TOP_LEVEL: 'top_level',
  METHOD: 'method',
  SYNTHDEF_NAME: 'synthdef_name',
  UGEN: 'ugen',
  KEYWORD: 'keyword'
};

export function useAutocomplete(value, cursorPosition, textareaRef, metrics = null) {
  const [showAutocomplete, setShowAutocomplete] = useState(false);
  const [autocompleteItems, setAutocompleteItems] = useState([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [autocompletePosition, setAutocompletePosition] = useState({ top: 0, left: 0 });
  const [currentContext, setCurrentContext] = useState(null);
  const [partialMatch, setPartialMatch] = useState('');

  // Get all synthdefs and ugens (cached)
  const synthdefs = useMemo(() => getAllSynthdefs(), []);
  const ugens = useMemo(() => getAllUGens(), []);

  // Check if cursor is inside a string
  const isInsideString = useCallback((code, pos) => {
    let inString = false;
    let stringChar = null;

    for (let i = 0; i < pos; i++) {
      const char = code[i];
      const prevChar = i > 0 ? code[i - 1] : '';

      // Skip escaped quotes
      if (prevChar === '\\') continue;

      if (!inString && (char === '"' || char === "'")) {
        inString = true;
        stringChar = char;
      } else if (inString && char === stringChar) {
        inString = false;
        stringChar = null;
      }
    }

    return inString;
  }, []);

  // Determine context from code
  const determineContext = useCallback((code, pos) => {
    const beforeCursor = code.slice(0, pos);
    const lines = beforeCursor.split('\n');
    const currentLine = lines[lines.length - 1];

    // Special case: Inside string after .synth(" or .on(" - allow synthdef completion
    const synthMatch = currentLine.match(/\.(synth|on)\s*\(\s*"([^"]*)$/);
    if (synthMatch) {
      return {
        type: CONTEXT.SYNTHDEF_NAME,
        partial: synthMatch[2],
        replaceStart: pos - synthMatch[2].length,
        needsQuote: false
      };
    }

    // If inside any other string, don't autocomplete
    if (isInsideString(code, pos)) {
      return null;
    }

    // After a dot (method completion)
    const methodMatch = currentLine.match(/(\w+)\.(\w*)$/);
    if (methodMatch) {
      return {
        type: CONTEXT.METHOD,
        partial: methodMatch[2],
        replaceStart: pos - methodMatch[2].length,
        objectName: methodMatch[1]
      };
    }

    // Inside .body() closure - check for UGens
    if (isInsideBodyClosure(beforeCursor)) {
      const identMatch = currentLine.match(/(\w+)$/);
      if (identMatch) {
        return {
          type: CONTEXT.UGEN,
          partial: identMatch[1],
          replaceStart: pos - identMatch[1].length
        };
      }
    }

    // Top-level identifier
    const topMatch = currentLine.match(/(\w+)$/);
    if (topMatch) {
      return {
        type: CONTEXT.TOP_LEVEL,
        partial: topMatch[1],
        replaceStart: pos - topMatch[1].length
      };
    }

    return null;
  }, [isInsideString]);

  // Check if we're inside a .body() closure
  function isInsideBodyClosure(code) {
    // Simple heuristic: count .body(| opens vs } closes
    let depth = 0;
    let inBody = false;

    for (let i = 0; i < code.length; i++) {
      if (code.slice(i, i + 6) === '.body(') {
        inBody = true;
      }
      if (inBody) {
        if (code[i] === '{') depth++;
        if (code[i] === '}') depth--;
        if (depth < 0) {
          inBody = false;
          depth = 0;
        }
      }
    }

    return inBody && depth > 0;
  }

  // Get filtered items based on context
  const getFilteredItems = useCallback((context) => {
    if (!context) return [];

    let items = [];
    const { type, partial } = context;

    switch (type) {
      case CONTEXT.SYNTHDEF_NAME:
        items = synthdefs.map(s => ({
          name: s.name,
          type: 'synthdef',
          description: s.description,
          detail: s.category,
          insertText: s.name
        }));
        break;

      case CONTEXT.METHOD:
        items = methods.map(m => ({
          name: m.name,
          type: 'method',
          signature: m.signature,
          description: m.description,
          insertText: m.insertText
        }));
        break;

      case CONTEXT.UGEN:
        // Include UGens, functions, and keywords in body context
        items = [
          ...ugens.map(u => ({
            name: u.name,
            type: 'ugen',
            signature: u.signature,
            description: u.description,
            insertText: u.name + '(${1})'
          })),
          ...functions.filter(f => f.category === 'dsp' || f.category === 'math').map(f => ({
            name: f.name,
            type: 'function',
            signature: f.signature,
            description: f.description,
            insertText: f.insertText
          }))
        ];
        break;

      case CONTEXT.TOP_LEVEL:
      default:
        items = [
          ...functions.map(f => ({
            name: f.name,
            type: 'function',
            signature: f.signature,
            description: f.description,
            insertText: f.insertText
          })),
          ...keywords.map(k => ({
            name: k,
            type: 'keyword',
            description: 'Rhai keyword',
            insertText: k
          }))
        ];
        break;
    }

    // Filter and sort by match score
    if (partial && partial.length > 0) {
      items = items
        .map(item => ({ ...item, score: fuzzyMatch(item.name, partial) }))
        .filter(item => item.score > 0)
        .sort((a, b) => b.score - a.score);
    }

    // Limit results
    return items.slice(0, 15);
  }, [synthdefs, ugens]);

  // Calculate popup position using dynamic metrics
  const calculatePosition = useCallback((pos) => {
    const textarea = textareaRef?.current;
    if (!textarea) return { top: 0, left: 0 };

    const lines = value.slice(0, pos).split('\n');
    const lineNumber = lines.length;
    const columnNumber = lines[lines.length - 1].length;

    // Use dynamic metrics if available, fallback to reasonable defaults
    const lineHeight = metrics?.lineHeight || 21;
    const charWidth = metrics?.charWidth || 8.4;
    const paddingTop = metrics?.paddingTop || 8;
    const paddingLeft = metrics?.paddingLeft || 12;
    const lineNumbersWidth = metrics?.lineNumbersWidth || 50;

    // Account for scroll
    const scrollTop = textarea.scrollTop;
    const scrollLeft = textarea.scrollLeft;

    return {
      top: (lineNumber * lineHeight) + paddingTop - scrollTop + lineHeight,
      left: (columnNumber * charWidth) + paddingLeft - scrollLeft + lineNumbersWidth
    };
  }, [value, textareaRef, metrics]);

  // Trigger autocomplete
  const triggerAutocomplete = useCallback((code, pos, force = false) => {
    const context = determineContext(code, pos);

    if (!context && !force) {
      setShowAutocomplete(false);
      return;
    }

    // Require minimum characters based on context (less aggressive)
    if (!force && context) {
      const minChars = context.type === CONTEXT.METHOD ? 1 : 2;
      if ((context.partial?.length || 0) < minChars) {
        setShowAutocomplete(false);
        return;
      }
    }

    const items = getFilteredItems(context);

    if (items.length === 0) {
      setShowAutocomplete(false);
      return;
    }

    setCurrentContext(context);
    setPartialMatch(context?.partial || '');
    setAutocompleteItems(items);
    setSelectedIndex(0);
    setAutocompletePosition(calculatePosition(pos));
    setShowAutocomplete(true);
  }, [determineContext, getFilteredItems, calculatePosition]);

  // Handle selection
  const handleAutocompleteSelect = useCallback((item, code, pos) => {
    if (!currentContext) return null;

    const { replaceStart } = currentContext;
    let insertText = item.insertText || item.name;

    // For synthdefs in .synth("..."), just insert the name
    if (currentContext.type === CONTEXT.SYNTHDEF_NAME) {
      insertText = item.name;
    }

    // Replace partial with full text
    const before = code.slice(0, replaceStart);
    const after = code.slice(pos);
    const newCode = before + insertText + after;

    // Find cursor position (look for ${1} placeholder or end of insert)
    let cursorPos = replaceStart + insertText.length;
    const placeholderMatch = insertText.match(/\$\{1[^}]*\}/);
    if (placeholderMatch) {
      cursorPos = replaceStart + placeholderMatch.index;
      // Remove placeholder markers
      insertText = insertText.replace(/\$\{\d+[^}]*\}/g, '');
    }

    setShowAutocomplete(false);

    return {
      text: before + insertText + after,
      cursor: replaceStart + insertText.length
    };
  }, [currentContext]);

  // Navigation
  const navigateAutocomplete = useCallback((delta) => {
    setSelectedIndex(prev => {
      const newIndex = prev + delta;
      if (newIndex < 0) return autocompleteItems.length - 1;
      if (newIndex >= autocompleteItems.length) return 0;
      return newIndex;
    });
  }, [autocompleteItems.length]);

  // Close
  const closeAutocomplete = useCallback(() => {
    setShowAutocomplete(false);
  }, []);

  // Get hover info for a word
  const getHoverInfo = useCallback((word, context = {}) => {
    // Check functions
    const func = functions.find(f => f.name === word);
    if (func) {
      return {
        type: 'function',
        name: func.name,
        signature: func.signature,
        description: func.description,
        category: func.category
      };
    }

    // Check methods (if after dot)
    if (context.afterDot) {
      const method = methods.find(m => m.name === word);
      if (method) {
        return {
          type: 'method',
          name: method.name,
          signature: method.signature,
          description: method.description,
          context: method.context
        };
      }
    }

    // Check UGens
    const ugen = ugens.find(u => u.name === word);
    if (ugen) {
      return {
        type: 'ugen',
        name: ugen.name,
        ugenName: ugen.ugenName,
        signature: ugen.signature,
        description: ugen.description,
        category: ugen.category
      };
    }

    // Check synthdefs
    const synthdef = synthdefs.find(s => s.name === word);
    if (synthdef) {
      return {
        type: 'synthdef',
        name: synthdef.name,
        description: synthdef.description,
        category: synthdef.category,
        genre: synthdef.genre,
        params: synthdef.params
      };
    }

    // Check keywords
    if (keywords.includes(word)) {
      return {
        type: 'keyword',
        name: word,
        description: 'Rhai language keyword'
      };
    }

    return null;
  }, [synthdefs, ugens]);

  return {
    showAutocomplete,
    autocompleteItems,
    selectedIndex,
    autocompletePosition,
    triggerAutocomplete,
    handleAutocompleteSelect,
    closeAutocomplete,
    navigateAutocomplete,
    getHoverInfo
  };
}
