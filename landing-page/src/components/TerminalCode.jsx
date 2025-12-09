import React from 'react';
import { highlightCode } from '../utils/syntaxHighlight';
import './TerminalCode.css';

function TerminalCode({ code, filename = 'code.vibe', className = '' }) {
  return (
    <div className={`terminal ${className}`}>
      <div className="terminal__header">
        <span className="terminal__dot terminal__dot--red"></span>
        <span className="terminal__dot terminal__dot--yellow"></span>
        <span className="terminal__dot terminal__dot--green"></span>
        <span className="terminal__filename">{filename}</span>
      </div>
      <pre className="terminal__code">
        <code>{highlightCode(code)}</code>
      </pre>
    </div>
  );
}

export default TerminalCode;
