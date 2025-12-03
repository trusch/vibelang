"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.findParameterAtPosition = findParameterAtPosition;
exports.formatValueLikeExisting = formatValueLikeExisting;
const vscode = require("vscode");
const numberRegex = /-?(?:\d+\.?\d*|\.\d+)/g;
function findParameterAtPosition(document, position) {
    if (position.line >= document.lineCount) {
        return undefined;
    }
    const line = document.lineAt(position.line);
    const commentIdx = line.text.indexOf('//');
    const codeLine = commentIdx !== -1 ? line.text.substring(0, commentIdx) : line.text;
    let match;
    while ((match = numberRegex.exec(codeLine)) !== null) {
        const raw = match[0];
        const start = match.index;
        const end = start + raw.length;
        if (position.character < start || position.character > end) {
            continue;
        }
        const prefix = codeLine.substring(0, start);
        const isDb = /db\s*\(\s*$/.test(prefix);
        const value = parseFloat(raw);
        const name = determineContextName(codeLine, start, value);
        const { min, max, step } = determineRange(name, value, line.text, isDb);
        return {
            name,
            value,
            range: new vscode.Range(new vscode.Position(position.line, start), new vscode.Position(position.line, end)),
            min,
            max,
            step,
            isDb,
            lineText: line.text,
            valueText: raw
        };
    }
    return undefined;
}
function formatValueLikeExisting(originalText, value) {
    let formatted = value.toFixed(2);
    if (!originalText.includes('.') && Number.isInteger(value)) {
        formatted = value.toString();
    }
    else if (!originalText.includes('.') && !Number.isInteger(value)) {
        formatted = Number(value.toFixed(3)).toString();
    }
    else if (originalText.includes('.')) {
        const precision = originalText.split('.')[1]?.length ?? 2;
        formatted = value.toFixed(precision);
    }
    return formatted;
}
function determineContextName(line, valueIndex, value) {
    const prefix = line.substring(0, valueIndex).trimEnd();
    let searchPrefix = prefix;
    if (/db\s*\(\s*$/.test(prefix)) {
        searchPrefix = prefix.replace(/db\s*\(\s*$/, '').trimEnd();
    }
    const letMatch = searchWithRegex(searchPrefix, /let\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*$/);
    if (letMatch)
        return letMatch;
    const assignMatch = searchWithRegex(searchPrefix, /([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*$/);
    if (assignMatch)
        return assignMatch;
    const paramMatch = searchWithRegex(searchPrefix, /param\s*\(\s*"([^"]+)"\s*,\s*$/);
    if (paramMatch)
        return paramMatch;
    const funcMatch = searchWithRegex(searchPrefix, /([a-zA-Z_][a-zA-Z0-9_]*)\s*\(\s*(?:[^)]*,)*\s*$/);
    if (funcMatch)
        return funcMatch;
    const colonMatch = searchWithRegex(searchPrefix, /([a-zA-Z_][a-zA-Z0-9_]*)\s*:\s*$/);
    if (colonMatch)
        return colonMatch;
    if (value > 0 && value < 1) {
        return 'ratio';
    }
    return 'value';
}
function determineRange(name, value, lineText, isDb) {
    const lowerName = name.toLowerCase();
    let min = 0;
    let max = 1;
    let step = 0.01;
    let derivedFromMetadata = false;
    if (isDb) {
        min = -60;
        max = 6;
        step = 0.1;
    }
    else if (lowerName.includes('freq') || lowerName.includes('cut') || lowerName.includes('rate') || (value > 50 && value < 22000)) {
        min = 20;
        max = value > 2000 ? 20000 : 2000;
        step = value > 2000 ? 10 : 1;
    }
    else if (lowerName.includes('amp') || lowerName.includes('gain') || lowerName.includes('mix') || lowerName.includes('prob') || (value >= 0 && value <= 1)) {
        min = 0;
        max = 1;
        step = 0.01;
    }
    else if (value > 100) {
        min = 0;
        max = Math.max(value * 2, 200);
        step = 1;
    }
    else if (value < 0) {
        min = value * 2;
        max = 0;
        step = 0.1;
    }
    else {
        const span = Math.max(Math.abs(value), 1);
        min = value - span;
        max = value + span;
        step = span / 50;
    }
    const rangeMatch = lineText.match(/range:\s*(-?\d+(?:\.\d+)?)\s*\.\.\s*(-?\d+(?:\.\d+)?)/);
    if (rangeMatch) {
        min = parseFloat(rangeMatch[1]);
        max = parseFloat(rangeMatch[2]);
        derivedFromMetadata = true;
        step = Math.max((max - min) / 200, 0.001);
    }
    ({ min, max } = applyValueMixing(min, max, value, derivedFromMetadata));
    step = normalizeStep(step, min, max);
    return { min, max, step };
}
function applyValueMixing(min, max, value, metadata) {
    const span = Math.max(max - min, 0.0001);
    const valueSpan = Math.max(Math.abs(value) * 0.5, span * 0.2, 0.1);
    if (metadata) {
        const padding = Math.max(valueSpan * 0.1, 0.05);
        min = Math.min(min, value - padding);
        max = Math.max(max, value + padding);
    }
    else {
        min = Math.min(min, value - valueSpan);
        max = Math.max(max, value + valueSpan);
    }
    if (min === max) {
        min = value - 1;
        max = value + 1;
    }
    return { min, max };
}
function normalizeStep(step, min, max) {
    const span = Math.abs(max - min);
    if (span <= 0) {
        return 0.1;
    }
    const reasonable = span / 200;
    return Math.max(Math.min(step, reasonable), span / 1000, 0.0001);
}
function searchWithRegex(text, regex) {
    const match = text.match(regex);
    return match ? match[1] : undefined;
}
//# sourceMappingURL=parameterScanner.js.map