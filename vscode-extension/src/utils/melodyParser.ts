/**
 * Melody Parser Utility
 *
 * Parses VibeLang melody strings into piano roll representation
 * and generates melody strings from piano roll data.
 *
 * Melody Formats:
 * 1. Note pattern: "C4 - - . | E4 - - ." (bar-based, - ties, . is rest)
 * 2. Step pattern: "C4...E4...G4..." (dots continue previous note)
 * 3. Note array: ["C4", "E4", "G4"] (handled differently, not parsed here)
 * 4. Scale degrees: "1 2 3 4" or "1:maj 2:min" (requires scale context)
 * 5. Chords: "C4:maj7" or "1:maj7"
 *
 * Piano Roll Representation:
 * - Each note has: startBeat, duration, midiNote, velocity
 * - Chords have multiple notes at the same beat
 */

export interface MelodyNote {
    startBeat: number;
    duration: number;      // in beats
    midiNote: number;      // 0-127
    velocity: number;      // 0.0-1.0
    isChordTone?: boolean; // true if part of a multi-note chord
}

export interface MelodyGrid {
    notes: MelodyNote[];
    totalBeats: number;
    numBars: number;
    beatsPerBar: number;
    scale?: string;
    root?: string;
}

export interface MelodyConfig {
    numBars: number;
    beatsPerBar: number;
    scale?: string;
    root?: string;
}

// Note name to MIDI mapping (C4 = 60)
const NOTE_BASES: Record<string, number> = {
    'C': 0, 'D': 2, 'E': 4, 'F': 5, 'G': 7, 'A': 9, 'B': 11
};

// Scale intervals (semitones from root for each degree 1-7)
const SCALES: Record<string, number[]> = {
    'major': [0, 2, 4, 5, 7, 9, 11],
    'ionian': [0, 2, 4, 5, 7, 9, 11],
    'minor': [0, 2, 3, 5, 7, 8, 10],
    'natural_minor': [0, 2, 3, 5, 7, 8, 10],
    'aeolian': [0, 2, 3, 5, 7, 8, 10],
    'dorian': [0, 2, 3, 5, 7, 9, 10],
    'phrygian': [0, 1, 3, 5, 7, 8, 10],
    'lydian': [0, 2, 4, 6, 7, 9, 11],
    'mixolydian': [0, 2, 4, 5, 7, 9, 10],
    'locrian': [0, 1, 3, 5, 6, 8, 10],
    'harmonic_minor': [0, 2, 3, 5, 7, 8, 11],
    'melodic_minor': [0, 2, 3, 5, 7, 9, 11],
    'pentatonic': [0, 2, 4, 7, 9, 12, 14],
    'minor_pentatonic': [0, 3, 5, 7, 10, 12, 15],
    'blues': [0, 3, 5, 6, 7, 10, 12],
};

// Chord intervals (semitones from root)
const CHORDS: Record<string, number[]> = {
    'maj': [0, 4, 7],
    'major': [0, 4, 7],
    'min': [0, 3, 7],
    'm': [0, 3, 7],
    'minor': [0, 3, 7],
    'dim': [0, 3, 6],
    'diminished': [0, 3, 6],
    'aug': [0, 4, 8],
    'augmented': [0, 4, 8],
    'sus2': [0, 2, 7],
    'sus4': [0, 5, 7],
    'maj7': [0, 4, 7, 11],
    'major7': [0, 4, 7, 11],
    '7': [0, 4, 7, 10],
    'dom7': [0, 4, 7, 10],
    'min7': [0, 3, 7, 10],
    'm7': [0, 3, 7, 10],
    'dim7': [0, 3, 6, 9],
    'm7b5': [0, 3, 6, 10],
    'mmaj7': [0, 3, 7, 11],
    '9': [0, 4, 7, 10, 14],
    'maj9': [0, 4, 7, 11, 14],
    'm9': [0, 3, 7, 10, 14],
    'add9': [0, 4, 7, 14],
    '6': [0, 4, 7, 9],
    'm6': [0, 3, 7, 9],
    '5': [0, 7],
    'power': [0, 7],
};

/**
 * Parse a single note name to MIDI number
 * Supports: C4, C#4, Db4, C, C#, etc.
 */
export function parseNoteToMidi(noteName: string): number | null {
    const name = noteName.trim().toUpperCase();
    if (!name) return null;

    // Check if it's already a MIDI number
    const midiNum = parseInt(name, 10);
    if (!isNaN(midiNum) && midiNum >= 0 && midiNum <= 127) {
        return midiNum;
    }

    let idx = 0;

    // Parse note letter
    const letter = name[idx];
    if (!NOTE_BASES.hasOwnProperty(letter)) return null;
    let semitone = NOTE_BASES[letter];
    idx++;

    // Parse accidentals
    while (idx < name.length) {
        const ch = name[idx];
        if (ch === '#') {
            semitone++;
            idx++;
        } else if (ch === 'B' && idx > 0) {
            // Check if this 'B' is a flat or the next note
            // It's a flat if it follows the note letter directly
            const prevWasLetter = NOTE_BASES.hasOwnProperty(name[idx - 1]);
            if (prevWasLetter || name[idx - 1] === '#') {
                semitone--;
                idx++;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Parse octave
    let octave = 4; // Default octave
    if (idx < name.length) {
        const octaveStr = name.slice(idx).match(/^-?\d+/);
        if (octaveStr) {
            octave = parseInt(octaveStr[0], 10);
        }
    }

    // Calculate MIDI: (octave + 1) * 12 + semitone
    const midi = (octave + 1) * 12 + semitone;
    if (midi < 0 || midi > 127) return null;
    return midi;
}

/**
 * Convert MIDI number to note name
 */
export function midiToNoteName(midi: number): string {
    const notes = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
    const octave = Math.floor(midi / 12) - 1;
    const note = notes[midi % 12];
    return note + octave;
}

/**
 * Parse a note or chord string to MIDI note(s)
 * Supports: C4, C4:maj7, etc.
 */
function parseNoteOrChord(noteStr: string): number[] | null {
    const colonIdx = noteStr.indexOf(':');

    if (colonIdx === -1) {
        // Single note
        const midi = parseNoteToMidi(noteStr);
        return midi !== null ? [midi] : null;
    }

    // Note with chord quality
    const notePart = noteStr.slice(0, colonIdx);
    const quality = noteStr.slice(colonIdx + 1).toLowerCase();

    const rootMidi = parseNoteToMidi(notePart);
    if (rootMidi === null) return null;

    const intervals = CHORDS[quality];
    if (!intervals) return [rootMidi]; // Unknown quality, return root only

    return intervals.map(interval => rootMidi + interval).filter(m => m >= 0 && m <= 127);
}

/**
 * Resolve a scale degree to MIDI note(s)
 */
function resolveScaleDegree(
    degree: number,
    chordQuality: string | null,
    scale: string,
    rootNote: string
): number[] {
    const scaleIntervals = SCALES[scale.toLowerCase()] || SCALES['major'];
    const rootMidi = parseNoteToMidi(rootNote) ?? 60;

    const degreeIdx = (degree - 1) % scaleIntervals.length;
    const interval = scaleIntervals[degreeIdx];
    const noteMidi = rootMidi + interval;

    if (chordQuality) {
        const chordIntervals = CHORDS[chordQuality.toLowerCase()];
        if (chordIntervals) {
            return chordIntervals.map(ci => noteMidi + ci).filter(m => m >= 0 && m <= 127);
        }
    }

    return [noteMidi];
}

/**
 * Token types for melody parsing
 */
type MelodyToken =
    | { type: 'note'; notes: number[] }
    | { type: 'degree'; degree: number; quality: string | null }
    | { type: 'tie' }
    | { type: 'rest' };

/**
 * Tokenize a bar of melody
 */
function tokenizeBar(bar: string, scale?: string, root?: string): MelodyToken[] {
    const tokens: MelodyToken[] = [];
    let i = 0;

    while (i < bar.length) {
        const ch = bar[i];

        // Skip whitespace
        if (/\s/.test(ch)) {
            i++;
            continue;
        }

        // Tie
        if (ch === '-') {
            tokens.push({ type: 'tie' });
            i++;
            continue;
        }

        // Rest
        if (ch === '.' || ch === '_') {
            tokens.push({ type: 'rest' });
            i++;
            continue;
        }

        // Scale degree (1-7)
        if (/[1-7]/.test(ch)) {
            const degree = parseInt(ch, 10);
            i++;

            // Check for chord quality
            let quality: string | null = null;
            if (i < bar.length && bar[i] === ':') {
                i++; // skip ':'
                let qualityStr = '';
                while (i < bar.length && /[a-zA-Z0-9]/.test(bar[i])) {
                    qualityStr += bar[i];
                    i++;
                }
                quality = qualityStr || null;
            }

            // If we have scale context, resolve now
            if (scale && root) {
                const midiNotes = resolveScaleDegree(degree, quality, scale, root);
                tokens.push({ type: 'note', notes: midiNotes });
            } else {
                tokens.push({ type: 'degree', degree, quality });
            }
            continue;
        }

        // Note name (A-G)
        if (/[A-Ga-g]/.test(ch)) {
            let noteStr = ch.toUpperCase();
            i++;

            // Collect accidentals and octave
            while (i < bar.length) {
                const next = bar[i];
                if (next === '#' || next === 'b' || /\d/.test(next) || next === '-') {
                    // Handle negative octave
                    if (next === '-' && i + 1 < bar.length && /\d/.test(bar[i + 1])) {
                        noteStr += next;
                        i++;
                    } else if (next !== '-') {
                        noteStr += next;
                        i++;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            // Check for chord quality
            if (i < bar.length && bar[i] === ':') {
                i++; // skip ':'
                while (i < bar.length && /[a-zA-Z0-9]/.test(bar[i])) {
                    noteStr += ':' + bar[i];
                    i++;
                }
                // Re-add the colon to noteStr properly
                const colonPos = noteStr.lastIndexOf(':');
                if (colonPos === -1) {
                    // Reconstruct with chord quality
                    let quality = '';
                    let j = i - 1;
                    while (j >= 0 && /[a-zA-Z0-9]/.test(bar[j])) {
                        quality = bar[j] + quality;
                        j--;
                    }
                    noteStr += ':' + quality;
                }
            }

            const midiNotes = parseNoteOrChord(noteStr);
            if (midiNotes) {
                tokens.push({ type: 'note', notes: midiNotes });
            }
            continue;
        }

        // Unknown character, skip
        i++;
    }

    return tokens;
}

/**
 * Parse a melody string into MelodyGrid
 */
export function parseMelodyString(melody: string, config?: Partial<MelodyConfig>): MelodyGrid {
    const beatsPerBar = config?.beatsPerBar ?? 4;
    const scale = config?.scale;
    const root = config?.root;

    // Split by bar separator
    const bars = melody.split('|');
    const numBars = bars.length;
    const totalBeats = numBars * beatsPerBar;

    const notes: MelodyNote[] = [];
    let currentBeat = 0;

    // Track pending note(s) for ties
    let pendingNotes: number[] | null = null;
    let pendingStart = 0;
    let pendingDuration = 0;

    function commitPending() {
        if (pendingNotes) {
            for (let i = 0; i < pendingNotes.length; i++) {
                notes.push({
                    startBeat: pendingStart,
                    duration: pendingDuration,
                    midiNote: pendingNotes[i],
                    velocity: 1.0,
                    isChordTone: pendingNotes.length > 1,
                });
            }
            pendingNotes = null;
        }
    }

    for (let barIdx = 0; barIdx < bars.length; barIdx++) {
        const bar = bars[barIdx];
        const tokens = tokenizeBar(bar, scale, root);

        if (tokens.length === 0) {
            // Empty bar, commit pending and move on
            commitPending();
            currentBeat += beatsPerBar;
            continue;
        }

        const beatPerToken = beatsPerBar / tokens.length;

        for (let tokenIdx = 0; tokenIdx < tokens.length; tokenIdx++) {
            const token = tokens[tokenIdx];
            const beat = currentBeat + tokenIdx * beatPerToken;

            switch (token.type) {
                case 'tie':
                    // Extend pending note
                    if (pendingNotes) {
                        pendingDuration += beatPerToken;
                    }
                    break;

                case 'rest':
                    // Commit pending and rest
                    commitPending();
                    break;

                case 'note':
                    // Commit previous and start new
                    commitPending();
                    pendingNotes = token.notes;
                    pendingStart = beat;
                    pendingDuration = beatPerToken;
                    break;

                case 'degree':
                    // Degree without context - treat as rest for now
                    commitPending();
                    break;
            }
        }

        currentBeat += beatsPerBar;
    }

    // Commit any remaining pending note
    commitPending();

    return {
        notes,
        totalBeats,
        numBars,
        beatsPerBar,
        scale,
        root,
    };
}

/**
 * Generate a melody string from MelodyGrid
 * Uses the note pattern format: "C4 - - . | E4 - - ."
 */
export function generateMelodyString(grid: MelodyGrid, stepsPerBar: number = 4): string {
    const { notes, numBars, beatsPerBar } = grid;

    // Sort notes by start beat
    const sortedNotes = [...notes].sort((a, b) => a.startBeat - b.startBeat);

    const bars: string[] = [];
    const beatPerStep = beatsPerBar / stepsPerBar;

    for (let barIdx = 0; barIdx < numBars; barIdx++) {
        const barStart = barIdx * beatsPerBar;
        const barEnd = barStart + beatsPerBar;

        const steps: string[] = [];

        for (let stepIdx = 0; stepIdx < stepsPerBar; stepIdx++) {
            const stepBeat = barStart + stepIdx * beatPerStep;
            const stepEnd = stepBeat + beatPerStep;

            // Find notes that start at this step
            const startingNotes = sortedNotes.filter(n =>
                n.startBeat >= stepBeat && n.startBeat < stepEnd
            );

            // Find notes that are sustained through this step
            const sustainedNotes = sortedNotes.filter(n =>
                n.startBeat < stepBeat && (n.startBeat + n.duration) > stepBeat
            );

            if (startingNotes.length > 0) {
                // New note(s) start here
                // Group by same start time (chords)
                const chordNotes = startingNotes.filter(n =>
                    Math.abs(n.startBeat - startingNotes[0].startBeat) < 0.01
                );

                if (chordNotes.length > 1) {
                    // Chord - use the root note with chord detection
                    // For simplicity, just use the first note for now
                    const rootMidi = Math.min(...chordNotes.map(n => n.midiNote));
                    const noteName = midiToNoteName(rootMidi);
                    steps.push(noteName);
                } else {
                    steps.push(midiToNoteName(chordNotes[0].midiNote));
                }
            } else if (sustainedNotes.length > 0) {
                // Note is sustained (tie)
                steps.push('-');
            } else {
                // Rest
                steps.push('.');
            }
        }

        bars.push(steps.join(' '));
    }

    return bars.join(' | ');
}

/**
 * Create an empty melody grid
 */
export function createEmptyMelodyGrid(config: MelodyConfig): MelodyGrid {
    return {
        notes: [],
        totalBeats: config.numBars * config.beatsPerBar,
        numBars: config.numBars,
        beatsPerBar: config.beatsPerBar,
        scale: config.scale,
        root: config.root,
    };
}

/**
 * Add a note to the grid
 */
export function addNote(grid: MelodyGrid, note: MelodyNote): MelodyGrid {
    return {
        ...grid,
        notes: [...grid.notes, note],
    };
}

/**
 * Remove a note from the grid
 */
export function removeNote(grid: MelodyGrid, noteIndex: number): MelodyGrid {
    const newNotes = [...grid.notes];
    newNotes.splice(noteIndex, 1);
    return {
        ...grid,
        notes: newNotes,
    };
}

/**
 * Update a note in the grid
 */
export function updateNote(grid: MelodyGrid, noteIndex: number, updates: Partial<MelodyNote>): MelodyGrid {
    const newNotes = [...grid.notes];
    newNotes[noteIndex] = { ...newNotes[noteIndex], ...updates };
    return {
        ...grid,
        notes: newNotes,
    };
}

/**
 * Find a note at a specific position
 */
export function findNoteAt(grid: MelodyGrid, beat: number, midiNote: number): number {
    return grid.notes.findIndex(n =>
        n.midiNote === midiNote &&
        beat >= n.startBeat &&
        beat < n.startBeat + n.duration
    );
}

/**
 * Quantize beat to nearest grid position
 */
export function quantizeBeat(beat: number, gridSize: number): number {
    return Math.round(beat / gridSize) * gridSize;
}

/**
 * Transpose all notes
 */
export function transpose(grid: MelodyGrid, semitones: number): MelodyGrid {
    const newNotes = grid.notes.map(n => ({
        ...n,
        midiNote: Math.max(0, Math.min(127, n.midiNote + semitones)),
    }));
    return { ...grid, notes: newNotes };
}

/**
 * Shift all notes in time
 */
export function shiftTime(grid: MelodyGrid, beats: number): MelodyGrid {
    const newNotes = grid.notes.map(n => ({
        ...n,
        startBeat: Math.max(0, n.startBeat + beats),
    })).filter(n => n.startBeat < grid.totalBeats);
    return { ...grid, notes: newNotes };
}

/**
 * Get note statistics
 */
export function getMelodyStats(grid: MelodyGrid) {
    const notes = grid.notes;
    if (notes.length === 0) {
        return { count: 0, lowest: null, highest: null, range: 0 };
    }

    const midiNotes = notes.map(n => n.midiNote);
    const lowest = Math.min(...midiNotes);
    const highest = Math.max(...midiNotes);

    return {
        count: notes.length,
        lowest,
        highest,
        lowestName: midiToNoteName(lowest),
        highestName: midiToNoteName(highest),
        range: highest - lowest,
    };
}
