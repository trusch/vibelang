/**
 * VibeLang Effect Rack
 *
 * A visual representation of effects chains for groups.
 * Displays effects in a rack-style layout similar to DAWs.
 *
 * Features:
 * - Visual effect chain display
 * - Parameter editing with knobs/sliders
 * - Drag to reorder effects
 * - Add/remove effects
 * - Bypass toggle
 */

import * as vscode from 'vscode';
import { StateStore } from '../state/stateStore';
import { Group, Effect, SynthDef, SessionState } from '../api/types';

export class EffectRack {
    public static currentPanel: EffectRack | undefined;
    public static readonly viewType = 'vibelang.effectRack';

    private readonly _panel: vscode.WebviewPanel;
    private readonly _store: StateStore;
    private _disposables: vscode.Disposable[] = [];
    private _selectedGroup: string | null = null;

    private constructor(panel: vscode.WebviewPanel, store: StateStore, groupPath?: string) {
        this._panel = panel;
        this._store = store;
        this._selectedGroup = groupPath || null;

        this._updateContent();

        // Listen for state updates
        this._disposables.push(
            store.onFullUpdate(() => this._sendStateUpdate())
        );

        this._disposables.push(
            store.onStatusChange(() => this._updateContent())
        );

        // Handle messages from webview
        this._panel.webview.onDidReceiveMessage(
            (message) => this._handleMessage(message),
            null,
            this._disposables
        );

        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    }

    public static createOrShow(store: StateStore, groupPath?: string) {
        const column = vscode.ViewColumn.Beside;

        if (EffectRack.currentPanel) {
            EffectRack.currentPanel._selectedGroup = groupPath || null;
            EffectRack.currentPanel._panel.reveal(column);
            EffectRack.currentPanel._sendStateUpdate();
            return;
        }

        const panel = vscode.window.createWebviewPanel(
            EffectRack.viewType,
            'Effect Rack',
            column,
            {
                enableScripts: true,
                retainContextWhenHidden: true,
            }
        );

        EffectRack.currentPanel = new EffectRack(panel, store, groupPath);
    }

    public static revive(panel: vscode.WebviewPanel, store: StateStore) {
        EffectRack.currentPanel = new EffectRack(panel, store);
    }

    private _updateContent() {
        this._panel.webview.html = this._getHtmlContent();
        setTimeout(() => this._sendStateUpdate(), 100);
    }

    private _sendStateUpdate() {
        const state = this._store.state;
        if (state) {
            // Get effects for selected group
            const groupEffects = this._selectedGroup
                ? state.effects.filter(e => e.group_path === this._selectedGroup)
                : [];

            // Get available effect synthdefs
            const effectSynthdefs = state.synthdefs.filter(s =>
                s.name.startsWith('fx_') ||
                s.name.includes('reverb') ||
                s.name.includes('delay') ||
                s.name.includes('filter') ||
                s.name.includes('chorus') ||
                s.name.includes('comp') ||
                s.name.includes('eq') ||
                s.name.includes('dist')
            );

            this._panel.webview.postMessage({
                type: 'stateUpdate',
                data: {
                    groups: state.groups,
                    effects: groupEffects,
                    allEffects: state.effects,
                    synthdefs: effectSynthdefs,
                    selectedGroup: this._selectedGroup,
                },
            });
        }
    }

    private async _handleMessage(message: { command: string; [key: string]: unknown }) {
        switch (message.command) {
            case 'selectGroup':
                this._selectedGroup = message.groupPath as string;
                this._sendStateUpdate();
                break;

            case 'updateEffectParam':
                const effectId = message.effectId as string;
                const paramName = message.param as string;
                const paramValue = message.value as number;
                try {
                    await this._store.runtime.updateEffect(effectId, {
                        params: { [paramName]: paramValue }
                    });
                } catch (err) {
                    vscode.window.showErrorMessage(`Failed to update parameter: ${err}`);
                }
                break;

            case 'insertEffectCode':
                await this._insertEffectCode(
                    message.synthdefName as string,
                    message.groupPath as string
                );
                break;

            case 'goToSource':
                const location = message.sourceLocation as { file?: string; line?: number; column?: number };
                if (location?.file && location?.line) {
                    vscode.commands.executeCommand('vibelang.goToSource', location);
                }
                break;
        }
    }

    private async _insertEffectCode(synthdefName: string, groupPath: string) {
        const synthdef = this._store.state?.synthdefs.find(s => s.name === synthdefName);
        if (!synthdef) return;

        // Generate effect code
        const paramsCode = synthdef.params
            .filter(p => !['out', 'in', 'bus_in', 'bus_out', 'amp'].includes(p.name))
            .slice(0, 4)
            .map(p => `    .param("${p.name}", ${p.default_value})`)
            .join('\n');

        const groupName = groupPath.split('/').pop() || 'my_group';
        const code = `${groupName}.add_effect("${synthdef.name}")
${paramsCode};`;

        const editor = vscode.window.activeTextEditor;
        if (editor) {
            await editor.edit(edit => {
                edit.insert(editor.selection.active, code + '\n');
            });
        } else {
            await vscode.env.clipboard.writeText(code);
            vscode.window.showInformationMessage('Code copied to clipboard');
        }
    }

    private _getHtmlContent(): string {
        if (this._store.status !== 'connected') {
            return this._getDisconnectedHtml();
        }

        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Effect Rack</title>
    <style>
        :root {
            --bg-primary: #1a1a1a;
            --bg-secondary: #232323;
            --bg-tertiary: #2d2d2d;
            --bg-rack: #1e1e1e;
            --text-primary: #d4d4d4;
            --text-secondary: #858585;
            --text-muted: #5a5a5a;
            --accent-green: #9bbb59;
            --accent-orange: #d19a66;
            --accent-blue: #569cd6;
            --accent-purple: #c586c0;
            --accent-red: #d16969;
            --border: #3c3c3c;
        }

        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: 12px;
            height: 100vh;
            display: flex;
            flex-direction: column;
        }

        /* Header */
        .header {
            display: flex;
            align-items: center;
            gap: 12px;
            padding: 10px 16px;
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border);
        }

        .header-title {
            font-weight: 600;
            font-size: 13px;
        }

        .group-selector {
            padding: 6px 10px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: 11px;
            min-width: 150px;
        }

        .group-selector:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        .add-btn {
            padding: 6px 12px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: var(--bg-tertiary);
            color: var(--text-primary);
            cursor: pointer;
            font-size: 11px;
            margin-left: auto;
        }

        .add-btn:hover {
            background: #3a3a3a;
            border-color: var(--text-secondary);
        }

        /* Main Content */
        .content {
            flex: 1;
            overflow-y: auto;
            padding: 16px;
        }

        /* Effect Rack */
        .rack {
            display: flex;
            flex-direction: column;
            gap: 12px;
        }

        /* Effect Unit */
        .effect-unit {
            background: var(--bg-secondary);
            border-radius: 8px;
            border: 1px solid var(--border);
            overflow: hidden;
        }

        .effect-header {
            display: flex;
            align-items: center;
            gap: 10px;
            padding: 10px 14px;
            background: var(--bg-tertiary);
            border-bottom: 1px solid var(--border);
        }

        .effect-power {
            width: 12px;
            height: 12px;
            border-radius: 50%;
            background: var(--accent-green);
            box-shadow: 0 0 6px var(--accent-green);
            cursor: pointer;
        }

        .effect-power.bypassed {
            background: var(--text-muted);
            box-shadow: none;
        }

        .effect-name {
            font-weight: 600;
            font-size: 13px;
            flex: 1;
        }

        .effect-type {
            font-size: 10px;
            color: var(--text-muted);
            text-transform: uppercase;
            padding: 2px 6px;
            background: var(--bg-primary);
            border-radius: 3px;
        }

        .effect-actions {
            display: flex;
            gap: 4px;
            opacity: 0;
            transition: opacity 0.1s ease;
        }

        .effect-unit:hover .effect-actions {
            opacity: 1;
        }

        .effect-btn {
            width: 22px;
            height: 22px;
            border: none;
            border-radius: 3px;
            background: var(--bg-primary);
            color: var(--text-secondary);
            cursor: pointer;
            font-size: 10px;
        }

        .effect-btn:hover {
            background: var(--accent-blue);
            color: white;
        }

        /* Parameters Grid */
        .params-grid {
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
            gap: 16px;
            padding: 16px;
        }

        /* Knob Control */
        .param-control {
            display: flex;
            flex-direction: column;
            align-items: center;
            gap: 6px;
        }

        .knob-container {
            position: relative;
            width: 48px;
            height: 48px;
        }

        .knob {
            width: 48px;
            height: 48px;
            border-radius: 50%;
            background: linear-gradient(180deg, #3a3a3a 0%, #2a2a2a 100%);
            border: 2px solid var(--border);
            cursor: pointer;
            position: relative;
        }

        .knob-indicator {
            position: absolute;
            width: 2px;
            height: 14px;
            background: var(--accent-blue);
            top: 6px;
            left: 50%;
            transform-origin: center 18px;
            border-radius: 1px;
        }

        .knob-value {
            position: absolute;
            bottom: -20px;
            left: 50%;
            transform: translateX(-50%);
            font-size: 10px;
            color: var(--text-secondary);
            font-family: 'SF Mono', Monaco, monospace;
        }

        .param-name {
            font-size: 10px;
            color: var(--text-secondary);
            text-align: center;
            max-width: 80px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
        }

        /* Empty State */
        .empty-state {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            padding: 60px 40px;
            color: var(--text-secondary);
            text-align: center;
        }

        .empty-icon {
            font-size: 48px;
            margin-bottom: 16px;
            opacity: 0.5;
        }

        .empty-state h3 {
            font-size: 14px;
            font-weight: 500;
            margin-bottom: 8px;
        }

        .empty-state p {
            font-size: 12px;
            max-width: 300px;
            line-height: 1.5;
        }

        /* Effect Picker */
        .effect-picker {
            position: fixed;
            top: 50%;
            left: 50%;
            transform: translate(-50%, -50%);
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 8px;
            box-shadow: 0 8px 32px rgba(0,0,0,0.5);
            z-index: 1000;
            width: 320px;
            max-height: 400px;
            display: none;
        }

        .effect-picker.visible {
            display: block;
        }

        .effect-picker-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 12px 16px;
            border-bottom: 1px solid var(--border);
            font-weight: 600;
        }

        .effect-picker-close {
            cursor: pointer;
            color: var(--text-muted);
        }

        .effect-picker-close:hover {
            color: var(--text-primary);
        }

        .effect-picker-search {
            padding: 12px 16px;
            border-bottom: 1px solid var(--border);
        }

        .effect-picker-search input {
            width: 100%;
            padding: 8px 12px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: 12px;
        }

        .effect-picker-search input:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        .effect-picker-list {
            max-height: 260px;
            overflow-y: auto;
        }

        .effect-picker-item {
            padding: 10px 16px;
            cursor: pointer;
            display: flex;
            align-items: center;
            gap: 10px;
        }

        .effect-picker-item:hover {
            background: var(--bg-tertiary);
        }

        .effect-picker-icon {
            width: 28px;
            height: 28px;
            border-radius: 4px;
            background: rgba(197, 134, 192, 0.2);
            display: flex;
            align-items: center;
            justify-content: center;
            color: var(--accent-purple);
            font-size: 12px;
        }

        .effect-picker-info {
            flex: 1;
        }

        .effect-picker-name {
            font-weight: 500;
        }

        .effect-picker-desc {
            font-size: 10px;
            color: var(--text-muted);
        }

        /* Signal Flow */
        .signal-flow {
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 8px;
            color: var(--text-muted);
        }

        .signal-arrow {
            width: 24px;
            height: 2px;
            background: var(--border);
            position: relative;
        }

        .signal-arrow::after {
            content: '▶';
            position: absolute;
            right: -8px;
            top: 50%;
            transform: translateY(-50%);
            font-size: 8px;
        }

        /* Scrollbar */
        ::-webkit-scrollbar {
            width: 8px;
            height: 8px;
        }

        ::-webkit-scrollbar-track {
            background: var(--bg-primary);
        }

        ::-webkit-scrollbar-thumb {
            background: var(--bg-tertiary);
            border-radius: 4px;
        }

        ::-webkit-scrollbar-thumb:hover {
            background: #404040;
        }
    </style>
</head>
<body>
    <div class="header">
        <span class="header-title">Effect Rack</span>
        <select class="group-selector" id="groupSelector">
            <option value="">Select Group...</option>
        </select>
        <button class="add-btn" id="addEffectBtn">+ Add Effect</button>
    </div>

    <div class="content" id="content">
        <div class="empty-state">
            <div class="empty-icon">🎛️</div>
            <h3>Select a Group</h3>
            <p>Choose a group from the dropdown to view and edit its effect chain.</p>
        </div>
    </div>

    <!-- Effect Picker -->
    <div class="effect-picker" id="effectPicker">
        <div class="effect-picker-header">
            <span>Add Effect</span>
            <span class="effect-picker-close" id="pickerClose">✕</span>
        </div>
        <div class="effect-picker-search">
            <input type="text" id="pickerSearch" placeholder="Search effects...">
        </div>
        <div class="effect-picker-list" id="pickerList"></div>
    </div>

    <script>
        const vscode = acquireVsCodeApi();

        let state = {
            groups: [],
            effects: [],
            allEffects: [],
            synthdefs: [],
            selectedGroup: null
        };

        // Group selector
        const groupSelector = document.getElementById('groupSelector');
        groupSelector.addEventListener('change', (e) => {
            vscode.postMessage({
                command: 'selectGroup',
                groupPath: e.target.value
            });
        });

        // Add effect button
        document.getElementById('addEffectBtn').addEventListener('click', () => {
            if (state.selectedGroup) {
                showEffectPicker();
            } else {
                alert('Please select a group first');
            }
        });

        // Picker close
        document.getElementById('pickerClose').addEventListener('click', hideEffectPicker);

        // Picker search
        document.getElementById('pickerSearch').addEventListener('input', (e) => {
            renderEffectPicker(e.target.value.toLowerCase());
        });

        // Click outside picker to close
        document.addEventListener('click', (e) => {
            const picker = document.getElementById('effectPicker');
            const addBtn = document.getElementById('addEffectBtn');
            if (!picker.contains(e.target) && !addBtn.contains(e.target)) {
                hideEffectPicker();
            }
        });

        // Message handler
        window.addEventListener('message', (event) => {
            const message = event.data;
            if (message.type === 'stateUpdate') {
                state = message.data;
                render();
            }
        });

        function render() {
            renderGroupSelector();
            renderEffectRack();
        }

        function renderGroupSelector() {
            const currentValue = groupSelector.value;
            groupSelector.innerHTML = '<option value="">Select Group...</option>' +
                state.groups.map(g => \`
                    <option value="\${g.path}" \${g.path === state.selectedGroup ? 'selected' : ''}>
                        \${g.path}
                    </option>
                \`).join('');
        }

        function renderEffectRack() {
            const content = document.getElementById('content');

            if (!state.selectedGroup) {
                content.innerHTML = \`
                    <div class="empty-state">
                        <div class="empty-icon">🎛️</div>
                        <h3>Select a Group</h3>
                        <p>Choose a group from the dropdown to view and edit its effect chain.</p>
                    </div>
                \`;
                return;
            }

            if (state.effects.length === 0) {
                content.innerHTML = \`
                    <div class="empty-state">
                        <div class="empty-icon">🔊</div>
                        <h3>No Effects</h3>
                        <p>This group has no effects. Click "Add Effect" to add one.</p>
                    </div>
                \`;
                return;
            }

            content.innerHTML = '<div class="rack">' +
                state.effects.map((effect, index) => renderEffectUnit(effect, index)).join('') +
                '</div>';

            // Add event listeners for knobs
            setupKnobInteractions();
        }

        function renderEffectUnit(effect, index) {
            const synthdef = state.synthdefs.find(s => s.name === effect.synthdef_name);
            const displayParams = Object.entries(effect.params)
                .filter(([name]) => !['out', 'in', 'bus_in', 'bus_out'].includes(name))
                .slice(0, 6);

            const effectType = categorizeEffect(effect.synthdef_name);

            return \`
                \${index > 0 ? '<div class="signal-flow"><div class="signal-arrow"></div></div>' : ''}
                <div class="effect-unit" data-effect-id="\${effect.id}">
                    <div class="effect-header">
                        <div class="effect-power" title="Bypass"></div>
                        <span class="effect-name">\${formatEffectName(effect.synthdef_name)}</span>
                        <span class="effect-type">\${effectType}</span>
                        <div class="effect-actions">
                            <button class="effect-btn" data-action="source" title="Go to Source">📍</button>
                        </div>
                    </div>
                    <div class="params-grid">
                        \${displayParams.map(([name, value]) => renderParamKnob(effect.id, name, value, synthdef)).join('')}
                    </div>
                </div>
            \`;
        }

        function renderParamKnob(effectId, paramName, value, synthdef) {
            const paramDef = synthdef?.params.find(p => p.name === paramName);
            const min = paramDef?.min_value ?? 0;
            const max = paramDef?.max_value ?? 1;
            const normalized = (value - min) / (max - min);
            const rotation = -135 + normalized * 270; // -135 to 135 degrees

            return \`
                <div class="param-control">
                    <div class="knob-container">
                        <div class="knob" data-effect-id="\${effectId}" data-param="\${paramName}"
                             data-min="\${min}" data-max="\${max}" data-value="\${value}">
                            <div class="knob-indicator" style="transform: translateX(-50%) rotate(\${rotation}deg)"></div>
                        </div>
                        <span class="knob-value">\${formatValue(value)}</span>
                    </div>
                    <span class="param-name">\${paramName}</span>
                </div>
            \`;
        }

        function setupKnobInteractions() {
            document.querySelectorAll('.knob').forEach(knob => {
                let isDragging = false;
                let startY = 0;
                let startValue = 0;

                knob.addEventListener('mousedown', (e) => {
                    isDragging = true;
                    startY = e.clientY;
                    startValue = parseFloat(knob.dataset.value);
                    e.preventDefault();
                });

                document.addEventListener('mousemove', (e) => {
                    if (!isDragging) return;

                    const deltaY = startY - e.clientY;
                    const min = parseFloat(knob.dataset.min);
                    const max = parseFloat(knob.dataset.max);
                    const range = max - min;
                    const sensitivity = range / 100;

                    let newValue = startValue + deltaY * sensitivity;
                    newValue = Math.max(min, Math.min(max, newValue));

                    // Update visual
                    const normalized = (newValue - min) / range;
                    const rotation = -135 + normalized * 270;
                    knob.querySelector('.knob-indicator').style.transform = \`translateX(-50%) rotate(\${rotation}deg)\`;
                    knob.parentElement.querySelector('.knob-value').textContent = formatValue(newValue);
                    knob.dataset.value = newValue;

                    // Send update
                    vscode.postMessage({
                        command: 'updateEffectParam',
                        effectId: knob.dataset.effectId,
                        param: knob.dataset.param,
                        value: newValue
                    });
                });

                document.addEventListener('mouseup', () => {
                    isDragging = false;
                });
            });

            // Source buttons
            document.querySelectorAll('.effect-btn[data-action="source"]').forEach(btn => {
                btn.addEventListener('click', () => {
                    const effectUnit = btn.closest('.effect-unit');
                    const effectId = effectUnit.dataset.effectId;
                    const effect = state.effects.find(e => e.id === effectId);
                    if (effect?.source_location) {
                        vscode.postMessage({
                            command: 'goToSource',
                            sourceLocation: effect.source_location
                        });
                    }
                });
            });
        }

        function showEffectPicker() {
            document.getElementById('effectPicker').classList.add('visible');
            document.getElementById('pickerSearch').focus();
            renderEffectPicker('');
        }

        function hideEffectPicker() {
            document.getElementById('effectPicker').classList.remove('visible');
            document.getElementById('pickerSearch').value = '';
        }

        function renderEffectPicker(query) {
            const list = document.getElementById('pickerList');
            const filtered = state.synthdefs.filter(s => {
                if (!query) return true;
                return s.name.toLowerCase().includes(query) ||
                       categorizeEffect(s.name).toLowerCase().includes(query);
            });

            if (filtered.length === 0) {
                list.innerHTML = '<div style="padding: 16px; text-align: center; color: var(--text-muted);">No effects found</div>';
                return;
            }

            list.innerHTML = filtered.map(synthdef => \`
                <div class="effect-picker-item" data-name="\${synthdef.name}">
                    <div class="effect-picker-icon">\${getEffectIcon(synthdef.name)}</div>
                    <div class="effect-picker-info">
                        <div class="effect-picker-name">\${formatEffectName(synthdef.name)}</div>
                        <div class="effect-picker-desc">\${categorizeEffect(synthdef.name)} • \${synthdef.params.length} params</div>
                    </div>
                </div>
            \`).join('');

            // Event listeners
            list.querySelectorAll('.effect-picker-item').forEach(item => {
                item.addEventListener('click', () => {
                    vscode.postMessage({
                        command: 'insertEffectCode',
                        synthdefName: item.dataset.name,
                        groupPath: state.selectedGroup
                    });
                    hideEffectPicker();
                });
            });
        }

        function formatEffectName(name) {
            return name
                .replace(/^fx_/, '')
                .replace(/_/g, ' ')
                .replace(/\\b\\w/g, l => l.toUpperCase());
        }

        function categorizeEffect(name) {
            const lower = name.toLowerCase();
            if (lower.includes('reverb') || lower.includes('verb')) return 'Reverb';
            if (lower.includes('delay') || lower.includes('echo')) return 'Delay';
            if (lower.includes('filter') || lower.includes('lpf') || lower.includes('hpf')) return 'Filter';
            if (lower.includes('chorus') || lower.includes('flanger') || lower.includes('phaser')) return 'Modulation';
            if (lower.includes('comp') || lower.includes('limit')) return 'Dynamics';
            if (lower.includes('eq') || lower.includes('shelf')) return 'EQ';
            if (lower.includes('dist') || lower.includes('sat') || lower.includes('drive')) return 'Distortion';
            return 'Effect';
        }

        function getEffectIcon(name) {
            const category = categorizeEffect(name);
            const icons = {
                'Reverb': '🌊',
                'Delay': '⏱️',
                'Filter': '📊',
                'Modulation': '〰️',
                'Dynamics': '📈',
                'EQ': '⚖️',
                'Distortion': '⚡',
            };
            return icons[category] || '🎛️';
        }

        function formatValue(value) {
            if (Math.abs(value) < 0.01) return value.toExponential(1);
            if (Math.abs(value) >= 1000) return value.toFixed(0);
            if (Math.abs(value) >= 1) return value.toFixed(2);
            return value.toFixed(3);
        }

        // Initial render
        render();
    </script>
</body>
</html>`;
    }

    private _getDisconnectedHtml(): string {
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Effect Rack</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #1a1a1a;
            color: #d4d4d4;
            display: flex;
            align-items: center;
            justify-content: center;
            height: 100vh;
            margin: 0;
        }
        .empty-state {
            text-align: center;
            color: #858585;
        }
        .empty-icon {
            font-size: 64px;
            margin-bottom: 20px;
            opacity: 0.3;
        }
        h2 {
            font-size: 18px;
            font-weight: 500;
            margin-bottom: 8px;
            color: #d4d4d4;
        }
        p {
            max-width: 400px;
            line-height: 1.5;
        }
    </style>
</head>
<body>
    <div class="empty-state">
        <div class="empty-icon">🎛️</div>
        <h2>Not Connected</h2>
        <p>Connect to a VibeLang runtime to view and edit effects.</p>
    </div>
</body>
</html>`;
    }

    dispose() {
        EffectRack.currentPanel = undefined;
        this._panel.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
