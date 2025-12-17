"use strict";
/**
 * VibeLang Sound Designer v2
 *
 * A professional-grade visual synthesizer builder with:
 * - Node-based graph editing with drag-and-drop
 * - Context-sensitive envelope and LFO editors
 * - Live preview via VibeLang runtime API
 * - Real-time code generation
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.SoundDesignerPanel = void 0;
const vscode = require("vscode");
const path = require("path");
const fs = require("fs");
class SoundDesignerPanel {
    constructor(panel, extensionPath, store) {
        this._disposables = [];
        this._ugenCategories = [];
        this._panel = panel;
        this._extensionPath = extensionPath;
        this._store = store;
        this._loadUGenManifests();
        this._addBuiltInNodes();
        this._updateContent();
        this._panel.webview.onDidReceiveMessage((message) => this._handleMessage(message), null, this._disposables);
        // Listen for status changes to update connection state
        this._disposables.push(store.onStatusChange(() => this._sendConnectionStatus()));
        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    }
    _sendConnectionStatus() {
        const isConnected = this._store.status === 'connected';
        this._panel.webview.postMessage({
            type: 'connectionStatus',
            connected: isConnected,
            baseUrl: this._store.runtime.baseUrl,
        });
    }
    static createOrShow(extensionPath, store) {
        const column = vscode.ViewColumn.One;
        if (SoundDesignerPanel.currentPanel) {
            SoundDesignerPanel.currentPanel._panel.reveal(column);
            return;
        }
        const panel = vscode.window.createWebviewPanel(SoundDesignerPanel.viewType, 'Sound Designer', column, {
            enableScripts: true,
            retainContextWhenHidden: true,
            localResourceRoots: [vscode.Uri.file(extensionPath)]
        });
        SoundDesignerPanel.currentPanel = new SoundDesignerPanel(panel, extensionPath, store);
    }
    static revive(panel, extensionPath, store) {
        SoundDesignerPanel.currentPanel = new SoundDesignerPanel(panel, extensionPath, store);
    }
    _loadUGenManifests() {
        const manifestDir = path.join(this._extensionPath, 'ugen_manifests');
        const categoryConfig = {
            'oscillators': { icon: '~', color: '#9bbb59' },
            'noise': { icon: '|||', color: '#858585' },
            'filters': { icon: ')', color: '#569cd6' },
            'envelopes': { icon: '/', color: '#d19a66' },
            'delays': { icon: '>>', color: '#c586c0' },
            'dynamics': { icon: '<>', color: '#d16969' },
            'panning': { icon: '<->', color: '#4ec9b0' },
            'math': { icon: '+', color: '#dcdcaa' },
            'buffers': { icon: '[]', color: '#ce9178' },
            'control': { icon: '*', color: '#6a9955' },
            'inout': { icon: 'I/O', color: '#4fc1ff' },
            'reverb': { icon: '~~~', color: '#c586c0' },
            'granular': { icon: '...', color: '#f48771' },
            'triggers': { icon: '!', color: '#ff8c00' },
            'fft': { icon: 'FFT', color: '#b5cea8' },
            'physical': { icon: '~o~', color: '#9cdcfe' },
        };
        try {
            const files = fs.readdirSync(manifestDir).filter(f => f.endsWith('.json'));
            for (const file of files) {
                const categoryName = path.basename(file, '.json');
                const filePath = path.join(manifestDir, file);
                const content = fs.readFileSync(filePath, 'utf8');
                const ugens = JSON.parse(content);
                const config = categoryConfig[categoryName] || { icon: '?', color: '#858585' };
                this._ugenCategories.push({
                    name: categoryName.charAt(0).toUpperCase() + categoryName.slice(1),
                    icon: config.icon,
                    color: config.color,
                    ugens
                });
            }
            this._ugenCategories.sort((a, b) => a.name.localeCompare(b.name));
        }
        catch (e) {
            console.error('Failed to load UGen manifests:', e);
        }
    }
    _addBuiltInNodes() {
        const utilityNodes = [
            {
                name: 'Add',
                description: 'Add N signals together (expands dynamically)',
                rates: ['ar', 'kr'],
                inputs: [
                    { name: 'in0', type: 'signal', default: 0, description: 'Input 1' },
                    { name: 'in1', type: 'signal', default: 0, description: 'Input 2' }
                ],
                outputs: 1,
                category: 'Utility',
                expandable: true,
                defaultValue: 0
            },
            {
                name: 'Mul',
                description: 'Multiply N signals together (expands dynamically)',
                rates: ['ar', 'kr'],
                inputs: [
                    { name: 'in0', type: 'signal', default: 1, description: 'Input 1' },
                    { name: 'in1', type: 'signal', default: 1, description: 'Input 2' }
                ],
                outputs: 1,
                category: 'Utility',
                expandable: true,
                defaultValue: 1
            },
            {
                name: 'Const',
                description: 'Constant value',
                rates: ['kr'],
                inputs: [
                    { name: 'value', type: 'float', default: 1.0, description: 'Constant value' }
                ],
                outputs: 1,
                category: 'Utility'
            },
            {
                name: 'Mix',
                description: 'Mix two signals with crossfade',
                rates: ['ar', 'kr'],
                inputs: [
                    { name: 'a', type: 'signal', default: 0, description: 'First signal' },
                    { name: 'b', type: 'signal', default: 0, description: 'Second signal' },
                    { name: 'mix', type: 'float', default: 0.5, description: 'Mix (0=a, 1=b)' }
                ],
                outputs: 1,
                category: 'Utility'
            },
            {
                name: 'Scale',
                description: 'Scale signal: (in * mul) + add',
                rates: ['ar', 'kr'],
                inputs: [
                    { name: 'in', type: 'signal', default: 0, description: 'Input signal' },
                    { name: 'mul', type: 'float', default: 1, description: 'Multiply factor' },
                    { name: 'add', type: 'float', default: 0, description: 'Add offset' }
                ],
                outputs: 1,
                category: 'Utility'
            },
            {
                name: 'Envelope',
                description: 'ADSR/ASR/Perc envelope',
                rates: ['kr'],
                inputs: [
                    { name: 'gate', type: 'signal', default: 1, description: 'Gate signal' }
                ],
                outputs: 1,
                category: 'Utility'
            },
            {
                name: 'LFO',
                description: 'Low frequency oscillator',
                rates: ['kr', 'ar'],
                inputs: [],
                outputs: 1,
                category: 'Utility'
            }
        ];
        this._ugenCategories.unshift({
            name: 'Utility',
            icon: '⚙',
            color: '#e0e0e0',
            ugens: utilityNodes
        });
    }
    _updateContent() {
        this._panel.webview.html = this._getHtmlContent();
        // Send initial connection status after a short delay to allow webview to initialize
        setTimeout(() => this._sendConnectionStatus(), 100);
    }
    async _handleMessage(message) {
        switch (message.command) {
            case 'generateCode':
                const code = message.code;
                const doc = await vscode.workspace.openTextDocument({
                    content: code,
                    language: 'vibe'
                });
                await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
                break;
            case 'savePreset':
                const presetData = message.data;
                const uri = await vscode.window.showSaveDialog({
                    filters: { 'VibeLang Preset': ['vpreset'] },
                    defaultUri: vscode.Uri.file('synth.vpreset')
                });
                if (uri) {
                    fs.writeFileSync(uri.fsPath, presetData);
                    vscode.window.showInformationMessage('Preset saved!');
                }
                break;
            case 'loadPreset':
                const uris = await vscode.window.showOpenDialog({
                    filters: { 'VibeLang Preset': ['vpreset'] },
                    canSelectMany: false
                });
                if (uris && uris[0]) {
                    const data = fs.readFileSync(uris[0].fsPath, 'utf8');
                    this._panel.webview.postMessage({ command: 'presetLoaded', data });
                }
                break;
            case 'showInfo':
                vscode.window.showInformationMessage(message.text);
                break;
            case 'showError':
                vscode.window.showErrorMessage(message.text);
                break;
        }
    }
    _getHtmlContent() {
        const ugenCategoriesJson = JSON.stringify(this._ugenCategories);
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Sound Designer</title>
    <style>
        ${this._getStyles()}
    </style>
</head>
<body>
    <div id="app">
        <!-- Top Toolbar -->
        <header class="toolbar">
            <div class="toolbar-section">
                <div class="logo">
                    <span class="logo-icon">~</span>
                    <span class="logo-text">Sound Designer</span>
                </div>
                <div class="toolbar-divider"></div>
                <button class="tool-btn" onclick="app.newPatch()" title="New Patch">
                    <span class="btn-icon">+</span> New
                </button>
                <button class="tool-btn" onclick="app.loadPreset()" title="Load Preset">Load</button>
                <button class="tool-btn" onclick="app.savePreset()" title="Save Preset">Save</button>
            </div>
            <div class="toolbar-section center">
                <input type="text" id="synthName" class="synth-name" value="my_synth" spellcheck="false">
            </div>
            <div class="toolbar-section">
                <div class="connection-status" id="connectionStatus" title="VibeLang Runtime">
                    <span class="status-dot disconnected"></span>
                    <span class="status-text">Disconnected</span>
                </div>
                <div class="toolbar-divider"></div>
                <button class="tool-btn primary" onclick="app.openInEditor()">
                    <span class="btn-icon">&lt;/&gt;</span> Generate
                </button>
            </div>
        </header>

        <div class="main-layout">
            <!-- Left: UGen Palette -->
            <aside class="sidebar palette">
                <div class="sidebar-header">
                    <h2>UGens</h2>
                    <input type="text" class="search-box" placeholder="Search..." oninput="app.filterUGens(this.value)">
                </div>
                <div class="palette-list" id="paletteList"></div>
            </aside>

            <!-- Center: Node Canvas -->
            <main class="canvas-area">
                <div class="canvas-tools">
                    <span class="canvas-hint">Drag UGens to canvas • Connect outputs→inputs • Click cables to delete</span>
                    <div class="zoom-controls">
                        <button onclick="app.zoomOut()">−</button>
                        <span id="zoomLevel">100%</span>
                        <button onclick="app.zoomIn()">+</button>
                    </div>
                </div>
                <div class="canvas" id="canvas">
                    <svg class="cables-svg" id="cablesSvg"></svg>
                    <div class="nodes-container" id="nodesContainer"></div>
                </div>
            </main>

            <!-- Right: Inspector + Editors -->
            <aside class="sidebar inspector">
                <!-- Node Inspector -->
                <section class="panel inspector-panel">
                    <div class="panel-header">
                        <h3>Inspector</h3>
                    </div>
                    <div class="panel-body" id="inspectorBody">
                        <div class="empty-state">Select a node to edit its properties</div>
                    </div>
                </section>

                <!-- Envelope Editor (shown when Envelope node selected) -->
                <section class="panel envelope-panel" id="envelopePanel" style="display:none;">
                    <div class="panel-header">
                        <h3>Envelope</h3>
                        <select id="envType" class="mini-select" onchange="app.setEnvType(this.value)">
                            <option value="adsr">ADSR</option>
                            <option value="asr">ASR</option>
                            <option value="perc">Perc</option>
                        </select>
                    </div>
                    <div class="panel-body">
                        <canvas id="envCanvas" class="env-canvas"></canvas>
                        <div class="env-sliders">
                            <div class="slider-row" id="envAttackRow">
                                <label>Attack</label>
                                <input type="range" id="envAttack" min="0.001" max="2" step="0.001" value="0.01" oninput="app.updateEnvFromSlider()">
                                <span class="slider-val" id="envAttackVal">10ms</span>
                            </div>
                            <div class="slider-row" id="envDecayRow">
                                <label>Decay</label>
                                <input type="range" id="envDecay" min="0.001" max="2" step="0.001" value="0.1" oninput="app.updateEnvFromSlider()">
                                <span class="slider-val" id="envDecayVal">100ms</span>
                            </div>
                            <div class="slider-row" id="envSustainRow">
                                <label>Sustain</label>
                                <input type="range" id="envSustain" min="0" max="1" step="0.01" value="0.7" oninput="app.updateEnvFromSlider()">
                                <span class="slider-val" id="envSustainVal">70%</span>
                            </div>
                            <div class="slider-row" id="envReleaseRow">
                                <label>Release</label>
                                <input type="range" id="envRelease" min="0.001" max="4" step="0.001" value="0.3" oninput="app.updateEnvFromSlider()">
                                <span class="slider-val" id="envReleaseVal">300ms</span>
                            </div>
                        </div>
                    </div>
                </section>

                <!-- LFO Editor (shown when LFO node selected) -->
                <section class="panel lfo-panel" id="lfoPanel" style="display:none;">
                    <div class="panel-header">
                        <h3>LFO</h3>
                    </div>
                    <div class="panel-body">
                        <canvas id="lfoCanvas" class="lfo-canvas"></canvas>
                        <div class="waveform-btns">
                            <button class="wave-btn active" data-wave="sine" onclick="app.setLfoWave('sine')">∿</button>
                            <button class="wave-btn" data-wave="saw" onclick="app.setLfoWave('saw')">/|</button>
                            <button class="wave-btn" data-wave="tri" onclick="app.setLfoWave('tri')">△</button>
                            <button class="wave-btn" data-wave="square" onclick="app.setLfoWave('square')">⊓</button>
                        </div>
                        <div class="slider-row">
                            <label>Rate</label>
                            <input type="range" id="lfoRate" min="0.1" max="20" step="0.1" value="4" oninput="app.updateLfoFromSlider()">
                            <span class="slider-val" id="lfoRateVal">4 Hz</span>
                        </div>
                        <div class="slider-row">
                            <label>Depth</label>
                            <input type="range" id="lfoDepth" min="0" max="1" step="0.01" value="1" oninput="app.updateLfoFromSlider()">
                            <span class="slider-val" id="lfoDepthVal">100%</span>
                        </div>
                    </div>
                </section>

                <!-- Preview Piano -->
                <section class="panel preview-panel">
                    <div class="panel-header">
                        <h3>Preview</h3>
                        <div class="octave-ctrl">
                            <button onclick="app.octaveDown()">◀</button>
                            <span id="octaveDisplay">C4</span>
                            <button onclick="app.octaveUp()">▶</button>
                        </div>
                    </div>
                    <div class="panel-body">
                        <div class="piano" id="piano"></div>
                        <div class="preview-hint" id="previewHint">
                            Connect to VibeLang runtime for live preview
                        </div>
                    </div>
                </section>
            </aside>
        </div>

        <!-- Bottom: Code Preview -->
        <div class="bottom-bar">
            <div class="code-header">
                <span>Generated Code</span>
                <button class="copy-btn" onclick="app.copyCode()">Copy</button>
            </div>
            <pre class="code-output" id="codeOutput"><code>// Build your synth by connecting nodes</code></pre>
        </div>
    </div>

    <script>
        const vscode = acquireVsCodeApi();
        const ugenCategories = ${ugenCategoriesJson};

        ${this._getJavaScript()}
    </script>
</body>
</html>`;
    }
    _getStyles() {
        return `
:root {
    --bg-0: #0a0a0a;
    --bg-1: #141414;
    --bg-2: #1e1e1e;
    --bg-3: #282828;
    --bg-4: #333333;
    --text: #e0e0e0;
    --text-dim: #888;
    --text-bright: #fff;
    --accent: #4fc3f7;
    --accent-dim: #2196f3;
    --green: #66bb6a;
    --orange: #ffb74d;
    --red: #ef5350;
    --purple: #ba68c8;
    --border: #3a3a3a;
    --node-bg: linear-gradient(180deg, #252528 0%, #1f1f22 100%);
}

* { box-sizing: border-box; margin: 0; padding: 0; }

body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: var(--bg-0);
    color: var(--text);
    font-size: 12px;
    overflow: hidden;
    height: 100vh;
    user-select: none;
}

#app {
    display: grid;
    grid-template-rows: 44px 1fr 120px;
    height: 100vh;
}

/* Toolbar */
.toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 12px;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
}

.toolbar-section {
    display: flex;
    align-items: center;
    gap: 8px;
}
.toolbar-section.center { flex: 1; justify-content: center; }

.logo {
    display: flex;
    align-items: center;
    gap: 6px;
}
.logo-icon {
    width: 26px; height: 26px;
    background: linear-gradient(135deg, var(--accent), var(--green));
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    font-weight: bold;
    color: #000;
}
.logo-text {
    font-weight: 600;
    font-size: 14px;
    color: var(--text-bright);
}

.toolbar-divider {
    width: 1px;
    height: 24px;
    background: var(--border);
    margin: 0 4px;
}

.tool-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-3);
    color: var(--text);
    font-size: 11px;
    cursor: pointer;
    transition: all 0.15s;
}
.tool-btn:hover { background: var(--bg-4); border-color: var(--text-dim); }
.tool-btn.primary {
    background: var(--accent-dim);
    border-color: var(--accent);
    color: white;
}
.tool-btn.primary:hover { background: var(--accent); }
.btn-icon { font-weight: bold; }

.synth-name {
    width: 200px;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-1);
    color: var(--text-bright);
    font-size: 14px;
    font-weight: 600;
    text-align: center;
    font-family: 'SF Mono', Monaco, monospace;
}
.synth-name:focus { outline: none; border-color: var(--accent); }

.connection-status {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 10px;
    color: var(--text-dim);
}
.status-dot {
    width: 8px; height: 8px;
    border-radius: 50%;
    background: var(--red);
}
.status-dot.connected { background: var(--green); }
.status-dot.connecting { background: var(--orange); animation: pulse 1s infinite; }
@keyframes pulse { 50% { opacity: 0.5; } }

/* Main Layout */
.main-layout {
    display: grid;
    grid-template-columns: 220px 1fr 280px;
    overflow: hidden;
}

/* Sidebars */
.sidebar {
    background: var(--bg-1);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
}
.sidebar.inspector {
    border-right: none;
    border-left: 1px solid var(--border);
    overflow-y: auto;
}

.sidebar-header {
    padding: 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-2);
}
.sidebar-header h2 {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--text-dim);
    margin-bottom: 8px;
}

.search-box {
    width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-1);
    color: var(--text);
    font-size: 11px;
}
.search-box:focus { outline: none; border-color: var(--accent); }

.palette-list {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
}

.category {
    margin-bottom: 4px;
}
.category-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 6px;
    cursor: pointer;
}
.category-header:hover { background: var(--bg-3); }
.category-header.open .cat-arrow { transform: rotate(90deg); }

.cat-icon {
    width: 20px; height: 20px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: bold;
    color: #000;
}
.cat-name { flex: 1; font-weight: 600; font-size: 11px; }
.cat-count { font-size: 9px; color: var(--text-dim); }
.cat-arrow { font-size: 8px; color: var(--text-dim); transition: transform 0.2s; }

.category-items {
    display: none;
    padding: 4px 0 4px 16px;
}
.category-items.open { display: block; }

.ugen-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-radius: 4px;
    cursor: grab;
    font-family: 'SF Mono', Monaco, monospace;
    font-size: 10px;
}
.ugen-item:hover { background: var(--bg-3); }
.ugen-item.dragging { opacity: 0.4; }
.ugen-dot { width: 6px; height: 6px; border-radius: 50%; }

/* Canvas */
.canvas-area {
    display: flex;
    flex-direction: column;
    background: var(--bg-0);
    overflow: hidden;
}

.canvas-tools {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
}
.canvas-hint { font-size: 10px; color: var(--text-dim); }
.zoom-controls {
    display: flex;
    align-items: center;
    gap: 4px;
}
.zoom-controls button {
    width: 24px; height: 24px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-3);
    color: var(--text);
    cursor: pointer;
}
.zoom-controls button:hover { background: var(--bg-4); }
.zoom-controls span { font-size: 10px; color: var(--text-dim); width: 40px; text-align: center; }

.canvas {
    flex: 1;
    position: relative;
    overflow: hidden;
    background:
        radial-gradient(circle at center, var(--bg-1) 0%, var(--bg-0) 100%),
        repeating-linear-gradient(0deg, transparent, transparent 19px, rgba(255,255,255,0.02) 20px),
        repeating-linear-gradient(90deg, transparent, transparent 19px, rgba(255,255,255,0.02) 20px);
}

.cables-svg {
    position: absolute;
    top: 0; left: 0;
    width: 100%; height: 100%;
    pointer-events: none;
    z-index: 10;
}
.cables-svg path { pointer-events: stroke; }

.nodes-container {
    position: absolute;
    top: 0; left: 0;
    width: 100%; height: 100%;
    z-index: 2;
}

/* Nodes */
.node {
    position: absolute;
    min-width: 120px;
    background: var(--node-bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.4);
    cursor: move;
    transition: box-shadow 0.15s, border-color 0.15s;
}
.node:hover { box-shadow: 0 6px 20px rgba(0,0,0,0.6); }
.node.selected { border-color: var(--accent); box-shadow: 0 0 0 2px rgba(79, 195, 247, 0.3); }

.node-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    border-radius: 8px 8px 0 0;
}
.node-icon {
    width: 18px; height: 18px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: bold;
    color: #000;
}
.node-title {
    flex: 1;
    font-family: 'SF Mono', Monaco, monospace;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-bright);
}
.node-close {
    width: 14px; height: 14px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 12px;
    opacity: 0;
    transition: opacity 0.15s;
}
.node:hover .node-close { opacity: 1; }
.node-close:hover { background: var(--red); color: white; }

.node-body { padding: 6px 0; }

.port-row {
    display: flex;
    align-items: center;
    padding: 3px 10px;
    gap: 6px;
}
.port-row.input { justify-content: flex-start; }
.port-row.output { justify-content: flex-end; }

.port {
    width: 10px; height: 10px;
    border-radius: 50%;
    border: 2px solid;
    background: var(--bg-1);
    cursor: crosshair;
    transition: transform 0.1s;
}
.port:hover { transform: scale(1.3); }
.port.input { border-color: var(--accent); }
.port.output { border-color: var(--green); }
.port.connected { background: currentColor; }
.port.valid-target {
    transform: scale(1.3);
    box-shadow: 0 0 6px var(--accent);
    border-color: var(--accent) !important;
}
.port.snap-target {
    transform: scale(1.6);
    box-shadow: 0 0 12px var(--accent), 0 0 24px var(--accent), 0 0 36px var(--accent);
    border-color: var(--accent) !important;
    background: var(--accent) !important;
    animation: pulse-snap 0.3s ease-in-out infinite;
}
@keyframes pulse-snap {
    0%, 100% { transform: scale(1.6); }
    50% { transform: scale(1.8); }
}

.port-label { font-size: 9px; color: var(--text-dim); }

.param-input {
    width: 50px;
    padding: 2px 4px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--bg-1);
    color: var(--text);
    font-family: 'SF Mono', Monaco, monospace;
    font-size: 9px;
    text-align: right;
}
.param-input:focus { outline: none; border-color: var(--accent); }

/* Cables */
.cable-hit {
    fill: none;
    stroke: transparent;
    stroke-width: 16;
    stroke-linecap: round;
    cursor: pointer;
    pointer-events: stroke;
}
.cable {
    fill: none;
    stroke-width: 2;
    stroke-linecap: round;
    filter: drop-shadow(0 2px 4px rgba(0,0,0,0.3));
    cursor: pointer;
    pointer-events: stroke;
    transition: stroke-width 0.15s, stroke 0.15s, filter 0.15s;
}
.cable.hovered {
    stroke-width: 4;
    stroke: var(--red) !important;
    filter: drop-shadow(0 0 8px rgba(239, 83, 80, 0.5));
}
.cable.temp {
    stroke-dasharray: 8 4;
    animation: dash-anim 0.5s linear infinite;
    pointer-events: none;
}
.cable.temp.snapped {
    stroke-dasharray: none;
    stroke-width: 3;
    filter: drop-shadow(0 0 4px var(--accent));
    animation: none;
}
@keyframes dash-anim { to { stroke-dashoffset: -12; } }

/* Inspector Panels */
.panel {
    border-bottom: 1px solid var(--border);
}
.panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    background: var(--bg-2);
}
.panel-header h3 {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-dim);
}
.panel-body { padding: 12px; }

.empty-state {
    text-align: center;
    color: var(--text-dim);
    font-style: italic;
    padding: 20px;
}

.mini-select {
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-3);
    color: var(--text);
    font-size: 10px;
    cursor: pointer;
}

/* Envelope Canvas */
.env-canvas {
    width: 100%;
    height: 80px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 6px;
    margin-bottom: 10px;
}

.env-sliders { display: flex; flex-direction: column; gap: 8px; }

.slider-row {
    display: flex;
    align-items: center;
    gap: 8px;
}
.slider-row label {
    width: 50px;
    font-size: 10px;
    color: var(--text-dim);
}
.slider-row input[type="range"] {
    flex: 1;
    height: 4px;
    -webkit-appearance: none;
    background: var(--bg-3);
    border-radius: 2px;
}
.slider-row input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px; height: 12px;
    background: var(--orange);
    border-radius: 50%;
    cursor: pointer;
}
.slider-val {
    width: 50px;
    font-size: 10px;
    color: var(--orange);
    text-align: right;
    font-family: 'SF Mono', Monaco, monospace;
}

/* LFO */
.lfo-canvas {
    width: 100%;
    height: 50px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 6px;
    margin-bottom: 10px;
}

.waveform-btns {
    display: flex;
    gap: 4px;
    margin-bottom: 10px;
}
.wave-btn {
    flex: 1;
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-3);
    color: var(--text-dim);
    cursor: pointer;
    font-size: 12px;
}
.wave-btn:hover { background: var(--bg-4); }
.wave-btn.active { background: var(--purple); border-color: var(--purple); color: white; }

/* Piano */
.piano {
    display: flex;
    height: 70px;
    background: var(--bg-1);
    border-radius: 6px;
    overflow: hidden;
    position: relative;
    margin-bottom: 8px;
}
.piano-key {
    position: relative;
    flex: 1;
    background: linear-gradient(to bottom, #fafafa 0%, #e8e8e8 100%);
    border: 1px solid #bbb;
    border-radius: 0 0 4px 4px;
    cursor: pointer;
    z-index: 1;
    transition: background 0.05s;
}
.piano-key:hover { background: linear-gradient(to bottom, #fff 0%, #f0f0f0 100%); }
.piano-key:active, .piano-key.pressed {
    background: linear-gradient(to bottom, #ddd 0%, #ccc 100%);
    border-top-width: 2px;
}
.piano-key.black {
    position: absolute;
    width: 10%;
    height: 55%;
    background: linear-gradient(to bottom, #333 0%, #111 100%);
    border: 1px solid #000;
    border-radius: 0 0 3px 3px;
    z-index: 2;
}
.piano-key.black:hover { background: linear-gradient(to bottom, #444 0%, #222 100%); }
.piano-key.black:active, .piano-key.black.pressed {
    background: linear-gradient(to bottom, #555 0%, #333 100%);
}

.octave-ctrl {
    display: flex;
    align-items: center;
    gap: 4px;
}
.octave-ctrl button {
    width: 20px; height: 20px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-3);
    color: var(--text-dim);
    cursor: pointer;
    font-size: 8px;
}
.octave-ctrl button:hover { background: var(--bg-4); }
.octave-ctrl span { font-size: 10px; color: var(--text); }

.preview-hint {
    font-size: 9px;
    color: var(--text-dim);
    text-align: center;
}
.preview-hint.connected { color: var(--green); }

/* Bottom Bar */
.bottom-bar {
    background: var(--bg-2);
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
}
.code-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    font-size: 10px;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.5px;
}
.copy-btn {
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-3);
    color: var(--text);
    font-size: 9px;
    cursor: pointer;
}
.copy-btn:hover { background: var(--bg-4); }

.code-output {
    flex: 1;
    margin: 0;
    padding: 10px 12px;
    font-family: 'SF Mono', Monaco, monospace;
    font-size: 10px;
    line-height: 1.5;
    color: var(--text);
    background: var(--bg-1);
    overflow: auto;
    white-space: pre-wrap;
}
.code-output .kw { color: var(--purple); }
.code-output .fn { color: var(--accent); }
.code-output .str { color: var(--orange); }
.code-output .num { color: var(--green); }
.code-output .cmt { color: var(--text-dim); }

/* Scrollbars */
::-webkit-scrollbar { width: 6px; height: 6px; }
::-webkit-scrollbar-track { background: var(--bg-1); }
::-webkit-scrollbar-thumb { background: var(--bg-4); border-radius: 3px; }
        `;
    }
    _getJavaScript() {
        return `
class SoundDesigner {
    constructor() {
        this.nodes = [];
        this.connections = [];
        this.selectedNode = null;
        this.nextId = 1;
        this.zoom = 1;
        this.baseOctave = 4;

        // Drag state
        this.dragging = null;
        this.dragOffset = { x: 0, y: 0 };
        this.cableDrag = null;

        // Runtime connection
        this.runtimeConnected = false;
        this.runtimeUrl = 'http://localhost:1606';
        this.previewDirty = true;  // Track if synthdef needs redeployment
        this.creatingVoice = false;  // Prevent concurrent voice creation

        // LFO animation
        this.lfoPhase = 0;

        // Envelope drag state
        this.envPoints = null;
        this.envDimensions = null;
        this.envHoveredPoint = null;
        this.envDraggingPoint = null;
        this.envDragType = null;

        this.init();
    }

    init() {
        this.renderPalette();
        this.renderPiano();
        this.setupEvents();
        this.addInitialNodes();
        this.checkRuntimeConnection();
        this.startAnimations();

        // Periodic runtime check
        setInterval(() => this.checkRuntimeConnection(), 3000);
    }

    // ===========================================
    // Palette
    // ===========================================

    renderPalette() {
        const list = document.getElementById('paletteList');
        list.innerHTML = '';

        ugenCategories.forEach((cat, idx) => {
            const catEl = document.createElement('div');
            catEl.className = 'category';
            catEl.innerHTML = \`
                <div class="category-header \${idx === 0 ? 'open' : ''}" onclick="app.toggleCategory(this)">
                    <div class="cat-icon" style="background: \${cat.color}">\${cat.icon}</div>
                    <span class="cat-name">\${cat.name}</span>
                    <span class="cat-count">\${cat.ugens.length}</span>
                    <span class="cat-arrow">▶</span>
                </div>
                <div class="category-items \${idx === 0 ? 'open' : ''}">
                    \${cat.ugens.map(u => \`
                        <div class="ugen-item" draggable="true"
                             data-ugen="\${u.name}" data-cat="\${cat.name}" data-color="\${cat.color}"
                             title="\${u.description}">
                            <span class="ugen-dot" style="background: \${cat.color}"></span>
                            \${u.name}
                        </div>
                    \`).join('')}
                </div>
            \`;
            list.appendChild(catEl);
        });

        // Drag events for palette items
        document.querySelectorAll('.ugen-item').forEach(item => {
            item.addEventListener('dragstart', e => {
                e.target.classList.add('dragging');
                e.dataTransfer.setData('ugen', e.target.dataset.ugen);
                e.dataTransfer.setData('cat', e.target.dataset.cat);
                e.dataTransfer.setData('color', e.target.dataset.color);
            });
            item.addEventListener('dragend', e => e.target.classList.remove('dragging'));
        });
    }

    toggleCategory(header) {
        header.classList.toggle('open');
        header.nextElementSibling.classList.toggle('open');
    }

    filterUGens(query) {
        const q = query.toLowerCase();
        document.querySelectorAll('.ugen-item').forEach(item => {
            item.style.display = item.dataset.ugen.toLowerCase().includes(q) ? 'flex' : 'none';
        });
    }

    // ===========================================
    // Events
    // ===========================================

    setupEvents() {
        const canvas = document.getElementById('canvas');
        const container = document.getElementById('nodesContainer');

        // Drop on canvas
        canvas.addEventListener('dragover', e => { e.preventDefault(); });
        canvas.addEventListener('drop', e => this.onCanvasDrop(e));

        // Node interaction
        container.addEventListener('mousedown', e => this.onNodeMouseDown(e));
        document.addEventListener('mousemove', e => this.onMouseMove(e));
        document.addEventListener('mouseup', e => this.onMouseUp(e));

        // Keyboard
        document.addEventListener('keydown', e => this.onKeyDown(e));
    }

    onCanvasDrop(e) {
        e.preventDefault();
        const ugen = e.dataTransfer.getData('ugen');
        const cat = e.dataTransfer.getData('cat');
        const color = e.dataTransfer.getData('color');

        if (!ugen) return;

        const canvas = document.getElementById('canvas');
        const rect = canvas.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const y = e.clientY - rect.top;

        this.createNode(ugen, cat, color, x, y);
    }

    onNodeMouseDown(e) {
        // Port click - start cable (use closest() to handle clicks on port or its children)
        const port = e.target.closest('.port');
        if (port) {
            this.startCableDrag(e, port);
            return;
        }

        // Node click
        const nodeEl = e.target.closest('.node');
        if (!nodeEl) return;

        const node = this.nodes.find(n => n.id === nodeEl.id);
        if (!node) return;

        this.selectNode(node);

        // Start dragging node
        this.dragging = node;
        this.dragOffset = {
            x: e.clientX - node.x,
            y: e.clientY - node.y
        };

        e.preventDefault();
    }

    onMouseMove(e) {
        // Dragging node
        if (this.dragging) {
            this.dragging.x = Math.max(0, e.clientX - this.dragOffset.x);
            this.dragging.y = Math.max(0, e.clientY - this.dragOffset.y);

            const el = document.getElementById(this.dragging.id);
            if (el) {
                el.style.left = this.dragging.x + 'px';
                el.style.top = this.dragging.y + 'px';
            }
            this.renderCables();
        }

        // Dragging cable
        if (this.cableDrag) {
            this.updateTempCable(e);
        }
    }

    onMouseUp(e) {
        if (this.cableDrag) {
            this.finishCableDrag(e);
        }
        this.dragging = null;
    }

    onKeyDown(e) {
        if ((e.key === 'Delete' || e.key === 'Backspace') && this.selectedNode) {
            if (this.selectedNode.type !== 'output') {
                this.deleteNode(this.selectedNode.id);
            }
        }
    }

    // ===========================================
    // Nodes
    // ===========================================

    addInitialNodes() {
        // Output node (cannot be deleted)
        this.createNode('Output', 'Utility', '#e0e0e0', 550, 200, { type: 'output' });

        // Parameter nodes
        this.createNode('freq', 'Utility', '#66bb6a', 60, 100, { type: 'param', value: 440 });
        this.createNode('amp', 'Utility', '#66bb6a', 60, 200, { type: 'param', value: 0.5 });
        this.createNode('gate', 'Utility', '#66bb6a', 60, 300, { type: 'param', value: 1 });
    }

    createNode(name, category, color, x, y, config = {}) {
        // Use timestamp + random to ensure truly unique IDs
        const id = 'node_' + Date.now() + '_' + Math.random().toString(36).substr(2, 9);
        const ugenData = this.findUGen(name);

        // Deep copy inputs array so each node instance has its own
        const inputsCopy = config.type === 'output' ? [{ name: 'signal', default: 0 }] :
                          config.type === 'param' ? [] :
                          ugenData ? JSON.parse(JSON.stringify(ugenData.inputs)) : [];

        const node = {
            id,
            name,
            category,
            color,
            x, y,
            type: config.type || 'ugen',
            value: config.value,
            inputs: inputsCopy,
            outputs: config.type === 'output' ? 0 :
                     config.type === 'param' ? 1 :
                     ugenData ? ugenData.outputs : 1,
            rates: ugenData ? ugenData.rates : ['ar'],
            rate: ugenData && ugenData.rates.includes('ar') ? 'ar' : 'kr',
            params: {},
            // Expandable nodes (Add, Mul) can grow inputs dynamically
            expandable: ugenData?.expandable || false,
            // Envelope specific
            envelope: name === 'Envelope' ? { type: 'adsr', a: 0.01, d: 0.1, s: 0.7, r: 0.3 } : null,
            // LFO specific
            lfo: name === 'LFO' ? { wave: 'sine', rate: 4, depth: 1 } : null
        };

        // Initialize params
        if (node.inputs) {
            node.inputs.forEach(inp => {
                node.params[inp.name] = inp.default;
            });
        }

        this.nodes.push(node);
        this.renderNode(node);
        this.updateCode();

        return node;
    }

    findUGen(name) {
        for (const cat of ugenCategories) {
            const ugen = cat.ugens.find(u => u.name === name);
            if (ugen) return ugen;
        }
        return null;
    }

    renderNode(node) {
        const container = document.getElementById('nodesContainer');
        const el = document.createElement('div');
        el.className = 'node' + (node.type === 'output' ? ' output-node' : '');
        el.id = node.id;
        el.style.left = node.x + 'px';
        el.style.top = node.y + 'px';

        // Build inputs HTML
        let inputsHtml = '';
        if (node.inputs && node.inputs.length > 0) {
            inputsHtml = node.inputs.map((inp, i) => \`
                <div class="port-row input">
                    <div class="port input" data-node="\${node.id}" data-port="\${i}" data-dir="input" data-param="\${inp.name}"></div>
                    <span class="port-label">\${inp.name}</span>
                    <input type="number" class="param-input" value="\${node.params[inp.name] ?? inp.default}"
                           onchange="app.setNodeParam('\${node.id}', '\${inp.name}', this.value)"
                           onclick="event.stopPropagation()">
                </div>
            \`).join('');
        }

        // Param node value
        let valueHtml = '';
        if (node.type === 'param') {
            valueHtml = \`
                <div class="port-row">
                    <input type="number" class="param-input" style="width:70px;margin:0 auto;"
                           value="\${node.value}"
                           onchange="app.setParamValue('\${node.id}', this.value)"
                           onclick="event.stopPropagation()">
                </div>
            \`;
        }

        // Extra info for envelope/lfo
        let extraHtml = '';
        if (node.envelope) {
            extraHtml = \`<div class="port-row"><span class="port-label" style="margin:0 auto;">\${node.envelope.type.toUpperCase()}</span></div>\`;
        }
        if (node.lfo) {
            extraHtml = \`<div class="port-row"><span class="port-label" style="margin:0 auto;">\${node.lfo.wave} \${node.lfo.rate}Hz</span></div>\`;
        }

        // Output port
        let outputHtml = '';
        if (node.outputs > 0) {
            outputHtml = \`
                <div class="port-row output">
                    <span class="port-label">out</span>
                    <div class="port output" data-node="\${node.id}" data-port="0" data-dir="output"></div>
                </div>
            \`;
        }

        el.innerHTML = \`
            <div class="node-header">
                <div class="node-icon" style="background: \${node.color}">\${this.getNodeIcon(node)}</div>
                <span class="node-title">\${node.name}</span>
                \${node.type !== 'output' ? '<button class="node-close" onclick="app.deleteNode(\\'' + node.id + '\\')">&times;</button>' : ''}
            </div>
            <div class="node-body">
                \${inputsHtml}
                \${valueHtml}
                \${extraHtml}
                \${outputHtml}
            </div>
        \`;

        container.appendChild(el);
    }

    getNodeIcon(node) {
        if (node.type === 'output') return '▸';
        if (node.type === 'param') return 'P';
        if (node.name === 'Add') return '+';
        if (node.name === 'Mul') return '×';
        if (node.name === 'Const') return '#';
        if (node.name === 'Mix') return '⊕';
        if (node.name === 'Scale') return '⋈';
        if (node.name === 'Envelope') return '/\\\\';
        if (node.name === 'LFO') return '∿';
        const cat = ugenCategories.find(c => c.name === node.category);
        return cat ? cat.icon : '~';
    }

    deleteNode(nodeId) {
        const node = this.nodes.find(n => n.id === nodeId);
        if (!node || node.type === 'output') return;

        // Remove connections
        this.connections = this.connections.filter(c => c.from.node !== nodeId && c.to.node !== nodeId);
        this.nodes = this.nodes.filter(n => n.id !== nodeId);

        document.getElementById(nodeId)?.remove();
        this.renderCables();
        this.updateCode();

        if (this.selectedNode?.id === nodeId) {
            this.selectedNode = null;
            this.updateInspector();
        }
    }

    selectNode(node) {
        document.querySelectorAll('.node.selected').forEach(el => el.classList.remove('selected'));
        this.selectedNode = node;

        if (node) {
            document.getElementById(node.id)?.classList.add('selected');
        }

        this.updateInspector();
    }

    setNodeParam(nodeId, param, value) {
        const node = this.nodes.find(n => n.id === nodeId);
        if (node) {
            node.params[param] = parseFloat(value);
            this.updateCode();
        }
    }

    setParamValue(nodeId, value) {
        const node = this.nodes.find(n => n.id === nodeId);
        if (node) {
            node.value = parseFloat(value);
            this.updateCode();
        }
    }

    // ===========================================
    // Cables
    // ===========================================

    startCableDrag(e, port) {
        // Port element is now passed directly from onNodeMouseDown
        if (!port) {
            console.log('[SoundDesigner] startCableDrag: no port provided');
            return;
        }

        console.log('[SoundDesigner] startCableDrag: node=' + port.dataset.node +
                    ', port=' + port.dataset.port + ', dir=' + port.dataset.dir);

        this.cableDrag = {
            node: port.dataset.node,
            port: parseInt(port.dataset.port),
            dir: port.dataset.dir,
            param: port.dataset.param,
            el: port
        };

        // Highlight valid drop targets
        this.highlightValidTargets(true);

        e.stopPropagation();
        e.preventDefault();
    }

    highlightValidTargets(show) {
        document.querySelectorAll('.port').forEach(port => {
            if (show && this.cableDrag) {
                // Valid target: different node AND different direction
                const isValid = port.dataset.node !== this.cableDrag.node &&
                               port.dataset.dir !== this.cableDrag.dir;
                if (isValid) {
                    port.classList.add('valid-target');
                } else {
                    port.classList.remove('valid-target');
                }
            } else {
                port.classList.remove('valid-target');
            }
        });
    }

    findNearestValidPort(mouseX, mouseY, snapThreshold) {
        let nearestPort = null;
        let nearestDistance = snapThreshold;

        document.querySelectorAll('.port').forEach(port => {
            // Skip if same node or same direction (invalid connection)
            if (port.dataset.node === this.cableDrag.node) return;
            if (port.dataset.dir === this.cableDrag.dir) return;

            const rect = port.getBoundingClientRect();
            const portCenterX = rect.left + rect.width / 2;
            const portCenterY = rect.top + rect.height / 2;

            const distance = Math.hypot(mouseX - portCenterX, mouseY - portCenterY);
            if (distance < nearestDistance) {
                nearestDistance = distance;
                nearestPort = port;
            }
        });

        return { port: nearestPort, distance: nearestDistance };
    }

    updateTempCable(e) {
        const svg = document.getElementById('cablesSvg');
        svg.querySelector('.temp')?.remove();

        const canvas = document.getElementById('canvas');
        const canvasRect = canvas.getBoundingClientRect();
        const portRect = this.cableDrag.el.getBoundingClientRect();

        const x1 = portRect.left + portRect.width/2 - canvasRect.left;
        const y1 = portRect.top + portRect.height/2 - canvasRect.top;

        // Find nearest valid port within snap distance
        const snapThreshold = 50; // pixels - generous snap distance
        const { port: snapPort } = this.findNearestValidPort(e.clientX, e.clientY, snapThreshold);

        // Clear previous snap highlight
        document.querySelectorAll('.port.snap-target').forEach(p => p.classList.remove('snap-target'));

        let x2, y2;
        if (snapPort) {
            // Snap to the port
            const snapRect = snapPort.getBoundingClientRect();
            x2 = snapRect.left + snapRect.width/2 - canvasRect.left;
            y2 = snapRect.top + snapRect.height/2 - canvasRect.top;

            // Highlight the snap target
            snapPort.classList.add('snap-target');

            // Store the snapped port for finishCableDrag
            this.cableDrag.snapPort = snapPort;
        } else {
            // No snap - follow mouse
            x2 = e.clientX - canvasRect.left;
            y2 = e.clientY - canvasRect.top;
            this.cableDrag.snapPort = null;
        }

        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('class', 'cable temp' + (snapPort ? ' snapped' : ''));
        path.setAttribute('d', this.cablePath(x1, y1, x2, y2));
        path.setAttribute('stroke', snapPort ? '#4fc3f7' : '#66bb6a');
        svg.appendChild(path);
    }

    finishCableDrag(e) {
        if (!this.cableDrag) return;

        const svg = document.getElementById('cablesSvg');
        svg.querySelector('.temp')?.remove();

        // Clear all highlights
        this.highlightValidTargets(false);
        document.querySelectorAll('.port.snap-target').forEach(p => p.classList.remove('snap-target'));

        // Always find nearest valid port at release time for accuracy
        // Use larger threshold (75px) for more forgiving release
        const { port: targetPort } = this.findNearestValidPort(e.clientX, e.clientY, 75);

        if (targetPort) {
            const targetDir = targetPort.dataset.dir;
            const targetNodeId = targetPort.dataset.node;
            const targetPortIndex = parseInt(targetPort.dataset.port);
            const targetParam = targetPort.dataset.param;

            // Validate the target node exists in DOM and in our nodes array
            const targetNodeEl = document.getElementById(targetNodeId);
            const targetNodeData = this.nodes.find(n => n.id === targetNodeId);
            if (!targetNodeEl || !targetNodeData) {
                console.log('[SoundDesigner] finishCableDrag: target node not found, aborting');
                this.cableDrag = null;
                return;
            }

            // Validate the source node exists
            const sourceNodeEl = document.getElementById(this.cableDrag.node);
            const sourceNodeData = this.nodes.find(n => n.id === this.cableDrag.node);
            if (!sourceNodeEl || !sourceNodeData) {
                console.log('[SoundDesigner] finishCableDrag: source node not found, aborting');
                this.cableDrag = null;
                return;
            }

            console.log('[SoundDesigner] finishCableDrag: connecting to port ' +
                        targetNodeId + ':' + targetPortIndex + ':' + targetDir);

            // Determine which is the source (output) and which is the dest (input)
            let fromNode, fromPort, toNode, toPort, toParam;

            if (this.cableDrag.dir === 'output') {
                fromNode = this.cableDrag.node;
                fromPort = this.cableDrag.port;
                toNode = targetNodeId;
                toPort = targetPortIndex;
                toParam = targetParam;
            } else {
                fromNode = targetNodeId;
                fromPort = targetPortIndex;
                toNode = this.cableDrag.node;
                toPort = this.cableDrag.port;
                toParam = this.cableDrag.param;
            }

            console.log('[SoundDesigner] finishCableDrag: creating connection from ' +
                        fromNode + ':' + fromPort + ' to ' + toNode + ':' + toPort + ' (' + toParam + ')');

            // Check if this input port already has a connection - if so, replace it
            const existingConnIndex = this.connections.findIndex(c =>
                c.to.node === toNode && c.to.port === toPort
            );

            if (existingConnIndex !== -1) {
                // Check if we're trying to create the same connection that already exists
                const existingConn = this.connections[existingConnIndex];
                if (existingConn.from.node === fromNode && existingConn.from.port === fromPort) {
                    console.log('[SoundDesigner] finishCableDrag: same connection already exists, skipping');
                    this.cableDrag = null;
                    return;
                }
                console.log('[SoundDesigner] finishCableDrag: replacing existing connection');
                this.connections.splice(existingConnIndex, 1);
            }

            this.connections.push({
                id: 'conn_' + Date.now(),
                from: { node: fromNode, port: fromPort },
                to: { node: toNode, port: toPort, param: toParam }
            });

            // Check if we need to expand the node (for Add/Mul)
            this.maybeExpandNode(toNode);

            this.renderCables();
            this.updateCode();
        } else {
            console.log('[SoundDesigner] finishCableDrag: no valid port within snap threshold');
        }

        this.cableDrag = null;
    }

    maybeExpandNode(nodeId) {
        const node = this.nodes.find(n => n.id === nodeId);
        if (!node || !node.expandable) return;

        // Check if all inputs are connected
        const connectedPorts = this.connections
            .filter(c => c.to.node === nodeId)
            .map(c => c.to.port);

        const allConnected = node.inputs.every((_, i) => connectedPorts.includes(i));

        if (allConnected) {
            // Add a new input
            const newIndex = node.inputs.length;
            const defaultVal = node.name === 'Add' ? 0 : 1;
            node.inputs.push({
                name: 'in' + newIndex,
                type: 'signal',
                default: defaultVal,
                description: 'Input ' + (newIndex + 1)
            });
            node.params['in' + newIndex] = defaultVal;

            // Re-render the node to show new input
            this.rerenderNode(node);
        }
    }

    rerenderNode(node) {
        const oldEl = document.getElementById(node.id);
        if (oldEl) oldEl.remove();
        this.renderNode(node);
        // Re-select if it was selected
        if (this.selectedNode?.id === node.id) {
            document.getElementById(node.id)?.classList.add('selected');
        }
    }

    cablePath(x1, y1, x2, y2) {
        const dx = Math.abs(x2 - x1);
        const offset = Math.min(dx * 0.5, 100);
        return \`M\${x1},\${y1} C\${x1+offset},\${y1} \${x2-offset},\${y2} \${x2},\${y2}\`;
    }

    renderCables() {
        const svg = document.getElementById('cablesSvg');
        const canvas = document.getElementById('canvas');
        const canvasRect = canvas.getBoundingClientRect();

        // Clear non-temp cables and hit areas
        svg.querySelectorAll('.cable:not(.temp), .cable-hit').forEach(el => el.remove());

        this.connections.forEach(conn => {
            const fromEl = document.getElementById(conn.from.node);
            const toEl = document.getElementById(conn.to.node);
            if (!fromEl || !toEl) return;

            const fromPort = fromEl.querySelector('.port[data-dir="output"]');
            const toPort = toEl.querySelector(\`.port[data-dir="input"][data-port="\${conn.to.port}"]\`);
            if (!fromPort || !toPort) return;

            const fromRect = fromPort.getBoundingClientRect();
            const toRect = toPort.getBoundingClientRect();

            const x1 = fromRect.left + fromRect.width/2 - canvasRect.left;
            const y1 = fromRect.top + fromRect.height/2 - canvasRect.top;
            const x2 = toRect.left + toRect.width/2 - canvasRect.left;
            const y2 = toRect.top + toRect.height/2 - canvasRect.top;

            const srcNode = this.nodes.find(n => n.id === conn.from.node);
            const color = srcNode?.color || '#66bb6a';
            const d = this.cablePath(x1, y1, x2, y2);

            // Create invisible hit area (wider, for easier clicking)
            const hitArea = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            hitArea.setAttribute('class', 'cable-hit');
            hitArea.setAttribute('d', d);
            hitArea.setAttribute('data-conn', conn.id);
            svg.appendChild(hitArea);

            // Create visible cable
            const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            path.setAttribute('class', 'cable');
            path.setAttribute('d', d);
            path.setAttribute('stroke', color);
            path.setAttribute('data-conn', conn.id);
            svg.appendChild(path);

            // Use JS for hover (CSS sibling selector doesn't work reliably)
            hitArea.onmouseenter = () => path.classList.add('hovered');
            hitArea.onmouseleave = () => path.classList.remove('hovered');
            hitArea.onclick = () => this.deleteConnection(conn.id);
            path.onmouseenter = () => path.classList.add('hovered');
            path.onmouseleave = () => path.classList.remove('hovered');
            path.onclick = () => this.deleteConnection(conn.id);
        });

        // Update port connection states
        this.updatePortStates();
    }

    updatePortStates() {
        // Reset all input ports to disconnected
        document.querySelectorAll('.port.input').forEach(port => {
            port.classList.remove('connected');
        });

        // Mark connected input ports
        this.connections.forEach(conn => {
            const toEl = document.getElementById(conn.to.node);
            if (toEl) {
                const port = toEl.querySelector(\`.port.input[data-port="\${conn.to.port}"]\`);
                if (port) port.classList.add('connected');
            }
        });

        // Also mark output ports that have connections
        document.querySelectorAll('.port.output').forEach(port => {
            port.classList.remove('connected');
        });
        this.connections.forEach(conn => {
            const fromEl = document.getElementById(conn.from.node);
            if (fromEl) {
                const port = fromEl.querySelector('.port.output');
                if (port) port.classList.add('connected');
            }
        });
    }

    deleteConnection(connId) {
        this.connections = this.connections.filter(c => c.id !== connId);
        this.renderCables();
        this.updateCode();
    }

    // ===========================================
    // Inspector
    // ===========================================

    updateInspector() {
        const body = document.getElementById('inspectorBody');
        const envPanel = document.getElementById('envelopePanel');
        const lfoPanel = document.getElementById('lfoPanel');

        // Hide special panels by default
        envPanel.style.display = 'none';
        lfoPanel.style.display = 'none';

        if (!this.selectedNode) {
            body.innerHTML = '<div class="empty-state">Select a node to edit its properties</div>';
            return;
        }

        const node = this.selectedNode;
        let html = \`<div style="margin-bottom:12px;"><strong style="font-size:13px;color:var(--text-bright);">\${node.name}</strong></div>\`;

        // Rate selector
        if (node.rates && node.rates.length > 1) {
            html += \`
                <div class="slider-row" style="margin-bottom:12px;">
                    <label>Rate</label>
                    <select class="mini-select" style="flex:1;" onchange="app.setNodeRate('\${node.id}', this.value)">
                        \${node.rates.map(r => \`<option value="\${r}" \${r === node.rate ? 'selected' : ''}>\${r}</option>\`).join('')}
                    </select>
                </div>
            \`;
        }

        // Parameters
        if (node.inputs && node.inputs.length > 0) {
            html += '<div style="font-size:10px;color:var(--text-dim);margin-bottom:6px;">PARAMETERS</div>';
            node.inputs.forEach(inp => {
                const val = node.params[inp.name] ?? inp.default;
                html += \`
                    <div class="slider-row">
                        <label>\${inp.name}</label>
                        <input type="number" class="param-input" style="flex:1;" value="\${val}"
                               onchange="app.setNodeParam('\${node.id}', '\${inp.name}', this.value)">
                    </div>
                \`;
            });
        }

        body.innerHTML = html;

        // Show envelope editor if envelope node
        if (node.envelope) {
            envPanel.style.display = 'block';
            this.syncEnvControls(node);
            this.drawEnvelope();
            this.initEnvCanvasEvents();
        }

        // Show LFO editor if LFO node
        if (node.lfo) {
            lfoPanel.style.display = 'block';
            this.syncLfoControls(node);
        }
    }

    setNodeRate(nodeId, rate) {
        const node = this.nodes.find(n => n.id === nodeId);
        if (node) {
            node.rate = rate;
            this.updateCode();
        }
    }

    // ===========================================
    // Envelope Editor
    // ===========================================

    syncEnvControls(node) {
        document.getElementById('envType').value = node.envelope.type;
        document.getElementById('envAttack').value = node.envelope.a;
        document.getElementById('envDecay').value = node.envelope.d;
        document.getElementById('envSustain').value = node.envelope.s;
        document.getElementById('envRelease').value = node.envelope.r;

        this.updateEnvLabels();
        this.updateEnvVisibility();
    }

    updateEnvLabels() {
        const a = parseFloat(document.getElementById('envAttack').value);
        const d = parseFloat(document.getElementById('envDecay').value);
        const s = parseFloat(document.getElementById('envSustain').value);
        const r = parseFloat(document.getElementById('envRelease').value);

        document.getElementById('envAttackVal').textContent = (a * 1000).toFixed(0) + 'ms';
        document.getElementById('envDecayVal').textContent = (d * 1000).toFixed(0) + 'ms';
        document.getElementById('envSustainVal').textContent = Math.round(s * 100) + '%';
        document.getElementById('envReleaseVal').textContent = (r * 1000).toFixed(0) + 'ms';
    }

    updateEnvVisibility() {
        const type = document.getElementById('envType').value;
        document.getElementById('envDecayRow').style.display = type === 'adsr' ? 'flex' : 'none';
        document.getElementById('envSustainRow').style.display = type !== 'perc' ? 'flex' : 'none';
    }

    setEnvType(type) {
        if (this.selectedNode?.envelope) {
            this.selectedNode.envelope.type = type;
            this.updateEnvVisibility();
            this.drawEnvelope();
            this.updateNodeDisplay(this.selectedNode);
            this.updateCode();
        }
    }

    updateEnvFromSlider() {
        if (!this.selectedNode?.envelope) return;

        this.selectedNode.envelope.a = parseFloat(document.getElementById('envAttack').value);
        this.selectedNode.envelope.d = parseFloat(document.getElementById('envDecay').value);
        this.selectedNode.envelope.s = parseFloat(document.getElementById('envSustain').value);
        this.selectedNode.envelope.r = parseFloat(document.getElementById('envRelease').value);

        this.updateEnvLabels();
        this.drawEnvelope();
        this.updateCode();
    }

    drawEnvelope() {
        const canvas = document.getElementById('envCanvas');
        const ctx = canvas.getContext('2d');

        // Set canvas size
        canvas.width = canvas.offsetWidth * 2;
        canvas.height = canvas.offsetHeight * 2;
        ctx.scale(2, 2);

        const w = canvas.offsetWidth;
        const h = canvas.offsetHeight;
        const pad = 10;

        ctx.clearRect(0, 0, w, h);

        // Grid lines
        ctx.strokeStyle = '#333';
        ctx.lineWidth = 0.5;
        for (let i = 1; i < 4; i++) {
            const y = pad + (h - 2*pad) * i / 4;
            ctx.beginPath();
            ctx.moveTo(pad, y);
            ctx.lineTo(w - pad, y);
            ctx.stroke();
        }

        if (!this.selectedNode?.envelope) return;

        const env = this.selectedNode.envelope;
        const { type, a, d, s, r } = env;

        let points = [];
        let totalTime;

        // Store points for drag detection
        if (type === 'adsr') {
            totalTime = a + d + 0.3 + r;
            const xs = (w - 2*pad) / totalTime;
            const ys = h - 2*pad;
            points = [
                { x: pad, y: h - pad, drag: null },  // Start (not draggable)
                { x: pad + a*xs, y: pad, drag: 'attack' },  // Attack peak
                { x: pad + (a+d)*xs, y: pad + (1-s)*ys, drag: 'decay' },  // Decay end
                { x: pad + (a+d+0.3)*xs, y: pad + (1-s)*ys, drag: null },  // Sustain end (auto)
                { x: pad + totalTime*xs, y: h - pad, drag: 'release' }  // Release end
            ];
        } else if (type === 'asr') {
            totalTime = a + 0.3 + r;
            const xs = (w - 2*pad) / totalTime;
            const ys = h - 2*pad;
            points = [
                { x: pad, y: h - pad, drag: null },
                { x: pad + a*xs, y: pad + (1-s)*ys, drag: 'attack' },
                { x: pad + (a+0.3)*xs, y: pad + (1-s)*ys, drag: null },
                { x: pad + totalTime*xs, y: h - pad, drag: 'release' }
            ];
        } else { // perc
            totalTime = a + r;
            const xs = (w - 2*pad) / totalTime;
            points = [
                { x: pad, y: h - pad, drag: null },
                { x: pad + a*xs, y: pad, drag: 'attack' },
                { x: pad + totalTime*xs, y: h - pad, drag: 'release' }
            ];
        }

        // Store for drag detection
        this.envPoints = points;
        this.envDimensions = { w, h, pad, totalTime };

        // Draw fill with curves
        ctx.beginPath();
        ctx.moveTo(points[0].x, points[0].y);
        for (let i = 1; i < points.length; i++) {
            const p0 = points[i - 1];
            const p1 = points[i];
            // Use quadratic curves for smoother look
            const cpx = p0.x + (p1.x - p0.x) * 0.5;
            const cpy1 = p0.y;
            const cpy2 = p1.y;
            ctx.bezierCurveTo(cpx, cpy1, cpx, cpy2, p1.x, p1.y);
        }
        ctx.lineTo(points[points.length-1].x, h - pad);
        ctx.lineTo(pad, h - pad);
        ctx.fillStyle = 'rgba(255, 183, 77, 0.15)';
        ctx.fill();

        // Draw line with curves
        ctx.beginPath();
        ctx.moveTo(points[0].x, points[0].y);
        for (let i = 1; i < points.length; i++) {
            const p0 = points[i - 1];
            const p1 = points[i];
            const cpx = p0.x + (p1.x - p0.x) * 0.5;
            const cpy1 = p0.y;
            const cpy2 = p1.y;
            ctx.bezierCurveTo(cpx, cpy1, cpx, cpy2, p1.x, p1.y);
        }
        ctx.strokeStyle = '#ffb74d';
        ctx.lineWidth = 2;
        ctx.stroke();

        // Points (draggable ones are larger)
        points.forEach((p, i) => {
            const draggable = p.drag !== null;
            const isHovered = this.envHoveredPoint === i;
            const isDragging = this.envDraggingPoint === i;

            ctx.beginPath();
            ctx.arc(p.x, p.y, draggable ? (isHovered || isDragging ? 7 : 5) : 3, 0, Math.PI * 2);

            if (draggable) {
                ctx.fillStyle = isDragging ? '#fff' : (isHovered ? '#ffd180' : '#ffb74d');
                ctx.fill();
                ctx.strokeStyle = '#fff';
                ctx.lineWidth = 1;
                ctx.stroke();
            } else {
                ctx.fillStyle = '#888';
                ctx.fill();
            }
        });
    }

    initEnvCanvasEvents() {
        const canvas = document.getElementById('envCanvas');
        if (!canvas || canvas.dataset.eventsInit) return;
        canvas.dataset.eventsInit = 'true';

        canvas.addEventListener('mousedown', (e) => this.onEnvMouseDown(e));
        canvas.addEventListener('mousemove', (e) => this.onEnvMouseMove(e));
        canvas.addEventListener('mouseup', () => this.onEnvMouseUp());
        canvas.addEventListener('mouseleave', () => this.onEnvMouseUp());
    }

    onEnvMouseDown(e) {
        if (!this.envPoints) return;
        const canvas = e.target;
        const rect = canvas.getBoundingClientRect();
        const x = (e.clientX - rect.left) * (canvas.width / rect.width) / 2;
        const y = (e.clientY - rect.top) * (canvas.height / rect.height) / 2;

        // Find closest draggable point
        for (let i = 0; i < this.envPoints.length; i++) {
            const p = this.envPoints[i];
            if (p.drag === null) continue;
            const dist = Math.sqrt((x - p.x) ** 2 + (y - p.y) ** 2);
            if (dist < 12) {
                this.envDraggingPoint = i;
                this.envDragType = p.drag;
                e.preventDefault();
                return;
            }
        }
    }

    onEnvMouseMove(e) {
        if (!this.envPoints || !this.envDimensions) return;
        const canvas = e.target;
        const rect = canvas.getBoundingClientRect();
        const x = (e.clientX - rect.left) * (canvas.width / rect.width) / 2;
        const y = (e.clientY - rect.top) * (canvas.height / rect.height) / 2;

        const { w, h, pad } = this.envDimensions;

        // Update hover state
        let newHover = null;
        for (let i = 0; i < this.envPoints.length; i++) {
            const p = this.envPoints[i];
            if (p.drag === null) continue;
            const dist = Math.sqrt((x - p.x) ** 2 + (y - p.y) ** 2);
            if (dist < 12) {
                newHover = i;
                canvas.style.cursor = 'grab';
                break;
            }
        }
        if (newHover === null) canvas.style.cursor = 'default';
        if (newHover !== this.envHoveredPoint) {
            this.envHoveredPoint = newHover;
            this.drawEnvelope();
        }

        // Handle dragging
        if (this.envDraggingPoint !== null && this.selectedNode?.envelope) {
            canvas.style.cursor = 'grabbing';
            const env = this.selectedNode.envelope;
            const type = env.type;

            // Clamp values
            const clampX = Math.max(pad, Math.min(w - pad, x));
            const clampY = Math.max(pad, Math.min(h - pad, y));

            // Convert to time/level values
            const totalWidth = w - 2 * pad;
            const totalHeight = h - 2 * pad;

            if (this.envDragType === 'attack') {
                // Attack: X controls time, Y controls peak level (only for ASR)
                const newA = Math.max(0.001, (clampX - pad) / totalWidth * this.envDimensions.totalTime * 0.5);
                env.a = Math.round(newA * 1000) / 1000;
                if (type === 'asr') {
                    const newS = 1 - (clampY - pad) / totalHeight;
                    env.s = Math.max(0, Math.min(1, Math.round(newS * 100) / 100));
                }
            } else if (this.envDragType === 'decay') {
                // Decay: X controls decay time, Y controls sustain level
                const attackEnd = pad + env.a / this.envDimensions.totalTime * totalWidth;
                const newD = Math.max(0.001, (clampX - attackEnd) / totalWidth * this.envDimensions.totalTime);
                env.d = Math.round(newD * 1000) / 1000;
                const newS = 1 - (clampY - pad) / totalHeight;
                env.s = Math.max(0, Math.min(1, Math.round(newS * 100) / 100));
            } else if (this.envDragType === 'release') {
                // Release: X controls total length (release time)
                const sustainEnd = pad + (env.a + (env.d || 0) + 0.3) / this.envDimensions.totalTime * totalWidth;
                const newR = Math.max(0.01, (clampX - sustainEnd) / totalWidth * this.envDimensions.totalTime * 2);
                env.r = Math.round(newR * 1000) / 1000;
            }

            // Update sliders
            this.syncEnvControls(this.selectedNode);
            this.drawEnvelope();
            this.updateCode();
        }
    }

    onEnvMouseUp() {
        if (this.envDraggingPoint !== null) {
            this.envDraggingPoint = null;
            this.envDragType = null;
            const canvas = document.getElementById('envCanvas');
            if (canvas) canvas.style.cursor = 'default';
        }
    }

    // ===========================================
    // LFO Editor
    // ===========================================

    syncLfoControls(node) {
        document.querySelectorAll('.wave-btn').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.wave === node.lfo.wave);
        });
        document.getElementById('lfoRate').value = node.lfo.rate;
        document.getElementById('lfoDepth').value = node.lfo.depth;

        document.getElementById('lfoRateVal').textContent = node.lfo.rate.toFixed(1) + ' Hz';
        document.getElementById('lfoDepthVal').textContent = Math.round(node.lfo.depth * 100) + '%';
    }

    setLfoWave(wave) {
        if (this.selectedNode?.lfo) {
            this.selectedNode.lfo.wave = wave;
            document.querySelectorAll('.wave-btn').forEach(btn => {
                btn.classList.toggle('active', btn.dataset.wave === wave);
            });
            this.updateNodeDisplay(this.selectedNode);
            this.updateCode();
        }
    }

    updateLfoFromSlider() {
        if (!this.selectedNode?.lfo) return;

        this.selectedNode.lfo.rate = parseFloat(document.getElementById('lfoRate').value);
        this.selectedNode.lfo.depth = parseFloat(document.getElementById('lfoDepth').value);

        document.getElementById('lfoRateVal').textContent = this.selectedNode.lfo.rate.toFixed(1) + ' Hz';
        document.getElementById('lfoDepthVal').textContent = Math.round(this.selectedNode.lfo.depth * 100) + '%';

        this.updateNodeDisplay(this.selectedNode);
        this.updateCode();
    }

    drawLfo() {
        const canvas = document.getElementById('lfoCanvas');
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        canvas.width = canvas.offsetWidth * 2;
        canvas.height = canvas.offsetHeight * 2;
        ctx.scale(2, 2);

        const w = canvas.offsetWidth;
        const h = canvas.offsetHeight;

        ctx.clearRect(0, 0, w, h);

        // Center line
        ctx.strokeStyle = '#333';
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(0, h/2);
        ctx.lineTo(w, h/2);
        ctx.stroke();

        // Get LFO settings from selected node or defaults
        const lfo = this.selectedNode?.lfo || { wave: 'sine', rate: 4, depth: 1 };

        ctx.beginPath();
        ctx.strokeStyle = '#ba68c8';
        ctx.lineWidth = 2;

        for (let x = 0; x < w; x++) {
            const t = (x / w) * 3 + this.lfoPhase;
            let y;

            switch (lfo.wave) {
                case 'sine': y = Math.sin(t * Math.PI * 2); break;
                case 'saw': y = ((t % 1) * 2) - 1; break;
                case 'tri': y = Math.abs((t % 1) * 4 - 2) - 1; break;
                case 'square': y = t % 1 < 0.5 ? 1 : -1; break;
                default: y = Math.sin(t * Math.PI * 2);
            }

            y *= lfo.depth;
            const py = (h/2) - (y * (h/2 - 6));

            if (x === 0) ctx.moveTo(x, py);
            else ctx.lineTo(x, py);
        }
        ctx.stroke();
    }

    updateNodeDisplay(node) {
        const el = document.getElementById(node.id);
        if (!el) return;

        // Update extra info in node
        const extraSpan = el.querySelector('.node-body .port-label[style*="margin:0 auto"]');
        if (extraSpan) {
            if (node.envelope) {
                extraSpan.textContent = node.envelope.type.toUpperCase();
            }
            if (node.lfo) {
                extraSpan.textContent = node.lfo.wave + ' ' + node.lfo.rate + 'Hz';
            }
        }
    }

    // ===========================================
    // Piano
    // ===========================================

    renderPiano() {
        const piano = document.getElementById('piano');

        // One octave + one note (C to C)
        const whites = ['C', 'D', 'E', 'F', 'G', 'A', 'B', 'C'];
        const blackPositions = [
            { note: 'C#', left: '11%' },
            { note: 'D#', left: '25.5%' },
            { note: 'F#', left: '54%' },
            { note: 'G#', left: '68%' },
            { note: 'A#', left: '82%' }
        ];

        // Clear existing keys
        piano.innerHTML = '';

        console.log('[SoundDesigner] renderPiano: baseOctave=' + this.baseOctave);

        // Create white keys
        whites.forEach((note, i) => {
            const midi = this.noteToMidi(note + this.baseOctave) + (note === 'C' && i === 7 ? 12 : 0);
            console.log('[SoundDesigner] White key ' + note + ' (i=' + i + ') -> midi ' + midi);
            const key = document.createElement('div');
            key.className = 'piano-key white';
            key.dataset.midi = String(midi);
            key.dataset.note = note + (note === 'C' && i === 7 ? (this.baseOctave + 1) : this.baseOctave);
            piano.appendChild(key);
        });

        // Create black keys
        blackPositions.forEach(({ note, left }) => {
            const midi = this.noteToMidi(note + this.baseOctave);
            console.log('[SoundDesigner] Black key ' + note + ' -> midi ' + midi);
            const key = document.createElement('div');
            key.className = 'piano-key black';
            key.style.left = left;
            key.dataset.midi = String(midi);
            key.dataset.note = note + this.baseOctave;
            piano.appendChild(key);
        });

        // Set up unified event handling for all keys
        this.setupPianoEvents();
    }

    setupPianoEvents() {
        const piano = document.getElementById('piano');

        // Track the currently active key (the one being pressed by mouse)
        let activeKey = null;
        let activeMidi = null;

        // Mouse down on a key - start playing
        piano.addEventListener('mousedown', (e) => {
            const key = e.target.closest('.piano-key');
            if (!key) return;

            e.preventDefault();
            e.stopPropagation();

            const midi = parseInt(key.dataset.midi, 10);
            console.log('[SoundDesigner] Piano mousedown: midi=' + midi + ', note=' + key.dataset.note);

            // Stop any previously active key
            if (activeKey && activeMidi !== null) {
                activeKey.classList.remove('pressed');
                this.sendNoteOff(activeMidi);
            }

            // Activate this key
            activeKey = key;
            activeMidi = midi;
            key.classList.add('pressed');
            this.sendNoteOn(midi);
        });

        // Mouse up anywhere - stop the active note
        document.addEventListener('mouseup', (e) => {
            if (activeKey && activeMidi !== null) {
                console.log('[SoundDesigner] Piano mouseup: stopping midi=' + activeMidi);
                activeKey.classList.remove('pressed');
                this.sendNoteOff(activeMidi);
                activeKey = null;
                activeMidi = null;
            }
        });

        // Mouse leaves a key while pressed - stop that note and potentially start another
        piano.addEventListener('mouseleave', (e) => {
            // Only handle if we have an active key and mouse button is still down
            if (activeKey && activeMidi !== null && e.buttons === 1) {
                console.log('[SoundDesigner] Piano mouseleave while pressed: stopping midi=' + activeMidi);
                activeKey.classList.remove('pressed');
                this.sendNoteOff(activeMidi);
                activeKey = null;
                activeMidi = null;
            }
        });

        // Mouse enters a key while button is down - start playing that key
        piano.addEventListener('mouseover', (e) => {
            if (e.buttons !== 1) return; // Only when mouse button is held

            const key = e.target.closest('.piano-key');
            if (!key || key === activeKey) return;

            const midi = parseInt(key.dataset.midi, 10);
            console.log('[SoundDesigner] Piano mouseover with button down: midi=' + midi);

            // Stop previous key if any
            if (activeKey && activeMidi !== null) {
                activeKey.classList.remove('pressed');
                this.sendNoteOff(activeMidi);
            }

            // Activate new key
            activeKey = key;
            activeMidi = midi;
            key.classList.add('pressed');
            this.sendNoteOn(midi);
        });
    }

    sendNoteOn(midi) {
        // Don't await - fire and forget for responsiveness
        this.playNote(midi);
    }

    sendNoteOff(midi) {
        // Don't await - fire and forget for responsiveness
        this.stopNote(midi);
    }

    noteToMidi(note) {
        const notes = { 'C': 0, 'C#': 1, 'D': 2, 'D#': 3, 'E': 4, 'F': 5, 'F#': 6, 'G': 7, 'G#': 8, 'A': 9, 'A#': 10, 'B': 11 };
        const match = note.match(/([A-G]#?)(\d+)/);
        if (!match) return 60;
        return notes[match[1]] + (parseInt(match[2]) + 1) * 12;
    }

    octaveUp() {
        if (this.baseOctave < 7) {
            this.baseOctave++;
            document.getElementById('octaveDisplay').textContent = 'C' + this.baseOctave;
            this.renderPiano();
        }
    }

    octaveDown() {
        if (this.baseOctave > 1) {
            this.baseOctave--;
            document.getElementById('octaveDisplay').textContent = 'C' + this.baseOctave;
            this.renderPiano();
        }
    }

    async ensurePreviewVoice() {
        const synthName = document.getElementById('synthName').value;
        const voiceName = '__preview_' + synthName;

        console.log('[SoundDesigner] ensurePreviewVoice: dirty=' + this.previewDirty + ', voiceName=' + voiceName);

        // If preview is dirty, we need to redeploy
        if (this.previewDirty) {
            this.creatingVoice = true;
            try {
                // Delete existing voice if it exists
                try {
                    console.log('[SoundDesigner] Deleting old voice...');
                    await fetch(this.runtimeUrl + '/voices/' + encodeURIComponent(voiceName), {
                        method: 'DELETE'
                    });
                } catch (e) {
                    // Voice might not exist, that's fine
                }

                // Generate and send the synthdef code to the runtime
                const code = this.generateCode();
                console.log('[SoundDesigner] Compiling synthdef...');
                try {
                    const evalRes = await fetch(this.runtimeUrl + '/eval', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ code: code })
                    });
                    const evalResult = await evalRes.json();
                    if (!evalResult.success) {
                        vscode.postMessage({
                            command: 'showError',
                            text: 'Failed to compile synth: ' + (evalResult.error || 'Unknown error')
                        });
                        return null;
                    }
                    console.log('[SoundDesigner] Synthdef compiled successfully');
                } catch (e) {
                    vscode.postMessage({
                        command: 'showError',
                        text: 'Cannot connect to VibeLang runtime. Run "vibelang --api" first.'
                    });
                    return null;
                }

                // Create preview voice using the registered synthdef
                console.log('[SoundDesigner] Creating voice...');
                try {
                    const res = await fetch(this.runtimeUrl + '/voices', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({
                            name: voiceName,
                            synth_name: synthName,
                            group_path: 'main',
                            polyphony: 8,
                            gain: 0.8,
                            params: {}
                        })
                    });
                    if (res.ok) {
                        this.previewDirty = false;  // Mark as clean
                        console.log('[SoundDesigner] Preview voice created: ' + voiceName);
                        return voiceName;
                    } else {
                        let errMsg = 'Unknown error';
                        try {
                            const err = await res.json();
                            errMsg = err.message || err.error || JSON.stringify(err);
                        } catch (e) {
                            errMsg = 'HTTP ' + res.status + ': ' + res.statusText;
                        }
                        console.error('[SoundDesigner] Voice creation failed:', errMsg);
                        vscode.postMessage({
                            command: 'showError',
                            text: 'Failed to create preview voice: ' + errMsg
                        });
                        return null;
                    }
                } catch (e) {
                    vscode.postMessage({
                        command: 'showError',
                        text: 'Network error creating voice: ' + e.message
                    });
                    return null;
                }
            } finally {
                this.creatingVoice = false;
            }
        }

        // Preview is not dirty, check if voice exists
        try {
            const res = await fetch(this.runtimeUrl + '/voices/' + encodeURIComponent(voiceName));
            if (res.ok) {
                console.log('[SoundDesigner] Voice already exists: ' + voiceName);
                return voiceName;
            }
        } catch (e) {
            // Voice doesn't exist
        }

        // Voice doesn't exist but preview isn't dirty - mark dirty and retry
        console.log('[SoundDesigner] Voice not found, marking dirty and retrying');
        this.previewDirty = true;
        return this.ensurePreviewVoice();
    }

    async playNote(midi) {
        console.log('[SoundDesigner] playNote called with midi:', midi, 'type:', typeof midi);

        // Prevent concurrent voice creation
        if (this.creatingVoice) {
            console.log('[SoundDesigner] Voice creation in progress, skipping');
            return;
        }

        // Visual state is now managed by setupPianoEvents - this function only handles audio

        if (this.runtimeConnected) {
            try {
                const voiceName = await this.ensurePreviewVoice();
                if (!voiceName) return;

                console.log('[SoundDesigner] Sending note-on for midi:', midi, 'to voice:', voiceName);
                // Use note-on for polyphonic preview
                const res = await fetch(this.runtimeUrl + '/voices/' + encodeURIComponent(voiceName) + '/note-on', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ note: midi, velocity: 100 })
                });

                if (!res.ok) {
                    let errMsg = 'Unknown error';
                    try {
                        const err = await res.json();
                        errMsg = err.message || err.error || JSON.stringify(err);
                    } catch (e) {
                        errMsg = 'HTTP ' + res.status;
                    }
                    console.error('[SoundDesigner] Note-on failed:', errMsg);
                    vscode.postMessage({
                        command: 'showError',
                        text: 'Note-on failed: ' + errMsg
                    });
                }
            } catch (e) {
                console.error('[SoundDesigner] Preview failed:', e);
                vscode.postMessage({
                    command: 'showError',
                    text: 'Preview error: ' + e.message
                });
            }
        } else {
            const freq = 440 * Math.pow(2, (midi - 69) / 12);
            vscode.postMessage({
                command: 'showInfo',
                text: \`Note: \${midi} (freq: \${freq.toFixed(1)} Hz) - Run "vibelang --api" to enable preview\`
            });
        }
    }

    async stopNote(midi) {
        // Visual state is now managed by setupPianoEvents - this function only handles audio

        if (this.runtimeConnected) {
            try {
                const synthName = document.getElementById('synthName').value;
                const voiceName = '__preview_' + synthName;

                await fetch(this.runtimeUrl + '/voices/' + encodeURIComponent(voiceName) + '/note-off', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ note: midi })
                });
            } catch (e) {
                // Ignore errors on note off
            }
        }
    }

    // ===========================================
    // Runtime Connection
    // ===========================================

    async checkRuntimeConnection() {
        const status = document.getElementById('connectionStatus');
        const dot = status.querySelector('.status-dot');
        const text = status.querySelector('.status-text');
        const hint = document.getElementById('previewHint');

        try {
            const res = await fetch(this.runtimeUrl + '/transport', {
                signal: AbortSignal.timeout(1000)
            });

            if (res.ok) {
                this.runtimeConnected = true;
                dot.className = 'status-dot connected';
                text.textContent = 'Connected';
                hint.textContent = 'Click piano keys to preview your synth';
                hint.className = 'preview-hint connected';
            } else {
                throw new Error('Not OK');
            }
        } catch (e) {
            this.runtimeConnected = false;
            dot.className = 'status-dot disconnected';
            text.textContent = 'Disconnected';
            hint.textContent = 'Run vibelang with --api to enable preview';
            hint.className = 'preview-hint';
        }
    }

    // ===========================================
    // Animations
    // ===========================================

    startAnimations() {
        const animate = () => {
            this.lfoPhase += 0.02;
            this.drawLfo();
            requestAnimationFrame(animate);
        };
        animate();
    }

    // ===========================================
    // Code Generation
    // ===========================================

    updateCode() {
        const code = this.generateCode();
        const el = document.getElementById('codeOutput');
        el.innerHTML = '<code>' + this.highlight(code) + '</code>';

        // Mark preview as needing redeployment
        this.previewDirty = true;
    }

    generateCode() {
        const name = document.getElementById('synthName').value || 'my_synth';
        const userParams = this.nodes.filter(n => n.type === 'param');
        const output = this.nodes.find(n => n.type === 'output');

        if (!output) return '// Add nodes and connect to Output';

        // Find all envelope and LFO nodes
        const envNodes = this.nodes.filter(n => n.envelope);
        const lfoNodes = this.nodes.filter(n => n.lfo);

        let code = '// VibeLang Synthesizer\\n';
        code += '// Generated by Sound Designer\\n\\n';
        code += 'define_synthdef("' + name + '", |builder| {\\n';
        code += '    builder\\n';

        // Essential parameters for note-on/note-off (always included)
        const essentialParams = [
            { name: 'freq', value: 440.0 },
            { name: 'gate', value: 1.0 },
            { name: 'velocity', value: 1.0 }
        ];

        // Filter out essential params if user already defined them
        const userParamNames = userParams.map(p => p.name);
        const finalEssentialParams = essentialParams.filter(p => !userParamNames.includes(p.name));

        // Add essential params first
        finalEssentialParams.forEach(p => {
            code += '        .param("' + p.name + '", ' + p.value.toFixed(1) + ')\\n';
        });

        // Add user-defined parameters (ensure float format for Rhai)
        userParams.forEach(p => {
            const val = parseFloat(p.value);
            code += '        .param("' + p.name + '", ' + (Number.isInteger(val) ? val.toFixed(1) : val) + ')\\n';
        });

        // Combine all param names for the closure
        const paramNames = [...finalEssentialParams.map(p => p.name), ...userParamNames];
        code += '        .body(|' + paramNames.join(', ') + '| {\\n\\n';

        // Envelopes (using modern envelope() builder API)
        envNodes.forEach((node, i) => {
            const env = node.envelope;
            const varName = i === 0 ? 'env' : 'env' + i;

            code += '            let ' + varName + ' = envelope()\\n';
            if (env.type === 'adsr') {
                code += '                .adsr(' + env.a + ', ' + env.d + ', ' + env.s + ', ' + env.r + ')\\n';
            } else if (env.type === 'asr') {
                code += '                .asr(' + env.a + ', ' + env.s + ', ' + env.r + ')\\n';
            } else {
                code += '                .perc(' + env.a + ', ' + env.r + ')\\n';
            }
            code += '                .gate(gate)\\n';
            code += '                .cleanup_on_finish()\\n';
            code += '                .build();\\n\\n';
        });

        // LFOs
        lfoNodes.forEach((node, i) => {
            const lfo = node.lfo;
            const varName = i === 0 ? 'lfo' : 'lfo' + i;
            let func = 'sin_osc_kr';
            if (lfo.wave === 'saw') func = 'lf_saw_kr';
            else if (lfo.wave === 'tri') func = 'lf_tri_kr';
            else if (lfo.wave === 'square') func = 'lf_pulse_kr';

            code += '            let ' + varName + ' = ' + func + '(' + lfo.rate + ') * ' + lfo.depth + ';\\n\\n';
        });

        // Signal chain
        code += this.generateSignalChain();

        code += '        })\\n';
        code += '});\\n';

        return code;
    }

    generateSignalChain() {
        const output = this.nodes.find(n => n.type === 'output');
        if (!output) return '            0.0\\n';

        const visited = new Set();
        const varNames = new Map();
        let varCounter = 0;
        let code = '';

        const getVar = (nodeId) => {
            if (!varNames.has(nodeId)) {
                varNames.set(nodeId, 'sig' + (varCounter++));
            }
            return varNames.get(nodeId);
        };

        const process = (nodeId) => {
            if (visited.has(nodeId)) return getVar(nodeId);
            visited.add(nodeId);

            const node = this.nodes.find(n => n.id === nodeId);
            if (!node) return '0.0';

            if (node.type === 'param') return node.name;

            if (node.type === 'output') {
                const conn = this.connections.find(c => c.to.node === nodeId);
                return conn ? process(conn.from.node) : '0.0';
            }

            const varName = getVar(nodeId);
            const incoming = this.connections.filter(c => c.to.node === nodeId);

            // Build arguments
            const args = [];
            if (node.inputs) {
                node.inputs.forEach((inp, i) => {
                    const conn = incoming.find(c => c.to.port === i);
                    if (conn) {
                        args.push(process(conn.from.node));
                    } else {
                        args.push(node.params[inp.name] ?? inp.default);
                    }
                });
            }

            let expr = '';

            // Handle special nodes
            if (node.name === 'Add') {
                // Filter out default zeros for cleaner code
                const nonZeroArgs = args.filter(a => a !== 0 && a !== '0');
                if (nonZeroArgs.length === 0) expr = '0.0';
                else if (nonZeroArgs.length === 1) expr = String(nonZeroArgs[0]);
                else expr = '(' + nonZeroArgs.join(' + ') + ')';
            } else if (node.name === 'Mul') {
                // Filter out default ones for cleaner code
                const nonOneArgs = args.filter(a => a !== 1 && a !== '1');
                if (nonOneArgs.length === 0) expr = '1.0';
                else if (nonOneArgs.length === 1) expr = String(nonOneArgs[0]);
                else expr = '(' + nonOneArgs.join(' * ') + ')';
            } else if (node.name === 'Scale') {
                expr = '((' + args[0] + ') * ' + args[1] + ' + ' + args[2] + ')';
            } else if (node.name === 'Mix') {
                expr = '((' + args[0] + ') * (1.0 - ' + args[2] + ') + (' + args[1] + ') * ' + args[2] + ')';
            } else if (node.name === 'Const') {
                expr = String(args[0]);
            } else if (node.envelope) {
                const idx = this.nodes.filter(n => n.envelope).indexOf(node);
                expr = idx === 0 ? 'env' : 'env' + idx;
            } else if (node.lfo) {
                const idx = this.nodes.filter(n => n.lfo).indexOf(node);
                expr = idx === 0 ? 'lfo' : 'lfo' + idx;
            } else {
                // Regular UGen
                const funcName = this.toSnakeCase(node.name) + '_' + node.rate;
                expr = funcName + '(' + args.join(', ') + ')';
            }

            code += '            let ' + varName + ' = ' + expr + ';\\n';
            return varName;
        };

        const outputConn = this.connections.find(c => c.to.node === output.id);
        if (outputConn) {
            const finalVar = process(outputConn.from.node);
            code += '\\n            ' + finalVar + '\\n';
        } else {
            code += '            0.0 // Connect to Output\\n';
        }

        return code;
    }

    toSnakeCase(str) {
        return str.replace(/([A-Z])/g, (m, p, o) => (o > 0 ? '_' : '') + p.toLowerCase());
    }

    highlight(code) {
        return code
            .replace(/\\/\\/.*$/gm, '<span class="cmt">$&</span>')
            .replace(/\\b(define_synthdef|let|fn|if|else|true|false)\\b/g, '<span class="kw">$1</span>')
            .replace(/\\b(builder|envelope|adsr|asr|perc|gate|cleanup_on_finish|build|sin_osc_ar|sin_osc_kr|saw_ar|lf_saw_kr|lf_tri_kr|lf_pulse_kr|lpf_ar|rlpf_ar|param|body)\\b/g, '<span class="fn">$1</span>')
            .replace(/"([^"]*)"/g, '<span class="str">"$1"</span>')
            .replace(/\\b(\\d+\\.\\d+|\\d+)\\b/g, '<span class="num">$1</span>');
    }

    // ===========================================
    // Actions
    // ===========================================

    newPatch() {
        if (confirm('Create new patch? Unsaved changes will be lost.')) {
            this.nodes = [];
            this.connections = [];
            this.nextId = 1;
            this.selectedNode = null;

            document.getElementById('nodesContainer').innerHTML = '';
            this.renderCables();
            this.addInitialNodes();
            this.updateInspector();
            this.updateCode();
        }
    }

    savePreset() {
        const data = JSON.stringify({
            name: document.getElementById('synthName').value,
            nodes: this.nodes,
            connections: this.connections
        }, null, 2);
        vscode.postMessage({ command: 'savePreset', data });
    }

    loadPreset() {
        vscode.postMessage({ command: 'loadPreset' });
    }

    openInEditor() {
        const code = this.generateCode();
        vscode.postMessage({ command: 'generateCode', code });
    }

    copyCode() {
        const code = this.generateCode();
        navigator.clipboard.writeText(code).then(() => {
            vscode.postMessage({ command: 'showInfo', text: 'Code copied to clipboard!' });
        });
    }

    zoomIn() {
        this.zoom = Math.min(2, this.zoom + 0.1);
        document.getElementById('zoomLevel').textContent = Math.round(this.zoom * 100) + '%';
    }

    zoomOut() {
        this.zoom = Math.max(0.25, this.zoom - 0.1);
        document.getElementById('zoomLevel').textContent = Math.round(this.zoom * 100) + '%';
    }
}

// Handle messages from extension
window.addEventListener('message', event => {
    const message = event.data;

    if (message.command === 'presetLoaded') {
        try {
            const data = JSON.parse(message.data);
            document.getElementById('synthName').value = data.name;
            app.nodes = data.nodes;
            app.connections = data.connections;
            app.selectedNode = null;

            // Update nextId to avoid ID conflicts with loaded nodes
            let maxId = 0;
            data.nodes.forEach(n => {
                const match = n.id.match(/^node_(\d+)$/);
                if (match) {
                    const id = parseInt(match[1], 10);
                    if (id > maxId) maxId = id;
                }
            });
            app.nextId = maxId + 1;

            document.getElementById('nodesContainer').innerHTML = '';
            data.nodes.forEach(n => app.renderNode(n));
            app.renderCables();
            app.updateInspector();
            app.updateCode();
        } catch (e) {
            console.error('Failed to load preset:', e);
        }
    } else if (message.type === 'connectionStatus') {
        // Update connection status from StateStore
        app.runtimeConnected = message.connected;
        if (message.baseUrl) {
            app.runtimeUrl = message.baseUrl;
        }
        // Update UI
        const hint = document.getElementById('previewHint');
        if (hint) {
            if (app.runtimeConnected) {
                hint.textContent = 'Click piano keys to preview your synth';
                hint.className = 'preview-hint connected';
            } else {
                hint.textContent = 'Run vibelang with --api to enable preview';
                hint.className = 'preview-hint';
            }
        }
    }
});

const app = new SoundDesigner();
        `;
    }
    dispose() {
        SoundDesignerPanel.currentPanel = undefined;
        this._panel.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
exports.SoundDesignerPanel = SoundDesignerPanel;
SoundDesignerPanel.viewType = 'vibelang.soundDesigner';
//# sourceMappingURL=soundDesigner.js.map