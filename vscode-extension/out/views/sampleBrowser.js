"use strict";
/**
 * VibeLang Sample & Preset Browser
 *
 * A panel for browsing, previewing, and managing:
 * - Loaded samples (audio files)
 * - SynthDefs (synthesizer presets)
 * - SFZ instruments
 *
 * Features:
 * - List view with metadata
 * - Preview playback
 * - Copyable code snippets for load_sample, voice, pattern, and slicing
 * - File browser for loading new samples
 * - Slicing workflow with equal divisions or custom slice points
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.SampleBrowser = void 0;
const vscode = require("vscode");
const fs = require("fs");
const path = require("path");
class SampleBrowser {
    constructor(panel, store, extensionPath) {
        this._disposables = [];
        this._stdlibData = null;
        this._testNoteOffTimeout = null;
        this._panel = panel;
        this._store = store;
        this._extensionPath = extensionPath;
        // Load stdlib metadata
        this._loadStdlibData();
        this._updateContent();
        // Listen for state updates
        this._disposables.push(store.onFullUpdate(() => this._sendStateUpdate()));
        this._disposables.push(store.onStatusChange(() => this._updateContent()));
        // Handle messages from webview
        this._panel.webview.onDidReceiveMessage((message) => this._handleMessage(message), null, this._disposables);
        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    }
    static createOrShow(store, extensionPath) {
        const column = vscode.ViewColumn.Beside;
        if (SampleBrowser.currentPanel) {
            SampleBrowser.currentPanel._panel.reveal(column);
            return;
        }
        const panel = vscode.window.createWebviewPanel(SampleBrowser.viewType, 'Sample Browser', column, {
            enableScripts: true,
            retainContextWhenHidden: true,
        });
        SampleBrowser.currentPanel = new SampleBrowser(panel, store, extensionPath);
    }
    static revive(panel, store, extensionPath) {
        SampleBrowser.currentPanel = new SampleBrowser(panel, store, extensionPath);
    }
    _loadStdlibData() {
        try {
            const stdlibPath = path.join(this._extensionPath, 'src', 'data', 'stdlib.json');
            if (fs.existsSync(stdlibPath)) {
                const content = fs.readFileSync(stdlibPath, 'utf-8');
                this._stdlibData = JSON.parse(content);
                console.log(`Loaded ${this._stdlibData?.synthdefs.length || 0} stdlib synthdefs`);
            }
        }
        catch (err) {
            console.error('Failed to load stdlib data:', err);
        }
    }
    _updateContent() {
        this._panel.webview.html = this._getHtmlContent();
        setTimeout(() => this._sendStateUpdate(), 100);
    }
    _sendStateUpdate() {
        const state = this._store.state;
        // Get runtime synthdefs (loaded ones)
        const runtimeSynthdefs = state?.synthdefs || [];
        const loadedNames = new Set(runtimeSynthdefs.map(s => s.name));
        // Get stdlib synthdefs that aren't loaded
        const stdlibSynthdefs = (this._stdlibData?.synthdefs || [])
            .filter(s => !loadedNames.has(s.name))
            .map(s => ({
            name: s.name,
            source: 'stdlib',
            params: s.params.map(p => ({
                name: p.name,
                default_value: typeof p.default === 'number' ? p.default : 0,
            })),
            // Extra metadata for stdlib
            _stdlib: {
                type: s.type,
                description: s.description,
                category: s.category,
                subcategory: s.subcategory,
                importPath: s.importPath,
                sourcePath: s.sourcePath,
            }
        }));
        this._panel.webview.postMessage({
            type: 'stateUpdate',
            data: {
                samples: state?.samples || [],
                synthdefs: [...runtimeSynthdefs, ...stdlibSynthdefs],
                voices: state?.voices || [],
            },
        });
    }
    async _handleMessage(message) {
        switch (message.command) {
            case 'loadSample':
                await this._loadSampleFromFile();
                break;
            case 'previewSample':
                await this._previewSample(message.sampleId);
                break;
            case 'stopPreview':
                await this._stopPreview();
                break;
            case 'insertSampleCode':
                await this._insertSampleCode(message.sampleId);
                break;
            case 'insertSynthDefCode':
                await this._insertSynthDefCode(message.synthdefName);
                break;
            case 'copyToClipboard':
                const text = message.text;
                await vscode.env.clipboard.writeText(text);
                vscode.window.showInformationMessage('Copied to clipboard');
                break;
            case 'goToSource':
                const location = message.sourceLocation;
                if (location?.file && location?.line) {
                    vscode.commands.executeCommand('vibelang.goToSource', location);
                }
                break;
            case 'testSynthDef':
                await this._testSynthDef(message.synthdefName);
                break;
            case 'stopTestSynthDef':
                await this._stopTestSynthDef(message.synthdefName);
                break;
            case 'viewSynthDefSource':
                await this._viewSynthDefSource(message.synthdefName);
                break;
        }
    }
    async _loadSampleFromFile() {
        const result = await vscode.window.showOpenDialog({
            canSelectFiles: true,
            canSelectMany: true,
            filters: {
                'Audio Files': ['wav', 'aiff', 'aif', 'flac', 'ogg', 'mp3'],
                'SFZ Instruments': ['sfz'],
                'All Files': ['*'],
            },
            title: 'Select Sample or Instrument',
        });
        if (result && result.length > 0) {
            // Generate code snippets for the selected files
            const snippets = result.map(uri => {
                const fileName = uri.fsPath.split('/').pop()?.replace(/\.[^.]+$/, '') || 'sample';
                const safeName = fileName.replace(/[^a-zA-Z0-9_]/g, '_').toLowerCase();
                return this._generateSampleCode(safeName, uri.fsPath);
            });
            const code = snippets.join('\n\n');
            await vscode.env.clipboard.writeText(code);
            vscode.window.showInformationMessage(`Code for ${result.length} sample(s) copied to clipboard`);
        }
    }
    _generateSampleCode(id, path) {
        return `// Load the sample
let ${id} = load_sample("${id}", "${path}");

// Create a voice using the sample
let ${id}_voice = voice("${id}_voice")
    .sample(${id})
    .group(my_group);

// Example pattern
let ${id}_pattern = pattern("${id}_pattern", ${id}_voice, "x...x...x...x...");
${id}_pattern.start();`;
    }
    async _previewSample(sampleId) {
        try {
            const runtime = this._store.runtime;
            if (!runtime) {
                vscode.window.showInformationMessage('Preview requires runtime connection');
                return;
            }
            // Trigger the sample's voice if it exists, or just show info
            const sample = this._store.state?.samples.find(s => s.id === sampleId);
            if (sample) {
                // Try to find a voice using this sample
                const voice = this._store.state?.voices.find(v => v.synth_name === sample.synthdef_name);
                if (voice) {
                    await runtime.triggerVoice(voice.name);
                }
                else {
                    vscode.window.showInformationMessage(`Create a voice using this sample to preview it`);
                }
            }
        }
        catch (err) {
            vscode.window.showErrorMessage(`Preview failed: ${err}`);
        }
    }
    async _stopPreview() {
        // TODO: implement stop preview
    }
    async _insertSampleCode(sampleId) {
        const sample = this._store.state?.samples.find(s => s.id === sampleId);
        if (!sample)
            return;
        const code = this._generateSampleCode(sample.id, sample.path);
        const editor = vscode.window.activeTextEditor;
        if (editor) {
            await editor.edit(edit => {
                edit.insert(editor.selection.active, code + '\n');
            });
        }
        else {
            await vscode.env.clipboard.writeText(code);
            vscode.window.showInformationMessage('Code copied to clipboard');
        }
    }
    async _insertSynthDefCode(synthdefName) {
        const synthdef = this._store.state?.synthdefs.find(s => s.name === synthdefName);
        if (!synthdef)
            return;
        const paramsCode = synthdef.params
            .filter(p => p.name !== 'out' && p.name !== 'amp' && p.name !== 'gate')
            .map(p => `    .param("${p.name}", ${p.default_value})`)
            .join('\n');
        const code = `let my_voice = voice("my_voice")
    .synth("${synthdef.name}")
${paramsCode}
    .group(my_group);`;
        const editor = vscode.window.activeTextEditor;
        if (editor) {
            await editor.edit(edit => {
                edit.insert(editor.selection.active, code + '\n');
            });
        }
        else {
            await vscode.env.clipboard.writeText(code);
            vscode.window.showInformationMessage('Code copied to clipboard');
        }
    }
    async _testSynthDef(synthdefName) {
        const runtime = this._store.runtime;
        if (!runtime) {
            vscode.window.showErrorMessage('Not connected to VibeLang runtime');
            this._panel.webview.postMessage({ type: 'testStopped' });
            return;
        }
        try {
            // Clean up any existing test voice first
            await this._stopTestSynthDef(synthdefName);
            // Check if this is an unloaded stdlib synthdef that needs to be imported
            const runtimeSynthdef = this._store.state?.synthdefs.find(s => s.name === synthdefName);
            const stdlibInfo = this._stdlibData?.synthdefs.find(s => s.name === synthdefName);
            if (!runtimeSynthdef && stdlibInfo) {
                // Need to import the stdlib synthdef first
                const importCode = `import "${stdlibInfo.importPath}";`;
                const result = await runtime.evalCode(importCode);
                if (!result.success) {
                    vscode.window.showErrorMessage(`Failed to load synthdef: ${result.error || 'Unknown error'}`);
                    this._panel.webview.postMessage({ type: 'testStopped' });
                    return;
                }
            }
            // Create a test voice using the synthdef
            const voiceName = SampleBrowser.TEST_VOICE_NAME;
            const voice = await runtime.createVoice({
                name: voiceName,
                synth_name: synthdefName,
                group_path: 'main',
                polyphony: 1,
                gain: 0.7,
            });
            if (!voice) {
                vscode.window.showErrorMessage(`Failed to create test voice for ${synthdefName}`);
                this._panel.webview.postMessage({ type: 'testStopped' });
                return;
            }
            // Play a test note (middle C, note 60)
            await runtime.noteOn(voiceName, 60, 100);
            // Schedule note-off after 1 second
            this._testNoteOffTimeout = setTimeout(async () => {
                try {
                    await runtime.noteOff(voiceName, 60);
                    // Wait a bit for the release, then clean up
                    setTimeout(async () => {
                        await this._cleanupTestVoice();
                        this._panel.webview.postMessage({ type: 'testStopped' });
                    }, 500);
                }
                catch {
                    // Ignore errors during cleanup
                }
            }, 1000);
        }
        catch (err) {
            vscode.window.showErrorMessage(`Test play failed: ${err}`);
            this._panel.webview.postMessage({ type: 'testStopped' });
        }
    }
    async _stopTestSynthDef(_synthdefName) {
        const runtime = this._store.runtime;
        if (!runtime)
            return;
        // Cancel any pending note-off
        if (this._testNoteOffTimeout) {
            clearTimeout(this._testNoteOffTimeout);
            this._testNoteOffTimeout = null;
        }
        try {
            const voiceName = SampleBrowser.TEST_VOICE_NAME;
            // Send note-off for any playing notes
            await runtime.noteOff(voiceName, 60);
            // Clean up the test voice
            await this._cleanupTestVoice();
        }
        catch {
            // Ignore errors - voice might not exist
        }
    }
    async _cleanupTestVoice() {
        const runtime = this._store.runtime;
        if (!runtime)
            return;
        try {
            await runtime.deleteVoice(SampleBrowser.TEST_VOICE_NAME);
        }
        catch {
            // Ignore - voice might not exist
        }
    }
    async _viewSynthDefSource(synthdefName) {
        const synthdef = this._store.state?.synthdefs.find(s => s.name === synthdefName);
        const stdlibInfo = this._stdlibData?.synthdefs.find(s => s.name === synthdefName);
        // Check if we have source location info from runtime
        if (synthdef && synthdef.source_location?.file) {
            const loc = synthdef.source_location;
            vscode.commands.executeCommand('vibelang.goToSource', loc);
            return;
        }
        // For stdlib synthdefs, show import path and offer to view source
        if (stdlibInfo || synthdef?.source === 'stdlib') {
            const info = stdlibInfo;
            if (info) {
                // Try to find and open the stdlib file
                const stdlibPath = this._findStdlibFile(info.sourcePath);
                if (stdlibPath) {
                    const doc = await vscode.workspace.openTextDocument(stdlibPath);
                    await vscode.window.showTextDocument(doc);
                    return;
                }
            }
            // Fallback: show info message
            const importPath = info?.importPath || `stdlib/${synthdefName}.vibe`;
            vscode.window.showInformationMessage(`"${synthdefName}" is from the standard library.\nImport: ${importPath}`, 'Copy Import').then(selection => {
                if (selection === 'Copy Import') {
                    vscode.env.clipboard.writeText(`import "${importPath}";`);
                }
            });
            return;
        }
        // For builtin synthdefs, they're compiled into the runtime
        if (synthdef?.source === 'builtin') {
            vscode.window.showInformationMessage(`"${synthdefName}" is a built-in synthdef compiled into the VibeLang runtime.`);
            return;
        }
        // For unknown synthdefs
        if (!synthdef && !stdlibInfo) {
            vscode.window.showErrorMessage(`SynthDef "${synthdefName}" not found`);
            return;
        }
        // For user synthdefs without source location
        vscode.window.showInformationMessage(`Source location for "${synthdefName}" is not available. Define the synthdef using synthdef() in your code.`);
    }
    _findStdlibFile(sourcePath) {
        // Try to find the stdlib file in common locations
        const possiblePaths = [
            // User's extracted stdlib
            path.join(process.env.HOME || '', '.local', 'share', 'vibelang', 'stdlib', sourcePath),
            path.join(process.env.HOME || '', 'vibelang', 'stdlib', sourcePath),
            // Development location
            path.join(this._extensionPath, '..', 'crates', 'vibelang-std', 'stdlib', sourcePath),
        ];
        for (const p of possiblePaths) {
            if (fs.existsSync(p)) {
                return p;
            }
        }
        return null;
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
    <title>Sample Browser</title>
    <style>
        :root {
            --bg-primary: #1a1a1a;
            --bg-secondary: #232323;
            --bg-tertiary: #2d2d2d;
            --text-primary: #d4d4d4;
            --text-secondary: #858585;
            --text-muted: #5a5a5a;
            --accent-green: #9bbb59;
            --accent-orange: #d19a66;
            --accent-blue: #569cd6;
            --accent-purple: #c586c0;
            --accent-red: #d16969;
            --border: #3c3c3c;
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
            height: 100vh;
            display: flex;
            flex-direction: column;
        }

        /* Toolbar */
        .toolbar {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 8px 12px;
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border);
        }

        .toolbar-title {
            font-weight: 600;
            font-size: 13px;
        }

        .search-box {
            flex: 1;
            padding: 6px 10px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: 11px;
        }

        .search-box:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        .btn {
            padding: 6px 12px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: var(--bg-tertiary);
            color: var(--text-primary);
            cursor: pointer;
            font-size: 11px;
            transition: all 0.1s ease;
        }

        .btn:hover {
            background: #3a3a3a;
            border-color: var(--text-secondary);
        }

        .btn-primary {
            background: var(--accent-blue);
            border-color: var(--accent-blue);
            color: white;
        }

        .btn-primary:hover {
            background: #4a8cc8;
        }

        /* Tabs */
        .tabs {
            display: flex;
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border);
        }

        .tab {
            padding: 10px 20px;
            cursor: pointer;
            font-size: 11px;
            font-weight: 500;
            color: var(--text-secondary);
            border-bottom: 2px solid transparent;
            transition: all 0.1s ease;
        }

        .tab:hover {
            color: var(--text-primary);
            background: var(--bg-tertiary);
        }

        .tab.active {
            color: var(--accent-blue);
            border-bottom-color: var(--accent-blue);
        }

        /* Content */
        .content {
            flex: 1;
            overflow: hidden;
            display: flex;
            flex-direction: column;
        }

        .tab-content {
            flex: 1;
            overflow-y: auto;
            display: none;
        }

        .tab-content.active {
            display: block;
        }

        /* List Items */
        .item-list {
            padding: 8px;
        }

        .item {
            display: flex;
            align-items: center;
            gap: 10px;
            padding: 10px 12px;
            background: var(--bg-secondary);
            border-radius: 4px;
            margin-bottom: 6px;
            cursor: pointer;
            transition: all 0.1s ease;
        }

        .item:hover {
            background: var(--bg-tertiary);
        }

        .item.selected {
            background: rgba(86, 156, 214, 0.2);
            border: 1px solid var(--accent-blue);
        }

        .item-icon {
            width: 36px;
            height: 36px;
            border-radius: 4px;
            background: var(--bg-tertiary);
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 16px;
            flex-shrink: 0;
        }

        .item-icon.sample {
            background: rgba(155, 187, 89, 0.2);
            color: var(--accent-green);
        }

        .item-icon.synth {
            background: rgba(197, 134, 192, 0.2);
            color: var(--accent-purple);
        }

        .item-icon.sfz {
            background: rgba(209, 154, 102, 0.2);
            color: var(--accent-orange);
        }

        .item-info {
            flex: 1;
            min-width: 0;
        }

        .item-name {
            font-weight: 500;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }

        .item-meta {
            font-size: 10px;
            color: var(--text-muted);
            margin-top: 2px;
        }

        .item-actions {
            display: flex;
            gap: 4px;
            opacity: 0;
            transition: opacity 0.1s ease;
        }

        .item:hover .item-actions {
            opacity: 1;
        }

        .item-btn {
            width: 26px;
            height: 26px;
            border: none;
            border-radius: 4px;
            background: var(--bg-tertiary);
            color: var(--text-secondary);
            cursor: pointer;
            font-size: 12px;
            display: flex;
            align-items: center;
            justify-content: center;
            transition: all 0.1s ease;
        }

        .item-btn:hover {
            background: var(--accent-blue);
            color: white;
        }

        .item-btn.play:hover {
            background: var(--accent-green);
        }

        .item-btn.stop {
            background: var(--accent-red);
            color: white;
        }

        .item-btn.stop:hover {
            background: #c05555;
        }

        /* Tags */
        .tag {
            font-size: 9px;
            padding: 2px 6px;
            border-radius: 3px;
            text-transform: uppercase;
            font-weight: 600;
        }

        .tag.builtin {
            background: rgba(86, 156, 214, 0.2);
            color: var(--accent-blue);
        }

        .tag.stdlib {
            background: rgba(155, 187, 89, 0.2);
            color: var(--accent-green);
        }

        .tag.user {
            background: rgba(209, 154, 102, 0.2);
            color: var(--accent-orange);
        }

        /* Empty State */
        .empty-state {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            padding: 40px;
            color: var(--text-secondary);
            text-align: center;
        }

        .empty-icon {
            font-size: 48px;
            margin-bottom: 16px;
            opacity: 0.5;
        }

        .empty-state h3 {
            font-size: 14px;
            font-weight: 500;
            margin-bottom: 8px;
        }

        .empty-state p {
            font-size: 12px;
            max-width: 300px;
            line-height: 1.5;
        }

        /* Detail Panel */
        .detail-panel {
            background: var(--bg-secondary);
            border-top: 1px solid var(--border);
            padding: 12px;
            display: none;
            max-height: 60vh;
            overflow-y: auto;
        }

        .detail-panel.visible {
            display: block;
        }

        .detail-header {
            display: flex;
            align-items: center;
            gap: 10px;
            margin-bottom: 12px;
        }

        .detail-name {
            font-size: 14px;
            font-weight: 600;
            flex: 1;
        }

        .detail-close {
            width: 24px;
            height: 24px;
            border: none;
            background: transparent;
            color: var(--text-muted);
            cursor: pointer;
            font-size: 14px;
        }

        .detail-close:hover {
            color: var(--text-primary);
        }

        .detail-info {
            display: grid;
            grid-template-columns: auto 1fr;
            gap: 6px 12px;
            font-size: 11px;
            margin-bottom: 12px;
        }

        .detail-label {
            color: var(--text-muted);
        }

        .detail-value {
            color: var(--text-primary);
            word-break: break-all;
        }

        /* Code Snippets Section */
        .code-section {
            margin-top: 16px;
        }

        .code-section-title {
            font-size: 11px;
            font-weight: 600;
            color: var(--text-secondary);
            text-transform: uppercase;
            margin-bottom: 8px;
            display: flex;
            align-items: center;
            gap: 8px;
        }

        .code-block {
            background: var(--bg-primary);
            border-radius: 4px;
            padding: 10px;
            font-family: 'SF Mono', Monaco, 'Consolas', monospace;
            font-size: 11px;
            position: relative;
            margin-bottom: 8px;
            overflow-x: auto;
        }

        .code-block pre {
            margin: 0;
            white-space: pre-wrap;
            word-break: break-all;
        }

        .code-block .copy-btn {
            position: absolute;
            top: 6px;
            right: 6px;
            padding: 4px 8px;
            font-size: 10px;
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            border-radius: 3px;
            color: var(--text-secondary);
            cursor: pointer;
        }

        .code-block .copy-btn:hover {
            background: var(--accent-blue);
            border-color: var(--accent-blue);
            color: white;
        }

        /* Syntax highlighting */
        .code-keyword { color: var(--accent-purple); }
        .code-string { color: var(--accent-green); }
        .code-function { color: var(--accent-blue); }
        .code-comment { color: var(--text-muted); font-style: italic; }
        .code-number { color: var(--accent-orange); }

        /* Slice Section */
        .slice-section {
            margin-top: 16px;
            padding-top: 16px;
            border-top: 1px solid var(--border);
        }

        .slice-controls {
            display: flex;
            gap: 8px;
            margin-bottom: 12px;
            align-items: center;
            flex-wrap: wrap;
        }

        .slice-input {
            width: 60px;
            padding: 4px 8px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: 11px;
        }

        .slice-input:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        .slice-label {
            font-size: 11px;
            color: var(--text-secondary);
        }

        .slice-preview {
            background: var(--bg-primary);
            border-radius: 4px;
            padding: 8px;
            margin-bottom: 12px;
        }

        .slice-bar {
            height: 24px;
            background: rgba(155, 187, 89, 0.3);
            border-radius: 4px;
            position: relative;
            display: flex;
        }

        .slice-segment {
            flex: 1;
            border-right: 1px solid var(--border);
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 9px;
            color: var(--text-muted);
        }

        .slice-segment:last-child {
            border-right: none;
        }

        .slice-segment:nth-child(odd) {
            background: rgba(155, 187, 89, 0.2);
        }

        .detail-params {
            background: var(--bg-primary);
            border-radius: 4px;
            padding: 8px;
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 10px;
            max-height: 120px;
            overflow-y: auto;
        }

        .param-row {
            display: flex;
            justify-content: space-between;
            padding: 2px 0;
        }

        .param-name {
            color: var(--accent-purple);
        }

        .param-value {
            color: var(--text-secondary);
        }

        /* Scrollbar */
        ::-webkit-scrollbar {
            width: 8px;
            height: 8px;
        }

        ::-webkit-scrollbar-track {
            background: var(--bg-primary);
        }

        ::-webkit-scrollbar-thumb {
            background: var(--bg-tertiary);
            border-radius: 4px;
        }

        ::-webkit-scrollbar-thumb:hover {
            background: #404040;
        }
    </style>
</head>
<body>
    <div class="toolbar">
        <span class="toolbar-title">Browser</span>
        <input type="text" class="search-box" id="searchBox" placeholder="Search samples and presets...">
        <button class="btn btn-primary" id="loadBtn">+ Load</button>
    </div>

    <div class="tabs">
        <div class="tab active" data-tab="samples">Samples</div>
        <div class="tab" data-tab="synthdefs">SynthDefs</div>
    </div>

    <div class="content">
        <div class="tab-content active" id="samplesTab">
            <div class="item-list" id="samplesList"></div>
        </div>
        <div class="tab-content" id="synthdefsTab">
            <div class="item-list" id="synthdefsList"></div>
        </div>
    </div>

    <div class="detail-panel" id="detailPanel">
        <div class="detail-header">
            <span class="detail-name" id="detailName">-</span>
            <button class="detail-close" id="detailClose">×</button>
        </div>
        <div class="detail-info" id="detailInfo"></div>
        <div id="detailContent"></div>
    </div>

    <script>
        const vscode = acquireVsCodeApi();

        let state = {
            samples: [],
            synthdefs: [],
            voices: []
        };

        let searchQuery = '';
        let selectedItem = null;
        let activeTab = 'samples';
        let sliceCount = 4;
        let playingTestSynthdef = null; // Track currently playing test synthdef

        // Tab switching
        document.querySelectorAll('.tab').forEach(tab => {
            tab.addEventListener('click', () => {
                document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
                document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
                tab.classList.add('active');
                activeTab = tab.dataset.tab;
                document.getElementById(activeTab + 'Tab').classList.add('active');
                selectedItem = null;
                document.getElementById('detailPanel').classList.remove('visible');
                render();
            });
        });

        // Search
        document.getElementById('searchBox').addEventListener('input', (e) => {
            searchQuery = e.target.value.toLowerCase();
            render();
        });

        // Load button
        document.getElementById('loadBtn').addEventListener('click', () => {
            vscode.postMessage({ command: 'loadSample' });
        });

        // Detail panel close
        document.getElementById('detailClose').addEventListener('click', () => {
            selectedItem = null;
            document.getElementById('detailPanel').classList.remove('visible');
        });

        // Message handler
        window.addEventListener('message', (event) => {
            const message = event.data;
            if (message.type === 'stateUpdate') {
                state = message.data;
                render();
            } else if (message.type === 'testStopped') {
                playingTestSynthdef = null;
                render();
            }
        });

        function escapeHtml(str) {
            return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
        }

        function highlightCode(code) {
            // Simple syntax highlighting - uses RegExp constructor to avoid template literal escaping issues
            var keywords = ['let', 'fn', 'if', 'else', 'for', 'while', 'return', 'true', 'false'];
            var functions = ['load_sample', 'voice', 'pattern', 'melody', 'group', 'synth', 'sample', 'start', 'stop', 'slice', 'analyze_bpm', 'warp_to_bpm', 'semitones', 'set_speed', 'set_pitch', 'attack', 'release', 'offset', 'length', 'loop_mode', 'amp', 'param'];

            var result = code;

            // Handle comments (lines containing //)
            var slash = String.fromCharCode(47);
            var commentRe = new RegExp('(' + slash + slash + '.*)', 'gm');
            result = result.replace(commentRe, '<span class="code-comment">$1</span>');

            // Handle strings
            result = result.replace(/"([^"]*)"/g, '<span class="code-string">"$1"</span>');

            // Handle keywords
            keywords.forEach(function(kw) {
                var re = new RegExp('(^|[^a-zA-Z_])(' + kw + ')([^a-zA-Z0-9_]|$)', 'g');
                result = result.replace(re, '$1<span class="code-keyword">$2</span>$3');
            });

            // Handle functions
            functions.forEach(function(fn) {
                var re = new RegExp('(^|[^a-zA-Z_])(' + fn + ')([^a-zA-Z0-9_]|$)', 'g');
                result = result.replace(re, '$1<span class="code-function">$2</span>$3');
            });

            // Handle numbers
            var numRe = new RegExp('([^a-zA-Z_])([0-9]+[.]?[0-9]*)([^a-zA-Z0-9_]|$)', 'g');
            result = result.replace(numRe, '$1<span class="code-number">$2</span>$3');

            return result;
        }

        function render() {
            if (activeTab === 'samples') {
                renderSamples();
            } else {
                renderSynthdefs();
            }
        }

        function renderSamples() {
            const list = document.getElementById('samplesList');
            const samples = state.samples.filter(s => {
                if (!searchQuery) return true;
                return s.id.toLowerCase().includes(searchQuery) ||
                       s.path.toLowerCase().includes(searchQuery);
            });

            if (samples.length === 0) {
                list.innerHTML = \`
                    <div class="empty-state">
                        <div class="empty-icon">🎵</div>
                        <h3>No Samples Loaded</h3>
                        <p>Click "Load" to add audio samples or SFZ instruments to your project.</p>
                    </div>
                \`;
                return;
            }

            list.innerHTML = samples.map(sample => {
                const isSfz = sample.path.endsWith('.sfz');
                const duration = sample.sample_rate > 0
                    ? (sample.num_frames / sample.sample_rate).toFixed(2) + 's'
                    : '-';
                const channels = sample.num_channels === 1 ? 'Mono' : 'Stereo';
                const fileName = sample.path.split('/').pop();

                return \`
                    <div class="item \${selectedItem === sample.id ? 'selected' : ''}"
                         data-id="\${sample.id}" data-type="sample">
                        <div class="item-icon \${isSfz ? 'sfz' : 'sample'}">
                            \${isSfz ? '🎹' : '🔊'}
                        </div>
                        <div class="item-info">
                            <div class="item-name">\${escapeHtml(sample.id)}</div>
                            <div class="item-meta">\${escapeHtml(fileName)} • \${channels} • \${duration}</div>
                        </div>
                        <div class="item-actions">
                            <button class="item-btn play" data-action="preview" title="Preview">▶</button>
                            <button class="item-btn" data-action="insert" title="Insert Code">⟨/⟩</button>
                            <button class="item-btn" data-action="copy" title="Copy Path">📋</button>
                        </div>
                    </div>
                \`;
            }).join('');

            // Event listeners
            list.querySelectorAll('.item').forEach(item => {
                item.addEventListener('click', (e) => {
                    if (!e.target.closest('.item-btn')) {
                        const id = item.dataset.id;
                        selectedItem = id;
                        showSampleDetail(state.samples.find(s => s.id === id));
                        render();
                    }
                });

                item.querySelectorAll('.item-btn').forEach(btn => {
                    btn.addEventListener('click', (e) => {
                        e.stopPropagation();
                        const action = btn.dataset.action;
                        const id = item.dataset.id;
                        if (action === 'preview') {
                            vscode.postMessage({ command: 'previewSample', sampleId: id });
                        } else if (action === 'insert') {
                            vscode.postMessage({ command: 'insertSampleCode', sampleId: id });
                        } else if (action === 'copy') {
                            const sample = state.samples.find(s => s.id === id);
                            if (sample) {
                                vscode.postMessage({ command: 'copyToClipboard', text: sample.path });
                            }
                        }
                    });
                });
            });
        }

        function renderSynthdefs() {
            const list = document.getElementById('synthdefsList');
            const synthdefs = state.synthdefs.filter(s => {
                if (!searchQuery) return true;
                return s.name.toLowerCase().includes(searchQuery);
            });

            // Group by source
            const groups = {
                stdlib: synthdefs.filter(s => s.source === 'stdlib'),
                builtin: synthdefs.filter(s => s.source === 'builtin'),
                user: synthdefs.filter(s => s.source === 'user' || !s.source)
            };

            if (synthdefs.length === 0) {
                list.innerHTML = \`
                    <div class="empty-state">
                        <div class="empty-icon">🎛️</div>
                        <h3>No SynthDefs Found</h3>
                        <p>SynthDefs will appear here when a VibeLang session is active.</p>
                    </div>
                \`;
                return;
            }

            let html = '';

            if (groups.stdlib.length > 0) {
                html += '<h4 style="padding: 8px 12px; color: var(--text-muted); font-size: 10px; text-transform: uppercase;">Standard Library</h4>';
                html += groups.stdlib.map(s => renderSynthdefItem(s)).join('');
            }

            if (groups.builtin.length > 0) {
                html += '<h4 style="padding: 8px 12px; color: var(--text-muted); font-size: 10px; text-transform: uppercase;">Built-in</h4>';
                html += groups.builtin.map(s => renderSynthdefItem(s)).join('');
            }

            if (groups.user.length > 0) {
                html += '<h4 style="padding: 8px 12px; color: var(--text-muted); font-size: 10px; text-transform: uppercase;">User Defined</h4>';
                html += groups.user.map(s => renderSynthdefItem(s)).join('');
            }

            list.innerHTML = html;

            // Event listeners
            list.querySelectorAll('.item').forEach(item => {
                item.addEventListener('click', (e) => {
                    if (!e.target.closest('.item-btn')) {
                        const name = item.dataset.id;
                        selectedItem = name;
                        showSynthdefDetail(state.synthdefs.find(s => s.name === name));
                        render();
                    }
                });

                item.querySelectorAll('.item-btn').forEach(btn => {
                    btn.addEventListener('click', (e) => {
                        e.stopPropagation();
                        const action = btn.dataset.action;
                        const name = item.dataset.id;
                        if (action === 'test') {
                            playingTestSynthdef = name;
                            render();
                            vscode.postMessage({ command: 'testSynthDef', synthdefName: name });
                        } else if (action === 'stopTest') {
                            playingTestSynthdef = null;
                            render();
                            vscode.postMessage({ command: 'stopTestSynthDef', synthdefName: name });
                        } else if (action === 'source') {
                            vscode.postMessage({ command: 'viewSynthDefSource', synthdefName: name });
                        } else if (action === 'insert') {
                            vscode.postMessage({ command: 'insertSynthDefCode', synthdefName: name });
                        } else if (action === 'copy') {
                            vscode.postMessage({ command: 'copyToClipboard', text: name });
                        }
                    });
                });
            });
        }

        function renderSynthdefItem(synthdef) {
            const paramCount = synthdef.params.length;
            const sourceTag = synthdef.source
                ? \`<span class="tag \${synthdef.source}">\${synthdef.source}</span>\`
                : '';
            const isPlaying = playingTestSynthdef === synthdef.name;

            // Get stdlib metadata if available
            const stdlib = synthdef._stdlib;
            const typeIcon = stdlib?.type === 'effect' ? '🔊' : '🎛️';
            const typeTag = stdlib?.type
                ? \`<span class="tag \${stdlib.type === 'effect' ? 'builtin' : 'stdlib'}">\${stdlib.type}</span>\`
                : '';
            const categoryInfo = stdlib?.category
                ? \`\${stdlib.category}\${stdlib.subcategory ? '/' + stdlib.subcategory : ''}\`
                : '';
            const description = stdlib?.description || '';
            const metaText = description
                ? description.substring(0, 60) + (description.length > 60 ? '...' : '')
                : \`\${paramCount} parameters\${categoryInfo ? ' • ' + categoryInfo : ''}\`;

            return \`
                <div class="item \${selectedItem === synthdef.name ? 'selected' : ''}"
                     data-id="\${synthdef.name}" data-type="synthdef">
                    <div class="item-icon synth">\${typeIcon}</div>
                    <div class="item-info">
                        <div class="item-name">\${escapeHtml(synthdef.name)} \${sourceTag} \${typeTag}</div>
                        <div class="item-meta">\${escapeHtml(metaText)}</div>
                    </div>
                    <div class="item-actions">
                        <button class="item-btn \${isPlaying ? 'stop' : 'play'}" data-action="\${isPlaying ? 'stopTest' : 'test'}" title="\${isPlaying ? 'Stop' : 'Test Play'}">\${isPlaying ? '⏹' : '▶'}</button>
                        <button class="item-btn" data-action="source" title="View Source">📄</button>
                        <button class="item-btn" data-action="insert" title="Insert Code">⟨/⟩</button>
                        <button class="item-btn" data-action="copy" title="Copy Name">📋</button>
                    </div>
                </div>
            \`;
        }

        function showSampleDetail(sample) {
            if (!sample) return;

            const panel = document.getElementById('detailPanel');
            const info = document.getElementById('detailInfo');
            const content = document.getElementById('detailContent');

            document.getElementById('detailName').textContent = sample.id;

            const duration = sample.sample_rate > 0
                ? (sample.num_frames / sample.sample_rate).toFixed(3)
                : 0;
            const durationStr = duration + ' sec';

            info.innerHTML = \`
                <span class="detail-label">Path:</span>
                <span class="detail-value">\${escapeHtml(sample.path)}</span>
                <span class="detail-label">Channels:</span>
                <span class="detail-value">\${sample.num_channels}</span>
                <span class="detail-label">Sample Rate:</span>
                <span class="detail-value">\${sample.sample_rate} Hz</span>
                <span class="detail-label">Duration:</span>
                <span class="detail-value">\${durationStr}</span>
                <span class="detail-label">Frames:</span>
                <span class="detail-value">\${sample.num_frames.toLocaleString()}</span>
                <span class="detail-label">Buffer ID:</span>
                <span class="detail-value">\${sample.buffer_id}</span>
            \`;

            // Generate code snippets
            const loadCode = \`let \${sample.id} = load_sample("\${sample.id}", "\${sample.path}");\`;
            const voiceCode = \`let \${sample.id}_voice = voice("\${sample.id}_voice")
    .sample(\${sample.id})
    .group(my_group);\`;
            const patternCode = \`let \${sample.id}_pattern = pattern("\${sample.id}_pattern", \${sample.id}_voice, "x...x...x...x...");
\${sample.id}_pattern.start();\`;

            // Generate slice code based on current slice count
            let slicePreview = '';
            for (let i = 0; i < sliceCount; i++) {
                slicePreview += \`<div class="slice-segment">\${i + 1}</div>\`;
            }

            const sliceDuration = duration / sliceCount;
            let sliceCode = \`// Slice into \${sliceCount} equal parts\\n\`;
            for (let i = 0; i < sliceCount; i++) {
                const start = (i * sliceDuration).toFixed(3);
                const end = ((i + 1) * sliceDuration).toFixed(3);
                sliceCode += \`let slice_\${i + 1} = \${sample.id}.slice(\${start}, \${end});\\n\`;
            }

            // Time-stretch/pitch-shift code
            const warpCode = \`// Analyze BPM and warp to target tempo
let \${sample.id}_warped = load_sample("\${sample.id}", "\${sample.path}")
    .analyze_bpm()
    .warp_to_bpm(120.0);

// Or manually set speed/pitch
let \${sample.id}_pitched = load_sample("\${sample.id}", "\${sample.path}")
    .semitones(-5)      // Pitch down 5 semitones
    .set_speed(0.5);    // Half speed\`;

            content.innerHTML = \`
                <div class="code-section">
                    <div class="code-section-title">Load Sample</div>
                    <div class="code-block">
                        <button class="copy-btn" onclick="copyCode(this)">Copy</button>
                        <pre>\${highlightCode(escapeHtml(loadCode))}</pre>
                    </div>
                </div>

                <div class="code-section">
                    <div class="code-section-title">Create Voice</div>
                    <div class="code-block">
                        <button class="copy-btn" onclick="copyCode(this)">Copy</button>
                        <pre>\${highlightCode(escapeHtml(voiceCode))}</pre>
                    </div>
                </div>

                <div class="code-section">
                    <div class="code-section-title">Create Pattern</div>
                    <div class="code-block">
                        <button class="copy-btn" onclick="copyCode(this)">Copy</button>
                        <pre>\${highlightCode(escapeHtml(patternCode))}</pre>
                    </div>
                </div>

                <div class="slice-section">
                    <div class="code-section-title">Sample Slicing</div>
                    <div class="slice-controls">
                        <span class="slice-label">Slices:</span>
                        <input type="number" class="slice-input" id="sliceCountInput" value="\${sliceCount}" min="2" max="64">
                        <button class="btn" onclick="updateSlices()">Update</button>
                    </div>
                    <div class="slice-preview">
                        <div class="slice-bar">
                            \${slicePreview}
                        </div>
                    </div>
                    <div class="code-block">
                        <button class="copy-btn" onclick="copyCode(this)">Copy</button>
                        <pre>\${highlightCode(escapeHtml(sliceCode))}</pre>
                    </div>
                </div>

                <div class="code-section">
                    <div class="code-section-title">Time-Stretch / Pitch-Shift</div>
                    <div class="code-block">
                        <button class="copy-btn" onclick="copyCode(this)">Copy</button>
                        <pre>\${highlightCode(escapeHtml(warpCode))}</pre>
                    </div>
                </div>
            \`;

            panel.classList.add('visible');
        }

        function showSynthdefDetail(synthdef) {
            if (!synthdef) return;

            const panel = document.getElementById('detailPanel');
            const info = document.getElementById('detailInfo');
            const content = document.getElementById('detailContent');

            document.getElementById('detailName').textContent = synthdef.name;

            info.innerHTML = \`
                <span class="detail-label">Source:</span>
                <span class="detail-value">\${synthdef.source || 'unknown'}</span>
                <span class="detail-label">Parameters:</span>
                <span class="detail-value">\${synthdef.params.length}</span>
            \`;

            // Generate import path if from stdlib
            const stdlib = synthdef._stdlib;
            const importPath = stdlib?.importPath;
            const importCode = importPath ? \`import "\${importPath}";\` : null;

            // Generate voice code with all parameters (without .group() call)
            const paramsCode = synthdef.params
                .filter(p => p.name !== 'out' && p.name !== 'amp' && p.name !== 'gate')
                .map(p => \`    .param("\${p.name}", \${p.default_value})\`)
                .join('\\n');

            const voiceCode = \`let my_voice = voice("my_voice")
    .synth("\${synthdef.name}")\${paramsCode ? '\\n' + paramsCode : ''};\`;

            // Parameter list
            let paramsHtml = '';
            if (synthdef.params.length > 0) {
                paramsHtml = synthdef.params.map(p => \`
                    <div class="param-row">
                        <span class="param-name">\${escapeHtml(p.name)}</span>
                        <span class="param-value">\${p.default_value}\${p.min_value != null ? \` (\${p.min_value} - \${p.max_value})\` : ''}</span>
                    </div>
                \`).join('');
            } else {
                paramsHtml = '<em style="color: var(--text-muted)">No parameters</em>';
            }

            // Build import section HTML if applicable
            const importSectionHtml = importCode ? \`
                <div class="code-section">
                    <div class="code-section-title">Import</div>
                    <div class="code-block">
                        <button class="copy-btn" onclick="copyCode(this)">Copy</button>
                        <pre>\${highlightCode(escapeHtml(importCode))}</pre>
                    </div>
                </div>
            \` : '';

            content.innerHTML = \`
                \${importSectionHtml}

                <div class="code-section">
                    <div class="code-section-title">Create Voice</div>
                    <div class="code-block">
                        <button class="copy-btn" onclick="copyCode(this)">Copy</button>
                        <pre>\${highlightCode(escapeHtml(voiceCode))}</pre>
                    </div>
                </div>

                <div class="code-section">
                    <div class="code-section-title">Parameters</div>
                    <div class="detail-params">
                        \${paramsHtml}
                    </div>
                </div>
            \`;

            panel.classList.add('visible');
        }

        function copyCode(btn) {
            const codeBlock = btn.parentElement;
            const pre = codeBlock.querySelector('pre');
            // Get text content without HTML tags
            const text = pre.textContent;
            vscode.postMessage({ command: 'copyToClipboard', text: text });
            btn.textContent = 'Copied!';
            setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
        }

        function updateSlices() {
            const input = document.getElementById('sliceCountInput');
            const newCount = parseInt(input.value, 10);
            if (newCount >= 2 && newCount <= 64) {
                sliceCount = newCount;
                // Re-render the detail panel
                const sample = state.samples.find(s => s.id === selectedItem);
                if (sample) {
                    showSampleDetail(sample);
                }
            }
        }

        // Initial render
        render();
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
    <title>Sample Browser</title>
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
        <div class="empty-icon">🎵</div>
        <h2>Not Connected</h2>
        <p>Connect to a VibeLang runtime to browse samples and presets.</p>
    </div>
</body>
</html>`;
    }
    dispose() {
        SampleBrowser.currentPanel = undefined;
        this._panel.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
exports.SampleBrowser = SampleBrowser;
SampleBrowser.viewType = 'vibelang.sampleBrowser';
SampleBrowser.TEST_VOICE_NAME = '_vibelang_sample_browser_test';
//# sourceMappingURL=sampleBrowser.js.map