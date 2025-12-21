/**
 * VibeLang Tag Store
 *
 * Manages user-defined tags for samples and synthdefs.
 * Tags are stored globally in VSCode's globalState, persisting across projects.
 */

import * as vscode from 'vscode';

export type TaggableType = 'sample' | 'synthdef';

interface TagStoreData {
    userTags: Record<string, string[]>;  // Key: "sample:{id}" or "synthdef:{name}"
}

const STORAGE_KEY = 'vibelang.userTags';

export class TagStoreManager {
    private _context: vscode.ExtensionContext;
    private _data: TagStoreData;
    private _onTagsChanged = new vscode.EventEmitter<{ type: TaggableType; id: string; tags: string[] }>();

    public readonly onTagsChanged = this._onTagsChanged.event;

    constructor(context: vscode.ExtensionContext) {
        this._context = context;
        this._data = this._load();
    }

    private _load(): TagStoreData {
        const stored = this._context.globalState.get<TagStoreData>(STORAGE_KEY);
        return stored || { userTags: {} };
    }

    private async _save(): Promise<void> {
        await this._context.globalState.update(STORAGE_KEY, this._data);
    }

    private _makeKey(type: TaggableType, id: string): string {
        return `${type}:${id}`;
    }

    /**
     * Get user tags for an item
     */
    getUserTags(type: TaggableType, id: string): string[] {
        const key = this._makeKey(type, id);
        return this._data.userTags[key] || [];
    }

    /**
     * Set all user tags for an item (replaces existing)
     */
    async setUserTags(type: TaggableType, id: string, tags: string[]): Promise<void> {
        const key = this._makeKey(type, id);
        const normalizedTags = tags.map(t => t.toLowerCase().trim()).filter(t => t.length > 0);
        const uniqueTags = [...new Set(normalizedTags)];

        if (uniqueTags.length === 0) {
            delete this._data.userTags[key];
        } else {
            this._data.userTags[key] = uniqueTags;
        }

        await this._save();
        this._onTagsChanged.fire({ type, id, tags: uniqueTags });
    }

    /**
     * Add a single tag to an item
     */
    async addTag(type: TaggableType, id: string, tag: string): Promise<void> {
        const normalizedTag = tag.toLowerCase().trim();
        if (!normalizedTag) return;

        const currentTags = this.getUserTags(type, id);
        if (!currentTags.includes(normalizedTag)) {
            await this.setUserTags(type, id, [...currentTags, normalizedTag]);
        }
    }

    /**
     * Remove a single tag from an item
     */
    async removeTag(type: TaggableType, id: string, tag: string): Promise<void> {
        const normalizedTag = tag.toLowerCase().trim();
        const currentTags = this.getUserTags(type, id);
        const newTags = currentTags.filter(t => t !== normalizedTag);

        if (newTags.length !== currentTags.length) {
            await this.setUserTags(type, id, newTags);
        }
    }

    /**
     * Toggle a tag on an item (add if missing, remove if present)
     */
    async toggleTag(type: TaggableType, id: string, tag: string): Promise<boolean> {
        const normalizedTag = tag.toLowerCase().trim();
        const currentTags = this.getUserTags(type, id);

        if (currentTags.includes(normalizedTag)) {
            await this.removeTag(type, id, tag);
            return false; // Tag was removed
        } else {
            await this.addTag(type, id, tag);
            return true; // Tag was added
        }
    }

    /**
     * Get all unique user tags across all items
     */
    getAllUserTags(): string[] {
        const allTags = new Set<string>();
        for (const tags of Object.values(this._data.userTags)) {
            for (const tag of tags) {
                allTags.add(tag);
            }
        }
        return Array.from(allTags).sort();
    }

    /**
     * Get tag counts (how many items have each tag)
     */
    getTagCounts(): Record<string, number> {
        const counts: Record<string, number> = {};
        for (const tags of Object.values(this._data.userTags)) {
            for (const tag of tags) {
                counts[tag] = (counts[tag] || 0) + 1;
            }
        }
        return counts;
    }

    /**
     * Get all user tags as a map (for sending to webview)
     */
    getAllTagsMap(): Record<string, string[]> {
        return { ...this._data.userTags };
    }

    /**
     * Check if an item has a specific tag
     */
    hasTag(type: TaggableType, id: string, tag: string): boolean {
        return this.getUserTags(type, id).includes(tag.toLowerCase().trim());
    }

    /**
     * Get items that have a specific tag
     */
    getItemsWithTag(tag: string): Array<{ type: TaggableType; id: string }> {
        const normalizedTag = tag.toLowerCase().trim();
        const items: Array<{ type: TaggableType; id: string }> = [];

        for (const [key, tags] of Object.entries(this._data.userTags)) {
            if (tags.includes(normalizedTag)) {
                const [type, ...idParts] = key.split(':');
                items.push({
                    type: type as TaggableType,
                    id: idParts.join(':')  // Handle IDs that might contain ':'
                });
            }
        }

        return items;
    }

    dispose() {
        this._onTagsChanged.dispose();
    }
}
