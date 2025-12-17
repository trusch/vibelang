"use strict";
/**
 * Base class for webview panels.
 *
 * Provides common functionality for:
 * - Singleton panel management (createOrShow/revive pattern)
 * - State store integration
 * - Message handling
 * - Disposable management
 * - HTML content generation
 *
 * Subclasses should implement:
 * - getHtmlContent(): Generate the panel's HTML
 * - handleMessage(message): Handle messages from the webview
 * - setupSubscriptions(): Subscribe to state updates
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.BaseWebviewPanel = void 0;
exports.registerWebviewSerializer = registerWebviewSerializer;
const vscode = require("vscode");
/**
 * Static registry for panel instances by viewType.
 * This allows us to manage singletons across different panel classes.
 */
const panelRegistry = new Map();
/**
 * Abstract base class for webview panels with StateStore integration.
 *
 * @template TMessage - The type of messages this panel handles
 */
class BaseWebviewPanel {
    /**
     * Create or show a panel of this type.
     * Call this from static createOrShow methods in subclasses.
     */
    static getOrCreatePanel(viewType, factory) {
        const existing = panelRegistry.get(viewType);
        if (existing) {
            existing._panel.reveal();
            return existing;
        }
        const panel = factory();
        panelRegistry.set(viewType, panel);
        return panel;
    }
    /**
     * Revive a panel from a serialized state.
     * Call this from static revive methods in subclasses.
     */
    static revivePanel(viewType, factory) {
        const panel = factory();
        panelRegistry.set(viewType, panel);
        return panel;
    }
    /**
     * Create a new webview panel.
     */
    constructor(panel, store, extensionUri) {
        this._disposables = [];
        this._panel = panel;
        this._store = store;
        this._extensionUri = extensionUri;
        // Set up message handling
        this._panel.webview.onDidReceiveMessage((message) => this.handleMessage(message), null, this._disposables);
        // Set up dispose handler
        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
        // Let subclass set up its subscriptions
        this.setupSubscriptions();
        // Initial content update
        this.updateContent();
    }
    /**
     * Create a webview panel with the given configuration.
     */
    static createWebviewPanel(config, extensionUri) {
        const localResourceRoots = config.localResourceRoots ?? [];
        if (extensionUri) {
            localResourceRoots.push(extensionUri);
        }
        return vscode.window.createWebviewPanel(config.viewType, config.title, config.column ?? vscode.ViewColumn.Two, {
            enableScripts: config.enableScripts ?? true,
            retainContextWhenHidden: config.retainContextWhenHidden ?? true,
            localResourceRoots: localResourceRoots.length > 0 ? localResourceRoots : undefined,
        });
    }
    /**
     * Update the webview content.
     * Can be overridden for custom update logic.
     */
    updateContent() {
        this._panel.webview.html = this.getHtmlContent();
    }
    /**
     * Send a message to the webview.
     */
    postMessage(message) {
        return this._panel.webview.postMessage(message);
    }
    /**
     * Set up state store subscriptions.
     * Override in subclasses to subscribe to relevant events.
     */
    setupSubscriptions() {
        // Default: subscribe to status changes
        this._disposables.push(this._store.onStatusChange(() => this.updateContent()));
    }
    /**
     * Dispose of this panel and clean up resources.
     */
    dispose() {
        // Remove from registry
        panelRegistry.delete(this.viewType);
        // Dispose the panel
        this._panel.dispose();
        // Dispose all subscriptions
        while (this._disposables.length) {
            const d = this._disposables.pop();
            if (d) {
                d.dispose();
            }
        }
    }
    /**
     * Get the webview panel (for serialization support).
     */
    get panel() {
        return this._panel;
    }
    /**
     * Check if a panel of the given type currently exists.
     */
    static hasPanel(viewType) {
        return panelRegistry.has(viewType);
    }
    /**
     * Get the current panel of the given type, if it exists.
     */
    static getPanel(viewType) {
        return panelRegistry.get(viewType);
    }
    // =========================================================================
    // HTML Helper Methods
    // =========================================================================
    /**
     * Wrap content in a standard HTML document with common styles.
     */
    wrapHtml(content, additionalStyles = '', additionalScripts = '') {
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>${this._panel.title}</title>
    <style>
        ${this.getBaseStyles()}
        ${additionalStyles}
    </style>
</head>
<body>
    ${content}
    <script>
        const vscode = acquireVsCodeApi();

        function sendMessage(message) {
            vscode.postMessage(message);
        }

        ${additionalScripts}
    </script>
</body>
</html>`;
    }
    /**
     * Get the base CSS styles used by all panels.
     * Can be overridden for custom base styles.
     */
    getBaseStyles() {
        return `
        :root {
            --bg-primary: #1a1a1a;
            --bg-secondary: #232323;
            --bg-tertiary: #2d2d2d;
            --text-primary: #d4d4d4;
            --text-secondary: #858585;
            --text-muted: #5a5a5a;
            --accent-green: #9bbb59;
            --accent-orange: #d19a66;
            --accent-red: #d16969;
            --accent-blue: #569cd6;
            --accent-purple: #c586c0;
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
        }

        .empty-state {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            height: 80vh;
            color: var(--text-secondary);
            text-align: center;
        }

        .empty-icon {
            font-size: 48px;
            margin-bottom: 16px;
            opacity: 0.5;
        }

        .panel-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 10px 16px;
            border-bottom: 1px solid var(--border);
            position: sticky;
            top: 0;
            background: var(--bg-secondary);
            z-index: 10;
        }

        .panel-header h1 {
            font-size: 13px;
            font-weight: 600;
            color: var(--text-secondary);
            text-transform: uppercase;
            letter-spacing: 1px;
        }

        button {
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            color: var(--text-primary);
            padding: 6px 12px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 11px;
        }

        button:hover {
            background: var(--bg-secondary);
            border-color: var(--text-secondary);
        }

        button:active {
            background: var(--bg-primary);
        }

        button.primary {
            background: var(--accent-blue);
            border-color: var(--accent-blue);
            color: #fff;
        }

        button.primary:hover {
            opacity: 0.9;
        }

        input[type="range"] {
            -webkit-appearance: none;
            background: var(--bg-tertiary);
            border-radius: 4px;
            cursor: pointer;
        }

        input[type="range"]::-webkit-slider-thumb {
            -webkit-appearance: none;
            width: 12px;
            height: 12px;
            background: var(--text-secondary);
            border-radius: 50%;
            cursor: pointer;
        }

        input[type="text"], input[type="number"], select {
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            color: var(--text-primary);
            padding: 6px 10px;
            border-radius: 4px;
            font-size: 11px;
        }

        input[type="text"]:focus, input[type="number"]:focus, select:focus {
            outline: none;
            border-color: var(--accent-blue);
        }

        .status-indicator {
            width: 8px;
            height: 8px;
            border-radius: 50%;
            background: var(--text-muted);
        }

        .status-indicator.connected {
            background: var(--accent-green);
            box-shadow: 0 0 6px var(--accent-green);
        }

        .status-indicator.error {
            background: var(--accent-red);
            box-shadow: 0 0 6px var(--accent-red);
        }
        `;
    }
    /**
     * Render an empty state with icon and message.
     */
    renderEmptyState(icon, title, message) {
        return `
        <div class="empty-state">
            <div class="empty-icon">${icon}</div>
            <h2>${title}</h2>
            <p>${message}</p>
        </div>
        `;
    }
    /**
     * Render a disconnected state.
     */
    renderDisconnectedState() {
        return this.renderEmptyState('🔌', 'Not Connected', 'Connect to a VibeLang runtime to use this panel.');
    }
    /**
     * Navigate to a source location.
     */
    async goToSource(file, line, column) {
        try {
            const doc = await vscode.workspace.openTextDocument(file);
            const editor = await vscode.window.showTextDocument(doc);
            const position = new vscode.Position(Math.max(0, line - 1), column ?? 0);
            editor.selection = new vscode.Selection(position, position);
            editor.revealRange(new vscode.Range(position, position), vscode.TextEditorRevealType.InCenter);
        }
        catch (e) {
            vscode.window.showErrorMessage(`Could not open file: ${file}`);
        }
    }
}
exports.BaseWebviewPanel = BaseWebviewPanel;
/**
 * Helper to register a webview panel serializer.
 */
function registerWebviewSerializer(context, viewType, reviveFactory) {
    if (vscode.window.registerWebviewPanelSerializer) {
        context.subscriptions.push(vscode.window.registerWebviewPanelSerializer(viewType, {
            async deserializeWebviewPanel(webviewPanel) {
                reviveFactory(webviewPanel);
            }
        }));
    }
}
//# sourceMappingURL=baseWebviewPanel.js.map