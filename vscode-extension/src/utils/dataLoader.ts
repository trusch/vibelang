import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';

export interface UGenInput {
    name: string;
    type: string;
    default: number;
    description: string;
}

export interface UGenDefinition {
    name: string;
    description: string;
    rates: string[];
    inputs: UGenInput[];
    outputs: number;
    category: string;
}

export interface RhaiFunction {
    name: string;
    description: string;
    signature: string;
    example: string;
}

export class DataLoader {
    private static _ugens: UGenDefinition[] = [];
    private static _rhaiApi: RhaiFunction[] = [];

    public static async loadUGens(extensionPath: string): Promise<UGenDefinition[]> {
        if (this._ugens.length > 0) return this._ugens;

        // First try resolving from workspace root if available
        // This supports development mode where manifests are in the project root
        let manifestPath = '';
        if (vscode.workspace.workspaceFolders) {
            const rootPath = vscode.workspace.workspaceFolders[0].uri.fsPath;
            manifestPath = path.join(rootPath, 'ugen_manifests');
        }

        // Fallback: check if manifests are bundled with the extension (for production)
        if (!fs.existsSync(manifestPath)) {
            manifestPath = path.join(extensionPath, 'ugen_manifests');
        }
        
        // Fallback 2: If we are in dev mode but opened a subfolder, maybe try relative paths?
        // For now, just logging failure.

        if (!fs.existsSync(manifestPath)) {
            console.warn(`Vibelang: UGen manifests not found at ${manifestPath}`);
            return [];
        }

        try {
            const files = fs.readdirSync(manifestPath);
            for (const file of files) {
                if (file.endsWith('.json')) {
                    const content = fs.readFileSync(path.join(manifestPath, file), 'utf-8');
                    const ugens = JSON.parse(content) as UGenDefinition[];
                    this._ugens.push(...ugens);
                }
            }
        } catch (e) {
            console.error('Error loading UGen manifests:', e);
        }

        return this._ugens;
    }

    public static loadRhaiApi(extensionPath: string): RhaiFunction[] {
        if (this._rhaiApi.length > 0) return this._rhaiApi;

        const apiPath = path.join(extensionPath, 'src', 'data', 'rhai-api.json');
        // In production (out folder), the path might differ. 
        // If src/data is not copied to out/, we need to adjust.
        // Typically we bundle it or copy it.
        // Let's check both src location (dev) and out location (prod expectation).
        
        let finalPath = apiPath;
        if (!fs.existsSync(finalPath)) {
            // Try searching relative to the compiled file in 'out'
             // extensionPath/out/data/rhai-api.json?
             finalPath = path.join(extensionPath, 'out', 'data', 'rhai-api.json');
        }
        
        // Actually, in TS compilation, src/data/*.json isn't automatically copied unless configured.
        // We might need to rely on it being in the source tree or copied manually.
        // As a fallback, let's assume it is in root/src/data if we are running from source.
        
        if (fs.existsSync(finalPath)) {
            try {
                const content = fs.readFileSync(finalPath, 'utf-8');
                this._rhaiApi = JSON.parse(content);
            } catch (e) {
                console.error('Error loading Rhai API:', e);
            }
        } else {
             console.warn(`Vibelang: Rhai API not found at ${finalPath}`);
        }

        return this._rhaiApi;
    }
}
