"use strict";
/**
 * VibeLang State Store
 *
 * Reactive state management for the VibeLang extension.
 * Provides centralized state with event-based updates for all views.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.StateStore = void 0;
const vscode = require("vscode");
const runtimeManager_1 = require("../api/runtimeManager");
/**
 * Centralized state store for the VibeLang extension.
 * Wraps RuntimeManager and provides computed properties and selection state.
 */
class StateStore {
    constructor() {
        this._selection = null;
        // Event emitters for UI updates
        this._onSelectionChange = new vscode.EventEmitter();
        this._onTransportUpdate = new vscode.EventEmitter();
        this._onGroupsUpdate = new vscode.EventEmitter();
        this._onFullUpdate = new vscode.EventEmitter();
        this.onSelectionChange = this._onSelectionChange.event;
        this.onTransportUpdate = this._onTransportUpdate.event;
        this.onGroupsUpdate = this._onGroupsUpdate.event;
        this.onFullUpdate = this._onFullUpdate.event;
        this._runtime = new runtimeManager_1.RuntimeManager({ autoConnect: true });
        this.onStatusChange = this._runtime.onStatusChange;
        this.onError = this._runtime.onError;
        // Subscribe to state updates from runtime
        this._runtime.onStateUpdate((state) => {
            this._onFullUpdate.fire(state);
            this._onTransportUpdate.fire(state.transport);
            this._onGroupsUpdate.fire(state.groups);
        });
    }
    // ==========================================================================
    // Getters
    // ==========================================================================
    get runtime() {
        return this._runtime;
    }
    get status() {
        return this._runtime.status;
    }
    get state() {
        return this._runtime.state;
    }
    get transport() {
        return this._runtime.state?.transport ?? null;
    }
    get groups() {
        return this._runtime.state?.groups ?? [];
    }
    get voices() {
        return this._runtime.state?.voices ?? [];
    }
    get patterns() {
        return this._runtime.state?.patterns ?? [];
    }
    get melodies() {
        return this._runtime.state?.melodies ?? [];
    }
    get sequences() {
        return this._runtime.state?.sequences ?? [];
    }
    get effects() {
        return this._runtime.state?.effects ?? [];
    }
    get selection() {
        return this._selection;
    }
    // ==========================================================================
    // Selection Management
    // ==========================================================================
    select(selection) {
        this._selection = selection;
        this._onSelectionChange.fire(selection);
    }
    selectGroup(path) {
        this.select({ type: 'group', id: path });
    }
    selectVoice(name) {
        this.select({ type: 'voice', id: name });
    }
    selectPattern(name) {
        this.select({ type: 'pattern', id: name });
    }
    selectMelody(name) {
        this.select({ type: 'melody', id: name });
    }
    selectSequence(name) {
        this.select({ type: 'sequence', id: name });
    }
    selectEffect(id) {
        this.select({ type: 'effect', id });
    }
    clearSelection() {
        this.select(null);
    }
    // ==========================================================================
    // Entity Lookup
    // ==========================================================================
    getGroup(path) {
        return this.groups.find((g) => g.path === path);
    }
    getVoice(name) {
        return this.voices.find((v) => v.name === name);
    }
    getPattern(name) {
        return this.patterns.find((p) => p.name === name);
    }
    getMelody(name) {
        return this.melodies.find((m) => m.name === name);
    }
    getSequence(name) {
        return this.sequences.find((s) => s.name === name);
    }
    getEffect(id) {
        return this.effects.find((e) => e.id === id);
    }
    getSelectedEntity() {
        if (!this._selection)
            return undefined;
        switch (this._selection.type) {
            case 'group':
                return this.getGroup(this._selection.id);
            case 'voice':
                return this.getVoice(this._selection.id);
            case 'pattern':
                return this.getPattern(this._selection.id);
            case 'melody':
                return this.getMelody(this._selection.id);
            case 'sequence':
                return this.getSequence(this._selection.id);
            case 'effect':
                return this.getEffect(this._selection.id);
            default:
                return undefined;
        }
    }
    // ==========================================================================
    // Computed Properties - Group Hierarchy
    // ==========================================================================
    /**
     * Build hierarchical tree structure from flat group list.
     * Each group contains its voices, patterns, melodies, effects, and child groups.
     */
    buildGroupTree() {
        const groupMap = new Map();
        // First pass: create all group items
        for (const group of this.groups) {
            groupMap.set(group.path, {
                type: 'group',
                group,
                voices: [],
                patterns: [],
                melodies: [],
                effects: [],
                children: [],
            });
        }
        // Add voices to their groups
        for (const voice of this.voices) {
            const item = groupMap.get(voice.group_path);
            if (item) {
                item.voices.push(voice);
            }
        }
        // Add patterns to their groups
        for (const pattern of this.patterns) {
            const item = groupMap.get(pattern.group_path);
            if (item) {
                item.patterns.push(pattern);
            }
        }
        // Add melodies to their groups
        for (const melody of this.melodies) {
            const item = groupMap.get(melody.group_path);
            if (item) {
                item.melodies.push(melody);
            }
        }
        // Add effects to their groups
        for (const effect of this.effects) {
            const item = groupMap.get(effect.group_path);
            if (item) {
                item.effects.push(effect);
            }
        }
        // Build hierarchy - add children to parents
        const rootItems = [];
        for (const [path, item] of groupMap) {
            if (item.group.parent_path) {
                const parent = groupMap.get(item.group.parent_path);
                if (parent) {
                    parent.children.push(item);
                }
                else {
                    rootItems.push(item);
                }
            }
            else {
                // Root level group (main)
                rootItems.push(item);
            }
        }
        return rootItems;
    }
    // ==========================================================================
    // Activity State
    // ==========================================================================
    /**
     * Check if a pattern is currently playing.
     */
    isPatternPlaying(name) {
        const pattern = this.getPattern(name);
        return pattern?.status.state === 'playing';
    }
    /**
     * Check if a melody is currently playing.
     */
    isMelodyPlaying(name) {
        const melody = this.getMelody(name);
        return melody?.status.state === 'playing';
    }
    /**
     * Check if a sequence is currently active.
     */
    isSequenceActive(name) {
        const live = this._runtime.state?.live;
        return live?.active_sequences?.some((s) => s.name === name) ?? false;
    }
    /**
     * Check if a group has any active patterns or melodies.
     */
    isGroupActive(path) {
        // Check patterns in this group
        for (const pattern of this.patterns) {
            if (pattern.group_path === path && pattern.status.state === 'playing') {
                return true;
            }
        }
        // Check melodies in this group
        for (const melody of this.melodies) {
            if (melody.group_path === path && melody.status.state === 'playing') {
                return true;
            }
        }
        // Check child groups recursively
        for (const group of this.groups) {
            if (group.parent_path === path && this.isGroupActive(group.path)) {
                return true;
            }
        }
        return false;
    }
    /**
     * Get the loop status state for display.
     */
    getLoopStatusIcon(state) {
        switch (state) {
            case 'playing':
                return '$(play)';
            case 'queued':
                return '$(clock)';
            case 'queued_stop':
                return '$(debug-pause)';
            case 'stopped':
            default:
                return '$(debug-stop)';
        }
    }
    // ==========================================================================
    // Connection Management
    // ==========================================================================
    async connect(host, port) {
        if (host && port) {
            this._runtime.setConnection(host, port);
        }
        return this._runtime.tryConnect();
    }
    disconnect() {
        this._runtime.disconnect();
    }
    // ==========================================================================
    // Cleanup
    // ==========================================================================
    dispose() {
        this._runtime.dispose();
        this._onSelectionChange.dispose();
        this._onTransportUpdate.dispose();
        this._onGroupsUpdate.dispose();
        this._onFullUpdate.dispose();
    }
}
exports.StateStore = StateStore;
//# sourceMappingURL=stateStore.js.map