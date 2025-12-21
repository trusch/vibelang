/**
 * VibeLang State Store
 *
 * Reactive state management for the VibeLang extension.
 * Provides centralized state with event-based updates for all views.
 */

import * as vscode from 'vscode';
import {
    SessionState,
    Group,
    Voice,
    Pattern,
    Melody,
    Sequence,
    Effect,
    TransportState,
    EntitySelection,
    GroupTreeItem,
    LoopState,
} from '../api/types';
import { RuntimeManager, ConnectionStatus } from '../api/runtimeManager';

/**
 * Centralized state store for the VibeLang extension.
 * Wraps RuntimeManager and provides computed properties and selection state.
 */
export class StateStore {
    private _runtime: RuntimeManager;
    private _selection: EntitySelection | null = null;

    // Event emitters for UI updates
    private _onSelectionChange = new vscode.EventEmitter<EntitySelection | null>();
    private _onTransportUpdate = new vscode.EventEmitter<TransportState>();
    private _onGroupsUpdate = new vscode.EventEmitter<Group[]>();
    private _onFullUpdate = new vscode.EventEmitter<SessionState>();

    public readonly onSelectionChange = this._onSelectionChange.event;
    public readonly onTransportUpdate = this._onTransportUpdate.event;
    public readonly onGroupsUpdate = this._onGroupsUpdate.event;
    public readonly onFullUpdate = this._onFullUpdate.event;

    // Forward runtime events
    public readonly onStatusChange: vscode.Event<ConnectionStatus>;
    public readonly onError: vscode.Event<string>;

    constructor() {
        // Read config options
        const config = vscode.workspace.getConfiguration('vibelang');
        const connectionTimeout = config.get<number>('runtime.connectionTimeout', 5000);
        const reconnectOnDisconnect = config.get<boolean>('runtime.reconnectOnDisconnect', true);

        this._runtime = new RuntimeManager({
            autoConnect: true,
            connectionTimeout,
            reconnectOnDisconnect,
        });

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

    get runtime(): RuntimeManager {
        return this._runtime;
    }

    get status(): ConnectionStatus {
        return this._runtime.status;
    }

    get state(): SessionState | null {
        return this._runtime.state;
    }

    get transport(): TransportState | null {
        return this._runtime.state?.transport ?? null;
    }

    get groups(): Group[] {
        return this._runtime.state?.groups ?? [];
    }

    get voices(): Voice[] {
        return this._runtime.state?.voices ?? [];
    }

    get patterns(): Pattern[] {
        return this._runtime.state?.patterns ?? [];
    }

    get melodies(): Melody[] {
        return this._runtime.state?.melodies ?? [];
    }

    get sequences(): Sequence[] {
        return this._runtime.state?.sequences ?? [];
    }

    get effects(): Effect[] {
        return this._runtime.state?.effects ?? [];
    }

    get selection(): EntitySelection | null {
        return this._selection;
    }

    // ==========================================================================
    // Selection Management
    // ==========================================================================

    select(selection: EntitySelection | null) {
        this._selection = selection;
        this._onSelectionChange.fire(selection);
    }

    selectGroup(path: string) {
        this.select({ type: 'group', id: path });
    }

    selectVoice(name: string) {
        this.select({ type: 'voice', id: name });
    }

    selectPattern(name: string) {
        this.select({ type: 'pattern', id: name });
    }

    selectMelody(name: string) {
        this.select({ type: 'melody', id: name });
    }

    selectSequence(name: string) {
        this.select({ type: 'sequence', id: name });
    }

    selectEffect(id: string) {
        this.select({ type: 'effect', id });
    }

    clearSelection() {
        this.select(null);
    }

    // ==========================================================================
    // Entity Lookup
    // ==========================================================================

    getGroup(path: string): Group | undefined {
        return this.groups.find((g) => g.path === path);
    }

    getVoice(name: string): Voice | undefined {
        return this.voices.find((v) => v.name === name);
    }

    getPattern(name: string): Pattern | undefined {
        return this.patterns.find((p) => p.name === name);
    }

    getMelody(name: string): Melody | undefined {
        return this.melodies.find((m) => m.name === name);
    }

    getSequence(name: string): Sequence | undefined {
        return this.sequences.find((s) => s.name === name);
    }

    getEffect(id: string): Effect | undefined {
        return this.effects.find((e) => e.id === id);
    }

    getSelectedEntity(): Group | Voice | Pattern | Melody | Sequence | Effect | undefined {
        if (!this._selection) return undefined;

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
    buildGroupTree(): GroupTreeItem[] {
        const groupMap = new Map<string, GroupTreeItem>();

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
        const rootItems: GroupTreeItem[] = [];

        for (const [path, item] of groupMap) {
            if (item.group.parent_path) {
                const parent = groupMap.get(item.group.parent_path);
                if (parent) {
                    parent.children.push(item);
                } else {
                    rootItems.push(item);
                }
            } else {
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
    isPatternPlaying(name: string): boolean {
        const pattern = this.getPattern(name);
        return pattern?.status.state === 'playing';
    }

    /**
     * Check if a melody is currently playing.
     */
    isMelodyPlaying(name: string): boolean {
        const melody = this.getMelody(name);
        return melody?.status.state === 'playing';
    }

    /**
     * Check if a sequence is currently active.
     */
    isSequenceActive(name: string): boolean {
        const live = this._runtime.state?.live;
        return live?.active_sequences?.some((s) => s.name === name) ?? false;
    }

    /**
     * Check if a group has any active patterns or melodies.
     */
    isGroupActive(path: string): boolean {
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
    getLoopStatusIcon(state: LoopState): string {
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

    async connect(host?: string, port?: number): Promise<boolean> {
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
