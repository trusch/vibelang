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
 * - Numpad recording for notes
 * - Live API updates (no file save required)
 * - Write-to-file button for explicit save
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
    generateMultiLaneMelodyStrings,
    parseMultiLaneMelodyStrings,
    countLanes,
    parseNoteToMidi,
} from '../utils/melodyParser';
import {
    getEditorBaseStyles,
    getRecordingPanelStyles,
    getCodePanelStyles,
    getTimingControlStyles,
    getTransportUtilsScript,
    getNumpadUtilsScript,
    getRecordButtonScript,
    getToastScript,
    renderKeyAssignmentPanel,
    renderCodePanel,
    renderTimingSlider,
    renderRecordButton,
    renderToastContainer,
} from './sharedComponents';

interface MelodyEditorState {
    melodyName: string;
    voiceName: string | null;
    grid: MelodyGrid;
    sourceLocation?: { file?: string; line?: number; column?: number };
    originalMelodyString?: string;
    locallyModified?: boolean;
    /** True if the melody is generated dynamically (no static .notes() in source) */
    isDynamic?: boolean;
}

export class MelodyEditor {
    public static currentPanel: MelodyEditor | undefined;
    public static readonly viewType = 'vibelang.melodyEditor';

    private readonly _panel: vscode.WebviewPanel;
    private readonly _store: StateStore;
    private _disposables: vscode.Disposable[] = [];
    private _currentMelody: MelodyEditorState | null = null;
    private _webviewReady = false;
    private _pendingMelodyName: string | null = null;

    // Recording feature: note key assignments
    private _noteKeyAssignments: Map<number, { midiNote: number; noteName: string }> = new Map();
    private _documentRefreshTimeout: NodeJS.Timeout | undefined;

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

        // Listen for document text changes
        this._disposables.push(
            vscode.workspace.onDidChangeTextDocument((e) => {
                if (e.document.fileName.endsWith('.vibe') && e.contentChanges.length > 0) {
                    this._scheduleDocumentRefresh();
                }
            })
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
            MelodyEditor.currentPanel._pendingMelodyName = melodyName;
        }
    }

    public static revive(panel: vscode.WebviewPanel, store: StateStore) {
        MelodyEditor.currentPanel = new MelodyEditor(panel, store);
    }

    public async loadMelody(melodyName: string) {
        const state = this._store.state;
        if (!state) return;

        const melody = state.melodies.find(m => m.name === melodyName);
        if (!melody) {
            vscode.window.showErrorMessage(`Melody "${melodyName}" not found`);
            return;
        }

        // Parse the melody from notes_patterns (supports polyphonic lanes) or events
        let grid: MelodyGrid;
        if (melody.notes_patterns && melody.notes_patterns.length > 0) {
            // Try to parse notes_patterns first
            grid = parseMultiLaneMelodyStrings(melody.notes_patterns);

            // If parsing produced no notes (e.g., scale degrees without context),
            // fall back to events which have the resolved MIDI notes
            if (grid.notes.length === 0 && melody.events && melody.events.length > 0) {
                grid = this._eventsToGrid(melody.events, melody.loop_beats);
            }
        } else {
            // Fallback to events
            grid = this._eventsToGrid(melody.events, melody.loop_beats);
        }

        // Detect if the melody is dynamic (generated via code, not static .notes() calls)
        const isDynamic = await this._checkIfDynamicMelody(melody.source_location);

        this._currentMelody = {
            melodyName: melody.name,
            voiceName: melody.voice_name || null,
            grid,
            sourceLocation: melody.source_location,
            originalMelodyString: melody.notes_patterns?.join(' | ') || '',
            locallyModified: false,
            isDynamic,
        };

        // Auto-assign default notes to numpad
        this._autoAssignNotes();

        this._sendMelodyUpdate();
    }

    /**
     * Check if a melody is dynamically generated (no static .notes() in source).
     * Returns true if the melody cannot be written back to the source file.
     */
    private async _checkIfDynamicMelody(sourceLocation?: { file?: string; line?: number }): Promise<boolean> {
        if (!sourceLocation?.file || !sourceLocation?.line) {
            return true; // No source location = dynamic
        }

        try {
            const uri = vscode.Uri.file(sourceLocation.file);
            let document = vscode.workspace.textDocuments.find(d => d.uri.fsPath === uri.fsPath);
            if (!document) {
                document = await vscode.workspace.openTextDocument(uri);
            }

            const startLineIndex = sourceLocation.line - 1;
            if (startLineIndex < 0 || startLineIndex >= document.lineCount) {
                return true;
            }

            // Search for .notes("...") call near the melody definition
            const maxSearchLines = 20;
            const endLineIndex = Math.min(startLineIndex + maxSearchLines, document.lineCount);
            const notesRegex = /\.notes\s*\(\s*"[^"]*"\s*\)/;

            for (let lineIdx = startLineIndex; lineIdx < endLineIndex; lineIdx++) {
                const lineText = document.lineAt(lineIdx).text;

                if (notesRegex.test(lineText)) {
                    return false; // Found .notes() - not dynamic
                }

                // Stop if we hit another melody/group definition
                if (lineIdx > startLineIndex && /\b(melody|define_group)\s*\(/.test(lineText)) {
                    break;
                }
            }

            return true; // No .notes() found = dynamic
        } catch {
            return true; // Error reading file = treat as dynamic
        }
    }

    private _eventsToGrid(events: Array<{ beat: number; note?: string; frequency?: number; duration?: number }>, loopBeats: number): MelodyGrid {
        const notes: MelodyNote[] = [];
        const beatsPerBar = 4;
        const numBars = Math.ceil(loopBeats / beatsPerBar);

        for (const event of events) {
            // Convert to MIDI note
            let midiNote = 60; // Default to C4
            if (event.note) {
                // Parse note name like "C4", "D#5", "Bb3" to MIDI
                const parsed = parseNoteToMidi(event.note);
                if (parsed !== null) {
                    midiNote = parsed;
                }
            } else if (event.frequency && event.frequency > 0) {
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

    /**
     * Auto-assign notes to numpad keys.
     * Keys 1-9 map to C4, D4, E4, F4, G4, A4, B4, C5, D5
     */
    private _autoAssignNotes() {
        this._noteKeyAssignments.clear();
        const baseNotes = [60, 62, 64, 65, 67, 69, 71, 72, 74]; // C4 through D5 (white keys)

        baseNotes.forEach((midiNote, index) => {
            this._noteKeyAssignments.set(index, {
                midiNote,
                noteName: midiToNoteName(midiNote),
            });
        });

        this._sendNoteAssignmentsUpdate();
    }

    /**
     * Auto-assign notes from a scale (provided by webview).
     */
    private _autoAssignNotesFromScale(notes: number[]) {
        this._noteKeyAssignments.clear();

        notes.forEach((midiNote, index) => {
            if (index < 9) { // Only assign to 9 numpad keys
                this._noteKeyAssignments.set(index, {
                    midiNote,
                    noteName: midiToNoteName(midiNote),
                });
            }
        });

        this._sendNoteAssignmentsUpdate();
    }

    private _sendNoteAssignmentsUpdate() {
        const assignments: Array<{ keyIndex: number; midiNote: number; noteName: string }> = [];
        for (const [keyIndex, assignment] of this._noteKeyAssignments) {
            assignments.push({
                keyIndex,
                midiNote: assignment.midiNote,
                noteName: assignment.noteName,
            });
        }

        this._panel.webview.postMessage({
            type: 'noteAssignmentsUpdate',
            data: { assignments },
        });
    }

    private _updateContent() {
        this._panel.webview.html = this._getHtmlContent();
    }

    private _scheduleDocumentRefresh() {
        if (this._documentRefreshTimeout) {
            clearTimeout(this._documentRefreshTimeout);
        }

        this._documentRefreshTimeout = setTimeout(() => {
            this._refreshMelodyFromDocument();
        }, 300);
    }

    private async _refreshMelodyFromDocument() {
        if (!this._currentMelody) return;
        if (this._currentMelody.locallyModified) return; // Don't overwrite local changes

        if (!this._currentMelody.sourceLocation?.file || !this._currentMelody.sourceLocation?.line) return;

        try {
            const uri = vscode.Uri.file(this._currentMelody.sourceLocation.file);
            const document = vscode.workspace.textDocuments.find(d => d.uri.fsPath === uri.fsPath);
            if (!document) return;

            const startLineIndex = this._currentMelody.sourceLocation.line - 1;
            if (startLineIndex < 0 || startLineIndex >= document.lineCount) return;

            // Search for .notes("...") call
            const maxSearchLines = 20;
            const endLineIndex = Math.min(startLineIndex + maxSearchLines, document.lineCount);
            const notesRegex = /\.notes\s*\(\s*"([^"]*)"\s*\)/;

            for (let lineIdx = startLineIndex; lineIdx < endLineIndex; lineIdx++) {
                const lineText = document.lineAt(lineIdx).text;
                const match = lineText.match(notesRegex);

                if (match) {
                    const melodyString = match[1];
                    const currentMelodyString = generateMelodyString(this._currentMelody.grid, 4);

                    if (melodyString !== currentMelodyString) {
                        // Melody changed in code - update grid
                        this._currentMelody.grid = parseMelodyString(melodyString);
                        this._sendMelodyUpdate();
                    }
                    break;
                }

                if (lineIdx > startLineIndex && /\b(melody|define_group)\s*\(/.test(lineText)) {
                    break;
                }
            }
        } catch (error) {
            console.error('Failed to refresh melody from document:', error);
        }
    }

    private _refreshMelodyList() {
        const state = this._store.state;
        if (!state) {
            // Send loading state when not yet connected
            this._panel.webview.postMessage({
                type: 'connectionStatus',
                data: { status: this._store.status },
            });
            return;
        }

        // Group melodies by their group path for organized display
        const melodies = state.melodies.map(m => ({
            name: m.name,
            voiceName: m.voice_name,
            groupPath: m.group_path,
            loopBeats: m.loop_beats,
            noteCount: m.events?.length || 0,
            isPlaying: m.is_looping || m.status?.state === 'playing',
        }));

        // Send connection status along with melody list
        this._panel.webview.postMessage({
            type: 'connectionStatus',
            data: { status: 'connected' },
        });

        this._panel.webview.postMessage({
            type: 'melodyList',
            data: melodies,
        });

        // If current melody exists, update its playing state
        if (this._currentMelody) {
            const melody = state.melodies.find(m => m.name === this._currentMelody?.melodyName);
            if (melody) {
                this._panel.webview.postMessage({
                    type: 'playingStateUpdate',
                    data: { isPlaying: melody.is_looping || melody.status?.state === 'playing' },
                });
            }
        }
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
                locallyModified: this._currentMelody.locallyModified,
                isDynamic: this._currentMelody.isDynamic,
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
            case 'ready':
                this._webviewReady = true;
                if (this._pendingMelodyName) {
                    this.loadMelody(this._pendingMelodyName);
                    this._pendingMelodyName = null;
                } else {
                    this._refreshMelodyList();
                }
                break;

            case 'loadMelody':
                this.loadMelody(message.melodyName as string);
                break;

            case 'updateGrid':
                if (this._currentMelody) {
                    this._currentMelody.grid = message.grid as MelodyGrid;
                    this._currentMelody.locallyModified = true;
                    // Update via API (live update without file save)
                    await this._updateMelodyViaApi();
                }
                break;

            case 'addNote': {
                if (this._currentMelody) {
                    const note: MelodyNote = {
                        startBeat: message.startBeat as number,
                        duration: message.duration as number,
                        midiNote: message.midiNote as number,
                        velocity: (message.velocity as number) || 1.0,
                    };
                    this._currentMelody.grid.notes.push(note);
                    this._currentMelody.locallyModified = true;
                    await this._updateMelodyViaApi();
                    this._sendMelodyUpdate();
                }
                break;
            }

            case 'updateNote': {
                if (this._currentMelody) {
                    const noteIndex = message.noteIndex as number;
                    const updates = message.updates as Partial<MelodyNote>;
                    if (this._currentMelody.grid.notes[noteIndex]) {
                        Object.assign(this._currentMelody.grid.notes[noteIndex], updates);
                        this._currentMelody.locallyModified = true;
                        await this._updateMelodyViaApi();
                    }
                }
                break;
            }

            case 'deleteNote': {
                if (this._currentMelody) {
                    const noteIndex = message.noteIndex as number;
                    this._currentMelody.grid.notes.splice(noteIndex, 1);
                    this._currentMelody.locallyModified = true;
                    await this._updateMelodyViaApi();
                    this._sendMelodyUpdate();
                }
                break;
            }

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
                    this._currentMelody.locallyModified = true;
                    await this._updateMelodyViaApi();
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
                    const duration = (message.duration as number) || 0.25;

                    // Get the voice's current amp to scale velocity appropriately
                    const voice = this._store.state?.voices.find(v => v.name === this._currentMelody?.voiceName);
                    const voiceAmp = voice?.params?.['amp'] ?? voice?.gain ?? 0.5;
                    // Scale velocity (0-127) by the voice's amp (0-1), with a reasonable default
                    const velocity = Math.round(Math.min(127, Math.max(1, 100 * voiceAmp)));

                    try {
                        await this._store.runtime.noteOn(
                            this._currentMelody.voiceName,
                            midiNote,
                            velocity
                        );
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

            case 'recordNote': {
                // Record a note at the current beat position
                if (this._currentMelody) {
                    const midiNote = message.midiNote as number;
                    const beat = message.beat as number;
                    const noteLength = (message.noteLength as number) || 0.25;

                    // Quantize to grid
                    const totalBeats = this._currentMelody.grid.totalBeats;
                    const loopBeat = ((beat % totalBeats) + totalBeats) % totalBeats;

                    // Add note
                    this._currentMelody.grid.notes.push({
                        startBeat: loopBeat,
                        duration: noteLength,
                        midiNote,
                        velocity: 1.0,
                    });
                    this._currentMelody.locallyModified = true;
                    await this._updateMelodyViaApi();
                    this._sendMelodyUpdate();
                }
                break;
            }

            case 'triggerAndRecord': {
                const midiNote = message.midiNote as number;

                // Trigger the voice for audio feedback (one-shot mode)
                if (this._currentMelody?.voiceName) {
                    // Get the voice's current amp to scale velocity appropriately
                    const voice = this._store.state?.voices.find(v => v.name === this._currentMelody?.voiceName);
                    const voiceAmp = voice?.params?.['amp'] ?? voice?.gain ?? 0.5;
                    const velocity = Math.round(Math.min(127, Math.max(1, 100 * voiceAmp)));

                    try {
                        await this._store.runtime.noteOn(
                            this._currentMelody.voiceName,
                            midiNote,
                            velocity
                        );
                        setTimeout(async () => {
                            if (this._currentMelody?.voiceName) {
                                await this._store.runtime.noteOff(
                                    this._currentMelody.voiceName,
                                    midiNote
                                );
                            }
                        }, 250);
                    } catch (err) {
                        console.error('Failed to trigger note:', err);
                    }
                }
                break;
            }

            case 'triggerNoteOn': {
                const midiNote = message.midiNote as number;

                // Trigger note-on for held note
                if (this._currentMelody?.voiceName) {
                    // Get the voice's current amp to scale velocity appropriately
                    const voice = this._store.state?.voices.find(v => v.name === this._currentMelody?.voiceName);
                    const voiceAmp = voice?.params?.['amp'] ?? voice?.gain ?? 0.5;
                    const velocity = Math.round(Math.min(127, Math.max(1, 100 * voiceAmp)));

                    try {
                        await this._store.runtime.noteOn(
                            this._currentMelody.voiceName,
                            midiNote,
                            velocity
                        );
                    } catch (err) {
                        console.error('Failed to trigger note-on:', err);
                    }
                }
                break;
            }

            case 'triggerNoteOff': {
                const midiNote = message.midiNote as number;

                // Trigger note-off when key is released
                if (this._currentMelody?.voiceName) {
                    try {
                        await this._store.runtime.noteOff(
                            this._currentMelody.voiceName,
                            midiNote
                        );
                    } catch (err) {
                        console.error('Failed to trigger note-off:', err);
                    }
                }
                break;
            }

            case 'assignNoteToKey': {
                const keyIndex = message.keyIndex as number;
                const midiNote = message.midiNote as number;
                this._noteKeyAssignments.set(keyIndex, {
                    midiNote,
                    noteName: midiToNoteName(midiNote),
                });
                this._sendNoteAssignmentsUpdate();
                break;
            }

            case 'autoAssignNotes':
                if (message.notes && Array.isArray(message.notes)) {
                    this._autoAssignNotesFromScale(message.notes as number[]);
                } else {
                    this._autoAssignNotes();
                }
                break;

            case 'copyToClipboard': {
                const text = message.text as string;
                await vscode.env.clipboard.writeText(text);
                vscode.window.showInformationMessage('Melody code copied to clipboard');
                break;
            }

            case 'writeBackToFile':
                await this._writeBackToFile();
                break;

            case 'clearMelody':
                if (this._currentMelody) {
                    this._currentMelody.grid.notes = [];
                    this._currentMelody.locallyModified = true;
                    await this._updateMelodyViaApi();
                    this._sendMelodyUpdate();
                }
                break;

            case 'transpose': {
                const semitones = message.semitones as number;
                if (this._currentMelody) {
                    this._currentMelody.grid.notes = this._currentMelody.grid.notes.map(n => ({
                        ...n,
                        midiNote: Math.min(96, Math.max(24, n.midiNote + semitones)),
                    }));
                    this._currentMelody.locallyModified = true;
                    await this._updateMelodyViaApi();
                    this._sendMelodyUpdate();
                }
                break;
            }
        }
    }

    /**
     * Update melody via HTTP API for live playback changes (no file save).
     * Uses lanes for polyphonic support - notes are auto-split into minimal lanes.
     */
    private async _updateMelodyViaApi() {
        if (!this._currentMelody) return;

        // Calculate optimal steps per bar based on note positions
        const stepsPerBar = this._calculateOptimalStepsPerBar(this._currentMelody.grid);
        // Generate multiple lane strings for polyphonic support
        const lanes = generateMultiLaneMelodyStrings(this._currentMelody.grid, stepsPerBar);

        try {
            await this._store.runtime.updateMelody(this._currentMelody.melodyName, {
                lanes: lanes,
                loop_beats: this._currentMelody.grid.totalBeats,
            });
        } catch (error) {
            console.error('Failed to update melody via API:', error);
        }
    }

    /**
     * Calculate the optimal steps per bar based on note positions.
     * Ensures fine-grained notes (1/16, 1/32) are preserved accurately.
     */
    private _calculateOptimalStepsPerBar(grid: MelodyGrid): number {
        const beatsPerBar = grid.beatsPerBar || 4;
        let smallestDivision = 1; // Start with whole beats

        for (const note of grid.notes) {
            // Check what the smallest beat fraction is
            const startFrac = note.startBeat % 1;
            const durFrac = note.duration % 1;
            if (startFrac !== 0 || durFrac !== 0) {
                // Find the finest division needed
                for (const div of [0.5, 0.25, 0.125, 0.0625, 0.03125]) {
                    if (Math.abs(note.startBeat % div) < 0.001 && Math.abs(note.duration % div) < 0.001) {
                        if (div < smallestDivision) smallestDivision = div;
                        break;
                    }
                    if (div < smallestDivision) smallestDivision = div;
                }
            }
        }

        // Use at least 4 steps per bar, but more if notes require finer resolution
        return Math.max(4, Math.round(beatsPerBar / smallestDivision));
    }

    /**
     * Write the melody back to the source file.
     * Supports polyphonic melodies by generating multiple .notes() calls if needed.
     */
    private async _writeBackToFile() {
        if (!this._currentMelody?.sourceLocation?.file || !this._currentMelody?.sourceLocation?.line) {
            vscode.window.showWarningMessage('No source location found for this melody');
            return;
        }

        // Calculate optimal steps per bar based on note positions
        const stepsPerBar = this._calculateOptimalStepsPerBar(this._currentMelody.grid);
        // Generate multiple lane strings for polyphonic support
        const lanes = generateMultiLaneMelodyStrings(this._currentMelody.grid, stepsPerBar);
        const laneCount = lanes.length;

        // Generate the .notes() calls string
        let notesCallsStr: string;
        if (laneCount <= 1) {
            // Single lane - backward compatible format
            notesCallsStr = `.notes("${lanes[0] || ''}")`;
        } else {
            // Multiple lanes - generate chained .notes() calls
            notesCallsStr = lanes.map(lane => `.notes("${lane}")`).join('\n    ');
        }

        try {
            const document = await vscode.workspace.openTextDocument(this._currentMelody.sourceLocation.file);
            const startLineIndex = this._currentMelody.sourceLocation.line - 1;

            // Search for .notes("...") call(s) - may span multiple lines
            const maxSearchLines = 30;
            const endLineIndex = Math.min(startLineIndex + maxSearchLines, document.lineCount);

            // Find the range of all consecutive .notes() calls
            let firstNotesLine = -1;
            let lastNotesLine = -1;
            let firstNotesStart = -1;
            let lastNotesEnd = -1;

            const notesRegex = /\.notes\s*\(\s*"([^"]*)"\s*\)/g;

            for (let lineIdx = startLineIndex; lineIdx < endLineIndex; lineIdx++) {
                const lineText = document.lineAt(lineIdx).text;
                let match;

                while ((match = notesRegex.exec(lineText)) !== null) {
                    if (firstNotesLine === -1) {
                        firstNotesLine = lineIdx;
                        firstNotesStart = match.index;
                    }
                    lastNotesLine = lineIdx;
                    lastNotesEnd = match.index + match[0].length;
                }

                // Stop if we hit another melody/group definition
                if (lineIdx > startLineIndex && /\b(melody|define_group)\s*\(/.test(lineText)) {
                    break;
                }
            }

            if (firstNotesLine !== -1) {
                const edit = new vscode.WorkspaceEdit();
                edit.replace(
                    document.uri,
                    new vscode.Range(firstNotesLine, firstNotesStart, lastNotesLine, lastNotesEnd),
                    notesCallsStr
                );

                await vscode.workspace.applyEdit(edit);
                await document.save();

                this._currentMelody.locallyModified = false;
                this._sendMelodyUpdate();
                const msg = laneCount > 1
                    ? `Melody saved with ${laneCount} polyphonic lanes`
                    : 'Melody saved to file';
                vscode.window.showInformationMessage(msg);
                return;
            }

            vscode.window.showWarningMessage('Could not find .notes() call in source file');
        } catch (error) {
            console.error('Failed to write melody to file:', error);
            vscode.window.showErrorMessage('Failed to save melody to file');
        }
    }

    private _getHtmlContent(): string {
        if (this._store.status !== 'connected') {
            return this._getDisconnectedHtml();
        }

        const styles = `
            ${getEditorBaseStyles()}
            ${getRecordingPanelStyles()}
            ${getCodePanelStyles()}
            ${getTimingControlStyles()}

            /* ========== Loading Indicator ========== */
            .loading-spinner {
                display: inline-block;
                animation: spin 1s linear infinite;
                margin-left: 8px;
            }
            @keyframes spin {
                from { transform: rotate(0deg); }
                to { transform: rotate(360deg); }
            }

            /* ========== Scale Controls Panel ========== */
            .scale-controls-panel {
                margin-top: 4px;
            }
            .scale-controls-container {
                padding: 6px 8px;
            }
            .scale-control-row {
                display: flex;
                gap: 16px;
                align-items: flex-start;
            }
            .scale-control-group {
                display: flex;
                flex-direction: column;
                gap: 4px;
            }
            .control-label {
                font-size: 10px;
                color: var(--text-muted);
                text-transform: uppercase;
                letter-spacing: 0.5px;
            }
            .scale-select {
                background: var(--bg-tertiary);
                border: 1px solid var(--border);
                border-radius: 4px;
                padding: 4px 8px;
                font-size: 12px;
                color: var(--text-primary);
                cursor: pointer;
                min-width: 100px;
            }
            .scale-select:hover {
                border-color: var(--accent-blue);
            }
            .octave-control {
                align-items: center;
            }
            .octave-buttons {
                display: flex;
                align-items: center;
                gap: 4px;
            }
            .octave-display {
                min-width: 28px;
                text-align: center;
                font-size: 14px;
                font-weight: 600;
                padding: 4px 8px;
                background: var(--bg-tertiary);
                border-radius: 4px;
                color: var(--text-secondary);
            }
            .octave-display.positive {
                color: var(--accent-green);
            }
            .octave-display.negative {
                color: var(--accent-orange);
            }
            .scale-hint {
                margin-top: 8px;
                font-size: 10px;
                color: var(--text-muted);
            }
            .scale-hint kbd {
                background: var(--vscode-keybindingLabel-background, var(--bg-primary));
                padding: 1px 4px;
                border-radius: 3px;
                font-family: var(--vscode-editor-font-family, 'SF Mono', Consolas, monospace);
            }

            /* ========== Piano Roll Specific Styles ========== */
            .main-content {
                flex: 1;
                display: flex;
                overflow: hidden;
                background: var(--bg-primary);
            }

            /* Piano Keyboard - Real piano look */
            .piano-keyboard {
                width: 52px;
                background: #1a1a1a;
                border-right: 2px solid #0a0a0a;
                overflow-y: auto;
                flex-shrink: 0;
                scrollbar-width: none;
            }
            .piano-keyboard::-webkit-scrollbar { display: none; }

            .piano-key {
                height: 18px;
                display: flex;
                align-items: center;
                justify-content: flex-end;
                padding-right: 4px;
                font-size: 9px;
                cursor: pointer;
                transition: all 0.05s ease;
                position: relative;
                box-sizing: border-box;
            }

            .piano-key.white {
                background: linear-gradient(90deg, #f0f0f0 0%, #e0e0e0 70%, #d0d0d0 100%);
                color: #666;
                border-bottom: 1px solid #bbb;
                box-shadow: inset -1px 0 0 #fff;
            }

            .piano-key.black {
                background: linear-gradient(90deg, #2a2a2a 0%, #1a1a1a 50%, #0a0a0a 100%);
                color: #444;
                font-size: 8px;
                width: 60%;
                border-radius: 0 2px 2px 0;
                box-shadow: 2px 1px 2px rgba(0,0,0,0.5);
                z-index: 1;
                border-bottom: 1px solid #000;
            }

            .piano-key.white:hover {
                background: linear-gradient(90deg, #d8efff 0%, #c8e8ff 70%, #b8e0ff 100%);
            }

            .piano-key.black:hover {
                background: linear-gradient(90deg, #3a3a3a 0%, #2a2a2a 50%, #1a1a1a 100%);
            }

            .piano-key:active, .piano-key.playing {
                background: var(--accent-green) !important;
                color: #000 !important;
            }

            .piano-key.c-note {
                font-weight: 700;
                color: #333;
            }

            .piano-key.c-note::before {
                content: '';
                position: absolute;
                left: 0;
                top: 0;
                bottom: 0;
                width: 3px;
                background: var(--accent-orange);
            }

            /* Piano Roll Container */
            .piano-roll-container {
                flex: 1;
                overflow: auto;
                position: relative;
                background: var(--bg-primary);
            }

            .piano-roll {
                position: relative;
                min-width: 100%;
            }

            /* Grid Rows */
            .grid-row {
                height: 18px;
                border-bottom: 1px solid var(--beat-line);
                position: relative;
            }

            .grid-row.white {
                background: var(--vscode-editorGutter-background, color-mix(in srgb, var(--bg-tertiary) 30%, transparent));
            }

            .grid-row.black {
                background: var(--bg-primary);
            }

            .grid-row:hover {
                background: var(--accent-blue-dim);
            }

            /* Grid Lines */
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
                width: 1px;
            }

            .grid-line.bar::before {
                content: '';
                position: absolute;
                top: 0;
                left: 0;
                width: 3px;
                height: 100%;
                background: linear-gradient(90deg, var(--bar-line) 0%, transparent 100%);
            }

            /* Notes */
            .note {
                position: absolute;
                height: 16px;
                background: var(--accent-blue);
                border-radius: 3px;
                cursor: pointer;
                border: 1px solid color-mix(in srgb, white 20%, transparent);
                display: flex;
                align-items: center;
                padding-left: 5px;
                font-size: 10px;
                font-weight: 500;
                color: var(--vscode-button-foreground, white);
                text-shadow: 0 1px 2px var(--shadow);
                overflow: hidden;
                white-space: nowrap;
                transition: all var(--transition-fast);
                box-shadow: 0 2px 4px var(--shadow);
            }

            .note:hover {
                filter: brightness(1.15);
                transform: translateY(-1px);
                box-shadow: 0 4px 8px var(--shadow);
            }

            .note.selected {
                background: var(--accent-green);
                border-color: white;
                box-shadow: 0 0 12px var(--accent-green-dim);
            }

            .note.just-recorded {
                animation: note-flash 0.4s ease-out;
            }

            @keyframes note-flash {
                0% {
                    transform: scale(1.2);
                    box-shadow: 0 0 20px var(--accent-green);
                    filter: brightness(1.4);
                }
                100% {
                    transform: scale(1);
                    box-shadow: 0 2px 4px var(--shadow);
                    filter: brightness(1);
                }
            }

            /* Selection rectangle for multi-select */
            .selection-rect {
                position: absolute;
                border: 1px dashed var(--accent-blue);
                background: color-mix(in srgb, var(--accent-blue) 15%, transparent);
                pointer-events: none;
                z-index: 60;
            }

            /* Creating note preview */
            .note-preview {
                position: absolute;
                height: 16px;
                background: color-mix(in srgb, var(--accent-green) 60%, transparent);
                border: 2px dashed var(--accent-green);
                border-radius: 3px;
                pointer-events: none;
                z-index: 55;
            }

            /* Ghost note flash for preview when playing */
            .note-flash {
                position: absolute;
                height: 16px;
                background: var(--accent-orange);
                border-radius: 3px;
                border: 1px solid color-mix(in srgb, white 30%, transparent);
                display: flex;
                align-items: center;
                padding-left: 5px;
                font-size: 10px;
                font-weight: 500;
                color: white;
                overflow: hidden;
                white-space: nowrap;
                pointer-events: none;
                z-index: 50;
                animation: ghost-note-flash 0.4s ease-out forwards;
            }

            @keyframes ghost-note-flash {
                0% {
                    opacity: 1;
                    transform: scale(1.1);
                    box-shadow: 0 0 15px var(--accent-orange);
                }
                100% {
                    opacity: 0;
                    transform: scale(1);
                    box-shadow: none;
                }
            }

            .note-resize-handle {
                position: absolute;
                right: 0;
                top: 0;
                bottom: 0;
                width: 8px;
                cursor: ew-resize;
                background: linear-gradient(90deg, transparent 0%, color-mix(in srgb, white 10%, transparent) 100%);
            }

            .note-resize-handle:hover {
                background: linear-gradient(90deg, transparent 0%, color-mix(in srgb, white 30%, transparent) 100%);
            }

            /* Timeline Header */
            .timeline-header {
                height: 28px;
                background: var(--bg-secondary);
                border-bottom: 1px solid var(--border);
                position: sticky;
                top: 0;
                z-index: 50;
                display: flex;
            }

            .timeline-header-spacer {
                width: 64px;
                flex-shrink: 0;
                border-right: 1px solid var(--border);
                background: var(--bg-secondary);
                display: flex;
                align-items: center;
                justify-content: center;
                font-size: 10px;
                color: var(--text-muted);
            }

            .timeline-header-content {
                flex: 1;
                position: relative;
                display: flex;
                align-items: center;
            }

            .bar-marker {
                position: absolute;
                font-size: 11px;
                font-weight: 600;
                color: var(--text-secondary);
                padding: 2px 6px;
                background: var(--bg-tertiary);
                border-radius: 3px;
                transform: translateX(4px);
            }

            /* Keyboard shortcut hints */
            .shortcut-hints {
                display: flex;
                gap: 12px;
                margin-left: auto;
                padding-right: 0;
                font-size: 10px;
                color: var(--text-muted);
            }

            .shortcut-hint kbd {
                background: var(--vscode-keybindingLabel-background, var(--bg-primary));
                padding: 1px 4px;
                border-radius: 3px;
                font-family: var(--vscode-editor-font-family, 'SF Mono', Consolas, monospace);
                margin-right: 4px;
            }
        `;

        const scripts = `
            ${getTransportUtilsScript()}
            ${getNumpadUtilsScript()}
            ${getRecordButtonScript()}
            ${getToastScript()}

            // Configuration
            const NOTE_HEIGHT = 18;  // Must match CSS .piano-key and .grid-row height
            const MIN_MIDI = 24;  // C1
            const MAX_MIDI = 96;  // C7
            const TOTAL_KEYS = MAX_MIDI - MIN_MIDI + 1;

            // Zoom configuration
            const MIN_ZOOM = 15;   // Minimum pixels per beat
            const MAX_ZOOM = 200;  // Maximum pixels per beat
            const DEFAULT_ZOOM = 60;
            let pixelsPerBeat = DEFAULT_ZOOM;

            // State
            let state = {
                melodies: [],
                currentMelody: null,
                grid: null,
                transport: { current_beat: 0, bpm: 120, running: false },
                selectedNotes: new Set(),  // Multiple selection support
                isDragging: false,
                dragMode: null,  // 'move', 'resize', 'create', 'select-rect'
                dragData: null,
                noteAssignments: new Map(),
                locallyModified: false,
                isDynamic: false,  // True if melody is dynamically generated (no static .notes() in source)
                octaveOffset: 0,  // Shift numpad notes by octaves (+/- keys)
                selectedScale: 'chromatic',  // Current scale for auto-assign
                selectedRoot: 'C',  // Root note for scale
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
                return beat * pixelsPerBeat;
            }

            function xToBeat(x) {
                const gridSize = parseFloat(gridSizeSelect.value);
                const beat = x / pixelsPerBeat;
                return Math.round(beat / gridSize) * gridSize;
            }

            // Zoom functions
            function zoomIn() {
                pixelsPerBeat = Math.min(MAX_ZOOM, pixelsPerBeat * 1.25);
                updateZoomDisplay();
                renderPianoRoll();
            }

            function zoomOut() {
                pixelsPerBeat = Math.max(MIN_ZOOM, pixelsPerBeat / 1.25);
                updateZoomDisplay();
                renderPianoRoll();
            }

            function fitToWindow() {
                if (!state.grid) return;
                const numBars = parseInt(numBarsSelect.value);
                const beatsPerBar = state.grid.beatsPerBar || 4;
                const totalBeats = numBars * beatsPerBar;
                const containerWidth = pianoRollContainer.clientWidth - 20; // Leave some padding
                pixelsPerBeat = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, containerWidth / totalBeats));
                updateZoomDisplay();
                renderPianoRoll();
            }

            function resetZoom() {
                pixelsPerBeat = DEFAULT_ZOOM;
                updateZoomDisplay();
                renderPianoRoll();
            }

            function updateZoomDisplay() {
                const zoomPercent = Math.round((pixelsPerBeat / DEFAULT_ZOOM) * 100);
                const zoomDisplay = document.getElementById('zoomDisplay');
                if (zoomDisplay) {
                    zoomDisplay.textContent = zoomPercent + '%';
                }
            }

            // Initialize
            function init() {
                renderPianoKeyboard();
                setupEventListeners();
                setupRecordButton();
                setupRecordingListeners();
                setupNotePanelEvents();
                setupCodePanelEvents();
                vscode.postMessage({ command: 'ready' });
                requestAnimationFrame(animatePlayhead);
            }

            function renderPianoKeyboard() {
                pianoKeyboard.innerHTML = '';
                for (let midi = MAX_MIDI; midi >= MIN_MIDI; midi--) {
                    const key = document.createElement('div');
                    key.className = 'piano-key ' + (isBlackKey(midi) ? 'black' : 'white');
                    if (midi % 12 === 0) key.classList.add('c-note');

                    const noteName = midiToNoteName(midi);
                    if (midi % 12 === 0) {
                        key.textContent = noteName;
                    }

                    key.dataset.midi = midi;
                    key.addEventListener('click', () => playNote(midi));
                    pianoKeyboard.appendChild(key);
                }
            }

            function playNote(midi, duration = 0.25) {
                vscode.postMessage({
                    command: 'previewNote',
                    midiNote: midi,
                    velocity: 100,
                    duration: duration,
                });
                // Show visual feedback on the piano roll
                showNoteFlash(midi, duration);
            }

            // Show a temporary ghost note on the piano roll when a note is played
            function showNoteFlash(midi, duration = 0.25) {
                if (!state.grid || midi < MIN_MIDI || midi > MAX_MIDI) return;

                // Get current beat position from transport
                const currentBeat = transportState.running ? getInterpolatedBeat(timingOffsetMs) : 0;
                const gridSize = parseFloat(gridSizeSelect.value);
                const quantizedBeat = Math.round(currentBeat / gridSize) * gridSize;

                // Create a ghost note element
                const ghost = document.createElement('div');
                ghost.className = 'note-flash';
                ghost.style.left = beatToX(quantizedBeat) + 'px';
                ghost.style.top = (midiToY(midi) + 1) + 'px';
                ghost.style.width = (duration * pixelsPerBeat - 2) + 'px';
                ghost.textContent = midiToNoteName(midi);
                pianoRoll.appendChild(ghost);

                // Remove after animation completes
                setTimeout(() => {
                    ghost.remove();
                }, 400);
            }

            function renderPianoRoll() {
                if (!state.grid) return;

                const numBars = parseInt(numBarsSelect.value);
                const beatsPerBar = state.grid.beatsPerBar || 4;
                const totalBeats = numBars * beatsPerBar;
                const width = totalBeats * pixelsPerBeat;
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

                // Update code output
                updateCodeOutput();
            }

            function renderTimelineHeader(totalBeats, beatsPerBar) {
                timelineHeader.innerHTML = '';
                timelineHeader.style.width = (totalBeats * pixelsPerBeat) + 'px';

                for (let bar = 0; bar < totalBeats / beatsPerBar; bar++) {
                    const marker = document.createElement('div');
                    marker.className = 'bar-marker';
                    marker.style.left = (bar * beatsPerBar * pixelsPerBeat) + 'px';
                    marker.textContent = (bar + 1).toString();
                    timelineHeader.appendChild(marker);
                }
            }

            function renderNotes() {
                if (!state.grid) return;

                pianoRoll.querySelectorAll('.note').forEach(el => el.remove());

                for (let i = 0; i < state.grid.notes.length; i++) {
                    const note = state.grid.notes[i];
                    const noteEl = document.createElement('div');
                    noteEl.className = 'note';
                    noteEl.style.left = beatToX(note.startBeat) + 'px';
                    noteEl.style.top = (midiToY(note.midiNote) + 1) + 'px';
                    noteEl.style.width = (note.duration * pixelsPerBeat - 2) + 'px';
                    noteEl.dataset.index = i;
                    noteEl.textContent = midiToNoteName(note.midiNote);

                    // Multiple selection support
                    if (state.selectedNotes.has(i)) {
                        noteEl.classList.add('selected');
                    }

                    const handle = document.createElement('div');
                    handle.className = 'note-resize-handle';
                    noteEl.appendChild(handle);

                    // Click handling is now done by the pianoRoll mousedown handler
                    pianoRoll.appendChild(noteEl);
                }
            }

            // Auto-scroll to center on the notes when melody is loaded
            function scrollToNotes() {
                if (!state.grid || state.grid.notes.length === 0) return;

                // Find the MIDI range of notes
                const midiNotes = state.grid.notes.map(n => n.midiNote);
                const minMidi = Math.min(...midiNotes);
                const maxMidi = Math.max(...midiNotes);
                const centerMidi = Math.round((minMidi + maxMidi) / 2);

                // Calculate Y position to center on the notes
                const centerY = midiToY(centerMidi);
                const containerHeight = pianoRollContainer.clientHeight;
                const targetScrollTop = Math.max(0, centerY - containerHeight / 2);

                pianoRollContainer.scrollTop = targetScrollTop;
                pianoKeyboard.scrollTop = targetScrollTop;
            }

            function onDrag(e) {
                if (!state.isDragging || !state.dragData) return;

                // Calculate content coordinates using container rect + scroll offset
                const containerRect = pianoRollContainer.getBoundingClientRect();
                const x = Math.max(0, e.clientX - containerRect.left + pianoRollContainer.scrollLeft);
                const y = Math.max(0, e.clientY - containerRect.top + pianoRollContainer.scrollTop);
                const gridSize = parseFloat(gridSizeSelect.value);

                if (state.dragMode === 'resize') {
                    // Resize single note
                    const dx = e.clientX - state.dragData.startX;
                    const deltaBeat = Math.round((dx / pixelsPerBeat) / gridSize) * gridSize;
                    const note = state.grid.notes[state.dragData.noteIndex];
                    note.duration = Math.max(gridSize, state.dragData.originalDuration + deltaBeat);
                    renderNotes();
                } else if (state.dragMode === 'move') {
                    // Move all selected notes together
                    const dx = e.clientX - state.dragData.startX;
                    const dy = e.clientY - state.dragData.startY;
                    const deltaBeat = Math.round((dx / pixelsPerBeat) / gridSize) * gridSize;
                    const deltaMidi = -Math.round(dy / NOTE_HEIGHT);

                    for (const orig of state.dragData.originals) {
                        const note = state.grid.notes[orig.index];
                        note.startBeat = Math.max(0, orig.beat + deltaBeat);
                        note.midiNote = Math.min(MAX_MIDI, Math.max(MIN_MIDI, orig.midi + deltaMidi));
                    }
                    renderNotes();
                } else if (state.dragMode === 'create') {
                    // Update preview element width while creating note
                    const preview = document.getElementById('notePreview');
                    if (preview) {
                        const currentBeat = x / pixelsPerBeat;
                        const duration = Math.max(gridSize, Math.round((currentBeat - state.dragData.startBeat) / gridSize) * gridSize);
                        preview.style.width = (duration * pixelsPerBeat - 2) + 'px';
                        state.dragData.currentDuration = duration;
                    }
                } else if (state.dragMode === 'select-rect') {
                    // Update selection rectangle
                    const selRect = document.getElementById('selectionRect');
                    if (selRect) {
                        const startX = state.dragData.startX;
                        const startY = state.dragData.startY;
                        const minX = Math.min(startX, x);
                        const minY = Math.min(startY, y);
                        const width = Math.abs(x - startX);
                        const height = Math.abs(y - startY);

                        selRect.style.left = minX + 'px';
                        selRect.style.top = minY + 'px';
                        selRect.style.width = width + 'px';
                        selRect.style.height = height + 'px';

                        // Find notes within rectangle and highlight them
                        const minBeat = minX / pixelsPerBeat;
                        const maxBeat = (minX + width) / pixelsPerBeat;
                        const maxMidi = MAX_MIDI - Math.floor(minY / NOTE_HEIGHT);
                        const minMidi = MAX_MIDI - Math.floor((minY + height) / NOTE_HEIGHT);

                        state.selectedNotes.clear();
                        state.grid.notes.forEach((note, idx) => {
                            const noteEnd = note.startBeat + note.duration;
                            if (note.startBeat < maxBeat && noteEnd > minBeat &&
                                note.midiNote >= minMidi && note.midiNote <= maxMidi) {
                                state.selectedNotes.add(idx);
                            }
                        });
                        renderNotes();
                    }
                }
            }

            function endDrag() {
                if (!state.isDragging) {
                    document.removeEventListener('mousemove', onDrag);
                    document.removeEventListener('mouseup', endDrag);
                    return;
                }

                const dragMode = state.dragMode;
                state.isDragging = false;
                state.dragMode = null;

                if (dragMode === 'resize') {
                    // Send update for resized note
                    const note = state.grid.notes[state.dragData.noteIndex];
                    vscode.postMessage({
                        command: 'updateNote',
                        noteIndex: state.dragData.noteIndex,
                        updates: { duration: note.duration },
                    });
                } else if (dragMode === 'move') {
                    // Send updates for all moved notes
                    for (const orig of state.dragData.originals) {
                        const note = state.grid.notes[orig.index];
                        vscode.postMessage({
                            command: 'updateNote',
                            noteIndex: orig.index,
                            updates: {
                                startBeat: note.startBeat,
                                midiNote: note.midiNote,
                            },
                        });
                    }
                } else if (dragMode === 'create') {
                    // Remove preview and add actual note
                    const preview = document.getElementById('notePreview');
                    if (preview) preview.remove();

                    const duration = state.dragData.currentDuration || parseFloat(noteLengthSelect.value);
                    vscode.postMessage({
                        command: 'addNote',
                        startBeat: state.dragData.startBeat,
                        duration: duration,
                        midiNote: state.dragData.midi,
                        velocity: 1.0,
                    });
                } else if (dragMode === 'select-rect') {
                    // Remove selection rectangle
                    const selRect = document.getElementById('selectionRect');
                    if (selRect) selRect.remove();
                }

                state.dragData = null;
                updateInfo();
                updateCodeOutput();
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
                        const numBars = parseInt(numBarsSelect.value);
                        vscode.postMessage({
                            command: 'resizeGrid',
                            config: { numBars, beatsPerBar: state.grid.beatsPerBar || 4 },
                        });
                    }
                });

                // Redraw grid when grid size changes
                gridSizeSelect.addEventListener('change', () => {
                    renderPianoRoll();
                });

                document.getElementById('playBtn').addEventListener('click', () => {
                    vscode.postMessage({ command: 'togglePlayback' });
                });

                document.getElementById('sourceBtn').addEventListener('click', () => {
                    vscode.postMessage({ command: 'goToSource' });
                });

                document.getElementById('clearBtn').addEventListener('click', () => {
                    vscode.postMessage({ command: 'clearMelody' });
                });

                document.getElementById('transposeUpBtn').addEventListener('click', () => {
                    vscode.postMessage({ command: 'transpose', semitones: 12 });
                });

                document.getElementById('transposeDownBtn').addEventListener('click', () => {
                    vscode.postMessage({ command: 'transpose', semitones: -12 });
                });

                // Zoom controls
                document.getElementById('zoomInBtn').addEventListener('click', zoomIn);
                document.getElementById('zoomOutBtn').addEventListener('click', zoomOut);
                document.getElementById('fitBtn').addEventListener('click', fitToWindow);

                // Comprehensive mouse handling for piano roll
                pianoRoll.addEventListener('mousedown', (e) => {
                    if (!state.grid) return;

                    const target = e.target;
                    // Calculate content coordinates using container rect + scroll offset
                    // This is the most reliable method for scrollable containers
                    const containerRect = pianoRollContainer.getBoundingClientRect();
                    const x = Math.max(0, e.clientX - containerRect.left + pianoRollContainer.scrollLeft);
                    const y = Math.max(0, e.clientY - containerRect.top + pianoRollContainer.scrollTop);
                    const rawBeat = x / pixelsPerBeat;
                    const beat = xToBeat(x);
                    // Clamp midi to valid range to handle edge clicks
                    const rawMidi = yToMidi(y);
                    const midi = Math.min(MAX_MIDI, Math.max(MIN_MIDI, rawMidi));
                    const gridSize = parseFloat(gridSizeSelect.value);

                    // Check what we clicked on
                    const clickedOnNote = target && target.classList && target.classList.contains('note');
                    const clickedOnResize = target && target.classList && target.classList.contains('note-resize-handle');

                    if (clickedOnResize) {
                        // Start resizing the note
                        e.stopPropagation();
                        const noteEl = target.parentElement;
                        const noteIndex = parseInt(noteEl.dataset.index);
                        const note = state.grid.notes[noteIndex];
                        state.selectedNotes.clear();
                        state.selectedNotes.add(noteIndex);
                        state.isDragging = true;
                        state.dragMode = 'resize';
                        state.dragData = {
                            noteIndex,
                            startX: e.clientX,
                            originalDuration: note.duration,
                        };
                        renderNotes();
                    } else if (clickedOnNote) {
                        // Handle note selection
                        e.stopPropagation();
                        const noteIndex = parseInt(target.dataset.index);
                        const note = state.grid.notes[noteIndex];

                        if (e.shiftKey) {
                            // Toggle selection
                            if (state.selectedNotes.has(noteIndex)) {
                                state.selectedNotes.delete(noteIndex);
                            } else {
                                state.selectedNotes.add(noteIndex);
                            }
                        } else {
                            // Select only this note (unless already selected for multi-drag)
                            if (!state.selectedNotes.has(noteIndex)) {
                                state.selectedNotes.clear();
                                state.selectedNotes.add(noteIndex);
                            }
                        }

                        // Start dragging selected notes
                        state.isDragging = true;
                        state.dragMode = 'move';
                        state.dragData = {
                            startX: e.clientX,
                            startY: e.clientY,
                            // Store original positions for all selected notes
                            originals: Array.from(state.selectedNotes).map(idx => ({
                                index: idx,
                                beat: state.grid.notes[idx].startBeat,
                                midi: state.grid.notes[idx].midiNote,
                            })),
                        };
                        renderNotes();
                    } else if (e.shiftKey && midi >= MIN_MIDI && midi <= MAX_MIDI) {
                        // Shift+click on empty space: Start rectangle selection
                        state.isDragging = true;
                        state.dragMode = 'select-rect';
                        state.dragData = {
                            startX: x,
                            startY: y,
                            startBeat: rawBeat,
                            startMidi: midi,
                        };
                        // Create selection rectangle element
                        const selRect = document.createElement('div');
                        selRect.className = 'selection-rect';
                        selRect.id = 'selectionRect';
                        pianoRoll.appendChild(selRect);
                    } else if (midi >= MIN_MIDI && midi <= MAX_MIDI && beat >= 0) {
                        // Click on empty space: Start creating a note
                        state.selectedNotes.clear();
                        state.isDragging = true;
                        state.dragMode = 'create';
                        const noteLength = parseFloat(noteLengthSelect.value);
                        state.dragData = {
                            startX: x,
                            startBeat: beat,
                            midi: midi,
                            minDuration: gridSize,
                        };
                        // Create preview element
                        const preview = document.createElement('div');
                        preview.className = 'note-preview';
                        preview.id = 'notePreview';
                        preview.style.left = beatToX(beat) + 'px';
                        preview.style.top = (midiToY(midi) + 1) + 'px';
                        preview.style.width = (noteLength * pixelsPerBeat - 2) + 'px';
                        pianoRoll.appendChild(preview);
                        playNote(midi, noteLength);
                        renderNotes();
                    }

                    if (state.isDragging) {
                        document.addEventListener('mousemove', onDrag);
                        document.addEventListener('mouseup', endDrag);
                    }
                });

                // Double-click to delete note
                pianoRoll.addEventListener('dblclick', (e) => {
                    if (e.target.classList.contains('note')) {
                        const index = parseInt(e.target.dataset.index);
                        vscode.postMessage({ command: 'deleteNote', noteIndex: index });
                    }
                });

                // Keyboard shortcuts
                document.addEventListener('keydown', (e) => {
                    if (e.key === 'Delete' || e.key === 'Backspace') {
                        if (state.selectedNotes.size > 0 && state.grid) {
                            // Delete all selected notes (in reverse order to maintain indices)
                            const indices = Array.from(state.selectedNotes).sort((a, b) => b - a);
                            for (const idx of indices) {
                                vscode.postMessage({ command: 'deleteNote', noteIndex: idx });
                            }
                            state.selectedNotes.clear();
                        }
                    }
                    // Select all with Ctrl+A
                    if (e.key === 'a' && (e.ctrlKey || e.metaKey)) {
                        if (state.grid) {
                            e.preventDefault();
                            state.selectedNotes.clear();
                            state.grid.notes.forEach((_, idx) => state.selectedNotes.add(idx));
                            renderNotes();
                        }
                    }
                    // Escape to deselect all
                    if (e.key === 'Escape') {
                        state.selectedNotes.clear();
                        renderNotes();
                    }
                });

                // Handle messages from extension
                window.addEventListener('message', (event) => {
                    const message = event.data;
                    switch (message.type) {
                        case 'connectionStatus':
                            updateConnectionStatus(message.data);
                            break;
                        case 'melodyList':
                            updateMelodyList(message.data);
                            break;
                        case 'melodyUpdate':
                            updateMelody(message.data);
                            break;
                        case 'transportUpdate':
                            updateTransport(message.data);
                            break;
                        case 'noteAssignmentsUpdate':
                            updateNoteAssignments(message.data);
                            break;
                        case 'playingStateUpdate':
                            updatePlayingState(message.data);
                            break;
                    }
                });

                // Sync scroll between keyboard and roll
                pianoRollContainer.addEventListener('scroll', () => {
                    pianoKeyboard.scrollTop = pianoRollContainer.scrollTop;
                });

                // Ctrl+wheel zoom
                pianoRollContainer.addEventListener('wheel', (e) => {
                    if (e.ctrlKey || e.metaKey) {
                        e.preventDefault();
                        if (e.deltaY < 0) {
                            zoomIn();
                        } else {
                            zoomOut();
                        }
                    }
                }, { passive: false });
            }

            // Recording listeners
            function setupRecordingListeners() {
                document.addEventListener('keydown', (e) => {
                    // Don't intercept keys when an input/select/textarea is focused
                    const activeTag = document.activeElement?.tagName?.toLowerCase();
                    const isInputFocused = activeTag === 'input' || activeTag === 'select' || activeTag === 'textarea';

                    // Handle octave shift with + and - keys (but not arrow keys when input focused)
                    if (e.key === '+' || e.key === '=') {
                        e.preventDefault();
                        shiftOctave(1);
                        return;
                    }
                    if (e.key === '-' || e.key === '_') {
                        // Only handle minus if not in an input (allow typing negative numbers)
                        if (!isInputFocused) {
                            e.preventDefault();
                            shiftOctave(-1);
                            return;
                        }
                    }
                    // Arrow keys only shift octave when not in a form element
                    if (!isInputFocused) {
                        if (e.key === 'ArrowUp') {
                            e.preventDefault();
                            shiftOctave(1);
                            return;
                        }
                        if (e.key === 'ArrowDown') {
                            e.preventDefault();
                            shiftOctave(-1);
                            return;
                        }
                    }
                    // Use extended keydown with getCurrentBeat for held-note recording
                    handleNumpadKeydown(e, state.noteAssignments, onNoteKeyDown, 'notes', {
                        showHeld: true,
                        getCurrentBeat: () => getInterpolatedBeat(timingOffsetMs),
                    });
                });
                document.addEventListener('keyup', (e) => {
                    // Handle octave keys on keyup too
                    if (e.key === '+' || e.key === '=' || e.key === '-' || e.key === '_' ||
                        e.key === 'ArrowUp' || e.key === 'ArrowDown') {
                        return;
                    }
                    handleNumpadKeyupWithRelease(e, onNoteKeyRelease, 'notes', {
                        getCurrentBeat: () => getInterpolatedBeat(timingOffsetMs),
                    });
                });
            }

            // Called when numpad key is pressed down
            function onNoteKeyDown(keyIndex, assignment, options) {
                // Apply octave offset (12 semitones per octave)
                const actualMidiNote = Math.max(MIN_MIDI, Math.min(MAX_MIDI, assignment.midiNote + (state.octaveOffset * 12)));

                // Trigger voice for audio feedback (note-on)
                vscode.postMessage({
                    command: 'triggerNoteOn',
                    midiNote: actualMidiNote,
                });

                // Show visual feedback on the piano roll (even when not recording)
                const noteLength = parseFloat(noteLengthSelect.value);
                showNoteFlash(actualMidiNote, noteLength);
            }

            // Called when numpad key is released - create the note with held duration
            function onNoteKeyRelease(keyIndex, assignment, releaseData) {
                // Apply octave offset (12 semitones per octave)
                const actualMidiNote = Math.max(MIN_MIDI, Math.min(MAX_MIDI, assignment.midiNote + (state.octaveOffset * 12)));

                // Trigger note-off for audio
                vscode.postMessage({
                    command: 'triggerNoteOff',
                    midiNote: actualMidiNote,
                });

                // If visual recording is enabled and transport is running, place a note
                if (isRecording && transportState.running && releaseData.startBeat !== undefined) {
                    const startBeat = releaseData.startBeat;
                    const endBeat = releaseData.endBeat;
                    const rawDuration = endBeat - startBeat;

                    // Quantize duration to grid (based on note length selector)
                    const minNoteLength = parseFloat(noteLengthSelect.value);
                    const quantizedDuration = Math.max(minNoteLength, quantizeToGrid(rawDuration, minNoteLength));

                    // Quantize start beat to grid
                    const quantizedStart = quantizeToGrid(startBeat, minNoteLength);

                    vscode.postMessage({
                        command: 'recordNote',
                        midiNote: actualMidiNote,
                        beat: quantizedStart,
                        noteLength: quantizedDuration,
                    });
                }
            }

            // Quantize a value to the nearest grid division
            function quantizeToGrid(value, gridSize) {
                return Math.round(value / gridSize) * gridSize;
            }

            function shiftOctave(direction) {
                state.octaveOffset = Math.max(-3, Math.min(3, state.octaveOffset + direction));
                updateOctaveDisplay();
                updateNoteKeyGrid(); // Update numpad labels to show actual notes with offset
                showToast(direction > 0 ? 'Octave +' + state.octaveOffset : 'Octave ' + state.octaveOffset);
            }

            function updateOctaveDisplay() {
                const octaveDisplay = document.getElementById('octaveDisplay');
                if (octaveDisplay) {
                    const sign = state.octaveOffset > 0 ? '+' : '';
                    octaveDisplay.textContent = sign + state.octaveOffset;
                    octaveDisplay.classList.toggle('positive', state.octaveOffset > 0);
                    octaveDisplay.classList.toggle('negative', state.octaveOffset < 0);
                }
            }

            // Scale definitions (semitone intervals from root)
            const SCALES = {
                chromatic: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                major: [0, 2, 4, 5, 7, 9, 11],
                minor: [0, 2, 3, 5, 7, 8, 10],
                minor_harmonic: [0, 2, 3, 5, 7, 8, 11],
                minor_melodic: [0, 2, 3, 5, 7, 9, 11],
                pentatonic_major: [0, 2, 4, 7, 9],
                pentatonic_minor: [0, 3, 5, 7, 10],
                blues: [0, 3, 5, 6, 7, 10],
                dorian: [0, 2, 3, 5, 7, 9, 10],
                phrygian: [0, 1, 3, 5, 7, 8, 10],
                lydian: [0, 2, 4, 6, 7, 9, 11],
                mixolydian: [0, 2, 4, 5, 7, 9, 10],
                locrian: [0, 1, 3, 5, 6, 8, 10],
            };

            const NOTE_TO_MIDI = { 'C': 0, 'C#': 1, 'D': 2, 'D#': 3, 'E': 4, 'F': 5, 'F#': 6, 'G': 7, 'G#': 8, 'A': 9, 'A#': 10, 'B': 11 };

            function getScaleNotes(root, scaleName, octave) {
                const rootMidi = NOTE_TO_MIDI[root] + (octave * 12);
                const intervals = SCALES[scaleName] || SCALES.chromatic;
                const notes = [];
                for (const interval of intervals) {
                    const midi = rootMidi + interval;
                    if (midi >= MIN_MIDI && midi <= MAX_MIDI) {
                        notes.push(midi);
                    }
                }
                // Extend to get enough notes for 9 keys if needed
                while (notes.length < 9 && notes[notes.length - 1] + 12 <= MAX_MIDI) {
                    const nextOctave = notes.slice(0, intervals.length).map(n => n + 12);
                    for (const n of nextOctave) {
                        if (n <= MAX_MIDI && notes.length < 9) notes.push(n);
                    }
                }
                return notes.slice(0, 9);
            }

            function autoAssignFromScale() {
                const root = document.getElementById('rootNoteSelect').value;
                const scale = document.getElementById('scaleSelect').value;
                const baseOctave = 4; // Start from octave 4

                state.selectedRoot = root;
                state.selectedScale = scale;

                const scaleNotes = getScaleNotes(root, scale, baseOctave);

                vscode.postMessage({
                    command: 'autoAssignNotes',
                    notes: scaleNotes,
                    root: root,
                    scale: scale
                });
            }

            // Note assignment panel events
            function setupNotePanelEvents() {
                // Toggle panel collapse
                document.getElementById('notesToggle')?.addEventListener('click', () => {
                    const panel = document.getElementById('notesPanel');
                    panel.classList.toggle('collapsed');
                    document.getElementById('notesToggle').textContent = panel.classList.contains('collapsed') ? '▶' : '▼';
                });

                // Auto-assign button - now uses scale settings
                document.getElementById('notesAutoAssign')?.addEventListener('click', () => {
                    autoAssignFromScale();
                });

                // Key slot click - show picker
                document.querySelectorAll('#notesKeyGrid .key-slot').forEach(slot => {
                    slot.addEventListener('click', (e) => {
                        const keyIndex = parseInt(slot.dataset.key);
                        showNotePicker(keyIndex, slot.getBoundingClientRect());
                    });
                });

                // Close picker when clicking outside
                document.addEventListener('click', (e) => {
                    const picker = document.getElementById('notesPicker');
                    const isKeySlot = e.target.closest('#notesKeyGrid .key-slot');
                    if (!picker.contains(e.target) && !isKeySlot) {
                        picker.classList.remove('visible');
                    }
                });

                // Octave buttons
                document.getElementById('octaveUpBtn')?.addEventListener('click', () => shiftOctave(1));
                document.getElementById('octaveDownBtn')?.addEventListener('click', () => shiftOctave(-1));

                // Scale/root change triggers auto-assign
                document.getElementById('scaleSelect')?.addEventListener('change', () => autoAssignFromScale());
                document.getElementById('rootNoteSelect')?.addEventListener('change', () => autoAssignFromScale());
            }

            let activePickerKeyIndex = null;

            function showNotePicker(keyIndex, rect) {
                activePickerKeyIndex = keyIndex;
                const picker = document.getElementById('notesPicker');

                picker.style.left = (rect.right + 4) + 'px';
                picker.style.top = rect.top + 'px';

                // Generate notes C3 to C6
                const notes = [];
                for (let midi = 48; midi <= 84; midi++) {
                    if (!isBlackKey(midi)) {
                        notes.push({ midi, name: midiToNoteName(midi) });
                    }
                }

                picker.innerHTML = notes.map(n => {
                    const current = state.noteAssignments.get(keyIndex);
                    const isSelected = current && current.midiNote === n.midi;
                    return '<div class="key-picker-item ' + (isSelected ? 'selected' : '') + '" data-midi="' + n.midi + '">' + n.name + '</div>';
                }).join('');

                picker.classList.add('visible');

                // Picker item click
                picker.querySelectorAll('.key-picker-item').forEach(item => {
                    item.addEventListener('click', () => {
                        const midiNote = parseInt(item.dataset.midi);
                        vscode.postMessage({
                            command: 'assignNoteToKey',
                            keyIndex: activePickerKeyIndex,
                            midiNote: midiNote,
                        });
                        picker.classList.remove('visible');
                    });
                });
            }

            // Code panel events
            function setupCodePanelEvents() {
                document.getElementById('codeToggle')?.addEventListener('click', () => {
                    const panel = document.getElementById('codePanel');
                    panel.classList.toggle('collapsed');
                    document.getElementById('codeToggle').textContent = panel.classList.contains('collapsed') ? '▶' : '▼';
                });

                document.getElementById('codeCopy')?.addEventListener('click', () => {
                    const code = document.getElementById('codeOutput').textContent;
                    if (code && !code.startsWith('Edit')) {
                        vscode.postMessage({ command: 'copyToClipboard', text: code });
                    }
                });

                document.getElementById('codeSave')?.addEventListener('click', () => {
                    vscode.postMessage({ command: 'writeBackToFile' });
                });
            }

            function updateConnectionStatus(data) {
                const loadingIndicator = document.getElementById('loadingIndicator');
                if (data.status === 'connecting') {
                    if (loadingIndicator) loadingIndicator.style.display = 'inline';
                    melodySelect.innerHTML = '<option value="">Connecting...</option>';
                    melodySelect.disabled = true;
                } else if (data.status === 'connected') {
                    if (loadingIndicator) loadingIndicator.style.display = 'none';
                    melodySelect.disabled = false;
                } else if (data.status === 'disconnected' || data.status === 'error') {
                    if (loadingIndicator) loadingIndicator.style.display = 'none';
                    melodySelect.innerHTML = '<option value="">Not connected</option>';
                    melodySelect.disabled = true;
                }
            }

            function updateMelodyList(melodies) {
                state.melodies = melodies;

                // Group melodies by their group path
                const groups = new Map();
                for (const m of melodies) {
                    const groupPath = m.groupPath || 'main';
                    if (!groups.has(groupPath)) {
                        groups.set(groupPath, []);
                    }
                    groups.get(groupPath).push(m);
                }

                // Build the select with optgroups
                melodySelect.innerHTML = '<option value="">Select a melody...</option>';

                // Sort groups alphabetically
                const sortedGroups = Array.from(groups.entries()).sort((a, b) => a[0].localeCompare(b[0]));

                for (const [groupPath, groupMelodies] of sortedGroups) {
                    if (sortedGroups.length > 1) {
                        // Multiple groups - use optgroups
                        const optgroup = document.createElement('optgroup');
                        optgroup.label = groupPath;

                        for (const m of groupMelodies) {
                            const option = document.createElement('option');
                            option.value = m.name;
                            option.textContent = m.name + (m.voiceName ? ' → ' + m.voiceName : '') + (m.isPlaying ? ' ▶' : '');
                            optgroup.appendChild(option);
                        }
                        melodySelect.appendChild(optgroup);
                    } else {
                        // Single group - flat list
                        for (const m of groupMelodies) {
                            const option = document.createElement('option');
                            option.value = m.name;
                            option.textContent = m.name + (m.voiceName ? ' → ' + m.voiceName : '') + (m.isPlaying ? ' ▶' : '');
                            melodySelect.appendChild(option);
                        }
                    }
                }

                if (state.currentMelody) {
                    melodySelect.value = state.currentMelody.melodyName;
                }
            }

            function updateMelody(data) {
                state.currentMelody = data;
                state.grid = data.grid;
                state.locallyModified = data.locallyModified || false;
                state.isDynamic = data.isDynamic || false;

                if (state.grid) {
                    numBarsSelect.value = state.grid.numBars;
                }

                emptyState.style.display = 'none';
                document.querySelector('.main-content').style.display = 'flex';

                // Update modified indicator
                const modIndicator = document.getElementById('modifiedIndicator');
                if (modIndicator) {
                    modIndicator.style.display = state.locallyModified ? 'inline' : 'none';
                }

                // Update save button state based on isDynamic
                updateSaveButtonState();

                renderPianoRoll();
                updateInfo();

                // Auto-scroll to show the notes (center on the note range)
                scrollToNotes();

                const melody = state.melodies.find(m => m.name === data.melodyName);
                document.getElementById('playBtn').textContent = melody?.isPlaying ? '⏸' : '▶';
            }

            function updateSaveButtonState() {
                const saveBtn = document.getElementById('codeSave');
                if (!saveBtn) return;

                if (state.isDynamic) {
                    saveBtn.disabled = true;
                    saveBtn.setAttribute('data-tooltip', 'Cannot save: melody is generated dynamically (no static .notes() in source code). Use Copy to get the generated code.');
                    saveBtn.classList.add('disabled-dynamic');
                } else {
                    saveBtn.disabled = false;
                    saveBtn.setAttribute('data-tooltip', 'Save changes back to source file');
                    saveBtn.classList.remove('disabled-dynamic');
                }
            }

            function updateNoteAssignments(data) {
                state.noteAssignments = new Map();
                for (const a of data.assignments) {
                    state.noteAssignments.set(a.keyIndex, { midiNote: a.midiNote, noteName: a.noteName });
                }
                updateNoteKeyGrid();
            }

            function updateNoteKeyGrid() {
                const slots = document.querySelectorAll('#notesKeyGrid .key-slot');
                slots.forEach(slot => {
                    const keyIndex = parseInt(slot.dataset.key);
                    const assignment = state.noteAssignments.get(keyIndex);
                    const labelEl = slot.querySelector('.key-label');
                    if (assignment) {
                        // Apply octave offset to show actual note that will play
                        const actualMidiNote = Math.max(MIN_MIDI, Math.min(MAX_MIDI, assignment.midiNote + (state.octaveOffset * 12)));
                        labelEl.textContent = midiToNoteName(actualMidiNote);
                        labelEl.classList.remove('empty');
                    } else {
                        labelEl.textContent = '-';
                        labelEl.classList.add('empty');
                    }
                });
            }

            function updatePlayingState(data) {
                document.getElementById('playBtn').textContent = data.isPlaying ? '⏸' : '▶';
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

            function updateCodeOutput() {
                if (!state.grid || !state.currentMelody) return;

                const lanes = generateMultiLaneMelodyStrings(state.grid);
                const laneCount = lanes.length;

                let code;
                if (laneCount <= 1) {
                    // Single lane - simple format
                    code = 'melody("' + state.currentMelody.melodyName + '").on(' + (state.currentMelody.voiceName || 'voice') + ').notes("' + (lanes[0] || '') + '").start();';
                } else {
                    // Multiple lanes - polyphonic format
                    const notesCalls = lanes.map(lane => '    .notes("' + lane + '")').join('\\n');
                    code = 'melody("' + state.currentMelody.melodyName + '")\\n    .on(' + (state.currentMelody.voiceName || 'voice') + ')\\n' + notesCalls + '\\n    .start();';
                }

                const codeEl = document.getElementById('codeOutput');
                if (codeEl) {
                    codeEl.textContent = code.replace(/\\\\n/g, '\\n');
                    codeEl.classList.remove('empty');
                }

                // Update lane count badge
                const lanesBadge = document.getElementById('lanesBadge');
                if (lanesBadge) {
                    if (laneCount > 1) {
                        lanesBadge.textContent = laneCount + ' lanes';
                        lanesBadge.style.display = 'inline';
                    } else {
                        lanesBadge.style.display = 'none';
                    }
                }
            }

            // Split notes into non-overlapping lanes (greedy algorithm)
            // Chord detection from MIDI notes
            function detectChordFromMidiNotes(midiNotes) {
                if (midiNotes.length < 2) return null;

                const sorted = [...midiNotes].sort((a, b) => a - b);
                const lowestNote = sorted[0];

                const chordTypes = [
                    { name: 'maj7', intervals: [0, 4, 7, 11] },
                    { name: '7', intervals: [0, 4, 7, 10] },
                    { name: 'm7', intervals: [0, 3, 7, 10] },
                    { name: 'dim7', intervals: [0, 3, 6, 9] },
                    { name: 'm7b5', intervals: [0, 3, 6, 10] },
                    { name: 'mmaj7', intervals: [0, 3, 7, 11] },
                    { name: 'add9', intervals: [0, 4, 7, 14] },
                    { name: '6', intervals: [0, 4, 7, 9] },
                    { name: 'm6', intervals: [0, 3, 7, 9] },
                    { name: 'maj', intervals: [0, 4, 7] },
                    { name: 'min', intervals: [0, 3, 7] },
                    { name: 'dim', intervals: [0, 3, 6] },
                    { name: 'aug', intervals: [0, 4, 8] },
                    { name: 'sus2', intervals: [0, 2, 7] },
                    { name: 'sus4', intervals: [0, 5, 7] },
                    { name: '5', intervals: [0, 7] },
                ];

                const normalizedIntervals = sorted.map(n => (n - lowestNote) % 12);
                const uniqueNormalized = [...new Set(normalizedIntervals)].sort((a, b) => a - b);

                // Try matching with lowest note as root
                for (const chord of chordTypes) {
                    const chordNormalized = chord.intervals.map(i => i % 12);
                    const uniqueChordNormalized = [...new Set(chordNormalized)].sort((a, b) => a - b);

                    if (uniqueChordNormalized.length === uniqueNormalized.length &&
                        uniqueChordNormalized.every((v, i) => v === uniqueNormalized[i])) {
                        return { root: lowestNote, quality: chord.name };
                    }
                }

                // Try inversions
                for (let i = 1; i < sorted.length; i++) {
                    const potentialRoot = sorted[i];
                    const rootClass = potentialRoot % 12;
                    const intervalsFromRoot = sorted.map(n => {
                        const interval = (n % 12) - rootClass;
                        return interval < 0 ? interval + 12 : interval;
                    }).sort((a, b) => a - b);
                    const uniqueFromRoot = [...new Set(intervalsFromRoot)];

                    for (const chord of chordTypes) {
                        const chordNormalized = chord.intervals.map(i => i % 12);
                        const uniqueChordNormalized = [...new Set(chordNormalized)].sort((a, b) => a - b);

                        if (uniqueChordNormalized.length === uniqueFromRoot.length &&
                            uniqueChordNormalized.every((v, i) => v === uniqueFromRoot[i])) {
                            const rootInLowestOctave = (lowestNote - (lowestNote % 12)) + rootClass;
                            return { root: rootInLowestOctave, quality: chord.name };
                        }
                    }
                }
                return null;
            }

            function splitIntoLanes(notes) {
                if (notes.length === 0) return [[]];

                const sorted = [...notes].sort((a, b) => a.startBeat - b.startBeat || a.midiNote - b.midiNote);
                const lanes = [];

                for (const note of sorted) {
                    let placed = false;
                    for (let i = 0; i < lanes.length; i++) {
                        const lane = lanes[i];
                        if (lane.length === 0) {
                            lane.push(note);
                            placed = true;
                            break;
                        }
                        const last = lane[lane.length - 1];
                        if (note.startBeat >= last.startBeat + last.duration - 0.001) {
                            lane.push(note);
                            placed = true;
                            break;
                        }
                    }
                    if (!placed) {
                        lanes.push([note]);
                    }
                }
                return lanes.length > 0 ? lanes : [[]];
            }

            // Generate melody strings for all lanes
            function generateMultiLaneMelodyStrings(grid) {
                const lanes = splitIntoLanes(grid.notes);
                return lanes.map(laneNotes => generateMelodyStringFromGrid({ ...grid, notes: laneNotes }));
            }

            function generateMelodyStringFromGrid(grid) {
                if (!grid.notes || grid.notes.length === 0) return '. . . . | . . . .';

                const numBars = grid.numBars || 4;
                const beatsPerBar = grid.beatsPerBar || 4;

                // Determine the finest grid resolution needed for the notes
                let smallestDivision = 1; // Start with whole beats
                for (const note of grid.notes) {
                    // Check what the smallest beat fraction is
                    const startFrac = note.startBeat % 1;
                    const durFrac = note.duration % 1;
                    if (startFrac !== 0 || durFrac !== 0) {
                        // Find the finest division needed
                        for (const div of [0.5, 0.25, 0.125, 0.0625, 0.03125]) {
                            if (Math.abs(note.startBeat % div) < 0.001 && Math.abs(note.duration % div) < 0.001) {
                                if (div < smallestDivision) smallestDivision = div;
                                break;
                            }
                            if (div < smallestDivision) smallestDivision = div;
                        }
                    }
                }

                // Use at least 4 steps per bar, but more if notes require finer resolution
                const stepsPerBar = Math.max(4, Math.round(beatsPerBar / smallestDivision));
                const beatPerStep = beatsPerBar / stepsPerBar;
                const sortedNotes = [...grid.notes].sort((a, b) => a.startBeat - b.startBeat);

                const bars = [];
                for (let barIdx = 0; barIdx < numBars; barIdx++) {
                    const barStart = barIdx * beatsPerBar;
                    const steps = [];

                    for (let stepIdx = 0; stepIdx < stepsPerBar; stepIdx++) {
                        const stepBeat = barStart + stepIdx * beatPerStep;
                        const stepEnd = stepBeat + beatPerStep;

                        const startingNotes = sortedNotes.filter(n => n.startBeat >= stepBeat - 0.001 && n.startBeat < stepEnd - 0.001);
                        const sustainedNotes = sortedNotes.filter(n => n.startBeat < stepBeat - 0.001 && (n.startBeat + n.duration) > stepBeat + 0.001);

                        if (startingNotes.length > 0) {
                            // Group notes with same start beat and duration (chords)
                            const firstNote = startingNotes[0];
                            const chordNotes = startingNotes.filter(n =>
                                Math.abs(n.startBeat - firstNote.startBeat) < 0.01 &&
                                Math.abs(n.duration - firstNote.duration) < 0.01
                            );

                            if (chordNotes.length > 1) {
                                // Try to detect chord quality
                                const midiNotes = chordNotes.map(n => n.midiNote);
                                const detected = detectChordFromMidiNotes(midiNotes);

                                if (detected) {
                                    const rootName = midiToNoteName(detected.root);
                                    steps.push(rootName + ':' + detected.quality);
                                } else {
                                    // Unrecognized chord - use root note
                                    const rootMidi = Math.min(...midiNotes);
                                    steps.push(midiToNoteName(rootMidi));
                                }
                            } else {
                                steps.push(midiToNoteName(chordNotes[0].midiNote));
                            }
                        } else if (sustainedNotes.length > 0) {
                            steps.push('-');
                        } else {
                            steps.push('.');
                        }
                    }
                    bars.push(steps.join(' '));
                }
                return bars.join(' | ');
            }

            function animatePlayhead() {
                if (transportState.running && state.grid) {
                    const totalBeats = state.grid.totalBeats;
                    const loopBeat = getInterpolatedBeat() % totalBeats;
                    playhead.style.left = beatToX(loopBeat) + 'px';
                    playhead.classList.add('visible');
                } else {
                    playhead.classList.remove('visible');
                }
                requestAnimationFrame(animatePlayhead);
            }

            // Start
            init();
        `;

        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Melody Editor</title>
    <style>
        ${styles}
    </style>
</head>
<body>
    <div class="toolbar">
        <div class="toolbar-group">
            <span class="toolbar-label">Melody</span>
            <select id="melodySelect" data-tooltip="Select a melody to edit">
                <option value="">Select a melody...</option>
            </select>
            <span id="loadingIndicator" class="loading-spinner" style="display: none;" data-tooltip="Connecting to runtime...">⏳</span>
            <button class="btn btn-icon" id="playBtn" data-tooltip="Play/Stop melody">▶</button>
            <button class="btn btn-icon" id="sourceBtn" data-tooltip="Jump to source code">📄</button>
            <span id="modifiedIndicator" class="badge badge-modified" style="display: none;">Modified</span>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <span class="toolbar-label">Bars</span>
            <select id="numBars" data-tooltip="Number of bars in the melody">
                <option value="1">1</option>
                <option value="2">2</option>
                <option value="4" selected>4</option>
                <option value="8">8</option>
                <option value="16">16</option>
            </select>
        </div>

        <div class="toolbar-group">
            <span class="toolbar-label">Grid</span>
            <select id="gridSize" data-tooltip="Snap notes to grid">
                <option value="1">1 beat</option>
                <option value="0.5">1/2</option>
                <option value="0.25" selected>1/4</option>
                <option value="0.125">1/8</option>
                <option value="0.0625">1/16</option>
                <option value="0.03125">1/32</option>
            </select>
        </div>

        <div class="toolbar-group">
            <span class="toolbar-label">Length</span>
            <select id="noteLength" data-tooltip="Default note duration when clicking">
                <option value="1">1 beat</option>
                <option value="0.5">1/2</option>
                <option value="0.25" selected>1/4</option>
                <option value="0.125">1/8</option>
                <option value="0.0625">1/16</option>
                <option value="0.03125">1/32</option>
            </select>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <button class="btn btn-danger" id="clearBtn" data-tooltip="Clear all notes">Clear</button>
            <button class="btn" id="transposeUpBtn" data-tooltip="Transpose up one octave">+1 oct</button>
            <button class="btn" id="transposeDownBtn" data-tooltip="Transpose down one octave">-1 oct</button>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <span class="toolbar-label">Zoom</span>
            <button class="btn btn-icon" id="zoomOutBtn" data-tooltip="Zoom out">-</button>
            <span class="zoom-display" id="zoomDisplay">100%</span>
            <button class="btn btn-icon" id="zoomInBtn" data-tooltip="Zoom in">+</button>
            <button class="btn" id="fitBtn" data-tooltip="Fit to window width">Fit</button>
        </div>

        <div class="toolbar-divider"></div>

        ${renderTimingSlider(-50)}

        <div class="toolbar-group">
            ${renderRecordButton()}
        </div>
    </div>

    <div class="timeline-header">
        <div class="timeline-header-spacer">BAR</div>
        <div class="timeline-header-content" id="timelineHeader"></div>
        <div class="shortcut-hints">
            <span class="shortcut-hint"><kbd>Click</kbd> Add note</span>
            <span class="shortcut-hint"><kbd>Drag</kbd> Move/resize</span>
            <span class="shortcut-hint"><kbd>Del</kbd> Delete</span>
        </div>
    </div>

    <div class="main-content">
        <div class="piano-keyboard" id="pianoKeyboard"></div>
        <div class="piano-roll-container" id="pianoRollContainer">
            <div class="piano-roll" id="pianoRoll">
                <div class="playhead" id="playhead"></div>
            </div>
        </div>

        <div class="empty-state" id="emptyState">
            <div class="empty-icon">🎹</div>
            <h3>No Melody Selected</h3>
            <p>Select a melody from the dropdown above to start editing. Click the piano roll to add notes.</p>
        </div>
    </div>

    <div class="info-bar">
        <div class="info-item">
            <span class="info-label">Notes</span>
            <span class="info-value" id="noteCount">0</span>
        </div>
        <div class="info-item">
            <span class="info-label">Range</span>
            <span class="info-value" id="noteRange">-</span>
        </div>
        <div class="info-item">
            <span class="info-label">Duration</span>
            <span class="info-value" id="duration">16 beats</span>
        </div>
        <div class="info-item">
            <span id="lanesBadge" class="badge badge-info" style="display: none;" data-tooltip="Polyphonic: notes auto-split into non-overlapping lanes">1 lane</span>
        </div>
        <div class="shortcut-hints" style="margin-left: auto;">
            <span class="shortcut-hint"><kbd>1-9</kbd> Record notes (when REC enabled)</span>
        </div>
    </div>

    <!-- Note Assignment Panel (for numpad recording) -->
    ${renderKeyAssignmentPanel('notes', 'Numpad Recording', 'Press keys 1-9 to record notes', true)}

    <!-- Scale/Octave Controls Panel -->
    <div class="key-assignment-panel scale-controls-panel" id="scaleControlsPanel">
        <div class="panel-header">
            <span class="panel-icon">🎼</span>
            <span class="panel-title">Scale & Octave<span class="panel-subtitle">Configure auto-assign</span></span>
        </div>
        <div class="scale-controls-container">
            <div class="scale-control-row">
                <div class="scale-control-group">
                    <label class="control-label">Root</label>
                    <select id="rootNoteSelect" class="scale-select" data-tooltip="Root note for scale">
                        <option value="C">C</option>
                        <option value="C#">C#/Db</option>
                        <option value="D">D</option>
                        <option value="D#">D#/Eb</option>
                        <option value="E">E</option>
                        <option value="F">F</option>
                        <option value="F#">F#/Gb</option>
                        <option value="G">G</option>
                        <option value="G#">G#/Ab</option>
                        <option value="A">A</option>
                        <option value="A#">A#/Bb</option>
                        <option value="B">B</option>
                    </select>
                </div>
                <div class="scale-control-group">
                    <label class="control-label">Scale</label>
                    <select id="scaleSelect" class="scale-select" data-tooltip="Scale for auto-assign">
                        <option value="chromatic">Chromatic</option>
                        <option value="major">Major</option>
                        <option value="minor">Minor (Natural)</option>
                        <option value="minor_harmonic">Minor (Harmonic)</option>
                        <option value="minor_melodic">Minor (Melodic)</option>
                        <option value="pentatonic_major">Pentatonic Major</option>
                        <option value="pentatonic_minor">Pentatonic Minor</option>
                        <option value="blues">Blues</option>
                        <option value="dorian">Dorian</option>
                        <option value="phrygian">Phrygian</option>
                        <option value="lydian">Lydian</option>
                        <option value="mixolydian">Mixolydian</option>
                        <option value="locrian">Locrian</option>
                    </select>
                </div>
                <div class="scale-control-group octave-control">
                    <label class="control-label">Octave</label>
                    <div class="octave-buttons">
                        <button class="btn btn-small" id="octaveDownBtn" data-tooltip="Shift down octave (- key)">−</button>
                        <span class="octave-display" id="octaveDisplay" data-tooltip="Current octave offset">0</span>
                        <button class="btn btn-small" id="octaveUpBtn" data-tooltip="Shift up octave (+ key)">+</button>
                    </div>
                </div>
            </div>
            <div class="scale-hint">
                <kbd>+</kbd>/<kbd>−</kbd> or <kbd>↑</kbd>/<kbd>↓</kbd> to shift octave
            </div>
        </div>
    </div>

    <!-- Generated Code Panel -->
    ${renderCodePanel('code', 'Generated Code', 'Edit the melody above to see code here...')}

    <!-- Toast notifications -->
    ${renderToastContainer()}

    <script>
        const vscode = acquireVsCodeApi();
        ${scripts}
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
            font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif);
            background: var(--vscode-editor-background);
            color: var(--vscode-editor-foreground);
            display: flex;
            align-items: center;
            justify-content: center;
            height: 100vh;
            margin: 0;
        }
        .empty-state {
            text-align: center;
            color: var(--vscode-descriptionForeground);
        }
        .empty-icon {
            font-size: 64px;
            margin-bottom: 20px;
            opacity: 0.3;
        }
        h2 { font-size: 18px; font-weight: 500; margin-bottom: 8px; color: var(--vscode-editor-foreground); }
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
