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
const sharedComponents_1 = require("./sharedComponents");
class PatternEditor {
    constructor(panel, store) {
        this._disposables = [];
        this._lanes = [];
        this._selectedGroup = null;
        this._pendingPatternName = null;
        this._webviewReady = false;
        this._eventBuffer = new Map();
        this._voiceKeyAssignments = new Map();
        this._panel = panel;
        this._store = store;
        this._updateContent();
        // Listen for state updates
        this._disposables.push(store.onTransportUpdate((transport) => this._sendTransportUpdate(transport)));
        this._disposables.push(store.onFullUpdate(() => this._refreshFromState()));
        // Listen for document text changes to detect pattern edits in code
        this._disposables.push(vscode.workspace.onDidChangeTextDocument((e) => {
            if (e.document.fileName.endsWith('.vibe') && e.contentChanges.length > 0) {
                // Debounce the refresh to avoid too many updates while typing
                this._scheduleDocumentRefresh();
            }
        }));
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
        // Auto-assign voices to numpad keys when loading a group
        this._autoAssignVoices();
        this._sendLanesUpdate();
    }
    /**
     * Auto-assign voices from loaded lanes to numpad keys 1-9.
     * Maps lanes in order to key indices 0-8 (displayed as keys 1-9).
     */
    _autoAssignVoices() {
        this._voiceKeyAssignments.clear();
        // Assign voices from lanes to keys 1-9 (indices 0-8)
        this._lanes.slice(0, 9).forEach((lane, index) => {
            if (lane.voiceName) {
                this._voiceKeyAssignments.set(index, {
                    voiceName: lane.voiceName,
                    patternName: lane.patternName,
                });
            }
        });
        this._sendVoiceAssignmentsUpdate();
    }
    /**
     * Send voice key assignments to webview.
     */
    _sendVoiceAssignmentsUpdate() {
        const assignments = [];
        for (const [keyIndex, assignment] of this._voiceKeyAssignments) {
            assignments.push({
                keyIndex,
                voiceName: assignment.voiceName,
                patternName: assignment.patternName,
            });
        }
        this._panel.webview.postMessage({
            type: 'voiceAssignmentsUpdate',
            data: { assignments },
        });
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
        // Get voices for the selected group
        const groupVoices = this._selectedGroup
            ? state.voices
                .filter(v => v.group_path === this._selectedGroup)
                .map(v => ({ name: v.name, groupPath: v.group_path }))
            : [];
        this._panel.webview.postMessage({
            type: 'stateUpdate',
            data: { groups, allPatterns, groupVoices, selectedGroup: this._selectedGroup },
        });
        // Update existing lanes from state - code is law, always sync from source
        // But skip lanes that have been locally modified via pattern editor
        let lanesChanged = false;
        for (let i = 0; i < this._lanes.length; i++) {
            const lane = this._lanes[i];
            const pattern = state.patterns.find(p => p.name === lane.patternName);
            if (pattern) {
                lane.isPlaying = pattern.is_looping || pattern.status?.state === 'playing';
                // Skip pattern content sync if the lane has been locally modified
                // via the pattern editor (but not yet saved to source file)
                if (lane.locallyModified)
                    continue;
                // Check if the pattern content has changed in the source code
                const currentPatternString = pattern.step_pattern || '';
                const lanePatternString = (0, patternParser_1.generatePatternString)(lane.grid);
                if (currentPatternString && currentPatternString !== lanePatternString) {
                    // Code changed - reload grid from source (code is law)
                    lane.grid = (0, patternParser_1.parsePatternString)(currentPatternString);
                    lane.sourceLocation = pattern.source_location;
                    lanesChanged = true;
                }
            }
        }
        // Check for new patterns in the selected group that should be added
        if (this._selectedGroup) {
            const currentPatternNames = new Set(this._lanes.map(l => l.patternName));
            const groupPatterns = state.patterns.filter(p => p.group_path === this._selectedGroup);
            for (const pattern of groupPatterns) {
                if (!currentPatternNames.has(pattern.name)) {
                    // New pattern in group - add it as a lane
                    this._lanes.push(this._patternToLane(pattern, this._lanes.length));
                    lanesChanged = true;
                }
            }
            // Also check for removed patterns
            const statePatternNames = new Set(groupPatterns.map(p => p.name));
            const lanesToRemove = [];
            for (let i = 0; i < this._lanes.length; i++) {
                if (!statePatternNames.has(this._lanes[i].patternName)) {
                    lanesToRemove.push(i);
                }
            }
            // Remove in reverse order to maintain indices
            for (let i = lanesToRemove.length - 1; i >= 0; i--) {
                this._lanes.splice(lanesToRemove[i], 1);
                lanesChanged = true;
            }
        }
        // If lanes changed, re-run auto-assign for voice keys
        if (lanesChanged) {
            this._autoAssignVoices();
        }
        // If we have lanes, resend them
        if (this._lanes.length > 0) {
            this._sendLanesUpdate();
        }
    }
    /**
     * Schedule a debounced refresh when document text changes.
     * This provides quicker updates while the user is editing code.
     */
    _scheduleDocumentRefresh() {
        if (this._documentRefreshTimeout) {
            clearTimeout(this._documentRefreshTimeout);
        }
        // Wait 300ms after the last keystroke before refreshing
        this._documentRefreshTimeout = setTimeout(() => {
            this._refreshLanesFromDocuments();
        }, 300);
    }
    /**
     * Refresh lanes by re-parsing pattern strings from open documents.
     * This is called when document text changes, before the runtime reloads.
     * Handles multi-line pattern definitions by searching forward from the source location.
     */
    async _refreshLanesFromDocuments() {
        let lanesChanged = false;
        for (let i = 0; i < this._lanes.length; i++) {
            const lane = this._lanes[i];
            // Skip lanes that have been locally modified via pattern editor
            // These will only be updated when the user explicitly saves to source
            if (lane.locallyModified)
                continue;
            if (!lane.sourceLocation?.file || !lane.sourceLocation?.line)
                continue;
            try {
                // Find the document (either open or on disk)
                const uri = vscode.Uri.file(lane.sourceLocation.file);
                let document;
                // Check if document is already open
                document = vscode.workspace.textDocuments.find(d => d.uri.fsPath === uri.fsPath);
                if (!document)
                    continue; // Only update from open documents for responsiveness
                const startLineIndex = lane.sourceLocation.line - 1;
                if (startLineIndex < 0 || startLineIndex >= document.lineCount)
                    continue;
                // Search up to 20 lines forward for the .step() call
                // Pattern definitions can span multiple lines
                const maxSearchLines = 20;
                const endLineIndex = Math.min(startLineIndex + maxSearchLines, document.lineCount);
                // Extract pattern string from .step("...") call
                const stepRegex = /\.step\s*\(\s*"([^"]*)"\s*\)/;
                for (let lineIdx = startLineIndex; lineIdx < endLineIndex; lineIdx++) {
                    const lineText = document.lineAt(lineIdx).text;
                    const match = lineText.match(stepRegex);
                    if (match) {
                        const patternString = match[1];
                        const currentGridString = (0, patternParser_1.generatePatternString)(lane.grid);
                        if (patternString !== currentGridString) {
                            // Pattern changed in code - update grid
                            lane.grid = (0, patternParser_1.parsePatternString)(patternString);
                            lanesChanged = true;
                        }
                        break; // Found the .step() call, stop searching
                    }
                    // Stop searching if we hit another pattern() or define_group() call
                    // (we've gone past our pattern definition)
                    if (lineIdx > startLineIndex && /\b(pattern|define_group)\s*\(/.test(lineText)) {
                        break;
                    }
                }
            }
            catch (error) {
                console.error('Failed to refresh lane from document:', error);
            }
        }
        if (lanesChanged) {
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
            case 'addVoiceLane': {
                const voiceName = message.voiceName;
                const voiceConfig = message.config;
                await this._addVoiceLane(voiceName, voiceConfig);
                break;
            }
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
                    const lane = this._lanes[laneIndex];
                    const grid = lane.grid;
                    // Don't override grid config from step updates - the lane's grid is authoritative
                    // Grid config should only change via explicit resize commands
                    // Extend grid if step is beyond current length
                    if (stepIndex >= grid.steps.length) {
                        // Fill with empty steps up to the new index
                        while (grid.steps.length <= stepIndex) {
                            grid.steps.push({ velocity: 0, accent: false });
                        }
                    }
                    grid.steps[stepIndex] = { velocity, accent };
                    // Mark lane as locally modified to prevent document refresh from overwriting
                    lane.locallyModified = true;
                    // Update pattern via HTTP API (live update without file save)
                    await this._updatePatternViaApi(laneIndex);
                }
                break;
            case 'updateLane':
                const updateLaneIndex = message.laneIndex;
                const newGrid = message.grid;
                if (this._lanes[updateLaneIndex]) {
                    this._lanes[updateLaneIndex].grid = newGrid;
                    // Mark lane as locally modified to prevent document refresh from overwriting
                    this._lanes[updateLaneIndex].locallyModified = true;
                    // Update pattern via HTTP API (live update without file save)
                    await this._updatePatternViaApi(updateLaneIndex);
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
                    const lane = this._lanes[clearLaneIndex];
                    lane.grid.steps = lane.grid.steps.map(() => ({ velocity: 0, accent: false }));
                    // Mark lane as locally modified to prevent document refresh from overwriting
                    lane.locallyModified = true;
                    this._sendLanesUpdate();
                    await this._updatePatternViaApi(clearLaneIndex);
                }
                break;
            // ========== Recording Feature Message Handlers ==========
            case 'triggerAndRecord': {
                const voiceName = message.voiceName;
                // Trigger the voice for audio feedback
                try {
                    await this._store.runtime.triggerVoice(voiceName);
                }
                catch (err) {
                    console.error('Failed to trigger voice:', err);
                }
                break;
            }
            case 'assignVoiceToKey': {
                const keyIndex = message.keyIndex;
                const assignVoiceName = message.voiceName;
                const lane = this._lanes.find(l => l.voiceName === assignVoiceName);
                this._voiceKeyAssignments.set(keyIndex, {
                    voiceName: assignVoiceName,
                    patternName: lane?.patternName || `${assignVoiceName}_pattern`,
                });
                this._sendVoiceAssignmentsUpdate();
                break;
            }
            case 'autoAssignVoices':
                this._autoAssignVoices();
                break;
            case 'writeToFile': {
                const code = message.code;
                await this._writeCodeToFile(code);
                break;
            }
            case 'copyToClipboard': {
                const text = message.text;
                await vscode.env.clipboard.writeText(text);
                vscode.window.showInformationMessage('Pattern code copied to clipboard');
                break;
            }
            case 'getAvailableVoices':
                this._sendAvailableVoices();
                break;
            case 'writeBackAllToFile':
                await this._writeBackAllLanesToFile();
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
    /**
     * Add a new empty lane for a voice that doesn't have an existing pattern.
     */
    async _addVoiceLane(voiceName, config) {
        // Check if already in lanes
        if (this._lanes.some(l => l.voiceName === voiceName)) {
            return;
        }
        // Generate lane color
        const colors = [
            '#9bbb59', '#d19a66', '#569cd6', '#c586c0', '#d16969',
            '#4ec9b0', '#dcdcaa', '#ce9178', '#6a9955', '#b5cea8'
        ];
        const color = colors[this._lanes.length % colors.length];
        // Use provided config, or fallback to first lane, or defaults
        const stepsPerBar = config?.stepsPerBar || this._lanes[0]?.grid.stepsPerBar || 16;
        const numBars = config?.numBars || this._lanes[0]?.grid.numBars || 1;
        const newLane = {
            patternName: `${voiceName}_pattern`,
            voiceName: voiceName,
            groupPath: this._selectedGroup || '',
            grid: (0, patternParser_1.createEmptyGrid)({ stepsPerBar, numBars, beatsPerBar: 4 }),
            sourceLocation: undefined, // No source - it's a new pattern
            isPlaying: false,
            color,
        };
        this._lanes.push(newLane);
        this._autoAssignVoices(); // Re-assign voices including the new one
        this._sendLanesUpdate();
    }
    /**
     * Update pattern via HTTP API for live playback changes (no file save).
     */
    async _updatePatternViaApi(laneIndex) {
        const lane = this._lanes[laneIndex];
        if (!lane)
            return;
        const patternString = (0, patternParser_1.generatePatternString)(lane.grid);
        const loopBeats = lane.grid.numBars * lane.grid.beatsPerBar;
        console.log(`[PatternEditor] Updating pattern '${lane.patternName}':`, {
            patternString,
            loopBeats,
            gridSteps: lane.grid.steps.length,
            stepsPerBar: lane.grid.stepsPerBar,
            numBars: lane.grid.numBars,
        });
        try {
            await this._store.runtime.updatePattern(lane.patternName, {
                pattern_string: patternString,
                loop_beats: loopBeats,
            });
        }
        catch (error) {
            console.error('Failed to update pattern via API:', error);
        }
    }
    /**
     * Write the pattern back to the source file (explicit user action).
     * Handles multi-line pattern definitions by searching forward from the source location.
     */
    async _updateLaneCode(laneIndex) {
        const lane = this._lanes[laneIndex];
        if (!lane?.sourceLocation?.file || !lane?.sourceLocation?.line) {
            return false;
        }
        const newPatternString = (0, patternParser_1.generatePatternString)(lane.grid);
        try {
            const document = await vscode.workspace.openTextDocument(lane.sourceLocation.file);
            const startLineIndex = lane.sourceLocation.line - 1;
            // Search up to 20 lines forward for the .step() call
            // Pattern definitions can span multiple lines
            const maxSearchLines = 20;
            const endLineIndex = Math.min(startLineIndex + maxSearchLines, document.lineCount);
            // Find the .step("...") call across multiple lines
            const stepRegex = /\.step\s*\(\s*"([^"]*)"\s*\)/;
            for (let lineIdx = startLineIndex; lineIdx < endLineIndex; lineIdx++) {
                const lineText = document.lineAt(lineIdx).text;
                const match = lineText.match(stepRegex);
                if (match) {
                    const start = lineText.indexOf(match[0]);
                    const end = start + match[0].length;
                    const edit = new vscode.WorkspaceEdit();
                    edit.replace(document.uri, new vscode.Range(lineIdx, start, lineIdx, end), `.step("${newPatternString}")`);
                    await vscode.workspace.applyEdit(edit);
                    // Save the document to trigger live reload
                    await document.save();
                    // Clear the locally modified flag since changes are now in the source file
                    lane.locallyModified = false;
                    return true;
                }
                // Stop searching if we hit another pattern() or define_group() call
                // (we've gone past our pattern definition)
                if (lineIdx > startLineIndex && /\b(pattern|define_group)\s*\(/.test(lineText)) {
                    break;
                }
            }
        }
        catch (error) {
            console.error('Failed to update pattern in code:', error);
        }
        return false;
    }
    /**
     * Write all modified lanes back to their source files.
     */
    async _writeBackAllLanesToFile() {
        let successCount = 0;
        let failCount = 0;
        for (let i = 0; i < this._lanes.length; i++) {
            const success = await this._updateLaneCode(i);
            if (success) {
                successCount++;
            }
            else if (this._lanes[i].sourceLocation?.file) {
                failCount++;
            }
        }
        if (successCount > 0) {
            vscode.window.showInformationMessage(`Written ${successCount} pattern(s) back to file(s)`);
        }
        else if (failCount > 0) {
            vscode.window.showWarningMessage('Could not write patterns - no source location found');
        }
    }
    /**
     * Generate pattern code from the current visual grid state.
     */
    _generateCodeFromVisualGrid() {
        const codeLines = [];
        for (const lane of this._lanes) {
            if (!lane.voiceName)
                continue;
            const patternString = (0, patternParser_1.generatePatternString)(lane.grid);
            codeLines.push(`pattern("${lane.patternName}").on(${lane.voiceName}).step("${patternString}").start();`);
        }
        return codeLines.join('\n');
    }
    async _resizeAllLanes(config) {
        for (let i = 0; i < this._lanes.length; i++) {
            const lane = this._lanes[i];
            const newGrid = (0, patternParser_1.createEmptyGrid)(config);
            const oldTotalSteps = lane.grid.totalSteps;
            // When expanding, duplicate the existing pattern to fill new bars
            // When shrinking, just copy the portion that fits
            for (let j = 0; j < newGrid.totalSteps; j++) {
                // Use modulo to repeat the existing pattern
                const sourceIndex = j % oldTotalSteps;
                if (lane.grid.steps[sourceIndex]) {
                    newGrid.steps[j] = { ...lane.grid.steps[sourceIndex] };
                }
            }
            lane.grid = newGrid;
            // Mark lane as locally modified to prevent document refresh from overwriting
            lane.locallyModified = true;
            await this._updatePatternViaApi(i);
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
        // Mark lane as locally modified to prevent document refresh from overwriting
        lane.locallyModified = true;
        this._sendLanesUpdate();
        await this._updatePatternViaApi(laneIndex);
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
    // ========== Recording Feature: Buffer Management ==========
    /**
     * Prune old events from the rolling buffer to keep only last MAX_BUFFER_BARS worth.
     */
    _pruneEventBuffer() {
        const maxBeats = PatternEditor.MAX_BUFFER_BARS * 4; // 4 beats per bar
        const currentBeat = this._store.state?.transport?.current_beat || 0;
        const minBeat = currentBeat - maxBeats;
        for (const [voiceName, events] of this._eventBuffer) {
            // Remove events older than minBeat
            const pruned = events.filter(e => e.beat >= minBeat);
            this._eventBuffer.set(voiceName, pruned);
        }
    }
    /**
     * Get statistics about the current buffer state.
     */
    _getBufferStats() {
        const allEvents = Array.from(this._eventBuffer.values()).flat();
        if (allEvents.length === 0) {
            return { capturedBars: 0, nonEmptyBars: 0, totalEvents: 0 };
        }
        const currentBeat = this._store.state?.transport?.current_beat || 0;
        const minBeat = Math.min(...allEvents.map(e => e.beat));
        const capturedBars = Math.ceil((currentBeat - minBeat) / 4);
        // Count non-empty bars (bars that contain at least one event)
        const barSet = new Set(allEvents.map(e => Math.floor(e.beat / 4)));
        return {
            capturedBars: Math.min(capturedBars, PatternEditor.MAX_BUFFER_BARS),
            nonEmptyBars: barSet.size,
            totalEvents: allEvents.length,
        };
    }
    /**
     * Send buffer status update to webview.
     */
    _sendBufferStatus() {
        const stats = this._getBufferStats();
        this._panel.webview.postMessage({
            type: 'bufferStatus',
            data: stats,
        });
    }
    /**
     * Send available voices to webview for voice picker.
     */
    _sendAvailableVoices() {
        const state = this._store.state;
        if (!state)
            return;
        // Get all voices from all groups
        const voices = state.voices.map(v => ({
            name: v.name,
            groupPath: v.group_path,
        }));
        this._panel.webview.postMessage({
            type: 'availableVoices',
            data: { voices },
        });
    }
    // ========== Recording Feature: Pattern Code Generation ==========
    /**
     * Generate pattern code from the rolling buffer for the last N bars.
     * @param takeBars Number of bars to capture
     * @param currentBeat The current beat position (interpolated for accuracy)
     */
    _generatePatternCode(takeBars = 4, currentBeat) {
        const stepsPerBar = this._lanes[0]?.grid.stepsPerBar || 16;
        const beatsPerBar = 4;
        const totalSteps = stepsPerBar * takeBars;
        const beatsPerStep = beatsPerBar / stepsPerBar;
        // Use provided beat or fall back to stored transport beat
        const beatNow = currentBeat ?? this._store.state?.transport?.current_beat ?? 0;
        const windowStart = beatNow - (takeBars * beatsPerBar);
        const windowEnd = beatNow;
        const codeLines = [];
        for (const [voiceName, events] of this._eventBuffer) {
            // Filter events to the take window
            const windowEvents = events.filter(e => e.beat >= windowStart && e.beat < windowEnd);
            if (windowEvents.length === 0)
                continue;
            // Create empty grid
            const steps = Array(totalSteps).fill('.');
            // Quantize events to grid (relative to window start)
            for (const event of windowEvents) {
                const relativeBeat = event.beat - windowStart;
                const stepIndex = Math.round(relativeBeat / beatsPerStep);
                if (stepIndex >= 0 && stepIndex < totalSteps) {
                    steps[stepIndex] = event.velocity >= 0.95 ? 'x' :
                        String(Math.max(1, Math.round(event.velocity * 9)));
                }
            }
            // Add bar separators
            const bars = [];
            for (let i = 0; i < takeBars; i++) {
                bars.push(steps.slice(i * stepsPerBar, (i + 1) * stepsPerBar).join(''));
            }
            const patternString = bars.join('|');
            // Find pattern name for this voice from key assignments
            let patternName = `${voiceName}_pattern`;
            for (const [, assignment] of this._voiceKeyAssignments) {
                if (assignment.voiceName === voiceName) {
                    patternName = assignment.patternName;
                    break;
                }
            }
            codeLines.push(`pattern("${patternName}").on(${voiceName}).step("${patternString}").start();`);
        }
        return codeLines.join('\n');
    }
    /**
     * Write generated code to file (at cursor or pick file).
     */
    async _writeCodeToFile(code) {
        const editor = vscode.window.activeTextEditor;
        if (editor && editor.document.fileName.endsWith('.vibe')) {
            // Insert at cursor position in active .vibe file
            await editor.edit(editBuilder => {
                editBuilder.insert(editor.selection.active, code + '\n');
            });
            vscode.window.showInformationMessage('Pattern code inserted at cursor');
        }
        else {
            // Show quick pick to select file
            const files = await vscode.workspace.findFiles('**/*.vibe');
            if (files.length === 0) {
                vscode.window.showErrorMessage('No .vibe files found in workspace');
                return;
            }
            const items = files.map(f => ({
                label: vscode.workspace.asRelativePath(f),
                uri: f,
            }));
            const selected = await vscode.window.showQuickPick(items, {
                placeHolder: 'Select file to write patterns to',
            });
            if (selected) {
                const doc = await vscode.workspace.openTextDocument(selected.uri);
                const edit = new vscode.WorkspaceEdit();
                edit.insert(selected.uri, new vscode.Position(doc.lineCount, 0), '\n' + code + '\n');
                await vscode.workspace.applyEdit(edit);
                await doc.save();
                vscode.window.showInformationMessage(`Pattern code written to ${selected.label}`);
            }
        }
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
            /* Background colors - using VSCode theme variables */
            --bg-primary: var(--vscode-editor-background);
            --bg-secondary: var(--vscode-sideBar-background, var(--vscode-editor-background));
            --bg-tertiary: var(--vscode-editorWidget-background, var(--vscode-sideBar-background));
            --bg-lane: var(--vscode-list-hoverBackground, var(--vscode-editor-background));
            --bg-elevated: var(--vscode-dropdown-background, var(--vscode-input-background));

            /* Text colors - using VSCode theme variables */
            --text-primary: var(--vscode-editor-foreground);
            --text-secondary: var(--vscode-descriptionForeground, var(--vscode-foreground));
            --text-muted: var(--vscode-disabledForeground, var(--vscode-descriptionForeground));

            /* Accent colors - using VSCode theme variables where possible */
            --accent-green: var(--vscode-charts-green, var(--vscode-terminal-ansiGreen, #3fb950));
            --accent-green-dim: color-mix(in srgb, var(--accent-green) 15%, transparent);
            --accent-orange: var(--vscode-charts-orange, var(--vscode-terminal-ansiYellow, #d29922));
            --accent-orange-dim: color-mix(in srgb, var(--accent-orange) 15%, transparent);
            --accent-blue: var(--vscode-textLink-foreground, var(--vscode-charts-blue, #58a6ff));
            --accent-blue-dim: color-mix(in srgb, var(--accent-blue) 15%, transparent);
            --accent-purple: var(--vscode-charts-purple, var(--vscode-terminal-ansiMagenta, #a371f7));
            --accent-red: var(--vscode-errorForeground, var(--vscode-charts-red, #f85149));
            --accent-red-dim: color-mix(in srgb, var(--accent-red) 15%, transparent);

            /* UI colors - using VSCode theme variables */
            --border: var(--vscode-panel-border, var(--vscode-widget-border, var(--vscode-editorWidget-border)));
            --step-off: var(--vscode-editor-background);
            --step-hover: var(--accent-blue-dim);
            --playhead: var(--vscode-errorForeground, #f85149);
            --beat-line: color-mix(in srgb, var(--border) 60%, transparent);
            --bar-line: color-mix(in srgb, var(--text-secondary) 40%, transparent);
            --shadow: var(--vscode-widget-shadow, rgba(0, 0, 0, 0.4));

            /* Transitions */
            --transition-fast: 0.1s ease;
            --transition-normal: 0.2s ease;
        }

        /* Tooltips */
        [data-tooltip] {
            position: relative;
        }
        [data-tooltip]::after {
            content: attr(data-tooltip);
            position: absolute;
            bottom: 100%;
            left: 50%;
            transform: translateX(-50%) translateY(-4px);
            padding: 6px 10px;
            background: var(--bg-elevated);
            color: var(--text-primary);
            font-size: 11px;
            font-weight: 400;
            white-space: nowrap;
            border-radius: 6px;
            box-shadow: 0 4px 12px rgba(0,0,0,0.4);
            opacity: 0;
            pointer-events: none;
            transition: all 0.15s ease;
            z-index: 1000;
        }
        [data-tooltip]:hover::after {
            opacity: 1;
            transform: translateX(-50%) translateY(-8px);
        }

        /* Toast notifications */
        .toast {
            position: fixed;
            bottom: 80px;
            left: 50%;
            transform: translateX(-50%) translateY(20px);
            padding: 10px 20px;
            background: var(--bg-elevated);
            color: var(--text-primary);
            border-radius: 8px;
            box-shadow: 0 4px 20px rgba(0,0,0,0.4);
            font-size: 13px;
            font-weight: 500;
            opacity: 0;
            pointer-events: none;
            transition: all 0.2s ease;
            z-index: 9999;
        }
        .toast.visible {
            opacity: 1;
            transform: translateX(-50%) translateY(0);
        }
        .toast.success { border-left: 3px solid var(--accent-green); }
        .toast.error { border-left: 3px solid var(--accent-red); }

        * { box-sizing: border-box; margin: 0; padding: 0; }

        body {
            font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif);
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: var(--vscode-font-size, 12px);
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
            padding: 8px 0;
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
            border: 1px solid var(--vscode-input-border, var(--border));
            border-radius: 4px;
            background: var(--vscode-input-background);
            color: var(--vscode-input-foreground);
            font-size: 11px;
            font-family: inherit;
        }

        select:focus, input:focus {
            outline: none;
            border-color: var(--vscode-focusBorder, var(--accent-blue));
        }

        .btn {
            padding: 4px 10px;
            border: 1px solid var(--vscode-button-border, transparent);
            border-radius: 4px;
            background: var(--vscode-button-secondaryBackground, var(--bg-tertiary));
            color: var(--vscode-button-secondaryForeground, var(--text-primary));
            cursor: pointer;
            font-size: 11px;
            font-family: inherit;
            transition: all 0.1s ease;
        }

        .btn:hover {
            background: var(--vscode-button-secondaryHoverBackground, var(--bg-elevated));
        }

        .btn.active {
            background: var(--vscode-button-background, var(--accent-green));
            color: var(--vscode-button-foreground, #fff);
            border-color: var(--vscode-button-background, var(--accent-green));
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
            background: var(--vscode-button-hoverBackground, var(--accent-blue));
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
            background: var(--vscode-list-activeSelectionBackground, var(--accent-blue));
            color: var(--vscode-list-activeSelectionForeground, white);
        }

        .lane-btn.play.active {
            background: var(--accent-green);
            color: var(--vscode-button-foreground, white);
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
            box-sizing: border-box; /* Include borders in width calculation */
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
            box-sizing: border-box; /* Include borders in width calculation */
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
            pointer-events: none; /* Ensure clicks hit the cell, not the dot */
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
            padding: 6px 0;
            background: var(--vscode-statusBar-background, var(--bg-secondary));
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
            font-family: var(--vscode-editor-font-family, 'SF Mono', Monaco, monospace);
        }

        /* ========== Recording Feature Styles ========== */

        /* Capture status (always recording indicator) */
        .capture-group {
            margin-left: auto;
        }
        .capture-status {
            display: flex;
            align-items: center;
            gap: 4px;
            font-size: 11px;
            color: var(--text-secondary);
        }
        .record-dot {
            width: 8px;
            height: 8px;
            background: var(--accent-red);
            border-radius: 50%;
            animation: pulse 2s infinite;
        }
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.5; }
        }

        /* Voice assignment panel - uses shared styles from getRecordingPanelStyles() */
        ${(0, sharedComponents_1.getRecordingPanelStyles)()}

        /* Generated code panel and record button - uses shared styles */
        ${(0, sharedComponents_1.getCodePanelStyles)()}
        ${(0, sharedComponents_1.getTimingControlStyles)()}

        /* Flash effect for recorded steps */
        .step.just-recorded .step-dot {
            animation: step-flash 0.3s ease-out;
        }
        @keyframes step-flash {
            0% { transform: scale(1.5); box-shadow: 0 0 12px currentColor; }
            100% { transform: scale(1); box-shadow: none; }
        }

        /* Box selection styles */
        .selection-box {
            position: absolute;
            border: 1px dashed var(--accent-blue);
            background: rgba(86, 156, 214, 0.15);
            pointer-events: none;
            z-index: 50;
            display: none;
        }

        .selection-box.visible {
            display: block;
        }

        .step.selected {
            background: rgba(86, 156, 214, 0.25);
        }

        .step.selected .step-dot {
            box-shadow: 0 0 4px var(--accent-blue), 0 0 8px rgba(86, 156, 214, 0.5);
        }

        /* Cursor styles for selection and drag */
        .grid-body.selecting {
            cursor: crosshair;
        }

        .grid-body.dragging-selection {
            cursor: ew-resize;
        }

        .step.selected:hover {
            cursor: ew-resize;
        }
    </style>
</head>
<body>
    <div class="toolbar">
        <div class="toolbar-group">
            <span class="toolbar-label">Group</span>
            <select id="groupSelect" data-tooltip="Select a group to show its patterns">
                <option value="">Select group...</option>
            </select>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <span class="toolbar-label">Steps</span>
            <select id="stepsPerBar" data-tooltip="Steps per bar (resolution)">
                <option value="4">4</option>
                <option value="8">8</option>
                <option value="16" selected>16</option>
                <option value="32">32</option>
            </select>
        </div>

        <div class="toolbar-group">
            <span class="toolbar-label">Bars</span>
            <select id="numBars" data-tooltip="Number of bars in pattern">
                <option value="1" selected>1</option>
                <option value="2">2</option>
                <option value="4">4</option>
                <option value="8">8</option>
            </select>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <span class="toolbar-label">Euclidean</span>
            <input type="number" id="euclideanHits" min="0" max="64" value="4" style="width: 50px;" data-tooltip="Number of hits for Euclidean rhythm">
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group" data-tooltip="Adjust timing offset to compensate for latency">
            <span class="toolbar-label">Offset</span>
            <input type="range" id="timingOffset" min="-200" max="100" value="-50" style="width: 100px;">
            <span id="timingOffsetValue" style="width: 50px; text-align: right; font-family: monospace;">-50ms</span>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <button class="btn record-btn" id="recordBtn" data-tooltip="Enable recording mode. Numpad keys will record to patterns.">
                <span class="record-indicator"></span> REC
            </button>
        </div>
    </div>

    <!-- Voice Picker (hidden dropdown) -->
    <div class="voice-picker" id="voicePicker"></div>

    <div class="main-container">
        <div class="lane-list">
            <div class="lane-list-header">
                <span class="lane-list-title">Patterns</span>
                <button class="add-lane-btn" id="addLaneBtn" data-tooltip="Add a pattern to the editor">+</button>
            </div>
            <div class="lane-items" id="laneItems"></div>
        </div>

        <div class="grid-area">
            <div class="grid-header" id="gridHeader"></div>
            <div class="grid-body" id="gridBody">
                <div class="grid-lanes" id="gridLanes"></div>
                <div class="playhead" id="playhead"></div>
                <div class="selection-box" id="selectionBox"></div>
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

    <!-- Voice Assignment Panel (for numpad recording) - using shared component -->
    ${(0, sharedComponents_1.renderKeyAssignmentPanel)('voices', 'Numpad Recording', 'Press 1-9 to trigger voices', true)}

    <!-- Generated Code Panel -->
    <div class="generated-code-panel" id="generatedCodePanel">
        <div class="code-panel-header">
            <span class="code-panel-title">Generated Code</span>
            <div class="code-panel-controls">
                <button class="btn btn-small" id="copyCodeBtn" data-tooltip="Copy code to clipboard">📋 Copy</button>
                <button class="btn btn-small" id="writeBackBtn" data-tooltip="Save changes back to source file" style="background: var(--accent-orange-dim); color: var(--accent-orange); border-color: var(--accent-orange);">💾 Save to File</button>
                <button class="btn btn-small" id="toggleCodePanel" data-tooltip="Toggle panel">▼</button>
            </div>
        </div>
        <pre class="code-output empty" id="codeOutput">Edit patterns above to see code here...</pre>
    </div>

    <!-- Toast notifications -->
    <div class="toast" id="toast"></div>

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
            groupVoices: [], // Voices in the selected group
            lanes: [],
            selectedGroup: null,
            transport: { current_beat: 0, bpm: 120, running: false },
            lastTransportUpdate: performance.now(), // Timestamp for smooth interpolation
            stepsPerBar: 16,
            numBars: 1,
            // Recording feature state
            voiceAssignments: new Map(), // keyIndex (0-8) → { voiceName, patternName }
            availableVoices: [],
            generatedCode: '',
            // Visual recording mode
            isRecording: false,
            timingOffsetMs: -50, // Default negative offset to compensate for latency
            // Box selection state
            selectedSteps: [], // Array of { laneIndex, stepIndex }
            isBoxSelecting: false,
            isDraggingSelection: false,
            selectionStart: null, // { x, y } - grid-relative coordinates
            selectionEnd: null,
            dragStartStep: null, // Step index where drag started
        };

        // Track held keys to prevent key repeat
        let heldKeys = new Set();

        // Elements
        const groupSelect = document.getElementById('groupSelect');
        const stepsPerBarSelect = document.getElementById('stepsPerBar');
        const numBarsSelect = document.getElementById('numBars');
        const laneItems = document.getElementById('laneItems');
        const gridHeader = document.getElementById('gridHeader');
        const gridLanes = document.getElementById('gridLanes');
        const gridBody = document.getElementById('gridBody');
        const playhead = document.getElementById('playhead');
        const selectionBox = document.getElementById('selectionBox');

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
                        state.groupVoices = message.data.groupVoices || [];
                        state.selectedGroup = message.data.selectedGroup;
                        updateGroupSelect();
                        break;
                    case 'lanesUpdate':
                        // Skip lane updates while dragging selection to prevent snapping back
                        if (state.isDraggingSelection || state.isBoxSelecting) break;
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
                        state.lastTransportUpdate = performance.now();
                        break;
                    case 'voiceAssignmentsUpdate':
                        // Convert array back to Map
                        state.voiceAssignments = new Map();
                        for (const a of message.data.assignments) {
                            state.voiceAssignments.set(a.keyIndex, { voiceName: a.voiceName, patternName: a.patternName });
                        }
                        updateKeyGrid();
                        break;
                    case 'bufferStatus':
                        state.bufferStats = message.data;
                        updateBufferStatus();
                        break;
                    case 'generatedCode':
                        state.generatedCode = message.data.code;
                        updateCodeOutput();
                        break;
                    case 'availableVoices':
                        state.availableVoices = message.data.voices;
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
                row.dataset.laneIndex = laneIndex;

                const totalSteps = state.stepsPerBar * state.numBars;
                const stepsPerBeat = state.stepsPerBar / 4;

                for (let stepIndex = 0; stepIndex < totalSteps; stepIndex++) {
                    const step = lane.grid.steps[stepIndex] || { velocity: 0, accent: false };
                    const stepEl = document.createElement('div');
                    stepEl.className = 'step';
                    stepEl.dataset.laneIndex = laneIndex;
                    stepEl.dataset.stepIndex = stepIndex;

                    const stepInBar = stepIndex % state.stepsPerBar;
                    if (stepInBar === 0) stepEl.classList.add('bar-start');
                    else if (stepInBar % stepsPerBeat === 0) stepEl.classList.add('beat-start');

                    // Check if this step is selected
                    const isSelected = state.selectedSteps.some(
                        s => s.laneIndex === laneIndex && s.stepIndex === stepIndex
                    );
                    if (isSelected) stepEl.classList.add('selected');

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
                        // If we just finished box selecting or dragging, don't toggle
                        if (state.isBoxSelecting || state.isDraggingSelection) return;
                        toggleStep(laneIndex, stepIndex, e.shiftKey);
                    });

                    // Mousedown for velocity drag or selection drag
                    stepEl.addEventListener('mousedown', (e) => {
                        if (e.button !== 0) return;

                        // Check if clicking on a selected step with active markers
                        const isSelected = state.selectedSteps.some(
                            s => s.laneIndex === laneIndex && s.stepIndex === stepIndex
                        );
                        const hasMarker = lane.grid.steps[stepIndex]?.velocity > 0;

                        if (isSelected && hasMarker && state.selectedSteps.length > 0) {
                            // Start dragging the selection
                            startSelectionDrag(stepIndex, e);
                            e.stopPropagation();
                        } else if (hasMarker && !state.isBoxSelecting) {
                            // Normal velocity drag on non-selected step
                            startVelocityDrag(laneIndex, stepIndex, e);
                        }
                    });

                    row.appendChild(stepEl);
                }

                gridLanes.appendChild(row);
            });

            // Auto-update code output when grid changes
            updateCodeOutput();
        }

        function toggleStep(laneIndex, stepIndex, accent) {
            // Skip toggle if we just finished a drag operation
            if (dragMoved) return;

            // Clear selection when clicking a step
            if (state.selectedSteps.length > 0) {
                state.selectedSteps = [];
                renderGridLanes();
            }

            const lane = state.lanes[laneIndex];
            if (!lane) return;

            // Handle steps beyond current grid length (will be extended by backend)
            const step = lane.grid.steps[stepIndex] || { velocity: 0, accent: false };
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

            // Optimistic update - extend array if needed
            while (lane.grid.steps.length <= stepIndex) {
                lane.grid.steps.push({ velocity: 0, accent: false });
            }
            lane.grid.steps[stepIndex] = { velocity: newVelocity, accent: newAccent };
            renderGridLanes();

            // Send update to backend via API (not file writeback)
            vscode.postMessage({
                command: 'updateStep',
                laneIndex,
                stepIndex,
                velocity: newVelocity,
                accent: newAccent,
                // Include current UI config so backend can sync grid metadata
                config: { stepsPerBar: state.stepsPerBar, numBars: state.numBars, beatsPerBar: 4 }
            });
        }

        // Track if we're in the middle of a drag operation
        let isDragging = false;
        let dragMoved = false;

        function startVelocityDrag(laneIndex, stepIndex, startEvent) {
            const lane = state.lanes[laneIndex];
            const startY = startEvent.clientY;
            const startVel = lane.grid.steps[stepIndex].velocity;
            isDragging = true;
            dragMoved = false;

            function onMove(e) {
                const deltaY = startY - e.clientY;
                // Only consider it a drag if we've moved at least 5 pixels
                if (Math.abs(deltaY) > 5) {
                    dragMoved = true;
                }
                if (dragMoved) {
                    const newVel = Math.max(0.1, Math.min(1.0, startVel + deltaY / 100));
                    lane.grid.steps[stepIndex].velocity = newVel;
                    renderGridLanes();
                }
            }

            function onUp() {
                document.removeEventListener('mousemove', onMove);
                document.removeEventListener('mouseup', onUp);
                // Only send update if we actually dragged
                if (dragMoved) {
                    vscode.postMessage({
                        command: 'updateLane',
                        laneIndex,
                        grid: lane.grid,
                    });
                }
                // Reset drag state after a short delay to allow click to check it
                setTimeout(() => {
                    isDragging = false;
                    dragMoved = false;
                }, 10);
            }

            document.addEventListener('mousemove', onMove);
            document.addEventListener('mouseup', onUp);
        }

        // ========== Box Selection Functions ==========

        const STEP_WIDTH = 24;
        const LANE_HEIGHT = 40;

        function getGridPosition(clientX, clientY) {
            const rect = gridLanes.getBoundingClientRect();
            return {
                x: clientX - rect.left + gridBody.scrollLeft,
                y: clientY - rect.top + gridBody.scrollTop
            };
        }

        function getStepFromPosition(x, y) {
            const stepIndex = Math.floor(x / STEP_WIDTH);
            const laneIndex = Math.floor(y / LANE_HEIGHT);
            const totalSteps = state.stepsPerBar * state.numBars;
            return {
                stepIndex: Math.max(0, Math.min(stepIndex, totalSteps - 1)),
                laneIndex: Math.max(0, Math.min(laneIndex, state.lanes.length - 1))
            };
        }

        function startBoxSelection(e) {
            if (e.target.closest('.step')?.classList.contains('on')) {
                // Don't start box selection if clicking on an active step
                return;
            }

            const startX = e.clientX;
            const startY = e.clientY;
            const pos = getGridPosition(startX, startY);
            let dragStarted = false;
            const DRAG_THRESHOLD = 5; // pixels before considering it a drag

            function onMove(e) {
                const deltaX = Math.abs(e.clientX - startX);
                const deltaY = Math.abs(e.clientY - startY);

                // Only start box selection if we've moved past the threshold
                if (!dragStarted && (deltaX > DRAG_THRESHOLD || deltaY > DRAG_THRESHOLD)) {
                    dragStarted = true;
                    state.isBoxSelecting = true;
                    state.selectionStart = pos;
                    gridBody.classList.add('selecting');
                    selectionBox.classList.add('visible');
                }

                if (dragStarted) {
                    state.selectionEnd = getGridPosition(e.clientX, e.clientY);
                    updateSelectionBox();
                    updateSelectedStepsFromBox();
                }
            }

            function onUp(e) {
                document.removeEventListener('mousemove', onMove);
                document.removeEventListener('mouseup', onUp);

                if (dragStarted) {
                    selectionBox.classList.remove('visible');
                    gridBody.classList.remove('selecting');

                    // Finalize selection - only include active markers
                    finalizeBoxSelection();

                    // Reset box selection state after a short delay
                    setTimeout(() => {
                        state.isBoxSelecting = false;
                        state.selectionStart = null;
                        state.selectionEnd = null;
                    }, 10);
                }
            }

            document.addEventListener('mousemove', onMove);
            document.addEventListener('mouseup', onUp);
        }

        function updateSelectionBox() {
            if (!state.selectionStart || !state.selectionEnd) return;

            const x1 = Math.min(state.selectionStart.x, state.selectionEnd.x);
            const y1 = Math.min(state.selectionStart.y, state.selectionEnd.y);
            const x2 = Math.max(state.selectionStart.x, state.selectionEnd.x);
            const y2 = Math.max(state.selectionStart.y, state.selectionEnd.y);

            selectionBox.style.left = x1 + 'px';
            selectionBox.style.top = y1 + 'px';
            selectionBox.style.width = (x2 - x1) + 'px';
            selectionBox.style.height = (y2 - y1) + 'px';
        }

        function updateSelectedStepsFromBox() {
            if (!state.selectionStart || !state.selectionEnd) return;

            const x1 = Math.min(state.selectionStart.x, state.selectionEnd.x);
            const y1 = Math.min(state.selectionStart.y, state.selectionEnd.y);
            const x2 = Math.max(state.selectionStart.x, state.selectionEnd.x);
            const y2 = Math.max(state.selectionStart.y, state.selectionEnd.y);

            const start = getStepFromPosition(x1, y1);
            const end = getStepFromPosition(x2, y2);

            // Highlight steps within the box (visual feedback during drag)
            const steps = gridLanes.querySelectorAll('.step');
            steps.forEach(stepEl => {
                const li = parseInt(stepEl.dataset.laneIndex);
                const si = parseInt(stepEl.dataset.stepIndex);
                const inBox = li >= start.laneIndex && li <= end.laneIndex &&
                              si >= start.stepIndex && si <= end.stepIndex;
                stepEl.classList.toggle('selected', inBox);
            });
        }

        function finalizeBoxSelection() {
            if (!state.selectionStart || !state.selectionEnd) {
                state.selectedSteps = [];
                return;
            }

            const x1 = Math.min(state.selectionStart.x, state.selectionEnd.x);
            const y1 = Math.min(state.selectionStart.y, state.selectionEnd.y);
            const x2 = Math.max(state.selectionStart.x, state.selectionEnd.x);
            const y2 = Math.max(state.selectionStart.y, state.selectionEnd.y);

            const start = getStepFromPosition(x1, y1);
            const end = getStepFromPosition(x2, y2);

            // Only select steps that have active markers
            state.selectedSteps = [];
            for (let laneIndex = start.laneIndex; laneIndex <= end.laneIndex; laneIndex++) {
                const lane = state.lanes[laneIndex];
                if (!lane) continue;
                for (let stepIndex = start.stepIndex; stepIndex <= end.stepIndex; stepIndex++) {
                    const step = lane.grid.steps[stepIndex];
                    if (step && step.velocity > 0) {
                        state.selectedSteps.push({ laneIndex, stepIndex });
                    }
                }
            }

            renderGridLanes();
        }

        // ========== Selection Drag (Shift Rhythm) ==========

        function startSelectionDrag(startStepIndex, e) {
            if (state.selectedSteps.length === 0) return;

            state.isDraggingSelection = true;
            state.dragStartStep = startStepIndex;
            const startX = e.clientX;
            let lastDelta = 0;

            gridBody.classList.add('dragging-selection');

            // Store original step data for all selected steps
            const originalData = state.selectedSteps.map(sel => {
                const lane = state.lanes[sel.laneIndex];
                return {
                    laneIndex: sel.laneIndex,
                    stepIndex: sel.stepIndex,
                    step: { ...lane.grid.steps[sel.stepIndex] }
                };
            });

            function onMove(e) {
                if (!state.isDraggingSelection) return;

                const deltaX = e.clientX - startX;
                const stepDelta = Math.round(deltaX / STEP_WIDTH);

                if (stepDelta !== lastDelta) {
                    lastDelta = stepDelta;
                    applySelectionShift(originalData, stepDelta);
                }
            }

            function onUp() {
                document.removeEventListener('mousemove', onMove);
                document.removeEventListener('mouseup', onUp);

                gridBody.classList.remove('dragging-selection');

                // Commit the changes to backend
                commitSelectionChanges();

                setTimeout(() => {
                    state.isDraggingSelection = false;
                    state.dragStartStep = null;
                }, 10);
            }

            document.addEventListener('mousemove', onMove);
            document.addEventListener('mouseup', onUp);
        }

        function applySelectionShift(originalData, stepDelta) {
            const totalSteps = state.stepsPerBar * state.numBars;

            // First, clear all original positions
            for (const orig of originalData) {
                const lane = state.lanes[orig.laneIndex];
                lane.grid.steps[orig.stepIndex] = { velocity: 0, accent: false };
            }

            // Then, place at new positions (wrapping within bounds)
            const newSelected = [];
            for (const orig of originalData) {
                let newStepIndex = orig.stepIndex + stepDelta;

                // Wrap around within the pattern
                while (newStepIndex < 0) newStepIndex += totalSteps;
                while (newStepIndex >= totalSteps) newStepIndex -= totalSteps;

                const lane = state.lanes[orig.laneIndex];

                // Ensure steps array is long enough
                while (lane.grid.steps.length <= newStepIndex) {
                    lane.grid.steps.push({ velocity: 0, accent: false });
                }

                lane.grid.steps[newStepIndex] = { ...orig.step };
                newSelected.push({ laneIndex: orig.laneIndex, stepIndex: newStepIndex });
            }

            // Update selection to new positions
            state.selectedSteps = newSelected;
            renderGridLanes();
        }

        function commitSelectionChanges() {
            // Find which lanes were affected and update them
            const affectedLanes = new Set(state.selectedSteps.map(s => s.laneIndex));

            for (const laneIndex of affectedLanes) {
                const lane = state.lanes[laneIndex];
                vscode.postMessage({
                    command: 'updateLane',
                    laneIndex,
                    grid: lane.grid,
                });
            }
        }

        function clearSelection() {
            if (state.selectedSteps.length > 0) {
                state.selectedSteps = [];
                renderGridLanes();
            }
        }

        // Setup box selection on grid body
        function setupBoxSelection() {
            gridBody.addEventListener('mousedown', (e) => {
                // Only start box selection if clicking directly on grid-body or grid-lanes background
                // (not on a step element with a marker)
                if (e.button !== 0) return;
                if (e.target === gridBody || e.target === gridLanes ||
                    (e.target.classList.contains('step') && !e.target.classList.contains('on'))) {
                    startBoxSelection(e);
                }
            });

            // Escape key to clear selection
            document.addEventListener('keydown', (e) => {
                if (e.key === 'Escape') {
                    clearSelection();
                }
            });
        }

        function showPatternPicker(rect) {
            const picker = document.getElementById('patternPicker');
            const list = document.getElementById('patternPickerList');

            picker.style.left = rect.right + 4 + 'px';
            picker.style.top = rect.top + 'px';

            // Get voices already in lanes
            const laneVoices = new Set(state.lanes.map(l => l.voiceName).filter(v => v));

            // Get voices from the group that don't have a lane yet
            const availableVoices = state.groupVoices.filter(v => !laneVoices.has(v.name));

            // Also get existing patterns not in lanes (for patterns without voice assignment)
            const lanePatterns = new Set(state.lanes.map(l => l.patternName));
            const availablePatterns = state.allPatterns
                .filter(p => p.groupPath === state.selectedGroup && !lanePatterns.has(p.name) && !laneVoices.has(p.voiceName));

            if (availableVoices.length === 0 && availablePatterns.length === 0) {
                list.innerHTML = '<div style="padding: 12px; color: var(--text-muted); text-align: center;">All voices are shown</div>';
            } else {
                let html = '';

                // Show available voices first (to create new patterns)
                if (availableVoices.length > 0) {
                    html += '<div style="padding: 6px 12px; font-size: 10px; color: var(--text-muted); border-bottom: 1px solid var(--border);">Voices</div>';
                    html += availableVoices.map(v => \`
                        <div class="pattern-picker-item" data-voice="\${v.name}">
                            \${v.name} <span style="color: var(--accent-green)">+ new pattern</span>
                        </div>
                    \`).join('');
                }

                // Show existing patterns without voice assignment
                if (availablePatterns.length > 0) {
                    html += '<div style="padding: 6px 12px; font-size: 10px; color: var(--text-muted); border-bottom: 1px solid var(--border);">Existing Patterns</div>';
                    html += availablePatterns.map(p => \`
                        <div class="pattern-picker-item" data-pattern="\${p.name}">
                            \${p.name} <span style="color: var(--text-muted)">→ \${p.voiceName || 'no voice'}</span>
                        </div>
                    \`).join('');
                }

                list.innerHTML = html;

                // Voice items - add new lane
                list.querySelectorAll('.pattern-picker-item[data-voice]').forEach(item => {
                    item.addEventListener('click', () => {
                        vscode.postMessage({
                            command: 'addVoiceLane',
                            voiceName: item.dataset.voice,
                            config: { stepsPerBar: state.stepsPerBar, numBars: state.numBars, beatsPerBar: 4 }
                        });
                        picker.classList.remove('visible');
                    });
                });

                // Pattern items - add existing pattern
                list.querySelectorAll('.pattern-picker-item[data-pattern]').forEach(item => {
                    item.addEventListener('click', () => {
                        vscode.postMessage({ command: 'addPattern', patternName: item.dataset.pattern });
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

                // Interpolate beat position based on time elapsed since last transport update
                const now = performance.now();
                const elapsedMs = now - state.lastTransportUpdate;
                const elapsedBeats = (elapsedMs / 1000) * (bpm / 60);
                const interpolatedBeat = current_beat + elapsedBeats;

                const totalBeats = state.numBars * 4;
                const loopBeat = interpolatedBeat % totalBeats;
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

        // ========== Recording Feature Functions ==========

        // Map numpad/digit keys to index 0-8
        function numpadKeyToIndex(code) {
            const mapping = {
                'Numpad1': 0, 'Numpad2': 1, 'Numpad3': 2,
                'Numpad4': 3, 'Numpad5': 4, 'Numpad6': 5,
                'Numpad7': 6, 'Numpad8': 7, 'Numpad9': 8,
                'Digit1': 0, 'Digit2': 1, 'Digit3': 2,
                'Digit4': 3, 'Digit5': 4, 'Digit6': 5,
                'Digit7': 6, 'Digit8': 7, 'Digit9': 8,
            };
            return mapping[code] !== undefined ? mapping[code] : null;
        }

        // Get the interpolated current beat (accounting for time since last transport update)
        // Applies timing offset to compensate for latency
        function getInterpolatedBeat() {
            const { current_beat, bpm, running } = state.transport;
            if (!running) return current_beat;

            const now = performance.now();
            const elapsedMs = now - state.lastTransportUpdate;
            // Apply timing offset - negative values shift recorded beats earlier (compensate for latency)
            const adjustedElapsedMs = elapsedMs + state.timingOffsetMs;
            const elapsedBeats = (adjustedElapsedMs / 1000) * (bpm / 60);
            return current_beat + elapsedBeats;
        }

        // Flash a key slot when pressed
        function flashKeySlot(keyIndex) {
            const slot = document.querySelector(\`#voicesKeyGrid .key-slot[data-key="\${keyIndex}"]\`);
            if (slot) {
                slot.classList.add('flash');
                setTimeout(() => slot.classList.remove('flash'), 150);
            }
        }

        // Place a visual step in the grid during recording
        function placeVisualStep(voiceName, beat) {
            // Find the lane for this voice
            const laneIndex = state.lanes.findIndex(l => l.voiceName === voiceName);
            if (laneIndex === -1) return;

            const lane = state.lanes[laneIndex];
            const totalBeats = state.numBars * 4;
            const stepsPerBeat = state.stepsPerBar / 4;

            // Calculate step position within the loop
            const loopBeat = ((beat % totalBeats) + totalBeats) % totalBeats; // Handle negative beats
            const stepIndex = Math.round(loopBeat * stepsPerBeat);
            const clampedStep = Math.max(0, Math.min(stepIndex, state.stepsPerBar * state.numBars - 1));

            // Update the grid
            while (lane.grid.steps.length <= clampedStep) {
                lane.grid.steps.push({ velocity: 0, accent: false });
            }
            lane.grid.steps[clampedStep] = { velocity: 1.0, accent: false };

            // Update visual immediately
            renderGridLanes();

            // Flash the step
            const rows = gridLanes.querySelectorAll('.lane-row');
            if (rows[laneIndex]) {
                const steps = rows[laneIndex].querySelectorAll('.step');
                if (steps[clampedStep]) {
                    steps[clampedStep].classList.add('just-recorded');
                    setTimeout(() => steps[clampedStep].classList.remove('just-recorded'), 300);
                }
            }

            // Send update to backend to persist
            vscode.postMessage({
                command: 'updateStep',
                laneIndex,
                stepIndex: clampedStep,
                velocity: 1.0,
                accent: false,
                config: { stepsPerBar: state.stepsPerBar, numBars: state.numBars, beatsPerBar: 4 }
            });
        }

        // Update key grid display
        function updateKeyGrid() {
            const slots = document.querySelectorAll('#voicesKeyGrid .key-slot');
            slots.forEach(slot => {
                const keyIndex = parseInt(slot.dataset.key);
                const assignment = state.voiceAssignments.get(keyIndex);
                const labelEl = slot.querySelector('.key-label');
                if (assignment) {
                    labelEl.textContent = assignment.voiceName;
                    labelEl.classList.remove('empty');
                } else {
                    labelEl.textContent = 'click to assign';
                    labelEl.classList.add('empty');
                }
            });
        }

        // Toast notifications
        let toastTimeout = null;
        function showToast(message, type = 'success') {
            const toast = document.getElementById('toast');
            if (!toast) return;
            if (toastTimeout) clearTimeout(toastTimeout);
            toast.textContent = message;
            toast.className = 'toast visible ' + type;
            toastTimeout = setTimeout(() => toast.classList.remove('visible'), 2500);
        }

        // Update buffer status display
        function updateBufferStatus() {
            const countEl = document.getElementById('capturedBarsCount');
            if (countEl) {
                countEl.textContent = state.bufferStats.totalEvents;
            }
        }

        // Update code output display
        // Generate code from visual grid and update display
        function updateCodeOutput() {
            // Generate code from lanes
            const codeLines = [];
            for (const lane of state.lanes) {
                if (!lane.voiceName) continue;
                const patternStr = generatePatternStringFromGrid(lane.grid);
                codeLines.push(\`pattern("\${lane.patternName}").on(\${lane.voiceName}).step("\${patternStr}").start();\`);
            }
            state.generatedCode = codeLines.join('\\n');

            const codeEl = document.getElementById('codeOutput');
            if (codeEl) {
                if (state.generatedCode) {
                    codeEl.textContent = state.generatedCode;
                    codeEl.classList.remove('empty');
                } else {
                    codeEl.textContent = 'Edit patterns above to see code here...';
                    codeEl.classList.add('empty');
                }
            }
        }

        // Generate pattern string from grid data (formatted with spaces every 4 chars)
        function generatePatternStringFromGrid(grid) {
            const bars = [];
            for (let barIndex = 0; barIndex < grid.numBars; barIndex++) {
                let barChars = [];
                const startStep = barIndex * grid.stepsPerBar;
                for (let stepIndex = 0; stepIndex < grid.stepsPerBar; stepIndex++) {
                    const step = grid.steps[startStep + stepIndex];
                    if (!step || step.velocity === 0) {
                        barChars.push('.');
                    } else if (step.accent) {
                        barChars.push('X');
                    } else if (step.velocity >= 0.95) {
                        barChars.push('x');
                    } else {
                        const digit = Math.round((step.velocity - 0.1) / 0.9 * 9);
                        barChars.push(Math.max(1, Math.min(9, digit)).toString());
                    }
                }
                // Group into chunks of 4, separated by spaces
                const groups = [];
                for (let i = 0; i < barChars.length; i += 4) {
                    groups.push(barChars.slice(i, i + 4).join(''));
                }
                bars.push(groups.join(' '));
            }
            return bars.join('|');
        }

        // Show voice picker for a key slot
        let activePickerKeyIndex = null;
        function showVoicePicker(keyIndex, rect) {
            activePickerKeyIndex = keyIndex;
            const picker = document.getElementById('voicesPicker');

            // Request available voices
            vscode.postMessage({ command: 'getAvailableVoices' });

            // Position and populate picker
            picker.style.left = (rect.right + 4) + 'px';
            picker.style.top = rect.top + 'px';

            // Get voices from lanes (available in group)
            const voices = state.lanes.map(l => l.voiceName).filter(v => v);
            picker.innerHTML = voices.length > 0
                ? '<div class="key-picker-header">Select Voice</div>' + voices.map(v => {
                    const current = state.voiceAssignments.get(keyIndex);
                    const isSelected = current && current.voiceName === v;
                    return \`<div class="key-picker-item \${isSelected ? 'selected' : ''}" data-voice="\${v}">\${v}</div>\`;
                }).join('')
                : '<div style="padding: 8px; color: var(--text-muted);">No voices in group</div>';

            picker.classList.add('visible');
        }

        // Setup recording event listeners
        function setupRecordingListeners() {
            // Keyboard recording (always on)
            document.addEventListener('keydown', (e) => {
                if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT') {
                    return;
                }

                const keyIndex = numpadKeyToIndex(e.code);
                if (keyIndex === null) return;

                e.preventDefault();
                if (heldKeys.has(e.code)) return; // Ignore key repeat
                heldKeys.add(e.code);

                const assignment = state.voiceAssignments.get(keyIndex);
                if (!assignment) return;

                // Flash the key slot
                flashKeySlot(keyIndex);

                // Use interpolated beat for accurate timing (with offset compensation)
                const currentBeat = getInterpolatedBeat();

                // If visual recording is enabled, place a step in the grid
                if (state.isRecording && state.transport.running) {
                    placeVisualStep(assignment.voiceName, currentBeat);
                }

                // Trigger voice for audio feedback
                vscode.postMessage({
                    command: 'triggerAndRecord',
                    keyIndex: keyIndex,
                    voiceName: assignment.voiceName,
                    beat: currentBeat,
                    velocity: 1.0
                });
            });

            document.addEventListener('keyup', (e) => {
                if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT') {
                    return;
                }
                heldKeys.delete(e.code);
            });

            // Record button toggle
            document.getElementById('recordBtn').addEventListener('click', () => {
                state.isRecording = !state.isRecording;
                const btn = document.getElementById('recordBtn');
                if (state.isRecording) {
                    btn.classList.add('active');
                    showToast('Recording enabled - press numpad keys 1-9 to record', 'success');
                } else {
                    btn.classList.remove('active');
                }
            });

            // Timing offset slider
            const timingSlider = document.getElementById('timingOffset');
            const timingValue = document.getElementById('timingOffsetValue');
            timingSlider.addEventListener('input', () => {
                state.timingOffsetMs = parseInt(timingSlider.value);
                timingValue.textContent = state.timingOffsetMs + 'ms';
            });

            // Auto-assign button
            document.getElementById('voicesAutoAssign').addEventListener('click', () => {
                vscode.postMessage({ command: 'autoAssignVoices' });
            });

            // Toggle voice assignment panel
            document.getElementById('voicesToggle').addEventListener('click', () => {
                const panel = document.getElementById('voicesPanel');
                panel.classList.toggle('collapsed');
            });

            // Copy code button
            document.getElementById('copyCodeBtn').addEventListener('click', () => {
                if (state.generatedCode) {
                    vscode.postMessage({ command: 'copyToClipboard', text: state.generatedCode });
                    showToast('Code copied to clipboard', 'success');
                }
            });

            // Write back to source files button
            document.getElementById('writeBackBtn').addEventListener('click', () => {
                vscode.postMessage({ command: 'writeBackAllToFile' });
            });

            // Toggle code panel
            document.getElementById('toggleCodePanel').addEventListener('click', () => {
                const panel = document.getElementById('generatedCodePanel');
                panel.classList.toggle('collapsed');
                document.getElementById('toggleCodePanel').textContent = panel.classList.contains('collapsed') ? '▶' : '▼';
            });

            // Key slot click - show voice picker
            document.querySelectorAll('#voicesKeyGrid .key-slot').forEach(slot => {
                slot.addEventListener('click', (e) => {
                    const keyIndex = parseInt(slot.dataset.key);
                    showVoicePicker(keyIndex, slot.getBoundingClientRect());
                });
            });

            // Voice picker item click
            document.getElementById('voicesPicker').addEventListener('click', (e) => {
                if (e.target.classList.contains('key-picker-item')) {
                    const voiceName = e.target.dataset.voice;
                    if (activePickerKeyIndex !== null && voiceName) {
                        vscode.postMessage({
                            command: 'assignVoiceToKey',
                            keyIndex: activePickerKeyIndex,
                            voiceName: voiceName
                        });
                    }
                    document.getElementById('voicesPicker').classList.remove('visible');
                    activePickerKeyIndex = null;
                }
            });

            // Close voice picker when clicking outside
            document.addEventListener('click', (e) => {
                const picker = document.getElementById('voicesPicker');
                const isKeySlot = e.target.closest('.key-slot');
                if (!picker.contains(e.target) && !isKeySlot) {
                    picker.classList.remove('visible');
                    activePickerKeyIndex = null;
                }
            });
        }

        // Extend init to include recording listeners and box selection
        const originalInit = init;
        init = function() {
            setupEventListeners();
            setupRecordingListeners();
            setupBoxSelection();
            vscode.postMessage({ command: 'ready' });
            requestAnimationFrame(animatePlayhead);
        };

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
            font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif);
            background: var(--vscode-editor-background);
            color: var(--vscode-editor-foreground);
            display: flex;
            align-items: center;
            justify-content: center;
            height: 100vh;
            margin: 0;
        }
        .empty-state { text-align: center; color: var(--vscode-descriptionForeground); }
        .empty-icon { font-size: 64px; margin-bottom: 20px; opacity: 0.3; }
        h2 { font-size: 18px; font-weight: 500; margin-bottom: 8px; color: var(--vscode-editor-foreground); }
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
        if (this._documentRefreshTimeout) {
            clearTimeout(this._documentRefreshTimeout);
        }
        this._panel.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
exports.PatternEditor = PatternEditor;
PatternEditor.viewType = 'vibelang.patternEditor';
// Recording feature: rolling buffer and voice key assignments
PatternEditor.MAX_BUFFER_BARS = 256;
//# sourceMappingURL=patternEditor.js.map