"use strict";
/**
 * VibeLang Pattern Editor (Multi-Lane Drum Machine)
 *
 * Visual step sequencer for editing multiple patterns simultaneously.
 * Features:
 * - Multi-lane view (one row per pattern)
 * - Configurable grid size (4/8/16/32/64 steps per bar)
 * - Click to toggle steps, Shift+click for accent
 * - Drag to adjust velocity
 * - Euclidean rhythm generation
 * - Real-time playhead sync
 * - Live code write-back (saves file for instant feedback)
 * - Group-based pattern display
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.PatternEditor = void 0;
const vscode = require("vscode");
const patternParser_1 = require("../utils/patternParser");
class PatternEditor {
    constructor(panel, store) {
        this._disposables = [];
        this._lanes = [];
        this._selectedGroup = null;
        this._pendingPatternName = null;
        this._webviewReady = false;
        this._panel = panel;
        this._store = store;
        this._updateContent();
        // Listen for state updates
        this._disposables.push(store.onTransportUpdate((transport) => this._sendTransportUpdate(transport)));
        this._disposables.push(store.onFullUpdate(() => this._refreshFromState()));
        // Handle messages from webview
        this._panel.webview.onDidReceiveMessage((message) => this._handleMessage(message), null, this._disposables);
        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    }
    static createOrShow(store, patternName) {
        const column = vscode.ViewColumn.Two;
        if (PatternEditor.currentPanel) {
            PatternEditor.currentPanel._panel.reveal(column);
            if (patternName) {
                PatternEditor.currentPanel._loadPatternByName(patternName);
            }
            return;
        }
        const panel = vscode.window.createWebviewPanel(PatternEditor.viewType, 'Pattern Editor', column, {
            enableScripts: true,
            retainContextWhenHidden: true,
        });
        PatternEditor.currentPanel = new PatternEditor(panel, store);
        if (patternName) {
            // Store pending pattern name to load when webview is ready
            PatternEditor.currentPanel._pendingPatternName = patternName;
        }
    }
    static revive(panel, store) {
        PatternEditor.currentPanel = new PatternEditor(panel, store);
    }
    loadPattern(patternName) {
        this._loadPatternByName(patternName);
    }
    _loadPatternByName(patternName) {
        const state = this._store.state;
        if (!state)
            return;
        const pattern = state.patterns.find(p => p.name === patternName);
        if (!pattern) {
            vscode.window.showErrorMessage(`Pattern "${patternName}" not found`);
            return;
        }
        // Set the group from this pattern
        this._selectedGroup = pattern.group_path;
        // Load all patterns from this group
        this._loadGroupPatterns(pattern.group_path);
    }
    _loadGroupPatterns(groupPath) {
        const state = this._store.state;
        if (!state)
            return;
        const groupPatterns = state.patterns.filter(p => p.group_path === groupPath);
        this._lanes = groupPatterns.map((pattern, index) => this._patternToLane(pattern, index));
        this._selectedGroup = groupPath;
        this._sendLanesUpdate();
    }
    _patternToLane(pattern, index) {
        const patternString = pattern.step_pattern || '';
        let grid;
        if (patternString) {
            // Use the original step pattern if available
            grid = (0, patternParser_1.parsePatternString)(patternString);
        }
        else if (pattern.events && pattern.events.length > 0) {
            // Fallback: reconstruct from events
            grid = this._reconstructPatternFromEvents(pattern.events, pattern.loop_beats);
        }
        else {
            // Default empty grid
            grid = (0, patternParser_1.createEmptyGrid)({ stepsPerBar: 16, numBars: 1, beatsPerBar: 4 });
        }
        // Generate a color based on voice name
        const colors = [
            '#9bbb59', '#d19a66', '#569cd6', '#c586c0', '#d16969',
            '#4ec9b0', '#dcdcaa', '#ce9178', '#6a9955', '#b5cea8'
        ];
        const color = colors[index % colors.length];
        return {
            patternName: pattern.name,
            voiceName: pattern.voice_name || null,
            groupPath: pattern.group_path,
            grid,
            sourceLocation: pattern.source_location,
            isPlaying: pattern.is_looping || pattern.status?.state === 'playing',
            color,
        };
    }
    _updateContent() {
        this._panel.webview.html = this._getHtmlContent();
    }
    _refreshFromState() {
        const state = this._store.state;
        if (!state)
            return;
        // Send available groups and patterns
        const groups = state.groups.map(g => ({
            path: g.path,
            name: g.name,
            patternCount: state.patterns.filter(p => p.group_path === g.path).length,
        }));
        const allPatterns = state.patterns.map(p => ({
            name: p.name,
            voiceName: p.voice_name,
            groupPath: p.group_path,
            isPlaying: p.is_looping || p.status?.state === 'playing',
        }));
        this._panel.webview.postMessage({
            type: 'stateUpdate',
            data: { groups, allPatterns, selectedGroup: this._selectedGroup },
        });
        // Update existing lanes' playing status
        for (const lane of this._lanes) {
            const pattern = state.patterns.find(p => p.name === lane.patternName);
            if (pattern) {
                lane.isPlaying = pattern.is_looping || pattern.status?.state === 'playing';
            }
        }
        // If we have lanes, resend them
        if (this._lanes.length > 0) {
            this._sendLanesUpdate();
        }
    }
    _sendLanesUpdate() {
        this._panel.webview.postMessage({
            type: 'lanesUpdate',
            data: {
                lanes: this._lanes,
                selectedGroup: this._selectedGroup,
            },
        });
    }
    _sendTransportUpdate(transport) {
        this._panel.webview.postMessage({
            type: 'transportUpdate',
            data: transport,
        });
    }
    async _handleMessage(message) {
        switch (message.command) {
            case 'ready':
                this._webviewReady = true;
                // Load pending pattern if any
                if (this._pendingPatternName) {
                    this._loadPatternByName(this._pendingPatternName);
                    this._pendingPatternName = null;
                }
                else {
                    this._refreshFromState();
                }
                break;
            case 'selectGroup':
                this._selectedGroup = message.groupPath;
                this._loadGroupPatterns(message.groupPath);
                break;
            case 'addPattern':
                const patternName = message.patternName;
                await this._addPatternToLanes(patternName);
                break;
            case 'removePattern':
                const removeIndex = message.laneIndex;
                this._lanes.splice(removeIndex, 1);
                this._sendLanesUpdate();
                break;
            case 'updateStep':
                const laneIndex = message.laneIndex;
                const stepIndex = message.stepIndex;
                const velocity = message.velocity;
                const accent = message.accent;
                if (this._lanes[laneIndex]) {
                    this._lanes[laneIndex].grid.steps[stepIndex] = { velocity, accent };
                    await this._updateLaneCode(laneIndex);
                }
                break;
            case 'updateLane':
                const updateLaneIndex = message.laneIndex;
                const newGrid = message.grid;
                if (this._lanes[updateLaneIndex]) {
                    this._lanes[updateLaneIndex].grid = newGrid;
                    await this._updateLaneCode(updateLaneIndex);
                }
                break;
            case 'resizeGrid':
                const config = message.config;
                await this._resizeAllLanes(config);
                break;
            case 'togglePlayback':
                const playLaneIndex = message.laneIndex;
                if (this._lanes[playLaneIndex]) {
                    const lane = this._lanes[playLaneIndex];
                    if (lane.isPlaying) {
                        await this._store.runtime.stopPattern(lane.patternName);
                    }
                    else {
                        await this._store.runtime.startPattern(lane.patternName);
                    }
                }
                break;
            case 'goToSource':
                const sourceLaneIndex = message.laneIndex;
                if (this._lanes[sourceLaneIndex]?.sourceLocation) {
                    vscode.commands.executeCommand('vibelang.goToSource', this._lanes[sourceLaneIndex].sourceLocation);
                }
                break;
            case 'applyEuclidean':
                const eucLaneIndex = message.laneIndex;
                const hits = message.hits;
                if (this._lanes[eucLaneIndex]) {
                    this._applyEuclidean(eucLaneIndex, hits);
                }
                break;
            case 'clearLane':
                const clearLaneIndex = message.laneIndex;
                if (this._lanes[clearLaneIndex]) {
                    const grid = this._lanes[clearLaneIndex].grid;
                    grid.steps = grid.steps.map(() => ({ velocity: 0, accent: false }));
                    this._sendLanesUpdate();
                    await this._updateLaneCode(clearLaneIndex);
                }
                break;
        }
    }
    async _addPatternToLanes(patternName) {
        const state = this._store.state;
        if (!state)
            return;
        // Check if already in lanes
        if (this._lanes.some(l => l.patternName === patternName)) {
            return;
        }
        const pattern = state.patterns.find(p => p.name === patternName);
        if (pattern) {
            this._lanes.push(this._patternToLane(pattern, this._lanes.length));
            this._sendLanesUpdate();
        }
    }
    async _updateLaneCode(laneIndex) {
        const lane = this._lanes[laneIndex];
        if (!lane?.sourceLocation?.file || !lane?.sourceLocation?.line) {
            return;
        }
        const newPatternString = (0, patternParser_1.generatePatternString)(lane.grid);
        try {
            const document = await vscode.workspace.openTextDocument(lane.sourceLocation.file);
            const lineIndex = lane.sourceLocation.line - 1;
            const textLine = document.lineAt(lineIndex);
            const lineText = textLine.text;
            // Find the .step("...") call
            const stepRegex = /\.step\s*\(\s*"([^"]*)"\s*\)/;
            const match = lineText.match(stepRegex);
            if (match) {
                const start = lineText.indexOf(match[0]);
                const end = start + match[0].length;
                const edit = new vscode.WorkspaceEdit();
                edit.replace(document.uri, new vscode.Range(lineIndex, start, lineIndex, end), `.step("${newPatternString}")`);
                await vscode.workspace.applyEdit(edit);
                // Save the document to trigger live reload
                await document.save();
            }
        }
        catch (error) {
            console.error('Failed to update pattern in code:', error);
        }
    }
    async _resizeAllLanes(config) {
        for (let i = 0; i < this._lanes.length; i++) {
            const lane = this._lanes[i];
            const newGrid = (0, patternParser_1.createEmptyGrid)(config);
            // Copy existing steps
            for (let j = 0; j < Math.min(newGrid.totalSteps, lane.grid.totalSteps); j++) {
                if (lane.grid.steps[j]) {
                    newGrid.steps[j] = { ...lane.grid.steps[j] };
                }
            }
            lane.grid = newGrid;
            await this._updateLaneCode(i);
        }
        this._sendLanesUpdate();
    }
    async _applyEuclidean(laneIndex, hits) {
        const lane = this._lanes[laneIndex];
        if (!lane)
            return;
        const { stepsPerBar, numBars } = lane.grid;
        const euclidean = this._generateEuclidean(hits, stepsPerBar);
        const newSteps = [];
        for (let barIndex = 0; barIndex < numBars; barIndex++) {
            for (let i = 0; i < stepsPerBar; i++) {
                newSteps.push({ ...euclidean[i] });
            }
        }
        lane.grid.steps = newSteps;
        this._sendLanesUpdate();
        await this._updateLaneCode(laneIndex);
    }
    _generateEuclidean(hits, steps) {
        if (steps === 0)
            return [];
        if (hits >= steps) {
            return Array(steps).fill(null).map(() => ({ velocity: 1.0, accent: false }));
        }
        if (hits === 0) {
            return Array(steps).fill(null).map(() => ({ velocity: 0, accent: false }));
        }
        const pattern = [];
        let bucket = 0;
        for (let i = 0; i < steps; i++) {
            bucket += hits;
            if (bucket >= steps) {
                bucket -= steps;
                pattern.push({ velocity: 1.0, accent: false });
            }
            else {
                pattern.push({ velocity: 0, accent: false });
            }
        }
        return pattern;
    }
    /**
     * Fallback function to reconstruct a pattern grid from events when step_pattern is not available.
     * Analyzes beat positions and velocities to create a visual grid.
     */
    _reconstructPatternFromEvents(events, loopBeats) {
        // Determine grid configuration from loop length
        const numBars = Math.ceil(loopBeats / 4);
        const stepsPerBar = 16; // Default to 16th notes
        const totalSteps = stepsPerBar * numBars;
        const beatsPerStep = loopBeats / totalSteps;
        const grid = (0, patternParser_1.createEmptyGrid)({ stepsPerBar, numBars, beatsPerBar: 4 });
        // Place events into grid
        for (const event of events) {
            const stepIndex = Math.round(event.beat / beatsPerStep);
            if (stepIndex >= 0 && stepIndex < totalSteps) {
                // Get velocity from params (amp parameter or default to 1.0)
                const velocity = event.params?.amp ?? 1.0;
                const accent = velocity > 1.0;
                grid.steps[stepIndex] = { velocity: Math.min(1.0, velocity), accent };
            }
        }
        return grid;
    }
    _getHtmlContent() {
        if (this._store.status !== 'connected') {
            return this._getDisconnectedHtml();
        }
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Pattern Editor</title>
    <style>
        :root {
            --bg-primary: #1a1a1a;
            --bg-secondary: #232323;
            --bg-tertiary: #2d2d2d;
            --bg-lane: #1e1e1e;
            --text-primary: #d4d4d4;
            --text-secondary: #858585;
            --text-muted: #5a5a5a;
            --accent-green: #9bbb59;
            --accent-orange: #d19a66;
            --accent-blue: #569cd6;
            --accent-purple: #c586c0;
            --accent-red: #d16969;
            --border: #3c3c3c;
            --step-off: #2a2a2a;
            --step-hover: #3a3a3a;
            --playhead: #ff6b6b;
            --beat-line: #3a3a3a;
            --bar-line: #555555;
        }

        * { box-sizing: border-box; margin: 0; padding: 0; }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: 12px;
            overflow: hidden;
            height: 100vh;
            display: flex;
            flex-direction: column;
        }

        /* Toolbar */
        .toolbar {
            display: flex;
            align-items: center;
            gap: 12px;
            padding: 8px 12px;
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border);
            flex-wrap: wrap;
        }

        .toolbar-group {
            display: flex;
            align-items: center;
            gap: 6px;
        }

        .toolbar-divider {
            width: 1px;
            height: 20px;
            background: var(--border);
            margin: 0 6px;
        }

        .toolbar-label {
            font-size: 11px;
            color: var(--text-secondary);
        }

        select, input {
            padding: 4px 8px;
            border: 1px solid var(--border);
            border-radius: 3px;
            background: var(--bg-tertiary);
            color: var(--text-primary);
            font-size: 11px;
        }

        select:focus, input:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        .btn {
            padding: 4px 10px;
            border: 1px solid var(--border);
            border-radius: 3px;
            background: var(--bg-tertiary);
            color: var(--text-primary);
            cursor: pointer;
            font-size: 11px;
            transition: all 0.1s ease;
        }

        .btn:hover {
            background: #3a3a3a;
            border-color: var(--text-secondary);
        }

        .btn.active {
            background: var(--accent-green);
            color: #000;
            border-color: var(--accent-green);
        }

        /* Main container */
        .main-container {
            flex: 1;
            display: flex;
            overflow: hidden;
        }

        /* Lane list (left sidebar) */
        .lane-list {
            width: 200px;
            min-width: 200px;
            background: var(--bg-secondary);
            border-right: 1px solid var(--border);
            display: flex;
            flex-direction: column;
        }

        .lane-list-header {
            padding: 8px 10px;
            background: var(--bg-tertiary);
            border-bottom: 1px solid var(--border);
            display: flex;
            align-items: center;
            gap: 8px;
        }

        .lane-list-title {
            flex: 1;
            font-weight: 600;
            font-size: 11px;
        }

        .add-lane-btn {
            width: 22px;
            height: 22px;
            border: none;
            border-radius: 3px;
            background: var(--accent-blue);
            color: white;
            cursor: pointer;
            font-size: 14px;
            display: flex;
            align-items: center;
            justify-content: center;
        }

        .add-lane-btn:hover {
            background: #4a8cc8;
        }

        .lane-items {
            flex: 1;
            overflow-y: auto;
        }

        .lane-item {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 8px 10px;
            border-bottom: 1px solid var(--border);
            cursor: pointer;
            height: 40px;
        }

        .lane-item:hover {
            background: var(--bg-tertiary);
        }

        .lane-color {
            width: 4px;
            height: 28px;
            border-radius: 2px;
        }

        .lane-info {
            flex: 1;
            min-width: 0;
        }

        .lane-name {
            font-weight: 500;
            font-size: 11px;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }

        .lane-voice {
            font-size: 9px;
            color: var(--text-muted);
        }

        .lane-controls {
            display: flex;
            gap: 2px;
            opacity: 0;
            transition: opacity 0.1s;
        }

        .lane-item:hover .lane-controls {
            opacity: 1;
        }

        .lane-btn {
            width: 20px;
            height: 20px;
            border: none;
            border-radius: 2px;
            background: var(--bg-tertiary);
            color: var(--text-secondary);
            cursor: pointer;
            font-size: 10px;
            display: flex;
            align-items: center;
            justify-content: center;
        }

        .lane-btn:hover {
            background: var(--accent-blue);
            color: white;
        }

        .lane-btn.play.active {
            background: var(--accent-green);
            color: white;
        }

        .lane-btn.remove:hover {
            background: var(--accent-red);
        }

        /* Grid area */
        .grid-area {
            flex: 1;
            display: flex;
            flex-direction: column;
            overflow: hidden;
        }

        /* Grid header */
        .grid-header {
            display: flex;
            height: 28px;
            background: var(--bg-tertiary);
            border-bottom: 1px solid var(--border);
            margin-left: 0;
        }

        .grid-header-cell {
            width: 24px;
            min-width: 24px;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 9px;
            color: var(--text-muted);
            border-right: 1px solid var(--bg-primary);
        }

        .grid-header-cell.bar-start {
            border-left: 2px solid var(--bar-line);
            color: var(--text-primary);
            font-weight: 600;
        }

        .grid-header-cell.beat-start {
            border-left: 1px solid var(--beat-line);
            color: var(--text-secondary);
        }

        /* Grid body */
        .grid-body {
            flex: 1;
            overflow: auto;
            position: relative;
        }

        .grid-lanes {
            position: relative;
        }

        /* Lane row */
        .lane-row {
            display: flex;
            height: 40px;
            border-bottom: 1px solid var(--border);
            background: var(--bg-lane);
        }

        .lane-row:nth-child(even) {
            background: var(--bg-secondary);
        }

        /* Step cell */
        .step {
            width: 24px;
            min-width: 24px;
            height: 100%;
            border-right: 1px solid var(--bg-primary);
            cursor: pointer;
            position: relative;
            display: flex;
            align-items: center;
            justify-content: center;
        }

        .step:hover {
            background: var(--step-hover);
        }

        .step.bar-start {
            border-left: 2px solid var(--bar-line);
        }

        .step.beat-start {
            border-left: 1px solid var(--beat-line);
        }

        .step-dot {
            width: 16px;
            height: 16px;
            border-radius: 3px;
            opacity: 0;
            transition: all 0.1s ease;
        }

        .step.on .step-dot {
            opacity: 1;
        }

        .step.on .step-dot.velocity-low {
            transform: scale(0.6);
        }

        .step.on .step-dot.velocity-mid {
            transform: scale(0.8);
        }

        .step.on .step-dot.velocity-high {
            transform: scale(1.0);
        }

        .step.accent .step-dot {
            box-shadow: 0 0 6px currentColor;
        }

        /* Playhead */
        .playhead {
            position: absolute;
            top: 0;
            bottom: 0;
            width: 2px;
            background: var(--playhead);
            pointer-events: none;
            z-index: 100;
            box-shadow: 0 0 8px rgba(255, 107, 107, 0.8);
            display: none;
        }

        .playhead.visible {
            display: block;
        }

        /* Pattern picker dropdown */
        .pattern-picker {
            position: fixed;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 4px;
            box-shadow: 0 4px 16px rgba(0,0,0,0.4);
            z-index: 1000;
            max-height: 300px;
            width: 240px;
            display: none;
        }

        .pattern-picker.visible {
            display: block;
        }

        .pattern-picker-header {
            padding: 8px 12px;
            font-weight: 600;
            font-size: 11px;
            border-bottom: 1px solid var(--border);
        }

        .pattern-picker-list {
            max-height: 250px;
            overflow-y: auto;
        }

        .pattern-picker-item {
            padding: 8px 12px;
            cursor: pointer;
            font-size: 11px;
        }

        .pattern-picker-item:hover {
            background: var(--bg-tertiary);
        }

        .pattern-picker-item.disabled {
            opacity: 0.5;
            cursor: default;
        }

        /* Empty state */
        .empty-state {
            flex: 1;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            color: var(--text-secondary);
            text-align: center;
            padding: 40px;
        }

        .empty-icon {
            font-size: 48px;
            margin-bottom: 16px;
            opacity: 0.3;
        }

        .empty-state h3 {
            font-size: 16px;
            margin-bottom: 8px;
            color: var(--text-primary);
        }

        /* Info bar */
        .info-bar {
            display: flex;
            align-items: center;
            gap: 16px;
            padding: 6px 12px;
            background: var(--bg-secondary);
            border-top: 1px solid var(--border);
            font-size: 10px;
        }

        .info-item {
            display: flex;
            align-items: center;
            gap: 4px;
        }

        .info-label {
            color: var(--text-muted);
        }

        .info-value {
            color: var(--text-primary);
            font-family: 'SF Mono', Monaco, monospace;
        }
    </style>
</head>
<body>
    <div class="toolbar">
        <div class="toolbar-group">
            <span class="toolbar-label">Group:</span>
            <select id="groupSelect">
                <option value="">Select group...</option>
            </select>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <span class="toolbar-label">Steps/Bar:</span>
            <select id="stepsPerBar">
                <option value="4">4</option>
                <option value="8">8</option>
                <option value="16" selected>16</option>
                <option value="32">32</option>
            </select>
        </div>

        <div class="toolbar-group">
            <span class="toolbar-label">Bars:</span>
            <select id="numBars">
                <option value="1" selected>1</option>
                <option value="2">2</option>
                <option value="4">4</option>
                <option value="8">8</option>
            </select>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <span class="toolbar-label">Euclidean:</span>
            <input type="number" id="euclideanHits" min="0" max="64" value="4" style="width: 40px;">
        </div>
    </div>

    <div class="main-container">
        <div class="lane-list">
            <div class="lane-list-header">
                <span class="lane-list-title">Patterns</span>
                <button class="add-lane-btn" id="addLaneBtn" title="Add Pattern">+</button>
            </div>
            <div class="lane-items" id="laneItems"></div>
        </div>

        <div class="grid-area">
            <div class="grid-header" id="gridHeader"></div>
            <div class="grid-body" id="gridBody">
                <div class="grid-lanes" id="gridLanes"></div>
                <div class="playhead" id="playhead"></div>
            </div>
        </div>
    </div>

    <div class="info-bar">
        <div class="info-item">
            <span class="info-label">Lanes:</span>
            <span class="info-value" id="laneCount">0</span>
        </div>
        <div class="info-item">
            <span class="info-label">Steps:</span>
            <span class="info-value" id="stepCount">16</span>
        </div>
        <div class="info-item">
            <span class="info-label">Duration:</span>
            <span class="info-value" id="duration">4 beats</span>
        </div>
    </div>

    <!-- Pattern Picker -->
    <div class="pattern-picker" id="patternPicker">
        <div class="pattern-picker-header">Add Pattern</div>
        <div class="pattern-picker-list" id="patternPickerList"></div>
    </div>

    <script>
        const vscode = acquireVsCodeApi();

        let state = {
            groups: [],
            allPatterns: [],
            lanes: [],
            selectedGroup: null,
            transport: { current_beat: 0, bpm: 120, running: false },
            stepsPerBar: 16,
            numBars: 1,
        };

        // Elements
        const groupSelect = document.getElementById('groupSelect');
        const stepsPerBarSelect = document.getElementById('stepsPerBar');
        const numBarsSelect = document.getElementById('numBars');
        const laneItems = document.getElementById('laneItems');
        const gridHeader = document.getElementById('gridHeader');
        const gridLanes = document.getElementById('gridLanes');
        const gridBody = document.getElementById('gridBody');
        const playhead = document.getElementById('playhead');

        // Init
        function init() {
            setupEventListeners();
            vscode.postMessage({ command: 'ready' });
            requestAnimationFrame(animatePlayhead);
        }

        function setupEventListeners() {
            groupSelect.addEventListener('change', (e) => {
                if (e.target.value) {
                    vscode.postMessage({ command: 'selectGroup', groupPath: e.target.value });
                }
            });

            stepsPerBarSelect.addEventListener('change', () => {
                state.stepsPerBar = parseInt(stepsPerBarSelect.value);
                vscode.postMessage({
                    command: 'resizeGrid',
                    config: { stepsPerBar: state.stepsPerBar, numBars: state.numBars, beatsPerBar: 4 }
                });
            });

            numBarsSelect.addEventListener('change', () => {
                state.numBars = parseInt(numBarsSelect.value);
                vscode.postMessage({
                    command: 'resizeGrid',
                    config: { stepsPerBar: state.stepsPerBar, numBars: state.numBars, beatsPerBar: 4 }
                });
            });

            document.getElementById('addLaneBtn').addEventListener('click', (e) => {
                showPatternPicker(e.target.getBoundingClientRect());
            });

            document.addEventListener('click', (e) => {
                const picker = document.getElementById('patternPicker');
                const addBtn = document.getElementById('addLaneBtn');
                if (!picker.contains(e.target) && !addBtn.contains(e.target)) {
                    picker.classList.remove('visible');
                }
            });

            window.addEventListener('message', (event) => {
                const message = event.data;
                switch (message.type) {
                    case 'stateUpdate':
                        state.groups = message.data.groups;
                        state.allPatterns = message.data.allPatterns;
                        state.selectedGroup = message.data.selectedGroup;
                        updateGroupSelect();
                        break;
                    case 'lanesUpdate':
                        state.lanes = message.data.lanes;
                        state.selectedGroup = message.data.selectedGroup;
                        if (state.lanes.length > 0) {
                            state.stepsPerBar = state.lanes[0].grid.stepsPerBar;
                            state.numBars = state.lanes[0].grid.numBars;
                            stepsPerBarSelect.value = state.stepsPerBar;
                            numBarsSelect.value = state.numBars;
                        }
                        render();
                        break;
                    case 'transportUpdate':
                        state.transport = message.data;
                        break;
                }
            });
        }

        function updateGroupSelect() {
            groupSelect.innerHTML = '<option value="">Select group...</option>';
            for (const g of state.groups) {
                const option = document.createElement('option');
                option.value = g.path;
                option.textContent = g.path + ' (' + g.patternCount + ' patterns)';
                if (g.path === state.selectedGroup) {
                    option.selected = true;
                }
                groupSelect.appendChild(option);
            }
        }

        function render() {
            renderLaneList();
            renderGridHeader();
            renderGridLanes();
            updateInfo();
        }

        function renderLaneList() {
            laneItems.innerHTML = '';

            if (state.lanes.length === 0) {
                laneItems.innerHTML = '<div style="padding: 20px; text-align: center; color: var(--text-muted);">Select a group to load patterns</div>';
                return;
            }

            state.lanes.forEach((lane, index) => {
                const item = document.createElement('div');
                item.className = 'lane-item';

                item.innerHTML = \`
                    <div class="lane-color" style="background: \${lane.color}"></div>
                    <div class="lane-info">
                        <div class="lane-name">\${lane.patternName}</div>
                        <div class="lane-voice">\${lane.voiceName || 'No voice'}</div>
                    </div>
                    <div class="lane-controls">
                        <button class="lane-btn play \${lane.isPlaying ? 'active' : ''}" data-action="play" title="Play/Stop">\${lane.isPlaying ? '⏸' : '▶'}</button>
                        <button class="lane-btn" data-action="euclidean" title="Apply Euclidean">◉</button>
                        <button class="lane-btn" data-action="clear" title="Clear">⟳</button>
                        <button class="lane-btn" data-action="source" title="Go to Source">📄</button>
                        <button class="lane-btn remove" data-action="remove" title="Remove">✕</button>
                    </div>
                \`;

                item.querySelectorAll('.lane-btn').forEach(btn => {
                    btn.addEventListener('click', (e) => {
                        e.stopPropagation();
                        const action = btn.dataset.action;
                        if (action === 'play') {
                            vscode.postMessage({ command: 'togglePlayback', laneIndex: index });
                        } else if (action === 'euclidean') {
                            const hits = parseInt(document.getElementById('euclideanHits').value) || 4;
                            vscode.postMessage({ command: 'applyEuclidean', laneIndex: index, hits });
                        } else if (action === 'clear') {
                            vscode.postMessage({ command: 'clearLane', laneIndex: index });
                        } else if (action === 'source') {
                            vscode.postMessage({ command: 'goToSource', laneIndex: index });
                        } else if (action === 'remove') {
                            vscode.postMessage({ command: 'removePattern', laneIndex: index });
                        }
                    });
                });

                laneItems.appendChild(item);
            });
        }

        function renderGridHeader() {
            gridHeader.innerHTML = '';
            const totalSteps = state.stepsPerBar * state.numBars;
            const stepsPerBeat = state.stepsPerBar / 4;

            for (let i = 0; i < totalSteps; i++) {
                const cell = document.createElement('div');
                cell.className = 'grid-header-cell';

                const barIndex = Math.floor(i / state.stepsPerBar);
                const stepInBar = i % state.stepsPerBar;

                if (stepInBar === 0) {
                    cell.classList.add('bar-start');
                    cell.textContent = (barIndex + 1).toString();
                } else if (stepInBar % stepsPerBeat === 0) {
                    cell.classList.add('beat-start');
                }

                gridHeader.appendChild(cell);
            }
        }

        function renderGridLanes() {
            gridLanes.innerHTML = '';

            state.lanes.forEach((lane, laneIndex) => {
                const row = document.createElement('div');
                row.className = 'lane-row';

                const totalSteps = state.stepsPerBar * state.numBars;
                const stepsPerBeat = state.stepsPerBar / 4;

                for (let stepIndex = 0; stepIndex < totalSteps; stepIndex++) {
                    const step = lane.grid.steps[stepIndex] || { velocity: 0, accent: false };
                    const stepEl = document.createElement('div');
                    stepEl.className = 'step';

                    const stepInBar = stepIndex % state.stepsPerBar;
                    if (stepInBar === 0) stepEl.classList.add('bar-start');
                    else if (stepInBar % stepsPerBeat === 0) stepEl.classList.add('beat-start');

                    if (step.velocity > 0) {
                        stepEl.classList.add('on');
                        if (step.accent) stepEl.classList.add('accent');

                        const dot = document.createElement('div');
                        dot.className = 'step-dot';
                        dot.style.backgroundColor = lane.color;

                        if (step.velocity < 0.4) dot.classList.add('velocity-low');
                        else if (step.velocity < 0.7) dot.classList.add('velocity-mid');
                        else dot.classList.add('velocity-high');

                        stepEl.appendChild(dot);
                    }

                    stepEl.addEventListener('click', (e) => {
                        toggleStep(laneIndex, stepIndex, e.shiftKey);
                    });

                    // Velocity drag
                    stepEl.addEventListener('mousedown', (e) => {
                        if (e.button === 0 && lane.grid.steps[stepIndex]?.velocity > 0) {
                            startVelocityDrag(laneIndex, stepIndex, e);
                        }
                    });

                    row.appendChild(stepEl);
                }

                gridLanes.appendChild(row);
            });
        }

        function toggleStep(laneIndex, stepIndex, accent) {
            const lane = state.lanes[laneIndex];
            if (!lane) return;

            const step = lane.grid.steps[stepIndex];
            let newVelocity, newAccent;

            if (step.velocity > 0 && !accent) {
                newVelocity = 0;
                newAccent = false;
            } else if (step.velocity > 0 && accent) {
                newVelocity = step.velocity;
                newAccent = !step.accent;
            } else {
                newVelocity = 1.0;
                newAccent = accent;
            }

            vscode.postMessage({
                command: 'updateStep',
                laneIndex,
                stepIndex,
                velocity: newVelocity,
                accent: newAccent,
            });

            // Optimistic update
            lane.grid.steps[stepIndex] = { velocity: newVelocity, accent: newAccent };
            renderGridLanes();
        }

        function startVelocityDrag(laneIndex, stepIndex, startEvent) {
            const lane = state.lanes[laneIndex];
            const startY = startEvent.clientY;
            const startVel = lane.grid.steps[stepIndex].velocity;

            function onMove(e) {
                const deltaY = startY - e.clientY;
                const newVel = Math.max(0.1, Math.min(1.0, startVel + deltaY / 100));
                lane.grid.steps[stepIndex].velocity = newVel;
                renderGridLanes();
            }

            function onUp() {
                document.removeEventListener('mousemove', onMove);
                document.removeEventListener('mouseup', onUp);
                vscode.postMessage({
                    command: 'updateLane',
                    laneIndex,
                    grid: lane.grid,
                });
            }

            document.addEventListener('mousemove', onMove);
            document.addEventListener('mouseup', onUp);
        }

        function showPatternPicker(rect) {
            const picker = document.getElementById('patternPicker');
            const list = document.getElementById('patternPickerList');

            picker.style.left = rect.right + 4 + 'px';
            picker.style.top = rect.top + 'px';

            // Get patterns not already in lanes
            const laneNames = state.lanes.map(l => l.patternName);
            const availablePatterns = state.allPatterns.filter(p => !laneNames.includes(p.name));

            if (availablePatterns.length === 0) {
                list.innerHTML = '<div style="padding: 12px; color: var(--text-muted); text-align: center;">All patterns are shown</div>';
            } else {
                list.innerHTML = availablePatterns.map(p => \`
                    <div class="pattern-picker-item" data-name="\${p.name}">
                        \${p.name} <span style="color: var(--text-muted)">→ \${p.voiceName || 'no voice'}</span>
                    </div>
                \`).join('');

                list.querySelectorAll('.pattern-picker-item').forEach(item => {
                    item.addEventListener('click', () => {
                        vscode.postMessage({ command: 'addPattern', patternName: item.dataset.name });
                        picker.classList.remove('visible');
                    });
                });
            }

            picker.classList.add('visible');
        }

        function updateInfo() {
            document.getElementById('laneCount').textContent = state.lanes.length;
            document.getElementById('stepCount').textContent = state.stepsPerBar * state.numBars;
            document.getElementById('duration').textContent = (state.numBars * 4) + ' beats';
        }

        function animatePlayhead() {
            if (state.transport.running && state.lanes.length > 0) {
                const { current_beat, bpm } = state.transport;
                const totalBeats = state.numBars * 4;
                const loopBeat = current_beat % totalBeats;
                const totalSteps = state.stepsPerBar * state.numBars;
                const stepWidth = 24;
                const position = (loopBeat / totalBeats) * totalSteps * stepWidth;

                playhead.style.left = position + 'px';
                playhead.classList.add('visible');
            } else {
                playhead.classList.remove('visible');
            }

            requestAnimationFrame(animatePlayhead);
        }

        init();
    </script>
</body>
</html>`;
    }
    _getDisconnectedHtml() {
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Pattern Editor</title>
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
        .empty-state { text-align: center; color: #858585; }
        .empty-icon { font-size: 64px; margin-bottom: 20px; opacity: 0.3; }
        h2 { font-size: 18px; font-weight: 500; margin-bottom: 8px; color: #d4d4d4; }
        p { max-width: 400px; line-height: 1.5; }
    </style>
</head>
<body>
    <div class="empty-state">
        <div class="empty-icon">🥁</div>
        <h2>Not Connected</h2>
        <p>Connect to a VibeLang runtime to use the Pattern Editor.</p>
    </div>
</body>
</html>`;
    }
    dispose() {
        PatternEditor.currentPanel = undefined;
        this._panel.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
exports.PatternEditor = PatternEditor;
PatternEditor.viewType = 'vibelang.patternEditor';
//# sourceMappingURL=patternEditor.js.map