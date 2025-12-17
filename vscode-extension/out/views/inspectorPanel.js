"use strict";
/**
 * VibeLang Inspector Panel
 *
 * Webview panel showing details and editable parameters for the selected entity.
 * Uses Ableton-style dark theme aesthetics.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.InspectorPanel = void 0;
const vscode = require("vscode");
class InspectorPanel {
    constructor(panel, store) {
        this._disposables = [];
        this._isInteracting = false;
        this._pendingUpdate = false;
        this._panel = panel;
        this._store = store;
        // Set initial HTML
        this._updateContent();
        // Listen for selection changes
        this._disposables.push(store.onSelectionChange(() => this._updateContent()));
        // Listen for state updates to refresh current view
        // But skip if user is currently interacting with a slider
        this._disposables.push(store.onFullUpdate(() => {
            if (this._isInteracting) {
                this._pendingUpdate = true;
            }
            else {
                this._updateContent();
            }
        }));
        // Handle messages from webview
        this._panel.webview.onDidReceiveMessage((message) => this._handleMessage(message), null, this._disposables);
        // Handle panel disposal
        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    }
    static createOrShow(store) {
        const column = vscode.ViewColumn.Beside;
        if (InspectorPanel.currentPanel) {
            InspectorPanel.currentPanel._panel.reveal(column);
            return;
        }
        const panel = vscode.window.createWebviewPanel(InspectorPanel.viewType, 'Inspector', column, {
            enableScripts: true,
            retainContextWhenHidden: true,
        });
        InspectorPanel.currentPanel = new InspectorPanel(panel, store);
    }
    static revive(panel, store) {
        InspectorPanel.currentPanel = new InspectorPanel(panel, store);
    }
    _updateContent() {
        const selection = this._store.selection;
        const entity = this._store.getSelectedEntity();
        this._panel.webview.html = this._getHtmlContent(selection, entity);
    }
    async _handleMessage(message) {
        switch (message.command) {
            case 'interactionStart':
                this._isInteracting = true;
                break;
            case 'interactionEnd':
                this._isInteracting = false;
                // Apply any pending update now that interaction is done
                if (this._pendingUpdate) {
                    this._pendingUpdate = false;
                    this._updateContent();
                }
                break;
            case 'setParam':
                await this._setParam(message.entityType, message.entityId, message.param, message.value);
                break;
            case 'mute':
                await this._toggleMute(message.entityType, message.entityId);
                break;
            case 'solo':
                await this._toggleSolo(message.entityType, message.entityId);
                break;
            case 'start':
                await this._start(message.entityType, message.entityId);
                break;
            case 'stop':
                await this._stop(message.entityType, message.entityId);
                break;
            case 'goToSource':
                const entity = this._store.getSelectedEntity();
                if (entity && 'source_location' in entity && entity.source_location) {
                    vscode.commands.executeCommand('vibelang.goToSource', entity.source_location);
                }
                break;
        }
    }
    async _setParam(entityType, entityId, param, value) {
        switch (entityType) {
            case 'group':
                await this._store.runtime.setGroupParam(entityId, param, value);
                break;
            case 'voice':
                await this._store.runtime.setVoiceParam(entityId, param, value);
                break;
            case 'effect':
                await this._store.runtime.setEffectParam(entityId, param, value);
                break;
        }
    }
    async _toggleMute(entityType, entityId) {
        const entity = this._store.getSelectedEntity();
        if (!entity || !('muted' in entity))
            return;
        switch (entityType) {
            case 'group':
                if (entity.muted) {
                    await this._store.runtime.unmuteGroup(entityId);
                }
                else {
                    await this._store.runtime.muteGroup(entityId);
                }
                break;
            case 'voice':
                if (entity.muted) {
                    await this._store.runtime.unmuteVoice(entityId);
                }
                else {
                    await this._store.runtime.muteVoice(entityId);
                }
                break;
        }
    }
    async _toggleSolo(entityType, entityId) {
        const entity = this._store.getSelectedEntity();
        if (!entity || !('soloed' in entity))
            return;
        if (entityType === 'group') {
            if (entity.soloed) {
                await this._store.runtime.unsoloGroup(entityId);
            }
            else {
                await this._store.runtime.soloGroup(entityId);
            }
        }
    }
    async _start(entityType, entityId) {
        switch (entityType) {
            case 'pattern':
                await this._store.runtime.startPattern(entityId);
                break;
            case 'melody':
                await this._store.runtime.startMelody(entityId);
                break;
            case 'sequence':
                await this._store.runtime.startSequence(entityId);
                break;
        }
    }
    async _stop(entityType, entityId) {
        switch (entityType) {
            case 'pattern':
                await this._store.runtime.stopPattern(entityId);
                break;
            case 'melody':
                await this._store.runtime.stopMelody(entityId);
                break;
            case 'sequence':
                await this._store.runtime.stopSequence(entityId);
                break;
        }
    }
    _getHtmlContent(selection, entity) {
        if (!selection || !entity) {
            return this._getEmptyHtml();
        }
        let content = '';
        switch (selection.type) {
            case 'group':
                content = this._renderGroup(entity);
                break;
            case 'voice':
                content = this._renderVoice(entity);
                break;
            case 'pattern':
                content = this._renderPattern(entity);
                break;
            case 'melody':
                content = this._renderMelody(entity);
                break;
            case 'sequence':
                content = this._renderSequence(entity);
                break;
            case 'effect':
                content = this._renderEffect(entity);
                break;
        }
        return this._wrapHtml(content);
    }
    _getEmptyHtml() {
        return this._wrapHtml(`
            <div class="empty-state">
                <div class="empty-icon">🎹</div>
                <h2>No Selection</h2>
                <p>Select an item from the Session Explorer to view its details.</p>
            </div>
        `);
    }
    _renderGroup(group) {
        const params = this._renderParams(group.params, 'group', group.path);
        return `
            <div class="entity-header">
                <div class="entity-type">GROUP</div>
                <h1>${group.name}</h1>
                <div class="entity-path">${group.path}</div>
            </div>

            <div class="controls-row">
                <button class="control-btn ${group.muted ? 'active' : ''}"
                        onclick="sendMessage({command:'mute', entityType:'group', entityId:'${group.path}'})">
                    🔇 Mute
                </button>
                <button class="control-btn ${group.soloed ? 'active' : ''}"
                        onclick="sendMessage({command:'solo', entityType:'group', entityId:'${group.path}'})">
                    ⭐ Solo
                </button>
                <button class="control-btn source-btn"
                        onclick="sendMessage({command:'goToSource'})">
                    📍 Go to Source
                </button>
            </div>

            <div class="section">
                <h2>Parameters</h2>
                ${params}
            </div>

            <div class="section">
                <h2>Info</h2>
                <div class="info-grid">
                    <div class="info-label">Audio Bus</div>
                    <div class="info-value">${group.audio_bus}</div>
                    <div class="info-label">Node ID</div>
                    <div class="info-value">${group.node_id}</div>
                    <div class="info-label">Children</div>
                    <div class="info-value">${group.children.length} groups</div>
                </div>
            </div>
        `;
    }
    _renderVoice(voice) {
        const params = this._renderParams(voice.params, 'voice', voice.name);
        return `
            <div class="entity-header">
                <div class="entity-type">VOICE</div>
                <h1>${voice.name}</h1>
                <div class="entity-synth">${voice.synth_name}</div>
            </div>

            <div class="controls-row">
                <button class="control-btn ${voice.muted ? 'active' : ''}"
                        onclick="sendMessage({command:'mute', entityType:'voice', entityId:'${voice.name}'})">
                    🔇 Mute
                </button>
                <button class="control-btn source-btn"
                        onclick="sendMessage({command:'goToSource'})">
                    📍 Go to Source
                </button>
            </div>

            <div class="section">
                <h2>Parameters</h2>
                <div class="param-row">
                    <label>Gain</label>
                    <input type="range" min="0" max="2" step="0.01" value="${voice.gain}"
                           id="slider-voice-${voice.name.replace(/[^a-zA-Z0-9-_]/g, '_')}-gain"
                           onmousedown="sendMessage({command:'interactionStart'})"
                           ontouchstart="sendMessage({command:'interactionStart'})"
                           onmouseup="sendMessage({command:'interactionEnd'})"
                           ontouchend="sendMessage({command:'interactionEnd'})"
                           oninput="updateGainValue('value-voice-${voice.name.replace(/[^a-zA-Z0-9-_]/g, '_')}-gain', this.value); sendMessage({command:'setParam', entityType:'voice', entityId:'${voice.name}', param:'gain', value:parseFloat(this.value)})">
                    <span class="param-value" id="value-voice-${voice.name.replace(/[^a-zA-Z0-9-_]/g, '_')}-gain">${(voice.gain * 100).toFixed(0)}%</span>
                </div>
                ${params}
            </div>

            <div class="section">
                <h2>Info</h2>
                <div class="info-grid">
                    <div class="info-label">Group</div>
                    <div class="info-value">${voice.group_path}</div>
                    <div class="info-label">Polyphony</div>
                    <div class="info-value">${voice.polyphony}</div>
                    ${voice.sfz_instrument ? `
                        <div class="info-label">SFZ</div>
                        <div class="info-value">${voice.sfz_instrument}</div>
                    ` : ''}
                    ${voice.vst_instrument ? `
                        <div class="info-label">VST</div>
                        <div class="info-value">${voice.vst_instrument}</div>
                    ` : ''}
                </div>
            </div>
        `;
    }
    _renderPattern(pattern) {
        const isPlaying = pattern.status.state === 'playing';
        return `
            <div class="entity-header">
                <div class="entity-type">PATTERN</div>
                <h1>${pattern.name}</h1>
                <div class="entity-status ${pattern.status.state}">${pattern.status.state.toUpperCase()}</div>
            </div>

            <div class="controls-row">
                ${isPlaying ? `
                    <button class="control-btn stop"
                            onclick="sendMessage({command:'stop', entityType:'pattern', entityId:'${pattern.name}'})">
                        ⏹ Stop
                    </button>
                ` : `
                    <button class="control-btn play"
                            onclick="sendMessage({command:'start', entityType:'pattern', entityId:'${pattern.name}'})">
                        ▶ Start
                    </button>
                `}
                <button class="control-btn source-btn"
                        onclick="sendMessage({command:'goToSource'})">
                    📍 Go to Source
                </button>
            </div>

            <div class="section">
                <h2>Info</h2>
                <div class="info-grid">
                    <div class="info-label">Voice</div>
                    <div class="info-value">${pattern.voice_name}</div>
                    <div class="info-label">Loop Length</div>
                    <div class="info-value">${pattern.loop_beats} beats</div>
                    <div class="info-label">Events</div>
                    <div class="info-value">${pattern.events.length} triggers</div>
                    <div class="info-label">Group</div>
                    <div class="info-value">${pattern.group_path}</div>
                </div>
            </div>

            <div class="section">
                <h2>Events Timeline</h2>
                <div class="mini-timeline">
                    ${this._renderMiniTimeline(pattern.events.map(e => e.beat), pattern.loop_beats)}
                </div>
            </div>
        `;
    }
    _renderMelody(melody) {
        const isPlaying = melody.status.state === 'playing';
        return `
            <div class="entity-header">
                <div class="entity-type">MELODY</div>
                <h1>${melody.name}</h1>
                <div class="entity-status ${melody.status.state}">${melody.status.state.toUpperCase()}</div>
            </div>

            <div class="controls-row">
                ${isPlaying ? `
                    <button class="control-btn stop"
                            onclick="sendMessage({command:'stop', entityType:'melody', entityId:'${melody.name}'})">
                        ⏹ Stop
                    </button>
                ` : `
                    <button class="control-btn play"
                            onclick="sendMessage({command:'start', entityType:'melody', entityId:'${melody.name}'})">
                        ▶ Start
                    </button>
                `}
                <button class="control-btn source-btn"
                        onclick="sendMessage({command:'goToSource'})">
                    📍 Go to Source
                </button>
            </div>

            <div class="section">
                <h2>Info</h2>
                <div class="info-grid">
                    <div class="info-label">Voice</div>
                    <div class="info-value">${melody.voice_name}</div>
                    <div class="info-label">Loop Length</div>
                    <div class="info-value">${melody.loop_beats} beats</div>
                    <div class="info-label">Notes</div>
                    <div class="info-value">${melody.events.length} notes</div>
                    <div class="info-label">Group</div>
                    <div class="info-value">${melody.group_path}</div>
                </div>
            </div>

            <div class="section">
                <h2>Notes</h2>
                <div class="note-list">
                    ${melody.events.slice(0, 16).map(e => `
                        <span class="note-chip">${e.note}</span>
                    `).join('')}
                    ${melody.events.length > 16 ? `<span class="more">+${melody.events.length - 16} more</span>` : ''}
                </div>
            </div>
        `;
    }
    _renderSequence(sequence) {
        const isActive = this._store.isSequenceActive(sequence.name);
        return `
            <div class="entity-header">
                <div class="entity-type">SEQUENCE</div>
                <h1>${sequence.name}</h1>
                <div class="entity-status ${isActive ? 'playing' : 'stopped'}">${isActive ? 'PLAYING' : 'STOPPED'}</div>
            </div>

            <div class="controls-row">
                ${isActive ? `
                    <button class="control-btn stop"
                            onclick="sendMessage({command:'stop', entityType:'sequence', entityId:'${sequence.name}'})">
                        ⏹ Stop
                    </button>
                ` : `
                    <button class="control-btn play"
                            onclick="sendMessage({command:'start', entityType:'sequence', entityId:'${sequence.name}'})">
                        ▶ Start
                    </button>
                `}
                <button class="control-btn source-btn"
                        onclick="sendMessage({command:'goToSource'})">
                    📍 Go to Source
                </button>
            </div>

            <div class="section">
                <h2>Info</h2>
                <div class="info-grid">
                    <div class="info-label">Loop Length</div>
                    <div class="info-value">${sequence.loop_beats} beats</div>
                    <div class="info-label">Clips</div>
                    <div class="info-value">${sequence.clips.length}</div>
                </div>
            </div>

            <div class="section">
                <h2>Clips</h2>
                <div class="clip-list">
                    ${sequence.clips.map(clip => `
                        <div class="clip-item">
                            <span class="clip-type">${clip.type}</span>
                            <span class="clip-name">${clip.name}</span>
                            <span class="clip-time">${clip.start_beat}b</span>
                        </div>
                    `).join('')}
                </div>
            </div>
        `;
    }
    _renderEffect(effect) {
        const params = this._renderParams(effect.params, 'effect', effect.id);
        return `
            <div class="entity-header">
                <div class="entity-type">EFFECT</div>
                <h1>${effect.id}</h1>
                <div class="entity-synth">${effect.synthdef_name}</div>
            </div>

            <div class="controls-row">
                <button class="control-btn source-btn"
                        onclick="sendMessage({command:'goToSource'})">
                    📍 Go to Source
                </button>
            </div>

            <div class="section">
                <h2>Parameters</h2>
                ${params}
            </div>

            <div class="section">
                <h2>Info</h2>
                <div class="info-grid">
                    <div class="info-label">Group</div>
                    <div class="info-value">${effect.group_path}</div>
                    <div class="info-label">Position</div>
                    <div class="info-value">${effect.position ?? 0}</div>
                    ${effect.vst_plugin ? `
                        <div class="info-label">VST Plugin</div>
                        <div class="info-value">${effect.vst_plugin}</div>
                    ` : ''}
                </div>
            </div>
        `;
    }
    _renderParams(params, entityType, entityId) {
        const entries = Object.entries(params);
        if (entries.length === 0) {
            return '<div class="no-params">No parameters</div>';
        }
        // Normalize and deduplicate params by lowercase key
        // This prevents duplicates like "cutoff" and "Cutoff" appearing separately
        const normalizedParams = new Map();
        for (const [key, value] of entries) {
            const lowerKey = key.toLowerCase();
            // Keep the first occurrence (typically the "correct" casing from the backend)
            if (!normalizedParams.has(lowerKey)) {
                normalizedParams.set(lowerKey, { originalKey: key, value });
            }
        }
        return Array.from(normalizedParams.values()).map(({ originalKey, value }) => {
            const sliderId = `slider-${entityType}-${entityId}-${originalKey}`.replace(/[^a-zA-Z0-9-_]/g, '_');
            const valueId = `value-${entityType}-${entityId}-${originalKey}`.replace(/[^a-zA-Z0-9-_]/g, '_');
            return `
            <div class="param-row">
                <label title="${originalKey}">${originalKey}</label>
                <input type="range" min="0" max="1" step="0.01" value="${value}"
                       id="${sliderId}"
                       onmousedown="sendMessage({command:'interactionStart'})"
                       ontouchstart="sendMessage({command:'interactionStart'})"
                       onmouseup="sendMessage({command:'interactionEnd'})"
                       ontouchend="sendMessage({command:'interactionEnd'})"
                       oninput="updateSliderValue('${valueId}', this.value); sendMessage({command:'setParam', entityType:'${entityType}', entityId:'${entityId}', param:'${originalKey}', value:parseFloat(this.value)})">
                <span class="param-value" id="${valueId}">${value.toFixed(2)}</span>
            </div>
        `;
        }).join('');
    }
    _renderMiniTimeline(beats, loopLength) {
        const width = 200;
        const markers = beats.map(beat => {
            const x = (beat / loopLength) * width;
            return `<div class="timeline-marker" style="left: ${x}px"></div>`;
        }).join('');
        return `
            <div class="timeline-track" style="width: ${width}px">
                ${markers}
            </div>
        `;
    }
    _wrapHtml(content) {
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Inspector</title>
    <style>
        :root {
            --bg-primary: #1a1a1a;
            --bg-secondary: #232323;
            --bg-tertiary: #2d2d2d;
            --bg-panel: #1e1e1e;
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
            padding: 0;
            font-size: 12px;
            line-height: 1.4;
        }

        .inspector-container {
            padding: 16px;
        }

        /* Empty State */
        .empty-state {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            height: 60vh;
            color: var(--text-secondary);
            text-align: center;
        }

        .empty-icon {
            font-size: 48px;
            margin-bottom: 16px;
            opacity: 0.5;
        }

        .empty-state h2 {
            font-size: 18px;
            font-weight: 500;
            margin-bottom: 8px;
        }

        /* Entity Header */
        .entity-header {
            margin-bottom: 20px;
            padding: 16px;
            background: linear-gradient(to bottom, var(--bg-secondary), var(--bg-primary));
            border-radius: 6px;
            border: 1px solid var(--border);
        }

        .entity-type {
            font-size: 9px;
            font-weight: 700;
            letter-spacing: 1.5px;
            color: var(--accent-green);
            margin-bottom: 6px;
            text-shadow: 0 0 10px rgba(155, 187, 89, 0.3);
        }

        .entity-header h1 {
            font-size: 22px;
            font-weight: 600;
            margin-bottom: 6px;
            letter-spacing: -0.5px;
        }

        .entity-path, .entity-synth {
            font-size: 11px;
            color: var(--text-secondary);
            font-family: 'SF Mono', Monaco, monospace;
            background: var(--bg-tertiary);
            padding: 4px 8px;
            border-radius: 3px;
            display: inline-block;
        }

        .entity-status {
            display: inline-block;
            padding: 4px 10px;
            border-radius: 12px;
            font-size: 9px;
            font-weight: 700;
            letter-spacing: 1px;
            margin-top: 12px;
        }

        .entity-status.playing {
            background: var(--accent-green);
            color: #000;
            box-shadow: 0 0 12px rgba(155, 187, 89, 0.4);
            animation: pulse 1.5s ease-in-out infinite;
        }

        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.7; }
        }

        .entity-status.queued {
            background: var(--accent-orange);
            color: #000;
        }

        .entity-status.stopped {
            background: var(--bg-tertiary);
            color: var(--text-muted);
            border: 1px solid var(--border);
        }

        /* Controls */
        .controls-row {
            display: flex;
            gap: 8px;
            margin-bottom: 20px;
            flex-wrap: wrap;
            padding: 0 4px;
        }

        .control-btn {
            padding: 8px 16px;
            border: none;
            border-radius: 4px;
            background: linear-gradient(to bottom, var(--bg-tertiary), #252525);
            color: var(--text-primary);
            cursor: pointer;
            font-size: 11px;
            font-weight: 600;
            transition: all 0.15s ease;
            box-shadow: 0 2px 4px rgba(0,0,0,0.2);
        }

        .control-btn:hover {
            background: linear-gradient(to bottom, #3a3a3a, #2d2d2d);
            transform: translateY(-1px);
            box-shadow: 0 3px 6px rgba(0,0,0,0.3);
        }

        .control-btn:active {
            transform: translateY(0);
            box-shadow: 0 1px 2px rgba(0,0,0,0.2);
        }

        .control-btn.active {
            background: linear-gradient(to bottom, var(--accent-orange), #b8844d);
            color: #000;
            box-shadow: 0 0 8px rgba(209, 154, 102, 0.4);
        }

        .control-btn.play {
            background: linear-gradient(to bottom, var(--accent-green), #7a9948);
            color: #000;
            box-shadow: 0 0 8px rgba(155, 187, 89, 0.4);
        }

        .control-btn.play:hover {
            background: linear-gradient(to bottom, #aece6a, var(--accent-green));
        }

        .control-btn.stop {
            background: linear-gradient(to bottom, var(--accent-red), #b55656);
            color: #fff;
            box-shadow: 0 0 8px rgba(209, 105, 105, 0.4);
        }

        .control-btn.stop:hover {
            background: linear-gradient(to bottom, #e07a7a, var(--accent-red));
        }

        .control-btn.source-btn {
            margin-left: auto;
            background: transparent;
            border: 1px solid var(--border);
            color: var(--text-secondary);
            box-shadow: none;
        }

        .control-btn.source-btn:hover {
            background: var(--bg-tertiary);
            color: var(--text-primary);
            border-color: var(--text-secondary);
        }

        /* Sections */
        .section {
            margin-bottom: 20px;
            padding: 0 4px;
        }

        .section h2 {
            font-size: 10px;
            font-weight: 700;
            letter-spacing: 1px;
            color: var(--text-muted);
            text-transform: uppercase;
            margin-bottom: 12px;
            padding-bottom: 6px;
            border-bottom: 1px solid var(--border);
        }

        /* Parameters */
        .param-row {
            display: grid;
            grid-template-columns: 80px 1fr 55px;
            gap: 12px;
            align-items: center;
            margin-bottom: 10px;
            padding: 8px 12px;
            background: var(--bg-secondary);
            border-radius: 4px;
        }

        .param-row label {
            font-size: 11px;
            color: var(--text-secondary);
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            font-weight: 500;
        }

        .param-row input[type="range"] {
            width: 100%;
            height: 6px;
            background: var(--bg-tertiary);
            border-radius: 3px;
            -webkit-appearance: none;
        }

        .param-row input[type="range"]::-webkit-slider-thumb {
            -webkit-appearance: none;
            width: 14px;
            height: 14px;
            background: linear-gradient(to bottom, var(--accent-green), #7a9948);
            border-radius: 50%;
            cursor: pointer;
            box-shadow: 0 2px 4px rgba(0,0,0,0.3);
            transition: transform 0.1s ease;
        }

        .param-row input[type="range"]::-webkit-slider-thumb:hover {
            transform: scale(1.15);
        }

        .param-value {
            font-size: 10px;
            font-family: 'SF Mono', Monaco, monospace;
            color: var(--accent-green);
            text-align: right;
            background: var(--bg-tertiary);
            padding: 3px 6px;
            border-radius: 3px;
        }

        .no-params {
            color: var(--text-muted);
            font-style: italic;
            padding: 12px;
            text-align: center;
            background: var(--bg-secondary);
            border-radius: 4px;
        }

        /* Info Grid */
        .info-grid {
            display: grid;
            grid-template-columns: 100px 1fr;
            gap: 6px 16px;
            background: var(--bg-secondary);
            padding: 12px;
            border-radius: 4px;
        }

        .info-label {
            font-size: 10px;
            color: var(--text-muted);
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        .info-value {
            font-size: 11px;
            font-family: 'SF Mono', Monaco, monospace;
            color: var(--text-primary);
        }

        /* Timeline */
        .mini-timeline {
            padding: 8px 0;
        }

        .timeline-track {
            position: relative;
            height: 24px;
            background: linear-gradient(to bottom, var(--bg-tertiary), #252525);
            border-radius: 4px;
            border: 1px solid var(--border);
        }

        .timeline-marker {
            position: absolute;
            width: 3px;
            height: calc(100% - 4px);
            top: 2px;
            background: var(--accent-green);
            border-radius: 2px;
            box-shadow: 0 0 4px rgba(155, 187, 89, 0.5);
        }

        /* Notes */
        .note-list {
            display: flex;
            flex-wrap: wrap;
            gap: 6px;
        }

        .note-chip {
            display: inline-block;
            padding: 4px 8px;
            background: linear-gradient(to bottom, var(--bg-tertiary), #252525);
            border: 1px solid var(--border);
            border-radius: 4px;
            font-size: 10px;
            font-family: 'SF Mono', Monaco, monospace;
            color: var(--accent-orange);
        }

        .more {
            color: var(--text-muted);
            font-style: italic;
            padding: 4px 8px;
        }

        /* Clips */
        .clip-list {
            display: flex;
            flex-direction: column;
            gap: 6px;
        }

        .clip-item {
            display: flex;
            gap: 12px;
            padding: 10px 12px;
            background: linear-gradient(to bottom, var(--bg-secondary), #1c1c1c);
            border: 1px solid var(--border);
            border-radius: 4px;
            font-size: 11px;
            align-items: center;
            transition: all 0.15s ease;
        }

        .clip-item:hover {
            border-color: var(--text-muted);
            background: linear-gradient(to bottom, var(--bg-tertiary), var(--bg-secondary));
        }

        .clip-type {
            color: var(--accent-blue);
            font-size: 8px;
            font-weight: 700;
            text-transform: uppercase;
            letter-spacing: 1px;
            background: rgba(86, 156, 214, 0.15);
            padding: 3px 6px;
            border-radius: 3px;
            min-width: 55px;
            text-align: center;
        }

        .clip-name {
            flex: 1;
            font-weight: 500;
        }

        .clip-time {
            color: var(--text-muted);
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 10px;
            background: var(--bg-tertiary);
            padding: 2px 6px;
            border-radius: 3px;
        }
    </style>
</head>
<body>
    ${content}
    <script>
        const vscode = acquireVsCodeApi();

        function sendMessage(message) {
            vscode.postMessage(message);
        }

        // Update the value display in real-time as slider moves
        function updateSliderValue(valueId, value) {
            const el = document.getElementById(valueId);
            if (el) {
                el.textContent = parseFloat(value).toFixed(2);
            }
        }

        // Update gain value display (shown as percentage)
        function updateGainValue(valueId, value) {
            const el = document.getElementById(valueId);
            if (el) {
                el.textContent = (parseFloat(value) * 100).toFixed(0) + '%';
            }
        }

        // Handle mouse leaving the window while dragging
        document.addEventListener('mouseup', () => {
            sendMessage({command:'interactionEnd'});
        });
    </script>
</body>
</html>`;
    }
    dispose() {
        InspectorPanel.currentPanel = undefined;
        this._panel.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
exports.InspectorPanel = InspectorPanel;
InspectorPanel.viewType = 'vibelang.inspector';
//# sourceMappingURL=inspectorPanel.js.map