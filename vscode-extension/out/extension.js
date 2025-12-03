"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = require("vscode");
const completion_1 = require("./features/completion");
const hover_1 = require("./features/hover");
const sliders_1 = require("./features/sliders");
const formatter_1 = require("./features/formatter");
function activate(context) {
    console.log('Vibelang extension is now active!');
    // Register Completion Provider
    const completionProvider = vscode.languages.registerCompletionItemProvider('vibe', new completion_1.VibelangCompletionItemProvider(context.extensionPath), '.' // Trigger completion on dot (for method chaining)
    );
    context.subscriptions.push(completionProvider);
    // Register Hover Provider
    const hoverProvider = vscode.languages.registerHoverProvider('vibe', new hover_1.VibelangHoverProvider(context.extensionPath));
    context.subscriptions.push(hoverProvider);
    // Register Document Formatting Provider
    const formattingProvider = vscode.languages.registerDocumentFormattingEditProvider('vibe', new formatter_1.VibelangDocumentFormattingEditProvider());
    context.subscriptions.push(formattingProvider);
    // Register Range Formatting Provider
    const rangeFormattingProvider = vscode.languages.registerDocumentRangeFormattingEditProvider('vibe', new formatter_1.VibelangDocumentRangeFormattingEditProvider());
    context.subscriptions.push(rangeFormattingProvider);
    // Register On-Type Formatting Provider (for auto-indent)
    const onTypeFormattingProvider = vscode.languages.registerOnTypeFormattingEditProvider('vibe', new formatter_1.VibelangOnTypeFormattingEditProvider(), '\n', '}' // Trigger on newline and closing brace
    );
    context.subscriptions.push(onTypeFormattingProvider);
    // Register Command for Sliders
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.openSliders', () => {
        sliders_1.ParameterSlidersPanel.createOrShow(context.extensionUri);
    }));
    // Register Format Document Command
    context.subscriptions.push(vscode.commands.registerCommand('vibelang.formatDocument', () => {
        const editor = vscode.window.activeTextEditor;
        if (editor && editor.document.languageId === 'vibe') {
            vscode.commands.executeCommand('editor.action.formatDocument');
        }
    }));
    if (vscode.window.registerWebviewPanelSerializer) {
        // Make sure we register a serializer in activation event
        vscode.window.registerWebviewPanelSerializer(sliders_1.ParameterSlidersPanel.viewType, {
            async deserializeWebviewPanel(webviewPanel, state) {
                sliders_1.ParameterSlidersPanel.revive(webviewPanel, context.extensionUri);
            }
        });
    }
}
function deactivate() { }
//# sourceMappingURL=extension.js.map