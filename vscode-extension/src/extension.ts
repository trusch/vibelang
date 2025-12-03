import * as vscode from 'vscode';
import { VibelangCompletionItemProvider } from './features/completion';
import { VibelangHoverProvider } from './features/hover';
import { ParameterSlidersPanel } from './features/sliders';
import {
    VibelangDocumentFormattingEditProvider,
    VibelangDocumentRangeFormattingEditProvider,
    VibelangOnTypeFormattingEditProvider
} from './features/formatter';

export function activate(context: vscode.ExtensionContext) {
    console.log('Vibelang extension is now active!');

    // Register Completion Provider
    const completionProvider = vscode.languages.registerCompletionItemProvider(
        'vibe',
        new VibelangCompletionItemProvider(context.extensionPath),
        '.' // Trigger completion on dot (for method chaining)
    );
    context.subscriptions.push(completionProvider);

    // Register Hover Provider
    const hoverProvider = vscode.languages.registerHoverProvider(
        'vibe',
        new VibelangHoverProvider(context.extensionPath)
    );
    context.subscriptions.push(hoverProvider);

    // Register Document Formatting Provider
    const formattingProvider = vscode.languages.registerDocumentFormattingEditProvider(
        'vibe',
        new VibelangDocumentFormattingEditProvider()
    );
    context.subscriptions.push(formattingProvider);

    // Register Range Formatting Provider
    const rangeFormattingProvider = vscode.languages.registerDocumentRangeFormattingEditProvider(
        'vibe',
        new VibelangDocumentRangeFormattingEditProvider()
    );
    context.subscriptions.push(rangeFormattingProvider);

    // Register On-Type Formatting Provider (for auto-indent)
    const onTypeFormattingProvider = vscode.languages.registerOnTypeFormattingEditProvider(
        'vibe',
        new VibelangOnTypeFormattingEditProvider(),
        '\n', '}' // Trigger on newline and closing brace
    );
    context.subscriptions.push(onTypeFormattingProvider);

    // Register Command for Sliders
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.openSliders', () => {
            ParameterSlidersPanel.createOrShow(context.extensionUri);
        })
    );

    // Register Format Document Command
    context.subscriptions.push(
        vscode.commands.registerCommand('vibelang.formatDocument', () => {
            const editor = vscode.window.activeTextEditor;
            if (editor && editor.document.languageId === 'vibe') {
                vscode.commands.executeCommand('editor.action.formatDocument');
            }
        })
    );

    if (vscode.window.registerWebviewPanelSerializer) {
        // Make sure we register a serializer in activation event
        vscode.window.registerWebviewPanelSerializer(ParameterSlidersPanel.viewType, {
            async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel, state: any) {
                ParameterSlidersPanel.revive(webviewPanel, context.extensionUri);
            }
        });
    }
}

export function deactivate() {}

