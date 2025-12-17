/**
 * VibeLang Melody Editor (Piano Roll)
 *
 * Visual piano roll for editing melodies.
 * Features:
 * - Piano keyboard on left
 * - Horizontal timeline with notes
 * - Click to add notes, drag to resize/move
 * - Configurable grid size and bars
 * - Snap to grid
 * - Real-time playhead sync
 * - Bi-directional code sync
 * - Scale highlighting
 */

import * as vscode from 'vscode';
import { StateStore } from '../state/stateStore';
import {
    MelodyGrid,
    MelodyConfig,
    MelodyNote,
    parseMelodyString,
    generateMelodyString,
    createEmptyMelodyGrid,
    midiToNoteName,
} from '../utils/melodyParser';

interface MelodyEditorState {
    melodyName: string;
    voiceName: string | null;
    grid: MelodyGrid;
    sourceLocation?: { file?: string; line?: number; column?: number };
    originalMelodyString?: string;
}

export class MelodyEditor {
    public static currentPanel: MelodyEditor | undefined;
    public static readonly viewType = 'vibelang.melodyEditor';

    private readonly _panel: vscode.WebviewPanel;
    private readonly _store: StateStore;
    private _disposables: vscode.Disposable[] = [];
    private _currentMelody: MelodyEditorState | null = null;

    private constructor(panel: vscode.WebviewPanel, store: StateStore) {
        this._panel = panel;
        this._store = store;

        this._updateContent();

        // Listen for state updates (for playhead sync)
        this._disposables.push(
            store.onTransportUpdate((transport) => this._sendTransportUpdate(transport))
        );

        this._disposables.push(
            store.onFullUpdate(() => this._refreshMelodyList())
        );

        // Handle messages from webview
        this._panel.webview.onDidReceiveMessage(
            (message) => this._handleMessage(message),
            null,
            this._disposables
        );

        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    }

    public static createOrShow(store: StateStore, melodyName?: string) {
        const column = vscode.ViewColumn.Two;

        if (MelodyEditor.currentPanel) {
            MelodyEditor.currentPanel._panel.reveal(column);
            if (melodyName) {
                MelodyEditor.currentPanel.loadMelody(melodyName);
            }
            return;
        }

        const panel = vscode.window.createWebviewPanel(
            MelodyEditor.viewType,
            'Melody Editor',
            column,
            {
                enableScripts: true,
                retainContextWhenHidden: true,
            }
        );

        MelodyEditor.currentPanel = new MelodyEditor(panel, store);

        if (melodyName) {
            MelodyEditor.currentPanel.loadMelody(melodyName);
        }
    }

    public static revive(panel: vscode.WebviewPanel, store: StateStore) {
        MelodyEditor.currentPanel = new MelodyEditor(panel, store);
    }

    public loadMelody(melodyName: string) {
        const state = this._store.state;
        if (!state) return;

        const melody = state.melodies.find(m => m.name === melodyName);
        if (!melody) {
            vscode.window.showErrorMessage(`Melody "${melodyName}" not found`);
            return;
        }

        // Parse the melody - for now, create from events
        // TODO: Parse original melody string if available
        const melodyString = ''; // Would need to store in API
        const grid = melodyString
            ? parseMelodyString(melodyString)
            : this._eventsToGrid(melody.events, melody.loop_beats);

        this._currentMelody = {
            melodyName: melody.name,
            voiceName: melody.voice_name || null,
            grid,
            sourceLocation: melody.source_location,
            originalMelodyString: melodyString,
        };

        this._sendMelodyUpdate();
    }

    private _eventsToGrid(events: Array<{ beat: number; note?: string; frequency?: number; duration?: number }>, loopBeats: number): MelodyGrid {
        const notes: MelodyNote[] = [];
        const beatsPerBar = 4;
        const numBars = Math.ceil(loopBeats / beatsPerBar);

        for (const event of events) {
            // Convert frequency to MIDI if needed
            let midiNote = 60;
            if (event.frequency) {
                // MIDI = 69 + 12 * log2(freq / 440)
                midiNote = Math.round(69 + 12 * Math.log2(event.frequency / 440));
            }

            notes.push({
                startBeat: event.beat,
                duration: event.duration || 1,
                midiNote,
                velocity: 1.0,
            });
        }

        return {
            notes,
            totalBeats: loopBeats,
            numBars,
            beatsPerBar,
        };
    }

    private _updateContent() {
        this._panel.webview.html = this._getHtmlContent();
        // Send initial state after a short delay
        setTimeout(() => {
            this._refreshMelodyList();
            if (this._currentMelody) {
                this._sendMelodyUpdate();
            }
        }, 100);
    }

    private _refreshMelodyList() {
        const state = this._store.state;
        if (!state) return;

        const melodies = state.melodies.map(m => ({
            name: m.name,
            voiceName: m.voice_name,
            loopBeats: m.loop_beats,
            noteCount: m.events?.length || 0,
            isPlaying: m.is_looping || m.status?.state === 'playing',
        }));

        this._panel.webview.postMessage({
            type: 'melodyList',
            data: melodies,
        });
    }

    private _sendMelodyUpdate() {
        if (!this._currentMelody) return;

        this._panel.webview.postMessage({
            type: 'melodyUpdate',
            data: {
                melodyName: this._currentMelody.melodyName,
                voiceName: this._currentMelody.voiceName,
                grid: this._currentMelody.grid,
                sourceLocation: this._currentMelody.sourceLocation,
            },
        });
    }

    private _sendTransportUpdate(transport: { current_beat: number; bpm: number; running: boolean }) {
        if (!this._currentMelody) return;

        this._panel.webview.postMessage({
            type: 'transportUpdate',
            data: transport,
        });
    }

    private async _handleMessage(message: { command: string; [key: string]: unknown }) {
        switch (message.command) {
            case 'loadMelody':
                this.loadMelody(message.melodyName as string);
                break;

            case 'updateGrid':
                if (this._currentMelody) {
                    this._currentMelody.grid = message.grid as MelodyGrid;
                    // Generate new melody string and update code
                    const stepsPerBar = (message.stepsPerBar as number) || 4;
                    const newMelodyString = generateMelodyString(this._currentMelody.grid, stepsPerBar);
                    await this._updateCodeWithMelody(newMelodyString);
                }
                break;

            case 'resizeGrid':
                if (this._currentMelody) {
                    const config = message.config as MelodyConfig;
                    const newGrid = createEmptyMelodyGrid(config);
                    // Copy existing notes that fit
                    for (const note of this._currentMelody.grid.notes) {
                        if (note.startBeat < newGrid.totalBeats) {
                            newGrid.notes.push({
                                ...note,
                                duration: Math.min(note.duration, newGrid.totalBeats - note.startBeat),
                            });
                        }
                    }
                    this._currentMelody.grid = newGrid;
                    this._sendMelodyUpdate();
                }
                break;

            case 'goToSource':
                if (this._currentMelody?.sourceLocation) {
                    vscode.commands.executeCommand('vibelang.goToSource', this._currentMelody.sourceLocation);
                }
                break;

            case 'togglePlayback':
                if (this._currentMelody) {
                    const state = this._store.state;
                    const melody = state?.melodies.find(m => m.name === this._currentMelody?.melodyName);
                    if (melody?.is_looping || melody?.status?.state === 'playing') {
                        await this._store.runtime.stopMelody(this._currentMelody.melodyName);
                    } else {
                        await this._store.runtime.startMelody(this._currentMelody.melodyName);
                    }
                }
                break;

            case 'previewNote':
                // Trigger a note on the associated voice
                if (this._currentMelody?.voiceName) {
                    const midiNote = message.midiNote as number;
                    const velocity = (message.velocity as number) || 100;
                    const duration = (message.duration as number) || 0.25; // Duration in seconds

                    try {
                        await this._store.runtime.noteOn(
                            this._currentMelody.voiceName,
                            midiNote,
                            velocity
                        );
                        // Schedule note off after duration
                        setTimeout(async () => {
                            if (this._currentMelody?.voiceName) {
                                await this._store.runtime.noteOff(
                                    this._currentMelody.voiceName,
                                    midiNote
                                );
                            }
                        }, duration * 1000);
                    } catch {
                        // Ignore preview errors
                    }
                }
                break;
        }
    }

    private async _updateCodeWithMelody(newMelodyString: string) {
        if (!this._currentMelody?.sourceLocation?.file || !this._currentMelody?.sourceLocation?.line) {
            return;
        }

        const filePath = this._currentMelody.sourceLocation.file;
        const lineNumber = this._currentMelody.sourceLocation.line;

        try {
            const document = await vscode.workspace.openTextDocument(filePath);
            const lineIndex = lineNumber - 1;
            const textLine = document.lineAt(lineIndex);
            const lineText = textLine.text;

            // Find the .notes("...") call and replace the melody string
            const notesRegex = /\.notes\s*\(\s*"([^"]*)"\s*\)/;
            const match = lineText.match(notesRegex);

            if (match) {
                const start = lineText.indexOf(match[0]);
                const end = start + match[0].length;

                const edit = new vscode.WorkspaceEdit();
                edit.replace(
                    document.uri,
                    new vscode.Range(lineIndex, start, lineIndex, end),
                    `.notes("${newMelodyString}")`
                );

                await vscode.workspace.applyEdit(edit);

                // Save the document to trigger live reload
                await document.save();
            }
        } catch (error) {
            console.error('Failed to update melody in code:', error);
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
    <title>Melody Editor</title>
    <style>
        :root {
            --bg-primary: #1a1a1a;
            --bg-secondary: #232323;
            --bg-tertiary: #2d2d2d;
            --text-primary: #d4d4d4;
            --text-secondary: #858585;
            --accent-green: #9bbb59;
            --accent-orange: #d19a66;
            --accent-blue: #569cd6;
            --accent-purple: #c586c0;
            --border: #3c3c3c;
            --note-color: #569cd6;
            --note-selected: #9bbb59;
            --playhead: #ff6b6b;
            --beat-line: #3a3a3a;
            --bar-line: #5a5a5a;
            --white-key: #404040;
            --black-key: #2a2a2a;
            --key-pressed: #569cd6;
            --row-alt: rgba(255, 255, 255, 0.02);
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

        .btn {
            padding: 4px 10px;
            border: 1px solid var(--border);
            border-radius: 3px;
            background: var(--bg-tertiary);
            color: var(--text-primary);
            cursor: pointer;
            font-size: 11px;
        }

        .btn:hover {
            background: #3a3a3a;
        }

        .btn-icon {
            width: 28px;
            height: 28px;
            padding: 0;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 14px;
        }

        /* Main content */
        .main-content {
            flex: 1;
            display: flex;
            overflow: hidden;
        }

        /* Piano keyboard */
        .piano-keyboard {
            width: 60px;
            background: var(--bg-secondary);
            border-right: 1px solid var(--border);
            overflow-y: auto;
            flex-shrink: 0;
        }

        .piano-key {
            height: 16px;
            border-bottom: 1px solid var(--bg-primary);
            display: flex;
            align-items: center;
            justify-content: flex-end;
            padding-right: 4px;
            font-size: 9px;
            color: var(--text-secondary);
            cursor: pointer;
        }

        .piano-key.white {
            background: var(--white-key);
        }

        .piano-key.black {
            background: var(--black-key);
            color: #666;
        }

        .piano-key:hover {
            background: var(--key-pressed);
        }

        .piano-key.c-note {
            font-weight: bold;
            color: var(--text-primary);
        }

        /* Piano roll grid */
        .piano-roll-container {
            flex: 1;
            overflow: auto;
            position: relative;
        }

        .piano-roll {
            position: relative;
            min-width: 100%;
        }

        .grid-row {
            height: 16px;
            border-bottom: 1px solid var(--bg-primary);
            position: relative;
        }

        .grid-row.white {
            background: var(--white-key);
        }

        .grid-row.black {
            background: var(--black-key);
        }

        .grid-row:hover {
            filter: brightness(1.1);
        }

        /* Grid lines */
        .grid-line {
            position: absolute;
            top: 0;
            bottom: 0;
            width: 1px;
            pointer-events: none;
        }

        .grid-line.beat {
            background: var(--beat-line);
        }

        .grid-line.bar {
            background: var(--bar-line);
            width: 2px;
        }

        /* Notes */
        .note {
            position: absolute;
            height: 14px;
            background: var(--note-color);
            border-radius: 2px;
            cursor: pointer;
            border: 1px solid rgba(255, 255, 255, 0.3);
            display: flex;
            align-items: center;
            padding-left: 4px;
            font-size: 9px;
            color: white;
            text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
            overflow: hidden;
            white-space: nowrap;
        }

        .note.selected {
            background: var(--note-selected);
            border-color: white;
        }

        .note:hover {
            filter: brightness(1.2);
        }

        .note-resize-handle {
            position: absolute;
            right: 0;
            top: 0;
            bottom: 0;
            width: 6px;
            cursor: ew-resize;
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

        /* Timeline header */
        .timeline-header {
            height: 24px;
            background: var(--bg-tertiary);
            border-bottom: 1px solid var(--border);
            position: sticky;
            top: 0;
            z-index: 50;
            display: flex;
        }

        .timeline-header-spacer {
            width: 60px;
            flex-shrink: 0;
            border-right: 1px solid var(--border);
        }

        .timeline-header-content {
            flex: 1;
            position: relative;
        }

        .bar-marker {
            position: absolute;
            font-size: 10px;
            color: var(--text-primary);
            transform: translateX(-50%);
        }

        /* Info bar */
        .info-bar {
            display: flex;
            align-items: center;
            gap: 16px;
            padding: 8px 12px;
            background: var(--bg-secondary);
            border-top: 1px solid var(--border);
            font-size: 11px;
        }

        .info-item {
            display: flex;
            align-items: center;
            gap: 4px;
        }

        .info-label {
            color: var(--text-secondary);
        }

        .info-value {
            color: var(--text-primary);
            font-family: 'SF Mono', Monaco, monospace;
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
    </style>
</head>
<body>
    <div class="toolbar">
        <div class="toolbar-group">
            <span class="toolbar-label">Melody:</span>
            <select id="melodySelect">
                <option value="">Select a melody...</option>
            </select>
            <button class="btn btn-icon" id="playBtn" title="Play/Stop">▶</button>
            <button class="btn btn-icon" id="sourceBtn" title="Go to Source">📄</button>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <span class="toolbar-label">Bars:</span>
            <select id="numBars">
                <option value="1">1</option>
                <option value="2">2</option>
                <option value="4" selected>4</option>
                <option value="8">8</option>
                <option value="16">16</option>
            </select>
        </div>

        <div class="toolbar-group">
            <span class="toolbar-label">Grid:</span>
            <select id="gridSize">
                <option value="1">1 beat</option>
                <option value="0.5">1/2 beat</option>
                <option value="0.25" selected>1/4 beat</option>
                <option value="0.125">1/8 beat</option>
            </select>
        </div>

        <div class="toolbar-group">
            <span class="toolbar-label">Note:</span>
            <select id="noteLength">
                <option value="1">1 beat</option>
                <option value="0.5">1/2 beat</option>
                <option value="0.25" selected>1/4 beat</option>
                <option value="0.125">1/8 beat</option>
            </select>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <button class="btn" id="clearBtn">Clear</button>
            <button class="btn" id="transposeUpBtn">+1 oct</button>
            <button class="btn" id="transposeDownBtn">-1 oct</button>
        </div>
    </div>

    <div class="timeline-header">
        <div class="timeline-header-spacer"></div>
        <div class="timeline-header-content" id="timelineHeader"></div>
    </div>

    <div class="main-content">
        <div class="piano-keyboard" id="pianoKeyboard"></div>
        <div class="piano-roll-container" id="pianoRollContainer">
            <div class="piano-roll" id="pianoRoll">
                <div class="playhead" id="playhead"></div>
            </div>
        </div>

        <div class="empty-state" id="emptyState" style="display: none;">
            <div class="empty-icon">🎹</div>
            <h3>No Melody Selected</h3>
            <p>Select a melody from the dropdown above.</p>
        </div>
    </div>

    <div class="info-bar">
        <div class="info-item">
            <span class="info-label">Notes:</span>
            <span class="info-value" id="noteCount">0</span>
        </div>
        <div class="info-item">
            <span class="info-label">Range:</span>
            <span class="info-value" id="noteRange">-</span>
        </div>
        <div class="info-item">
            <span class="info-label">Duration:</span>
            <span class="info-value" id="duration">16 beats</span>
        </div>
    </div>

    <script>
        const vscode = acquireVsCodeApi();

        // Configuration
        const NOTE_HEIGHT = 16;
        const PIXELS_PER_BEAT = 60;
        const MIN_MIDI = 24;  // C1
        const MAX_MIDI = 96;  // C7
        const TOTAL_KEYS = MAX_MIDI - MIN_MIDI + 1;

        // State
        let state = {
            melodies: [],
            currentMelody: null,
            grid: null,
            transport: { current_beat: 0, bpm: 120, running: false },
            selectedNote: null,
            isDragging: false,
            dragMode: null, // 'move', 'resize', 'create'
            dragData: null,
        };

        // Elements
        const melodySelect = document.getElementById('melodySelect');
        const numBarsSelect = document.getElementById('numBars');
        const gridSizeSelect = document.getElementById('gridSize');
        const noteLengthSelect = document.getElementById('noteLength');
        const pianoKeyboard = document.getElementById('pianoKeyboard');
        const pianoRoll = document.getElementById('pianoRoll');
        const pianoRollContainer = document.getElementById('pianoRollContainer');
        const timelineHeader = document.getElementById('timelineHeader');
        const playhead = document.getElementById('playhead');
        const emptyState = document.getElementById('emptyState');

        // Helper functions
        function isBlackKey(midi) {
            const note = midi % 12;
            return [1, 3, 6, 8, 10].includes(note);
        }

        function midiToNoteName(midi) {
            const notes = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
            const octave = Math.floor(midi / 12) - 1;
            return notes[midi % 12] + octave;
        }

        function midiToY(midi) {
            return (MAX_MIDI - midi) * NOTE_HEIGHT;
        }

        function yToMidi(y) {
            return MAX_MIDI - Math.floor(y / NOTE_HEIGHT);
        }

        function beatToX(beat) {
            return beat * PIXELS_PER_BEAT;
        }

        function xToBeat(x) {
            const gridSize = parseFloat(gridSizeSelect.value);
            const beat = x / PIXELS_PER_BEAT;
            return Math.round(beat / gridSize) * gridSize;
        }

        // Initialize
        function init() {
            renderPianoKeyboard();
            setupEventListeners();
        }

        function renderPianoKeyboard() {
            pianoKeyboard.innerHTML = '';
            for (let midi = MAX_MIDI; midi >= MIN_MIDI; midi--) {
                const key = document.createElement('div');
                key.className = 'piano-key ' + (isBlackKey(midi) ? 'black' : 'white');
                if (midi % 12 === 0) key.classList.add('c-note');

                const noteName = midiToNoteName(midi);
                // Only show C notes
                if (midi % 12 === 0) {
                    key.textContent = noteName;
                }

                key.dataset.midi = midi;
                key.addEventListener('click', () => playNote(midi));
                pianoKeyboard.appendChild(key);
            }
        }

        function playNote(midi, duration = 0.25) {
            // Send note preview to runtime
            vscode.postMessage({
                command: 'previewNote',
                midiNote: midi,
                velocity: 100,
                duration: duration,
            });
        }

        function renderPianoRoll() {
            if (!state.grid) return;

            const numBars = parseInt(numBarsSelect.value);
            const beatsPerBar = state.grid.beatsPerBar || 4;
            const totalBeats = numBars * beatsPerBar;
            const width = totalBeats * PIXELS_PER_BEAT;
            const height = TOTAL_KEYS * NOTE_HEIGHT;

            pianoRoll.style.width = width + 'px';
            pianoRoll.style.height = height + 'px';

            // Clear existing content (except playhead)
            const existingNotes = pianoRoll.querySelectorAll('.note, .grid-row, .grid-line');
            existingNotes.forEach(el => el.remove());

            // Render grid rows
            for (let midi = MAX_MIDI; midi >= MIN_MIDI; midi--) {
                const row = document.createElement('div');
                row.className = 'grid-row ' + (isBlackKey(midi) ? 'black' : 'white');
                row.style.top = midiToY(midi) + 'px';
                row.style.height = NOTE_HEIGHT + 'px';
                row.style.width = width + 'px';
                row.style.position = 'absolute';
                row.dataset.midi = midi;
                pianoRoll.appendChild(row);
            }

            // Render grid lines
            for (let beat = 0; beat <= totalBeats; beat++) {
                const line = document.createElement('div');
                line.className = 'grid-line ' + (beat % beatsPerBar === 0 ? 'bar' : 'beat');
                line.style.left = beatToX(beat) + 'px';
                pianoRoll.appendChild(line);
            }

            // Render timeline header
            renderTimelineHeader(totalBeats, beatsPerBar);

            // Render notes
            renderNotes();
        }

        function renderTimelineHeader(totalBeats, beatsPerBar) {
            timelineHeader.innerHTML = '';
            timelineHeader.style.width = (totalBeats * PIXELS_PER_BEAT) + 'px';

            for (let bar = 0; bar < totalBeats / beatsPerBar; bar++) {
                const marker = document.createElement('div');
                marker.className = 'bar-marker';
                marker.style.left = (bar * beatsPerBar * PIXELS_PER_BEAT) + 'px';
                marker.textContent = (bar + 1).toString();
                timelineHeader.appendChild(marker);
            }
        }

        function renderNotes() {
            if (!state.grid) return;

            // Remove existing note elements
            pianoRoll.querySelectorAll('.note').forEach(el => el.remove());

            for (let i = 0; i < state.grid.notes.length; i++) {
                const note = state.grid.notes[i];
                const noteEl = document.createElement('div');
                noteEl.className = 'note';
                noteEl.style.left = beatToX(note.startBeat) + 'px';
                noteEl.style.top = (midiToY(note.midiNote) + 1) + 'px';
                noteEl.style.width = (note.duration * PIXELS_PER_BEAT - 2) + 'px';
                noteEl.dataset.index = i;
                noteEl.textContent = midiToNoteName(note.midiNote);

                if (state.selectedNote === i) {
                    noteEl.classList.add('selected');
                }

                // Resize handle
                const handle = document.createElement('div');
                handle.className = 'note-resize-handle';
                noteEl.appendChild(handle);

                // Event listeners
                noteEl.addEventListener('mousedown', (e) => startNoteDrag(e, i));

                pianoRoll.appendChild(noteEl);
            }
        }

        function startNoteDrag(e, noteIndex) {
            e.stopPropagation();
            state.selectedNote = noteIndex;
            renderNotes();

            const note = state.grid.notes[noteIndex];
            const isResize = e.target.classList.contains('note-resize-handle');

            state.isDragging = true;
            state.dragMode = isResize ? 'resize' : 'move';
            state.dragData = {
                noteIndex,
                startX: e.clientX,
                startY: e.clientY,
                originalBeat: note.startBeat,
                originalDuration: note.duration,
                originalMidi: note.midiNote,
            };

            document.addEventListener('mousemove', onDrag);
            document.addEventListener('mouseup', endDrag);
        }

        function onDrag(e) {
            if (!state.isDragging || !state.dragData) return;

            const dx = e.clientX - state.dragData.startX;
            const dy = e.clientY - state.dragData.startY;
            const gridSize = parseFloat(gridSizeSelect.value);

            const note = state.grid.notes[state.dragData.noteIndex];

            if (state.dragMode === 'resize') {
                const deltaBeat = Math.round((dx / PIXELS_PER_BEAT) / gridSize) * gridSize;
                note.duration = Math.max(gridSize, state.dragData.originalDuration + deltaBeat);
            } else {
                const deltaBeat = Math.round((dx / PIXELS_PER_BEAT) / gridSize) * gridSize;
                const deltaMidi = -Math.round(dy / NOTE_HEIGHT);

                note.startBeat = Math.max(0, state.dragData.originalBeat + deltaBeat);
                note.midiNote = Math.min(MAX_MIDI, Math.max(MIN_MIDI, state.dragData.originalMidi + deltaMidi));
            }

            renderNotes();
        }

        function endDrag() {
            if (state.isDragging) {
                state.isDragging = false;
                state.dragMode = null;
                state.dragData = null;
                updateInfo();
                sendGridUpdate();
            }
            document.removeEventListener('mousemove', onDrag);
            document.removeEventListener('mouseup', endDrag);
        }

        function setupEventListeners() {
            melodySelect.addEventListener('change', (e) => {
                if (e.target.value) {
                    vscode.postMessage({ command: 'loadMelody', melodyName: e.target.value });
                }
            });

            numBarsSelect.addEventListener('change', () => {
                if (state.grid) {
                    state.grid.numBars = parseInt(numBarsSelect.value);
                    state.grid.totalBeats = state.grid.numBars * state.grid.beatsPerBar;
                    renderPianoRoll();
                    updateInfo();
                }
            });

            document.getElementById('playBtn').addEventListener('click', () => {
                vscode.postMessage({ command: 'togglePlayback' });
            });

            document.getElementById('sourceBtn').addEventListener('click', () => {
                vscode.postMessage({ command: 'goToSource' });
            });

            document.getElementById('clearBtn').addEventListener('click', () => {
                if (state.grid) {
                    state.grid.notes = [];
                    renderNotes();
                    updateInfo();
                    sendGridUpdate();
                }
            });

            document.getElementById('transposeUpBtn').addEventListener('click', () => {
                transpose(12);
            });

            document.getElementById('transposeDownBtn').addEventListener('click', () => {
                transpose(-12);
            });

            // Click on piano roll to add note
            pianoRoll.addEventListener('mousedown', (e) => {
                if (e.target === pianoRoll || e.target.classList.contains('grid-row')) {
                    const rect = pianoRoll.getBoundingClientRect();
                    const x = e.clientX - rect.left + pianoRollContainer.scrollLeft;
                    const y = e.clientY - rect.top + pianoRollContainer.scrollTop;

                    const beat = xToBeat(x);
                    const midi = yToMidi(y);
                    const noteLength = parseFloat(noteLengthSelect.value);

                    if (state.grid && midi >= MIN_MIDI && midi <= MAX_MIDI) {
                        state.grid.notes.push({
                            startBeat: beat,
                            duration: noteLength,
                            midiNote: midi,
                            velocity: 1.0,
                        });
                        state.selectedNote = state.grid.notes.length - 1;
                        playNote(midi, noteLength); // Preview the note
                        renderNotes();
                        updateInfo();
                        sendGridUpdate();
                    }
                }
            });

            // Double-click to delete note
            pianoRoll.addEventListener('dblclick', (e) => {
                if (e.target.classList.contains('note')) {
                    const index = parseInt(e.target.dataset.index);
                    state.grid.notes.splice(index, 1);
                    state.selectedNote = null;
                    renderNotes();
                    updateInfo();
                    sendGridUpdate();
                }
            });

            // Keyboard shortcuts
            document.addEventListener('keydown', (e) => {
                if (e.key === 'Delete' || e.key === 'Backspace') {
                    if (state.selectedNote !== null && state.grid) {
                        state.grid.notes.splice(state.selectedNote, 1);
                        state.selectedNote = null;
                        renderNotes();
                        updateInfo();
                        sendGridUpdate();
                    }
                }
            });

            // Handle messages from extension
            window.addEventListener('message', (event) => {
                const message = event.data;
                switch (message.type) {
                    case 'melodyList':
                        updateMelodyList(message.data);
                        break;
                    case 'melodyUpdate':
                        updateMelody(message.data);
                        break;
                    case 'transportUpdate':
                        updateTransport(message.data);
                        break;
                }
            });

            // Playhead animation
            requestAnimationFrame(animatePlayhead);

            // Sync scroll between keyboard and roll
            pianoRollContainer.addEventListener('scroll', () => {
                pianoKeyboard.scrollTop = pianoRollContainer.scrollTop;
            });
        }

        function updateMelodyList(melodies) {
            state.melodies = melodies;

            melodySelect.innerHTML = '<option value="">Select a melody...</option>';
            for (const m of melodies) {
                const option = document.createElement('option');
                option.value = m.name;
                option.textContent = m.name + (m.voiceName ? ' → ' + m.voiceName : '') + (m.isPlaying ? ' ▶' : '');
                melodySelect.appendChild(option);
            }

            if (state.currentMelody) {
                melodySelect.value = state.currentMelody.melodyName;
            }
        }

        function updateMelody(data) {
            state.currentMelody = data;
            state.grid = data.grid;

            // Update UI
            if (state.grid) {
                numBarsSelect.value = state.grid.numBars;
            }

            emptyState.style.display = 'none';
            document.querySelector('.main-content').style.display = 'flex';

            renderPianoRoll();
            updateInfo();

            // Update play button
            const melody = state.melodies.find(m => m.name === data.melodyName);
            document.getElementById('playBtn').textContent = melody?.isPlaying ? '⏸' : '▶';
        }

        function updateTransport(transport) {
            state.transport = transport;
        }

        function updateInfo() {
            if (!state.grid) return;

            const notes = state.grid.notes;
            document.getElementById('noteCount').textContent = notes.length;
            document.getElementById('duration').textContent = state.grid.totalBeats + ' beats';

            if (notes.length > 0) {
                const midiNotes = notes.map(n => n.midiNote);
                const lowest = Math.min(...midiNotes);
                const highest = Math.max(...midiNotes);
                document.getElementById('noteRange').textContent =
                    midiToNoteName(lowest) + ' - ' + midiToNoteName(highest);
            } else {
                document.getElementById('noteRange').textContent = '-';
            }
        }

        function transpose(semitones) {
            if (!state.grid) return;
            state.grid.notes = state.grid.notes.map(n => ({
                ...n,
                midiNote: Math.min(MAX_MIDI, Math.max(MIN_MIDI, n.midiNote + semitones)),
            }));
            renderNotes();
            updateInfo();
            sendGridUpdate();
        }

        function sendGridUpdate() {
            if (!state.grid || !state.currentMelody) return;
            const stepsPerBar = Math.round(state.grid.beatsPerBar / parseFloat(gridSizeSelect.value));
            vscode.postMessage({
                command: 'updateGrid',
                grid: state.grid,
                stepsPerBar,
            });
        }

        function animatePlayhead() {
            if (state.transport.running && state.grid) {
                const totalBeats = state.grid.totalBeats;
                const loopBeat = state.transport.current_beat % totalBeats;
                playhead.style.left = beatToX(loopBeat) + 'px';
                playhead.classList.add('visible');
            } else {
                playhead.classList.remove('visible');
            }
            requestAnimationFrame(animatePlayhead);
        }

        // Start
        init();
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
    <title>Melody Editor</title>
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
        h2 { font-size: 18px; font-weight: 500; margin-bottom: 8px; color: #d4d4d4; }
    </style>
</head>
<body>
    <div class="empty-state">
        <div class="empty-icon">🎹</div>
        <h2>Not Connected</h2>
        <p>Connect to a VibeLang runtime to use the Melody Editor.</p>
    </div>
</body>
</html>`;
    }

    dispose() {
        MelodyEditor.currentPanel = undefined;
        this._panel.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
