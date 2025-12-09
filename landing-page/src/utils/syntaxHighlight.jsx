import React from 'react';

// Simple syntax highlighter for VibeLang code
export function highlightCode(code) {
  const lines = code.split('\n');

  return lines.map((line, lineIndex) => {
    const tokens = tokenizeLine(line);
    return (
      <React.Fragment key={lineIndex}>
        {tokens.map((token, i) => (
          <span key={i} className={token.type ? `hl-${token.type}` : undefined}>
            {token.value}
          </span>
        ))}
        {lineIndex < lines.length - 1 && '\n'}
      </React.Fragment>
    );
  });
}

function tokenizeLine(line) {
  const tokens = [];
  let remaining = line;

  while (remaining.length > 0) {
    let matched = false;

    // Comments
    if (remaining.startsWith('//')) {
      tokens.push({ type: 'comment', value: remaining });
      remaining = '';
      matched = true;
    }

    // Strings
    if (!matched) {
      const stringMatch = remaining.match(/^"[^"]*"/);
      if (stringMatch) {
        tokens.push({ type: 'string', value: stringMatch[0] });
        remaining = remaining.slice(stringMatch[0].length);
        matched = true;
      }
    }

    // Keywords
    if (!matched) {
      const keywordMatch = remaining.match(/^(let|import|fn|if|else|for|while|return|true|false)\b/);
      if (keywordMatch) {
        tokens.push({ type: 'keyword', value: keywordMatch[0] });
        remaining = remaining.slice(keywordMatch[0].length);
        matched = true;
      }
    }

    // Built-in functions
    if (!matched) {
      const builtinMatch = remaining.match(/^(set_tempo|define_group|define_synthdef|voice|pattern|melody|sequence|fx|fade|sample|db|bars|note|env_adsr|saw_ar|pulse_ar|sin_ar|rlpf_ar|NewEnvGenBuilder)\b/);
      if (builtinMatch) {
        tokens.push({ type: 'builtin', value: builtinMatch[0] });
        remaining = remaining.slice(builtinMatch[0].length);
        matched = true;
      }
    }

    // Methods (after dots)
    if (!matched) {
      const methodMatch = remaining.match(/^\.(synth|gain|poly|on|step|notes|start|param|apply|body|over_bars|from|to|on_voice|on_group|loop_bars|clip|gate|euclid|set_param|with_done_action|build)\b/);
      if (methodMatch) {
        tokens.push({ type: 'punctuation', value: '.' });
        tokens.push({ type: 'method', value: methodMatch[0].slice(1) });
        remaining = remaining.slice(methodMatch[0].length);
        matched = true;
      }
    }

    // Numbers
    if (!matched) {
      const numberMatch = remaining.match(/^-?\d+\.?\d*/);
      if (numberMatch) {
        tokens.push({ type: 'number', value: numberMatch[0] });
        remaining = remaining.slice(numberMatch[0].length);
        matched = true;
      }
    }

    // Identifiers
    if (!matched) {
      const identMatch = remaining.match(/^[a-zA-Z_][a-zA-Z0-9_]*/);
      if (identMatch) {
        tokens.push({ type: 'identifier', value: identMatch[0] });
        remaining = remaining.slice(identMatch[0].length);
        matched = true;
      }
    }

    // Operators and punctuation
    if (!matched) {
      const opMatch = remaining.match(/^(\.\.|\|\||&&|==|!=|<=|>=|=>|->|\+|-|\*|\/|=|<|>|\|)/);
      if (opMatch) {
        tokens.push({ type: 'operator', value: opMatch[0] });
        remaining = remaining.slice(opMatch[0].length);
        matched = true;
      }
    }

    // Brackets and parens
    if (!matched) {
      const bracketMatch = remaining.match(/^[(){}\[\]]/);
      if (bracketMatch) {
        tokens.push({ type: 'bracket', value: bracketMatch[0] });
        remaining = remaining.slice(1);
        matched = true;
      }
    }

    // Other punctuation
    if (!matched) {
      const punctMatch = remaining.match(/^[,;:]/);
      if (punctMatch) {
        tokens.push({ type: 'punctuation', value: punctMatch[0] });
        remaining = remaining.slice(1);
        matched = true;
      }
    }

    // Whitespace
    if (!matched) {
      const wsMatch = remaining.match(/^\s+/);
      if (wsMatch) {
        tokens.push({ type: null, value: wsMatch[0] });
        remaining = remaining.slice(wsMatch[0].length);
        matched = true;
      }
    }

    // Fallback: single character
    if (!matched && remaining.length > 0) {
      tokens.push({ type: null, value: remaining[0] });
      remaining = remaining.slice(1);
    }
  }

  return tokens;
}

// Styled code component with syntax highlighting
export function HighlightedCode({ code, className = '' }) {
  return (
    <pre className={`highlighted-code ${className}`}>
      <code>{highlightCode(code)}</code>
    </pre>
  );
}

export default HighlightedCode;
