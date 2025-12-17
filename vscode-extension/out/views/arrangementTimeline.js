"use strict";
/**
 * VibeLang Arrangement Timeline
 *
 * Professional DAW-style horizontal timeline view showing:
 * - Time ruler with bars/beats
 * - Tracks for groups and sequences
 * - Clips (patterns, melodies) as colored blocks
 * - Real-time playhead with smooth animation
 * - Zoom and scroll controls
 * - Click-to-navigate to source code
 *
 * Uses canvas-based rendering for smooth performance with many elements.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.ArrangementTimeline = void 0;
const vscode = require("vscode");
const automationTypes_1 = require("../utils/automationTypes");
class ArrangementTimeline {
    constructor(panel, store) {
        this._disposables = [];
        this._automationLanes = [];
        this._panel = panel;
        this._store = store;
        this._updateContent();
        // Listen for state updates
        this._disposables.push(store.onFullUpdate(() => this._sendStateUpdate()));
        this._disposables.push(store.onTransportUpdate((transport) => this._sendTransportUpdate(transport)));
        this._disposables.push(store.onStatusChange(() => this._updateContent()));
        // Handle messages from webview
        this._panel.webview.onDidReceiveMessage((message) => this._handleMessage(message), null, this._disposables);
        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    }
    static createOrShow(store) {
        const column = vscode.ViewColumn.One;
        if (ArrangementTimeline.currentPanel) {
            ArrangementTimeline.currentPanel._panel.reveal(column);
            return;
        }
        const panel = vscode.window.createWebviewPanel(ArrangementTimeline.viewType, 'Arrangement', column, {
            enableScripts: true,
            retainContextWhenHidden: true,
        });
        ArrangementTimeline.currentPanel = new ArrangementTimeline(panel, store);
    }
    static revive(panel, store) {
        ArrangementTimeline.currentPanel = new ArrangementTimeline(panel, store);
    }
    _updateContent() {
        this._panel.webview.html = this._getHtmlContent();
        // Send initial state after a short delay to ensure webview is ready
        setTimeout(() => this._sendStateUpdate(), 100);
    }
    _sendStateUpdate() {
        const state = this._store.state;
        if (state) {
            this._panel.webview.postMessage({
                type: 'stateUpdate',
                data: {
                    ...this._buildTimelineData(state),
                    automationLanes: this._automationLanes,
                    availableTargets: this._getAvailableAutomationTargets(state),
                },
            });
        }
    }
    _getAvailableAutomationTargets(state) {
        const targets = [];
        // Groups - common parameters
        for (const group of state.groups) {
            targets.push({ type: 'group', name: group.path, param: 'amp' });
            targets.push({ type: 'group', name: group.path, param: 'pan' });
            // Add custom params
            for (const param of Object.keys(group.params || {})) {
                targets.push({ type: 'group', name: group.path, param });
            }
        }
        // Voices - synth parameters
        for (const voice of state.voices) {
            targets.push({ type: 'voice', name: voice.name, param: 'gain' });
            for (const param of Object.keys(voice.params || {})) {
                targets.push({ type: 'voice', name: voice.name, param });
            }
        }
        // Effects - effect parameters
        for (const effect of state.effects) {
            for (const param of Object.keys(effect.params || {})) {
                targets.push({ type: 'effect', name: effect.id, param });
            }
        }
        return targets;
    }
    _sendTransportUpdate(transport) {
        this._panel.webview.postMessage({
            type: 'transportUpdate',
            data: transport,
        });
    }
    _buildTimelineData(state) {
        // Build a clean timeline with proper hierarchy:
        // - Groups (depth 0): container for sequences
        // - Master sequences (depth 1): the main playing sequences
        // - Clip lanes (depth 2): patterns, melodies, or sub-sequences within
        const tracks = [];
        // Lookup maps
        const patternsByName = new Map(state.patterns.map(p => [p.name, p]));
        const melodiesByName = new Map(state.melodies.map(m => [m.name, m]));
        const sequencesByName = new Map(state.sequences.map(s => [s.name, s]));
        // Find the active (playing) sequences - these are the "master" sequences
        const activeSequenceNames = new Set(state.live.active_sequences.map(s => s.name));
        // Find ALL playing patterns and melodies (regardless of how they were started)
        const playingPatterns = state.patterns.filter(p => p.is_looping || p.status?.state === 'playing' || p.status?.state === 'queued');
        const playingMelodies = state.melodies.filter(m => m.is_looping || m.status?.state === 'playing' || m.status?.state === 'queued');
        // Calculate global max loop beats from ALL playing sources
        let maxLoopBeats = 4;
        // From active sequences
        for (const active of state.live.active_sequences) {
            const seq = sequencesByName.get(active.name);
            if (seq && seq.loop_beats > maxLoopBeats) {
                maxLoopBeats = seq.loop_beats;
            }
        }
        // From playing patterns
        for (const p of playingPatterns) {
            maxLoopBeats = Math.max(maxLoopBeats, p.loop_beats || 4);
        }
        // From playing melodies
        for (const m of playingMelodies) {
            maxLoopBeats = Math.max(maxLoopBeats, m.loop_beats || 4);
        }
        // Helper to get loop beats for any clip type
        const getItemLoopBeats = (name, type) => {
            if (type === 'pattern')
                return patternsByName.get(name)?.loop_beats || 4;
            if (type === 'melody')
                return melodiesByName.get(name)?.loop_beats || 4;
            if (type === 'sequence')
                return sequencesByName.get(name)?.loop_beats || 4;
            return 4;
        };
        // Helper to get source location
        const getSourceLocation = (name, type) => {
            if (type === 'pattern')
                return patternsByName.get(name)?.source_location;
            if (type === 'melody')
                return melodiesByName.get(name)?.source_location;
            if (type === 'sequence')
                return sequencesByName.get(name)?.source_location;
            return undefined;
        };
        // Find which group each active sequence belongs to
        const findGroupForSequence = (seq) => {
            for (const clip of seq.clips) {
                if (clip.type === 'pattern') {
                    const p = patternsByName.get(clip.name);
                    if (p?.group_path)
                        return p.group_path;
                }
                if (clip.type === 'melody') {
                    const m = melodiesByName.get(clip.name);
                    if (m?.group_path)
                        return m.group_path;
                }
                if (clip.type === 'sequence') {
                    // Recursively find group from sub-sequence
                    const subSeq = sequencesByName.get(clip.name);
                    if (subSeq) {
                        const groupPath = findGroupForSequence(subSeq);
                        if (groupPath)
                            return groupPath;
                    }
                }
            }
            return null;
        };
        // Group active sequences by their group
        const groupActiveSequences = new Map();
        for (const active of state.live.active_sequences) {
            const seq = sequencesByName.get(active.name);
            if (!seq)
                continue;
            const groupPath = findGroupForSequence(seq);
            if (groupPath) {
                if (!groupActiveSequences.has(groupPath)) {
                    groupActiveSequences.set(groupPath, []);
                }
                groupActiveSequences.get(groupPath).push(seq);
            }
        }
        // Build tracks for groups with active sequences
        for (const group of state.groups) {
            const groupPath = group.path;
            const groupId = `group:${groupPath}`;
            const activeSeqs = groupActiveSequences.get(groupPath) || [];
            if (activeSeqs.length === 0)
                continue;
            // Count effects for this group
            const groupEffectsCount = state.effects.filter(e => e.group_path === groupPath).length;
            // GROUP ROW (depth 0)
            tracks.push({
                id: groupId,
                name: group.name,
                type: 'group',
                color: '#858585',
                clips: [{
                        id: `${groupId}:bar`,
                        name: group.name,
                        type: 'group',
                        startBeat: 0,
                        endBeat: maxLoopBeats,
                        color: '#4a4a4a',
                        active: true,
                    }],
                muted: group.muted,
                soloed: group.soloed,
                active: true,
                sourceLocation: group.source_location,
                depth: 0,
                hasChildren: true,
                childCount: activeSeqs.length,
                // Mixer controls
                groupPath: groupPath,
                amp: group.params['amp'] ?? 1.0,
                pan: group.params['pan'] ?? 0,
                effectsCount: groupEffectsCount,
            });
            // MASTER SEQUENCE ROWS (depth 1)
            for (const seq of activeSeqs) {
                const seqId = `seq:${seq.name}`;
                const seqLoopBeats = seq.loop_beats || maxLoopBeats;
                // Helper to collect unique pattern/melody names recursively
                const collectUniqueNames = (clips) => {
                    const names = new Set();
                    for (const clip of clips) {
                        if (clip.type === 'sequence') {
                            const subSeq = sequencesByName.get(clip.name);
                            if (subSeq) {
                                const subNames = collectUniqueNames(subSeq.clips);
                                subNames.forEach(n => names.add(n));
                            }
                        }
                        else {
                            names.add(clip.name);
                        }
                    }
                    return names;
                };
                const uniquePatternNames = collectUniqueNames(seq.clips);
                // Sequence row - shows full tiled duration (spans maxLoopBeats, not just seqLoopBeats)
                tracks.push({
                    id: seqId,
                    name: seq.name,
                    type: 'sequence',
                    color: '#569cd6',
                    clips: [{
                            id: `${seqId}:bar`,
                            name: seq.name,
                            type: 'sequence',
                            startBeat: 0,
                            endBeat: maxLoopBeats,
                            color: '#569cd6',
                            active: true,
                            sourceLocation: seq.source_location,
                        }],
                    loopBeats: seqLoopBeats,
                    active: true,
                    sourceLocation: seq.source_location,
                    depth: 1,
                    parentId: groupId,
                    hasChildren: uniquePatternNames.size > 0,
                    childCount: uniquePatternNames.size,
                });
                const collectClips = (clips, parentStart, parentEnd, parentLoopBeats) => {
                    const result = [];
                    for (const clip of clips) {
                        const clipStart = (clip.start_beat || 0) + parentStart;
                        const clipEnd = Math.min((clip.end_beat ?? parentLoopBeats) + parentStart, parentEnd);
                        if (clip.type === 'sequence') {
                            // Recurse into sub-sequence
                            const subSeq = sequencesByName.get(clip.name);
                            if (subSeq) {
                                const subSeqLoopBeats = subSeq.loop_beats || 4;
                                const clipDuration = clipEnd - clipStart;
                                const numLoops = Math.ceil(clipDuration / subSeqLoopBeats);
                                // For each loop of the sub-sequence
                                for (let loop = 0; loop < numLoops; loop++) {
                                    const loopStart = clipStart + (loop * subSeqLoopBeats);
                                    const loopEnd = Math.min(loopStart + subSeqLoopBeats, clipEnd);
                                    if (loopStart >= clipEnd)
                                        break;
                                    // Collect clips from sub-sequence, offset by loop position
                                    const subClips = collectClips(subSeq.clips, loopStart, loopEnd, subSeqLoopBeats);
                                    result.push(...subClips);
                                }
                            }
                        }
                        else {
                            // Pattern or melody - add directly
                            const itemLoopBeats = getItemLoopBeats(clip.name, clip.type);
                            const clipDuration = clipEnd - clipStart;
                            const numReps = Math.ceil(clipDuration / itemLoopBeats);
                            for (let i = 0; i < numReps; i++) {
                                const blockStart = clipStart + (i * itemLoopBeats);
                                const blockEnd = Math.min(blockStart + itemLoopBeats, clipEnd);
                                if (blockStart >= clipEnd)
                                    break;
                                result.push({
                                    name: clip.name,
                                    type: clip.type,
                                    startBeat: blockStart,
                                    endBeat: blockEnd,
                                });
                            }
                        }
                    }
                    return result;
                };
                // Collect all clips from the master sequence, tiled to fill maxLoopBeats
                // This ensures short sequences (like 4-beat drum patterns) repeat to fill
                // the timeline alongside longer sequences (like 32-beat melodies)
                const allClipOccurrences = [];
                const numSeqLoops = Math.ceil(maxLoopBeats / seqLoopBeats);
                for (let loop = 0; loop < numSeqLoops; loop++) {
                    const loopStart = loop * seqLoopBeats;
                    const loopEnd = Math.min(loopStart + seqLoopBeats, maxLoopBeats);
                    if (loopStart >= maxLoopBeats)
                        break;
                    const loopClips = collectClips(seq.clips, loopStart, loopEnd, seqLoopBeats);
                    allClipOccurrences.push(...loopClips);
                }
                // Group by name
                const clipsByName = new Map();
                for (const occ of allClipOccurrences) {
                    if (!clipsByName.has(occ.name)) {
                        clipsByName.set(occ.name, []);
                    }
                    clipsByName.get(occ.name).push(occ);
                }
                // Create one lane per unique pattern/melody name
                for (const [clipName, occurrences] of clipsByName) {
                    const laneId = `${seqId}:${clipName}`;
                    const firstOcc = occurrences[0];
                    const clipType = firstOcc.type;
                    const itemLoopBeats = getItemLoopBeats(clipName, clipType);
                    const timelineClips = occurrences.map((occ, i) => ({
                        id: `${laneId}:${i}`,
                        name: clipName,
                        type: clipType,
                        startBeat: occ.startBeat,
                        endBeat: occ.endBeat,
                        color: this._getClipColor(clipType),
                        active: true,
                        sourceLocation: getSourceLocation(clipName, clipType),
                    }));
                    tracks.push({
                        id: laneId,
                        name: clipName,
                        type: clipType,
                        color: this._getClipColor(clipType),
                        clips: timelineClips,
                        loopBeats: itemLoopBeats,
                        active: true,
                        sourceLocation: getSourceLocation(clipName, clipType),
                        depth: 2,
                        parentId: seqId,
                        hasChildren: false,
                    });
                }
            }
        }
        // Check if the sequence-based approach produced useful clip lanes (depth 2)
        // If not, show playing patterns/melodies directly with proper tiling
        const hasClipLanes = tracks.some(t => t.depth === 2 && t.clips.length > 0);
        if (!hasClipLanes && (playingPatterns.length > 0 || playingMelodies.length > 0)) {
            // Clear any partial tracks from the sequence-based approach
            tracks.length = 0;
            // Group playing items by their group
            const groupPlayingItems = new Map();
            for (const p of playingPatterns) {
                const groupPath = p.group_path;
                if (!groupPlayingItems.has(groupPath)) {
                    groupPlayingItems.set(groupPath, { patterns: [], melodies: [] });
                }
                groupPlayingItems.get(groupPath).patterns.push(p);
            }
            for (const m of playingMelodies) {
                const groupPath = m.group_path;
                if (!groupPlayingItems.has(groupPath)) {
                    groupPlayingItems.set(groupPath, { patterns: [], melodies: [] });
                }
                groupPlayingItems.get(groupPath).melodies.push(m);
            }
            // Build tracks for groups with playing items
            for (const group of state.groups) {
                const groupPath = group.path;
                const groupId = `group:${groupPath}`;
                const playingItems = groupPlayingItems.get(groupPath);
                if (!playingItems)
                    continue;
                const totalItems = playingItems.patterns.length + playingItems.melodies.length;
                // Count effects for this group
                const groupEffectsCount = state.effects.filter(e => e.group_path === groupPath).length;
                // GROUP ROW (depth 0)
                tracks.push({
                    id: groupId,
                    name: group.name,
                    type: 'group',
                    color: '#858585',
                    clips: [{
                            id: `${groupId}:bar`,
                            name: group.name,
                            type: 'group',
                            startBeat: 0,
                            endBeat: maxLoopBeats,
                            color: '#4a4a4a',
                            active: true,
                        }],
                    muted: group.muted,
                    soloed: group.soloed,
                    active: true,
                    sourceLocation: group.source_location,
                    depth: 0,
                    hasChildren: true,
                    childCount: totalItems,
                    // Mixer controls
                    groupPath: groupPath,
                    amp: group.params['amp'] ?? 1.0,
                    pan: group.params['pan'] ?? 0,
                    effectsCount: groupEffectsCount,
                });
                // PATTERN/MELODY LANES (depth 1) - show each as looping clips tiled to maxLoopBeats
                for (const pattern of playingItems.patterns) {
                    const laneId = `pattern:${pattern.name}`;
                    const patternLoopBeats = pattern.loop_beats || 4;
                    const numReps = Math.ceil(maxLoopBeats / patternLoopBeats);
                    const timelineClips = [];
                    for (let i = 0; i < numReps; i++) {
                        const blockStart = i * patternLoopBeats;
                        const blockEnd = Math.min(blockStart + patternLoopBeats, maxLoopBeats);
                        if (blockStart >= maxLoopBeats)
                            break;
                        timelineClips.push({
                            id: `${laneId}:${i}`,
                            name: pattern.name,
                            type: 'pattern',
                            startBeat: blockStart,
                            endBeat: blockEnd,
                            color: this._getClipColor('pattern'),
                            active: true,
                            sourceLocation: pattern.source_location,
                            stepPattern: pattern.step_pattern,
                        });
                    }
                    tracks.push({
                        id: laneId,
                        name: pattern.name,
                        type: 'pattern',
                        color: this._getClipColor('pattern'),
                        clips: timelineClips,
                        loopBeats: patternLoopBeats,
                        active: true,
                        sourceLocation: pattern.source_location,
                        depth: 1,
                        parentId: groupId,
                        hasChildren: false,
                        stepPattern: pattern.step_pattern,
                    });
                }
                for (const melody of playingItems.melodies) {
                    const laneId = `melody:${melody.name}`;
                    const melodyLoopBeats = melody.loop_beats || 4;
                    const numReps = Math.ceil(maxLoopBeats / melodyLoopBeats);
                    // Convert melody events to visualization format
                    const melodyVizEvents = (melody.events || []).map(e => ({
                        beat: e.beat,
                        midiNote: e.frequency ? Math.round(69 + 12 * Math.log2(e.frequency / 440)) : 60,
                        duration: e.duration || 0.5,
                        velocity: e.velocity || 1.0,
                    }));
                    const timelineClips = [];
                    for (let i = 0; i < numReps; i++) {
                        const blockStart = i * melodyLoopBeats;
                        const blockEnd = Math.min(blockStart + melodyLoopBeats, maxLoopBeats);
                        if (blockStart >= maxLoopBeats)
                            break;
                        timelineClips.push({
                            id: `${laneId}:${i}`,
                            name: melody.name,
                            type: 'melody',
                            startBeat: blockStart,
                            endBeat: blockEnd,
                            color: this._getClipColor('melody'),
                            active: true,
                            sourceLocation: melody.source_location,
                            melodyEvents: melodyVizEvents,
                        });
                    }
                    tracks.push({
                        id: laneId,
                        name: melody.name,
                        type: 'melody',
                        color: this._getClipColor('melody'),
                        clips: timelineClips,
                        loopBeats: melodyLoopBeats,
                        active: true,
                        sourceLocation: melody.source_location,
                        depth: 1,
                        parentId: groupId,
                        hasChildren: false,
                        melodyEvents: melodyVizEvents,
                    });
                }
            }
        }
        else if (tracks.length === 0) {
            // No active sequences and no playing patterns - show all groups with their defined sequences
            for (const group of state.groups) {
                const groupPath = group.path;
                const groupId = `group:${groupPath}`;
                // Find sequences for this group
                const groupSeqs = [];
                for (const seq of state.sequences) {
                    const seqGroupPath = findGroupForSequence(seq);
                    if (seqGroupPath === groupPath) {
                        groupSeqs.push(seq);
                    }
                }
                if (groupSeqs.length === 0)
                    continue;
                // Count effects for this group
                const groupEffectsCount = state.effects.filter(e => e.group_path === groupPath).length;
                tracks.push({
                    id: groupId,
                    name: group.name,
                    type: 'group',
                    color: '#858585',
                    clips: [],
                    muted: group.muted,
                    soloed: group.soloed,
                    active: false,
                    sourceLocation: group.source_location,
                    depth: 0,
                    hasChildren: true,
                    childCount: groupSeqs.length,
                    // Mixer controls
                    groupPath: groupPath,
                    amp: group.params['amp'] ?? 1.0,
                    pan: group.params['pan'] ?? 0,
                    effectsCount: groupEffectsCount,
                });
                for (const seq of groupSeqs) {
                    tracks.push({
                        id: `seq:${seq.name}`,
                        name: seq.name,
                        type: 'sequence',
                        color: '#569cd6',
                        clips: [],
                        loopBeats: seq.loop_beats,
                        active: false,
                        sourceLocation: seq.source_location,
                        depth: 1,
                        parentId: groupId,
                        hasChildren: seq.clips.length > 0,
                        childCount: seq.clips.length,
                    });
                }
            }
        }
        return {
            tracks,
            transport: state.transport,
            bpm: state.transport.bpm,
            timeSignature: state.transport.time_signature,
            maxLoopBeats,
        };
    }
    _getClipColor(type) {
        switch (type) {
            case 'pattern':
                return '#9bbb59';
            case 'melody':
                return '#d19a66';
            case 'fade':
                return '#c586c0';
            case 'sequence':
                return '#569cd6';
            default:
                return '#858585';
        }
    }
    _getClipSourceLocation(clip, state) {
        switch (clip.type) {
            case 'pattern':
                return state.patterns.find((p) => p.name === clip.name)?.source_location;
            case 'melody':
                return state.melodies.find((m) => m.name === clip.name)?.source_location;
            case 'sequence':
                return state.sequences.find((s) => s.name === clip.name)?.source_location;
            default:
                return undefined;
        }
    }
    async _handleMessage(message) {
        switch (message.command) {
            case 'goToSource':
                const location = message.sourceLocation;
                if (location?.file && location?.line) {
                    vscode.commands.executeCommand('vibelang.goToSource', location);
                }
                break;
            case 'toggleTransport':
                vscode.commands.executeCommand('vibelang.toggleTransport');
                break;
            case 'stopTransport':
                vscode.commands.executeCommand('vibelang.stopTransport');
                break;
            case 'seek':
                await this._store.runtime.seekTransport(message.beat);
                break;
            case 'startPattern':
                await this._store.runtime.startPattern(message.name);
                break;
            case 'stopPattern':
                await this._store.runtime.stopPattern(message.name);
                break;
            case 'startMelody':
                await this._store.runtime.startMelody(message.name);
                break;
            case 'stopMelody':
                await this._store.runtime.stopMelody(message.name);
                break;
            case 'startSequence':
                await this._store.runtime.startSequence(message.name);
                break;
            case 'stopSequence':
                await this._store.runtime.stopSequence(message.name);
                break;
            case 'muteTrack':
                const track = message.trackId;
                if (track.startsWith('group:')) {
                    const path = track.substring(6);
                    const group = this._store.getGroup(path);
                    if (group?.muted) {
                        await this._store.runtime.unmuteGroup(path);
                    }
                    else {
                        await this._store.runtime.muteGroup(path);
                    }
                }
                break;
            case 'soloTrack':
                const trackId = message.trackId;
                if (trackId.startsWith('group:')) {
                    const path = trackId.substring(6);
                    const group = this._store.getGroup(path);
                    if (group?.soloed) {
                        await this._store.runtime.unsoloGroup(path);
                    }
                    else {
                        await this._store.runtime.soloGroup(path);
                    }
                }
                break;
            // Mini-mixer controls
            case 'setGroupAmp':
                await this._store.runtime.setGroupParam(message.groupPath, 'amp', message.value);
                break;
            case 'setGroupPan':
                await this._store.runtime.setGroupParam(message.groupPath, 'pan', message.value);
                break;
            // Selection and navigation
            case 'selectTrack':
                this._handleTrackSelection(message.trackId, message.trackType);
                break;
            case 'openEffectRack':
                vscode.commands.executeCommand('vibelang.openEffectRack', message.groupPath);
                break;
            case 'openPatternEditor':
                vscode.commands.executeCommand('vibelang.openPatternEditor', message.name);
                break;
            case 'openMelodyEditor':
                vscode.commands.executeCommand('vibelang.openMelodyEditor', message.name);
                break;
            case 'openInspector':
                this._handleTrackSelection(message.trackId, message.trackType);
                vscode.commands.executeCommand('vibelang.openInspector');
                break;
            // Automation lane commands
            case 'addAutomationLane':
                this._addAutomationLane(message.target);
                break;
            case 'removeAutomationLane':
                this._removeAutomationLane(message.laneId);
                break;
            case 'toggleAutomationLane':
                this._toggleAutomationLaneVisibility(message.laneId);
                break;
            case 'addAutomationPoint':
                this._addAutomationPoint(message.laneId, message.beat, message.value);
                break;
            case 'updateAutomationPoint':
                this._updateAutomationPoint(message.laneId, message.pointId, message.beat, message.value, message.curveType);
                break;
            case 'removeAutomationPoint':
                this._removeAutomationPoint(message.laneId, message.pointId);
                break;
            case 'setAutomationCurveType':
                this._setPointCurveType(message.laneId, message.pointId, message.curveType);
                break;
            case 'generateAutomationCode':
                this._generateAndInsertAutomationCode(message.laneId);
                break;
            case 'clearAutomationLane':
                this._clearAutomationLane(message.laneId);
                break;
            case 'showWarning':
                vscode.window.showWarningMessage(message.message);
                break;
        }
    }
    // Automation lane management methods
    _addAutomationLane(target) {
        // Check if lane already exists
        const existing = this._automationLanes.find(lane => lane.target.type === target.type &&
            lane.target.name === target.name &&
            lane.target.param === target.param);
        if (existing) {
            existing.visible = true;
            this._sendAutomationUpdate();
            return;
        }
        // Determine min/max values based on target
        let minValue = 0;
        let maxValue = 1;
        if (target.param === 'amp' || target.param === 'gain') {
            minValue = 0;
            maxValue = 1;
        }
        else if (target.param === 'pan') {
            minValue = -1;
            maxValue = 1;
        }
        else if (target.param === 'freq' || target.param === 'cutoff') {
            minValue = 20;
            maxValue = 20000;
        }
        const lane = (0, automationTypes_1.createAutomationLane)(target, minValue, maxValue);
        this._automationLanes.push(lane);
        this._sendAutomationUpdate();
    }
    _removeAutomationLane(laneId) {
        this._automationLanes = this._automationLanes.filter(l => l.id !== laneId);
        this._sendAutomationUpdate();
    }
    _toggleAutomationLaneVisibility(laneId) {
        const lane = this._automationLanes.find(l => l.id === laneId);
        if (lane) {
            lane.visible = !lane.visible;
            this._sendAutomationUpdate();
        }
    }
    _addAutomationPoint(laneId, beat, value) {
        const lane = this._automationLanes.find(l => l.id === laneId);
        if (lane) {
            const point = (0, automationTypes_1.createAutomationPoint)(beat, value, 'smooth');
            lane.points.push(point);
            lane.points.sort((a, b) => a.beat - b.beat);
            this._sendAutomationUpdate();
        }
    }
    _updateAutomationPoint(laneId, pointId, beat, value, curveType) {
        const lane = this._automationLanes.find(l => l.id === laneId);
        if (lane) {
            const point = lane.points.find(p => p.id === pointId);
            if (point) {
                point.beat = beat;
                point.value = Math.max(0, Math.min(1, value));
                if (curveType) {
                    point.curveType = curveType;
                }
                lane.points.sort((a, b) => a.beat - b.beat);
                this._sendAutomationUpdate();
            }
        }
    }
    _removeAutomationPoint(laneId, pointId) {
        const lane = this._automationLanes.find(l => l.id === laneId);
        if (lane) {
            lane.points = lane.points.filter(p => p.id !== pointId);
            this._sendAutomationUpdate();
        }
    }
    _setPointCurveType(laneId, pointId, curveType) {
        const lane = this._automationLanes.find(l => l.id === laneId);
        if (lane) {
            const point = lane.points.find(p => p.id === pointId);
            if (point) {
                point.curveType = curveType;
                this._sendAutomationUpdate();
            }
        }
    }
    _clearAutomationLane(laneId) {
        const lane = this._automationLanes.find(l => l.id === laneId);
        if (lane) {
            lane.points = [];
            this._sendAutomationUpdate();
        }
    }
    async _generateAndInsertAutomationCode(laneId) {
        const lane = this._automationLanes.find(l => l.id === laneId);
        if (!lane || lane.points.length < 2) {
            vscode.window.showWarningMessage('Need at least 2 points to generate automation code');
            return;
        }
        const code = (0, automationTypes_1.generateFadeCode)(lane);
        if (!code)
            return;
        // Ask where to insert
        const result = await vscode.window.showQuickPick([
            { label: 'Copy to Clipboard', description: 'Copy the generated code to clipboard' },
            { label: 'Insert at Cursor', description: 'Insert at current cursor position in active editor' },
            { label: 'Create New File', description: 'Create a new .vibe file with the automation code' },
        ], { placeHolder: 'Where to put the generated fade() code?' });
        if (!result)
            return;
        if (result.label === 'Copy to Clipboard') {
            await vscode.env.clipboard.writeText(code);
            vscode.window.showInformationMessage('Automation code copied to clipboard');
        }
        else if (result.label === 'Insert at Cursor') {
            const editor = vscode.window.activeTextEditor;
            if (editor) {
                await editor.edit(edit => {
                    edit.insert(editor.selection.active, code + '\n');
                });
            }
            else {
                await vscode.env.clipboard.writeText(code);
                vscode.window.showInformationMessage('No active editor - copied to clipboard instead');
            }
        }
        else if (result.label === 'Create New File') {
            const doc = await vscode.workspace.openTextDocument({
                content: `// Generated automation for ${lane.target.name}.${lane.target.param}\n\n${code}\n`,
                language: 'vibe'
            });
            await vscode.window.showTextDocument(doc);
        }
    }
    _sendAutomationUpdate() {
        this._panel.webview.postMessage({
            type: 'automationUpdate',
            data: this._automationLanes,
        });
    }
    _handleTrackSelection(trackId, trackType) {
        // Parse the track ID to determine what to select
        if (trackId.startsWith('group:')) {
            const path = trackId.substring(6);
            this._store.selectGroup(path);
        }
        else if (trackId.startsWith('pattern:')) {
            const name = trackId.substring(8);
            this._store.selectPattern(name);
        }
        else if (trackId.startsWith('melody:')) {
            const name = trackId.substring(7);
            this._store.selectMelody(name);
        }
        else if (trackId.startsWith('seq:')) {
            const name = trackId.substring(4);
            this._store.selectSequence(name);
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
    <title>Arrangement</title>
    <style>
        :root {
            --bg-primary: #1a1a1a;
            --bg-secondary: #232323;
            --bg-tertiary: #2d2d2d;
            --bg-track: #1e1e1e;
            --bg-track-alt: #222222;
            --text-primary: #d4d4d4;
            --text-secondary: #858585;
            --text-muted: #5a5a5a;
            --accent-green: #9bbb59;
            --accent-orange: #d19a66;
            --accent-blue: #569cd6;
            --accent-purple: #c586c0;
            --accent-red: #d16969;
            --border: #3c3c3c;
            --grid-line: #2a2a2a;
            --grid-bar: #3a3a3a;
            --playhead: #ff6b6b;
            --clip-pattern: #4a6b3a;
            --clip-melody: #6b5a3a;
            --clip-sequence: #3a5a6b;
            --clip-fade: #5a3a6b;
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
            height: 40px;
            flex-shrink: 0;
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

        .btn {
            padding: 4px 10px;
            border: 1px solid var(--border);
            border-radius: 3px;
            background: var(--bg-tertiary);
            color: var(--text-primary);
            cursor: pointer;
            font-size: 11px;
            transition: all 0.1s ease;
            display: flex;
            align-items: center;
            gap: 4px;
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

        .btn-icon {
            width: 28px;
            height: 28px;
            padding: 0;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 14px;
        }

        .transport-time {
            font-family: 'SF Mono', Monaco, 'Courier New', monospace;
            font-size: 16px;
            color: var(--accent-green);
            background: var(--bg-primary);
            padding: 4px 12px;
            border-radius: 3px;
            border: 1px solid var(--border);
            min-width: 100px;
            text-align: center;
        }

        .bpm-display {
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 12px;
            color: var(--text-secondary);
        }

        .zoom-control {
            display: flex;
            align-items: center;
            gap: 4px;
        }

        .zoom-slider {
            width: 80px;
            height: 4px;
            -webkit-appearance: none;
            background: var(--bg-tertiary);
            border-radius: 2px;
        }

        .zoom-slider::-webkit-slider-thumb {
            -webkit-appearance: none;
            width: 12px;
            height: 12px;
            background: var(--text-secondary);
            border-radius: 50%;
            cursor: pointer;
        }

        /* Main Container */
        .main-container {
            display: flex;
            flex: 1;
            overflow: hidden;
        }

        /* Track Headers */
        .track-headers {
            width: 180px;
            min-width: 180px;
            background: var(--bg-secondary);
            border-right: 1px solid var(--border);
            display: flex;
            flex-direction: column;
            overflow: hidden;
        }

        .ruler-spacer {
            height: 30px;
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border);
        }

        .track-headers-scroll {
            flex: 1;
            overflow-y: auto;
            overflow-x: hidden;
            /* Hide scrollbar but allow programmatic scrolling */
            scrollbar-width: none;
            -ms-overflow-style: none;
        }
        .track-headers-scroll::-webkit-scrollbar {
            display: none;
        }

        .track-header {
            height: 48px;
            padding: 0 8px;
            display: flex;
            align-items: center;
            gap: 8px;
            border-bottom: 1px solid var(--border);
            cursor: pointer;
            transition: background 0.1s ease;
            position: relative;
        }

        .track-header:hover {
            background: var(--bg-tertiary);
        }

        .track-header.active {
            background: rgba(155, 187, 89, 0.1);
        }

        .track-header.muted {
            opacity: 0.4;
        }

        /* Hierarchy indentation */
        .track-header.depth-0 {
            background: var(--bg-secondary);
            font-weight: 600;
        }

        .track-header.depth-1 {
            padding-left: 20px;
            background: var(--bg-track);
        }

        .track-header.depth-2 {
            padding-left: 36px;
            background: var(--bg-track-alt);
        }

        /* Hierarchy connector lines */
        .track-header.depth-1::before,
        .track-header.depth-2::before {
            content: '';
            position: absolute;
            left: 10px;
            top: 0;
            bottom: 0;
            width: 1px;
            background: var(--border);
        }

        .track-header.depth-1::after,
        .track-header.depth-2::after {
            content: '';
            position: absolute;
            left: 10px;
            top: 50%;
            width: 6px;
            height: 1px;
            background: var(--border);
        }

        .track-header.depth-2::before {
            left: 26px;
        }

        .track-header.depth-2::after {
            left: 26px;
        }

        /* Last child - connector ends at middle */
        .track-header.depth-1.last-child::before,
        .track-header.depth-2.last-child::before {
            bottom: 50%;
        }

        /* Track expand/collapse arrow */
        .track-expand {
            width: 20px;
            height: 20px;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 10px;
            color: var(--text-muted);
            cursor: pointer;
            border-radius: 3px;
            flex-shrink: 0;
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            transition: all 0.15s ease;
        }

        .track-expand:hover {
            background: var(--accent-blue);
            color: white;
            border-color: var(--accent-blue);
        }

        .track-expand.expanded {
            transform: rotate(90deg);
        }

        /* Child count badge */
        .child-count {
            font-size: 9px;
            color: var(--text-muted);
            background: var(--bg-tertiary);
            padding: 1px 5px;
            border-radius: 8px;
            margin-left: 4px;
        }

        .track-color {
            width: 4px;
            height: 32px;
            border-radius: 2px;
            flex-shrink: 0;
        }

        .track-info {
            flex: 1;
            min-width: 0;
        }

        .track-name {
            font-size: 11px;
            font-weight: 500;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }

        .track-header.depth-0 .track-name {
            font-weight: 600;
            font-size: 12px;
            color: var(--text-primary);
        }

        .track-type {
            font-size: 9px;
            color: var(--text-muted);
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        /* Type badges with colors */
        .track-type.type-group {
            color: #858585;
        }

        .track-type.type-sequence {
            color: var(--accent-blue);
        }

        .track-type.type-pattern {
            color: var(--accent-green);
        }

        .track-type.type-melody {
            color: var(--accent-orange);
        }

        .track-controls {
            display: flex;
            gap: 2px;
        }

        .track-btn {
            width: 18px;
            height: 18px;
            border: none;
            border-radius: 2px;
            font-size: 9px;
            font-weight: 600;
            cursor: pointer;
            background: var(--bg-tertiary);
            color: var(--text-muted);
            transition: all 0.1s ease;
        }

        .track-btn:hover {
            color: var(--text-primary);
        }

        .track-btn.mute.active {
            background: var(--accent-red);
            color: #fff;
        }

        .track-btn.solo.active {
            background: var(--accent-orange);
            color: #000;
        }

        /* Mini Mixer Controls */
        .track-mixer {
            display: flex;
            align-items: center;
            gap: 6px;
            margin-right: 4px;
        }

        .mini-fader {
            display: flex;
            align-items: center;
            gap: 3px;
        }

        .mini-fader-label {
            font-size: 8px;
            color: var(--text-muted);
            width: 12px;
            text-align: right;
        }

        .mini-fader input[type="range"] {
            width: 50px;
            height: 4px;
            -webkit-appearance: none;
            appearance: none;
            background: var(--bg-tertiary);
            border-radius: 2px;
            cursor: pointer;
        }

        .mini-fader input[type="range"]::-webkit-slider-thumb {
            -webkit-appearance: none;
            appearance: none;
            width: 10px;
            height: 10px;
            border-radius: 50%;
            background: var(--text-secondary);
            cursor: pointer;
            transition: background 0.1s;
        }

        .mini-fader input[type="range"]::-webkit-slider-thumb:hover {
            background: var(--accent-blue);
        }

        .mini-fader input[type="range"]:active::-webkit-slider-thumb {
            background: var(--accent-blue);
            transform: scale(1.2);
        }

        .mini-fader-value {
            font-size: 8px;
            color: var(--text-muted);
            width: 24px;
            text-align: left;
            font-family: monospace;
        }

        .mini-pan {
            display: flex;
            align-items: center;
            gap: 3px;
        }

        .pan-knob {
            width: 18px;
            height: 18px;
            border-radius: 50%;
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            position: relative;
            cursor: pointer;
        }

        .pan-knob::after {
            content: '';
            position: absolute;
            top: 2px;
            left: 50%;
            width: 2px;
            height: 6px;
            background: var(--text-secondary);
            transform-origin: center 7px;
            transform: translateX(-50%) rotate(var(--pan-rotation, 0deg));
            border-radius: 1px;
        }

        .pan-knob:hover {
            border-color: var(--accent-blue);
        }

        .pan-knob:hover::after {
            background: var(--accent-blue);
        }

        .pan-value {
            font-size: 8px;
            color: var(--text-muted);
            width: 16px;
            text-align: center;
            font-family: monospace;
        }

        /* Activity Indicator */
        .activity-indicator {
            width: 6px;
            height: 6px;
            border-radius: 50%;
            background: var(--bg-tertiary);
            transition: all 0.15s ease;
            flex-shrink: 0;
        }

        .activity-indicator.active {
            background: var(--accent-green);
            box-shadow: 0 0 6px var(--accent-green);
            animation: pulse 0.8s ease-in-out infinite;
        }

        @keyframes pulse {
            0%, 100% { opacity: 1; transform: scale(1); }
            50% { opacity: 0.7; transform: scale(1.2); }
        }

        /* Effects Chain Indicator */
        .effects-chain {
            display: flex;
            align-items: center;
            gap: 2px;
            padding: 2px 4px;
            background: var(--bg-tertiary);
            border-radius: 3px;
            font-size: 8px;
            color: var(--text-muted);
            cursor: pointer;
            transition: all 0.1s;
        }

        .effects-chain:hover {
            background: var(--accent-purple);
            color: white;
        }

        .effects-chain .fx-icon {
            font-size: 9px;
        }

        .effects-chain .fx-count {
            font-weight: 600;
        }

        /* Context Menu */
        .context-menu {
            position: fixed;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 4px;
            padding: 4px 0;
            min-width: 160px;
            box-shadow: 0 4px 12px rgba(0,0,0,0.4);
            z-index: 1000;
            font-size: 12px;
        }

        .context-menu-item {
            padding: 6px 12px;
            cursor: pointer;
            display: flex;
            align-items: center;
            gap: 8px;
            color: var(--text-primary);
        }

        .context-menu-item:hover {
            background: var(--accent-blue);
            color: white;
        }

        .context-menu-item.disabled {
            color: var(--text-muted);
            cursor: default;
        }

        .context-menu-item.disabled:hover {
            background: transparent;
            color: var(--text-muted);
        }

        .context-menu-separator {
            height: 1px;
            background: var(--border);
            margin: 4px 0;
        }

        .context-menu-item .shortcut {
            margin-left: auto;
            color: var(--text-muted);
            font-size: 10px;
        }

        /* Selection Highlight */
        .track-header.selected {
            background: rgba(86, 156, 214, 0.2) !important;
            border-left: 3px solid var(--accent-blue);
        }

        .clip-selected {
            box-shadow: 0 0 0 2px var(--accent-blue) !important;
        }

        /* Details Panel */
        .details-panel {
            position: absolute;
            right: 10px;
            top: 50px;
            width: 280px;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 6px;
            box-shadow: 0 4px 16px rgba(0,0,0,0.4);
            z-index: 100;
            display: none;
            overflow: hidden;
        }

        .details-panel.visible {
            display: block;
        }

        .details-panel-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 10px 12px;
            background: var(--bg-tertiary);
            border-bottom: 1px solid var(--border);
        }

        .details-panel-title {
            display: flex;
            align-items: center;
            gap: 8px;
            font-weight: 600;
            font-size: 12px;
        }

        .details-panel-type {
            display: inline-block;
            padding: 2px 6px;
            border-radius: 3px;
            font-size: 9px;
            text-transform: uppercase;
            font-weight: 600;
        }

        .details-panel-type.pattern { background: var(--accent-green); color: #000; }
        .details-panel-type.melody { background: var(--accent-orange); color: #000; }
        .details-panel-type.sequence { background: var(--accent-blue); color: #fff; }
        .details-panel-type.group { background: var(--text-secondary); color: #000; }

        .details-panel-close {
            width: 20px;
            height: 20px;
            border: none;
            background: transparent;
            color: var(--text-muted);
            cursor: pointer;
            font-size: 14px;
            border-radius: 3px;
            display: flex;
            align-items: center;
            justify-content: center;
        }

        .details-panel-close:hover {
            background: var(--accent-red);
            color: white;
        }

        .details-panel-content {
            padding: 12px;
            max-height: 400px;
            overflow-y: auto;
        }

        .details-section {
            margin-bottom: 14px;
        }

        .details-section:last-child {
            margin-bottom: 0;
        }

        .details-section-title {
            font-size: 9px;
            text-transform: uppercase;
            color: var(--text-muted);
            margin-bottom: 6px;
            letter-spacing: 0.5px;
        }

        .details-row {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 4px 0;
            font-size: 11px;
        }

        .details-row .label {
            color: var(--text-secondary);
        }

        .details-row .value {
            font-family: 'SF Mono', Monaco, monospace;
            color: var(--text-primary);
        }

        .details-pattern-preview {
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 11px;
            background: var(--bg-primary);
            padding: 8px;
            border-radius: 4px;
            word-break: break-all;
            line-height: 1.4;
        }

        .details-pattern-preview .hit { color: var(--accent-green); font-weight: 600; }
        .details-pattern-preview .rest { color: var(--text-muted); }
        .details-pattern-preview .bar { color: var(--accent-purple); }

        .details-actions {
            display: flex;
            gap: 8px;
            flex-wrap: wrap;
        }

        .details-action-btn {
            flex: 1;
            min-width: 80px;
            padding: 6px 10px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: var(--bg-tertiary);
            color: var(--text-primary);
            font-size: 11px;
            cursor: pointer;
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 4px;
            transition: all 0.1s ease;
        }

        .details-action-btn:hover {
            background: var(--accent-blue);
            color: white;
            border-color: var(--accent-blue);
        }

        .details-action-btn.primary {
            background: var(--accent-green);
            color: #000;
            border-color: var(--accent-green);
        }

        .details-action-btn.primary:hover {
            background: #b0d468;
        }

        .details-source-link {
            color: var(--accent-blue);
            cursor: pointer;
            text-decoration: underline;
        }

        .details-source-link:hover {
            color: #7cc0f0;
        }

        /* Clip Tooltip */
        .clip-tooltip {
            position: fixed;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 4px;
            padding: 8px 10px;
            font-size: 11px;
            max-width: 250px;
            box-shadow: 0 4px 12px rgba(0,0,0,0.4);
            z-index: 1000;
            pointer-events: none;
            opacity: 0;
            transition: opacity 0.15s;
        }

        .clip-tooltip.visible {
            opacity: 1;
        }

        .clip-tooltip-title {
            font-weight: 600;
            color: var(--text-primary);
            margin-bottom: 4px;
        }

        .clip-tooltip-type {
            display: inline-block;
            padding: 1px 4px;
            border-radius: 2px;
            font-size: 9px;
            text-transform: uppercase;
            margin-left: 6px;
            font-weight: 500;
        }

        .clip-tooltip-type.pattern { background: var(--accent-green); color: #000; }
        .clip-tooltip-type.melody { background: var(--accent-orange); color: #000; }
        .clip-tooltip-type.sequence { background: var(--accent-blue); color: #fff; }
        .clip-tooltip-type.fade { background: var(--accent-purple); color: #fff; }

        .clip-tooltip-info {
            color: var(--text-secondary);
            margin-top: 4px;
            line-height: 1.4;
        }

        .clip-tooltip-info div {
            display: flex;
            justify-content: space-between;
            gap: 12px;
        }

        .clip-tooltip-info .label {
            color: var(--text-muted);
        }

        .clip-tooltip-info .value {
            font-family: monospace;
            color: var(--text-primary);
        }

        /* Inline Automation Lane */
        .track-header.automation-lane {
            height: 36px;
            background: var(--bg-track-alt);
            border-left: 2px solid var(--accent-purple);
        }

        .track-header.automation-lane .track-info {
            padding-left: 4px;
        }

        .track-header.automation-lane .track-name {
            font-size: 10px;
            color: var(--text-secondary);
        }

        .automation-lane-preview {
            flex: 1;
            height: 24px;
            margin: 0 8px;
            background: rgba(197, 134, 192, 0.1);
            border-radius: 3px;
            position: relative;
            overflow: hidden;
        }

        .automation-lane-preview canvas {
            width: 100%;
            height: 100%;
        }

        .automation-lane-controls {
            display: flex;
            gap: 4px;
            align-items: center;
        }

        .automation-lane-btn {
            width: 16px;
            height: 16px;
            border: none;
            border-radius: 2px;
            font-size: 10px;
            cursor: pointer;
            background: var(--bg-tertiary);
            color: var(--text-muted);
            display: flex;
            align-items: center;
            justify-content: center;
        }

        .automation-lane-btn:hover {
            background: var(--accent-purple);
            color: white;
        }

        .add-automation-inline {
            display: flex;
            align-items: center;
            gap: 4px;
            padding: 2px 6px;
            margin-left: 4px;
            font-size: 9px;
            color: var(--text-muted);
            background: transparent;
            border: 1px dashed var(--border);
            border-radius: 3px;
            cursor: pointer;
            transition: all 0.15s;
        }

        .add-automation-inline:hover {
            border-color: var(--accent-purple);
            color: var(--accent-purple);
            background: rgba(197, 134, 192, 0.1);
        }

        /* Fun personality touches */
        .vibe-check {
            position: absolute;
            bottom: 8px;
            right: 8px;
            font-size: 9px;
            color: var(--text-muted);
            opacity: 0.5;
            pointer-events: none;
            transition: opacity 0.3s;
        }

        .main-container:hover .vibe-check {
            opacity: 1;
        }

        /* Vibe Status */
        .vibe-status {
            display: flex;
            align-items: center;
            gap: 4px;
            padding: 2px 8px;
            border-radius: 10px;
            background: var(--bg-tertiary);
            font-size: 10px;
            color: var(--text-muted);
            transition: all 0.3s ease;
        }

        .vibe-status.vibing {
            background: linear-gradient(135deg, rgba(155, 187, 89, 0.3), rgba(86, 156, 214, 0.3));
            color: var(--text-primary);
        }

        .vibe-status.fire {
            background: linear-gradient(135deg, rgba(255, 100, 50, 0.4), rgba(255, 200, 50, 0.3));
            color: #fff;
            animation: vibeGlow 1s ease-in-out infinite alternate;
        }

        @keyframes vibeGlow {
            from { box-shadow: 0 0 5px rgba(255, 100, 50, 0.5); }
            to { box-shadow: 0 0 15px rgba(255, 100, 50, 0.8); }
        }

        .vibe-emoji {
            font-size: 12px;
        }

        .vibe-text {
            font-weight: 500;
        }

        /* Groove indicator */
        .groove-meter {
            display: flex;
            align-items: center;
            gap: 4px;
            font-size: 9px;
            color: var(--text-muted);
        }

        .groove-bars {
            display: flex;
            gap: 1px;
            height: 12px;
            align-items: flex-end;
        }

        .groove-bar {
            width: 3px;
            background: var(--accent-green);
            border-radius: 1px;
            transition: height 0.1s ease;
        }

        /* Fire mode for when things get intense */
        .track-header.on-fire {
            animation: fireGlow 0.5s ease-in-out infinite alternate;
        }

        @keyframes fireGlow {
            from { box-shadow: inset 0 0 10px rgba(255, 100, 50, 0.3); }
            to { box-shadow: inset 0 0 20px rgba(255, 100, 50, 0.5); }
        }

        /* Bounce animation for active clips */
        .clip-bounce {
            animation: clipBounce 0.3s ease;
        }

        @keyframes clipBounce {
            0%, 100% { transform: scale(1); }
            50% { transform: scale(1.02); }
        }

        /* Waveform shimmer effect */
        .shimmer {
            background: linear-gradient(90deg, transparent, rgba(255,255,255,0.1), transparent);
            background-size: 200% 100%;
            animation: shimmer 2s infinite;
        }

        @keyframes shimmer {
            0% { background-position: -200% 0; }
            100% { background-position: 200% 0; }
        }

        /* Timeline Area */
        .timeline-area {
            flex: 1;
            display: flex;
            flex-direction: column;
            overflow: hidden;
            position: relative;
        }

        /* Ruler */
        .ruler {
            height: 30px;
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border);
            position: relative;
            overflow: hidden;
            flex-shrink: 0;
        }

        .ruler-canvas {
            position: absolute;
            top: 0;
            left: 0;
        }

        /* Tracks Container */
        .tracks-container {
            flex: 1;
            position: relative;
            overflow: auto;
        }

        .tracks-canvas {
            position: absolute;
            top: 0;
            left: 0;
        }

        /* Playhead */
        .playhead {
            position: absolute;
            top: 0;
            left: 0;
            width: 2px;
            background: linear-gradient(to bottom, var(--playhead), #ff4444);
            pointer-events: none;
            z-index: 100;
            box-shadow: 0 0 12px rgba(255, 107, 107, 0.8),
                        0 0 24px rgba(255, 107, 107, 0.4),
                        0 0 36px rgba(255, 107, 107, 0.2);
            will-change: transform;
            transform: translateX(0px);
        }

        .playhead::before {
            content: '';
            position: absolute;
            top: 0;
            left: -6px;
            width: 14px;
            height: 14px;
            background: var(--playhead);
            border-radius: 2px 2px 0 0;
            clip-path: polygon(50% 100%, 0 0, 100% 0);
            box-shadow: 0 0 8px rgba(255, 107, 107, 0.8);
        }

        .playhead::after {
            content: '';
            position: absolute;
            top: 0;
            left: -1px;
            width: 4px;
            height: 100%;
            background: linear-gradient(to right,
                rgba(255, 107, 107, 0.3),
                rgba(255, 107, 107, 0),
                rgba(255, 107, 107, 0));
        }

        /* Empty State */
        .empty-state {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            height: 100%;
            color: var(--text-secondary);
            text-align: center;
            padding: 40px;
        }

        .empty-icon {
            font-size: 64px;
            margin-bottom: 20px;
            opacity: 0.3;
        }

        .empty-state h2 {
            font-size: 18px;
            font-weight: 500;
            margin-bottom: 8px;
            color: var(--text-primary);
        }

        .empty-state p {
            max-width: 400px;
            line-height: 1.5;
        }

        /* Scrollbar styling */
        ::-webkit-scrollbar {
            width: 10px;
            height: 10px;
        }

        ::-webkit-scrollbar-track {
            background: var(--bg-primary);
        }

        ::-webkit-scrollbar-thumb {
            background: var(--bg-tertiary);
            border-radius: 5px;
        }

        ::-webkit-scrollbar-thumb:hover {
            background: #404040;
        }

        ::-webkit-scrollbar-corner {
            background: var(--bg-primary);
        }

        /* Clip Tooltip */
        .clip-tooltip {
            position: fixed;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 4px;
            padding: 8px 12px;
            font-size: 11px;
            pointer-events: none;
            z-index: 1000;
            box-shadow: 0 4px 12px rgba(0,0,0,0.3);
            display: none;
        }

        .clip-tooltip.visible {
            display: block;
        }

        .clip-tooltip-name {
            font-weight: 600;
            margin-bottom: 4px;
        }

        .clip-tooltip-info {
            color: var(--text-secondary);
        }

        /* Automation Panel */
        .automation-panel {
            background: var(--bg-secondary);
            border-top: 1px solid var(--border);
            max-height: 300px;
            display: flex;
            flex-direction: column;
            transition: max-height 0.2s ease;
        }

        .automation-panel.collapsed {
            max-height: 32px;
        }

        .automation-header {
            display: flex;
            align-items: center;
            padding: 6px 12px;
            gap: 8px;
            background: var(--bg-tertiary);
            border-bottom: 1px solid var(--border);
            cursor: pointer;
        }

        .automation-header:hover {
            background: #353535;
        }

        .automation-header-title {
            font-weight: 600;
            font-size: 11px;
            flex: 1;
        }

        .automation-header-toggle {
            font-size: 10px;
            color: var(--text-muted);
            transition: transform 0.2s ease;
        }

        .automation-panel:not(.collapsed) .automation-header-toggle {
            transform: rotate(180deg);
        }

        .automation-content {
            flex: 1;
            overflow: auto;
            display: flex;
        }

        .automation-panel.collapsed .automation-content {
            display: none;
        }

        .automation-lanes-list {
            width: 180px;
            min-width: 180px;
            background: var(--bg-secondary);
            border-right: 1px solid var(--border);
            overflow-y: auto;
        }

        .automation-lane-header {
            padding: 8px 10px;
            display: flex;
            align-items: center;
            gap: 6px;
            border-bottom: 1px solid var(--border);
            cursor: pointer;
            font-size: 11px;
        }

        .automation-lane-header:hover {
            background: var(--bg-tertiary);
        }

        .automation-lane-header.selected {
            background: rgba(86, 156, 214, 0.2);
            border-left: 2px solid var(--accent-blue);
        }

        .automation-lane-color {
            width: 3px;
            height: 20px;
            border-radius: 1px;
        }

        .automation-lane-info {
            flex: 1;
            min-width: 0;
        }

        .automation-lane-name {
            font-weight: 500;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }

        .automation-lane-target {
            font-size: 9px;
            color: var(--text-muted);
        }

        .automation-lane-actions {
            display: flex;
            gap: 2px;
            opacity: 0;
            transition: opacity 0.1s ease;
        }

        .automation-lane-header:hover .automation-lane-actions {
            opacity: 1;
        }

        .automation-lane-btn {
            width: 18px;
            height: 18px;
            border: none;
            border-radius: 2px;
            background: var(--bg-tertiary);
            color: var(--text-muted);
            cursor: pointer;
            font-size: 10px;
            display: flex;
            align-items: center;
            justify-content: center;
        }

        .automation-lane-btn:hover {
            background: var(--accent-blue);
            color: white;
        }

        .automation-lane-btn.delete:hover {
            background: var(--accent-red);
        }

        .automation-canvas-area {
            flex: 1;
            position: relative;
            overflow: hidden;
        }

        .automation-canvas {
            position: absolute;
            top: 0;
            left: 0;
        }

        .add-automation-btn {
            padding: 8px 10px;
            display: flex;
            align-items: center;
            gap: 6px;
            color: var(--text-muted);
            font-size: 11px;
            cursor: pointer;
            border-bottom: 1px solid var(--border);
        }

        .add-automation-btn:hover {
            background: var(--bg-tertiary);
            color: var(--text-primary);
        }

        /* Automation target picker */
        .automation-picker {
            position: fixed;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 4px;
            box-shadow: 0 4px 16px rgba(0,0,0,0.4);
            z-index: 1000;
            max-height: 300px;
            width: 280px;
            overflow: hidden;
            display: none;
        }

        .automation-picker.visible {
            display: block;
        }

        .automation-picker-header {
            padding: 8px 12px;
            font-weight: 600;
            font-size: 11px;
            border-bottom: 1px solid var(--border);
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        .automation-picker-close {
            cursor: pointer;
            color: var(--text-muted);
        }

        .automation-picker-close:hover {
            color: var(--text-primary);
        }

        .automation-picker-search {
            padding: 8px 12px;
            border-bottom: 1px solid var(--border);
        }

        .automation-picker-search input {
            width: 100%;
            padding: 6px 8px;
            border: 1px solid var(--border);
            border-radius: 3px;
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: 11px;
        }

        .automation-picker-search input:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        .automation-picker-list {
            max-height: 200px;
            overflow-y: auto;
        }

        .automation-picker-item {
            padding: 8px 12px;
            cursor: pointer;
            font-size: 11px;
            display: flex;
            align-items: center;
            gap: 8px;
        }

        .automation-picker-item:hover {
            background: var(--bg-tertiary);
        }

        .automation-picker-item-type {
            font-size: 9px;
            padding: 2px 5px;
            border-radius: 3px;
            text-transform: uppercase;
        }

        .automation-picker-item-type.group {
            background: rgba(133, 133, 133, 0.2);
            color: #858585;
        }

        .automation-picker-item-type.voice {
            background: rgba(155, 187, 89, 0.2);
            color: var(--accent-green);
        }

        .automation-picker-item-type.effect {
            background: rgba(197, 134, 192, 0.2);
            color: var(--accent-purple);
        }

        /* Curve type selector */
        .curve-type-selector {
            position: fixed;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 4px;
            box-shadow: 0 2px 8px rgba(0,0,0,0.3);
            z-index: 1001;
            display: none;
        }

        .curve-type-selector.visible {
            display: block;
        }

        .curve-type-option {
            padding: 6px 12px;
            cursor: pointer;
            font-size: 11px;
            display: flex;
            align-items: center;
            gap: 8px;
        }

        .curve-type-option:hover {
            background: var(--bg-tertiary);
        }

        .curve-type-option.selected {
            background: rgba(86, 156, 214, 0.2);
        }

        .curve-type-icon {
            width: 20px;
            height: 12px;
            position: relative;
        }

        .curve-type-icon svg {
            width: 100%;
            height: 100%;
        }
    </style>
</head>
<body>
    <div class="toolbar">
        <div class="toolbar-group">
            <button class="btn btn-icon" id="playBtn" title="Let's go! (Space)">▶</button>
            <button class="btn btn-icon" id="stopBtn" title="Bring it back (⏹)">⏹</button>
        </div>

        <div class="toolbar-divider"></div>

        <div class="transport-time" id="transportTime">1:1:1</div>
        <div class="bpm-display"><span id="bpmValue">120.0</span> BPM</div>

        <div class="toolbar-divider"></div>

        <div class="zoom-control">
            <span style="font-size: 10px; color: var(--text-muted);">−</span>
            <input type="range" class="zoom-slider" id="zoomSlider" min="10" max="100" value="40">
            <span style="font-size: 10px; color: var(--text-muted);">+</span>
        </div>

        <div style="flex: 1;"></div>

        <!-- Groove Meter - shows activity level -->
        <div class="groove-meter" id="grooveMeter" title="Groove Level">
            <span id="grooveLabel">chill</span>
            <div class="groove-bars">
                <div class="groove-bar" style="height: 3px;"></div>
                <div class="groove-bar" style="height: 5px;"></div>
                <div class="groove-bar" style="height: 4px;"></div>
                <div class="groove-bar" style="height: 6px;"></div>
                <div class="groove-bar" style="height: 3px;"></div>
            </div>
        </div>

        <div class="toolbar-divider"></div>

        <!-- Vibe Status -->
        <div id="vibeStatus" class="vibe-status" title="Current vibe">
            <span class="vibe-emoji">🎵</span>
            <span class="vibe-text">Ready</span>
        </div>

        <div class="toolbar-divider"></div>

        <div class="toolbar-group">
            <button class="btn" id="automationBtn" title="Toggle Automation Panel">◑ Auto</button>
            <button class="btn" id="fitBtn">Fit to View</button>
        </div>
    </div>

    <div class="main-container">
        <div class="track-headers">
            <div class="ruler-spacer"></div>
            <div class="track-headers-scroll" id="trackHeaders"></div>
        </div>

        <div class="timeline-area">
            <div class="ruler" id="ruler">
                <canvas class="ruler-canvas" id="rulerCanvas"></canvas>
                <div class="playhead" id="rulerPlayhead" style="height: 30px;"></div>
            </div>

            <div class="tracks-container" id="tracksContainer">
                <canvas class="tracks-canvas" id="tracksCanvas"></canvas>
                <div class="playhead" id="tracksPlayhead"></div>
            </div>
        </div>
    </div>

    <div class="clip-tooltip" id="clipTooltip">
        <div class="clip-tooltip-title"></div>
        <div class="clip-tooltip-info"></div>
    </div>

    <!-- Details Panel -->
    <div class="details-panel" id="detailsPanel">
        <div class="details-panel-header">
            <div class="details-panel-title">
                <span id="detailsName">—</span>
                <span class="details-panel-type" id="detailsType">—</span>
            </div>
            <button class="details-panel-close" id="detailsClose">✕</button>
        </div>
        <div class="details-panel-content" id="detailsContent">
            <!-- Dynamic content -->
        </div>
    </div>

    <!-- Automation Panel -->
    <div class="automation-panel collapsed" id="automationPanel">
        <div class="automation-header" id="automationHeader">
            <span class="automation-header-title">Automation</span>
            <span class="automation-header-toggle">▼</span>
        </div>
        <div class="automation-content">
            <div class="automation-lanes-list" id="automationLanesList">
                <div class="add-automation-btn" id="addAutomationBtn">
                    <span>+</span>
                    <span>Add Automation Lane</span>
                </div>
            </div>
            <div class="automation-canvas-area" id="automationCanvasArea">
                <canvas class="automation-canvas" id="automationCanvas"></canvas>
            </div>
        </div>
    </div>

    <!-- Automation Target Picker -->
    <div class="automation-picker" id="automationPicker">
        <div class="automation-picker-header">
            <span>Select Parameter</span>
            <span class="automation-picker-close" id="automationPickerClose">✕</span>
        </div>
        <div class="automation-picker-search">
            <input type="text" id="automationPickerSearch" placeholder="Search parameters...">
        </div>
        <div class="automation-picker-list" id="automationPickerList"></div>
    </div>

    <!-- Curve Type Selector -->
    <div class="curve-type-selector" id="curveTypeSelector">
        <div class="curve-type-option" data-curve="linear">
            <span class="curve-type-icon">╱</span>
            <span>Linear</span>
        </div>
        <div class="curve-type-option" data-curve="smooth">
            <span class="curve-type-icon">∿</span>
            <span>Smooth</span>
        </div>
        <div class="curve-type-option" data-curve="exponential">
            <span class="curve-type-icon">⌒</span>
            <span>Exponential</span>
        </div>
        <div class="curve-type-option" data-curve="step">
            <span class="curve-type-icon">⌐</span>
            <span>Step</span>
        </div>
    </div>

    <script>
        (function() {
        const vscode = acquireVsCodeApi();

        // State
        let state = {
            tracks: [],
            transport: { running: false, current_beat: 0, loop_beat: null, loop_beats: null, bpm: 120 },
            bpm: 120,
            timeSignature: { numerator: 4, denominator: 4 },
            maxLoopBeats: 16,
            automationLanes: [],
            availableTargets: []
        };

        // Automation state
        let selectedLaneId = null;
        let selectedPointId = null;
        let isDraggingPoint = false;
        let isDrawingAutomation = false;
        let automationGridSnap = 0.25; // 16th notes

        // Track expand/collapse state (persisted in webview)
        const expandedTracks = new Set();

        // Selected track ID
        let selectedTrackId = null;

        // Selected clip data
        let selectedClip = null;

        // Get visible tracks (filter based on expand state)
        function getVisibleTracks() {
            const visible = [];

            // Defensive check for tracks array
            if (!state.tracks || !Array.isArray(state.tracks)) {
                return visible;
            }

            // Build a map for quick parent lookup
            const trackById = new Map();
            for (const track of state.tracks) {
                trackById.set(track.id, track);
            }

            // Check if all ancestors are expanded
            function areAllAncestorsExpanded(track) {
                if (!track.parentId) return true; // No parent = visible

                const parent = trackById.get(track.parentId);
                if (!parent) return true; // Parent not found = visible

                // Parent must be expanded AND all its ancestors must be expanded
                if (!expandedTracks.has(parent.id)) return false;

                return areAllAncestorsExpanded(parent);
            }

            for (const track of state.tracks) {
                // Always show depth 0 (groups)
                if (track.depth === 0) {
                    visible.push(track);
                    continue;
                }
                // For depth 1+, check if ALL ancestors are expanded
                if (areAllAncestorsExpanded(track)) {
                    visible.push(track);
                }
            }
            return visible;
        }

        // Smooth playhead - track transport state for precise interpolation
        let transportStartTime = 0;      // performance.now() when transport started
        let transportStartBeat = 0;      // Beat position when transport started
        let displayBeat = 0;             // Current display beat (smoothed)
        let targetBeat = 0;              // Target beat from server
        let wasRunning = false;

        // Helper to get display beat with smooth interpolation and looping
        function getDisplayBeat() {
            const maxBeats = state.maxLoopBeats || 16;
            if (maxBeats > 0) {
                return displayBeat % maxBeats;
            }
            return displayBeat;
        }

        // Get raw beat without looping (for calculations)
        function getRawBeat() {
            return displayBeat;
        }

        // Calculate current beat based on transport timing
        function calculateCurrentBeat() {
            if (state.transport?.running) {
                const now = performance.now();
                const elapsedMs = now - transportStartTime;
                const bpm = state.transport?.bpm || 120;
                const beatsElapsed = (elapsedMs / 60000) * bpm;
                return transportStartBeat + beatsElapsed;
            } else {
                return state.transport?.current_beat ?? 0;
            }
        }

        // Update transport timing anchor when receiving server updates
        function updateBeatAnchor() {
            const serverBeat = state.transport?.current_beat ?? 0;
            const running = state.transport?.running ?? false;

            if (running && !wasRunning) {
                // Transport just started - set anchor
                transportStartTime = performance.now();
                transportStartBeat = serverBeat;
                displayBeat = serverBeat;
            } else if (running) {
                // Transport running - check for drift and correct smoothly
                const calculatedBeat = calculateCurrentBeat();
                const drift = Math.abs(serverBeat - calculatedBeat);

                if (drift > 0.5) {
                    // Large drift - resync immediately (e.g., after seek)
                    transportStartTime = performance.now();
                    transportStartBeat = serverBeat;
                    displayBeat = serverBeat;
                } else if (drift > 0.05) {
                    // Small drift - adjust anchor slightly to correct over time
                    transportStartBeat = serverBeat - (calculatedBeat - transportStartBeat);
                }
            } else {
                // Transport stopped
                displayBeat = serverBeat;
            }

            targetBeat = serverBeat;
            wasRunning = running;
        }

        // Smooth animation update - called every frame
        function updateDisplayBeat() {
            if (state.transport?.running) {
                displayBeat = calculateCurrentBeat();
            }
        }

        // View settings
        let pixelsPerBeat = 40;
        let trackHeight = 48;
        let scrollLeft = 0;
        let scrollTop = 0;

        // Animation
        let animationFrame = null;
        let lastBeat = 0;

        // Debounce for state updates
        let renderDebounceTimer = null;
        function debouncedRender() {
            if (renderDebounceTimer) return;
            renderDebounceTimer = setTimeout(() => {
                renderDebounceTimer = null;
                render();
            }, 50);
        }

        // Elements
        const trackHeaders = document.getElementById('trackHeaders');
        const rulerCanvas = document.getElementById('rulerCanvas');
        const tracksCanvas = document.getElementById('tracksCanvas');
        const tracksContainer = document.getElementById('tracksContainer');
        const rulerPlayhead = document.getElementById('rulerPlayhead');
        const tracksPlayhead = document.getElementById('tracksPlayhead');
        const transportTime = document.getElementById('transportTime');
        const bpmValue = document.getElementById('bpmValue');
        const playBtn = document.getElementById('playBtn');
        const zoomSlider = document.getElementById('zoomSlider');
        const clipTooltip = document.getElementById('clipTooltip');

        // Check for required elements
        if (!rulerCanvas || !tracksCanvas || !tracksContainer || !trackHeaders) {
            console.error('Required elements not found');
            return;
        }

        // Contexts
        const rulerCtx = rulerCanvas.getContext('2d');
        const tracksCtx = tracksCanvas.getContext('2d');

        if (!rulerCtx || !tracksCtx) {
            console.error('Could not get canvas contexts');
            return;
        }

        // Initialize
        function init() {
            setupEventListeners();
            resize();
            render();
            startAnimation();
        }

        function setupEventListeners() {
            // Zoom
            if (zoomSlider) {
                zoomSlider.addEventListener('input', (e) => {
                    pixelsPerBeat = parseInt(e.target.value);
                    render();
                });
            }

            // Fun play button messages
            const playTitles = [
                "Let's go! (Space)",
                "Drop the beat! (Space)",
                "Make some noise! (Space)",
                "It's showtime! (Space)",
                "Unleash the vibes! (Space)",
                "Hit it! (Space)",
                "3... 2... 1... GO! (Space)",
            ];
            const pauseTitles = [
                "Take five (Space)",
                "Chill for a sec (Space)",
                "Hold up! (Space)",
                "Breather time (Space)",
                "Pause the magic (Space)",
            ];

            // Transport controls
            if (playBtn) {
                playBtn.addEventListener('click', () => {
                    vscode.postMessage({ command: 'toggleTransport' });
                });

                playBtn.addEventListener('mouseenter', () => {
                    const running = state.transport?.running;
                    const titles = running ? pauseTitles : playTitles;
                    playBtn.title = titles[Math.floor(Math.random() * titles.length)];
                });
            }

            const stopBtn = document.getElementById('stopBtn');
            if (stopBtn) {
                stopBtn.addEventListener('click', () => {
                    vscode.postMessage({ command: 'stopTransport' });
                    // Don't seek - pure pause behavior
                });
            }

            const fitBtn = document.getElementById('fitBtn');
            if (fitBtn) {
                fitBtn.addEventListener('click', fitToView);
            }

            // Scroll sync - sync track headers with tracks canvas
            tracksContainer.addEventListener('scroll', () => {
                scrollLeft = tracksContainer.scrollLeft;
                scrollTop = tracksContainer.scrollTop;
                // trackHeaders IS the scroll container, not a parent of it
                trackHeaders.scrollTop = scrollTop;
                renderRuler();
                updatePlayheadPosition();
            });

            // Also sync in reverse - if user scrolls headers, sync tracks
            trackHeaders.addEventListener('scroll', () => {
                if (trackHeaders.scrollTop !== scrollTop) {
                    scrollTop = trackHeaders.scrollTop;
                    tracksContainer.scrollTop = scrollTop;
                }
            });

            // Click on ruler to seek
            const ruler = document.getElementById('ruler');
            if (ruler) {
                ruler.addEventListener('click', (e) => {
                    const rect = rulerCanvas.getBoundingClientRect();
                    const x = e.clientX - rect.left + scrollLeft;
                    const beat = x / pixelsPerBeat;
                    vscode.postMessage({ command: 'seek', beat: Math.max(0, beat) });
                });
            }

            // Click on clip
            tracksCanvas.addEventListener('click', handleCanvasClick);
            tracksCanvas.addEventListener('dblclick', handleCanvasDoubleClick);
            tracksCanvas.addEventListener('contextmenu', handleCanvasContextMenu);
            tracksCanvas.addEventListener('mousemove', handleCanvasMouseMove);
            tracksCanvas.addEventListener('mouseleave', () => {
                if (clipTooltip) clipTooltip.classList.remove('visible');
            });

            // Resize
            window.addEventListener('resize', () => {
                resize();
                render();
            });

            // Messages from extension
            window.addEventListener('message', (event) => {
                const message = event.data;
                switch (message.type) {
                    case 'stateUpdate':
                        state = { ...state, ...message.data };
                        // Auto-expand all tracks with children on first load
                        if (expandedTracks.size === 0 && state.tracks.length > 0) {
                            for (const track of state.tracks) {
                                if (track.hasChildren) {
                                    expandedTracks.add(track.id);
                                }
                            }
                        }
                        updateBeatAnchor();
                        renderTrackHeaders();
                        renderAutomationLanes();
                        updateVibeStatus();
                        debouncedRender(); // Debounced to prevent excessive re-renders
                        break;
                    case 'transportUpdate':
                        state.transport = message.data;
                        updateBeatAnchor();
                        updateTransportDisplay();
                        updateVibeStatus();
                        break;
                    case 'automationUpdate':
                        state.automationLanes = message.data;
                        renderAutomationLanes();
                        renderAutomationCanvas();
                        break;
                }
            });

            // Keyboard shortcuts
            document.addEventListener('keydown', (e) => {
                // Skip if typing in an input
                if (e.target.closest('input')) return;

                // Space - Play/Stop
                if (e.code === 'Space') {
                    e.preventDefault();
                    vscode.postMessage({ command: 'toggleTransport' });
                    return;
                }

                // Delete key removes selected automation point
                if (e.code === 'Delete' || e.code === 'Backspace') {
                    if (selectedLaneId && selectedPointId) {
                        e.preventDefault();
                        vscode.postMessage({
                            command: 'removeAutomationPoint',
                            laneId: selectedLaneId,
                            pointId: selectedPointId
                        });
                        selectedPointId = null;
                    }
                    return;
                }

                // Track shortcuts (require selected track)
                if (selectedTrackId) {
                    const selectedTrack = state.tracks.find(t => t.id === selectedTrackId);
                    if (!selectedTrack) return;

                    // Enter - Go to source
                    if (e.code === 'Enter' && selectedTrack.sourceLocation) {
                        e.preventDefault();
                        vscode.postMessage({ command: 'goToSource', sourceLocation: selectedTrack.sourceLocation });
                        return;
                    }

                    // E - Edit (open pattern/melody editor)
                    if (e.code === 'KeyE') {
                        e.preventDefault();
                        if (selectedTrack.type === 'pattern') {
                            vscode.postMessage({ command: 'openPatternEditor', name: selectedTrack.name });
                        } else if (selectedTrack.type === 'melody') {
                            vscode.postMessage({ command: 'openMelodyEditor', name: selectedTrack.name });
                        }
                        return;
                    }

                    // M - Mute (for groups)
                    if (e.code === 'KeyM' && selectedTrack.type === 'group') {
                        e.preventDefault();
                        vscode.postMessage({ command: 'muteTrack', trackId: selectedTrackId });
                        return;
                    }

                    // S - Solo (for groups)
                    if (e.code === 'KeyS' && !e.ctrlKey && !e.metaKey && selectedTrack.type === 'group') {
                        e.preventDefault();
                        vscode.postMessage({ command: 'soloTrack', trackId: selectedTrackId });
                        return;
                    }

                    // I - Open Inspector
                    if (e.code === 'KeyI') {
                        e.preventDefault();
                        vscode.postMessage({ command: 'openInspector', trackId: selectedTrackId, trackType: selectedTrack.type });
                        return;
                    }

                    // Arrow Up/Down - Navigate tracks
                    if (e.code === 'ArrowUp' || e.code === 'ArrowDown') {
                        e.preventDefault();
                        const visibleTracks = getVisibleTracks();
                        const currentIndex = visibleTracks.findIndex(t => t.id === selectedTrackId);
                        if (currentIndex !== -1) {
                            const newIndex = e.code === 'ArrowUp'
                                ? Math.max(0, currentIndex - 1)
                                : Math.min(visibleTracks.length - 1, currentIndex + 1);
                            if (newIndex !== currentIndex) {
                                selectedTrackId = visibleTracks[newIndex].id;
                                vscode.postMessage({ command: 'selectTrack', trackId: selectedTrackId, trackType: visibleTracks[newIndex].type });
                                renderTrackHeaders();
                            }
                        }
                        return;
                    }

                    // Arrow Left/Right - Collapse/Expand
                    if ((e.code === 'ArrowLeft' || e.code === 'ArrowRight') && selectedTrack.hasChildren) {
                        e.preventDefault();
                        const isExpanded = expandedTracks.has(selectedTrackId);
                        if (e.code === 'ArrowRight' && !isExpanded) {
                            toggleExpand(selectedTrackId);
                        } else if (e.code === 'ArrowLeft' && isExpanded) {
                            toggleExpand(selectedTrackId);
                        }
                        return;
                    }
                }

                // Escape - Deselect / close context menu
                if (e.code === 'Escape') {
                    hideContextMenu();
                    if (selectedTrackId) {
                        selectedTrackId = null;
                        renderTrackHeaders();
                    }
                }
            });

            // Automation panel toggle
            document.getElementById('automationBtn').addEventListener('click', () => {
                const panel = document.getElementById('automationPanel');
                panel.classList.toggle('collapsed');
                renderAutomationCanvas();
            });

            document.getElementById('automationHeader').addEventListener('click', () => {
                const panel = document.getElementById('automationPanel');
                panel.classList.toggle('collapsed');
                renderAutomationCanvas();
            });

            // Add automation lane button
            document.getElementById('addAutomationBtn').addEventListener('click', (e) => {
                showAutomationPicker(e.target.getBoundingClientRect());
            });

            // Automation picker close
            document.getElementById('automationPickerClose').addEventListener('click', hideAutomationPicker);

            // Details panel close
            document.getElementById('detailsClose').addEventListener('click', hideDetailsPanel);

            // Automation picker search
            document.getElementById('automationPickerSearch').addEventListener('input', (e) => {
                filterAutomationTargets(e.target.value);
            });

            // Click outside picker to close
            document.addEventListener('click', (e) => {
                const picker = document.getElementById('automationPicker');
                const addBtn = document.getElementById('addAutomationBtn');
                if (!picker.contains(e.target) && !addBtn.contains(e.target)) {
                    hideAutomationPicker();
                }
                const curveSelector = document.getElementById('curveTypeSelector');
                if (!curveSelector.contains(e.target)) {
                    curveSelector.classList.remove('visible');
                }
            });

            // Automation canvas interactions
            setupAutomationCanvasListeners();
        }

        // Automation canvas event listeners
        function setupAutomationCanvasListeners() {
            const canvas = document.getElementById('automationCanvas');

            canvas.addEventListener('click', handleAutomationClick);
            canvas.addEventListener('mousedown', handleAutomationMouseDown);
            canvas.addEventListener('mousemove', handleAutomationMouseMove);
            canvas.addEventListener('mouseup', handleAutomationMouseUp);
            canvas.addEventListener('mouseleave', handleAutomationMouseUp);
            canvas.addEventListener('dblclick', handleAutomationDoubleClick);
            canvas.addEventListener('contextmenu', handleAutomationContextMenu);
        }

        function resize() {
            const container = tracksContainer;
            const rulerRect = document.getElementById('ruler').getBoundingClientRect();

            // High DPI support
            const dpr = window.devicePixelRatio || 1;

            // Ruler canvas
            rulerCanvas.width = rulerRect.width * dpr;
            rulerCanvas.height = 30 * dpr;
            rulerCanvas.style.width = rulerRect.width + 'px';
            rulerCanvas.style.height = '30px';
            rulerCtx.scale(dpr, dpr);

            // Tracks canvas - use maxLoopBeats for width and visible tracks for height
            const visibleTracks = getVisibleTracks();
            const totalBeats = Math.max(64, getMaxBeat() + 16);
            const totalWidth = Math.max(container.clientWidth, totalBeats * pixelsPerBeat);
            const totalHeight = Math.max(container.clientHeight, visibleTracks.length * trackHeight);

            tracksCanvas.width = totalWidth * dpr;
            tracksCanvas.height = totalHeight * dpr;
            tracksCanvas.style.width = totalWidth + 'px';
            tracksCanvas.style.height = totalHeight + 'px';
            tracksCtx.scale(dpr, dpr);

            // Playhead height
            tracksPlayhead.style.height = totalHeight + 'px';
        }

        function getMaxBeat() {
            // Use maxLoopBeats from state - this is calculated server-side
            let max = state.maxLoopBeats || 16;
            // Also check any visible clips
            for (const track of state.tracks) {
                if (track.loopBeats) {
                    max = Math.max(max, track.loopBeats);
                }
                for (const clip of track.clips || []) {
                    max = Math.max(max, clip.endBeat);
                }
            }
            return max;
        }

        function render() {
            try {
                resize();
                renderRuler();
                renderTracks();
                updatePlayheadPosition();
            } catch (e) {
                console.error('Render error:', e);
            }
        }

        function renderRuler() {
            const width = rulerCanvas.width / (window.devicePixelRatio || 1);
            const height = 30;
            const ctx = rulerCtx;

            ctx.clearRect(0, 0, width, height);

            // Background
            ctx.fillStyle = '#232323';
            ctx.fillRect(0, 0, width, height);

            const beatsPerBar = state.timeSignature?.numerator || 4;
            const startBeat = Math.floor(scrollLeft / pixelsPerBeat);
            const endBeat = Math.ceil((scrollLeft + width) / pixelsPerBeat);

            // Draw beat markers
            for (let beat = startBeat; beat <= endBeat; beat++) {
                const x = beat * pixelsPerBeat - scrollLeft;

                if (beat % beatsPerBar === 0) {
                    // Bar marker
                    ctx.strokeStyle = '#5a5a5a';
                    ctx.lineWidth = 1;
                    ctx.beginPath();
                    ctx.moveTo(x, 0);
                    ctx.lineTo(x, height);
                    ctx.stroke();

                    // Bar number
                    const barNum = Math.floor(beat / beatsPerBar) + 1;
                    ctx.fillStyle = '#d4d4d4';
                    ctx.font = '10px -apple-system, sans-serif';
                    ctx.fillText(barNum.toString(), x + 4, 12);
                } else {
                    // Beat marker
                    ctx.strokeStyle = '#3a3a3a';
                    ctx.lineWidth = 1;
                    ctx.beginPath();
                    ctx.moveTo(x, 20);
                    ctx.lineTo(x, height);
                    ctx.stroke();
                }
            }

            // Bottom border
            ctx.strokeStyle = '#3c3c3c';
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(0, height - 0.5);
            ctx.lineTo(width, height - 0.5);
            ctx.stroke();
        }

        function renderTracks() {
            const width = tracksCanvas.width / (window.devicePixelRatio || 1);
            const height = tracksCanvas.height / (window.devicePixelRatio || 1);
            const ctx = tracksCtx;

            ctx.clearRect(0, 0, width, height);

            // Get visible tracks based on expand state
            const visibleTracks = getVisibleTracks();

            // Show empty state if no tracks
            if (visibleTracks.length === 0) {
                ctx.fillStyle = '#1e1e1e';
                ctx.fillRect(0, 0, width, height);
                ctx.fillStyle = '#666';
                ctx.font = '14px -apple-system, sans-serif';
                ctx.textAlign = 'center';
                // Fun empty state messages
                const emptyMessages = [
                    { title: '🎵 Silence is golden...', subtitle: 'but music is platinum. Start a sequence!' },
                    { title: '🎹 The stage is set...', subtitle: 'Start a sequence and let the magic happen!' },
                    { title: '🎸 *cricket sounds*', subtitle: 'Nothing playing yet. Time to drop some beats?' },
                    { title: '🎤 *tap tap* Is this thing on?', subtitle: 'Start a sequence to see it here!' },
                    { title: '🥁 Waiting for the drop...', subtitle: 'No sequences running. Make some noise!' },
                    { title: '🎺 The orchestra awaits...', subtitle: 'Your sequences will appear here!' },
                    { title: '🎧 Ready when you are!', subtitle: 'Start a pattern or sequence to visualize it.' },
                ];
                const msgIndex = Math.floor(Math.random() * emptyMessages.length);
                const msg = emptyMessages[msgIndex] || emptyMessages[0];
                ctx.fillText(msg.title, width / 2, height / 2 - 10);
                ctx.fillStyle = '#555';
                ctx.font = '12px -apple-system, sans-serif';
                ctx.fillText(msg.subtitle, width / 2, height / 2 + 15);
                ctx.textAlign = 'left';
                return;
            }

            const beatsPerBar = state.timeSignature?.numerator || 4;

            // Draw grid
            const startBeat = 0;
            const endBeat = Math.ceil(width / pixelsPerBeat);

            for (let beat = startBeat; beat <= endBeat; beat++) {
                const x = beat * pixelsPerBeat;

                if (beat % beatsPerBar === 0) {
                    ctx.strokeStyle = '#3a3a3a';
                    ctx.lineWidth = 1;
                } else {
                    ctx.strokeStyle = '#2a2a2a';
                    ctx.lineWidth = 0.5;
                }

                ctx.beginPath();
                ctx.moveTo(x + 0.5, 0);
                ctx.lineTo(x + 0.5, height);
                ctx.stroke();
            }

            // Draw track backgrounds and clips
            visibleTracks.forEach((track, index) => {
                const y = index * trackHeight;
                const depth = track.depth ?? 0;

                // Track background based on depth level
                if (depth === 0) {
                    // Group header - darker
                    ctx.fillStyle = '#232323';
                } else if (depth === 1) {
                    // Sequence/pattern/melody - alternating
                    ctx.fillStyle = index % 2 === 0 ? '#1e1e1e' : '#222222';
                } else {
                    // Nested items
                    ctx.fillStyle = index % 2 === 0 ? '#1a1a1a' : '#1c1c1c';
                }
                ctx.fillRect(0, y, width, trackHeight);

                // Draw hierarchy connector line in track area
                if (depth > 0) {
                    ctx.strokeStyle = '#333';
                    ctx.lineWidth = 1;
                    ctx.setLineDash([2, 2]);
                    ctx.beginPath();
                    ctx.moveTo(2, y);
                    ctx.lineTo(2, y + trackHeight);
                    ctx.stroke();
                    ctx.setLineDash([]);
                }

                // Track bottom border
                ctx.strokeStyle = depth === 0 ? '#4a4a4a' : '#3c3c3c';
                ctx.lineWidth = 1;
                ctx.beginPath();
                ctx.moveTo(0, y + trackHeight - 0.5);
                ctx.lineTo(width, y + trackHeight - 0.5);
                ctx.stroke();

                // Draw clips
                (track.clips || []).forEach(clip => {
                    drawClip(ctx, clip, y, track);
                });

                // For group headers without clips, draw a subtle indicator
                if (depth === 0 && (!track.clips || track.clips.length === 0)) {
                    ctx.fillStyle = '#444';
                    ctx.font = '10px -apple-system, sans-serif';
                    ctx.fillText('Group: ' + track.name, 10, y + trackHeight / 2 + 3);
                }
            });
        }

        function drawClip(ctx, clip, trackY, track) {
            const x = clip.startBeat * pixelsPerBeat;
            const clipWidth = (clip.endBeat - clip.startBeat) * pixelsPerBeat;

            // Skip clips that are too small or off-screen
            if (clipWidth < 2) return;

            const y = trackY + 4;
            const clipHeight = trackHeight - 8;
            const radius = 3;
            const baseColor = clip.color || '#4a6b3a';

            // Simple solid fill (no gradient for performance)
            ctx.fillStyle = clip.active ? lightenColor(baseColor, 15) : baseColor;
            ctx.beginPath();
            ctx.roundRect(x + 1, y, clipWidth - 2, clipHeight, radius);
            ctx.fill();

            // Simple border
            ctx.strokeStyle = clip.active ? lightenColor(baseColor, 40) : lightenColor(baseColor, 15);
            ctx.lineWidth = clip.active ? 2 : 1;
            ctx.stroke();

            // Draw content visualization (pattern steps or melody notes)
            try {
                ctx.save();
                ctx.beginPath();
                ctx.rect(x + 2, y + 1, clipWidth - 4, clipHeight - 2);
                ctx.clip();

                if (clip.type === 'pattern' && clip.stepPattern && clipWidth > 40) {
                    // Draw pattern steps
                    drawPatternSteps(ctx, clip.stepPattern, x + 2, y + 14, clipWidth - 4, clipHeight - 16, clip);
                } else if (clip.type === 'melody' && clip.melodyEvents && clip.melodyEvents.length > 0 && clipWidth > 40) {
                    // Draw melody notes (mini piano roll)
                    drawMelodyNotes(ctx, clip.melodyEvents, x + 2, y + 14, clipWidth - 4, clipHeight - 16, clip);
                }

                ctx.restore();
            } catch (e) {
                ctx.restore();
                console.error('Error drawing clip content:', e);
            }

            // Clip name (always draw on top)
            if (clipWidth > 30) {
                ctx.fillStyle = '#fff';
                ctx.font = '9px -apple-system, sans-serif';
                ctx.save();
                ctx.beginPath();
                ctx.rect(x + 3, y, clipWidth - 6, clipHeight);
                ctx.clip();
                ctx.fillText(clip.name, x + 4, y + 11);
                ctx.restore();
            }
        }

        // Draw pattern steps visualization
        function drawPatternSteps(ctx, stepPattern, x, y, width, height, clip) {
            // Parse the step pattern string
            // Format: "x..x..x.|x.x.x.x." where x = hit, . = rest, | = bar separator
            const cleanPattern = stepPattern.replace(/[|]/g, '');
            const steps = [];
            for (let i = 0; i < cleanPattern.length; i++) {
                const char = cleanPattern[i];
                if (char === 'x' || char === 'X') {
                    steps.push({ index: i, accent: char === 'X' });
                } else if (char === 'o' || char === 'O') {
                    steps.push({ index: i, accent: char === 'O' });
                }
            }

            const totalSteps = cleanPattern.length;
            if (totalSteps === 0) return;

            const stepWidth = width / totalSteps;
            const dotRadius = Math.min(stepWidth * 0.35, height * 0.3, 4);

            // Draw dots for each hit
            ctx.fillStyle = lightenColor(clip.color || '#9bbb59', 50);
            for (const step of steps) {
                const dotX = x + (step.index + 0.5) * stepWidth;
                const dotY = y + height / 2;
                ctx.beginPath();
                ctx.arc(dotX, dotY, step.accent ? dotRadius * 1.3 : dotRadius, 0, Math.PI * 2);
                ctx.fill();
            }

            // Draw current step highlight if playing
            if (clip.active && state.transport?.running) {
                const currentBeat = getDisplayBeat();
                const clipDuration = clip.endBeat - clip.startBeat;
                const beatInClip = ((currentBeat - clip.startBeat) % clipDuration + clipDuration) % clipDuration;
                const stepsPerBeat = totalSteps / clipDuration;
                const currentStep = Math.floor(beatInClip * stepsPerBeat) % totalSteps;

                const highlightX = x + currentStep * stepWidth;
                ctx.fillStyle = 'rgba(255, 255, 255, 0.3)';
                ctx.fillRect(highlightX, y - 2, stepWidth, height + 4);
            }
        }

        // Draw melody notes visualization (mini piano roll)
        function drawMelodyNotes(ctx, events, x, y, width, height, clip) {
            if (!events || events.length === 0) return;

            // Find min/max MIDI notes for scaling
            let minNote = 127, maxNote = 0;
            for (const event of events) {
                if (event.midiNote < minNote) minNote = event.midiNote;
                if (event.midiNote > maxNote) maxNote = event.midiNote;
            }

            // Expand range slightly for visual clarity
            const noteRange = Math.max(maxNote - minNote, 12);
            minNote = Math.max(0, minNote - 2);
            maxNote = Math.min(127, maxNote + 2);
            const adjustedRange = maxNote - minNote || 12;

            const clipDuration = clip.endBeat - clip.startBeat;
            const beatsPerPixel = clipDuration / width;
            const noteHeight = Math.max(2, Math.min(height / adjustedRange, 4));

            // Draw each note as a horizontal bar
            ctx.fillStyle = lightenColor(clip.color || '#d19a66', 50);
            for (const event of events) {
                const noteX = x + (event.beat / clipDuration) * width;
                const noteDuration = event.duration || 0.25;
                const noteWidth = Math.max(2, (noteDuration / clipDuration) * width);
                const noteY = y + height - ((event.midiNote - minNote) / adjustedRange) * height - noteHeight;

                ctx.fillRect(noteX, noteY, noteWidth, noteHeight);
            }

            // Draw current position highlight if playing
            if (clip.active && state.transport?.running) {
                const currentBeat = getDisplayBeat();
                const beatInClip = ((currentBeat - clip.startBeat) % clipDuration + clipDuration) % clipDuration;
                const posX = x + (beatInClip / clipDuration) * width;

                ctx.strokeStyle = 'rgba(255, 255, 255, 0.5)';
                ctx.lineWidth = 1;
                ctx.beginPath();
                ctx.moveTo(posX, y - 2);
                ctx.lineTo(posX, y + height + 2);
                ctx.stroke();
            }
        }

        function hashString(str) {
            let hash = 0;
            for (let i = 0; i < str.length; i++) {
                hash = ((hash << 5) - hash) + str.charCodeAt(i);
                hash |= 0;
            }
            return hash;
        }

        function darkenColor(color, percent) {
            const num = parseInt(color.replace('#', ''), 16);
            const amt = Math.round(2.55 * percent);
            const R = Math.max(0, (num >> 16) - amt);
            const G = Math.max(0, ((num >> 8) & 0x00FF) - amt);
            const B = Math.max(0, (num & 0x0000FF) - amt);
            return '#' + (0x1000000 + R * 0x10000 + G * 0x100 + B).toString(16).slice(1);
        }

        function getTypeIcon(type) {
            switch (type) {
                case 'pattern': return '⬢';
                case 'melody': return '♪';
                case 'sequence': return '▤';
                case 'fade': return '◐';
                default: return '●';
            }
        }

        function lightenColor(color, percent) {
            const num = parseInt(color.replace('#', ''), 16);
            const amt = Math.round(2.55 * percent);
            const R = Math.min(255, (num >> 16) + amt);
            const G = Math.min(255, ((num >> 8) & 0x00FF) + amt);
            const B = Math.min(255, (num & 0x0000FF) + amt);
            return '#' + (0x1000000 + R * 0x10000 + G * 0x100 + B).toString(16).slice(1);
        }

        function renderTrackHeaders() {
            trackHeaders.innerHTML = '';

            // Get visible tracks based on expand state
            const visibleTracks = getVisibleTracks();

            // Find last child of each parent for connector styling
            const lastChildMap = new Map();
            for (let i = visibleTracks.length - 1; i >= 0; i--) {
                const track = visibleTracks[i];
                if (track.parentId && !lastChildMap.has(track.parentId)) {
                    lastChildMap.set(track.parentId, track.id);
                }
            }

            // Show empty state if no tracks
            if (!state.tracks || state.tracks.length === 0) {
                const emptyMsg = document.createElement('div');
                emptyMsg.className = 'empty-tracks';
                emptyMsg.style.cssText = 'padding: 20px; color: var(--text-muted); text-align: center; font-size: 12px;';
                const emptyTips = [
                    '🎼 No groups yet! Load a .vibe file to start vibing.',
                    '🎹 Empty canvas, infinite possibilities. Load a .vibe file!',
                    '🎵 This is where the magic happens. Just need a .vibe file...',
                    '🎧 Ready to rock! Drop a .vibe file and let\\'s go!',
                    '🎸 *elevator music plays* Load a .vibe file to begin!',
                ];
                const tipIndex = Math.floor(Math.random() * emptyTips.length);
                emptyMsg.innerHTML = emptyTips[tipIndex] || emptyTips[0];
                trackHeaders.appendChild(emptyMsg);
                return;
            }

            visibleTracks.forEach((track, index) => {
                const isLastChild = lastChildMap.get(track.parentId) === track.id;
                const depth = track.depth ?? 0;
                const childCount = track.childCount || 0;
                const isExpanded = expandedTracks.has(track.id);
                const isSelected = selectedTrackId === track.id;

                const header = document.createElement('div');
                header.className = 'track-header' +
                    ' depth-' + depth +
                    (track.active ? ' active' : '') +
                    (track.muted ? ' muted' : '') +
                    (isLastChild ? ' last-child' : '') +
                    (isSelected ? ' selected' : '');
                header.dataset.trackId = track.id;

                // Type icon based on track type
                const typeIcon = {
                    'group': '📁',
                    'sequence': '🎵',
                    'pattern': '⬢',
                    'melody': '♪'
                }[track.type] || '●';

                // Build header content - show expand button only for tracks with children
                let expandBtn = '';
                if (track.hasChildren) {
                    expandBtn = \`<span class="track-expand \${isExpanded ? 'expanded' : ''}" data-action="expand" data-track-id="\${track.id}" title="\${isExpanded ? 'Collapse' : 'Expand'}">▶</span>\`;
                } else {
                    // Spacer for alignment
                    expandBtn = '<span style="width: 20px; display: inline-block;"></span>';
                }

                let countBadge = '';
                if (childCount > 0) {
                    countBadge = \`<span class="child-count">\${childCount}</span>\`;
                }

                // Mini-mixer controls for groups
                let mixerControls = '';
                if (track.type === 'group' && track.groupPath) {
                    const ampDb = track.amp != null ? (track.amp > 0 ? (20 * Math.log10(track.amp)).toFixed(1) : '-∞') : '0.0';
                    const panDisplay = track.pan != null ? (track.pan === 0 ? 'C' : (track.pan > 0 ? 'R' + Math.round(track.pan * 50) : 'L' + Math.round(Math.abs(track.pan) * 50))) : 'C';
                    const panRotation = track.pan != null ? (track.pan * 135) : 0;

                    mixerControls = \`
                        <div class="track-mixer">
                            <div class="activity-indicator \${track.active ? 'active' : ''}"></div>
                            <div class="mini-fader" title="Volume">
                                <input type="range" min="0" max="1" step="0.01" value="\${track.amp ?? 1}"
                                       data-action="amp" data-group-path="\${track.groupPath}">
                                <span class="mini-fader-value">\${ampDb}dB</span>
                            </div>
                            <div class="mini-pan" title="Pan">
                                <div class="pan-knob" data-action="pan" data-group-path="\${track.groupPath}"
                                     style="--pan-rotation: \${panRotation}deg"></div>
                                <span class="pan-value">\${panDisplay}</span>
                            </div>
                            \${track.effectsCount > 0 ? \`
                                <div class="effects-chain" data-action="effects" data-group-path="\${track.groupPath}" title="Open Effects">
                                    <span class="fx-icon">🎛️</span>
                                    <span class="fx-count">\${track.effectsCount}</span>
                                </div>
                            \` : ''}
                        </div>
                    \`;
                }

                header.innerHTML = \`
                    \${expandBtn}
                    <div class="track-color" style="background: \${track.color}"></div>
                    <div class="track-info">
                        <div class="track-name">\${track.name}\${countBadge}</div>
                        <div class="track-type type-\${track.type}">\${typeIcon} \${track.type}\${track.loopBeats ? \` · \${track.loopBeats} beats\` : ''}</div>
                    </div>
                    \${mixerControls}
                    <div class="track-controls">
                        \${track.type === 'group' ? \`
                            <button class="track-btn mute \${track.muted ? 'active' : ''}" data-action="mute" title="Mute">M</button>
                            <button class="track-btn solo \${track.soloed ? 'active' : ''}" data-action="solo" title="Solo">S</button>
                        \` : ''}
                    </div>
                \`;

                // Click on header (not buttons/controls) to select or expand
                header.addEventListener('click', (e) => {
                    if (!e.target.closest('.track-btn') && !e.target.closest('.track-expand') &&
                        !e.target.closest('.track-mixer') && !e.target.closest('input')) {
                        // Select this track
                        selectedTrackId = track.id;
                        vscode.postMessage({ command: 'selectTrack', trackId: track.id, trackType: track.type });
                        renderTrackHeaders(); // Re-render to show selection

                        if (track.hasChildren) {
                            // Double-click to expand/collapse
                        } else if (track.sourceLocation && e.detail === 2) {
                            // Double-click goes to source
                            vscode.postMessage({ command: 'goToSource', sourceLocation: track.sourceLocation });
                        }
                    }
                });

                // Double-click to expand/collapse or open editor
                header.addEventListener('dblclick', (e) => {
                    if (!e.target.closest('.track-btn') && !e.target.closest('.track-mixer') && !e.target.closest('input')) {
                        if (track.hasChildren) {
                            toggleExpand(track.id);
                        } else if (track.type === 'pattern') {
                            vscode.postMessage({ command: 'openPatternEditor', name: track.name });
                        } else if (track.type === 'melody') {
                            vscode.postMessage({ command: 'openMelodyEditor', name: track.name });
                        } else if (track.sourceLocation) {
                            vscode.postMessage({ command: 'goToSource', sourceLocation: track.sourceLocation });
                        }
                    }
                });

                // Context menu
                header.addEventListener('contextmenu', (e) => {
                    e.preventDefault();
                    showContextMenu(e.clientX, e.clientY, track);
                });

                // Mute/Solo buttons
                header.querySelectorAll('.track-btn').forEach(btn => {
                    btn.addEventListener('click', (e) => {
                        e.stopPropagation();
                        const action = btn.dataset.action;
                        if (action === 'mute') {
                            vscode.postMessage({ command: 'muteTrack', trackId: track.id });
                        } else if (action === 'solo') {
                            vscode.postMessage({ command: 'soloTrack', trackId: track.id });
                        }
                    });
                });

                // Expand/collapse button
                const expandBtnEl = header.querySelector('.track-expand');
                if (expandBtnEl) {
                    expandBtnEl.addEventListener('click', (e) => {
                        e.stopPropagation();
                        const trackId = expandBtnEl.dataset.trackId;
                        toggleExpand(trackId);
                    });
                }

                // Volume fader
                const volumeSlider = header.querySelector('input[data-action="amp"]');
                if (volumeSlider) {
                    volumeSlider.addEventListener('input', (e) => {
                        e.stopPropagation();
                        const value = parseFloat(e.target.value);
                        const groupPath = e.target.dataset.groupPath;
                        vscode.postMessage({ command: 'setGroupAmp', groupPath, value });
                    });
                    volumeSlider.addEventListener('click', (e) => e.stopPropagation());
                }

                // Pan knob (simple click-and-drag)
                const panKnob = header.querySelector('.pan-knob');
                if (panKnob) {
                    let isDragging = false;
                    let startY = 0;
                    let startPan = track.pan ?? 0;

                    panKnob.addEventListener('mousedown', (e) => {
                        e.stopPropagation();
                        isDragging = true;
                        startY = e.clientY;
                        startPan = track.pan ?? 0;
                        document.addEventListener('mousemove', onPanMove);
                        document.addEventListener('mouseup', onPanUp);
                    });

                    function onPanMove(e) {
                        if (!isDragging) return;
                        const deltaY = startY - e.clientY;
                        const newPan = Math.max(-1, Math.min(1, startPan + deltaY / 100));
                        const groupPath = panKnob.dataset.groupPath;
                        vscode.postMessage({ command: 'setGroupPan', groupPath, value: newPan });
                    }

                    function onPanUp() {
                        isDragging = false;
                        document.removeEventListener('mousemove', onPanMove);
                        document.removeEventListener('mouseup', onPanUp);
                    }

                    // Double-click to reset pan to center
                    panKnob.addEventListener('dblclick', (e) => {
                        e.stopPropagation();
                        const groupPath = panKnob.dataset.groupPath;
                        vscode.postMessage({ command: 'setGroupPan', groupPath, value: 0 });
                    });
                }

                // Effects chain button
                const effectsBtn = header.querySelector('.effects-chain');
                if (effectsBtn) {
                    effectsBtn.addEventListener('click', (e) => {
                        e.stopPropagation();
                        const groupPath = effectsBtn.dataset.groupPath;
                        vscode.postMessage({ command: 'openEffectRack', groupPath });
                    });
                }

                trackHeaders.appendChild(header);
            });
        }

        // Context menu handling
        let contextMenuEl = null;

        function showContextMenu(x, y, track) {
            hideContextMenu();

            contextMenuEl = document.createElement('div');
            contextMenuEl.className = 'context-menu';
            contextMenuEl.style.left = x + 'px';
            contextMenuEl.style.top = y + 'px';

            const items = [];

            // Common items
            if (track.sourceLocation) {
                items.push({ label: '📄 Go to Source', shortcut: 'Enter', action: () => {
                    vscode.postMessage({ command: 'goToSource', sourceLocation: track.sourceLocation });
                }});
            }

            items.push({ label: '🔍 Show in Inspector', shortcut: 'I', action: () => {
                vscode.postMessage({ command: 'openInspector', trackId: track.id, trackType: track.type });
            }});

            // Type-specific items
            if (track.type === 'group') {
                items.push({ separator: true });
                items.push({ label: track.muted ? '🔊 Unmute' : '🔇 Mute', shortcut: 'M', action: () => {
                    vscode.postMessage({ command: 'muteTrack', trackId: track.id });
                }});
                items.push({ label: track.soloed ? '🔈 Unsolo' : '🔉 Solo', shortcut: 'S', action: () => {
                    vscode.postMessage({ command: 'soloTrack', trackId: track.id });
                }});
                if (track.effectsCount > 0) {
                    items.push({ separator: true });
                    items.push({ label: '🎛️ Open Effect Rack', action: () => {
                        vscode.postMessage({ command: 'openEffectRack', groupPath: track.groupPath });
                    }});
                }
                items.push({ separator: true });
                items.push({ label: '📈 Add Automation Lane', action: () => {
                    vscode.postMessage({
                        command: 'addAutomationLane',
                        target: { type: 'group', name: track.groupPath, param: 'amp' }
                    });
                }});
            } else if (track.type === 'pattern') {
                items.push({ separator: true });
                items.push({ label: '✏️ Edit in Pattern Editor', shortcut: 'E', action: () => {
                    vscode.postMessage({ command: 'openPatternEditor', name: track.name });
                }});
            } else if (track.type === 'melody') {
                items.push({ separator: true });
                items.push({ label: '✏️ Edit in Melody Editor', shortcut: 'E', action: () => {
                    vscode.postMessage({ command: 'openMelodyEditor', name: track.name });
                }});
            }

            // Fun Easter egg items (with low probability)
            const funItems = [
                { label: '🎲 Randomize Everything', action: () => showFunMessage('Nice try! But chaos is not a feature... yet.') },
                { label: '🚀 Boost to 11', action: () => showFunMessage('🎸 THIS ONE GOES TO ELEVEN!') },
                { label: '🔮 Predict the Hit', action: () => showFunMessage('🎱 Signs point to: "Needs more cowbell"') },
                { label: '🎰 Lucky Drop', action: () => showFunMessage('🎵 You won: Inspiration! (non-transferable)') },
                { label: '☕ Coffee Break', action: () => showFunMessage('☕ The code is compiling... just kidding, go make music!') },
                { label: '🌈 RGB Mode', action: () => { triggerRGBMode(); } },
                { label: '🎤 Vibe Check', action: () => showVibeCheckResult() },
            ];

            // 20% chance to show a fun item
            if (Math.random() < 0.2) {
                items.push({ separator: true });
                const funItem = funItems[Math.floor(Math.random() * funItems.length)];
                items.push(funItem);
            }

            items.forEach(item => {
                if (item.separator) {
                    const sep = document.createElement('div');
                    sep.className = 'context-menu-separator';
                    contextMenuEl.appendChild(sep);
                } else {
                    const menuItem = document.createElement('div');
                    menuItem.className = 'context-menu-item';
                    menuItem.innerHTML = \`\${item.label}\${item.shortcut ? \`<span class="shortcut">\${item.shortcut}</span>\` : ''}\`;
                    menuItem.addEventListener('click', () => {
                        item.action();
                        hideContextMenu();
                    });
                    contextMenuEl.appendChild(menuItem);
                }
            });

            document.body.appendChild(contextMenuEl);

            // Adjust position if off-screen
            const rect = contextMenuEl.getBoundingClientRect();
            if (rect.right > window.innerWidth) {
                contextMenuEl.style.left = (window.innerWidth - rect.width - 10) + 'px';
            }
            if (rect.bottom > window.innerHeight) {
                contextMenuEl.style.top = (window.innerHeight - rect.height - 10) + 'px';
            }

            // Close on click outside
            setTimeout(() => {
                document.addEventListener('click', hideContextMenu, { once: true });
            }, 0);
        }

        function hideContextMenu() {
            if (contextMenuEl) {
                contextMenuEl.remove();
                contextMenuEl = null;
            }
        }

        // Fun message popup
        function showFunMessage(message) {
            const popup = document.createElement('div');
            popup.className = 'fun-popup';
            popup.textContent = message;
            popup.style.cssText = \`
                position: fixed;
                top: 50%;
                left: 50%;
                transform: translate(-50%, -50%) scale(0);
                background: linear-gradient(135deg, var(--bg-secondary), var(--bg-tertiary));
                border: 2px solid var(--accent-purple);
                border-radius: 12px;
                padding: 20px 30px;
                font-size: 16px;
                color: var(--text-primary);
                z-index: 10000;
                box-shadow: 0 10px 40px rgba(0,0,0,0.5);
                animation: popIn 0.3s ease forwards;
                text-align: center;
            \`;
            document.body.appendChild(popup);

            // Add animation keyframes
            const style = document.createElement('style');
            style.textContent = \`
                @keyframes popIn {
                    0% { transform: translate(-50%, -50%) scale(0); opacity: 0; }
                    70% { transform: translate(-50%, -50%) scale(1.1); }
                    100% { transform: translate(-50%, -50%) scale(1); opacity: 1; }
                }
                @keyframes popOut {
                    0% { transform: translate(-50%, -50%) scale(1); opacity: 1; }
                    100% { transform: translate(-50%, -50%) scale(0); opacity: 0; }
                }
            \`;
            document.head.appendChild(style);

            setTimeout(() => {
                popup.style.animation = 'popOut 0.2s ease forwards';
                setTimeout(() => {
                    popup.remove();
                    style.remove();
                }, 200);
            }, 2000);
        }

        // RGB mode Easter egg
        function triggerRGBMode() {
            const container = document.querySelector('.main-container');
            if (!container) return;

            container.style.animation = 'rgbShift 2s ease';
            const style = document.createElement('style');
            style.id = 'rgb-mode';
            style.textContent = \`
                @keyframes rgbShift {
                    0% { filter: hue-rotate(0deg); }
                    25% { filter: hue-rotate(90deg); }
                    50% { filter: hue-rotate(180deg); }
                    75% { filter: hue-rotate(270deg); }
                    100% { filter: hue-rotate(360deg); }
                }
            \`;
            document.head.appendChild(style);

            showFunMessage('🌈 RGB GAMING MODE ACTIVATED!');

            setTimeout(() => {
                container.style.animation = '';
                style.remove();
            }, 2000);
        }

        // Vibe check result
        function showVibeCheckResult() {
            const vibes = [
                '✨ Immaculate vibes detected!',
                '🔥 Your vibe is absolutely fire!',
                '😎 Cool vibes, certified chill.',
                '🎸 Rock star energy confirmed!',
                '🕺 Disco ball levels of groove!',
                '🚀 Vibes are out of this world!',
                '🎹 Classical genius vibes!',
                '🥁 Drum machine energy!',
                '🎷 Smooth jazz vibes detected!',
                '🤖 Daft Punk-level vibes!',
            ];
            const result = vibes[Math.floor(Math.random() * vibes.length)];
            showFunMessage(result);
        }

        // Konami code Easter egg
        const konamiCode = ['ArrowUp', 'ArrowUp', 'ArrowDown', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ArrowLeft', 'ArrowRight', 'KeyB', 'KeyA'];
        let konamiIndex = 0;

        document.addEventListener('keydown', (e) => {
            if (e.code === konamiCode[konamiIndex]) {
                konamiIndex++;
                if (konamiIndex === konamiCode.length) {
                    // Konami code entered!
                    triggerKonamiEasterEgg();
                    konamiIndex = 0;
                }
            } else {
                konamiIndex = 0;
            }
        });

        function triggerKonamiEasterEgg() {
            showFunMessage('🎮 +30 EXTRA VIBES! You found the secret code!');

            // Make everything bounce
            const headers = document.querySelectorAll('.track-header');
            headers.forEach((header, i) => {
                setTimeout(() => {
                    header.style.animation = 'bounce 0.5s ease';
                    setTimeout(() => {
                        header.style.animation = '';
                    }, 500);
                }, i * 50);
            });

            // Add bounce animation
            const style = document.createElement('style');
            style.textContent = \`
                @keyframes bounce {
                    0%, 100% { transform: translateY(0); }
                    50% { transform: translateY(-10px); }
                }
            \`;
            document.head.appendChild(style);
            setTimeout(() => style.remove(), 3000);
        }

        // Toggle expand state for a track
        function toggleExpand(trackId) {
            if (expandedTracks.has(trackId)) {
                expandedTracks.delete(trackId);
            } else {
                expandedTracks.add(trackId);
            }
            // Re-render headers and tracks
            renderTrackHeaders();
            render();
        }

        function updatePlayheadPosition() {
            const beat = getDisplayBeat();
            const absoluteX = beat * pixelsPerBeat;
            const rulerX = absoluteX - scrollLeft;

            // Use transform for GPU-accelerated smooth animation
            rulerPlayhead.style.transform = \`translateX(\${rulerX}px)\`;
            rulerPlayhead.style.display = rulerX >= -10 ? 'block' : 'none';

            tracksPlayhead.style.transform = \`translateX(\${absoluteX}px)\`;
        }

        function updateTransportDisplay() {
            const beat = getDisplayBeat();
            const bpm = state.transport?.bpm || 120;
            const running = state.transport?.running || false;
            const beatsPerBar = state.timeSignature?.numerator || 4;
            const looping = state.transport?.loop_beats != null;

            // Update time display
            const bar = Math.floor(beat / beatsPerBar) + 1;
            const beatInBar = Math.floor(beat % beatsPerBar) + 1;
            const sub = Math.floor((beat % 1) * 4) + 1;
            transportTime.textContent = \`\${bar}:\${beatInBar}:\${sub}\`;

            // Update BPM
            bpmValue.textContent = bpm.toFixed(1);

            // Update play button
            playBtn.textContent = running ? '⏸' : '▶';
            playBtn.classList.toggle('active', running);

            // Update playhead
            updatePlayheadPosition();

            // Auto-scroll to follow playhead
            if (running) {
                const displayBeat = getDisplayBeat();
                const playheadX = displayBeat * pixelsPerBeat;
                const viewWidth = tracksContainer.clientWidth;
                const viewRight = scrollLeft + viewWidth * 0.8;
                const viewLeft = scrollLeft + viewWidth * 0.2;

                // If looping and beat jumps back, instantly scroll to start
                if (looping && displayBeat < lastBeat - 1) {
                    tracksContainer.scrollLeft = 0;
                } else if (playheadX > viewRight) {
                    tracksContainer.scrollLeft = playheadX - viewWidth * 0.5;
                } else if (playheadX < viewLeft && scrollLeft > 0) {
                    tracksContainer.scrollLeft = Math.max(0, playheadX - viewWidth * 0.5);
                }
                lastBeat = displayBeat;
            }
        }

        // Vibe status updates with personality
        const vibeStates = {
            idle: { emoji: '😴', text: 'Napping', className: '' },
            ready: { emoji: '🎵', text: 'Ready', className: '' },
            chill: { emoji: '😎', text: 'Chill', className: 'vibing' },
            vibing: { emoji: '🎸', text: 'Vibing', className: 'vibing' },
            grooving: { emoji: '🕺', text: 'Grooving', className: 'vibing' },
            fire: { emoji: '🔥', text: 'ON FIRE', className: 'fire' },
            legendary: { emoji: '⚡', text: 'LEGENDARY', className: 'fire' },
        };

        const grooveLabels = ['zen', 'chill', 'smooth', 'groovy', 'hype', 'FIRE'];

        function updateVibeStatus() {
            const vibeStatus = document.getElementById('vibeStatus');
            const grooveMeter = document.getElementById('grooveMeter');
            const grooveLabel = document.getElementById('grooveLabel');
            const grooveBars = grooveMeter?.querySelectorAll('.groove-bar');

            if (!vibeStatus || !grooveMeter) return;

            const running = state.transport?.running || false;
            const trackCount = state.tracks?.length || 0;
            const activeCount = (state.tracks || []).filter(t => t.active).length;
            const bpm = state.transport?.bpm || 120;

            // Calculate "intensity" based on activity
            let intensity = 0;
            if (running) intensity += 1;
            if (activeCount > 0) intensity += 1;
            if (activeCount > 3) intensity += 1;
            if (activeCount > 6) intensity += 1;
            if (bpm > 140) intensity += 1;
            if (bpm > 160) intensity += 1;

            // Determine vibe state
            let vibeState;
            if (!running && trackCount === 0) {
                vibeState = vibeStates.idle;
            } else if (!running) {
                vibeState = vibeStates.ready;
            } else if (intensity <= 1) {
                vibeState = vibeStates.chill;
            } else if (intensity <= 2) {
                vibeState = vibeStates.vibing;
            } else if (intensity <= 4) {
                vibeState = vibeStates.grooving;
            } else if (intensity <= 5) {
                vibeState = vibeStates.fire;
            } else {
                vibeState = vibeStates.legendary;
            }

            // Update vibe status display
            const emojiEl = vibeStatus.querySelector('.vibe-emoji');
            const textEl = vibeStatus.querySelector('.vibe-text');
            if (emojiEl) emojiEl.textContent = vibeState.emoji;
            if (textEl) textEl.textContent = vibeState.text;
            vibeStatus.className = 'vibe-status ' + vibeState.className;

            // Update groove meter
            const grooveLevel = Math.min(5, Math.floor(intensity));
            if (grooveLabel) {
                grooveLabel.textContent = grooveLabels[grooveLevel];
            }

            // Animate groove bars based on beat
            if (grooveBars && running) {
                const beat = getDisplayBeat();
                grooveBars.forEach((bar, i) => {
                    const baseHeight = 3 + (grooveLevel * 1.5);
                    const variation = Math.sin((beat * 2 + i * 0.7) * Math.PI) * 4;
                    bar.style.height = Math.max(2, baseHeight + variation) + 'px';
                    bar.style.background = intensity > 4 ? 'var(--accent-orange)' : 'var(--accent-green)';
                });
            } else if (grooveBars) {
                grooveBars.forEach((bar, i) => {
                    bar.style.height = (3 + i % 3) + 'px';
                    bar.style.background = 'var(--text-muted)';
                });
            }
        }

        function startAnimation() {
            let lastRenderTime = 0;
            let lastVibeUpdate = 0;
            function animate(timestamp) {
                // Always update display beat for smooth playhead
                updateDisplayBeat();

                // Update playhead position every frame for smoothness
                updatePlayheadPosition();

                if (state.transport?.running) {
                    // Update transport display (time, bar:beat)
                    updateTransportDisplayText();

                    // Update vibe status periodically
                    if (timestamp - lastVibeUpdate > 100) {
                        updateVibeStatus();
                        lastVibeUpdate = timestamp;
                    }

                    // Re-render tracks less frequently (for any active clip indicators)
                    if (timestamp - lastRenderTime > 100) {
                        lastRenderTime = timestamp;
                    }
                }
                animationFrame = requestAnimationFrame(animate);
            }
            animate(0);
        }

        // Separate function for just updating the text display (not playhead)
        function updateTransportDisplayText() {
            const beat = getDisplayBeat();
            const bpm = state.transport?.bpm || 120;
            const beatsPerBar = state.timeSignature?.numerator || 4;

            // Update time display
            const bar = Math.floor(beat / beatsPerBar) + 1;
            const beatInBar = Math.floor(beat % beatsPerBar) + 1;
            const sub = Math.floor((beat % 1) * 4) + 1;
            transportTime.textContent = \`\${bar}:\${beatInBar}:\${sub}\`;

            // Auto-scroll to follow playhead
            const playheadX = beat * pixelsPerBeat;
            const viewWidth = tracksContainer.clientWidth;
            const viewRight = scrollLeft + viewWidth * 0.8;
            const viewLeft = scrollLeft + viewWidth * 0.2;

            if (playheadX > viewRight) {
                tracksContainer.scrollLeft = playheadX - viewWidth * 0.5;
            } else if (playheadX < viewLeft && scrollLeft > 0) {
                tracksContainer.scrollLeft = Math.max(0, playheadX - viewWidth * 0.5);
            }
        }

        function fitToView() {
            const maxBeat = getMaxBeat();
            const viewWidth = tracksContainer.clientWidth;
            pixelsPerBeat = Math.max(10, Math.min(100, (viewWidth - 50) / maxBeat));
            zoomSlider.value = pixelsPerBeat;
            tracksContainer.scrollLeft = 0;
            render();
        }

        function handleCanvasClick(e) {
            const clipData = getClipAndTrackAtPosition(e);
            if (clipData) {
                const { clip, track } = clipData;
                // Just highlight the clip, don't show details panel
                selectedClip = { clip, track };
                render();
            } else {
                // Click on empty space - deselect and hide details panel
                selectedClip = null;
                hideDetailsPanel();
                render();
            }
        }

        function handleCanvasDoubleClick(e) {
            const clip = getClipAtPosition(e);
            if (clip) {
                if (clip.sourceLocation && clip.sourceLocation.file && clip.sourceLocation.line) {
                    vscode.postMessage({ command: 'goToSource', sourceLocation: clip.sourceLocation });
                } else {
                    // Show a message if source location is not available
                    vscode.postMessage({ command: 'showWarning', message: 'Source location not available for ' + clip.name });
                }
            }
        }

        function handleCanvasContextMenu(e) {
            e.preventDefault();
            const clipData = getClipAndTrackAtPosition(e);
            if (clipData) {
                const { clip, track } = clipData;
                showClipContextMenu(e.clientX, e.clientY, clip, track);
            }
        }

        function showClipContextMenu(x, y, clip, track) {
            hideContextMenu();

            contextMenuEl = document.createElement('div');
            contextMenuEl.className = 'context-menu';
            contextMenuEl.style.left = x + 'px';
            contextMenuEl.style.top = y + 'px';

            const items = [];

            // Go to source
            if (clip.sourceLocation && clip.sourceLocation.file) {
                items.push({ label: '📄 Go to Source', shortcut: 'Enter', action: () => {
                    vscode.postMessage({ command: 'goToSource', sourceLocation: clip.sourceLocation });
                }});
            } else {
                items.push({ label: '📄 Go to Source', shortcut: '', disabled: true, action: () => {
                    vscode.postMessage({ command: 'showWarning', message: 'Source location not available for ' + clip.name });
                }});
            }

            // Show details
            items.push({ label: '🔍 Show Details', shortcut: 'I', action: () => {
                selectClip(clip, track);
            }});

            // Type-specific actions
            if (clip.type === 'pattern') {
                items.push({ separator: true });
                items.push({ label: clip.active ? '⏹ Stop Pattern' : '▶ Play Pattern', action: () => {
                    vscode.postMessage({ command: clip.active ? 'stopPattern' : 'startPattern', name: clip.name });
                }});
                items.push({ label: '✏️ Edit in Pattern Editor', shortcut: 'E', action: () => {
                    vscode.postMessage({ command: 'openPatternEditor', name: clip.name });
                }});
            } else if (clip.type === 'melody') {
                items.push({ separator: true });
                items.push({ label: clip.active ? '⏹ Stop Melody' : '▶ Play Melody', action: () => {
                    vscode.postMessage({ command: clip.active ? 'stopMelody' : 'startMelody', name: clip.name });
                }});
                items.push({ label: '✏️ Edit in Melody Editor', shortcut: 'E', action: () => {
                    vscode.postMessage({ command: 'openMelodyEditor', name: clip.name });
                }});
            }

            items.forEach(item => {
                if (item.separator) {
                    const sep = document.createElement('div');
                    sep.className = 'context-menu-separator';
                    contextMenuEl.appendChild(sep);
                } else {
                    const menuItem = document.createElement('div');
                    menuItem.className = 'context-menu-item' + (item.disabled ? ' disabled' : '');
                    menuItem.innerHTML = \`\${item.label}\${item.shortcut ? \`<span class="shortcut">\${item.shortcut}</span>\` : ''}\`;
                    if (!item.disabled) {
                        menuItem.addEventListener('click', () => {
                            item.action();
                            hideContextMenu();
                        });
                    }
                    contextMenuEl.appendChild(menuItem);
                }
            });

            document.body.appendChild(contextMenuEl);

            // Adjust position if off-screen
            const rect = contextMenuEl.getBoundingClientRect();
            if (rect.right > window.innerWidth) {
                contextMenuEl.style.left = (window.innerWidth - rect.width - 10) + 'px';
            }
            if (rect.bottom > window.innerHeight) {
                contextMenuEl.style.top = (window.innerHeight - rect.height - 10) + 'px';
            }

            // Close on click outside
            setTimeout(() => {
                document.addEventListener('click', hideContextMenu, { once: true });
            }, 0);
        }

        // Details panel functions
        function selectClip(clip, track) {
            selectedClip = { clip, track };
            showDetailsPanel(clip, track);
        }

        function showDetailsPanel(clip, track) {
            const panel = document.getElementById('detailsPanel');
            const nameEl = document.getElementById('detailsName');
            const typeEl = document.getElementById('detailsType');
            const contentEl = document.getElementById('detailsContent');

            nameEl.textContent = clip.name;
            typeEl.textContent = clip.type;
            typeEl.className = 'details-panel-type ' + clip.type;

            // Build content based on clip type
            let html = '';

            // Basic info section
            html += '<div class="details-section">';
            html += '<div class="details-section-title">Info</div>';

            const duration = (clip.endBeat - clip.startBeat);
            const bars = duration / (state.timeSignature?.numerator || 4);
            const startBar = Math.floor(clip.startBeat / (state.timeSignature?.numerator || 4)) + 1;
            const startBeatInBar = (clip.startBeat % (state.timeSignature?.numerator || 4)) + 1;

            html += \`<div class="details-row"><span class="label">Duration</span><span class="value">\${duration.toFixed(1)} beats</span></div>\`;
            html += \`<div class="details-row"><span class="label">Length</span><span class="value">\${bars.toFixed(1)} bars</span></div>\`;
            html += \`<div class="details-row"><span class="label">Start</span><span class="value">Bar \${startBar}, Beat \${startBeatInBar.toFixed(1)}</span></div>\`;
            html += \`<div class="details-row"><span class="label">Status</span><span class="value" style="color: \${clip.active ? 'var(--accent-green)' : 'var(--text-muted)'}">\${clip.active ? 'Playing' : 'Stopped'}</span></div>\`;
            html += '</div>';

            // Type-specific content
            if (clip.type === 'pattern' && clip.stepPattern) {
                html += '<div class="details-section">';
                html += '<div class="details-section-title">Pattern</div>';

                const stepCount = clip.stepPattern.replace(/[|]/g, '').length;
                const hitCount = (clip.stepPattern.match(/[xXoO]/g) || []).length;
                html += \`<div class="details-row"><span class="label">Steps</span><span class="value">\${hitCount}/\${stepCount} hits</span></div>\`;

                // Pattern preview with highlighting
                let patternHtml = clip.stepPattern
                    .replace(/[xXoO]/g, '<span class="hit">$&</span>')
                    .replace(/[-._]/g, '<span class="rest">$&</span>')
                    .replace(/[|]/g, '<span class="bar">|</span>');
                html += \`<div class="details-pattern-preview">\${patternHtml}</div>\`;
                html += '</div>';
            } else if (clip.type === 'melody' && clip.melodyEvents) {
                html += '<div class="details-section">';
                html += '<div class="details-section-title">Melody</div>';
                html += \`<div class="details-row"><span class="label">Notes</span><span class="value">\${clip.melodyEvents.length} events</span></div>\`;

                // Note range
                if (clip.melodyEvents.length > 0) {
                    const notes = clip.melodyEvents.map(e => e.midiNote);
                    const minNote = Math.min(...notes);
                    const maxNote = Math.max(...notes);
                    html += \`<div class="details-row"><span class="label">Range</span><span class="value">\${midiToNoteName(minNote)} - \${midiToNoteName(maxNote)}</span></div>\`;
                }
                html += '</div>';
            }

            // Source location
            html += '<div class="details-section">';
            html += '<div class="details-section-title">Source</div>';
            if (clip.sourceLocation && clip.sourceLocation.file) {
                const filename = clip.sourceLocation.file.split('/').pop() || 'Unknown';
                const line = clip.sourceLocation.line || 1;
                html += \`<div class="details-row"><span class="label">File</span><span class="value details-source-link" onclick="goToSource()">\${filename}:\${line}</span></div>\`;
            } else {
                html += '<div class="details-row"><span class="label">File</span><span class="value" style="color: var(--text-muted)">Not available</span></div>';
            }
            html += '</div>';

            // Actions
            html += '<div class="details-section">';
            html += '<div class="details-section-title">Actions</div>';
            html += '<div class="details-actions">';

            if (clip.type === 'pattern') {
                html += \`<button class="details-action-btn primary" onclick="togglePlayClip()">\${clip.active ? '⏹ Stop' : '▶ Play'}</button>\`;
                html += '<button class="details-action-btn" onclick="editClip()">✏️ Edit</button>';
            } else if (clip.type === 'melody') {
                html += \`<button class="details-action-btn primary" onclick="togglePlayClip()">\${clip.active ? '⏹ Stop' : '▶ Play'}</button>\`;
                html += '<button class="details-action-btn" onclick="editClip()">✏️ Edit</button>';
            }

            if (clip.sourceLocation && clip.sourceLocation.file) {
                html += '<button class="details-action-btn" onclick="goToSource()">📄 Source</button>';
            }

            html += '</div>';
            html += '</div>';

            contentEl.innerHTML = html;
            panel.classList.add('visible');
        }

        function hideDetailsPanel() {
            const panel = document.getElementById('detailsPanel');
            panel.classList.remove('visible');
            selectedClip = null;
        }

        // Helper functions for details panel buttons
        window.goToSource = function() {
            if (selectedClip?.clip?.sourceLocation) {
                vscode.postMessage({ command: 'goToSource', sourceLocation: selectedClip.clip.sourceLocation });
            }
        };

        window.togglePlayClip = function() {
            if (!selectedClip?.clip) return;
            const { clip } = selectedClip;
            if (clip.type === 'pattern') {
                vscode.postMessage({ command: clip.active ? 'stopPattern' : 'startPattern', name: clip.name });
            } else if (clip.type === 'melody') {
                vscode.postMessage({ command: clip.active ? 'stopMelody' : 'startMelody', name: clip.name });
            }
        };

        window.editClip = function() {
            if (!selectedClip?.clip) return;
            const { clip } = selectedClip;
            if (clip.type === 'pattern') {
                vscode.postMessage({ command: 'openPatternEditor', name: clip.name });
            } else if (clip.type === 'melody') {
                vscode.postMessage({ command: 'openMelodyEditor', name: clip.name });
            }
        };

        function midiToNoteName(midi) {
            const noteNames = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
            const octave = Math.floor(midi / 12) - 1;
            const note = noteNames[midi % 12];
            return note + octave;
        }

        function handleCanvasMouseMove(e) {
            const clipData = getClipAndTrackAtPosition(e);
            if (clipData) {
                const { clip, track } = clipData;
                const tooltip = clipTooltip;
                const duration = (clip.endBeat - clip.startBeat);
                const bars = duration / (state.timeSignature?.numerator || 4);

                // Build enhanced tooltip content
                let titleHtml = \`\${clip.name}<span class="clip-tooltip-type \${clip.type}">\${clip.type}</span>\`;
                tooltip.querySelector('.clip-tooltip-title').innerHTML = titleHtml;

                // Build info rows
                let infoHtml = '';

                // Duration
                infoHtml += \`<div><span class="label">Duration</span><span class="value">\${duration.toFixed(1)} beats (\${bars.toFixed(1)} bars)</span></div>\`;

                // Position
                const startBar = Math.floor(clip.startBeat / (state.timeSignature?.numerator || 4)) + 1;
                const startBeatInBar = (clip.startBeat % (state.timeSignature?.numerator || 4)) + 1;
                infoHtml += \`<div><span class="label">Start</span><span class="value">Bar \${startBar}, Beat \${startBeatInBar.toFixed(1)}</span></div>\`;

                // Type-specific info
                if (clip.type === 'pattern' && clip.stepPattern) {
                    const stepCount = clip.stepPattern.replace(/[|]/g, '').length;
                    const hitCount = (clip.stepPattern.match(/[xXoO]/g) || []).length;
                    infoHtml += \`<div><span class="label">Steps</span><span class="value">\${hitCount}/\${stepCount} hits</span></div>\`;
                } else if (clip.type === 'melody' && clip.melodyEvents) {
                    const noteCount = clip.melodyEvents.length;
                    infoHtml += \`<div><span class="label">Notes</span><span class="value">\${noteCount} events</span></div>\`;
                }

                // Status
                if (clip.active) {
                    infoHtml += \`<div><span class="label">Status</span><span class="value" style="color: var(--accent-green);">Playing</span></div>\`;
                }

                tooltip.querySelector('.clip-tooltip-info').innerHTML = infoHtml;

                // Position tooltip
                tooltip.style.left = (e.clientX + 12) + 'px';
                tooltip.style.top = (e.clientY + 12) + 'px';

                // Adjust if off-screen
                const tooltipRect = tooltip.getBoundingClientRect();
                if (e.clientX + tooltipRect.width + 20 > window.innerWidth) {
                    tooltip.style.left = (e.clientX - tooltipRect.width - 12) + 'px';
                }
                if (e.clientY + tooltipRect.height + 20 > window.innerHeight) {
                    tooltip.style.top = (e.clientY - tooltipRect.height - 12) + 'px';
                }

                tooltip.classList.add('visible');
                tracksCanvas.style.cursor = 'pointer';
            } else {
                if (clipTooltip) clipTooltip.classList.remove('visible');
                tracksCanvas.style.cursor = 'default';
            }
        }

        function getClipAndTrackAtPosition(e) {
            const clip = getClipAtPosition(e);
            if (!clip) return null;

            const rect = tracksCanvas.getBoundingClientRect();
            const y = e.clientY - rect.top;
            const visibleTracks = getVisibleTracks();
            const trackIndex = Math.floor(y / trackHeight);

            if (trackIndex >= 0 && trackIndex < visibleTracks.length) {
                return { clip, track: visibleTracks[trackIndex] };
            }
            return { clip, track: null };
        }

        function getClipAtPosition(e) {
            const rect = tracksCanvas.getBoundingClientRect();
            const x = e.clientX - rect.left;
            const y = e.clientY - rect.top;

            const visibleTracks = getVisibleTracks();
            const trackIndex = Math.floor(y / trackHeight);
            const beat = x / pixelsPerBeat;

            if (trackIndex >= 0 && trackIndex < visibleTracks.length) {
                const track = visibleTracks[trackIndex];
                for (const clip of track.clips || []) {
                    if (beat >= clip.startBeat && beat <= clip.endBeat) {
                        return clip;
                    }
                }
            }
            return null;
        }

        // =====================================================================
        // Automation Functions
        // =====================================================================

        function renderAutomationLanes() {
            const list = document.getElementById('automationLanesList');
            const lanes = state.automationLanes || [];

            if (!list) return;

            // Keep the add button, remove other children
            const addBtn = document.getElementById('addAutomationBtn');
            list.innerHTML = '';
            if (addBtn) list.appendChild(addBtn);

            lanes.forEach(lane => {
                const header = document.createElement('div');
                header.className = 'automation-lane-header' + (selectedLaneId === lane.id ? ' selected' : '');
                header.dataset.laneId = lane.id;

                header.innerHTML = \`
                    <div class="automation-lane-color" style="background: \${lane.color}"></div>
                    <div class="automation-lane-info">
                        <div class="automation-lane-name">\${lane.target.param}</div>
                        <div class="automation-lane-target">\${lane.target.type}: \${lane.target.name}</div>
                    </div>
                    <div class="automation-lane-actions">
                        <button class="automation-lane-btn" data-action="code" title="Generate Code">⟨/⟩</button>
                        <button class="automation-lane-btn" data-action="clear" title="Clear Points">⟳</button>
                        <button class="automation-lane-btn delete" data-action="delete" title="Remove Lane">✕</button>
                    </div>
                \`;

                // Click to select
                header.addEventListener('click', (e) => {
                    if (!e.target.closest('.automation-lane-btn')) {
                        selectedLaneId = lane.id;
                        selectedPointId = null;
                        renderAutomationLanes();
                        renderAutomationCanvas();
                    }
                });

                // Button actions
                header.querySelectorAll('.automation-lane-btn').forEach(btn => {
                    btn.addEventListener('click', (e) => {
                        e.stopPropagation();
                        const action = btn.dataset.action;
                        if (action === 'delete') {
                            vscode.postMessage({ command: 'removeAutomationLane', laneId: lane.id });
                        } else if (action === 'clear') {
                            vscode.postMessage({ command: 'clearAutomationLane', laneId: lane.id });
                        } else if (action === 'code') {
                            vscode.postMessage({ command: 'generateAutomationCode', laneId: lane.id });
                        }
                    });
                });

                list.appendChild(header);
            });

            // Auto-select first lane if none selected
            if (lanes.length > 0 && !selectedLaneId) {
                selectedLaneId = lanes[0].id;
                renderAutomationLanes();
            }
        }

        function renderAutomationCanvas() {
            const canvasArea = document.getElementById('automationCanvasArea');
            const canvas = document.getElementById('automationCanvas');
            const ctx = canvas.getContext('2d');

            if (!canvasArea || !canvas) return;

            const rect = canvasArea.getBoundingClientRect();
            const dpr = window.devicePixelRatio || 1;

            // Size canvas
            const width = rect.width;
            const height = rect.height || 150;

            canvas.width = width * dpr;
            canvas.height = height * dpr;
            canvas.style.width = width + 'px';
            canvas.style.height = height + 'px';
            ctx.scale(dpr, dpr);

            // Clear
            ctx.fillStyle = '#1a1a1a';
            ctx.fillRect(0, 0, width, height);

            // Get selected lane
            const lane = (state.automationLanes || []).find(l => l.id === selectedLaneId);

            // Draw grid
            const maxBeats = state.maxLoopBeats || 16;
            const beatsPerBar = state.timeSignature?.numerator || 4;

            // Vertical grid (beats)
            for (let beat = 0; beat <= maxBeats; beat++) {
                const x = (beat / maxBeats) * width;
                ctx.strokeStyle = beat % beatsPerBar === 0 ? '#3a3a3a' : '#2a2a2a';
                ctx.lineWidth = beat % beatsPerBar === 0 ? 1 : 0.5;
                ctx.beginPath();
                ctx.moveTo(x, 0);
                ctx.lineTo(x, height);
                ctx.stroke();
            }

            // Horizontal grid (values)
            for (let i = 0; i <= 4; i++) {
                const y = (i / 4) * height;
                ctx.strokeStyle = '#2a2a2a';
                ctx.lineWidth = 0.5;
                ctx.beginPath();
                ctx.moveTo(0, y);
                ctx.lineTo(width, y);
                ctx.stroke();
            }

            if (!lane) {
                // No lane selected - show message
                ctx.fillStyle = '#666';
                ctx.font = '12px -apple-system, sans-serif';
                ctx.textAlign = 'center';
                ctx.fillText('Select or add an automation lane', width / 2, height / 2);
                return;
            }

            const points = [...(lane.points || [])].sort((a, b) => a.beat - b.beat);

            if (points.length === 0) {
                ctx.fillStyle = '#666';
                ctx.font = '11px -apple-system, sans-serif';
                ctx.textAlign = 'center';
                ctx.fillText('Click to add automation points', width / 2, height / 2);
                return;
            }

            // Draw filled area under curve
            if (points.length > 1) {
                ctx.fillStyle = lane.color + '30'; // Add alpha
                ctx.beginPath();
                ctx.moveTo((points[0].beat / maxBeats) * width, height);

                for (let i = 0; i < points.length; i++) {
                    const p = points[i];
                    const x = (p.beat / maxBeats) * width;
                    const y = height - (p.value * height);

                    if (i === 0) {
                        ctx.lineTo(x, y);
                    } else {
                        const prevP = points[i - 1];
                        const prevX = (prevP.beat / maxBeats) * width;
                        const prevY = height - (prevP.value * height);

                        drawCurveSegment(ctx, prevX, prevY, x, y, prevP.curveType);
                    }
                }

                ctx.lineTo((points[points.length - 1].beat / maxBeats) * width, height);
                ctx.closePath();
                ctx.fill();
            }

            // Draw curve line
            if (points.length > 1) {
                ctx.strokeStyle = lane.color;
                ctx.lineWidth = 2;
                ctx.beginPath();

                for (let i = 0; i < points.length; i++) {
                    const p = points[i];
                    const x = (p.beat / maxBeats) * width;
                    const y = height - (p.value * height);

                    if (i === 0) {
                        ctx.moveTo(x, y);
                    } else {
                        const prevP = points[i - 1];
                        const prevX = (prevP.beat / maxBeats) * width;
                        const prevY = height - (prevP.value * height);

                        drawCurveSegment(ctx, prevX, prevY, x, y, prevP.curveType);
                    }
                }

                ctx.stroke();
            }

            // Draw points
            points.forEach(p => {
                const x = (p.beat / maxBeats) * width;
                const y = height - (p.value * height);
                const isSelected = p.id === selectedPointId;

                // Point circle
                ctx.beginPath();
                ctx.arc(x, y, isSelected ? 7 : 5, 0, Math.PI * 2);
                ctx.fillStyle = isSelected ? '#fff' : lane.color;
                ctx.fill();

                if (isSelected) {
                    ctx.strokeStyle = lane.color;
                    ctx.lineWidth = 2;
                    ctx.stroke();
                }

                // Value label
                if (isSelected) {
                    const valueText = (p.value * 100).toFixed(0) + '%';
                    ctx.font = '10px -apple-system, sans-serif';
                    ctx.fillStyle = '#fff';
                    ctx.textAlign = 'center';
                    ctx.fillText(valueText, x, y - 12);
                }
            });

            // Draw playhead
            const currentBeat = getDisplayBeat();
            const playheadX = (currentBeat / maxBeats) * width;
            ctx.strokeStyle = '#ff6b6b';
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(playheadX, 0);
            ctx.lineTo(playheadX, height);
            ctx.stroke();
        }

        function drawCurveSegment(ctx, x1, y1, x2, y2, curveType) {
            switch (curveType) {
                case 'linear':
                    ctx.lineTo(x2, y2);
                    break;
                case 'step':
                    ctx.lineTo(x2, y1);
                    ctx.lineTo(x2, y2);
                    break;
                case 'smooth':
                    const cp1x = x1 + (x2 - x1) * 0.5;
                    const cp1y = y1;
                    const cp2x = x1 + (x2 - x1) * 0.5;
                    const cp2y = y2;
                    ctx.bezierCurveTo(cp1x, cp1y, cp2x, cp2y, x2, y2);
                    break;
                case 'exponential':
                    const cpx = x1 + (x2 - x1) * 0.7;
                    const cpy = y2;
                    ctx.quadraticCurveTo(cpx, cpy, x2, y2);
                    break;
                default:
                    ctx.lineTo(x2, y2);
            }
        }

        function showAutomationPicker(rect) {
            const picker = document.getElementById('automationPicker');
            const list = document.getElementById('automationPickerList');

            // Position picker
            picker.style.left = rect.left + 'px';
            picker.style.top = (rect.bottom + 4) + 'px';

            // Populate list
            renderAutomationTargets('');

            picker.classList.add('visible');
            document.getElementById('automationPickerSearch').focus();
        }

        function hideAutomationPicker() {
            document.getElementById('automationPicker').classList.remove('visible');
            document.getElementById('automationPickerSearch').value = '';
        }

        function filterAutomationTargets(query) {
            renderAutomationTargets(query.toLowerCase());
        }

        function renderAutomationTargets(query) {
            const list = document.getElementById('automationPickerList');
            list.innerHTML = '';

            const targets = state.availableTargets || [];
            const filtered = targets.filter(t => {
                if (!query) return true;
                const text = \`\${t.type} \${t.name} \${t.param}\`.toLowerCase();
                return text.includes(query);
            });

            if (filtered.length === 0) {
                list.innerHTML = '<div style="padding: 12px; color: var(--text-muted); text-align: center;">No parameters found</div>';
                return;
            }

            filtered.slice(0, 20).forEach(target => {
                const item = document.createElement('div');
                item.className = 'automation-picker-item';
                item.innerHTML = \`
                    <span class="automation-picker-item-type \${target.type}">\${target.type}</span>
                    <span>\${target.name}.\${target.param}</span>
                \`;
                item.addEventListener('click', () => {
                    vscode.postMessage({
                        command: 'addAutomationLane',
                        target: target
                    });
                    hideAutomationPicker();
                });
                list.appendChild(item);
            });
        }

        // Automation canvas event handlers
        function handleAutomationClick(e) {
            if (isDraggingPoint) return;

            const canvas = document.getElementById('automationCanvas');
            const rect = canvas.getBoundingClientRect();
            const x = e.clientX - rect.left;
            const y = e.clientY - rect.top;
            const width = rect.width;
            const height = rect.height;

            const maxBeats = state.maxLoopBeats || 16;
            const beat = (x / width) * maxBeats;
            const value = 1 - (y / height);

            // Check if clicking on existing point
            const lane = (state.automationLanes || []).find(l => l.id === selectedLaneId);
            if (lane) {
                const clickedPoint = getPointAtPosition(lane, x, y, width, height);
                if (clickedPoint) {
                    selectedPointId = clickedPoint.id;
                    renderAutomationCanvas();
                    return;
                }
            }

            // Add new point
            if (selectedLaneId) {
                const snappedBeat = snapBeat(beat);
                vscode.postMessage({
                    command: 'addAutomationPoint',
                    laneId: selectedLaneId,
                    beat: Math.max(0, Math.min(maxBeats, snappedBeat)),
                    value: Math.max(0, Math.min(1, value))
                });
            }
        }

        function handleAutomationMouseDown(e) {
            const canvas = document.getElementById('automationCanvas');
            const rect = canvas.getBoundingClientRect();
            const x = e.clientX - rect.left;
            const y = e.clientY - rect.top;
            const width = rect.width;
            const height = rect.height;

            const lane = (state.automationLanes || []).find(l => l.id === selectedLaneId);
            if (lane) {
                const point = getPointAtPosition(lane, x, y, width, height);
                if (point) {
                    isDraggingPoint = true;
                    selectedPointId = point.id;
                    canvas.style.cursor = 'grabbing';
                }
            }
        }

        function handleAutomationMouseMove(e) {
            if (!isDraggingPoint || !selectedPointId || !selectedLaneId) return;

            const canvas = document.getElementById('automationCanvas');
            const rect = canvas.getBoundingClientRect();
            const x = e.clientX - rect.left;
            const y = e.clientY - rect.top;
            const width = rect.width;
            const height = rect.height;

            const maxBeats = state.maxLoopBeats || 16;
            const beat = snapBeat((x / width) * maxBeats);
            const value = 1 - (y / height);

            vscode.postMessage({
                command: 'updateAutomationPoint',
                laneId: selectedLaneId,
                pointId: selectedPointId,
                beat: Math.max(0, Math.min(maxBeats, beat)),
                value: Math.max(0, Math.min(1, value))
            });
        }

        function handleAutomationMouseUp() {
            isDraggingPoint = false;
            const canvas = document.getElementById('automationCanvas');
            if (canvas) {
                canvas.style.cursor = 'crosshair';
            }
        }

        function handleAutomationDoubleClick(e) {
            const canvas = document.getElementById('automationCanvas');
            const rect = canvas.getBoundingClientRect();
            const x = e.clientX - rect.left;
            const y = e.clientY - rect.top;
            const width = rect.width;
            const height = rect.height;

            const lane = (state.automationLanes || []).find(l => l.id === selectedLaneId);
            if (lane) {
                const point = getPointAtPosition(lane, x, y, width, height);
                if (point) {
                    // Double-click on point - remove it
                    vscode.postMessage({
                        command: 'removeAutomationPoint',
                        laneId: selectedLaneId,
                        pointId: point.id
                    });
                }
            }
        }

        function handleAutomationContextMenu(e) {
            e.preventDefault();

            const canvas = document.getElementById('automationCanvas');
            const rect = canvas.getBoundingClientRect();
            const x = e.clientX - rect.left;
            const y = e.clientY - rect.top;
            const width = rect.width;
            const height = rect.height;

            const lane = (state.automationLanes || []).find(l => l.id === selectedLaneId);
            if (lane) {
                const point = getPointAtPosition(lane, x, y, width, height);
                if (point) {
                    // Show curve type selector
                    selectedPointId = point.id;
                    showCurveTypeSelector(e.clientX, e.clientY, point.curveType);
                }
            }
        }

        function getPointAtPosition(lane, x, y, width, height) {
            const maxBeats = state.maxLoopBeats || 16;
            const points = lane.points || [];
            const hitRadius = 10;

            for (const p of points) {
                const px = (p.beat / maxBeats) * width;
                const py = height - (p.value * height);
                const dist = Math.sqrt((x - px) ** 2 + (y - py) ** 2);
                if (dist <= hitRadius) {
                    return p;
                }
            }
            return null;
        }

        function snapBeat(beat) {
            if (automationGridSnap <= 0) return beat;
            return Math.round(beat / automationGridSnap) * automationGridSnap;
        }

        function showCurveTypeSelector(x, y, currentType) {
            const selector = document.getElementById('curveTypeSelector');
            selector.style.left = x + 'px';
            selector.style.top = y + 'px';

            // Update selected state
            selector.querySelectorAll('.curve-type-option').forEach(opt => {
                opt.classList.toggle('selected', opt.dataset.curve === currentType);
                opt.onclick = () => {
                    if (selectedLaneId && selectedPointId) {
                        vscode.postMessage({
                            command: 'setAutomationCurveType',
                            laneId: selectedLaneId,
                            pointId: selectedPointId,
                            curveType: opt.dataset.curve
                        });
                    }
                    selector.classList.remove('visible');
                };
            });

            selector.classList.add('visible');
        }

        // Start
        init();
        })();
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
    <title>Arrangement</title>
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
        <div class="empty-icon">🎼</div>
        <h2>Not Connected</h2>
        <p>Connect to a VibeLang runtime to see the arrangement timeline.</p>
    </div>
</body>
</html>`;
    }
    dispose() {
        ArrangementTimeline.currentPanel = undefined;
        this._panel.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
exports.ArrangementTimeline = ArrangementTimeline;
ArrangementTimeline.viewType = 'vibelang.arrangement';
//# sourceMappingURL=arrangementTimeline.js.map