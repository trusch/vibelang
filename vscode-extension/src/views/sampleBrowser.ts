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

import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { StateStore } from '../state/stateStore';
import { Sample, SynthDef } from '../api/types';
import { getWaveformData, WaveformData } from '../utils/waveformProvider';
import { TagStoreManager, TaggableType } from '../utils/tagStore';

// Type for stdlib synthdef metadata
interface StdlibSynthdef {
    name: string;
    type: 'instrument' | 'effect';
    description: string | null;
    category: string;
    subcategory: string | null;
    importPath: string;
    sourcePath: string;
    params: { name: string; default: number | string }[];
}

interface StdlibData {
    version: string;
    synthdefs: StdlibSynthdef[];
    categories: string[];
}

export class SampleBrowser {
    public static currentPanel: SampleBrowser | undefined;
    public static readonly viewType = 'vibelang.sampleBrowser';

    private readonly _panel: vscode.WebviewPanel;
    private readonly _store: StateStore;
    private readonly _context: vscode.ExtensionContext;
    private readonly _extensionPath: string;
    private readonly _tagStore: TagStoreManager;
    private _disposables: vscode.Disposable[] = [];
    private _stdlibData: StdlibData | null = null;

    private constructor(panel: vscode.WebviewPanel, store: StateStore, context: vscode.ExtensionContext) {
        this._panel = panel;
        this._store = store;
        this._context = context;
        this._extensionPath = context.extensionPath;
        this._tagStore = new TagStoreManager(context);

        // Load stdlib metadata
        this._loadStdlibData();

        this._updateContent();

        // Listen for state updates
        this._disposables.push(
            store.onFullUpdate(() => this._sendStateUpdate())
        );

        this._disposables.push(
            store.onStatusChange(() => this._updateContent())
        );

        // Listen for tag changes
        this._disposables.push(
            this._tagStore.onTagsChanged(() => this._sendStateUpdate())
        );

        // Handle messages from webview
        this._panel.webview.onDidReceiveMessage(
            (message) => this._handleMessage(message),
            null,
            this._disposables
        );

        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    }

    public static createOrShow(store: StateStore, context: vscode.ExtensionContext) {
        const column = vscode.ViewColumn.Beside;

        if (SampleBrowser.currentPanel) {
            SampleBrowser.currentPanel._panel.reveal(column);
            return;
        }

        const panel = vscode.window.createWebviewPanel(
            SampleBrowser.viewType,
            'Sample Browser',
            column,
            {
                enableScripts: true,
                retainContextWhenHidden: true,
            }
        );

        SampleBrowser.currentPanel = new SampleBrowser(panel, store, context);
    }

    public static revive(panel: vscode.WebviewPanel, store: StateStore, context: vscode.ExtensionContext) {
        SampleBrowser.currentPanel = new SampleBrowser(panel, store, context);
    }

    private _loadStdlibData() {
        try {
            const stdlibPath = path.join(this._extensionPath, 'src', 'data', 'stdlib.json');
            if (fs.existsSync(stdlibPath)) {
                const content = fs.readFileSync(stdlibPath, 'utf-8');
                this._stdlibData = JSON.parse(content);
                console.log(`Loaded ${this._stdlibData?.synthdefs.length || 0} stdlib synthdefs`);
            }
        } catch (err) {
            console.error('Failed to load stdlib data:', err);
        }
    }

    private _updateContent() {
        this._panel.webview.html = this._getHtmlContent();
        setTimeout(() => this._sendStateUpdate(), 100);
    }

    private _sendStateUpdate() {
        const state = this._store.state;

        // Build a map of stdlib synthdefs by name for quick lookup
        const stdlibByName = new Map<string, StdlibSynthdef>();
        for (const s of this._stdlibData?.synthdefs || []) {
            stdlibByName.set(s.name, s);
        }

        // Get runtime synthdefs and merge stdlib metadata if applicable
        const runtimeSynthdefs = (state?.synthdefs || []).map(s => {
            const stdlibInfo = stdlibByName.get(s.name);
            if (stdlibInfo) {
                // This runtime synthdef is from stdlib - preserve the stdlib source and metadata
                return {
                    ...s,
                    source: 'stdlib' as const,
                    _stdlib: {
                        type: stdlibInfo.type,
                        description: stdlibInfo.description,
                        category: stdlibInfo.category,
                        subcategory: stdlibInfo.subcategory,
                        importPath: stdlibInfo.importPath,
                        sourcePath: stdlibInfo.sourcePath,
                    }
                };
            }
            return s;
        });
        const loadedNames = new Set(runtimeSynthdefs.map(s => s.name));

        // Get stdlib synthdefs that aren't loaded yet
        const stdlibSynthdefs: SynthDef[] = (this._stdlibData?.synthdefs || [])
            .filter(s => !loadedNames.has(s.name))
            .map(s => ({
                name: s.name,
                source: 'stdlib' as const,
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
            } as SynthDef & { _stdlib: Record<string, unknown> }));

        this._panel.webview.postMessage({
            type: 'stateUpdate',
            data: {
                samples: state?.samples || [],
                synthdefs: [...runtimeSynthdefs, ...stdlibSynthdefs],
                voices: state?.voices || [],
                userTags: this._tagStore.getAllTagsMap(),
                allUserTags: this._tagStore.getAllUserTags(),
            },
        });
    }

    private async _handleMessage(message: { command: string; [key: string]: unknown }) {
        switch (message.command) {
            case 'loadSample':
                await this._loadSampleFromFile();
                break;

            case 'previewSample':
                await this._previewSample(message.sampleId as string);
                break;

            case 'stopPreview':
                await this._stopPreview();
                break;

            case 'insertSampleCode':
                await this._insertSampleCode(message.sampleId as string);
                break;

            case 'insertSynthDefCode':
                await this._insertSynthDefCode(message.synthdefName as string);
                break;

            case 'copyToClipboard':
                const text = message.text as string;
                await vscode.env.clipboard.writeText(text);
                vscode.window.showInformationMessage('Copied to clipboard');
                break;

            case 'goToSource':
                const location = message.sourceLocation as { file?: string; line?: number; column?: number };
                if (location?.file && location?.line) {
                    vscode.commands.executeCommand('vibelang.goToSource', location);
                }
                break;

            case 'testSynthDef':
                await this._testSynthDef(message.synthdefName as string);
                break;

            case 'stopTestSynthDef':
                await this._stopTestSynthDef(message.synthdefName as string);
                break;

            case 'viewSynthDefSource':
                await this._viewSynthDefSource(message.synthdefName as string);
                break;

            case 'testSynthDefAtPitch':
                await this._testSynthDefAtPitch(
                    message.synthdefName as string,
                    message.midiNote as number,
                    message.isOneShot as boolean,
                    message.sustained as boolean | undefined,
                    message.forceNewVoice as boolean | undefined
                );
                break;

            case 'testSampleAtPitch':
                await this._testSampleAtPitch(
                    message.sampleId as string,
                    message.midiNote as number,
                    message.isOneShot as boolean,
                    message.sustained as boolean | undefined,
                    message.forceNewVoice as boolean | undefined
                );
                break;

            case 'releaseTestNote':
                await this._releaseTestNote(message.midiNote as number);
                break;

            case 'stopTestVoice':
                await this._stopTestSynthDef('');
                break;

            case 'enterChoppingMode':
                await this._enterChoppingMode(message.sampleId as string);
                break;

            case 'playSampleFull':
                await this._playSampleFull(message.sampleId as string);
                break;

            case 'stopSamplePlayback':
                await this._stopSamplePlayback();
                break;

            case 'previewSlice':
                await this._previewSlice(
                    message.sampleId as string,
                    message.startSeconds as number,
                    message.endSeconds as number
                );
                break;

            case 'saveSlicesToFile':
                await this._saveSlicesToFile(
                    message.sampleId as string,
                    message.sliceCode as string
                );
                break;

            // Tag management commands
            case 'addTag':
                await this._tagStore.addTag(
                    message.type as TaggableType,
                    message.id as string,
                    message.tag as string
                );
                break;

            case 'removeTag':
                await this._tagStore.removeTag(
                    message.type as TaggableType,
                    message.id as string,
                    message.tag as string
                );
                break;

            case 'setTags':
                await this._tagStore.setUserTags(
                    message.type as TaggableType,
                    message.id as string,
                    message.tags as string[]
                );
                break;

            case 'toggleTag':
                await this._tagStore.toggleTag(
                    message.type as TaggableType,
                    message.id as string,
                    message.tag as string
                );
                break;
        }
    }

    private async _loadSampleFromFile() {
        // Check if there's an active .vibe file
        const editor = vscode.window.activeTextEditor;
        if (!editor || !editor.document.fileName.endsWith('.vibe')) {
            vscode.window.showWarningMessage('Please open a .vibe file first to load samples');
            return;
        }

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
            const mainFilePath = editor.document.fileName;
            const mainFileDir = path.dirname(mainFilePath);
            const mainFileName = path.basename(mainFilePath, '.vibe');

            // Determine the samples file path
            let samplesFileName = `${mainFileName}_samples.vibe`;
            let samplesFilePath = path.join(mainFileDir, samplesFileName);

            // Check if the samples file already exists
            const samplesFileExists = fs.existsSync(samplesFilePath);

            // Get existing sample names to avoid conflicts
            let existingNames = new Set<string>();
            if (samplesFileExists) {
                const existingContent = fs.readFileSync(samplesFilePath, 'utf-8');
                const nameRegex = /let\s+(\w+)\s*=\s*load_sample/g;
                let nameMatch;
                while ((nameMatch = nameRegex.exec(existingContent)) !== null) {
                    existingNames.add(nameMatch[1]);
                }
            }

            // Generate load_sample definitions for each selected file
            const loadSampleStatements = result.map(uri => {
                const fileName = uri.fsPath.split('/').pop()?.replace(/\.[^.]+$/, '') || 'sample';
                let safeName = fileName.replace(/[^a-zA-Z0-9_]/g, '_').toLowerCase();

                // Ensure unique name
                if (existingNames.has(safeName)) {
                    const hash = Math.random().toString(36).substring(2, 6);
                    safeName = `${safeName}_${hash}`;
                }
                existingNames.add(safeName);

                return `let ${safeName} = load_sample("${safeName}", "${uri.fsPath}");`;
            });

            if (samplesFileExists) {
                // Append to existing samples file
                const existingContent = fs.readFileSync(samplesFilePath, 'utf-8');
                const newContent = existingContent.trimEnd() + '\n\n' + loadSampleStatements.join('\n') + '\n';
                fs.writeFileSync(samplesFilePath, newContent);
            } else {
                // Create new samples file
                const header = `// Sample definitions for ${mainFileName}.vibe\n// Auto-generated by VibeLang Sample Browser\n\n`;
                const content = header + loadSampleStatements.join('\n') + '\n';
                fs.writeFileSync(samplesFilePath, content);

                // Add import to the main file if it doesn't exist
                const mainContent = editor.document.getText();
                const importStatement = `import "./${samplesFileName}";`;

                if (!mainContent.includes(importStatement)) {
                    // Find the best position to insert the import (after other imports or at the top)
                    const importRegex = /^import\s+["'].*["'];?\s*$/gm;
                    let lastImportMatch: RegExpExecArray | null = null;
                    let match: RegExpExecArray | null;
                    while ((match = importRegex.exec(mainContent)) !== null) {
                        lastImportMatch = match;
                    }

                    await editor.edit(editBuilder => {
                        if (lastImportMatch) {
                            // Insert after the last import
                            const insertPosition = editor.document.positionAt(lastImportMatch.index + lastImportMatch[0].length);
                            editBuilder.insert(insertPosition, '\n' + importStatement);
                        } else {
                            // Insert at the beginning of the file
                            editBuilder.insert(new vscode.Position(0, 0), importStatement + '\n\n');
                        }
                    });
                }
            }

            // Open the samples file and save it to trigger evaluation
            const samplesDoc = await vscode.workspace.openTextDocument(samplesFilePath);
            await vscode.window.showTextDocument(samplesDoc, vscode.ViewColumn.Beside);

            // Save the samples file
            await samplesDoc.save();

            // Also save the main file if we modified it (to trigger import processing)
            if (!samplesFileExists) {
                await editor.document.save();
            }

            // Directly evaluate the sample code to make samples immediately available
            const runtime = this._store.runtime;
            if (runtime) {
                try {
                    // Evaluate each sample definition
                    for (const stmt of loadSampleStatements) {
                        await runtime.evalCode(stmt);
                    }
                } catch (err) {
                    console.error('Failed to evaluate sample code:', err);
                }
            }

            vscode.window.showInformationMessage(
                `Added ${result.length} sample(s) to ${samplesFileName}`
            );

            // Force a state update to the webview after a short delay
            // (to allow runtime to fully process the newly loaded samples)
            setTimeout(() => {
                this._sendStateUpdate();
            }, 300);
        }
    }

    private _generateSampleCode(id: string, path: string): string {
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

    private async _previewSample(sampleId: string) {
        const runtime = this._store.runtime;
        if (!runtime) {
            vscode.window.showErrorMessage('Not connected to VibeLang runtime');
            return;
        }

        try {
            // Find the sample
            const sample = this._store.state?.samples.find(s => s.id === sampleId);
            if (!sample) {
                vscode.window.showErrorMessage(`Sample "${sampleId}" not found`);
                return;
            }

            // Clean up any existing test voice first
            await this._stopTestSynthDef(sampleId);

            // Create a test voice using the sample's synthdef
            const voiceName = SampleBrowser.TEST_VOICE_NAME;
            const voice = await runtime.createVoice({
                name: voiceName,
                synth_name: sample.synthdef_name,
                group_path: 'main',
                polyphony: 1,
                gain: 0.7,
            });

            if (!voice) {
                vscode.window.showErrorMessage(`Failed to create test voice for sample ${sampleId}`);
                return;
            }

            this._testVoiceActive = true;
            this._activeTestNotes.add(60);

            // Play the sample at middle C (note 60)
            await runtime.noteOn(voiceName, 60, 100);

            // Calculate duration based on sample length, capped for long samples
            const sampleDuration = sample.sample_rate > 0
                ? sample.num_frames / sample.sample_rate
                : 1;
            const playDuration = Math.min(sampleDuration * 1000, 5000); // Cap at 5 seconds

            // Schedule note-off after sample plays
            this._testNoteOffTimeout = setTimeout(async () => {
                this._testNoteOffTimeout = null;
                try {
                    await runtime.noteOff(voiceName, 60);
                    this._activeTestNotes.delete(60);
                } catch {
                    // Ignore - voice might have been cleaned up
                }
                // Schedule cleanup after release time
                this._testCleanupTimeout = setTimeout(async () => {
                    this._testCleanupTimeout = null;
                    await this._cleanupTestVoice();
                }, 500);
            }, playDuration);

        } catch (err) {
            vscode.window.showErrorMessage(`Preview failed: ${err}`);
        }
    }

    private async _stopPreview() {
        await this._stopTestSynthDef('');
    }

    private async _insertSampleCode(sampleId: string) {
        const sample = this._store.state?.samples.find(s => s.id === sampleId);
        if (!sample) return;

        const code = this._generateSampleCode(sample.id, sample.path);

        const editor = vscode.window.activeTextEditor;
        if (editor) {
            await editor.edit(edit => {
                edit.insert(editor.selection.active, code + '\n');
            });
        } else {
            await vscode.env.clipboard.writeText(code);
            vscode.window.showInformationMessage('Code copied to clipboard');
        }
    }

    private async _insertSynthDefCode(synthdefName: string) {
        const synthdef = this._store.state?.synthdefs.find(s => s.name === synthdefName);
        if (!synthdef) return;

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
        } else {
            await vscode.env.clipboard.writeText(code);
            vscode.window.showInformationMessage('Code copied to clipboard');
        }
    }

    private static readonly TEST_VOICE_NAME = '_vibelang_sample_browser_test';
    private _testNoteOffTimeout: ReturnType<typeof setTimeout> | null = null;
    private _testCleanupTimeout: ReturnType<typeof setTimeout> | null = null;
    private _testVoiceActive = false; // Track if we have an active test voice
    private _activeTestNotes: Set<number> = new Set(); // Track ALL active test notes for proper cleanup

    private async _testSynthDef(synthdefName: string) {
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

            this._testVoiceActive = true;
            this._activeTestNotes.add(60);

            // Play a test note (middle C, note 60)
            await runtime.noteOn(voiceName, 60, 100);

            // Schedule note-off after 1 second
            this._testNoteOffTimeout = setTimeout(async () => {
                this._testNoteOffTimeout = null;
                try {
                    await runtime.noteOff(voiceName, 60);
                    this._activeTestNotes.delete(60);
                } catch {
                    // Ignore - voice might have been cleaned up
                }
                // Schedule cleanup after release time
                this._testCleanupTimeout = setTimeout(async () => {
                    this._testCleanupTimeout = null;
                    await this._cleanupTestVoice();
                    this._panel.webview.postMessage({ type: 'testStopped' });
                }, 500);
            }, 1000);

        } catch (err) {
            vscode.window.showErrorMessage(`Test play failed: ${err}`);
            this._panel.webview.postMessage({ type: 'testStopped' });
        }
    }

    private async _stopTestSynthDef(_synthdefName: string) {
        const runtime = this._store.runtime;
        if (!runtime) return;

        // Cancel any pending timeouts
        if (this._testNoteOffTimeout) {
            clearTimeout(this._testNoteOffTimeout);
            this._testNoteOffTimeout = null;
        }
        if (this._testCleanupTimeout) {
            clearTimeout(this._testCleanupTimeout);
            this._testCleanupTimeout = null;
        }

        // Send note-off for ALL active notes (ignore errors)
        if (this._testVoiceActive && this._activeTestNotes.size > 0) {
            const noteOffs = Array.from(this._activeTestNotes).map(async (note) => {
                try {
                    await runtime.noteOff(SampleBrowser.TEST_VOICE_NAME, note);
                } catch {
                    // Ignore - voice might not exist
                }
            });
            await Promise.all(noteOffs);
            this._activeTestNotes.clear();
        }

        // Always attempt cleanup
        await this._cleanupTestVoice();
    }

    private _currentTestSynthdef: string | null = null; // Track which synthdef/sample the test voice is for

    private async _cleanupTestVoice() {
        const runtime = this._store.runtime;
        if (!runtime) return;

        // Only cleanup if we think there's an active voice
        if (this._testVoiceActive) {
            this._testVoiceActive = false;
            this._currentTestSynthdef = null;
            this._activeTestNotes.clear(); // Clear tracked notes
            try {
                await runtime.deleteVoice(SampleBrowser.TEST_VOICE_NAME);
            } catch {
                // Ignore - voice might not exist
            }
        }
    }

    private async _testSynthDefAtPitch(synthdefName: string, midiNote: number, isOneShot: boolean, sustained?: boolean, forceNewVoice?: boolean) {
        const runtime = this._store.runtime;
        if (!runtime) {
            vscode.window.showErrorMessage('Not connected to VibeLang runtime');
            this._panel.webview.postMessage({ type: 'testStopped' });
            return;
        }

        try {
            // Check if we need to recreate the voice (different synthdef or forced)
            const needNewVoice = forceNewVoice ||
                                 !this._testVoiceActive ||
                                 this._currentTestSynthdef !== synthdefName;

            // Clean up any existing test voice first (unless sustained with same synthdef)
            if (needNewVoice || !sustained) {
                await this._stopTestSynthDef(synthdefName);
            }

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

            // Create a test voice using the synthdef (if we don't have one already or need new one)
            const voiceName = SampleBrowser.TEST_VOICE_NAME;
            if (needNewVoice || !this._testVoiceActive) {
                const voice = await runtime.createVoice({
                    name: voiceName,
                    synth_name: synthdefName,
                    group_path: 'main',
                    polyphony: 8, // Allow multiple notes for sustained play
                    gain: 0.7,
                });

                if (!voice) {
                    vscode.window.showErrorMessage(`Failed to create test voice for ${synthdefName}`);
                    this._panel.webview.postMessage({ type: 'testStopped' });
                    return;
                }
                this._testVoiceActive = true;
                this._currentTestSynthdef = synthdefName;
            }

            // Track this note as active
            this._activeTestNotes.add(midiNote);

            // Play the note at the specified pitch
            await runtime.noteOn(voiceName, midiNote, 100);

            // For sustained notes (held keys), don't schedule auto note-off
            if (sustained) {
                // Note will be released when key is released via releaseTestNote
                return;
            }

            // For one-shot samples, use a shorter duration
            const noteDuration = isOneShot ? 100 : 1000; // 100ms for one-shot, 1s for sustained

            // Schedule note-off (capture midiNote in closure)
            const noteToRelease = midiNote;
            this._testNoteOffTimeout = setTimeout(async () => {
                this._testNoteOffTimeout = null;
                try {
                    await runtime.noteOff(voiceName, noteToRelease);
                    this._activeTestNotes.delete(noteToRelease);
                } catch {
                    // Ignore - voice might have been cleaned up
                }
                // Schedule cleanup after release time (only if no other notes active)
                this._testCleanupTimeout = setTimeout(async () => {
                    this._testCleanupTimeout = null;
                    if (this._activeTestNotes.size === 0) {
                        await this._cleanupTestVoice();
                        this._panel.webview.postMessage({ type: 'testStopped' });
                    }
                }, isOneShot ? 200 : 500);
            }, noteDuration);

        } catch (err) {
            vscode.window.showErrorMessage(`Test play failed: ${err}`);
            this._panel.webview.postMessage({ type: 'testStopped' });
        }
    }

    private async _testSampleAtPitch(sampleId: string, midiNote: number, isOneShot: boolean, sustained?: boolean, forceNewVoice?: boolean) {
        const runtime = this._store.runtime;
        if (!runtime) {
            vscode.window.showErrorMessage('Not connected to VibeLang runtime');
            return;
        }

        try {
            // Find the sample
            const sample = this._store.state?.samples.find(s => s.id === sampleId);
            if (!sample) {
                vscode.window.showErrorMessage(`Sample "${sampleId}" not found`);
                return;
            }

            // Check if we need to recreate the voice (different sample or forced)
            const needNewVoice = forceNewVoice ||
                                 !this._testVoiceActive ||
                                 this._currentTestSynthdef !== sampleId;

            // Clean up any existing test voice first (unless sustained with same sample)
            if (needNewVoice || !sustained) {
                await this._stopTestSynthDef(sampleId);
            }

            // Create a test voice using the sample's synthdef (if we don't have one already or need new one)
            const voiceName = SampleBrowser.TEST_VOICE_NAME;
            if (needNewVoice || !this._testVoiceActive) {
                const voice = await runtime.createVoice({
                    name: voiceName,
                    synth_name: sample.synthdef_name,
                    group_path: 'main',
                    polyphony: 8, // Allow multiple notes for sustained play
                    gain: 0.7,
                });

                if (!voice) {
                    vscode.window.showErrorMessage(`Failed to create test voice for sample ${sampleId}`);
                    return;
                }
                this._testVoiceActive = true;
                this._currentTestSynthdef = sampleId;
            }

            // Track this note as active
            this._activeTestNotes.add(midiNote);

            // Play the sample at the specified pitch
            await runtime.noteOn(voiceName, midiNote, 100);

            // For sustained notes (held keys), don't schedule auto note-off
            if (sustained) {
                // Note will be released when key is released via releaseTestNote
                return;
            }

            // Calculate duration based on sample length if available, capped for long samples
            const sampleDuration = sample.sample_rate > 0
                ? Math.min((sample.num_frames / sample.sample_rate) * 1000, 3000) // Cap at 3 seconds
                : 1000;
            const noteDuration = isOneShot ? sampleDuration : 1000;

            // Schedule note-off and cleanup (capture midiNote in closure)
            const noteToRelease = midiNote;
            this._testNoteOffTimeout = setTimeout(async () => {
                this._testNoteOffTimeout = null;
                try {
                    await runtime.noteOff(voiceName, noteToRelease);
                    this._activeTestNotes.delete(noteToRelease);
                } catch {
                    // Ignore
                }
                // Only cleanup if no other notes active
                this._testCleanupTimeout = setTimeout(async () => {
                    this._testCleanupTimeout = null;
                    if (this._activeTestNotes.size === 0) {
                        await this._cleanupTestVoice();
                    }
                }, 300);
            }, noteDuration);

        } catch (err) {
            vscode.window.showErrorMessage(`Sample test play failed: ${err}`);
        }
    }

    private async _releaseTestNote(midiNote: number) {
        const runtime = this._store.runtime;
        if (!runtime || !this._testVoiceActive) return;

        try {
            await runtime.noteOff(SampleBrowser.TEST_VOICE_NAME, midiNote);
            this._activeTestNotes.delete(midiNote);
        } catch {
            // Ignore - voice might not exist or note might not be playing
        }
    }

    private async _viewSynthDefSource(synthdefName: string) {
        const synthdef = this._store.state?.synthdefs.find(s => s.name === synthdefName);
        const stdlibInfo = this._stdlibData?.synthdefs.find(s => s.name === synthdefName);

        // Check if we have source location info from runtime
        if (synthdef && (synthdef as unknown as { source_location?: { file?: string; line?: number } }).source_location?.file) {
            const loc = (synthdef as unknown as { source_location: { file: string; line?: number } }).source_location;
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
            vscode.window.showInformationMessage(
                `"${synthdefName}" is from the standard library.\nImport: ${importPath}`,
                'Copy Import'
            ).then(selection => {
                if (selection === 'Copy Import') {
                    vscode.env.clipboard.writeText(`import "${importPath}";`);
                }
            });
            return;
        }

        // For builtin synthdefs, they're compiled into the runtime
        if (synthdef?.source === 'builtin') {
            vscode.window.showInformationMessage(
                `"${synthdefName}" is a built-in synthdef compiled into the VibeLang runtime.`
            );
            return;
        }

        // For unknown synthdefs
        if (!synthdef && !stdlibInfo) {
            vscode.window.showErrorMessage(`SynthDef "${synthdefName}" not found`);
            return;
        }

        // For user synthdefs without source location
        vscode.window.showInformationMessage(
            `Source location for "${synthdefName}" is not available. Define the synthdef using synthdef() in your code.`
        );
    }

    private _findStdlibFile(sourcePath: string): string | null {
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

    // ========== Chopping Mode Methods ==========

    private _choppingPlaybackVoice: string | null = null;

    private async _enterChoppingMode(sampleId: string) {
        const sample = this._store.state?.samples.find(s => s.id === sampleId);
        if (!sample) {
            vscode.window.showErrorMessage(`Sample not found: ${sampleId}`);
            return;
        }

        try {
            // Get waveform data from the audio file
            const waveformData = await getWaveformData(sample.path, { targetPoints: 1000 });

            // Find the samples file path for this sample
            const samplesFile = await this._findSamplesFileForSample(sampleId);

            // Send waveform data to webview
            this._panel.webview.postMessage({
                type: 'waveformData',
                sampleId: sampleId,
                samplePath: sample.path,
                waveform: waveformData,
                samplesFilePath: samplesFile || null,
            });
        } catch (err) {
            vscode.window.showErrorMessage(`Failed to load waveform: ${err}`);
        }
    }

    private async _playSampleFull(sampleId: string) {
        const runtime = this._store.runtime;
        if (!runtime) {
            return;
        }

        try {
            // Stop any existing playback
            await this._stopSamplePlayback();

            const sample = this._store.state?.samples.find(s => s.id === sampleId);
            if (!sample) return;

            // Create a temporary voice for playback
            const voiceName = '_vibelang_chopping_preview';
            this._choppingPlaybackVoice = voiceName;

            await runtime.createVoice({
                name: voiceName,
                synth_name: sample.synthdef_name,
                group_path: 'main',
                polyphony: 1,
                gain: 0.7,
            });

            // Play the sample
            await runtime.noteOn(voiceName, 60, 100);

            // Notify webview that playback started
            this._panel.webview.postMessage({
                type: 'playbackStarted',
                sampleId: sampleId,
            });
        } catch (err) {
            console.error('Failed to play sample:', err);
        }
    }

    private async _stopSamplePlayback() {
        const runtime = this._store.runtime;
        if (!runtime || !this._choppingPlaybackVoice) {
            return;
        }

        try {
            await runtime.noteOff(this._choppingPlaybackVoice, 60);
            await runtime.deleteVoice(this._choppingPlaybackVoice);
            this._choppingPlaybackVoice = null;

            this._panel.webview.postMessage({
                type: 'playbackStopped',
            });
        } catch (err) {
            console.error('Failed to stop playback:', err);
        }
    }

    private async _previewSlice(sampleId: string, startSeconds: number, endSeconds: number) {
        const runtime = this._store.runtime;
        if (!runtime) {
            return;
        }

        try {
            const sample = this._store.state?.samples.find(s => s.id === sampleId);
            if (!sample) return;

            // Use evalCode to play the slice
            const code = `
                let _preview_sample = load_sample("_preview", "${sample.path}");
                let _preview_slice = _preview_sample.slice(${startSeconds}, ${endSeconds});
                let _preview_voice = voice("_slice_preview").on_sample(_preview_slice).group("main");
                _preview_voice.trigger();
            `;
            await runtime.evalCode(code);
        } catch (err) {
            console.error('Failed to preview slice:', err);
        }
    }

    private async _findSamplesFileForSample(sampleId: string): Promise<string | null> {
        // Try to find a *_samples.vibe file that contains this sample
        const workspaceFolders = vscode.workspace.workspaceFolders;
        if (!workspaceFolders) return null;

        for (const folder of workspaceFolders) {
            const pattern = new vscode.RelativePattern(folder, '**/*_samples.vibe');
            const files = await vscode.workspace.findFiles(pattern, '**/node_modules/**', 50);

            for (const file of files) {
                const content = fs.readFileSync(file.fsPath, 'utf-8');
                // Check if this file contains a load_sample for our sampleId
                if (content.includes(`load_sample("${sampleId}"`) ||
                    content.includes(`sample("${sampleId}"`)) {
                    return file.fsPath;
                }
            }
        }

        return null;
    }

    private async _saveSlicesToFile(sampleId: string, sliceCode: string) {
        // Find the samples file for this sample
        let samplesFile = await this._findSamplesFileForSample(sampleId);

        if (!samplesFile) {
            // Try to find any open .vibe file and create a samples file for it
            const editor = vscode.window.activeTextEditor;
            if (editor && editor.document.fileName.endsWith('.vibe')) {
                const mainFilePath = editor.document.fileName;
                const mainFileDir = path.dirname(mainFilePath);
                const mainFileName = path.basename(mainFilePath, '.vibe');

                // Don't create a samples file from a samples file
                if (mainFileName.endsWith('_samples')) {
                    samplesFile = mainFilePath;
                } else {
                    samplesFile = path.join(mainFileDir, `${mainFileName}_samples.vibe`);

                    // Create the file if it doesn't exist
                    if (!fs.existsSync(samplesFile)) {
                        const header = `// Sample definitions for ${mainFileName}.vibe\n// Auto-generated by VibeLang Sample Browser\n\n`;
                        fs.writeFileSync(samplesFile, header);

                        // Add import to main file
                        const samplesFileName = path.basename(samplesFile);
                        const importStatement = `import "./${samplesFileName}";`;
                        const mainContent = editor.document.getText();

                        if (!mainContent.includes(importStatement)) {
                            await editor.edit(editBuilder => {
                                editBuilder.insert(new vscode.Position(0, 0), importStatement + '\n\n');
                            });
                        }
                    }
                }
            } else {
                vscode.window.showErrorMessage('Could not find a samples file. Please open a .vibe file first.');
                return;
            }
        }

        try {
            // Read existing content
            const existingContent = fs.readFileSync(samplesFile, 'utf-8');

            // Append slice code with separator
            const separator = '\n\n// ========== Slices from Sample Chopping ==========\n';
            const newContent = existingContent.trimEnd() + separator + sliceCode + '\n';

            // Write back
            fs.writeFileSync(samplesFile, newContent);

            // Notify user
            vscode.window.showInformationMessage(`Slices saved to ${path.basename(samplesFile)}`);

            // Open file
            const doc = await vscode.workspace.openTextDocument(samplesFile);
            await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
        } catch (err) {
            vscode.window.showErrorMessage(`Failed to save slices: ${err}`);
        }
    }

    private _getHtmlContent(): string {
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
            /* Background colors - using VSCode theme variables */
            --bg-primary: var(--vscode-editor-background);
            --bg-secondary: var(--vscode-sideBar-background, var(--vscode-editor-background));
            --bg-tertiary: var(--vscode-editorWidget-background, var(--vscode-sideBar-background));

            /* Text colors - using VSCode theme variables */
            --text-primary: var(--vscode-editor-foreground);
            --text-secondary: var(--vscode-descriptionForeground, var(--vscode-foreground));
            --text-muted: var(--vscode-disabledForeground, var(--vscode-descriptionForeground));

            /* Accent colors - using VSCode theme variables where possible */
            --accent-green: var(--vscode-charts-green, var(--vscode-terminal-ansiGreen, #9bbb59));
            --accent-orange: var(--vscode-charts-orange, var(--vscode-terminal-ansiYellow, #d19a66));
            --accent-blue: var(--vscode-textLink-foreground, var(--vscode-charts-blue, #569cd6));
            --accent-purple: var(--vscode-charts-purple, var(--vscode-terminal-ansiMagenta, #c586c0));
            --accent-red: var(--vscode-errorForeground, var(--vscode-charts-red, #d16969));

            /* UI colors */
            --border: var(--vscode-panel-border, var(--vscode-widget-border, var(--vscode-editorWidget-border)));
        }

        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }

        body {
            font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif);
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: var(--vscode-font-size, 12px);
            height: 100vh;
            display: flex;
            flex-direction: column;
        }

        /* Toolbar */
        .toolbar {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 8px 0;
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
            border: 1px solid var(--vscode-button-border, transparent);
            border-radius: 4px;
            background: var(--vscode-button-secondaryBackground, var(--bg-tertiary));
            color: var(--vscode-button-secondaryForeground, var(--text-primary));
            cursor: pointer;
            font-size: 11px;
            font-family: inherit;
            transition: all 0.1s ease;
        }

        .btn:hover {
            background: var(--vscode-button-secondaryHoverBackground, var(--bg-tertiary));
        }

        .btn-primary {
            background: var(--vscode-button-background, var(--accent-blue));
            border-color: var(--vscode-button-background, var(--accent-blue));
            color: var(--vscode-button-foreground, white);
        }

        .btn-primary:hover {
            background: var(--vscode-button-hoverBackground, var(--accent-blue));
        }

        .chop-sample-btn {
            width: 100%;
            padding: 12px 16px;
            margin-bottom: 16px;
            border: 2px solid var(--accent-orange);
            border-radius: 6px;
            background: linear-gradient(135deg, rgba(218, 150, 75, 0.15) 0%, rgba(218, 150, 75, 0.05) 100%);
            color: var(--accent-orange);
            cursor: pointer;
            font-size: 13px;
            font-weight: 600;
            transition: all 0.2s ease;
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 8px;
        }

        .chop-sample-btn:hover {
            background: linear-gradient(135deg, rgba(218, 150, 75, 0.25) 0%, rgba(218, 150, 75, 0.15) 100%);
            border-color: #e8a84b;
            transform: translateY(-1px);
            box-shadow: 0 4px 12px rgba(218, 150, 75, 0.2);
        }

        .chop-sample-btn:active {
            transform: translateY(0);
            box-shadow: none;
        }

        .octave-indicator {
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 11px;
            padding: 4px 8px;
            background: var(--bg-tertiary);
            border-radius: 4px;
            color: var(--accent-orange);
            user-select: none;
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

        /* Search Container with Inline Tags */
        .search-container {
            flex: 1;
            display: flex;
            flex-wrap: wrap;
            align-items: center;
            gap: 4px;
            padding: 4px 8px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: var(--bg-primary);
            min-height: 32px;
            position: relative;
        }

        .search-container:focus-within {
            border-color: var(--accent-blue);
        }

        .search-tag-chip {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            padding: 2px 6px;
            border-radius: 3px;
            font-size: 10px;
            font-weight: 500;
            background: rgba(86, 156, 214, 0.2);
            color: var(--accent-blue);
            cursor: pointer;
            transition: all 0.1s ease;
        }

        .search-tag-chip.user-tag {
            background: rgba(209, 154, 102, 0.2);
            color: var(--accent-orange);
        }

        .search-tag-chip.favorite {
            background: rgba(255, 215, 0, 0.15);
            color: #ffd700;
        }

        .search-tag-chip:hover {
            background: rgba(209, 105, 105, 0.3);
            color: var(--accent-red);
        }

        .search-tag-chip .remove {
            font-size: 12px;
            line-height: 1;
        }

        .search-input {
            flex: 1;
            min-width: 100px;
            border: none;
            background: transparent;
            color: var(--text-primary);
            font-size: 11px;
            outline: none;
        }

        .search-suggestions {
            position: absolute;
            top: 100%;
            left: 0;
            right: 0;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 4px;
            margin-top: 4px;
            max-height: 250px;
            overflow-y: auto;
            z-index: 100;
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
            display: none;
        }

        .search-suggestions.visible {
            display: block;
        }

        .suggestion-header {
            padding: 6px 10px;
            font-size: 9px;
            text-transform: uppercase;
            color: var(--text-muted);
            background: var(--bg-tertiary);
            font-weight: 600;
        }

        .suggestion {
            padding: 8px 10px;
            cursor: pointer;
            font-size: 11px;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        .suggestion:hover {
            background: var(--bg-tertiary);
        }

        .suggestion .count {
            color: var(--text-muted);
            font-size: 10px;
        }

        .suggestion.user-tag {
            color: var(--accent-orange);
        }

        .suggestion.favorite {
            color: #ffd700;
        }

        /* Item Tags Display */
        .item-tags {
            display: flex;
            flex-wrap: wrap;
            gap: 4px;
            margin-top: 4px;
        }

        .item-tag {
            font-size: 9px;
            padding: 1px 5px;
            border-radius: 2px;
            background: rgba(86, 156, 214, 0.15);
            color: var(--accent-blue);
        }

        .item-tag.user-tag {
            background: rgba(209, 154, 102, 0.2);
            color: var(--accent-orange);
        }

        .item-tag.favorite {
            background: rgba(255, 215, 0, 0.15);
            color: #ffd700;
        }

        /* Tag Editor in Detail Panel */
        .tag-editor {
            margin-top: 16px;
            padding: 12px;
            background: var(--bg-tertiary);
            border-radius: 6px;
        }

        .tag-editor-title {
            font-size: 11px;
            font-weight: 600;
            color: var(--text-secondary);
            margin-bottom: 8px;
        }

        .current-tags {
            display: flex;
            flex-wrap: wrap;
            gap: 6px;
            margin-bottom: 10px;
            min-height: 24px;
        }

        .current-tags .tag-chip {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            padding: 3px 8px;
            border-radius: 4px;
            font-size: 10px;
            font-weight: 500;
        }

        .current-tags .tag-chip.auto {
            background: rgba(86, 156, 214, 0.15);
            color: var(--accent-blue);
        }

        .current-tags .tag-chip.user-tag {
            background: rgba(209, 154, 102, 0.2);
            color: var(--accent-orange);
            cursor: pointer;
        }

        .current-tags .tag-chip.favorite {
            background: rgba(255, 215, 0, 0.15);
            color: #ffd700;
            cursor: pointer;
        }

        .current-tags .tag-chip.user-tag:hover,
        .current-tags .tag-chip.favorite:hover {
            background: rgba(209, 105, 105, 0.3);
            color: var(--accent-red);
        }

        .current-tags .tag-chip .remove {
            font-size: 11px;
            line-height: 1;
        }

        .tag-input-container {
            position: relative;
        }

        .tag-input {
            width: 100%;
            padding: 6px 10px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: 11px;
        }

        .tag-input:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        .tag-input-suggestions {
            position: absolute;
            top: 100%;
            left: 0;
            right: 0;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 4px;
            margin-top: 4px;
            max-height: 150px;
            overflow-y: auto;
            z-index: 100;
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
            display: none;
        }

        .tag-input-suggestions.visible {
            display: block;
        }

        .tag-suggestion {
            padding: 6px 10px;
            cursor: pointer;
            font-size: 11px;
        }

        .tag-suggestion:hover {
            background: var(--bg-tertiary);
        }

        .tag-suggestion.create-new {
            color: var(--accent-green);
            font-style: italic;
        }

        /* Quick Tag Buttons */
        .quick-tags {
            display: flex;
            gap: 6px;
            margin-top: 8px;
        }

        .quick-tag-btn {
            padding: 4px 10px;
            border: 1px solid var(--border);
            border-radius: 4px;
            background: transparent;
            color: var(--text-secondary);
            font-size: 10px;
            cursor: pointer;
            transition: all 0.1s ease;
        }

        .quick-tag-btn:hover {
            background: var(--bg-tertiary);
            border-color: var(--text-secondary);
        }

        .quick-tag-btn.active {
            background: rgba(255, 215, 0, 0.15);
            border-color: #ffd700;
            color: #ffd700;
        }

        /* Context Menu */
        .context-menu {
            position: fixed;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 6px;
            box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
            z-index: 1000;
            min-width: 160px;
            padding: 4px 0;
            display: none;
        }

        .context-menu.visible {
            display: block;
        }

        .context-menu-item {
            padding: 8px 12px;
            cursor: pointer;
            font-size: 11px;
            display: flex;
            align-items: center;
            gap: 8px;
        }

        .context-menu-item:hover {
            background: var(--bg-tertiary);
        }

        .context-menu-item.favorite {
            color: #ffd700;
        }

        .context-menu-separator {
            height: 1px;
            background: var(--border);
            margin: 4px 0;
        }

        .context-submenu {
            position: relative;
        }

        .context-submenu-items {
            position: absolute;
            left: 100%;
            top: 0;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 6px;
            box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
            min-width: 140px;
            padding: 4px 0;
            display: none;
        }

        .context-submenu:hover .context-submenu-items {
            display: block;
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

        /* ========== Chopping Mode Styles ========== */
        .chopping-panel {
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: linear-gradient(180deg, #1a1a1a 0%, #141414 100%);
            z-index: 100;
            display: none;
            flex-direction: column;
            animation: fadeIn 0.2s ease-out;
        }

        @keyframes fadeIn {
            from { opacity: 0; }
            to { opacity: 1; }
        }

        .chopping-panel.visible {
            display: flex;
        }

        .chopping-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 16px 20px;
            background: linear-gradient(180deg, rgba(40, 40, 40, 0.95) 0%, rgba(30, 30, 30, 0.95) 100%);
            border-bottom: 1px solid rgba(255, 255, 255, 0.08);
            backdrop-filter: blur(10px);
        }

        .chopping-header-left {
            display: flex;
            align-items: center;
            gap: 12px;
        }

        .chopping-icon {
            font-size: 20px;
            filter: grayscale(0);
        }

        .chopping-title-group {
            display: flex;
            flex-direction: column;
            gap: 2px;
        }

        .chopping-title {
            font-size: 15px;
            font-weight: 600;
            color: #fff;
            letter-spacing: -0.3px;
        }

        .chopping-subtitle {
            font-size: 11px;
            color: var(--text-secondary);
            font-family: 'SF Mono', Monaco, monospace;
        }

        .chopping-header .btn-done {
            background: linear-gradient(180deg, #4a4a4a 0%, #3a3a3a 100%);
            border: 1px solid rgba(255, 255, 255, 0.1);
            color: #fff;
            font-weight: 500;
            padding: 8px 20px;
            border-radius: 6px;
            transition: all 0.15s ease;
        }

        .chopping-header .btn-done:hover {
            background: linear-gradient(180deg, #5a5a5a 0%, #4a4a4a 100%);
            border-color: rgba(255, 255, 255, 0.2);
        }

        /* Waveform Container */
        .waveform-section {
            padding: 20px;
            background: rgba(0, 0, 0, 0.2);
        }

        .waveform-container {
            position: relative;
            height: 160px;
            background: linear-gradient(180deg, #1e1e1e 0%, #181818 100%);
            border-radius: 10px;
            overflow: hidden;
            box-shadow:
                inset 0 1px 0 rgba(255, 255, 255, 0.03),
                0 4px 20px rgba(0, 0, 0, 0.4);
            border: 1px solid rgba(255, 255, 255, 0.05);
        }

        #waveformCanvas {
            width: 100%;
            height: 100%;
            cursor: crosshair;
        }

        .playhead {
            position: absolute;
            top: 0;
            bottom: 0;
            width: 2px;
            background: linear-gradient(180deg, #ff4444 0%, #ff6666 100%);
            pointer-events: none;
            z-index: 10;
            display: none;
            box-shadow: 0 0 8px rgba(255, 68, 68, 0.6);
        }

        .playhead.active {
            display: block;
        }

        .playhead::before {
            content: '';
            position: absolute;
            top: 0;
            left: -4px;
            width: 10px;
            height: 10px;
            background: #ff4444;
            border-radius: 50%;
            box-shadow: 0 0 6px rgba(255, 68, 68, 0.8);
        }

        /* Transport Controls */
        .transport-section {
            display: flex;
            align-items: center;
            gap: 12px;
            padding: 0 20px 16px;
        }

        .transport-btn {
            width: 44px;
            height: 44px;
            border-radius: 50%;
            border: none;
            cursor: pointer;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 18px;
            transition: all 0.15s ease;
        }

        .transport-btn.play {
            background: linear-gradient(180deg, #5cb85c 0%, #449d44 100%);
            box-shadow: 0 3px 12px rgba(92, 184, 92, 0.3);
        }

        .transport-btn.play:hover {
            background: linear-gradient(180deg, #6ec86e 0%, #55ae55 100%);
            transform: scale(1.05);
        }

        .transport-btn.play:disabled {
            opacity: 0.5;
            cursor: not-allowed;
            transform: none;
        }

        .transport-btn.stop {
            background: linear-gradient(180deg, #d9534f 0%, #c9302c 100%);
            box-shadow: 0 3px 12px rgba(217, 83, 79, 0.3);
        }

        .transport-btn.stop:hover {
            background: linear-gradient(180deg, #e96460 0%, #d9413d 100%);
            transform: scale(1.05);
        }

        .position-display {
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 14px;
            color: #888;
            background: rgba(0, 0, 0, 0.3);
            padding: 8px 14px;
            border-radius: 6px;
            border: 1px solid rgba(255, 255, 255, 0.05);
            margin-left: auto;
            min-width: 160px;
            text-align: center;
        }

        .position-display .current {
            color: #fff;
        }

        /* Capture Hint */
        .capture-hint-section {
            padding: 0 20px 12px;
        }

        .capture-hint {
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 10px;
            padding: 14px 20px;
            font-size: 12px;
            color: #888;
            background: linear-gradient(180deg, rgba(255, 255, 255, 0.03) 0%, rgba(255, 255, 255, 0.01) 100%);
            border-radius: 8px;
            border: 1px dashed rgba(255, 255, 255, 0.1);
            transition: all 0.2s ease;
        }

        .capture-hint .kbd {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            min-width: 24px;
            height: 24px;
            padding: 0 6px;
            background: linear-gradient(180deg, #3a3a3a 0%, #2a2a2a 100%);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 4px;
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 11px;
            color: #ccc;
            box-shadow: 0 2px 0 #1a1a1a;
        }

        .capture-hint.capturing {
            background: linear-gradient(180deg, rgba(218, 150, 75, 0.2) 0%, rgba(218, 150, 75, 0.1) 100%);
            border-color: var(--accent-orange);
            border-style: solid;
            color: var(--accent-orange);
            animation: pulse-capture 0.8s ease-in-out infinite;
        }

        .capture-hint.capturing .kbd {
            background: var(--accent-orange);
            color: #000;
            border-color: var(--accent-orange);
        }

        @keyframes pulse-capture {
            0%, 100% { box-shadow: 0 0 0 0 rgba(218, 150, 75, 0.4); }
            50% { box-shadow: 0 0 0 8px rgba(218, 150, 75, 0); }
        }

        /* Main Content Split */
        .chopping-content {
            display: flex;
            flex: 1;
            overflow: hidden;
        }

        /* Slices List */
        .slices-container {
            flex: 1;
            overflow-y: auto;
            padding: 16px 20px;
            background: rgba(0, 0, 0, 0.15);
        }

        .slices-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: 14px;
        }

        .slices-title {
            font-size: 11px;
            font-weight: 600;
            color: #666;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        .slices-count {
            font-size: 11px;
            color: #555;
            background: rgba(255, 255, 255, 0.05);
            padding: 3px 8px;
            border-radius: 10px;
        }

        .slices-empty {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            padding: 40px 20px;
            color: #555;
            text-align: center;
        }

        .slices-empty-icon {
            font-size: 32px;
            margin-bottom: 12px;
            opacity: 0.5;
        }

        .slices-empty-text {
            font-size: 13px;
            margin-bottom: 4px;
        }

        .slices-empty-hint {
            font-size: 11px;
            color: #444;
        }

        .slice-item {
            background: linear-gradient(180deg, rgba(255, 255, 255, 0.04) 0%, rgba(255, 255, 255, 0.02) 100%);
            border-radius: 8px;
            padding: 12px 14px;
            margin-bottom: 10px;
            border-left: 4px solid;
            transition: all 0.15s ease;
            border-top: 1px solid rgba(255, 255, 255, 0.03);
        }

        .slice-item:hover {
            background: linear-gradient(180deg, rgba(255, 255, 255, 0.06) 0%, rgba(255, 255, 255, 0.03) 100%);
        }

        .slice-header {
            display: flex;
            align-items: center;
            gap: 10px;
            margin-bottom: 10px;
        }

        .slice-badge {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 26px;
            height: 26px;
            border-radius: 6px;
            font-weight: 700;
            font-size: 12px;
            color: #000;
        }

        .slice-duration {
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 11px;
            color: #666;
            background: rgba(0, 0, 0, 0.2);
            padding: 3px 8px;
            border-radius: 4px;
        }

        .slice-actions {
            margin-left: auto;
            display: flex;
            gap: 6px;
        }

        .slice-actions .btn-icon {
            width: 28px;
            height: 28px;
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 6px;
            background: rgba(255, 255, 255, 0.03);
            color: #888;
            cursor: pointer;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 12px;
            transition: all 0.15s ease;
        }

        .slice-actions .btn-icon:hover {
            background: var(--accent-blue);
            color: white;
            border-color: var(--accent-blue);
        }

        .slice-actions .btn-icon.delete:hover {
            background: var(--accent-red);
            border-color: var(--accent-red);
        }

        .slice-controls {
            display: grid;
            grid-template-columns: repeat(2, 1fr);
            gap: 10px;
        }

        .slice-control {
            display: flex;
            flex-direction: column;
            gap: 4px;
        }

        .slice-control-label {
            font-size: 10px;
            color: #555;
            text-transform: uppercase;
            letter-spacing: 0.3px;
        }

        .slice-control-input {
            width: 100%;
            padding: 6px 8px;
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 5px;
            background: rgba(0, 0, 0, 0.3);
            color: #ddd;
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 11px;
            transition: all 0.15s ease;
        }

        .slice-control-input:focus {
            outline: none;
            border-color: var(--accent-blue);
            background: rgba(0, 0, 0, 0.4);
        }

        .slice-control-input::-webkit-inner-spin-button {
            opacity: 0.5;
        }

        .slice-params-row {
            grid-column: 1 / -1;
            display: flex;
            gap: 10px;
            margin-top: 4px;
            padding-top: 10px;
            border-top: 1px solid rgba(255, 255, 255, 0.05);
        }

        .slice-param {
            flex: 1;
        }

        .slice-param select {
            width: 100%;
            padding: 5px 8px;
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 5px;
            background: rgba(0, 0, 0, 0.3);
            color: #ddd;
            font-size: 10px;
            cursor: pointer;
        }

        .slice-param select:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        /* Right Panel - Defaults & Output */
        .chopping-sidebar {
            width: 320px;
            display: flex;
            flex-direction: column;
            border-left: 1px solid rgba(255, 255, 255, 0.05);
            background: rgba(0, 0, 0, 0.1);
        }

        /* Defaults Section */
        .defaults-section {
            padding: 16px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.05);
        }

        .section-header {
            display: flex;
            align-items: center;
            gap: 8px;
            margin-bottom: 14px;
        }

        .section-icon {
            font-size: 14px;
            opacity: 0.6;
        }

        .section-title {
            font-size: 11px;
            font-weight: 600;
            color: #666;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        .defaults-grid {
            display: grid;
            grid-template-columns: repeat(2, 1fr);
            gap: 12px;
        }

        .default-field {
            display: flex;
            flex-direction: column;
            gap: 5px;
        }

        .default-field.full-width {
            grid-column: 1 / -1;
        }

        .default-field label {
            font-size: 10px;
            color: #555;
            text-transform: uppercase;
            letter-spacing: 0.3px;
        }

        .default-input {
            padding: 8px 10px;
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 5px;
            background: rgba(0, 0, 0, 0.25);
            color: #ddd;
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 11px;
        }

        .default-input:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        .default-select {
            padding: 8px 10px;
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 5px;
            background: rgba(0, 0, 0, 0.25);
            color: #ddd;
            font-size: 11px;
            cursor: pointer;
        }

        /* Code Output Section */
        .code-output-section {
            flex: 1;
            display: flex;
            flex-direction: column;
            padding: 16px;
            min-height: 0;
        }

        .code-output-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: 12px;
        }

        .target-file {
            font-size: 10px;
            color: #555;
            font-family: 'SF Mono', Monaco, monospace;
            background: rgba(0, 0, 0, 0.2);
            padding: 3px 8px;
            border-radius: 4px;
        }

        .code-output-block {
            flex: 1;
            background: #0d0d0d;
            border-radius: 8px;
            border: 1px solid rgba(255, 255, 255, 0.05);
            overflow: hidden;
            display: flex;
            flex-direction: column;
            min-height: 120px;
        }

        .code-output-content {
            flex: 1;
            padding: 12px 14px;
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 11px;
            line-height: 1.5;
            overflow-y: auto;
            color: #888;
        }

        .code-output-content pre {
            margin: 0;
            white-space: pre-wrap;
        }

        .code-output-content.has-code {
            color: #9bbb59;
        }

        .save-section {
            padding: 16px;
            border-top: 1px solid rgba(255, 255, 255, 0.05);
        }

        .btn-save-slices {
            width: 100%;
            padding: 12px 20px;
            background: linear-gradient(180deg, #5cb85c 0%, #449d44 100%);
            border: none;
            border-radius: 8px;
            color: white;
            font-weight: 600;
            font-size: 13px;
            cursor: pointer;
            transition: all 0.15s ease;
            box-shadow: 0 3px 12px rgba(92, 184, 92, 0.25);
        }

        .btn-save-slices:hover:not(:disabled) {
            background: linear-gradient(180deg, #6ec86e 0%, #55ae55 100%);
            transform: translateY(-1px);
            box-shadow: 0 5px 16px rgba(92, 184, 92, 0.35);
        }

        .btn-save-slices:disabled {
            opacity: 0.4;
            cursor: not-allowed;
            transform: none;
            box-shadow: none;
        }

        /* Slice Colors */
        .slice-1 { border-left-color: #ff6b6b; }
        .slice-2 { border-left-color: #4ecdc4; }
        .slice-3 { border-left-color: #ffe66d; }
        .slice-4 { border-left-color: #9575cd; }
        .slice-5 { border-left-color: #ffb74d; }
        .slice-6 { border-left-color: #64b5f6; }
        .slice-7 { border-left-color: #81c784; }
        .slice-8 { border-left-color: #f06292; }
        .slice-9 { border-left-color: #aed581; }

        .slice-badge-1 { background: #ff6b6b; }
        .slice-badge-2 { background: #4ecdc4; }
        .slice-badge-3 { background: #ffe66d; }
        .slice-badge-4 { background: #9575cd; }
        .slice-badge-5 { background: #ffb74d; }
        .slice-badge-6 { background: #64b5f6; }
        .slice-badge-7 { background: #81c784; }
        .slice-badge-8 { background: #f06292; }
        .slice-badge-9 { background: #aed581; }
    </style>
</head>
<body>
    <div class="toolbar">
        <span class="toolbar-title">Browser</span>
        <div class="search-container" id="searchContainer">
            <div id="activeTagChips"></div>
            <input type="text" class="search-input" id="searchBox" placeholder="Search samples, presets, or tags...">
            <div class="search-suggestions" id="searchSuggestions"></div>
        </div>
        <span class="octave-indicator" id="octaveIndicator" title="Current octave (use numpad +/- to change)">Oct: 4</span>
        <button class="btn btn-primary" id="loadBtn">+ Load</button>
    </div>

    <!-- Context Menu -->
    <div class="context-menu" id="contextMenu">
        <div class="context-menu-item favorite" data-action="toggleFavorite">
            <span>★</span> Toggle Favorite
        </div>
        <div class="context-menu-separator"></div>
        <div class="context-menu-item" data-action="addTag">
            <span>🏷️</span> Add Tag...
        </div>
    </div>

    <div class="tabs">
        <div class="tab active" data-tab="samples">Samples</div>
        <div class="tab" data-tab="synthdefs">Instruments</div>
        <div class="tab" data-tab="effects">Effects</div>
    </div>

    <div class="content">
        <div class="tab-content active" id="samplesTab">
            <div class="item-list" id="samplesList"></div>
        </div>
        <div class="tab-content" id="synthdefsTab">
            <div class="item-list" id="synthdefsList"></div>
        </div>
        <div class="tab-content" id="effectsTab">
            <div class="item-list" id="effectsList"></div>
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

    <!-- Chopping Mode Panel (Full Screen Overlay) -->
    <div class="chopping-panel" id="choppingPanel">
        <div class="chopping-header">
            <div class="chopping-header-left">
                <span class="chopping-icon">✂️</span>
                <div class="chopping-title-group">
                    <span class="chopping-title">Sample Chopper</span>
                    <span class="chopping-subtitle" id="choppingSubtitle">sample_name</span>
                </div>
            </div>
            <button class="btn btn-done" id="exitChoppingBtn">Done</button>
        </div>

        <div class="waveform-section">
            <div class="waveform-container">
                <canvas id="waveformCanvas"></canvas>
                <div class="playhead" id="playhead"></div>
            </div>
        </div>

        <div class="transport-section">
            <button class="transport-btn play" id="playBtn" title="Play sample">▶</button>
            <button class="transport-btn stop" id="stopBtn" title="Stop playback">⏹</button>
            <span class="position-display" id="positionDisplay">
                <span class="current">0:00.000</span> / 0:00.000
            </span>
        </div>

        <div class="capture-hint-section">
            <div class="capture-hint" id="captureHint">
                Hold <span class="kbd">1</span>-<span class="kbd">9</span> on numpad to capture slice (press = start, release = end)
            </div>
        </div>

        <div class="chopping-content">
            <div class="slices-container">
                <div class="slices-header">
                    <span class="slices-title">Captured Slices</span>
                    <span class="slices-count" id="slicesCount">0 / 9</span>
                </div>
                <div id="slicesList">
                    <div class="slices-empty">
                        <div class="slices-empty-icon">🎹</div>
                        <div class="slices-empty-text">No slices captured yet</div>
                        <div class="slices-empty-hint">Play the sample and hold numpad keys to capture</div>
                    </div>
                </div>
            </div>

            <div class="chopping-sidebar">
                <div class="defaults-section">
                    <div class="section-header">
                        <span class="section-icon">⚙️</span>
                        <span class="section-title">Defaults for New Slices</span>
                    </div>
                    <div class="defaults-grid">
                        <div class="default-field">
                            <label>Attack (s)</label>
                            <input type="number" class="default-input" id="defaultAttack" value="0.001" step="0.001" min="0">
                        </div>
                        <div class="default-field">
                            <label>Release (s)</label>
                            <input type="number" class="default-input" id="defaultRelease" value="0.01" step="0.001" min="0">
                        </div>
                        <div class="default-field full-width">
                            <label>Play Mode</label>
                            <select class="default-select" id="defaultPlayMode">
                                <option value="oneshot">One-shot</option>
                                <option value="sustained">Sustained</option>
                                <option value="loop">Loop</option>
                            </select>
                        </div>
                    </div>
                </div>

                <div class="code-output-section">
                    <div class="code-output-header">
                        <div class="section-header" style="margin-bottom: 0;">
                            <span class="section-icon">📝</span>
                            <span class="section-title">Generated Code</span>
                        </div>
                    </div>
                    <div class="target-file" id="targetFile" style="margin-bottom: 10px;"></div>
                    <div class="code-output-block">
                        <div class="code-output-content" id="sliceCodeOutput">// No slices captured yet</div>
                    </div>
                </div>

                <div class="save-section">
                    <button class="btn-save-slices" id="saveSlicesBtn" disabled>💾 Save Slices to File</button>
                </div>
            </div>
        </div>
    </div>

    <script>
        const vscode = acquireVsCodeApi();

        let state = {
            samples: [],
            synthdefs: [],
            voices: [],
            userTags: {},      // Map of "type:id" -> [tags]
            allUserTags: []    // All unique user tags for autocomplete
        };

        let searchQuery = '';
        let activeTagFilters = []; // Tags currently used as filters
        let selectedItem = null;
        let selectedItemType = null; // 'sample' or 'synthdef'
        let lastPlayedItem = null; // Track the item used for the last test voice
        let activeTab = 'samples';
        let sliceCount = 4;
        let playingTestSynthdef = null; // Track currently playing test synthdef
        let lastStateHash = ''; // Track state changes to avoid unnecessary re-renders
        let currentOctave = 4; // Base octave for numpad playback (C4 = MIDI 60)
        let contextMenuTarget = null; // {type: 'sample'|'synthdef', id: string}

        // ========== Fuzzy Search Implementation ==========
        function fuzzyMatch(text, pattern) {
            if (!pattern) return { match: true, score: 0 };
            text = text.toLowerCase();
            pattern = pattern.toLowerCase();

            // Exact match gets highest score
            if (text === pattern) return { match: true, score: 1.0 };
            if (text.includes(pattern)) return { match: true, score: 0.8 };

            // Fuzzy matching - check if all characters appear in order
            let patternIdx = 0;
            let score = 0;
            let consecutiveMatches = 0;
            let lastMatchIdx = -1;

            for (let i = 0; i < text.length && patternIdx < pattern.length; i++) {
                if (text[i] === pattern[patternIdx]) {
                    // Bonus for consecutive matches
                    if (lastMatchIdx === i - 1) {
                        consecutiveMatches++;
                        score += 0.1 + (consecutiveMatches * 0.05);
                    } else {
                        consecutiveMatches = 0;
                        score += 0.05;
                    }
                    // Bonus for match at word boundary
                    if (i === 0 || text[i-1] === ' ' || text[i-1] === '_' || text[i-1] === '-') {
                        score += 0.1;
                    }
                    lastMatchIdx = i;
                    patternIdx++;
                }
            }

            if (patternIdx === pattern.length) {
                // All pattern chars matched
                return { match: true, score: Math.min(score / pattern.length, 0.7) };
            }
            return { match: false, score: 0 };
        }

        // Search across multiple fields with weights
        function searchItem(item, query, fields) {
            if (!query) return { match: true, score: 1.0 };

            let bestScore = 0;
            for (const field of fields) {
                const value = field.getValue(item);
                if (!value) continue;

                // Handle arrays (like tags)
                if (Array.isArray(value)) {
                    for (const v of value) {
                        const result = fuzzyMatch(v, query);
                        if (result.match) {
                            bestScore = Math.max(bestScore, result.score * field.weight);
                        }
                    }
                } else {
                    const result = fuzzyMatch(value, query);
                    if (result.match) {
                        bestScore = Math.max(bestScore, result.score * field.weight);
                    }
                }
            }

            return { match: bestScore > 0, score: bestScore };
        }

        // ========== Auto-Tag Extraction ==========
        function extractAutoTags(item, type) {
            const tags = [];

            if (type === 'synthdef') {
                const stdlib = item._stdlib;
                if (stdlib) {
                    // Category and subcategory
                    if (stdlib.category) tags.push(stdlib.category);
                    if (stdlib.subcategory) tags.push(stdlib.subcategory);

                    // Type
                    if (stdlib.type) tags.push(stdlib.type);

                    // Parse description: "Genre: Acid House, Techno | Character: Classic 303"
                    if (stdlib.description) {
                        const parts = stdlib.description.split('|');
                        for (const part of parts) {
                            const colonIdx = part.indexOf(':');
                            if (colonIdx !== -1) {
                                const values = part.substring(colonIdx + 1).trim();
                                const extracted = values.split(',').map(v => v.trim().toLowerCase());
                                tags.push(...extracted);
                            }
                        }
                    }
                }

                // Parse name: "acid_303_classic" -> ["acid", "303", "classic"]
                const nameParts = item.name.split('_').filter(p => p.length > 1 && !/^\\d+$/.test(p));
                tags.push(...nameParts.map(p => p.toLowerCase()));
            } else if (type === 'sample') {
                // Extract from sample id/path
                const nameParts = item.id.split('_').filter(p => p.length > 1 && !/^\\d+$/.test(p));
                tags.push(...nameParts.map(p => p.toLowerCase()));

                // Check for common sample types in name
                const typeKeywords = ['kick', 'snare', 'hihat', 'hat', 'clap', 'tom', 'perc', 'bass', 'lead', 'pad', 'fx', 'loop', 'vocal'];
                for (const kw of typeKeywords) {
                    if (item.id.toLowerCase().includes(kw) || item.path.toLowerCase().includes(kw)) {
                        tags.push(kw);
                    }
                }
            }

            // Deduplicate and return
            return [...new Set(tags)].filter(t => t.length > 0);
        }

        // Get all tags (auto + user) for an item
        function getAllTags(item, type) {
            const autoTags = extractAutoTags(item, type);
            const id = type === 'sample' ? item.id : item.name;
            const userTags = state.userTags[type + ':' + id] || [];
            return {
                auto: autoTags,
                user: userTags,
                all: [...new Set([...autoTags, ...userTags])]
            };
        }

        // Build tag counts for suggestions
        function buildTagCounts() {
            const counts = {};

            // Count from synthdefs
            for (const s of state.synthdefs) {
                const tags = getAllTags(s, 'synthdef');
                for (const t of tags.all) {
                    counts[t] = (counts[t] || 0) + 1;
                }
            }

            // Count from samples
            for (const s of state.samples) {
                const tags = getAllTags(s, 'sample');
                for (const t of tags.all) {
                    counts[t] = (counts[t] || 0) + 1;
                }
            }

            return counts;
        }

        // Check if item matches all active tag filters
        function matchesTagFilters(item, type) {
            if (activeTagFilters.length === 0) return true;
            const tags = getAllTags(item, type);
            return activeTagFilters.every(filter => tags.all.includes(filter));
        }

        // Major scale intervals in semitones: C, D, E, F, G, A, B, C+, D+, E+
        const majorScaleIntervals = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16];

        // ========== Chopping Mode State ==========
        const SLICE_COLORS = [
            '#ff6b6b', '#4ecdc4', '#ffe66d', '#9575cd', '#ffb74d',
            '#64b5f6', '#81c784', '#f06292', '#aed581'
        ];

        let choppingState = {
            enabled: false,
            sampleId: null,
            samplePath: null,
            samplesFilePath: null,
            waveformData: null,
            slices: [null, null, null, null, null, null, null, null, null], // 9 slots
            isCapturing: false,
            captureKeyCode: null,
            captureStartTime: null,
            playbackStartTime: null,
            playbackOffset: 0,
            isPlaying: false,
            duration: 0,
        };

        // Map numpad key to slice index (0-8 for slots 1-9)
        function numpadKeyToSliceIndex(code) {
            const mapping = {
                'Numpad1': 0, 'Numpad2': 1, 'Numpad3': 2,
                'Numpad4': 3, 'Numpad5': 4, 'Numpad6': 5,
                'Numpad7': 6, 'Numpad8': 7, 'Numpad9': 8,
                'Digit1': 0, 'Digit2': 1, 'Digit3': 2,
                'Digit4': 3, 'Digit5': 4, 'Digit6': 5,
                'Digit7': 6, 'Digit8': 7, 'Digit9': 8,
            };
            return mapping[code] !== undefined ? mapping[code] : null;
        }

        // Map numpad key to scale degree (0-9)
        function numpadKeyToScaleDegree(key) {
            const mapping = {
                'Numpad1': 0, 'Numpad2': 1, 'Numpad3': 2,
                'Numpad4': 3, 'Numpad5': 4, 'Numpad6': 5,
                'Numpad7': 6, 'Numpad8': 7, 'Numpad9': 8,
                'Numpad0': 0, // 0 plays root
                // Also support regular number keys as fallback
                'Digit1': 0, 'Digit2': 1, 'Digit3': 2,
                'Digit4': 3, 'Digit5': 4, 'Digit6': 5,
                'Digit7': 6, 'Digit8': 7, 'Digit9': 8,
                'Digit0': 0,
            };
            return mapping[key];
        }

        // Calculate MIDI note from octave and scale degree
        function getMidiNote(octave, scaleDegree) {
            const baseNote = (octave + 1) * 12; // C at given octave
            return baseNote + majorScaleIntervals[scaleDegree];
        }

        // Update octave display
        function updateOctaveDisplay() {
            const indicator = document.getElementById('octaveIndicator');
            if (indicator) {
                indicator.textContent = 'Oct: ' + currentOctave;
            }
        }

        // Simple hash function to detect state changes
        function computeStateHash(s) {
            const sampleIds = s.samples.map(x => x.id).sort().join(',');
            const synthdefNames = s.synthdefs.map(x => x.name + ':' + x.source).sort().join(',');
            const userTagsHash = JSON.stringify(s.userTags || {});
            return sampleIds + '|' + synthdefNames + '|' + userTagsHash;
        }

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

        // Search with tag suggestions
        const searchBox = document.getElementById('searchBox');
        const searchSuggestions = document.getElementById('searchSuggestions');

        searchBox.addEventListener('input', (e) => {
            searchQuery = e.target.value.toLowerCase();
            updateSearchSuggestions();
            render();
        });

        searchBox.addEventListener('focus', () => {
            updateSearchSuggestions();
        });

        searchBox.addEventListener('blur', () => {
            // Delay hiding to allow click on suggestions
            setTimeout(() => {
                searchSuggestions.classList.remove('visible');
            }, 200);
        });

        function updateSearchSuggestions() {
            if (!searchQuery || searchQuery.length < 2) {
                searchSuggestions.classList.remove('visible');
                return;
            }

            const tagCounts = buildTagCounts();
            const matchingTags = Object.entries(tagCounts)
                .filter(([tag]) => tag.includes(searchQuery) && !activeTagFilters.includes(tag))
                .sort((a, b) => b[1] - a[1])
                .slice(0, 8);

            if (matchingTags.length === 0) {
                searchSuggestions.classList.remove('visible');
                return;
            }

            let html = '<div class="suggestion-header">Filter by tag</div>';
            for (const [tag, count] of matchingTags) {
                const isUserTag = state.allUserTags.includes(tag);
                const isFavorite = tag === 'favorite';
                const classes = ['suggestion'];
                if (isFavorite) classes.push('favorite');
                else if (isUserTag) classes.push('user-tag');

                html += \`<div class="\${classes.join(' ')}" data-tag="\${tag}">
                    <span>\${isFavorite ? '★ ' : ''}\${tag}</span>
                    <span class="count">\${count}</span>
                </div>\`;
            }

            searchSuggestions.innerHTML = html;
            searchSuggestions.classList.add('visible');

            // Add click handlers for suggestions
            searchSuggestions.querySelectorAll('.suggestion').forEach(el => {
                el.addEventListener('click', () => {
                    const tag = el.dataset.tag;
                    addTagFilter(tag);
                    searchBox.value = '';
                    searchQuery = '';
                    searchSuggestions.classList.remove('visible');
                    render();
                });
            });
        }

        function addTagFilter(tag) {
            if (!activeTagFilters.includes(tag)) {
                activeTagFilters.push(tag);
                renderActiveTagChips();
            }
        }

        function removeTagFilter(tag) {
            activeTagFilters = activeTagFilters.filter(t => t !== tag);
            renderActiveTagChips();
            render();
        }

        function renderActiveTagChips() {
            const container = document.getElementById('activeTagChips');
            if (activeTagFilters.length === 0) {
                container.innerHTML = '';
                return;
            }

            container.innerHTML = activeTagFilters.map(tag => {
                const isUserTag = state.allUserTags.includes(tag);
                const isFavorite = tag === 'favorite';
                const classes = ['search-tag-chip'];
                if (isFavorite) classes.push('favorite');
                else if (isUserTag) classes.push('user-tag');

                return \`<span class="\${classes.join(' ')}" data-tag="\${tag}">
                    \${isFavorite ? '★ ' : ''}\${tag}
                    <span class="remove">×</span>
                </span>\`;
            }).join('');

            container.querySelectorAll('.search-tag-chip').forEach(chip => {
                chip.addEventListener('click', () => {
                    removeTagFilter(chip.dataset.tag);
                });
            });
        }

        // ========== Context Menu ==========
        const contextMenu = document.getElementById('contextMenu');

        function showContextMenu(x, y, type, id) {
            contextMenuTarget = { type, id };

            // Check if already favorite
            const userTags = state.userTags[type + ':' + id] || [];
            const isFavorite = userTags.includes('favorite');

            // Update favorite text
            const favItem = contextMenu.querySelector('[data-action="toggleFavorite"]');
            favItem.innerHTML = isFavorite
                ? '<span>☆</span> Remove from Favorites'
                : '<span>★</span> Add to Favorites';

            // Position menu
            contextMenu.style.left = x + 'px';
            contextMenu.style.top = y + 'px';
            contextMenu.classList.add('visible');
        }

        function hideContextMenu() {
            contextMenu.classList.remove('visible');
            contextMenuTarget = null;
        }

        // Hide context menu on click outside
        document.addEventListener('click', (e) => {
            if (!contextMenu.contains(e.target)) {
                hideContextMenu();
            }
        });

        // Handle context menu actions
        contextMenu.addEventListener('click', (e) => {
            const action = e.target.closest('.context-menu-item')?.dataset.action;
            if (!action || !contextMenuTarget) return;

            if (action === 'toggleFavorite') {
                vscode.postMessage({
                    command: 'toggleTag',
                    type: contextMenuTarget.type,
                    id: contextMenuTarget.id,
                    tag: 'favorite'
                });
            } else if (action === 'addTag') {
                const tag = prompt('Enter tag name:');
                if (tag && tag.trim()) {
                    vscode.postMessage({
                        command: 'addTag',
                        type: contextMenuTarget.type,
                        id: contextMenuTarget.id,
                        tag: tag.trim()
                    });
                }
            }

            hideContextMenu();
        });

        // ========== Tag Editor in Detail Panel ==========
        function renderTagEditor(type, id) {
            const tags = type === 'sample'
                ? getAllTags(state.samples.find(s => s.id === id), 'sample')
                : getAllTags(state.synthdefs.find(s => s.name === id), 'synthdef');

            const autoTagsHtml = tags.auto.map(tag =>
                \`<span class="tag-chip auto">\${escapeHtml(tag)}</span>\`
            ).join('');

            const userTagsHtml = tags.user.map(tag => {
                const isFavorite = tag === 'favorite';
                const classes = ['tag-chip', isFavorite ? 'favorite' : 'user-tag'];
                return \`<span class="\${classes.join(' ')}" data-tag="\${escapeHtml(tag)}">
                    \${isFavorite ? '★ ' : ''}\${escapeHtml(tag)}
                    <span class="remove" onclick="removeUserTag('\${type}', '\${escapeHtml(id)}', '\${escapeHtml(tag)}')">×</span>
                </span>\`;
            }).join('');

            // Quick toggle for favorite
            const hasFavorite = tags.user.includes('favorite');
            const favoriteBtn = \`<button class="quick-tag-btn \${hasFavorite ? 'active' : ''}" onclick="toggleFavorite('\${type}', '\${escapeHtml(id)}')">
                ★ Favorite
            </button>\`;

            return \`
                <div class="tag-editor">
                    <div class="tag-editor-title">Tags</div>
                    <div class="current-tags">
                        \${autoTagsHtml}
                        \${userTagsHtml}
                        \${!autoTagsHtml && !userTagsHtml ? '<em style="color: var(--text-muted)">No tags</em>' : ''}
                    </div>
                    <div class="tag-input-container">
                        <input type="text" class="tag-input" id="tagInput" placeholder="Add tag..." onkeydown="handleTagInput(event, '\${type}', '\${escapeHtml(id)}')">
                        <div class="tag-input-suggestions" id="tagInputSuggestions"></div>
                    </div>
                    <div class="quick-tags">
                        \${favoriteBtn}
                    </div>
                </div>
            \`;
        }

        // Global functions for tag management (called from onclick)
        window.removeUserTag = function(type, id, tag) {
            vscode.postMessage({ command: 'removeTag', type, id, tag });
        };

        window.toggleFavorite = function(type, id) {
            vscode.postMessage({ command: 'toggleTag', type, id, tag: 'favorite' });
        };

        window.handleTagInput = function(event, type, id) {
            if (event.key === 'Enter') {
                const input = event.target;
                const tag = input.value.trim();
                if (tag) {
                    vscode.postMessage({ command: 'addTag', type, id, tag });
                    input.value = '';
                }
            }
        };

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
                const newHash = computeStateHash(message.data);
                const stateChanged = newHash !== lastStateHash;
                state = message.data;
                lastStateHash = newHash;
                // Only re-render if state actually changed
                if (stateChanged) {
                    render();
                }
            } else if (message.type === 'testStopped') {
                playingTestSynthdef = null;
                render();
            } else if (message.type === 'waveformData') {
                // Show chopping panel with waveform data
                showChoppingPanel(
                    message.sampleId,
                    message.samplePath,
                    message.waveform,
                    message.samplesFilePath
                );
            } else if (message.type === 'playbackStarted') {
                // Already handled by startChoppingPlayback
            } else if (message.type === 'playbackStopped') {
                // Already handled by stopChoppingPlayback
            }
        });

        // Track currently held keys (to prevent key repeat from triggering multiple note-ons)
        let heldKeys = new Set();

        // Keyboard event handler for numpad playback
        document.addEventListener('keydown', (e) => {
            // Don't handle if focused on an input
            if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT') {
                return;
            }

            // ========== Chopping Mode Keyboard Handling ==========
            if (choppingState.enabled) {
                // Escape to exit chopping mode
                if (e.code === 'Escape') {
                    e.preventDefault();
                    exitChoppingMode();
                    return;
                }

                // Space to toggle playback
                if (e.code === 'Space') {
                    e.preventDefault();
                    if (choppingState.isPlaying) {
                        stopChoppingPlayback();
                    } else {
                        startChoppingPlayback();
                    }
                    return;
                }

                // Numpad 1-9 for slice capture (key down = start)
                const sliceIndex = numpadKeyToSliceIndex(e.code);
                if (sliceIndex !== null) {
                    e.preventDefault();
                    if (!heldKeys.has(e.code)) {
                        heldKeys.add(e.code);
                        captureSliceStart(sliceIndex);
                    }
                    return;
                }

                return; // Don't process other keys in chopping mode
            }

            // ========== Normal Mode Keyboard Handling ==========

            // Handle arrow keys for navigation
            if (e.code === 'ArrowDown') {
                e.preventDefault();
                navigateItems(1);
                return;
            }
            if (e.code === 'ArrowUp') {
                e.preventDefault();
                navigateItems(-1);
                return;
            }

            // Handle octave shift with + and -
            if (e.code === 'NumpadAdd' || (e.key === '+' && !e.shiftKey)) {
                e.preventDefault();
                if (currentOctave < 7) {
                    currentOctave++;
                    updateOctaveDisplay();
                }
                return;
            }
            if (e.code === 'NumpadSubtract' || e.key === '-') {
                e.preventDefault();
                if (currentOctave > 1) {
                    currentOctave--;
                    updateOctaveDisplay();
                }
                return;
            }

            // Handle Enter for one-shot playback at root pitch
            if (e.code === 'NumpadEnter' || e.code === 'Enter') {
                e.preventDefault();
                triggerSelectedItemAtPitch(getMidiNote(currentOctave, 0), true, false);
                return;
            }

            // Handle numpad keys for scale playback (sustained)
            const scaleDegree = numpadKeyToScaleDegree(e.code);
            if (scaleDegree !== undefined) {
                e.preventDefault();
                // Ignore key repeat (key is already held)
                if (heldKeys.has(e.code)) {
                    return;
                }
                heldKeys.add(e.code);
                const midiNote = getMidiNote(currentOctave, scaleDegree);
                triggerSelectedItemAtPitch(midiNote, false, true); // sustained = true
                return;
            }
        });

        // Keyup handler to release sustained notes
        document.addEventListener('keyup', (e) => {
            // Don't handle if focused on an input
            if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT') {
                return;
            }

            // ========== Chopping Mode Key Release ==========
            if (choppingState.enabled) {
                const sliceIndex = numpadKeyToSliceIndex(e.code);
                if (sliceIndex !== null && heldKeys.has(e.code)) {
                    e.preventDefault();
                    heldKeys.delete(e.code);
                    captureSliceEnd(sliceIndex);
                }
                return;
            }

            // ========== Normal Mode Key Release ==========
            const scaleDegree = numpadKeyToScaleDegree(e.code);
            if (scaleDegree !== undefined && heldKeys.has(e.code)) {
                e.preventDefault();
                heldKeys.delete(e.code);
                const midiNote = getMidiNote(currentOctave, scaleDegree);
                releaseNote(midiNote);
            }
        });

        // Release a note
        function releaseNote(midiNote) {
            vscode.postMessage({
                command: 'releaseTestNote',
                midiNote: midiNote
            });
        }

        // Get current filtered list of items based on active tab, search, and tag filters
        // Returns items in the same order they appear in the rendered list
        function getCurrentItems() {
            if (activeTab === 'samples') {
                // Use same logic as renderSamples()
                const sampleSearchFields = [
                    { getValue: s => s.id, weight: 2.0 },
                    { getValue: s => s.path, weight: 0.5 },
                    { getValue: s => getAllTags(s, 'sample').all, weight: 1.5 }
                ];

                return state.samples
                    .map(s => {
                        const searchResult = searchItem(s, searchQuery, sampleSearchFields);
                        const tagMatch = matchesTagFilters(s, 'sample');
                        return { item: s, score: searchResult.score, match: searchResult.match && tagMatch };
                    })
                    .filter(x => x.match)
                    .sort((a, b) => {
                        if (b.score !== a.score) return b.score - a.score;
                        return a.item.id.localeCompare(b.item.id);
                    })
                    .map(x => x.item);
            } else if (activeTab === 'synthdefs') {
                // Use same logic as renderSynthdefs() - instruments only
                const synthdefSearchFields = [
                    { getValue: s => s.name, weight: 2.0 },
                    { getValue: s => s._stdlib?.description, weight: 1.0 },
                    { getValue: s => s._stdlib?.category, weight: 1.0 },
                    { getValue: s => s._stdlib?.subcategory, weight: 1.0 },
                    { getValue: s => getAllTags(s, 'synthdef').all, weight: 1.5 }
                ];

                const filtered = state.synthdefs
                    .filter(s => !isEffect(s))  // Only instruments
                    .map(s => {
                        const searchResult = searchItem(s, searchQuery, synthdefSearchFields);
                        const tagMatch = matchesTagFilters(s, 'synthdef');
                        return { item: s, score: searchResult.score, match: searchResult.match && tagMatch };
                    })
                    .filter(x => x.match);

                // Group by source: stdlib → builtin → user
                const stdlib = filtered.filter(x => x.item.source === 'stdlib')
                    .sort((a, b) => b.score - a.score || a.item.name.localeCompare(b.item.name))
                    .map(x => x.item);
                const builtin = filtered.filter(x => x.item.source === 'builtin')
                    .sort((a, b) => b.score - a.score || a.item.name.localeCompare(b.item.name))
                    .map(x => x.item);
                const user = filtered.filter(x => x.item.source === 'user' || !x.item.source)
                    .sort((a, b) => b.score - a.score || a.item.name.localeCompare(b.item.name))
                    .map(x => x.item);

                return [...stdlib, ...builtin, ...user];
            } else if (activeTab === 'effects') {
                // Use same logic as renderEffects() - effects only
                const synthdefSearchFields = [
                    { getValue: s => s.name, weight: 2.0 },
                    { getValue: s => s._stdlib?.description, weight: 1.0 },
                    { getValue: s => s._stdlib?.category, weight: 1.0 },
                    { getValue: s => s._stdlib?.subcategory, weight: 1.0 },
                    { getValue: s => getAllTags(s, 'synthdef').all, weight: 1.5 }
                ];

                const filtered = state.synthdefs
                    .filter(s => isEffect(s))  // Only effects
                    .map(s => {
                        const searchResult = searchItem(s, searchQuery, synthdefSearchFields);
                        const tagMatch = matchesTagFilters(s, 'synthdef');
                        return { item: s, score: searchResult.score, match: searchResult.match && tagMatch };
                    })
                    .filter(x => x.match);

                // Group by source: stdlib → builtin → user
                const stdlib = filtered.filter(x => x.item.source === 'stdlib')
                    .sort((a, b) => b.score - a.score || a.item.name.localeCompare(b.item.name))
                    .map(x => x.item);
                const builtin = filtered.filter(x => x.item.source === 'builtin')
                    .sort((a, b) => b.score - a.score || a.item.name.localeCompare(b.item.name))
                    .map(x => x.item);
                const user = filtered.filter(x => x.item.source === 'user' || !x.item.source)
                    .sort((a, b) => b.score - a.score || a.item.name.localeCompare(b.item.name))
                    .map(x => x.item);

                return [...stdlib, ...builtin, ...user];
            }
            return [];
        }

        // Get the ID/name of an item
        function getItemId(item) {
            return activeTab === 'samples' ? item.id : item.name;
        }

        // Navigate to next/previous item
        function navigateItems(direction) {
            const items = getCurrentItems();
            if (items.length === 0) return;

            const currentIndex = items.findIndex(item => getItemId(item) === selectedItem);
            let newIndex;

            if (currentIndex === -1) {
                // Nothing selected, select first or last
                newIndex = direction > 0 ? 0 : items.length - 1;
            } else {
                newIndex = currentIndex + direction;
                // Clamp to valid range
                if (newIndex < 0) newIndex = 0;
                if (newIndex >= items.length) newIndex = items.length - 1;
            }

            const newItem = items[newIndex];
            selectItem(getItemId(newItem), newItem);
        }

        // Select an item and show its detail
        function selectItem(itemId, itemData) {
            // If selection changed and we had a test voice, clean it up
            if (selectedItem !== itemId && lastPlayedItem !== null) {
                vscode.postMessage({ command: 'stopTestVoice' });
                lastPlayedItem = null;
            }

            selectedItem = itemId;

            if (activeTab === 'samples') {
                showSampleDetail(itemData || state.samples.find(s => s.id === itemId));
            } else {
                showSynthdefDetail(itemData || state.synthdefs.find(s => s.name === itemId));
            }

            render();

            // Scroll the selected item into view
            setTimeout(() => {
                const selectedEl = document.querySelector('.item.selected');
                if (selectedEl) {
                    selectedEl.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
                }
            }, 10);
        }

        // Trigger the currently selected item at a specific MIDI pitch
        function triggerSelectedItemAtPitch(midiNote, isOneShot, sustained) {
            if (!selectedItem) return;

            // Check if we're playing a different item than before
            const itemChanged = lastPlayedItem !== null && lastPlayedItem !== selectedItem;

            // If item changed and we're in sustained mode, force a new voice
            const forcedSustained = sustained && !itemChanged;

            if (activeTab === 'samples') {
                lastPlayedItem = selectedItem;
                vscode.postMessage({
                    command: 'testSampleAtPitch',
                    sampleId: selectedItem,
                    midiNote: midiNote,
                    isOneShot: isOneShot,
                    sustained: forcedSustained,
                    forceNewVoice: itemChanged
                });
            } else if (activeTab === 'synthdefs') {
                lastPlayedItem = selectedItem;
                playingTestSynthdef = selectedItem;
                render();
                vscode.postMessage({
                    command: 'testSynthDefAtPitch',
                    synthdefName: selectedItem,
                    midiNote: midiNote,
                    isOneShot: isOneShot,
                    sustained: forcedSustained,
                    forceNewVoice: itemChanged
                });
            }
        }

        // ========== Chopping Mode Functions ==========

        function enterChoppingMode(sampleId) {
            choppingState.sampleId = sampleId;
            choppingState.slices = [null, null, null, null, null, null, null, null, null];
            choppingState.isCapturing = false;
            choppingState.isPlaying = false;
            choppingState.playbackStartTime = null;

            // Request waveform data from extension
            vscode.postMessage({ command: 'enterChoppingMode', sampleId: sampleId });
        }

        function exitChoppingMode() {
            // Stop any playback
            if (choppingState.isPlaying) {
                stopChoppingPlayback();
            }

            choppingState.enabled = false;
            choppingState.waveformData = null;
            document.getElementById('choppingPanel').classList.remove('visible');
        }

        function showChoppingPanel(sampleId, samplePath, waveformData, samplesFilePath) {
            choppingState.enabled = true;
            choppingState.sampleId = sampleId;
            choppingState.samplePath = samplePath;
            choppingState.samplesFilePath = samplesFilePath;
            choppingState.waveformData = waveformData;
            choppingState.duration = waveformData.duration;

            // Update UI
            document.getElementById('choppingSubtitle').textContent = sampleId;
            document.getElementById('targetFile').textContent = samplesFilePath
                ? '→ ' + samplesFilePath.split('/').pop()
                : '→ New samples file';

            // Show panel
            document.getElementById('choppingPanel').classList.add('visible');

            // Draw waveform
            drawWaveform();

            // Render slices list
            renderSlicesList();

            // Update position display
            updatePositionDisplay(0);

            // Update code output
            updateSliceCodeOutput();
        }

        function drawWaveform() {
            const canvas = document.getElementById('waveformCanvas');
            const ctx = canvas.getContext('2d');

            // Set canvas size to match container
            const rect = canvas.parentElement.getBoundingClientRect();
            canvas.width = rect.width;
            canvas.height = rect.height;

            const width = canvas.width;
            const height = canvas.height;
            const centerY = height / 2;

            // Clear canvas with gradient background
            const bgGradient = ctx.createLinearGradient(0, 0, 0, height);
            bgGradient.addColorStop(0, '#1a1a1a');
            bgGradient.addColorStop(0.5, '#1e1e1e');
            bgGradient.addColorStop(1, '#1a1a1a');
            ctx.fillStyle = bgGradient;
            ctx.fillRect(0, 0, width, height);

            if (!choppingState.waveformData) return;

            const peaks = choppingState.waveformData.peaks;
            const numPeaks = peaks.length / 2; // min/max pairs
            const duration = choppingState.duration;

            // Draw time grid
            ctx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
            ctx.lineWidth = 1;
            const gridInterval = duration > 30 ? 10 : duration > 10 ? 5 : duration > 5 ? 1 : 0.5;
            for (let t = gridInterval; t < duration; t += gridInterval) {
                const x = (t / duration) * width;
                ctx.beginPath();
                ctx.moveTo(x, 0);
                ctx.lineTo(x, height);
                ctx.stroke();
            }

            // Draw center line
            ctx.strokeStyle = 'rgba(255, 255, 255, 0.08)';
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(0, centerY);
            ctx.lineTo(width, centerY);
            ctx.stroke();

            // Draw slice regions first (behind waveform)
            choppingState.slices.forEach((slice, index) => {
                if (!slice) return;
                const startX = (slice.startSeconds / duration) * width;
                const endX = (slice.endSeconds / duration) * width;

                // Slice fill with gradient
                const sliceGradient = ctx.createLinearGradient(0, 0, 0, height);
                sliceGradient.addColorStop(0, SLICE_COLORS[index] + '30');
                sliceGradient.addColorStop(0.5, SLICE_COLORS[index] + '50');
                sliceGradient.addColorStop(1, SLICE_COLORS[index] + '30');
                ctx.fillStyle = sliceGradient;
                ctx.fillRect(startX, 0, endX - startX, height);

                // Draw edges with glow effect
                ctx.shadowColor = SLICE_COLORS[index];
                ctx.shadowBlur = 4;
                ctx.strokeStyle = SLICE_COLORS[index];
                ctx.lineWidth = 2;
                ctx.beginPath();
                ctx.moveTo(startX, 0);
                ctx.lineTo(startX, height);
                ctx.stroke();
                ctx.beginPath();
                ctx.moveTo(endX, 0);
                ctx.lineTo(endX, height);
                ctx.stroke();
                ctx.shadowBlur = 0;

                // Draw slice number badge
                ctx.fillStyle = SLICE_COLORS[index];
                ctx.beginPath();
                ctx.roundRect(startX + 4, 4, 18, 18, 3);
                ctx.fill();
                ctx.fillStyle = '#000';
                ctx.font = 'bold 11px sans-serif';
                ctx.textAlign = 'center';
                ctx.fillText((index + 1).toString(), startX + 13, 17);
                ctx.textAlign = 'left';
            });

            // Draw waveform with gradient
            const waveGradient = ctx.createLinearGradient(0, 0, 0, height);
            waveGradient.addColorStop(0, '#7eb356');
            waveGradient.addColorStop(0.3, '#9bbb59');
            waveGradient.addColorStop(0.5, '#b5d178');
            waveGradient.addColorStop(0.7, '#9bbb59');
            waveGradient.addColorStop(1, '#7eb356');
            ctx.strokeStyle = waveGradient;
            ctx.lineWidth = 1;

            for (let i = 0; i < numPeaks; i++) {
                const x = (i / numPeaks) * width;
                const min = peaks[i * 2];
                const max = peaks[i * 2 + 1];

                const minY = centerY - (min * centerY * 0.85);
                const maxY = centerY - (max * centerY * 0.85);

                ctx.beginPath();
                ctx.moveTo(x, minY);
                ctx.lineTo(x, maxY);
                ctx.stroke();
            }

            // Draw subtle reflection effect
            ctx.globalAlpha = 0.15;
            for (let i = 0; i < numPeaks; i++) {
                const x = (i / numPeaks) * width;
                const max = peaks[i * 2 + 1];
                const reflectionY = centerY + (max * centerY * 0.3);

                ctx.beginPath();
                ctx.moveTo(x, centerY);
                ctx.lineTo(x, reflectionY);
                ctx.stroke();
            }
            ctx.globalAlpha = 1;
        }

        function startChoppingPlayback() {
            if (!choppingState.sampleId) return;

            choppingState.isPlaying = true;
            choppingState.playbackStartTime = performance.now();
            choppingState.playbackOffset = 0;

            vscode.postMessage({
                command: 'playSampleFull',
                sampleId: choppingState.sampleId
            });

            // Start playhead animation
            requestAnimationFrame(updatePlayhead);

            // Update button state
            document.getElementById('playBtn').textContent = '⏸';
            document.getElementById('playBtn').disabled = true;
        }

        function stopChoppingPlayback() {
            if (!choppingState.isPlaying) return;

            choppingState.isPlaying = false;
            choppingState.playbackStartTime = null;

            vscode.postMessage({ command: 'stopSamplePlayback' });

            // Hide playhead
            document.getElementById('playhead').classList.remove('active');

            // Reset button
            document.getElementById('playBtn').textContent = '▶';
            document.getElementById('playBtn').disabled = false;
        }

        function getCurrentPlaybackPosition() {
            if (!choppingState.isPlaying || !choppingState.playbackStartTime) {
                return 0;
            }
            const elapsed = (performance.now() - choppingState.playbackStartTime) / 1000;
            return Math.min(choppingState.playbackOffset + elapsed, choppingState.duration);
        }

        function updatePlayhead() {
            if (!choppingState.isPlaying) return;

            const pos = getCurrentPlaybackPosition();
            updatePositionDisplay(pos);

            // Update playhead position
            const container = document.getElementById('waveformCanvas').parentElement;
            const playhead = document.getElementById('playhead');
            const x = (pos / choppingState.duration) * container.clientWidth;

            playhead.style.left = x + 'px';
            playhead.classList.add('active');

            if (pos < choppingState.duration) {
                requestAnimationFrame(updatePlayhead);
            } else {
                // Playback ended
                choppingState.isPlaying = false;
                playhead.classList.remove('active');
                document.getElementById('playBtn').textContent = '▶';
                document.getElementById('playBtn').disabled = false;
            }
        }

        function updatePositionDisplay(pos) {
            const formatTime = (t) => {
                const mins = Math.floor(t / 60);
                const secs = t % 60;
                return mins + ':' + secs.toFixed(3).padStart(6, '0');
            };
            document.getElementById('positionDisplay').innerHTML =
                '<span class="current">' + formatTime(pos) + '</span> / ' + formatTime(choppingState.duration);
        }

        function captureSliceStart(sliceIndex) {
            if (choppingState.isCapturing) return;

            choppingState.isCapturing = true;
            choppingState.captureKeyCode = sliceIndex;
            choppingState.captureStartTime = getCurrentPlaybackPosition();

            // Visual feedback
            document.getElementById('captureHint').classList.add('capturing');
            document.getElementById('captureHint').innerHTML =
                '🔴 Capturing slice <span class="kbd">' + (sliceIndex + 1) + '</span> ... release key to set end point';
        }

        function captureSliceEnd(sliceIndex) {
            if (!choppingState.isCapturing || choppingState.captureKeyCode !== sliceIndex) return;

            const endTime = getCurrentPlaybackPosition();
            const startTime = choppingState.captureStartTime;

            // Only create slice if it has positive duration
            if (endTime > startTime) {
                const defaults = getSliceDefaults();
                choppingState.slices[sliceIndex] = {
                    index: sliceIndex,
                    startSeconds: startTime,
                    endSeconds: endTime,
                    attack: defaults.attack,
                    release: defaults.release,
                    playMode: defaults.playMode,
                };

                // Redraw and update
                drawWaveform();
                renderSlicesList();
                updateSliceCodeOutput();
            }

            choppingState.isCapturing = false;
            choppingState.captureKeyCode = null;
            choppingState.captureStartTime = null;

            // Reset visual feedback
            document.getElementById('captureHint').classList.remove('capturing');
            document.getElementById('captureHint').innerHTML =
                'Hold <span class="kbd">1</span>-<span class="kbd">9</span> on numpad to capture slice (press = start, release = end)';
        }

        function getSliceDefaults() {
            return {
                attack: parseFloat(document.getElementById('defaultAttack').value) || 0.001,
                release: parseFloat(document.getElementById('defaultRelease').value) || 0.01,
                playMode: document.getElementById('defaultPlayMode').value || 'oneshot',
            };
        }

        function renderSlicesList() {
            const list = document.getElementById('slicesList');
            const sliceCount = choppingState.slices.filter(s => s !== null).length;

            // Update count display
            document.getElementById('slicesCount').textContent = sliceCount + ' / 9';

            // Show empty state if no slices
            if (sliceCount === 0) {
                list.innerHTML = \`
                    <div class="slices-empty">
                        <div class="slices-empty-icon">🎹</div>
                        <div class="slices-empty-text">No slices captured yet</div>
                        <div class="slices-empty-hint">Play the sample and hold numpad keys to capture</div>
                    </div>
                \`;
                return;
            }

            let html = '';

            for (let i = 0; i < 9; i++) {
                const slice = choppingState.slices[i];
                if (!slice) continue;

                const colorClass = 'slice-' + (i + 1);
                const duration = (slice.endSeconds - slice.startSeconds).toFixed(3);

                html += \`
                    <div class="slice-item \${colorClass}">
                        <div class="slice-header">
                            <span class="slice-badge slice-badge-\${i + 1}">\${i + 1}</span>
                            <span class="slice-duration">\${duration}s</span>
                            <div class="slice-actions">
                                <button class="btn-icon" onclick="previewSlice(\${i})" title="Preview slice">▶</button>
                                <button class="btn-icon delete" onclick="deleteSlice(\${i})" title="Delete slice">✕</button>
                            </div>
                        </div>
                        <div class="slice-controls">
                            <div class="slice-control">
                                <span class="slice-control-label">Start</span>
                                <input type="number" class="slice-control-input" value="\${slice.startSeconds.toFixed(3)}"
                                       step="0.001" min="0" max="\${choppingState.duration}"
                                       onchange="updateSliceTime(\${i}, 'start', this.value)">
                            </div>
                            <div class="slice-control">
                                <span class="slice-control-label">End</span>
                                <input type="number" class="slice-control-input" value="\${slice.endSeconds.toFixed(3)}"
                                       step="0.001" min="0" max="\${choppingState.duration}"
                                       onchange="updateSliceTime(\${i}, 'end', this.value)">
                            </div>
                            <div class="slice-params-row">
                                <div class="slice-param">
                                    <span class="slice-control-label">Attack</span>
                                    <input type="number" class="slice-control-input" value="\${slice.attack}"
                                           step="0.001" min="0"
                                           onchange="updateSliceParam(\${i}, 'attack', this.value)">
                                </div>
                                <div class="slice-param">
                                    <span class="slice-control-label">Release</span>
                                    <input type="number" class="slice-control-input" value="\${slice.release}"
                                           step="0.001" min="0"
                                           onchange="updateSliceParam(\${i}, 'release', this.value)">
                                </div>
                                <div class="slice-param">
                                    <span class="slice-control-label">Mode</span>
                                    <select onchange="updateSliceParam(\${i}, 'playMode', this.value)">
                                        <option value="oneshot" \${slice.playMode === 'oneshot' ? 'selected' : ''}>One-shot</option>
                                        <option value="sustained" \${slice.playMode === 'sustained' ? 'selected' : ''}>Sustained</option>
                                        <option value="loop" \${slice.playMode === 'loop' ? 'selected' : ''}>Loop</option>
                                    </select>
                                </div>
                            </div>
                        </div>
                    </div>
                \`;
            }

            list.innerHTML = html;
        }

        function previewSlice(index) {
            const slice = choppingState.slices[index];
            if (!slice) return;

            vscode.postMessage({
                command: 'previewSlice',
                sampleId: choppingState.sampleId,
                startSeconds: slice.startSeconds,
                endSeconds: slice.endSeconds,
            });
        }

        function deleteSlice(index) {
            choppingState.slices[index] = null;
            drawWaveform();
            renderSlicesList();
            updateSliceCodeOutput();
        }

        function updateSliceTime(index, which, value) {
            const slice = choppingState.slices[index];
            if (!slice) return;

            const newValue = parseFloat(value);
            if (isNaN(newValue)) return;

            if (which === 'start') {
                slice.startSeconds = Math.max(0, Math.min(newValue, slice.endSeconds - 0.001));
            } else {
                slice.endSeconds = Math.max(slice.startSeconds + 0.001, Math.min(newValue, choppingState.duration));
            }

            drawWaveform();
            updateSliceCodeOutput();
        }

        function updateSliceParam(index, param, value) {
            const slice = choppingState.slices[index];
            if (!slice) return;

            if (param === 'attack' || param === 'release') {
                slice[param] = parseFloat(value) || 0;
            } else {
                slice[param] = value;
            }

            updateSliceCodeOutput();
        }

        function updateSliceCodeOutput() {
            const slices = choppingState.slices.filter(s => s !== null);
            const saveBtn = document.getElementById('saveSlicesBtn');
            const codeOutput = document.getElementById('sliceCodeOutput');

            if (slices.length === 0) {
                codeOutput.textContent = '// No slices captured yet';
                codeOutput.classList.remove('has-code');
                saveBtn.disabled = true;
                return;
            }

            saveBtn.disabled = false;
            codeOutput.classList.add('has-code');

            let code = '// Slices from ' + choppingState.sampleId + '\\n';

            slices.forEach(slice => {
                const varName = choppingState.sampleId + '_slice_' + (slice.index + 1);
                code += 'let ' + varName + ' = ' + choppingState.sampleId + '.slice(' +
                        slice.startSeconds.toFixed(3) + ', ' + slice.endSeconds.toFixed(3) + ')\\n';
                code += '    .attack(' + slice.attack + ')\\n';
                code += '    .release(' + slice.release + ')';
                if (slice.playMode === 'loop') {
                    code += '\\n    .loop_mode(true)';
                }
                code += ';\\n\\n';
            });

            document.getElementById('sliceCodeOutput').textContent = code;
        }

        function saveSlices() {
            const slices = choppingState.slices.filter(s => s !== null);
            if (slices.length === 0) return;

            let code = '';
            slices.forEach(slice => {
                const varName = choppingState.sampleId + '_slice_' + (slice.index + 1);
                code += 'let ' + varName + ' = ' + choppingState.sampleId + '.slice(' +
                        slice.startSeconds.toFixed(3) + ', ' + slice.endSeconds.toFixed(3) + ')\\n';
                code += '    .attack(' + slice.attack + ')\\n';
                code += '    .release(' + slice.release + ')';
                if (slice.playMode === 'loop') {
                    code += '\\n    .loop_mode(true)';
                }
                code += ';\\n\\n';
            });

            vscode.postMessage({
                command: 'saveSlicesToFile',
                sampleId: choppingState.sampleId,
                sliceCode: code,
            });
        }

        // ========== Chopping Mode Event Handlers ==========

        // Exit button
        document.getElementById('exitChoppingBtn').addEventListener('click', exitChoppingMode);

        // Play/Stop buttons
        document.getElementById('playBtn').addEventListener('click', startChoppingPlayback);
        document.getElementById('stopBtn').addEventListener('click', stopChoppingPlayback);

        // Save button
        document.getElementById('saveSlicesBtn').addEventListener('click', saveSlices);

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
            } else if (activeTab === 'synthdefs') {
                renderSynthdefs();
            } else if (activeTab === 'effects') {
                renderEffects();
            }
        }

        function renderSamples() {
            const list = document.getElementById('samplesList');

            // Search fields for samples
            const sampleSearchFields = [
                { getValue: s => s.id, weight: 2.0 },
                { getValue: s => s.path, weight: 0.5 },
                { getValue: s => getAllTags(s, 'sample').all, weight: 1.5 }
            ];

            // Filter and score samples
            let samples = state.samples
                .map(s => {
                    const searchResult = searchItem(s, searchQuery, sampleSearchFields);
                    const tagMatch = matchesTagFilters(s, 'sample');
                    return { sample: s, score: searchResult.score, match: searchResult.match && tagMatch };
                })
                .filter(x => x.match)
                .sort((a, b) => {
                    // Sort by score first, then alphabetically
                    if (b.score !== a.score) return b.score - a.score;
                    return a.sample.id.localeCompare(b.sample.id);
                })
                .map(x => x.sample);

            if (samples.length === 0) {
                const hasFilters = searchQuery || activeTagFilters.length > 0;
                list.innerHTML = \`
                    <div class="empty-state">
                        <div class="empty-icon">\${hasFilters ? '🔍' : '🎵'}</div>
                        <h3>\${hasFilters ? 'No Matching Samples' : 'No Samples Loaded'}</h3>
                        <p>\${hasFilters ? 'Try adjusting your search or filters.' : 'Click "Load" to add audio samples or SFZ instruments to your project.'}</p>
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

                // Get tags for display
                const tags = getAllTags(sample, 'sample');
                const displayTags = tags.all.slice(0, 5); // Limit displayed tags
                const tagsHtml = displayTags.map(tag => {
                    const isUser = tags.user.includes(tag);
                    const isFavorite = tag === 'favorite';
                    const classes = ['item-tag'];
                    if (isFavorite) classes.push('favorite');
                    else if (isUser) classes.push('user-tag');
                    return \`<span class="\${classes.join(' ')}">\${isFavorite ? '★' : ''}\${escapeHtml(tag)}</span>\`;
                }).join('');

                return \`
                    <div class="item \${selectedItem === sample.id ? 'selected' : ''}"
                         data-id="\${sample.id}" data-type="sample">
                        <div class="item-icon \${isSfz ? 'sfz' : 'sample'}">
                            \${isSfz ? '🎹' : '🔊'}
                        </div>
                        <div class="item-info">
                            <div class="item-name">\${escapeHtml(sample.id)}</div>
                            <div class="item-meta">\${escapeHtml(fileName)} • \${channels} • \${duration}</div>
                            \${tagsHtml ? '<div class="item-tags">' + tagsHtml + '</div>' : ''}
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
                        selectedItemType = 'sample';
                        showSampleDetail(state.samples.find(s => s.id === id));
                        render();
                    }
                });

                // Context menu
                item.addEventListener('contextmenu', (e) => {
                    e.preventDefault();
                    showContextMenu(e.clientX, e.clientY, 'sample', item.dataset.id);
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

        // Check if a synthdef is an effect
        function isEffect(synthdef) {
            // Check stdlib type first
            if (synthdef._stdlib?.type === 'effect') return true;
            // Check category for effects/fx
            const category = synthdef._stdlib?.category?.toLowerCase() || '';
            if (category === 'effects' || category === 'fx') return true;
            // Check name patterns for effects
            const name = synthdef.name.toLowerCase();
            const effectKeywords = ['reverb', 'delay', 'chorus', 'flanger', 'phaser', 'distort', 'compress', 'limiter', 'eq', 'filter', 'saturate', 'bitcrush', 'tremolo', 'vibrato', 'pan', 'stereo', 'fx_', '_fx'];
            return effectKeywords.some(kw => name.includes(kw));
        }

        function renderSynthdefs() {
            const list = document.getElementById('synthdefsList');

            // Search fields for synthdefs
            const synthdefSearchFields = [
                { getValue: s => s.name, weight: 2.0 },
                { getValue: s => s._stdlib?.description, weight: 1.0 },
                { getValue: s => s._stdlib?.category, weight: 1.0 },
                { getValue: s => s._stdlib?.subcategory, weight: 1.0 },
                { getValue: s => getAllTags(s, 'synthdef').all, weight: 1.5 }
            ];

            // Filter and score synthdefs (exclude effects)
            let synthdefs = state.synthdefs
                .filter(s => !isEffect(s))  // Only instruments
                .map(s => {
                    const searchResult = searchItem(s, searchQuery, synthdefSearchFields);
                    const tagMatch = matchesTagFilters(s, 'synthdef');
                    return { synthdef: s, score: searchResult.score, match: searchResult.match && tagMatch };
                })
                .filter(x => x.match);

            // Group by source and sort by score within groups
            const groups = {
                stdlib: synthdefs.filter(x => x.synthdef.source === 'stdlib')
                    .sort((a, b) => b.score - a.score || a.synthdef.name.localeCompare(b.synthdef.name))
                    .map(x => x.synthdef),
                builtin: synthdefs.filter(x => x.synthdef.source === 'builtin')
                    .sort((a, b) => b.score - a.score || a.synthdef.name.localeCompare(b.synthdef.name))
                    .map(x => x.synthdef),
                user: synthdefs.filter(x => x.synthdef.source === 'user' || !x.synthdef.source)
                    .sort((a, b) => b.score - a.score || a.synthdef.name.localeCompare(b.synthdef.name))
                    .map(x => x.synthdef)
            };

            if (synthdefs.length === 0) {
                const hasFilters = searchQuery || activeTagFilters.length > 0;
                list.innerHTML = \`
                    <div class="empty-state">
                        <div class="empty-icon">\${hasFilters ? '🔍' : '🎛️'}</div>
                        <h3>\${hasFilters ? 'No Matching Instruments' : 'No Instruments Found'}</h3>
                        <p>\${hasFilters ? 'Try adjusting your search or filters.' : 'Instruments will appear here when a VibeLang session is active.'}</p>
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
            attachSynthdefEventListeners(list);
        }

        function renderEffects() {
            const list = document.getElementById('effectsList');

            // Search fields for effects
            const synthdefSearchFields = [
                { getValue: s => s.name, weight: 2.0 },
                { getValue: s => s._stdlib?.description, weight: 1.0 },
                { getValue: s => s._stdlib?.category, weight: 1.0 },
                { getValue: s => s._stdlib?.subcategory, weight: 1.0 },
                { getValue: s => getAllTags(s, 'synthdef').all, weight: 1.5 }
            ];

            // Filter and score effects only
            let effects = state.synthdefs
                .filter(s => isEffect(s))  // Only effects
                .map(s => {
                    const searchResult = searchItem(s, searchQuery, synthdefSearchFields);
                    const tagMatch = matchesTagFilters(s, 'synthdef');
                    return { synthdef: s, score: searchResult.score, match: searchResult.match && tagMatch };
                })
                .filter(x => x.match);

            // Group by source and sort by score within groups
            const groups = {
                stdlib: effects.filter(x => x.synthdef.source === 'stdlib')
                    .sort((a, b) => b.score - a.score || a.synthdef.name.localeCompare(b.synthdef.name))
                    .map(x => x.synthdef),
                builtin: effects.filter(x => x.synthdef.source === 'builtin')
                    .sort((a, b) => b.score - a.score || a.synthdef.name.localeCompare(b.synthdef.name))
                    .map(x => x.synthdef),
                user: effects.filter(x => x.synthdef.source === 'user' || !x.synthdef.source)
                    .sort((a, b) => b.score - a.score || a.synthdef.name.localeCompare(b.synthdef.name))
                    .map(x => x.synthdef)
            };

            if (effects.length === 0) {
                const hasFilters = searchQuery || activeTagFilters.length > 0;
                list.innerHTML = \`
                    <div class="empty-state">
                        <div class="empty-icon">\${hasFilters ? '🔍' : '🔊'}</div>
                        <h3>\${hasFilters ? 'No Matching Effects' : 'No Effects Found'}</h3>
                        <p>\${hasFilters ? 'Try adjusting your search or filters.' : 'Effects will appear here when a VibeLang session is active.'}</p>
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
            attachSynthdefEventListeners(list);
        }

        // Shared event listener attachment for synthdefs/effects
        function attachSynthdefEventListeners(list) {
            list.querySelectorAll('.item').forEach(item => {
                item.addEventListener('click', (e) => {
                    if (!e.target.closest('.item-btn')) {
                        const name = item.dataset.id;
                        selectedItem = name;
                        selectedItemType = 'synthdef';
                        showSynthdefDetail(state.synthdefs.find(s => s.name === name));
                        render();
                    }
                });

                // Context menu
                item.addEventListener('contextmenu', (e) => {
                    e.preventDefault();
                    showContextMenu(e.clientX, e.clientY, 'synthdef', item.dataset.id);
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

            // Get tags for display
            const tags = getAllTags(synthdef, 'synthdef');
            const displayTags = tags.user.slice(0, 4); // Show only user tags (auto tags shown in meta)
            const tagsHtml = displayTags.map(tag => {
                const isFavorite = tag === 'favorite';
                const classes = ['item-tag'];
                if (isFavorite) classes.push('favorite');
                else classes.push('user-tag');
                return \`<span class="\${classes.join(' ')}">\${isFavorite ? '★' : ''}\${escapeHtml(tag)}</span>\`;
            }).join('');

            return \`
                <div class="item \${selectedItem === synthdef.name ? 'selected' : ''}"
                     data-id="\${synthdef.name}" data-type="synthdef">
                    <div class="item-icon synth">\${typeIcon}</div>
                    <div class="item-info">
                        <div class="item-name">\${escapeHtml(synthdef.name)} \${sourceTag} \${typeTag}</div>
                        <div class="item-meta">\${escapeHtml(metaText)}</div>
                        \${tagsHtml ? '<div class="item-tags">' + tagsHtml + '</div>' : ''}
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
                    <button class="chop-sample-btn" onclick="enterChoppingMode('\${sample.id}')">
                        🔪 Chop Sample (EP-133 Style)
                    </button>
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

                \${renderTagEditor('sample', sample.id)}
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

                \${renderTagEditor('synthdef', synthdef.name)}
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

    private _getDisconnectedHtml(): string {
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Sample Browser</title>
    <style>
        body {
            font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif);
            background: var(--vscode-editor-background);
            color: var(--vscode-editor-foreground);
            display: flex;
            align-items: center;
            justify-content: center;
            height: 100vh;
            margin: 0;
        }
        .empty-state {
            text-align: center;
            color: var(--vscode-descriptionForeground);
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
            color: var(--vscode-editor-foreground);
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
        this._tagStore.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
