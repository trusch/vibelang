"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DataLoader = void 0;
const vscode = require("vscode");
const fs = require("fs");
const path = require("path");
class DataLoader {
    /**
     * Clear all cached data. Call this when you need to reload data.
     */
    static clearCache() {
        this._ugens = [];
        this._rhaiApi = [];
        this._stdlib = [];
        this._initialized = false;
    }
    static async loadUGens(extensionPath) {
        if (this._ugens.length > 0)
            return this._ugens;
        // Try multiple locations for UGen manifests
        const possiblePaths = [];
        if (vscode.workspace.workspaceFolders) {
            const rootPath = vscode.workspace.workspaceFolders[0].uri.fsPath;
            // Primary location: crates/vibelang-dsp/ugen_manifests/
            possiblePaths.push(path.join(rootPath, 'crates', 'vibelang-dsp', 'ugen_manifests'));
            // Legacy location at workspace root
            possiblePaths.push(path.join(rootPath, 'ugen_manifests'));
        }
        // Bundled with extension (for production)
        possiblePaths.push(path.join(extensionPath, 'ugen_manifests'));
        possiblePaths.push(path.join(extensionPath, 'out', 'ugen_manifests'));
        let manifestPath = '';
        for (const p of possiblePaths) {
            if (fs.existsSync(p)) {
                manifestPath = p;
                break;
            }
        }
        if (!manifestPath) {
            console.warn(`Vibelang: UGen manifests not found in any of: ${possiblePaths.join(', ')}`);
            return [];
        }
        try {
            const files = fs.readdirSync(manifestPath);
            for (const file of files) {
                if (file.endsWith('.json')) {
                    const content = fs.readFileSync(path.join(manifestPath, file), 'utf-8');
                    const ugens = JSON.parse(content);
                    this._ugens.push(...ugens);
                }
            }
        }
        catch (e) {
            console.error('Error loading UGen manifests:', e);
        }
        return this._ugens;
    }
    static loadRhaiApi(extensionPath) {
        if (this._rhaiApi.length > 0)
            return this._rhaiApi;
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
            }
            catch (e) {
                console.error('Error loading Rhai API:', e);
            }
        }
        else {
            console.warn(`Vibelang: Rhai API not found at ${finalPath}`);
        }
        return this._rhaiApi;
    }
    static loadStdlib(extensionPath) {
        if (this._stdlib.length > 0)
            return this._stdlib;
        // Try multiple locations for stdlib data
        const possiblePaths = [
            path.join(extensionPath, 'src', 'data', 'stdlib.json'),
            path.join(extensionPath, 'out', 'data', 'stdlib.json'),
        ];
        let finalPath = '';
        for (const p of possiblePaths) {
            if (fs.existsSync(p)) {
                finalPath = p;
                break;
            }
        }
        if (finalPath) {
            try {
                const content = fs.readFileSync(finalPath, 'utf-8');
                const data = JSON.parse(content);
                // Handle both old format (array) and new format ({ synthdefs: [...] })
                if (Array.isArray(data)) {
                    this._stdlib = data;
                }
                else if (data.synthdefs && Array.isArray(data.synthdefs)) {
                    this._stdlib = data.synthdefs;
                }
            }
            catch (e) {
                console.error('Error loading stdlib:', e);
            }
        }
        else {
            console.warn(`Vibelang: Stdlib data not found in: ${possiblePaths.join(', ')}`);
        }
        return this._stdlib;
    }
    /**
     * Get a map of synthdef names to their import paths
     */
    static getImportMap(extensionPath) {
        const stdlib = this.loadStdlib(extensionPath);
        const map = new Map();
        for (const item of stdlib) {
            if (item.importPath) {
                map.set(item.name, item.importPath);
            }
        }
        return map;
    }
}
exports.DataLoader = DataLoader;
DataLoader._ugens = [];
DataLoader._rhaiApi = [];
DataLoader._stdlib = [];
DataLoader._initialized = false;
//# sourceMappingURL=dataLoader.js.map