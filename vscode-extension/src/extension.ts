/**
 * VibeLang VS Code Extension
 *
 * Provides language support and DAW-style studio interface for VibeLang.
 */

import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';

// Language Server Protocol client
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

// Language features (fallback when LSP is disabled)
import { VibelangCompletionItemProvider } from './features/completion';
import { VibelangHoverProvider } from './features/hover';
import { ParameterSlidersPanel } from './features/sliders';
import {
    VibelangDocumentFormattingEditProvider,
    VibelangDocumentRangeFormattingEditProvider,
    VibelangOnTypeFormattingEditProvider
} from './features/formatter';

// Studio features
import { StateStore } from './state/stateStore';
import { TransportBar } from './views/transportBar';
import { SessionExplorerProvider, registerSessionExplorerCommands } from './views/sessionExplorer';
import { InspectorPanel } from './views/inspectorPanel';
import { MixerViewProvider } from './views/mixerPanel';
import { ArrangementTimeline } from './views/arrangementTimeline';
import { SoundDesignerPanel } from './views/soundDesigner';
import { PatternEditor } from './views/patternEditor';
import { MelodyEditor } from './views/melodyEditor';
import { SampleBrowser } from './views/sampleBrowser';
import { EffectRack } from './views/effectRack';

// Global state store
let stateStore: StateStore | undefined;

// Language Server client
let languageClient: LanguageClient | undefined;

// Runtime process management
let runtimeProcess: cp.ChildProcess | undefined;
let runtimeOutputChannel: vscode.OutputChannel | undefined;

export function activate(context: vscode.ExtensionContext) {
    console.log('VibeLang extension is now active!');

    // ==========================================================================
    // Initialize State Store
    // ==========================================================================
    stateStore = new StateStore();
    context.subscriptions.push({ dispose: () => stateStore?.dispose() });

    // ==========================================================================
    // Language Features (LSP or fallback)
    // ==========================================================================

    const config = vscode.workspace.getConfiguration('vibelang');
    const lspEnabled = config.get<boolean>('lsp.enabled', true);

    // Function to start the LSP client
    async function startLanguageClient() {
        const currentConfig = vscode.workspace.getConfiguration('vibelang');
        const binaryPath = currentConfig.get<string>('runtime.binaryPath', 'vibe');

        const serverOptions: ServerOptions = {
            command: binaryPath,
            args: ['lsp'],
            options: {
                env: process.env
            }
        };

        const clientOptions: LanguageClientOptions = {
            documentSelector: [{ scheme: 'file', language: 'vibe' }],
            synchronize: {
                fileEvents: vscode.workspace.createFileSystemWatcher('**/*.vibe')
            },
            outputChannelName: 'VibeLang Language Server',
        };

        languageClient = new LanguageClient(
            'vibelang',
            'VibeLang Language Server',
            serverOptions,
            clientOptions
        );

        try {
            await languageClient.start();
            console.log('VibeLang Language Server started');
        } catch (err: any) {
            console.error('Failed to start VibeLang Language Server:', err);
            vscode.window.showWarningMessage(
                `Failed to start VibeLang Language Server: ${err.message}. ` +
                `Make sure the 'vibe' command is available in PATH or configure vibelang.runtime.binaryPath. ` +
                `Falling back to built-in providers.`
            );
            // Register fallback providers
            registerFallbackLanguageProviders(context);
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
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.restartLsp', restartLanguageClient)
    );

    if (lspEnabled) {
        // Start the Language Server
        startLanguageClient();

        context.subscriptions.push({
            dispose: () => {
                if (languageClient) {
                    languageClient.stop();
                }
            }
        });
    } else {
        // LSP disabled, use fallback providers
        registerFallbackLanguageProviders(context);
    }

    // Document Formatting Providers (not provided by LSP yet)
    const formattingProvider = vscode.languages.registerDocumentFormattingEditProvider(
        'vibe',
        new VibelangDocumentFormattingEditProvider()
    );
    context.subscriptions.push(formattingProvider);

    const rangeFormattingProvider = vscode.languages.registerDocumentRangeFormattingEditProvider(
        'vibe',
        new VibelangDocumentRangeFormattingEditProvider()
    );
    context.subscriptions.push(rangeFormattingProvider);

    const onTypeFormattingProvider = vscode.languages.registerOnTypeFormattingEditProvider(
        'vibe',
        new VibelangOnTypeFormattingEditProvider(),
        '\n', '}'
    );
    context.subscriptions.push(onTypeFormattingProvider);

    // ==========================================================================
    // Transport Status Bar
    // ==========================================================================
    const transportBar = new TransportBar(stateStore);
    context.subscriptions.push({ dispose: () => transportBar.dispose() });

    // Register transport commands
    TransportBar.registerCommands(context, stateStore);

    // ==========================================================================
    // Session Explorer Tree View
    // ==========================================================================
    const sessionExplorerProvider = new SessionExplorerProvider(stateStore);
    const sessionExplorerView = vscode.window.createTreeView('vibelang.sessionExplorer', {
        treeDataProvider: sessionExplorerProvider,
        showCollapseAll: true,
    });
    context.subscriptions.push(sessionExplorerView);

    // Register session explorer commands
    registerSessionExplorerCommands(context, stateStore);

    // ==========================================================================
    // Inspector Panel
    // ==========================================================================
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openInspector', () => {
            if (stateStore) {
                InspectorPanel.createOrShow(stateStore);
            }
        })
    );

    // Register serializer for Inspector Panel
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(InspectorPanel.viewType, {
            async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel) {
                if (stateStore) {
                    InspectorPanel.revive(webviewPanel, stateStore);
                }
            }
        });
    }

    // ==========================================================================
    // Mixer Panel (Bottom Panel View)
    // ==========================================================================
    const mixerViewProvider = new MixerViewProvider(stateStore);
    context.subscriptions.push(
        vscode.window.registerWebviewViewProvider(
            MixerViewProvider.viewType,
            mixerViewProvider,
            {
                // Keep the webview alive when hidden to preserve state
                webviewOptions: {
                    retainContextWhenHidden: true
                }
            }
        )
    );
    context.subscriptions.push({ dispose: () => mixerViewProvider.dispose() });

    // Command to focus the mixer view in the bottom panel
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openMixer', () => {
            vscode.commands.executeCommand('vibelang.mixerView.focus');
        })
    );

    // ==========================================================================
    // Arrangement Timeline Panel
    // ==========================================================================
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openArrangement', () => {
            if (stateStore) {
                ArrangementTimeline.createOrShow(stateStore);
            }
        })
    );

    // Register serializer for Arrangement Timeline
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(ArrangementTimeline.viewType, {
            async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel) {
                if (stateStore) {
                    ArrangementTimeline.revive(webviewPanel, stateStore);
                }
            }
        });
    }

    // ==========================================================================
    // Sound Designer Panel
    // ==========================================================================
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openSoundDesigner', () => {
            if (stateStore) {
                SoundDesignerPanel.createOrShow(context.extensionPath, stateStore);
            }
        })
    );

    // Register serializer for Sound Designer
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(SoundDesignerPanel.viewType, {
            async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel) {
                if (stateStore) {
                    SoundDesignerPanel.revive(webviewPanel, context.extensionPath, stateStore);
                }
            }
        });
    }

    // ==========================================================================
    // Pattern Editor Panel
    // ==========================================================================
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openPatternEditor', (patternName?: string) => {
            if (stateStore) {
                PatternEditor.createOrShow(stateStore, patternName);
            }
        })
    );

    // Register serializer for Pattern Editor
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(PatternEditor.viewType, {
            async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel) {
                if (stateStore) {
                    PatternEditor.revive(webviewPanel, stateStore);
                }
            }
        });
    }

    // ==========================================================================
    // Melody Editor Panel (Piano Roll)
    // ==========================================================================
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openMelodyEditor', (melodyName?: string) => {
            if (stateStore) {
                MelodyEditor.createOrShow(stateStore, melodyName);
            }
        })
    );

    // Register serializer for Melody Editor
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(MelodyEditor.viewType, {
            async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel) {
                if (stateStore) {
                    MelodyEditor.revive(webviewPanel, stateStore);
                }
            }
        });
    }

    // ==========================================================================
    // Sample Browser Panel
    // ==========================================================================
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openSampleBrowser', () => {
            if (stateStore) {
                SampleBrowser.createOrShow(stateStore, context.extensionPath);
            }
        })
    );

    // Register serializer for Sample Browser
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(SampleBrowser.viewType, {
            async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel) {
                if (stateStore) {
                    SampleBrowser.revive(webviewPanel, stateStore, context.extensionPath);
                }
            }
        });
    }

    // ==========================================================================
    // Effect Rack Panel
    // ==========================================================================
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openEffectRack', (groupPath?: string) => {
            if (stateStore) {
                EffectRack.createOrShow(stateStore, groupPath);
            }
        })
    );

    // Register serializer for Effect Rack
    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(EffectRack.viewType, {
            async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel) {
                if (stateStore) {
                    EffectRack.revive(webviewPanel, stateStore);
                }
            }
        });
    }

    // ==========================================================================
    // Parameter Sliders (existing feature)
    // ==========================================================================
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openSliders', () => {
            ParameterSlidersPanel.createOrShow(context.extensionUri);
        })
    );

    if (vscode.window.registerWebviewPanelSerializer) {
        vscode.window.registerWebviewPanelSerializer(ParameterSlidersPanel.viewType, {
            async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel) {
                ParameterSlidersPanel.revive(webviewPanel, context.extensionUri);
            }
        });
    }

    // ==========================================================================
    // Format Document Command
    // ==========================================================================
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.formatDocument', () => {
            const editor = vscode.window.activeTextEditor;
            if (editor && editor.document.languageId === 'vibe') {
                vscode.commands.executeCommand('editor.action.formatDocument');
            }
        })
    );

    // ==========================================================================
    // Runtime Process Management Commands
    // ==========================================================================

    // Create output channel for runtime logs
    runtimeOutputChannel = vscode.window.createOutputChannel('VibeLang Runtime');
    context.subscriptions.push(runtimeOutputChannel);

    // Boot Runtime Command
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.bootRuntime', async () => {
            if (runtimeProcess && !runtimeProcess.killed) {
                const choice = await vscode.window.showWarningMessage(
                    'VibeLang runtime is already running. Restart it?',
                    'Restart', 'Cancel'
                );
                if (choice === 'Restart') {
                    await vscode.commands.executeCommand('vibelang.restartRuntime');
                }
                return;
            }

            // Get the current .vibe file or ask user to select one
            let vibeFile: string | undefined;
            const activeEditor = vscode.window.activeTextEditor;

            if (activeEditor && activeEditor.document.languageId === 'vibe') {
                vibeFile = activeEditor.document.uri.fsPath;
            } else {
                // Look for .vibe files in workspace
                const vibeFiles = await vscode.workspace.findFiles('**/*.vibe', '**/node_modules/**', 10);
                if (vibeFiles.length === 0) {
                    vscode.window.showErrorMessage('No .vibe files found in workspace. Open a .vibe file first.');
                    return;
                } else if (vibeFiles.length === 1) {
                    vibeFile = vibeFiles[0].fsPath;
                } else {
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
            const binaryPath = config.get<string>('runtime.binaryPath', 'vibe');
            const workingDir = path.dirname(vibeFile);

            // Build command arguments: vibe <file> --api
            const args = [path.basename(vibeFile), '--api'];

            // Show output channel
            runtimeOutputChannel!.clear();
            runtimeOutputChannel!.show(true);
            runtimeOutputChannel!.appendLine(`Starting VibeLang runtime...`);
            runtimeOutputChannel!.appendLine(`File: ${vibeFile}`);
            runtimeOutputChannel!.appendLine(`Working directory: ${workingDir}`);
            runtimeOutputChannel!.appendLine(`Command: ${binaryPath} ${args.join(' ')}`);
            runtimeOutputChannel!.appendLine('---');

            try {
                runtimeProcess = cp.spawn(binaryPath, args, {
                    cwd: workingDir,
                    shell: true
                });

                runtimeProcess.stdout?.on('data', (data) => {
                    runtimeOutputChannel!.append(data.toString());
                });

                runtimeProcess.stderr?.on('data', (data) => {
                    runtimeOutputChannel!.append(data.toString());
                });

                runtimeProcess.on('error', (err) => {
                    runtimeOutputChannel!.appendLine(`\nError: ${err.message}`);
                    vscode.window.showErrorMessage(`Failed to start VibeLang runtime: ${err.message}`);
                    runtimeProcess = undefined;
                });

                runtimeProcess.on('exit', (code, signal) => {
                    runtimeOutputChannel!.appendLine(`\n--- Runtime exited (code: ${code}, signal: ${signal}) ---`);
                    runtimeProcess = undefined;
                });

                vscode.window.showInformationMessage('VibeLang runtime started. Connecting...');

                // Auto-connect after a short delay
                setTimeout(() => {
                    if (stateStore && stateStore.status !== 'connected') {
                        vscode.commands.executeCommand('vibelang.toggleConnection');
                    }
                }, 1500);

            } catch (err: any) {
                vscode.window.showErrorMessage(`Failed to start VibeLang runtime: ${err.message}`);
            }
        })
    );

    // Kill Runtime Command
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.killRuntime', async () => {
            if (!runtimeProcess || runtimeProcess.killed) {
                vscode.window.showInformationMessage('VibeLang runtime is not running.');
                return;
            }

            // Disconnect first
            if (stateStore && stateStore.status === 'connected') {
                vscode.commands.executeCommand('vibelang.toggleConnection');
            }

            runtimeOutputChannel!.appendLine('\n--- Stopping runtime... ---');

            // Try graceful shutdown first
            runtimeProcess.kill('SIGTERM');

            // Force kill after timeout
            const forceKillTimeout = setTimeout(() => {
                if (runtimeProcess && !runtimeProcess.killed) {
                    runtimeProcess.kill('SIGKILL');
                    runtimeOutputChannel!.appendLine('Force killed runtime.');
                }
            }, 3000);

            runtimeProcess.once('exit', () => {
                clearTimeout(forceKillTimeout);
                vscode.window.showInformationMessage('VibeLang runtime stopped.');
            });
        })
    );

    // Restart Runtime Command
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.restartRuntime', async () => {
            if (runtimeProcess && !runtimeProcess.killed) {
                // Disconnect first
                if (stateStore && stateStore.status === 'connected') {
                    await vscode.commands.executeCommand('vibelang.toggleConnection');
                }

                runtimeOutputChannel!.appendLine('\n--- Restarting runtime... ---');

                // Kill the process
                runtimeProcess.kill('SIGTERM');

                // Wait for it to exit then start again
                await new Promise<void>((resolve) => {
                    const timeout = setTimeout(() => {
                        if (runtimeProcess && !runtimeProcess.killed) {
                            runtimeProcess.kill('SIGKILL');
                        }
                        resolve();
                    }, 3000);

                    runtimeProcess!.once('exit', () => {
                        clearTimeout(timeout);
                        resolve();
                    });
                });

                // Small delay before restarting
                await new Promise(resolve => setTimeout(resolve, 500));
            }

            // Start it again
            await vscode.commands.executeCommand('vibelang.bootRuntime');
        })
    );

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
        vscode.window.showInformationMessage(
            'VibeLang extension activated! Open the Session Explorer to connect to a running VibeLang instance.',
            'Open Session Explorer'
        ).then(action => {
            if (action === 'Open Session Explorer') {
                vscode.commands.executeCommand('vibelang.sessionExplorer.focus');
            }
        });
        context.globalState.update('vibelang.hasShownWelcome', true);
    }
}

export function deactivate(): Thenable<void> | undefined {
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
 * Register fallback language providers (used when LSP is disabled or fails to start)
 */
function registerFallbackLanguageProviders(context: vscode.ExtensionContext) {
    // Completion Provider
    const completionProvider = vscode.languages.registerCompletionItemProvider(
        'vibe',
        new VibelangCompletionItemProvider(context.extensionPath),
        '.'
    );
    context.subscriptions.push(completionProvider);

    // Hover Provider
    const hoverProvider = vscode.languages.registerHoverProvider(
        'vibe',
        new VibelangHoverProvider(context.extensionPath)
    );
    context.subscriptions.push(hoverProvider);
}
