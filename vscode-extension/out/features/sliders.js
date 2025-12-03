"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ParameterSlidersPanel = void 0;
const vscode = require("vscode");
class ParameterSlidersPanel {
    static createOrShow(extensionUri) {
        const column = vscode.window.activeTextEditor
            ? vscode.window.activeTextEditor.viewColumn
            : undefined;
        if (ParameterSlidersPanel.currentPanel) {
            ParameterSlidersPanel.currentPanel._panel.reveal(column);
            return;
        }
        const panel = vscode.window.createWebviewPanel(ParameterSlidersPanel.viewType, 'Vibelang Parameters', vscode.ViewColumn.Two, {
            enableScripts: true,
            localResourceRoots: [vscode.Uri.joinPath(extensionUri, 'media')]
        });
        ParameterSlidersPanel.currentPanel = new ParameterSlidersPanel(panel, extensionUri);
    }
    static revive(panel, extensionUri) {
        ParameterSlidersPanel.currentPanel = new ParameterSlidersPanel(panel, extensionUri);
    }
    constructor(panel, extensionUri) {
        this._disposables = [];
        this._isWebviewUpdating = false; // Flag to prevent infinite loops
        this._lastParams = []; // To diff against
        this._panel = panel;
        this._extensionUri = extensionUri;
        this._update();
        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
        this._panel.onDidChangeViewState(e => {
            if (this._panel.visible) {
                this._update();
            }
        }, null, this._disposables);
        this._panel.webview.onDidReceiveMessage(message => {
            switch (message.command) {
                case 'updateParameter':
                    this._isWebviewUpdating = true;
                    this._updateParameterInDocument(message.line, message.startChar, message.endChar, message.value);
                    setTimeout(() => { this._isWebviewUpdating = false; }, 100);
                    return;
            }
        }, null, this._disposables);
        vscode.window.onDidChangeActiveTextEditor(() => this._update(), null, this._disposables);
        vscode.window.onDidChangeTextEditorSelection(e => {
            if (e.textEditor === vscode.window.activeTextEditor) {
                const selection = e.selections[0];
                this._handleSelectionChange(selection.active.line, selection.active.character);
            }
        }, null, this._disposables);
        vscode.workspace.onDidChangeTextDocument(e => {
            if (this._currentDocument && e.document.uri.toString() === this._currentDocument.uri.toString()) {
                if (this._isWebviewUpdating)
                    return;
                if (this._updateDebounceTimer)
                    clearTimeout(this._updateDebounceTimer);
                this._updateDebounceTimer = setTimeout(() => {
                    this._update();
                }, 200);
            }
        }, null, this._disposables);
    }
    dispose() {
        ParameterSlidersPanel.currentPanel = undefined;
        this._panel.dispose();
        while (this._disposables.length) {
            const x = this._disposables.pop();
            if (x) {
                x.dispose();
            }
        }
    }
    _handleSelectionChange(line, character) {
        this._panel.webview.postMessage({
            command: 'highlightParameter',
            line: line,
            character: character
        });
    }
    _update() {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
            return;
        }
        const doc = editor.document;
        if (doc.languageId !== 'vibe') {
            return;
        }
        this._currentDocument = doc;
        const params = this._parseParameters(doc);
        if (this._areParamsStructurallyEqual(this._lastParams, params)) {
            this._updateParamValues(params);
        }
        else {
            this._panel.webview.html = this._getHtmlForWebview(params);
        }
        this._lastParams = params;
    }
    _areParamsStructurallyEqual(oldParams, newParams) {
        if (oldParams.length !== newParams.length)
            return false;
        for (let i = 0; i < oldParams.length; i++) {
            const p1 = oldParams[i];
            const p2 = newParams[i];
            if (p1.id !== p2.id || p1.name !== p2.name || p1.line !== p2.line || p1.contextName !== p2.contextName) {
                return false;
            }
        }
        return true;
    }
    _updateParamValues(params) {
        this._panel.webview.postMessage({
            command: 'updateValues',
            params: params
        });
    }
    _parseParameters(doc) {
        const text = doc.getText();
        const params = [];
        const lines = text.split('\n');
        const regexNumber = /(-?\d+(\.\d+)?)/g;
        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];
            const commentIdx = line.indexOf('//');
            const codeLine = commentIdx !== -1 ? line.substring(0, commentIdx) : line;
            let match;
            while ((match = regexNumber.exec(codeLine)) !== null) {
                const valStr = match[0];
                const val = parseFloat(valStr);
                const startChar = match.index;
                const endChar = startChar + valStr.length;
                if (this._isInsideString(codeLine, startChar)) {
                    continue;
                }
                const prefix = codeLine.substring(0, startChar);
                const isDb = /db\s*\(\s*$/.test(prefix);
                const name = this._determineContextName(codeLine, startChar, val);
                const contextName = this._findParentContext(lines, i, startChar);
                const { min, max, step } = this._determineRange(name, val, line, isDb);
                params.push({
                    id: `param-${i}-${startChar}`,
                    name,
                    value: val,
                    line: i,
                    startChar,
                    endChar,
                    min,
                    max,
                    step,
                    isDb,
                    contextName
                });
            }
        }
        return params;
    }
    _determineContextName(line, valueIndex, value) {
        const prefix = line.substring(0, valueIndex).trimEnd();
        let searchPrefix = prefix;
        if (/db\s*\(\s*$/.test(prefix)) {
            searchPrefix = prefix.replace(/db\s*\(\s*$/, "").trimEnd();
        }
        const letMatch = searchPrefix.match(/let\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*$/);
        if (letMatch)
            return letMatch[1];
        const assignMatch = searchPrefix.match(/([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*$/);
        if (assignMatch)
            return assignMatch[1];
        const paramMatch = searchPrefix.match(/param\s*\(\s*"([^"]+)"\s*,\s*$/);
        if (paramMatch)
            return paramMatch[1];
        const funcMatch = searchPrefix.match(/([a-zA-Z_][a-zA-Z0-9_]*)\s*\(\s*(?:[^)]*,)*\s*$/);
        if (funcMatch)
            return funcMatch[1];
        const colonMatch = searchPrefix.match(/([a-zA-Z_][a-zA-Z0-9_]*)\s*:\s*$/);
        if (colonMatch)
            return colonMatch[1];
        return `val`;
    }
    _determineRange(name, value, lineText, isDb) {
        let min = 0;
        let max = 1;
        let step = 0.01;
        const nameLower = name.toLowerCase();
        if (isDb) {
            min = -60;
            max = 6;
            step = 0.1;
            if (value < min)
                min = value - 10;
            if (value > max)
                max = value + 10;
        }
        else {
            if (nameLower.includes('freq') || nameLower.includes('cut') || nameLower.includes('rate') || (value > 50 && value < 22000)) {
                min = 20;
                max = 2000;
                step = 1;
                if (value > 2000)
                    max = 20000;
            }
            else if (nameLower.includes('amp') || nameLower.includes('gain') || nameLower.includes('mix') || nameLower.includes('prob') || (value >= 0 && value <= 1)) {
                min = 0;
                max = 1;
                step = 0.01;
            }
            else if (value > 100) {
                min = 0;
                max = value * 2;
                step = 1;
            }
            else if (value < 0) {
                min = value * 2;
                max = 0;
                step = 0.1;
            }
        }
        const rangeMatch = lineText.match(/range:\s*(-?\d+(\.\d+)?)\.\.(-?\d+(\.\d+)?)/);
        if (rangeMatch) {
            min = parseFloat(rangeMatch[1]);
            max = parseFloat(rangeMatch[3]);
            step = (max - min) / 100;
        }
        return { min, max, step };
    }
    _isInsideString(line, index) {
        let inSingle = false;
        let inDouble = false;
        for (let i = 0; i < line.length; i++) {
            if (i >= index) {
                break;
            }
            const ch = line[i];
            if (ch === '\\') {
                i++; // skip escaped char
                continue;
            }
            if (ch === '"' && !inSingle) {
                inDouble = !inDouble;
                continue;
            }
            if (ch === '\'' && !inDouble) {
                inSingle = !inSingle;
                continue;
            }
        }
        return inSingle || inDouble;
    }
    _findParentContext(lines, lineIndex, charIndex) {
        for (let i = lineIndex; i >= 0; i--) {
            const rawLine = lines[i];
            const commentIdx = rawLine.indexOf('//');
            let searchLine = commentIdx !== -1 ? rawLine.substring(0, commentIdx) : rawLine;
            if (i === lineIndex) {
                searchLine = searchLine.substring(0, charIndex);
            }
            const match = searchLine.match(/\b(define_synthdef|pattern|melody|voice|fx)\s*\(\s*["']([^"']+)["']/);
            if (match) {
                return match[2];
            }
        }
        return undefined;
    }
    _updateParameterInDocument(line, startChar, _ignoredEndChar, value) {
        const doc = this._currentDocument;
        if (!doc || doc.isClosed)
            return;
        if (line >= doc.lineCount)
            return;
        const lineText = doc.lineAt(line).text;
        const textAfterStart = lineText.substring(startChar);
        const match = textAfterStart.match(/^(-?\d+(\.\d+)?)/);
        if (!match) {
            return;
        }
        const currentNumberStr = match[0];
        const dynamicEndChar = startChar + currentNumberStr.length;
        const currentTextAtRange = currentNumberStr;
        let newValStr = value.toFixed(2);
        if (!currentTextAtRange.includes('.') && Math.floor(value) === value) {
            newValStr = value.toString();
        }
        else if (!currentTextAtRange.includes('.') && value.toString().length < 5) {
            newValStr = value.toFixed(2);
        }
        else if (currentTextAtRange.includes('.')) {
            const parts = currentTextAtRange.split('.');
            const precision = parts[1] ? parts[1].length : 2;
            newValStr = value.toFixed(precision);
        }
        if (parseFloat(newValStr) === parseFloat(currentTextAtRange)) {
            if (Math.abs(value - parseFloat(currentTextAtRange)) < 0.00001) {
                return;
            }
        }
        const range = new vscode.Range(new vscode.Position(line, startChar), new vscode.Position(line, dynamicEndChar));
        // Standard edit without smart undo options
        const edit = new vscode.WorkspaceEdit();
        edit.replace(doc.uri, range, newValStr);
        vscode.workspace.applyEdit(edit).then(success => {
            if (success) {
                this._triggerAutoSave(doc);
            }
        });
    }
    _triggerAutoSave(doc) {
        if (this._saveDebounceTimer) {
            clearTimeout(this._saveDebounceTimer);
        }
        this._saveDebounceTimer = setTimeout(() => {
            doc.save();
        }, 50);
    }
    _getHtmlForWebview(params) {
        const paramsJson = JSON.stringify(params);
        return `<!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Vibelang Parameters</title>
            <style>
                body { 
                    font-family: var(--vscode-font-family); 
                    background-color: var(--vscode-editor-background);
                    color: var(--vscode-editor-foreground);
                    padding: 10px; 
                }
                .parameter { 
                    margin-bottom: 12px; 
                    padding: 10px;
                    border-radius: 6px;
                    background-color: var(--vscode-editor-inactiveSelectionBackground);
                    opacity: 0.6;
                    transition: all 0.2s ease;
                    border: 1px solid transparent;
                }
                .parameter:hover {
                    opacity: 0.9;
                }
                .parameter.active {
                    opacity: 1;
                    background-color: var(--vscode-list-activeSelectionBackground);
                    color: var(--vscode-list-activeSelectionForeground);
                    border-color: var(--vscode-focusBorder);
                    box-shadow: 0 0 8px rgba(0,0,0,0.2);
                    transform: scale(1.02);
                }
                .label { 
                    display: flex; 
                    justify-content: space-between; 
                    align-items: center;
                    margin-bottom: 8px; 
                    font-size: 13px; 
                    font-weight: 500;
                }
                .slider-container {
                    display: flex;
                    align-items: center;
                    gap: 6px;
                }
                .range-buttons {
                    display: flex;
                    flex-direction: column;
                    gap: 4px;
                }
                input[type=range] { 
                    flex-grow: 1;
                    cursor: pointer;
                }
                button {
                    background: none;
                    border: 1px solid var(--vscode-button-background);
                    color: var(--vscode-button-foreground);
                    background-color: var(--vscode-button-background);
                    border-radius: 3px;
                    width: 20px;
                    height: 20px;
                    cursor: pointer;
                    font-size: 12px;
                    padding: 0;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                }
                button:hover {
                    background-color: var(--vscode-button-hoverBackground);
                }
                .range-val {
                    font-size: 10px;
                    color: var(--vscode-descriptionForeground);
                    min-width: 24px;
                    text-align: center;
                    cursor: default;
                }
                .meta { 
                    font-size: 10px; 
                    opacity: 0.8; 
                    margin-left: 8px;
                    font-weight: normal;
                }
                .context-tag {
                    font-size: 9px;
                    text-transform: uppercase;
                    letter-spacing: 0.05em;
                    padding: 1px 4px;
                    border-radius: 4px;
                    margin-right: 6px;
                    background-color: var(--vscode-badge-background);
                    color: var(--vscode-badge-foreground);
                }
                .value-display {
                    font-family: monospace;
                    font-size: 12px;
                }
            </style>
        </head>
        <body>
            <div id="parameters"></div>
            <script>
                const vscode = acquireVsCodeApi();
                let params = ${paramsJson};
                
                render();
                
                function render() {
                    const container = document.getElementById('parameters');
                    if (container.children.length === params.length) {
                        const updates = [];
                        let mismatch = false;
                        params.forEach((p, i) => {
                            const el = container.children[i];
                            if (el.id !== p.id) {
                                mismatch = true;
                            }
                        });
                        if (!mismatch) return; 
                    }

                    const existingRanges = new Map();
                    container.querySelectorAll('input[type=range]').forEach(el => {
                         const id = el.id.replace('slider-', '');
                         existingRanges.set(id, {min: el.min, max: el.max});
                    });

                    container.innerHTML = params.map(p => {
                        const prevRange = existingRanges.get(p.id);
                        if (prevRange) {
                            p.min = prevRange.min;
                            p.max = prevRange.max;
                        }
                        return createParamHtml(p);
                    }).join('');
                }

                function createParamHtml(p) {
                    const contextTag = p.contextName ? \`<span class="context-tag">\${p.contextName}</span>\` : '';
                    return \`
                    <div class="parameter" id="\${p.id}" data-line="\${p.line}" data-start="\${p.startChar}" data-end="\${p.endChar}">
                        <div class="label">
                            <span>\${contextTag ? contextTag + ' ' : ''}\${p.name} \${p.isDb ? '<span class="meta">(dB)</span>' : ''}</span>
                            <span class="value-display" id="val-\${p.id}">\${p.value.toFixed(2)}</span>
                        </div>
                        <div class="slider-container">
                            <div class="range-buttons">
                                <button onclick="adjustRange('\${p.id}', 'min', 1)" title="Raise Minimum">+</button>
                                <button onclick="adjustRange('\${p.id}', 'min', -1)" title="Extend Minimum">-</button>
                            </div>
                            <span class="range-val" id="min-display-\${p.id}">\${p.min}</span>
                            <input type="range" 
                                   id="slider-\${p.id}"
                                   min="\${p.min}" 
                                   max="\${p.max}" 
                                   step="\${p.step}" 
                                   value="\${p.value}"
                                   oninput="updateParam('\${p.id}', \${p.line}, \${p.startChar}, \${p.endChar}, this.value)">
                            <span class="range-val" id="max-display-\${p.id}">\${p.max}</span>
                            <div class="range-buttons">
                                <button onclick="adjustRange('\${p.id}', 'max', 1)" title="Extend Maximum">+</button>
                                <button onclick="adjustRange('\${p.id}', 'max', -1)" title="Lower Maximum">-</button>
                            </div>
                        </div>
                    </div>
                    \`;
                }

                function updateParam(id, line, startChar, endChar, value) {
                    document.getElementById('val-' + id).textContent = parseFloat(value).toFixed(2);
                    vscode.postMessage({
                        command: 'updateParameter',
                        id: id,
                        value: parseFloat(value),
                        line: line,
                        startChar: startChar,
                        endChar: endChar
                    });
                }

                function adjustRange(id, bound, direction) {
                    const slider = document.getElementById('slider-' + id);
                    const minDisplay = document.getElementById('min-display-' + id);
                    const maxDisplay = document.getElementById('max-display-' + id);
                    
                    const currentVal = parseFloat(slider.value);
                    let min = parseFloat(slider.min);
                    let max = parseFloat(slider.max);
                    const span = Math.max(max - min, parseFloat(slider.step) * 10 || 1);
                    const delta = Math.max(span * 0.05, parseFloat(slider.step) || 0.01) * direction;

                    if (bound === 'min') {
                        min = Math.min(max - (parseFloat(slider.step) || 0.01), min + delta);
                        slider.min = min.toFixed(4);
                    } else {
                        max = Math.max(min + (parseFloat(slider.step) || 0.01), max + delta);
                        slider.max = max.toFixed(4);
                    }

                    slider.value = Math.min(Math.max(currentVal, parseFloat(slider.min)), parseFloat(slider.max));

                    minDisplay.textContent = parseFloat(slider.min);
                    maxDisplay.textContent = parseFloat(slider.max);
                }

                window.addEventListener('message', event => {
                    const message = event.data;
                    
                    if (message.command === 'updateValues') {
                        message.params.forEach(p => {
                            const slider = document.getElementById('slider-' + p.id);
                            const valDisplay = document.getElementById('val-' + p.id);
                            if (slider && valDisplay) {
                                if (parseFloat(slider.value) !== p.value) {
                                    slider.value = p.value;
                                    valDisplay.textContent = p.value.toFixed(2);
                                }
                            }
                        });
                        return;
                    }

                    if (message.command === 'highlightParameter') {
                        const allParams = document.querySelectorAll('.parameter');
                        let found = false;
                        
                        allParams.forEach(p => {
                            const line = parseInt(p.getAttribute('data-line'));
                            const start = parseInt(p.getAttribute('data-start'));
                            const end = parseInt(p.getAttribute('data-end'));
                            
                            if (line === message.line && message.character >= start && message.character <= end) {
                                if (!p.classList.contains('active')) {
                                    p.classList.add('active');
                                    const rect = p.getBoundingClientRect();
                                    if (rect.top < 0 || rect.bottom > window.innerHeight) {
                                        p.scrollIntoView({ behavior: 'smooth', block: 'center' });
                                    }
                                }
                                found = true;
                            } else {
                                p.classList.remove('active');
                            }
                        });
                        
                        if (!found) {
                             allParams.forEach(p => {
                                const line = parseInt(p.getAttribute('data-line'));
                                if (line === message.line && !found) {
                                    if (!p.classList.contains('active')) {
                                        p.classList.add('active');
                                        const rect = p.getBoundingClientRect();
                                        if (rect.top < 0 || rect.bottom > window.innerHeight) {
                                            p.scrollIntoView({ behavior: 'smooth', block: 'center' });
                                        }
                                    }
                                    found = true;
                                }
                             });
                        }
                    }
                });
            </script>
        </body>
        </html>`;
    }
}
exports.ParameterSlidersPanel = ParameterSlidersPanel;
ParameterSlidersPanel.viewType = 'vibelangSliders';
//# sourceMappingURL=sliders.js.map