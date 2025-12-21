"use strict";
/**
 * Melody Parser Utility
 *
 * Parses VibeLang melody strings into piano roll representation
 * and generates melody strings from piano roll data.
 *
 * Melody Formats:
 * 1. Note pattern: "C4 - - . | E4 - - ." (bar-based, - ties, . is rest)
 * 2. Note array: ["C4", "E4", "G4"] (handled differently, not parsed here)
 * 3. Scale degrees: "1 2 3 4" or "1:maj 2:min" (requires scale context)
 * 4. Chords: "C4:maj7" or "1:maj7"
 *
 * Bar separator handling:
 * - Leading/trailing `|` are ignored
 * - Consecutive `||` are collapsed to single bar separator
 *
 * Piano Roll Representation:
 * - Each note has: startBeat, duration, midiNote, velocity
 * - Chords have multiple notes at the same beat
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.detectChordFromMidiNotes = detectChordFromMidiNotes;
exports.parseNoteToMidi = parseNoteToMidi;
exports.midiToNoteName = midiToNoteName;
exports.parseMelodyString = parseMelodyString;
exports.generateMelodyString = generateMelodyString;
exports.createEmptyMelodyGrid = createEmptyMelodyGrid;
exports.addNote = addNote;
exports.removeNote = removeNote;
exports.updateNote = updateNote;
exports.findNoteAt = findNoteAt;
exports.quantizeBeat = quantizeBeat;
exports.transpose = transpose;
exports.shiftTime = shiftTime;
exports.getMelodyStats = getMelodyStats;
exports.splitIntoLanes = splitIntoLanes;
exports.generateMultiLaneMelodyStrings = generateMultiLaneMelodyStrings;
exports.parseMultiLaneMelodyStrings = parseMultiLaneMelodyStrings;
exports.countLanes = countLanes;
const barUtils_1 = require("./barUtils");
// Note name to MIDI mapping (C4 = 60)
const NOTE_BASES = {
    'C': 0, 'D': 2, 'E': 4, 'F': 5, 'G': 7, 'A': 9, 'B': 11
};
// Scale intervals (semitones from root for each degree 1-7)
const SCALES = {
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
const CHORDS = {
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
 * Detect a chord quality from a set of MIDI notes.
 * Returns the chord root (as MIDI) and quality string, or null if not a recognized chord.
 *
 * The function tries all possible roots (each note in the set) and finds the best match.
 * Priority: longer chords (7ths) over shorter (triads), exact matches over partial.
 */
function detectChordFromMidiNotes(midiNotes) {
    if (midiNotes.length < 2)
        return null;
    // Sort notes ascending
    const sorted = [...midiNotes].sort((a, b) => a - b);
    const lowestNote = sorted[0];
    // Calculate intervals relative to the lowest note
    const intervals = sorted.map(n => n - lowestNote);
    // Try to match against known chord types
    // Prefer longer matches (7th chords over triads)
    const chordTypes = [
        // 7th chords first (prefer more specific chords)
        { name: 'maj7', intervals: [0, 4, 7, 11] },
        { name: '7', intervals: [0, 4, 7, 10] },
        { name: 'm7', intervals: [0, 3, 7, 10] },
        { name: 'dim7', intervals: [0, 3, 6, 9] },
        { name: 'm7b5', intervals: [0, 3, 6, 10] },
        { name: 'mmaj7', intervals: [0, 3, 7, 11] },
        { name: 'add9', intervals: [0, 4, 7, 14] },
        { name: '6', intervals: [0, 4, 7, 9] },
        { name: 'm6', intervals: [0, 3, 7, 9] },
        // Triads
        { name: 'maj', intervals: [0, 4, 7] },
        { name: 'min', intervals: [0, 3, 7] },
        { name: 'dim', intervals: [0, 3, 6] },
        { name: 'aug', intervals: [0, 4, 8] },
        { name: 'sus2', intervals: [0, 2, 7] },
        { name: 'sus4', intervals: [0, 5, 7] },
        // Dyads
        { name: '5', intervals: [0, 7] },
    ];
    // Normalize intervals to within an octave (for inversions)
    const normalizedIntervals = intervals.map(i => i % 12);
    const uniqueNormalized = [...new Set(normalizedIntervals)].sort((a, b) => a - b);
    // Try to match with the lowest note as root
    for (const chord of chordTypes) {
        const chordNormalized = chord.intervals.map(i => i % 12);
        const uniqueChordNormalized = [...new Set(chordNormalized)].sort((a, b) => a - b);
        // Check if all chord tones are present
        if (uniqueChordNormalized.length === uniqueNormalized.length &&
            uniqueChordNormalized.every((v, i) => v === uniqueNormalized[i])) {
            return { root: lowestNote, quality: chord.name };
        }
    }
    // Try inversions - each note could be the root
    for (let i = 1; i < sorted.length; i++) {
        const potentialRoot = sorted[i];
        const rootClass = potentialRoot % 12;
        // Calculate intervals as if this note were the root
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
                // Found a match - return the actual root note (in the original octave range)
                const rootInLowestOctave = (lowestNote - (lowestNote % 12)) + rootClass;
                return { root: rootInLowestOctave, quality: chord.name };
            }
        }
    }
    return null; // No chord match found
}
/**
 * Parse a single note name to MIDI number
 * Supports: C4, C#4, Db4, C, C#, etc.
 */
function parseNoteToMidi(noteName) {
    const name = noteName.trim().toUpperCase();
    if (!name)
        return null;
    // Check if it's already a MIDI number
    const midiNum = parseInt(name, 10);
    if (!isNaN(midiNum) && midiNum >= 0 && midiNum <= 127) {
        return midiNum;
    }
    let idx = 0;
    // Parse note letter
    const letter = name[idx];
    if (!NOTE_BASES.hasOwnProperty(letter))
        return null;
    let semitone = NOTE_BASES[letter];
    idx++;
    // Parse accidentals
    while (idx < name.length) {
        const ch = name[idx];
        if (ch === '#') {
            semitone++;
            idx++;
        }
        else if (ch === 'B' && idx > 0) {
            // Check if this 'B' is a flat or the next note
            // It's a flat if it follows the note letter directly
            const prevWasLetter = NOTE_BASES.hasOwnProperty(name[idx - 1]);
            if (prevWasLetter || name[idx - 1] === '#') {
                semitone--;
                idx++;
            }
            else {
                break;
            }
        }
        else {
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
    if (midi < 0 || midi > 127)
        return null;
    return midi;
}
/**
 * Convert MIDI number to note name
 */
function midiToNoteName(midi) {
    const notes = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
    const octave = Math.floor(midi / 12) - 1;
    const note = notes[midi % 12];
    return note + octave;
}
/**
 * Parse a note or chord string to MIDI note(s)
 * Supports: C4, C4:maj7, etc.
 */
function parseNoteOrChord(noteStr) {
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
    if (rootMidi === null)
        return null;
    const intervals = CHORDS[quality];
    if (!intervals)
        return [rootMidi]; // Unknown quality, return root only
    return intervals.map(interval => rootMidi + interval).filter(m => m >= 0 && m <= 127);
}
/**
 * Resolve a scale degree to MIDI note(s)
 */
function resolveScaleDegree(degree, chordQuality, scale, rootNote) {
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
 * Tokenize a bar of melody
 */
function tokenizeBar(bar, scale, root) {
    const tokens = [];
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
            let quality = null;
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
            }
            else {
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
                    }
                    else if (next !== '-') {
                        noteStr += next;
                        i++;
                    }
                    else {
                        break;
                    }
                }
                else {
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
function parseMelodyString(melody, config) {
    const beatsPerBar = config?.beatsPerBar ?? 4;
    const scale = config?.scale;
    const root = config?.root;
    // Use unified bar splitting (handles leading/trailing/consecutive pipes)
    const bars = (0, barUtils_1.parseBars)(melody);
    const numBars = bars.length;
    // Handle empty input
    if (numBars === 0) {
        return createEmptyMelodyGrid({ numBars: 1, beatsPerBar, scale, root });
    }
    const totalBeats = numBars * beatsPerBar;
    const notes = [];
    let currentBeat = 0;
    // Track pending note(s) for ties
    let pendingNotes = null;
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
function generateMelodyString(grid, stepsPerBar = 4) {
    const { notes, numBars, beatsPerBar } = grid;
    // Sort notes by start beat
    const sortedNotes = [...notes].sort((a, b) => a.startBeat - b.startBeat);
    const bars = [];
    const beatPerStep = beatsPerBar / stepsPerBar;
    for (let barIdx = 0; barIdx < numBars; barIdx++) {
        const barStart = barIdx * beatsPerBar;
        const barEnd = barStart + beatsPerBar;
        const steps = [];
        for (let stepIdx = 0; stepIdx < stepsPerBar; stepIdx++) {
            const stepBeat = barStart + stepIdx * beatPerStep;
            const stepEnd = stepBeat + beatPerStep;
            // Find notes that start at this step
            const startingNotes = sortedNotes.filter(n => n.startBeat >= stepBeat && n.startBeat < stepEnd);
            // Find notes that are sustained through this step
            const sustainedNotes = sortedNotes.filter(n => n.startBeat < stepBeat && (n.startBeat + n.duration) > stepBeat);
            if (startingNotes.length > 0) {
                // New note(s) start here
                // Group by same start time and duration (chords)
                const firstNote = startingNotes[0];
                const chordNotes = startingNotes.filter(n => Math.abs(n.startBeat - firstNote.startBeat) < 0.01 &&
                    Math.abs(n.duration - firstNote.duration) < 0.01);
                if (chordNotes.length > 1) {
                    // Try to detect chord quality from the MIDI notes
                    const midiNotes = chordNotes.map(n => n.midiNote);
                    const detected = detectChordFromMidiNotes(midiNotes);
                    if (detected) {
                        // Use chord notation (e.g., "C4:maj7")
                        const rootName = midiToNoteName(detected.root);
                        steps.push(`${rootName}:${detected.quality}`);
                    }
                    else {
                        // Unrecognized chord - just use the root note
                        const rootMidi = Math.min(...midiNotes);
                        steps.push(midiToNoteName(rootMidi));
                    }
                }
                else {
                    steps.push(midiToNoteName(chordNotes[0].midiNote));
                }
            }
            else if (sustainedNotes.length > 0) {
                // Note is sustained (tie)
                steps.push('-');
            }
            else {
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
function createEmptyMelodyGrid(config) {
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
function addNote(grid, note) {
    return {
        ...grid,
        notes: [...grid.notes, note],
    };
}
/**
 * Remove a note from the grid
 */
function removeNote(grid, noteIndex) {
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
function updateNote(grid, noteIndex, updates) {
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
function findNoteAt(grid, beat, midiNote) {
    return grid.notes.findIndex(n => n.midiNote === midiNote &&
        beat >= n.startBeat &&
        beat < n.startBeat + n.duration);
}
/**
 * Quantize beat to nearest grid position
 */
function quantizeBeat(beat, gridSize) {
    return Math.round(beat / gridSize) * gridSize;
}
/**
 * Transpose all notes
 */
function transpose(grid, semitones) {
    const newNotes = grid.notes.map(n => ({
        ...n,
        midiNote: Math.max(0, Math.min(127, n.midiNote + semitones)),
    }));
    return { ...grid, notes: newNotes };
}
/**
 * Shift all notes in time
 */
function shiftTime(grid, beats) {
    const newNotes = grid.notes.map(n => ({
        ...n,
        startBeat: Math.max(0, n.startBeat + beats),
    })).filter(n => n.startBeat < grid.totalBeats);
    return { ...grid, notes: newNotes };
}
/**
 * Get note statistics
 */
function getMelodyStats(grid) {
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
/**
 * Split notes into non-overlapping lanes for polyphonic melodies.
 *
 * Uses a greedy algorithm to minimize the number of lanes:
 * 1. Sort notes by startBeat, then by midiNote (for determinism)
 * 2. For each note, find the first lane where it doesn't overlap
 * 3. If no lane available, create a new lane
 *
 * @param notes - Array of melody notes
 * @returns Array of lanes, each containing non-overlapping notes
 */
function splitIntoLanes(notes) {
    if (notes.length === 0)
        return [[]];
    // Sort by start beat, then by pitch (for determinism)
    const sorted = [...notes].sort((a, b) => a.startBeat - b.startBeat || a.midiNote - b.midiNote);
    const lanes = [];
    for (const note of sorted) {
        const noteEnd = note.startBeat + note.duration;
        // Find first lane where note doesn't overlap
        let placed = false;
        for (let i = 0; i < lanes.length; i++) {
            const lane = lanes[i];
            if (lane.length === 0) {
                lane.push({ ...note, laneIndex: i });
                placed = true;
                break;
            }
            const lastNote = lane[lane.length - 1];
            const lastEnd = lastNote.startBeat + lastNote.duration;
            // Check if note starts after last note ends (no overlap)
            // Use small epsilon for floating point comparison
            if (note.startBeat >= lastEnd - 0.001) {
                lane.push({ ...note, laneIndex: i });
                placed = true;
                break;
            }
        }
        // No suitable lane found, create new one
        if (!placed) {
            const newIndex = lanes.length;
            lanes.push([{ ...note, laneIndex: newIndex }]);
        }
    }
    // Ensure at least one (possibly empty) lane
    if (lanes.length === 0) {
        lanes.push([]);
    }
    return lanes;
}
/**
 * Generate multiple melody strings for polyphonic melodies.
 * Each lane becomes a separate .notes() call.
 *
 * @param grid - The melody grid
 * @param stepsPerBar - Steps per bar (default 4)
 * @returns Array of melody strings, one per lane
 */
function generateMultiLaneMelodyStrings(grid, stepsPerBar = 4) {
    const lanes = splitIntoLanes(grid.notes);
    // Generate a melody string for each lane
    return lanes.map(laneNotes => {
        // Create a temporary grid with just this lane's notes
        const laneGrid = {
            ...grid,
            notes: laneNotes.map(n => ({
                startBeat: n.startBeat,
                duration: n.duration,
                midiNote: n.midiNote,
                velocity: n.velocity,
                isChordTone: n.isChordTone,
            })),
        };
        return generateMelodyString(laneGrid, stepsPerBar);
    });
}
/**
 * Parse multiple melody strings (lanes) into a single MelodyGrid.
 * All notes from all lanes are merged into a flat array.
 *
 * @param lanes - Array of melody strings
 * @param config - Melody configuration
 * @returns Combined MelodyGrid with all notes
 */
function parseMultiLaneMelodyStrings(lanes, config) {
    if (lanes.length === 0) {
        return createEmptyMelodyGrid({
            numBars: config?.numBars ?? 1,
            beatsPerBar: config?.beatsPerBar ?? 4,
            scale: config?.scale,
            root: config?.root,
        });
    }
    // Parse each lane
    const grids = lanes.map(lane => parseMelodyString(lane, config));
    // Combine all notes
    const allNotes = [];
    for (const grid of grids) {
        allNotes.push(...grid.notes);
    }
    // Use the maximum dimensions
    const maxBars = Math.max(...grids.map(g => g.numBars));
    const beatsPerBar = grids[0]?.beatsPerBar ?? 4;
    return {
        notes: allNotes,
        totalBeats: maxBars * beatsPerBar,
        numBars: maxBars,
        beatsPerBar,
        scale: config?.scale,
        root: config?.root,
    };
}
/**
 * Count the number of lanes needed for a melody.
 *
 * @param notes - Array of melody notes
 * @returns Number of lanes needed
 */
function countLanes(notes) {
    return splitIntoLanes(notes).length;
}
//# sourceMappingURL=melodyParser.js.map