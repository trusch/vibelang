"use strict";
/**
 * VibeLang VS Code Extension
 *
 * Provides language support and DAW-style studio interface for VibeLang.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = require("vscode");
const cp = require("child_process");
const path = require("path");
// Language Server Protocol client
const node_1 = require("vscode-languageclient/node");
// Language features (fallback when LSP is disabled)
const completion_1 = require("./features/completion");
const hover_1 = require("./features/hover");
const dataLoader_1 = require("./utils/dataLoader");
const sliders_1 = require("./features/sliders");
const formatter_1 = require("./features/formatter");
// Studio features
const stateStore_1 = require("./state/stateStore");
const transportBar_1 = require("./views/transportBar");
const sessionExplorer_1 = require("./views/sessionExplorer");
const inspectorPanel_1 = require("./views/inspectorPanel");
const mixerPanel_1 = require("./views/mixerPanel");
const arrangementTimeline_1 = require("./views/arrangementTimeline");
const soundDesigner_1 = require("./views/soundDesigner");
const patternEditor_1 = require("./views/patternEditor");
const melodyEditor_1 = require("./views/melodyEditor");
const sampleBrowser_1 = require("./views/sampleBrowser");
const effectRack_1 = require("./views/effectRack");
// Global state store
let stateStore;
// Language Server client
let languageClient;
// Runtime process management
let runtimeProcess;
let runtimeOutputChannel;
function activate(context) {
    console.log('VibeLang extension is now active!');
    // ==========================================================================
    // Initialize State Store
    // ==========================================================================
    stateStore = new stateStore_1.StateStore();
    context.subscriptions.push({ dispose: () => stateStore?.dispose() });
    // ==========================================================================
    // Language Features (LSP or fallback)
    // ==========================================================================
    const config = vscode.workspace.getConfiguration('vibelang');
    const lspEnabled = config.get('lsp.enabled', true);
    // Diagnostic debounce state
    let diagnosticDebounceTimers = new Map();
    // Function to start the LSP client
    async function startLanguageClient() {
        const currentConfig = vscode.workspace.getConfiguration('vibelang');
        const binaryPath = currentConfig.get('runtime.binaryPath', 'vibe');
        const diagnosticDelay = currentConfig.get('lsp.diagnostics.delay', 300);
        const diagnosticsEnabled = currentConfig.get('lsp.diagnostics.enabled', true);
        const diagnosticsOnType = currentConfig.get('lsp.diagnostics.onType', true);
        const traceLevel = currentConfig.get('lsp.trace.server', 'off');
        const serverOptions = {
            command: binaryPath,
            args: ['lsp'],
            options: {
                env: process.env
            }
        };
        // Create middleware for diagnostic debouncing
        const middleware = {
            handleDiagnostics: (uri, diagnostics, next) => {
                if (!diagnosticsEnabled) {
                    // Clear diagnostics if disabled
                    next(uri, []);
                    return;
                }
                const uriString = uri.toString();
                // Clear any existing debounce timer for this document
                const existingTimer = diagnosticDebounceTimers.get(uriString);
                if (existingTimer) {
                    clearTimeout(existingTimer);
                }
                // If delay is 0, publish immediately
                if (diagnosticDelay === 0) {
                    next(uri, diagnostics);
                    return;
                }
                // Debounce: wait for delay before publishing diagnostics
                const timer = setTimeout(() => {
                    diagnosticDebounceTimers.delete(uriString);
                    next(uri, diagnostics);
                }, diagnosticDelay);
                diagnosticDebounceTimers.set(uriString, timer);
            }
        };
        const clientOptions = {
            documentSelector: [{ scheme: 'file', language: 'vibe' }],
            synchronize: {
                fileEvents: vscode.workspace.createFileSystemWatcher('**/*.vibe'),
                // Re-read config when settings change
                configurationSection: 'vibelang'
            },
            outputChannelName: 'VibeLang Language Server',
            middleware,
            initializationOptions: {
                diagnosticsEnabled,
                diagnosticsOnType,
                diagnosticDelay
            }
        };
        languageClient = new node_1.LanguageClient('vibelang', 'VibeLang Language Server', serverOptions, clientOptions);
        // Set trace level
        if (traceLevel !== 'off') {
            languageClient.setTrace(traceLevel === 'verbose' ? 2 : 1);
        }
        try {
            await languageClient.start();
            console.log('VibeLang Language Server started');
        }
        catch (err) {
            console.error('Failed to start VibeLang Language Server:', err);
            vscode.window.showWarningMessage(`Failed to start VibeLang Language Server: ${err.message}. ` +
                `Make sure the 'vibe' command is available in PATH or configure vibelang.runtime.binaryPath. ` +
                `Built-in providers will still provide basic language features.`);
        }
    }
    // Function to restart the LSP client
    async function restartLanguageClient() {
        if (languageClient) {
            vscode.window.showInformationMessage('Restarting VibeLang Language Server...');
            await languageClient.stop();
            languageClient = undefined;
        }
        await startLanguageClient();
        vscode.window.showInformationMessage('VibeLang Language Server restarted');
    }
    // Register restart LSP command
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.restartLsp', restartLanguageClient));
    // Watch for configuration changes that require LSP restart
    context.subscriptions.push(vscode.workspace.onDidChangeConfiguration(async (e) => {
        if (e.affectsConfiguration('vibelang.lsp')) {
            const action = await vscode.window.showInformationMessage('VibeLang LSP settings changed. Restart Language Server to apply?', 'Restart', 'Later');
            if (action === 'Restart') {
                await restartLanguageClient();
            }
        }
    }));
    if (lspEnabled) {
        // Start the Language Server
        startLanguageClient();
        context.subscriptions.push({
            dispose: () => {
                // Clear all debounce timers
                for (const timer of diagnosticDebounceTimers.values()) {
                    clearTimeout(timer);
                }
                diagnosticDebounceTimers.clear();
                if (languageClient) {
                    languageClient.stop();
                }
            }
        });
    }
    else {
        console.log('VibeLang LSP disabled by configuration');
    }
    // Always register hover and completion providers - they supplement the LSP
    // (or work standalone if LSP is disabled/fails)
    registerBuiltInLanguageProviders(context);
    // Document Formatting Providers (not provided by LSP yet)
    const formattingProvider = vscode.languages.registerDocumentFormattingEditProvider('vibe', new formatter_1.VibelangDocumentFormattingEditProvider(context.extensionPath));
    context.subscriptions.push(formattingProvider);
    const rangeFormattingProvider = vscode.languages.registerDocumentRangeFormattingEditProvider('vibe', new formatter_1.VibelangDocumentRangeFormattingEditProvider(context.extensionPath));
    context.subscriptions.push(rangeFormattingProvider);
    const onTypeFormattingProvider = vscode.languages.registerOnTypeFormattingEditProvider('vibe', new formatter_1.VibelangOnTypeFormattingEditProvider(), '\n', '}');
    context.subscriptions.push(onTypeFormattingProvider);
    // ==========================================================================
    // Transport Status Bar
    // ==========================================================================
    const transportBar = new transportBar_1.TransportBar(stateStore);
    context.subscriptions.push({ dispose: () => transportBar.dispose() });
    // Register transport commands
    transportBar_1.TransportBar.registerCommands(context, stateStore);
    // ==========================================================================
    // Session Explorer Tree View
    // ==========================================================================
    const sessionExplorerProvider = new sessionExplorer_1.SessionExplorerProvider(stateStore);
    const sessionExplorerView = vscode.window.createTreeView('vibelang.sessionExplorer', {
        treeDataProvider: sessionExplorerProvider,
        showCollapseAll: true,
    });
    context.subscriptions.push(sessionExplorerView);
    // Register session explorer commands
    (0, sessionExplorer_1.registerSessionExplorerCommands)(context, stateStore);
    // ==========================================================================
    // Inspector Panel
    // ==========================================================================
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openInspector', () => {
        if (stateStore) {
            inspectorPanel_1.InspectorPanel.createOrShow(stateStore);
        }
    }));
    // Register serializer for Inspector Panel
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(inspectorPanel_1.InspectorPanel.viewType, {
            async deserializeWebviewPanel(webviewPanel) {
                if (stateStore) {
                    inspectorPanel_1.InspectorPanel.revive(webviewPanel, stateStore);
                }
            }
        });
    }
    // ==========================================================================
    // Mixer Panel (Bottom Panel View)
    // ==========================================================================
    const mixerViewProvider = new mixerPanel_1.MixerViewProvider(stateStore);
    context.subscriptions.push(vscode.window.registerWebviewViewProvider(mixerPanel_1.MixerViewProvider.viewType, mixerViewProvider, {
        // Keep the webview alive when hidden to preserve state
        webviewOptions: {
            retainContextWhenHidden: true
        }
    }));
    context.subscriptions.push({ dispose: () => mixerViewProvider.dispose() });
    // Command to focus the mixer view in the bottom panel
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openMixer', () => {
        vscode.commands.executeCommand('vibelang.mixerView.focus');
    }));
    // ==========================================================================
    // Arrangement Timeline Panel
    // ==========================================================================
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openArrangement', () => {
        if (stateStore) {
            arrangementTimeline_1.ArrangementTimeline.createOrShow(stateStore);
        }
    }));
    // Register serializer for Arrangement Timeline
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(arrangementTimeline_1.ArrangementTimeline.viewType, {
            async deserializeWebviewPanel(webviewPanel) {
                if (stateStore) {
                    arrangementTimeline_1.ArrangementTimeline.revive(webviewPanel, stateStore);
                }
            }
        });
    }
    // ==========================================================================
    // Sound Designer Panel
    // ==========================================================================
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openSoundDesigner', () => {
        if (stateStore) {
            soundDesigner_1.SoundDesignerPanel.createOrShow(context.extensionPath, stateStore);
        }
    }));
    // Register serializer for Sound Designer
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(soundDesigner_1.SoundDesignerPanel.viewType, {
            async deserializeWebviewPanel(webviewPanel) {
                if (stateStore) {
                    soundDesigner_1.SoundDesignerPanel.revive(webviewPanel, context.extensionPath, stateStore);
                }
            }
        });
    }
    // ==========================================================================
    // Pattern Editor Panel
    // ==========================================================================
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openPatternEditor', (patternName) => {
        if (stateStore) {
            patternEditor_1.PatternEditor.createOrShow(stateStore, patternName);
        }
    }));
    // Register serializer for Pattern Editor
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(patternEditor_1.PatternEditor.viewType, {
            async deserializeWebviewPanel(webviewPanel) {
                if (stateStore) {
                    patternEditor_1.PatternEditor.revive(webviewPanel, stateStore);
                }
            }
        });
    }
    // ==========================================================================
    // Melody Editor Panel (Piano Roll)
    // ==========================================================================
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openMelodyEditor', (melodyName) => {
        if (stateStore) {
            melodyEditor_1.MelodyEditor.createOrShow(stateStore, melodyName);
        }
    }));
    // Register serializer for Melody Editor
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(melodyEditor_1.MelodyEditor.viewType, {
            async deserializeWebviewPanel(webviewPanel) {
                if (stateStore) {
                    melodyEditor_1.MelodyEditor.revive(webviewPanel, stateStore);
                }
            }
        });
    }
    // ==========================================================================
    // Sample Browser Panel
    // ==========================================================================
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openSampleBrowser', () => {
        if (stateStore) {
            sampleBrowser_1.SampleBrowser.createOrShow(stateStore, context);
        }
    }));
    // Register serializer for Sample Browser
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(sampleBrowser_1.SampleBrowser.viewType, {
            async deserializeWebviewPanel(webviewPanel) {
                if (stateStore) {
                    sampleBrowser_1.SampleBrowser.revive(webviewPanel, stateStore, context);
                }
            }
        });
    }
    // ==========================================================================
    // Effect Rack Panel
    // ==========================================================================
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openEffectRack', (groupPath) => {
        if (stateStore) {
            effectRack_1.EffectRack.createOrShow(stateStore, groupPath);
        }
    }));
    // Register serializer for Effect Rack
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(effectRack_1.EffectRack.viewType, {
            async deserializeWebviewPanel(webviewPanel) {
                if (stateStore) {
                    effectRack_1.EffectRack.revive(webviewPanel, stateStore);
                }
            }
        });
    }
    // ==========================================================================
    // Parameter Sliders (existing feature)
    // ==========================================================================
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openSliders', () => {
        sliders_1.ParameterSlidersPanel.createOrShow(context.extensionUri);
    }));
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(sliders_1.ParameterSlidersPanel.viewType, {
            async deserializeWebviewPanel(webviewPanel) {
                sliders_1.ParameterSlidersPanel.revive(webviewPanel, context.extensionUri);
            }
        });
    }
    // ==========================================================================
    // Format Document Command
    // ==========================================================================
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.formatDocument', () => {
        const editor = vscode.window.activeTextEditor;
        if (editor && editor.document.languageId === 'vibe') {
            vscode.commands.executeCommand('editor.action.formatDocument');
        }
    }));
    // ==========================================================================
    // Runtime Process Management Commands
    // ==========================================================================
    // Create output channel for runtime logs
    runtimeOutputChannel = vscode.window.createOutputChannel('VibeLang Runtime');
    context.subscriptions.push(runtimeOutputChannel);
    // Boot Runtime Command
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.bootRuntime', async () => {
        if (runtimeProcess && !runtimeProcess.killed) {
            const choice = await vscode.window.showWarningMessage('VibeLang runtime is already running. Restart it?', 'Restart', 'Cancel');
            if (choice === 'Restart') {
                await vscode.commands.executeCommand('vibelang.restartRuntime');
            }
            return;
        }
        // Get the current .vibe file or ask user to select one
        let vibeFile;
        const activeEditor = vscode.window.activeTextEditor;
        if (activeEditor && activeEditor.document.languageId === 'vibe') {
            vibeFile = activeEditor.document.uri.fsPath;
        }
        else {
            // Look for .vibe files in workspace
            const vibeFiles = await vscode.workspace.findFiles('**/*.vibe', '**/node_modules/**', 10);
            if (vibeFiles.length === 0) {
                vscode.window.showErrorMessage('No .vibe files found in workspace. Open a .vibe file first.');
                return;
            }
            else if (vibeFiles.length === 1) {
                vibeFile = vibeFiles[0].fsPath;
            }
            else {
                // Let user pick from available files
                const items = vibeFiles.map(f => ({
                    label: path.basename(f.fsPath),
                    description: vscode.workspace.asRelativePath(f),
                    fsPath: f.fsPath
                }));
                const picked = await vscode.window.showQuickPick(items, {
                    placeHolder: 'Select a .vibe file to run'
                });
                if (!picked) {
                    return;
                }
                vibeFile = picked.fsPath;
            }
        }
        // Get configuration
        const config = vscode.workspace.getConfiguration('vibelang');
        const binaryPath = config.get('runtime.binaryPath', 'vibe');
        const workingDir = path.dirname(vibeFile);
        // Build command arguments: vibe <file> --api
        const args = [path.basename(vibeFile), '--api'];
        // Show output channel
        runtimeOutputChannel.clear();
        runtimeOutputChannel.show(true);
        runtimeOutputChannel.appendLine(`Starting VibeLang runtime...`);
        runtimeOutputChannel.appendLine(`File: ${vibeFile}`);
        runtimeOutputChannel.appendLine(`Working directory: ${workingDir}`);
        runtimeOutputChannel.appendLine(`Command: ${binaryPath} ${args.join(' ')}`);
        runtimeOutputChannel.appendLine('---');
        try {
            runtimeProcess = cp.spawn(binaryPath, args, {
                cwd: workingDir,
                shell: true
            });
            runtimeProcess.stdout?.on('data', (data) => {
                runtimeOutputChannel.append(data.toString());
            });
            runtimeProcess.stderr?.on('data', (data) => {
                runtimeOutputChannel.append(data.toString());
            });
            runtimeProcess.on('error', (err) => {
                runtimeOutputChannel.appendLine(`\nError: ${err.message}`);
                vscode.window.showErrorMessage(`Failed to start VibeLang runtime: ${err.message}`);
                runtimeProcess = undefined;
            });
            runtimeProcess.on('exit', (code, signal) => {
                runtimeOutputChannel.appendLine(`\n--- Runtime exited (code: ${code}, signal: ${signal}) ---`);
                runtimeProcess = undefined;
            });
            vscode.window.showInformationMessage('VibeLang runtime started. Connecting...');
            // Auto-connect with retries - the RuntimeManager handles timing
            // Give the process a brief moment to start, then begin connection attempts
            setTimeout(async () => {
                if (stateStore && stateStore.status !== 'connected') {
                    // Use tryConnect directly which has built-in retry logic
                    const connected = await stateStore.connect();
                    if (!connected) {
                        const action = await vscode.window.showErrorMessage('Could not connect to VibeLang runtime after multiple attempts', 'Retry', 'Cancel');
                        if (action === 'Retry') {
                            stateStore.connect();
                        }
                    }
                }
            }, 500);
        }
        catch (err) {
            vscode.window.showErrorMessage(`Failed to start VibeLang runtime: ${err.message}`);
        }
    }));
    // Kill Runtime Command
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.killRuntime', async () => {
        if (!runtimeProcess || runtimeProcess.killed) {
            vscode.window.showInformationMessage('VibeLang runtime is not running.');
            return;
        }
        // Disconnect first
        if (stateStore && stateStore.status === 'connected') {
            vscode.commands.executeCommand('vibelang.toggleConnection');
        }
        runtimeOutputChannel.appendLine('\n--- Stopping runtime... ---');
        // Try graceful shutdown first
        runtimeProcess.kill('SIGTERM');
        // Force kill after timeout
        const forceKillTimeout = setTimeout(() => {
            if (runtimeProcess && !runtimeProcess.killed) {
                runtimeProcess.kill('SIGKILL');
                runtimeOutputChannel.appendLine('Force killed runtime.');
            }
        }, 3000);
        runtimeProcess.once('exit', () => {
            clearTimeout(forceKillTimeout);
            vscode.window.showInformationMessage('VibeLang runtime stopped.');
        });
    }));
    // Restart Runtime Command
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.restartRuntime', async () => {
        if (runtimeProcess && !runtimeProcess.killed) {
            // Disconnect first
            if (stateStore && stateStore.status === 'connected') {
                await vscode.commands.executeCommand('vibelang.toggleConnection');
            }
            runtimeOutputChannel.appendLine('\n--- Restarting runtime... ---');
            // Kill the process
            runtimeProcess.kill('SIGTERM');
            // Wait for it to exit then start again
            await new Promise((resolve) => {
                const timeout = setTimeout(() => {
                    if (runtimeProcess && !runtimeProcess.killed) {
                        runtimeProcess.kill('SIGKILL');
                    }
                    resolve();
                }, 3000);
                runtimeProcess.once('exit', () => {
                    clearTimeout(timeout);
                    resolve();
                });
            });
            // Small delay before restarting
            await new Promise(resolve => setTimeout(resolve, 500));
        }
        // Start it again
        await vscode.commands.executeCommand('vibelang.bootRuntime');
    }));
    // Cleanup on deactivation
    context.subscriptions.push({
        dispose: () => {
            if (runtimeProcess && !runtimeProcess.killed) {
                runtimeProcess.kill('SIGTERM');
            }
        }
    });
    // ==========================================================================
    // Show welcome message on first activation
    // ==========================================================================
    const hasShownWelcome = context.globalState.get('vibelang.hasShownWelcome');
    if (!hasShownWelcome) {
        vscode.window.showInformationMessage('VibeLang extension activated! Open the Session Explorer to connect to a running VibeLang instance.', 'Open Session Explorer').then(action => {
            if (action === 'Open Session Explorer') {
                vscode.commands.executeCommand('vibelang.sessionExplorer.focus');
            }
        });
        context.globalState.update('vibelang.hasShownWelcome', true);
    }
}
function deactivate() {
    if (stateStore) {
        stateStore.dispose();
        stateStore = undefined;
    }
    if (languageClient) {
        return languageClient.stop();
    }
    return undefined;
}
/**
 * Register built-in language providers for hover and completion.
 * These providers use the UGen manifests and Rhai API documentation
 * to provide tooltips and autocompletion for all built-in functions.
 * They work alongside the LSP (if running) or standalone.
 */
function registerBuiltInLanguageProviders(context) {
    // Clear cached data to ensure fresh data is loaded
    dataLoader_1.DataLoader.clearCache();
    // Completion Provider
    const completionProvider = vscode.languages.registerCompletionItemProvider('vibe', new completion_1.VibelangCompletionItemProvider(context.extensionPath), '.');
    context.subscriptions.push(completionProvider);
    // Hover Provider
    const hoverProvider = vscode.languages.registerHoverProvider('vibe', new hover_1.VibelangHoverProvider(context.extensionPath));
    context.subscriptions.push(hoverProvider);
}
//# sourceMappingURL=extension.js.map