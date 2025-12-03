"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VibelangOnTypeFormattingEditProvider = exports.VibelangDocumentRangeFormattingEditProvider = exports.VibelangDocumentFormattingEditProvider = void 0;
const vscode = require("vscode");
/**
 * VibeLang Document Formatter
 *
 * Formatting rules:
 * - 4 spaces for indentation
 * - Consistent spacing around operators
 * - Long method chains broken into multiple lines
 * - Proper spacing after keywords
 * - Consistent spacing in function calls
 */
const MAX_LINE_LENGTH = 100;
const CHAIN_BREAK_THRESHOLD = 80; // Break chains if line exceeds this
class VibelangDocumentFormattingEditProvider {
    provideDocumentFormattingEdits(document, options, token) {
        const edits = [];
        const fullText = document.getText();
        const formatted = this.formatDocument(fullText, options);
        if (formatted !== fullText) {
            const fullRange = new vscode.Range(document.positionAt(0), document.positionAt(fullText.length));
            edits.push(vscode.TextEdit.replace(fullRange, formatted));
        }
        return edits;
    }
    formatDocument(text, options) {
        const lines = text.split('\n');
        const formattedLines = [];
        const indentStr = options.insertSpaces ? ' '.repeat(options.tabSize) : '\t';
        let braceStack = 0;
        for (let i = 0; i < lines.length; i++) {
            let line = lines[i];
            // Skip empty lines (preserve them as-is)
            if (line.trim() === '') {
                formattedLines.push('');
                continue;
            }
            const trimmedLine = line.trim();
            // Handle comments - preserve them exactly but fix indentation
            if (trimmedLine.startsWith('//')) {
                const currentIndent = this.calculateIndent(braceStack, indentStr);
                formattedLines.push(currentIndent + trimmedLine);
                continue;
            }
            // Format the line content first
            let formattedContent = this.formatLineContent(trimmedLine);
            // Calculate base indentation from brace stack
            // If line starts with closing brace, reduce indent first
            let baseIndent = braceStack;
            if (formattedContent.startsWith('}')) {
                baseIndent = Math.max(0, baseIndent - 1);
            }
            // Check if this is a continuation line (starts with .)
            // Continuation lines get an extra indent level
            const isContinuation = formattedContent.startsWith('.');
            if (isContinuation) {
                baseIndent += 1;
            }
            // Break long method chains into multiple lines
            const expandedLines = this.breakMethodChains(formattedContent, baseIndent, indentStr, isContinuation);
            // Add all expanded lines
            for (const expandedLine of expandedLines) {
                formattedLines.push(expandedLine);
            }
            // Update brace stack based on the original formatted content
            braceStack += this.countBraceBalance(formattedContent);
            braceStack = Math.max(0, braceStack);
        }
        // Join and clean up
        let result = formattedLines.join('\n');
        // Ensure file ends with newline
        if (!result.endsWith('\n')) {
            result += '\n';
        }
        return result;
    }
    countBraceBalance(line) {
        // Protect strings from counting
        const withoutStrings = line.replace(/"([^"\\]|\\.)*"/g, '""');
        let balance = 0;
        // Only count curly braces for indentation, not parentheses or brackets
        // Parentheses are for function calls/expressions, not block structures
        for (const char of withoutStrings) {
            if (char === '{') {
                balance++;
            }
            else if (char === '}') {
                balance--;
            }
        }
        return balance;
    }
    breakMethodChains(line, baseIndent, indentStr, isContinuation) {
        const currentIndentStr = this.calculateIndent(baseIndent, indentStr);
        const fullLine = currentIndentStr + line;
        // Don't break short lines
        if (fullLine.length <= CHAIN_BREAK_THRESHOLD) {
            return [fullLine];
        }
        // Protect strings from splitting
        const stringPlaceholders = [];
        let protectedLine = line.replace(/"([^"\\]|\\.)*"/g, (match) => {
            stringPlaceholders.push(match);
            return `__STR${stringPlaceholders.length - 1}__`;
        });
        // Find method chain points (.) that aren't inside parentheses
        const chainPoints = [];
        let parenDepth = 0;
        let bracketDepth = 0;
        for (let i = 0; i < protectedLine.length; i++) {
            const char = protectedLine[i];
            if (char === '(' || char === '[') {
                parenDepth++;
            }
            else if (char === ')' || char === ']') {
                parenDepth--;
            }
            else if (char === '.' && parenDepth === 0 && bracketDepth === 0) {
                // Found a method chain point at top level
                if (i > 0 && /\w|\)/.test(protectedLine[i - 1])) {
                    chainPoints.push(i);
                }
            }
        }
        // If no chain points or only one method call, don't break
        if (chainPoints.length < 2) {
            return [fullLine];
        }
        // Split at chain points
        const segments = [];
        let lastPos = 0;
        for (const pos of chainPoints) {
            if (lastPos < pos) {
                segments.push(protectedLine.substring(lastPos, pos));
            }
            lastPos = pos;
        }
        if (lastPos < protectedLine.length) {
            segments.push(protectedLine.substring(lastPos));
        }
        // Restore strings in segments
        const restoredSegments = segments.map(seg => {
            let result = seg;
            for (let i = 0; i < stringPlaceholders.length; i++) {
                result = result.replace(`__STR${i}__`, stringPlaceholders[i]);
            }
            return result;
        });
        // Build result lines
        const result = [];
        const chainIndent = this.calculateIndent(baseIndent + 1, indentStr);
        for (let i = 0; i < restoredSegments.length; i++) {
            const segment = restoredSegments[i].trim();
            if (i === 0) {
                // First segment gets base indent
                result.push(currentIndentStr + segment);
            }
            else {
                // Continuation segments get extra indent
                result.push(chainIndent + segment);
            }
        }
        return result;
    }
    calculateIndent(level, indentStr) {
        return indentStr.repeat(level);
    }
    formatLineContent(line) {
        // Don't format empty lines or comments
        if (line === '' || line.startsWith('//')) {
            return line;
        }
        let result = line;
        // Protect strings from formatting
        const stringPlaceholders = [];
        result = result.replace(/"([^"\\]|\\.)*"/g, (match) => {
            stringPlaceholders.push(match);
            return `__STRING_${stringPlaceholders.length - 1}__`;
        });
        // Handle range operator (..) - no spaces
        result = result.replace(/\s*\.\.\s*/g, '..');
        // Handle comparison and logical operators
        result = result.replace(/\s*(==|!=|<=|>=|&&|\|\|)\s*/g, ' $1 ');
        // Handle assignment (but not ==, !=, etc.)
        result = result.replace(/([^=!<>])=([^=])/g, '$1 = $2');
        // Handle comparison operators < and > (but not <= or >=)
        result = result.replace(/([^<>=])<([^<=])/g, '$1 < $2');
        result = result.replace(/([^<>=])>([^>=])/g, '$1 > $2');
        // Handle arithmetic operators (but be careful with negative numbers)
        result = result.replace(/(\w|\))\s*\+\s*/g, '$1 + ');
        result = result.replace(/(\w|\))\s*-\s*(\w)/g, '$1 - $2');
        result = result.replace(/(\w|\))\s*\*\s*/g, '$1 * ');
        result = result.replace(/(\w|\))\s*\/\s*/g, '$1 / ');
        // Handle commas - space after, not before
        result = result.replace(/\s*,\s*/g, ', ');
        // Handle colons in named params - space after only
        // Match pattern: word followed by colon (named parameter)
        result = result.replace(/(\w+)\s*:\s*/g, '$1: ');
        // Handle parentheses - no space inside
        result = result.replace(/\(\s+/g, '(');
        result = result.replace(/\s+\)/g, ')');
        // Handle brackets - no space inside
        result = result.replace(/\[\s+/g, '[');
        result = result.replace(/\s+\]/g, ']');
        // Handle braces - space inside for closures
        result = result.replace(/\{\s*\|/g, '{ |');
        result = result.replace(/\|\s*\{/g, '| {');
        // Handle closure pipes - consistent spacing
        // Match ||{ pattern for empty closure params
        result = result.replace(/\|\|\s*\{/g, '|| {');
        // Match |params| pattern
        result = result.replace(/\|\s*([^|]+)\s*\|/g, '|$1|');
        // Handle semicolons - no space before
        result = result.replace(/\s+;/g, ';');
        // Handle method chaining dots - no space around
        result = result.replace(/\s*\.\s*(\w)/g, '.$1');
        // Keywords should have space after
        const keywords = ['let', 'if', 'else', 'for', 'while', 'fn', 'return', 'import'];
        for (const kw of keywords) {
            const regex = new RegExp(`\\b${kw}\\s+`, 'g');
            result = result.replace(regex, `${kw} `);
        }
        // Fix multiple spaces (but not at start of line)
        result = result.replace(/([^\s])  +/g, '$1 ');
        // Restore strings
        for (let i = 0; i < stringPlaceholders.length; i++) {
            result = result.replace(`__STRING_${i}__`, stringPlaceholders[i]);
        }
        return result.trim();
    }
}
exports.VibelangDocumentFormattingEditProvider = VibelangDocumentFormattingEditProvider;
/**
 * Range formatter - formats only selected text
 */
class VibelangDocumentRangeFormattingEditProvider {
    constructor() {
        this.formatter = new VibelangDocumentFormattingEditProvider();
    }
    provideDocumentRangeFormattingEdits(document, range, options, token) {
        // For simplicity, we'll format the whole document
        // A more sophisticated implementation would only format the range
        return this.formatter.provideDocumentFormattingEdits(document, options, token);
    }
}
exports.VibelangDocumentRangeFormattingEditProvider = VibelangDocumentRangeFormattingEditProvider;
/**
 * On-type formatter - formats as you type
 */
class VibelangOnTypeFormattingEditProvider {
    provideOnTypeFormattingEdits(document, position, ch, options, token) {
        const edits = [];
        const line = document.lineAt(position.line);
        const lineText = line.text;
        const indentStr = options.insertSpaces ? ' '.repeat(options.tabSize) : '\t';
        // Auto-indent after opening brace
        if (ch === '\n') {
            const prevLine = position.line > 0 ? document.lineAt(position.line - 1).text : '';
            const trimmedPrev = prevLine.trim();
            // If previous line ends with { or ||{, add indent
            if (trimmedPrev.endsWith('{') || trimmedPrev.endsWith('|| {')) {
                const currentIndent = this.getIndentLevel(prevLine, indentStr);
                const newIndent = indentStr.repeat(currentIndent + 1);
                // If current line is empty, set proper indent
                if (lineText.trim() === '') {
                    edits.push(vscode.TextEdit.replace(new vscode.Range(position.line, 0, position.line, lineText.length), newIndent));
                }
            }
        }
        // Auto-dedent on closing brace
        if (ch === '}') {
            const currentIndent = this.getIndentLevel(lineText, indentStr);
            if (currentIndent > 0) {
                const newIndent = indentStr.repeat(currentIndent - 1);
                const beforeBrace = lineText.substring(0, lineText.lastIndexOf('}'));
                if (beforeBrace.trim() === '') {
                    edits.push(vscode.TextEdit.replace(new vscode.Range(position.line, 0, position.line, beforeBrace.length), newIndent));
                }
            }
        }
        return edits;
    }
    getIndentLevel(line, indentStr) {
        let count = 0;
        let i = 0;
        while (i < line.length) {
            if (line.startsWith(indentStr, i)) {
                count++;
                i += indentStr.length;
            }
            else if (line[i] === ' ' || line[i] === '\t') {
                i++;
            }
            else {
                break;
            }
        }
        return count;
    }
}
exports.VibelangOnTypeFormattingEditProvider = VibelangOnTypeFormattingEditProvider;
//# sourceMappingURL=formatter.js.map