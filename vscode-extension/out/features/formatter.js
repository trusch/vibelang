"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VibelangOnTypeFormattingEditProvider = exports.VibelangDocumentRangeFormattingEditProvider = exports.VibelangDocumentFormattingEditProvider = void 0;
const vscode = require("vscode");
const dataLoader_1 = require("../utils/dataLoader");
/**
 * VibeLang Document Formatter
 *
 * Formatting rules:
 * - 4 spaces for indentation (configurable)
 * - Consistent spacing around operators
 * - Long method chains broken into multiple lines
 * - Proper spacing after keywords
 * - Consistent spacing in function calls
 * - Pattern strings formatted with spaces every N steps (configurable)
 * - Auto-import for missing stdlib synthdefs (configurable)
 * - Import sorting (configurable)
 */
const MAX_LINE_LENGTH = 100;
const CHAIN_BREAK_THRESHOLD = 80; // Break chains if line exceeds this
const MULTILINE_STRING_THRESHOLD = 60; // Convert pattern/melody strings to multiline if longer
function getFormatterConfig() {
    const config = vscode.workspace.getConfiguration('vibelang.format');
    return {
        patternSpacing: config.get('patternSpacing', true),
        patternGroupSize: config.get('patternGroupSize', 4),
        autoImport: config.get('autoImport', true),
        sortImports: config.get('sortImports', true),
    };
}
class VibelangDocumentFormattingEditProvider {
    constructor(extensionPath) {
        this.extensionPath = extensionPath;
    }
    provideDocumentFormattingEdits(document, options, token) {
        const edits = [];
        const fullText = document.getText();
        const config = getFormatterConfig();
        const formatted = this.formatDocument(fullText, options, config);
        if (formatted !== fullText) {
            const fullRange = new vscode.Range(document.positionAt(0), document.positionAt(fullText.length));
            edits.push(vscode.TextEdit.replace(fullRange, formatted));
        }
        return edits;
    }
    formatDocument(text, options, config) {
        const indentStr = options.insertSpaces ? ' '.repeat(options.tabSize) : '\t';
        // Step 0: Protect multiline backtick strings from line-by-line processing
        const multilineStrings = [];
        let protectedText = text.replace(/`[^`]*`/gs, (match) => {
            // Only protect if it actually contains newlines
            if (match.includes('\n')) {
                multilineStrings.push(match);
                return `__MULTILINE_STRING_${multilineStrings.length - 1}__`;
            }
            return match;
        });
        let lines = protectedText.split('\n');
        // Step 1: Auto-import missing synthdefs
        if (config.autoImport) {
            lines = this.addMissingImports(lines);
        }
        // Step 2: Sort imports
        if (config.sortImports) {
            lines = this.sortImports(lines);
        }
        // Step 3: Format each line
        const formattedLines = [];
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
            let formattedContent = this.formatLineContent(trimmedLine, config);
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
        // Restore multiline strings
        for (let i = 0; i < multilineStrings.length; i++) {
            result = result.replace(`__MULTILINE_STRING_${i}__`, multilineStrings[i]);
        }
        // Ensure file ends with newline
        if (!result.endsWith('\n')) {
            result += '\n';
        }
        return result;
    }
    /**
     * Add missing imports for synthdefs used in the document
     */
    addMissingImports(lines) {
        const importMap = dataLoader_1.DataLoader.getImportMap(this.extensionPath);
        const existingImports = new Set();
        const usedSynthdefs = new Set();
        const definedSynthdefs = new Set();
        // Parse existing imports
        for (const line of lines) {
            const importMatch = line.match(/^\s*import\s+"([^"]+)"\s*;?\s*$/);
            if (importMatch) {
                existingImports.add(importMatch[1]);
            }
            // Track synthdefs defined in this file
            const synthdefMatch = line.match(/define_synthdef\s*\(\s*"([^"]+)"/);
            if (synthdefMatch) {
                definedSynthdefs.add(synthdefMatch[1]);
            }
        }
        // Find used synthdefs
        const fullText = lines.join('\n');
        // Match .synth("name") patterns
        const synthMatches = fullText.matchAll(/\.synth\s*\(\s*"([^"]+)"\s*\)/g);
        for (const match of synthMatches) {
            usedSynthdefs.add(match[1]);
        }
        // Match voice("name").synth("name") - the synth name is what matters
        // Also match effect references in .effect("name")
        const effectMatches = fullText.matchAll(/\.effect\s*\(\s*"([^"]+)"\s*\)/g);
        for (const match of effectMatches) {
            usedSynthdefs.add(match[1]);
        }
        // Find missing imports
        const missingImports = [];
        for (const synthdef of usedSynthdefs) {
            // Skip if it's defined in this file
            if (definedSynthdefs.has(synthdef)) {
                continue;
            }
            const importPath = importMap.get(synthdef);
            if (importPath && !existingImports.has(importPath)) {
                missingImports.push(importPath);
                existingImports.add(importPath); // Prevent duplicates
            }
        }
        // Add missing imports at the top (after any existing imports)
        if (missingImports.length > 0) {
            // Find the last import line index
            let lastImportIndex = -1;
            for (let i = 0; i < lines.length; i++) {
                if (lines[i].trim().startsWith('import ')) {
                    lastImportIndex = i;
                }
            }
            // Sort missing imports
            missingImports.sort();
            // Insert missing imports
            const importStatements = missingImports.map(p => `import "${p}";`);
            if (lastImportIndex >= 0) {
                // Insert after the last existing import
                lines.splice(lastImportIndex + 1, 0, ...importStatements);
            }
            else {
                // No existing imports, add at the beginning
                // Find first non-empty, non-comment line
                let insertIndex = 0;
                for (let i = 0; i < lines.length; i++) {
                    const trimmed = lines[i].trim();
                    if (trimmed === '' || trimmed.startsWith('//')) {
                        insertIndex = i + 1;
                    }
                    else {
                        break;
                    }
                }
                lines.splice(insertIndex, 0, ...importStatements, '');
            }
        }
        return lines;
    }
    /**
     * Sort import statements alphabetically
     */
    sortImports(lines) {
        const importLines = [];
        const nonImportLines = [];
        // Separate imports from non-imports, preserving original indices
        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];
            const importMatch = line.match(/^\s*import\s+"([^"]+)"\s*;?\s*$/);
            if (importMatch) {
                importLines.push({ index: i, line: line.trim(), path: importMatch[1] });
            }
            else {
                nonImportLines.push({ index: i, line });
            }
        }
        // If no imports or only one import, nothing to sort
        if (importLines.length <= 1) {
            return lines;
        }
        // Sort imports by path
        importLines.sort((a, b) => a.path.localeCompare(b.path));
        // Find the range of consecutive imports at the start of the file
        // (after any leading comments/empty lines)
        let importStartIndex = -1;
        let importEndIndex = -1;
        let inImportBlock = false;
        for (let i = 0; i < lines.length; i++) {
            const trimmed = lines[i].trim();
            if (trimmed.startsWith('import ')) {
                if (!inImportBlock) {
                    importStartIndex = i;
                    inImportBlock = true;
                }
                importEndIndex = i;
            }
            else if (inImportBlock && trimmed !== '' && !trimmed.startsWith('//')) {
                // Non-import, non-empty, non-comment line ends the import block
                break;
            }
        }
        // Rebuild the file with sorted imports
        if (importStartIndex >= 0) {
            const result = [];
            // Add lines before imports
            for (let i = 0; i < importStartIndex; i++) {
                result.push(lines[i]);
            }
            // Add sorted imports (normalize format)
            for (const imp of importLines) {
                // Ensure consistent format: import "path";
                result.push(`import "${imp.path}";`);
            }
            // Add lines after imports
            for (let i = importEndIndex + 1; i < lines.length; i++) {
                result.push(lines[i]);
            }
            return result;
        }
        return lines;
    }
    countBraceBalance(line) {
        // Protect strings from counting (both double-quoted and backtick)
        let withoutStrings = line.replace(/"([^"\\]|\\.)*"/g, '""');
        withoutStrings = withoutStrings.replace(/`[^`]*`/g, '``');
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
        // Protect strings from splitting (both double-quoted and backtick)
        const stringPlaceholders = [];
        let protectedLine = line.replace(/"([^"\\]|\\.)*"/g, (match) => {
            stringPlaceholders.push(match);
            return `__STR${stringPlaceholders.length - 1}__`;
        });
        protectedLine = protectedLine.replace(/`[^`]*`/g, (match) => {
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
                // Check that it's not a decimal number (digit before AND digit after)
                const charBefore = i > 0 ? protectedLine[i - 1] : '';
                const charAfter = i < protectedLine.length - 1 ? protectedLine[i + 1] : '';
                const isDecimalNumber = /\d/.test(charBefore) && /\d/.test(charAfter);
                if (!isDecimalNumber && i > 0 && /\w|\)/.test(charBefore)) {
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
    formatLineContent(line, config) {
        // Don't format empty lines or comments
        if (line === '' || line.startsWith('//')) {
            return line;
        }
        let result = line;
        // Protect strings from formatting, but track pattern/melody strings
        const stringPlaceholders = [];
        // Regex patterns for both double-quoted and backtick strings
        const doubleQuotePattern = /"([^"\\]|\\.)*"/g;
        const backtickPattern = /`[^`]*`/g;
        // Detect if we're in a pattern context - look for .step( before the string
        result = result.replace(/\.step\s*\(\s*("([^"\\]|\\.)*"|`[^`]*`)/g, (match) => {
            // Extract the string part (either double-quoted or backtick)
            const stringMatch = match.match(/("([^"\\]|\\.)*"|`[^`]*`)/);
            if (stringMatch) {
                const quoteChar = stringMatch[0][0];
                stringPlaceholders.push({ value: stringMatch[0], isPattern: true, isMelody: false, quoteChar });
                return match.replace(stringMatch[0], `__STRING_${stringPlaceholders.length - 1}__`);
            }
            return match;
        });
        // Detect melody context - look for .notes( before the string
        result = result.replace(/\.notes\s*\(\s*("([^"\\]|\\.)*"|`[^`]*`)/g, (match) => {
            const stringMatch = match.match(/("([^"\\]|\\.)*"|`[^`]*`)/);
            if (stringMatch) {
                const quoteChar = stringMatch[0][0];
                stringPlaceholders.push({ value: stringMatch[0], isPattern: false, isMelody: true, quoteChar });
                return match.replace(stringMatch[0], `__STRING_${stringPlaceholders.length - 1}__`);
            }
            return match;
        });
        // Protect remaining backtick strings first (they can span multiple lines conceptually)
        result = result.replace(backtickPattern, (match) => {
            stringPlaceholders.push({ value: match, isPattern: false, isMelody: false, quoteChar: '`' });
            return `__STRING_${stringPlaceholders.length - 1}__`;
        });
        // Protect remaining double-quoted strings
        result = result.replace(doubleQuotePattern, (match) => {
            stringPlaceholders.push({ value: match, isPattern: false, isMelody: false, quoteChar: '"' });
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
        // Restore strings, formatting patterns and melodies if enabled
        for (let i = 0; i < stringPlaceholders.length; i++) {
            const placeholder = stringPlaceholders[i];
            let stringValue = placeholder.value;
            if (placeholder.isPattern && config.patternSpacing) {
                stringValue = this.formatPatternString(stringValue, config.patternGroupSize);
            }
            else if (placeholder.isMelody && config.patternSpacing) {
                stringValue = this.formatMelodyString(stringValue, config.patternGroupSize);
            }
            result = result.replace(`__STRING_${i}__`, stringValue);
        }
        return result.trim();
    }
    /**
     * Format a pattern string by adding spaces every N characters
     * Example: "x...x...x...x..." -> "x... x... x... x..."
     * Handles | as bar separator: "x...x...|x...x..." -> "x... x... | x... x..."
     * Supports both double-quoted and backtick strings
     */
    formatPatternString(str, groupSize) {
        // Remove quotes to work with content
        const quote = str[0];
        const content = str.slice(1, -1);
        // Skip if empty
        if (content.length === 0) {
            return str;
        }
        // Split by bar separator | first, format each bar, then join back
        const bars = content.split('|');
        const formattedBars = bars.map(bar => {
            // Remove existing whitespace (normalize) - includes newlines
            const normalized = bar.replace(/\s+/g, '');
            // If the bar is very short, don't add spaces
            if (normalized.length <= groupSize) {
                return normalized;
            }
            // Split into groups and join with spaces
            const groups = [];
            for (let i = 0; i < normalized.length; i += groupSize) {
                groups.push(normalized.slice(i, i + groupSize));
            }
            return groups.join(' ');
        });
        // Join bars with " | " (space around the bar separator)
        return `${quote}${formattedBars.join(' | ')}${quote}`;
    }
    /**
     * Format a melody string by adding spaces between notes and grouping by bar
     * Example: "C4 D4 E4 - | G4 - - -" remains readable
     * For long melodies, preserves bar structure with proper spacing
     * Supports both double-quoted and backtick strings
     */
    formatMelodyString(str, groupSize) {
        // Remove quotes to work with content
        const quote = str[0];
        const content = str.slice(1, -1);
        // Skip if empty
        if (content.length === 0) {
            return str;
        }
        // Split by bar separator | first
        const bars = content.split('|');
        const formattedBars = bars.map(bar => {
            // Normalize whitespace - replace multiple spaces/newlines with single space
            const normalized = bar.replace(/\s+/g, ' ').trim();
            return normalized;
        });
        // Join bars with " | " (space around the bar separator)
        return `${quote}${formattedBars.join(' | ')}${quote}`;
    }
}
exports.VibelangDocumentFormattingEditProvider = VibelangDocumentFormattingEditProvider;
/**
 * Range formatter - formats only selected text
 */
class VibelangDocumentRangeFormattingEditProvider {
    constructor(extensionPath) {
        this.formatter = new VibelangDocumentFormattingEditProvider(extensionPath);
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