/**
 * VibeLang Session Explorer
 *
 * Tree view showing the session hierarchy:
 * - Groups (with nested groups)
 *   - Voices
 *   - Patterns (with activity indicator)
 *   - Melodies (with activity indicator)
 *   - Effects
 */

import * as vscode from 'vscode';
import { StateStore } from '../state/stateStore';
import {
    Group,
    Voice,
    Pattern,
    Melody,
    Effect,
    Sequence,
    SourceLocation,
} from '../api/types';

// =============================================================================
// Tree Item Types
// =============================================================================

type TreeItemType =
    | 'group'
    | 'voice'
    | 'pattern'
    | 'melody'
    | 'effect'
    | 'sequence'
    | 'category';

interface BaseTreeData {
    type: TreeItemType;
    label: string;
    id: string;
    sourceLocation?: SourceLocation;
}

interface GroupData extends BaseTreeData {
    type: 'group';
    group: Group;
}

interface VoiceData extends BaseTreeData {
    type: 'voice';
    voice: Voice;
}

interface PatternData extends BaseTreeData {
    type: 'pattern';
    pattern: Pattern;
}

interface MelodyData extends BaseTreeData {
    type: 'melody';
    melody: Melody;
}

interface EffectData extends BaseTreeData {
    type: 'effect';
    effect: Effect;
}

interface SequenceData extends BaseTreeData {
    type: 'sequence';
    sequence: Sequence;
}

interface CategoryData extends BaseTreeData {
    type: 'category';
    category: 'voices' | 'patterns' | 'melodies' | 'effects' | 'sequences';
    parentPath: string;
}

type TreeData =
    | GroupData
    | VoiceData
    | PatternData
    | MelodyData
    | EffectData
    | SequenceData
    | CategoryData;

// =============================================================================
// Tree Data Provider
// =============================================================================

export class SessionExplorerProvider implements vscode.TreeDataProvider<TreeData> {
    private _store: StateStore;
    private _onDidChangeTreeData = new vscode.EventEmitter<TreeData | undefined | null>();
    readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

    constructor(store: StateStore) {
        this._store = store;

        // Refresh tree when state updates
        store.onFullUpdate(() => {
            this._onDidChangeTreeData.fire(undefined);
        });

        store.onStatusChange(() => {
            this._onDidChangeTreeData.fire(undefined);
        });
    }

    refresh() {
        this._onDidChangeTreeData.fire(undefined);
    }

    // ==========================================================================
    // TreeDataProvider Implementation
    // ==========================================================================

    getTreeItem(element: TreeData): vscode.TreeItem {
        const item = new vscode.TreeItem(
            element.label,
            this.getCollapsibleState(element)
        );

        item.id = `${element.type}:${element.id}`;
        item.contextValue = element.type;

        // Set icon and description based on type
        switch (element.type) {
            case 'group':
                this.configureGroupItem(item, element);
                break;
            case 'voice':
                this.configureVoiceItem(item, element);
                break;
            case 'pattern':
                this.configurePatternItem(item, element);
                break;
            case 'melody':
                this.configureMelodyItem(item, element);
                break;
            case 'effect':
                this.configureEffectItem(item, element);
                break;
            case 'sequence':
                this.configureSequenceItem(item, element);
                break;
            case 'category':
                this.configureCategoryItem(item, element);
                break;
        }

        // Add command to navigate to source location
        if (element.sourceLocation?.file && element.sourceLocation?.line) {
            item.command = {
                command: 'vibelang.goToSource',
                title: 'Go to Definition',
                arguments: [element.sourceLocation],
            };
        } else if (element.type !== 'category') {
            // Select the entity in the inspector
            item.command = {
                command: 'vibelang.selectEntity',
                title: 'Select',
                arguments: [element.type, element.id],
            };
        }

        return item;
    }

    async getChildren(element?: TreeData): Promise<TreeData[]> {
        if (this._store.status !== 'connected') {
            return [];
        }

        if (!element) {
            // Root level - return groups + sequences
            return this.getRootItems();
        }

        switch (element.type) {
            case 'group':
                return this.getGroupChildren(element);
            case 'category':
                return this.getCategoryChildren(element);
            default:
                return [];
        }
    }

    getParent(element: TreeData): TreeData | null {
        // TODO: Implement if needed for reveal functionality
        return null;
    }

    // ==========================================================================
    // Item Configuration
    // ==========================================================================

    private getCollapsibleState(element: TreeData): vscode.TreeItemCollapsibleState {
        switch (element.type) {
            case 'group':
                return vscode.TreeItemCollapsibleState.Expanded;
            case 'category':
                return vscode.TreeItemCollapsibleState.Collapsed;
            default:
                return vscode.TreeItemCollapsibleState.None;
        }
    }

    private configureGroupItem(item: vscode.TreeItem, data: GroupData) {
        const group = data.group;
        const isActive = this._store.isGroupActive(group.path);
        const isMuted = group.muted;
        const isSoloed = group.soloed;

        // Icon based on state
        if (isMuted) {
            item.iconPath = new vscode.ThemeIcon('mute', new vscode.ThemeColor('disabledForeground'));
        } else if (isSoloed) {
            item.iconPath = new vscode.ThemeIcon('star-full', new vscode.ThemeColor('charts.yellow'));
        } else if (isActive) {
            item.iconPath = new vscode.ThemeIcon('pulse', new vscode.ThemeColor('charts.green'));
        } else {
            item.iconPath = new vscode.ThemeIcon('folder');
        }

        // Description shows amp if not 1.0
        const amp = group.params['amp'] ?? 1.0;
        if (amp !== 1.0) {
            item.description = `${Math.round(amp * 100)}%`;
        }

        item.tooltip = new vscode.MarkdownString();
        item.tooltip.appendMarkdown(`**${group.name}**\n\n`);
        item.tooltip.appendMarkdown(`Path: \`${group.path}\`\n\n`);
        if (isMuted) item.tooltip.appendMarkdown('🔇 Muted\n\n');
        if (isSoloed) item.tooltip.appendMarkdown('⭐ Soloed\n\n');
        item.tooltip.appendMarkdown(`Amp: ${(amp * 100).toFixed(0)}%`);
    }

    private configureVoiceItem(item: vscode.TreeItem, data: VoiceData) {
        const voice = data.voice;

        // Icon based on type
        if (voice.sfz_instrument) {
            item.iconPath = new vscode.ThemeIcon('piano');
        } else if (voice.vst_instrument) {
            item.iconPath = new vscode.ThemeIcon('extensions');
        } else {
            item.iconPath = new vscode.ThemeIcon('symbol-event');
        }

        // Mute state
        if (voice.muted) {
            item.iconPath = new vscode.ThemeIcon('mute', new vscode.ThemeColor('disabledForeground'));
        }

        // Description
        item.description = voice.synth_name;

        // Tooltip
        item.tooltip = new vscode.MarkdownString();
        item.tooltip.appendMarkdown(`**Voice: ${voice.name}**\n\n`);
        item.tooltip.appendMarkdown(`Synth: \`${voice.synth_name}\`\n\n`);
        item.tooltip.appendMarkdown(`Polyphony: ${voice.polyphony}\n\n`);
        item.tooltip.appendMarkdown(`Gain: ${(voice.gain * 100).toFixed(0)}%`);
    }

    private configurePatternItem(item: vscode.TreeItem, data: PatternData) {
        const pattern = data.pattern;
        const isPlaying = pattern.status.state === 'playing';
        const isQueued = pattern.status.state === 'queued';

        // Icon based on state - activity indicator
        if (isPlaying) {
            item.iconPath = new vscode.ThemeIcon('play-circle', new vscode.ThemeColor('charts.green'));
        } else if (isQueued) {
            item.iconPath = new vscode.ThemeIcon('clock', new vscode.ThemeColor('charts.yellow'));
        } else {
            item.iconPath = new vscode.ThemeIcon('list-ordered');
        }

        // Description
        item.description = `${pattern.loop_beats}b → ${pattern.voice_name}`;

        // Tooltip
        item.tooltip = new vscode.MarkdownString();
        item.tooltip.appendMarkdown(`**Pattern: ${pattern.name}**\n\n`);
        item.tooltip.appendMarkdown(`Voice: \`${pattern.voice_name}\`\n\n`);
        item.tooltip.appendMarkdown(`Loop: ${pattern.loop_beats} beats\n\n`);
        item.tooltip.appendMarkdown(`Events: ${pattern.events.length}\n\n`);
        item.tooltip.appendMarkdown(`Status: ${pattern.status.state}`);
    }

    private configureMelodyItem(item: vscode.TreeItem, data: MelodyData) {
        const melody = data.melody;
        const isPlaying = melody.status.state === 'playing';
        const isQueued = melody.status.state === 'queued';

        // Icon based on state - activity indicator
        if (isPlaying) {
            item.iconPath = new vscode.ThemeIcon('play-circle', new vscode.ThemeColor('charts.green'));
        } else if (isQueued) {
            item.iconPath = new vscode.ThemeIcon('clock', new vscode.ThemeColor('charts.yellow'));
        } else {
            item.iconPath = new vscode.ThemeIcon('note');
        }

        // Description
        item.description = `${melody.loop_beats}b → ${melody.voice_name}`;

        // Tooltip
        item.tooltip = new vscode.MarkdownString();
        item.tooltip.appendMarkdown(`**Melody: ${melody.name}**\n\n`);
        item.tooltip.appendMarkdown(`Voice: \`${melody.voice_name}\`\n\n`);
        item.tooltip.appendMarkdown(`Loop: ${melody.loop_beats} beats\n\n`);
        item.tooltip.appendMarkdown(`Notes: ${melody.events.length}\n\n`);
        item.tooltip.appendMarkdown(`Status: ${melody.status.state}`);
    }

    private configureEffectItem(item: vscode.TreeItem, data: EffectData) {
        const effect = data.effect;

        item.iconPath = new vscode.ThemeIcon('wand');
        item.description = effect.synthdef_name;

        // Tooltip
        item.tooltip = new vscode.MarkdownString();
        item.tooltip.appendMarkdown(`**Effect: ${effect.id}**\n\n`);
        item.tooltip.appendMarkdown(`SynthDef: \`${effect.synthdef_name}\`\n\n`);
        if (effect.vst_plugin) {
            item.tooltip.appendMarkdown(`VST: ${effect.vst_plugin}\n\n`);
        }
        item.tooltip.appendMarkdown(`Position: ${effect.position ?? 0}`);
    }

    private configureSequenceItem(item: vscode.TreeItem, data: SequenceData) {
        const sequence = data.sequence;
        const isActive = this._store.isSequenceActive(sequence.name);

        // Icon based on state
        if (isActive) {
            item.iconPath = new vscode.ThemeIcon('play-circle', new vscode.ThemeColor('charts.green'));
        } else {
            item.iconPath = new vscode.ThemeIcon('layers');
        }

        // Description
        item.description = `${sequence.loop_beats}b, ${sequence.clips.length} clips`;

        // Tooltip
        item.tooltip = new vscode.MarkdownString();
        item.tooltip.appendMarkdown(`**Sequence: ${sequence.name}**\n\n`);
        item.tooltip.appendMarkdown(`Loop: ${sequence.loop_beats} beats\n\n`);
        item.tooltip.appendMarkdown(`Clips: ${sequence.clips.length}`);
    }

    private configureCategoryItem(item: vscode.TreeItem, data: CategoryData) {
        switch (data.category) {
            case 'voices':
                item.iconPath = new vscode.ThemeIcon('symbol-event');
                break;
            case 'patterns':
                item.iconPath = new vscode.ThemeIcon('list-ordered');
                break;
            case 'melodies':
                item.iconPath = new vscode.ThemeIcon('note');
                break;
            case 'effects':
                item.iconPath = new vscode.ThemeIcon('wand');
                break;
            case 'sequences':
                item.iconPath = new vscode.ThemeIcon('layers');
                break;
        }
    }

    // ==========================================================================
    // Children Retrieval
    // ==========================================================================

    private getRootItems(): TreeData[] {
        const items: TreeData[] = [];

        // Add root groups (those without parent_path or parent_path is null)
        for (const group of this._store.groups) {
            if (!group.parent_path) {
                items.push({
                    type: 'group',
                    label: group.name,
                    id: group.path,
                    group,
                    sourceLocation: group.source_location,
                });
            }
        }

        // Add sequences section if there are any
        if (this._store.sequences.length > 0) {
            items.push({
                type: 'category',
                label: `Sequences (${this._store.sequences.length})`,
                id: 'cat:sequences:root',
                category: 'sequences',
                parentPath: '',
            });
        }

        return items;
    }

    private getGroupChildren(data: GroupData): TreeData[] {
        const items: TreeData[] = [];
        const groupPath = data.group.path;

        // Child groups
        for (const group of this._store.groups) {
            if (group.parent_path === groupPath) {
                items.push({
                    type: 'group',
                    label: group.name,
                    id: group.path,
                    group,
                    sourceLocation: group.source_location,
                });
            }
        }

        // Voices in this group
        const voices = this._store.voices.filter((v) => v.group_path === groupPath);
        if (voices.length > 0) {
            items.push({
                type: 'category',
                label: `Voices (${voices.length})`,
                id: `cat:voices:${groupPath}`,
                category: 'voices',
                parentPath: groupPath,
            });
        }

        // Patterns in this group
        const patterns = this._store.patterns.filter((p) => p.group_path === groupPath);
        if (patterns.length > 0) {
            items.push({
                type: 'category',
                label: `Patterns (${patterns.length})`,
                id: `cat:patterns:${groupPath}`,
                category: 'patterns',
                parentPath: groupPath,
            });
        }

        // Melodies in this group
        const melodies = this._store.melodies.filter((m) => m.group_path === groupPath);
        if (melodies.length > 0) {
            items.push({
                type: 'category',
                label: `Melodies (${melodies.length})`,
                id: `cat:melodies:${groupPath}`,
                category: 'melodies',
                parentPath: groupPath,
            });
        }

        // Effects in this group
        const effects = this._store.effects.filter((e) => e.group_path === groupPath);
        if (effects.length > 0) {
            items.push({
                type: 'category',
                label: `Effects (${effects.length})`,
                id: `cat:effects:${groupPath}`,
                category: 'effects',
                parentPath: groupPath,
            });
        }

        return items;
    }

    private getCategoryChildren(data: CategoryData): TreeData[] {
        const items: TreeData[] = [];
        const parentPath = data.parentPath;

        switch (data.category) {
            case 'voices':
                for (const voice of this._store.voices) {
                    if (voice.group_path === parentPath) {
                        items.push({
                            type: 'voice',
                            label: voice.name,
                            id: voice.name,
                            voice,
                            sourceLocation: voice.source_location,
                        });
                    }
                }
                break;

            case 'patterns':
                for (const pattern of this._store.patterns) {
                    if (pattern.group_path === parentPath) {
                        items.push({
                            type: 'pattern',
                            label: pattern.name,
                            id: pattern.name,
                            pattern,
                            sourceLocation: pattern.source_location,
                        });
                    }
                }
                break;

            case 'melodies':
                for (const melody of this._store.melodies) {
                    if (melody.group_path === parentPath) {
                        items.push({
                            type: 'melody',
                            label: melody.name,
                            id: melody.name,
                            melody,
                            sourceLocation: melody.source_location,
                        });
                    }
                }
                break;

            case 'effects':
                for (const effect of this._store.effects) {
                    if (effect.group_path === parentPath) {
                        items.push({
                            type: 'effect',
                            label: effect.id,
                            id: effect.id,
                            effect,
                            sourceLocation: effect.source_location,
                        });
                    }
                }
                break;

            case 'sequences':
                for (const sequence of this._store.sequences) {
                    items.push({
                        type: 'sequence',
                        label: sequence.name,
                        id: sequence.name,
                        sequence,
                        sourceLocation: sequence.source_location,
                    });
                }
                break;
        }

        return items;
    }
}

// =============================================================================
// Commands Registration
// =============================================================================

export function registerSessionExplorerCommands(
    context: vscode.ExtensionContext,
    store: StateStore
) {
    // Go to source location
    context.subscriptions.push(
        vscode.commands.registerCommand(
            'vibelang.goToSource',
            async (location: SourceLocation) => {
                if (!location.file || !location.line) {
                    vscode.window.showWarningMessage('No source location available');
                    return;
                }

                try {
                    const uri = vscode.Uri.file(location.file);
                    const doc = await vscode.workspace.openTextDocument(uri);
                    const editor = await vscode.window.showTextDocument(doc);

                    const line = Math.max(0, (location.line ?? 1) - 1);
                    const column = Math.max(0, (location.column ?? 1) - 1);
                    const position = new vscode.Position(line, column);

                    editor.selection = new vscode.Selection(position, position);
                    editor.revealRange(
                        new vscode.Range(position, position),
                        vscode.TextEditorRevealType.InCenter
                    );
                } catch (e) {
                    vscode.window.showErrorMessage(`Could not open file: ${location.file}`);
                }
            }
        )
    );

    // Select entity
    context.subscriptions.push(
        vscode.commands.registerCommand(
            'vibelang.selectEntity',
            (type: string, id: string) => {
                switch (type) {
                    case 'group':
                        store.selectGroup(id);
                        break;
                    case 'voice':
                        store.selectVoice(id);
                        break;
                    case 'pattern':
                        store.selectPattern(id);
                        break;
                    case 'melody':
                        store.selectMelody(id);
                        break;
                    case 'sequence':
                        store.selectSequence(id);
                        break;
                    case 'effect':
                        store.selectEffect(id);
                        break;
                }
                // Open inspector panel
                vscode.commands.executeCommand('vibelang.openInspector');
            }
        )
    );

    // Context menu commands for patterns/melodies
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.startPattern', async (data: PatternData) => {
            if (data?.pattern) {
                await store.runtime.startPattern(data.pattern.name);
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.stopPattern', async (data: PatternData) => {
            if (data?.pattern) {
                await store.runtime.stopPattern(data.pattern.name);
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.startMelody', async (data: MelodyData) => {
            if (data?.melody) {
                await store.runtime.startMelody(data.melody.name);
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.stopMelody', async (data: MelodyData) => {
            if (data?.melody) {
                await store.runtime.stopMelody(data.melody.name);
            }
        })
    );

    // Mute/Solo commands for groups
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.muteGroup', async (data: GroupData) => {
            if (data?.group) {
                if (data.group.muted) {
                    await store.runtime.unmuteGroup(data.group.path);
                } else {
                    await store.runtime.muteGroup(data.group.path);
                }
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.soloGroup', async (data: GroupData) => {
            if (data?.group) {
                if (data.group.soloed) {
                    await store.runtime.unsoloGroup(data.group.path);
                } else {
                    await store.runtime.soloGroup(data.group.path);
                }
            }
        })
    );

    // Sequence commands
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.startSequence', async (data: SequenceData) => {
            if (data?.sequence) {
                await store.runtime.startSequence(data.sequence.name);
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.stopSequence', async (data: SequenceData) => {
            if (data?.sequence) {
                await store.runtime.stopSequence(data.sequence.name);
            }
        })
    );

    // Refresh command
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.refreshSession', () => {
            store.runtime.tryConnect();
        })
    );
}
