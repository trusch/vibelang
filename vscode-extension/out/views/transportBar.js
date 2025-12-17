"use strict";
/**
 * VibeLang Transport Status Bar
 *
 * Status bar items for transport controls with Ableton-style aesthetics.
 * Shows: Connection status, Play/Stop, BPM, Beat position
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.TransportBar = void 0;
const vscode = require("vscode");
/**
 * Transport bar providing play/stop controls and beat display.
 */
class TransportBar {
    constructor(store) {
        this._disposables = [];
        this._store = store;
        // Create status bar items with specific priorities (higher = more left)
        // Connection status (leftmost in our group)
        this._connectionItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
        this._connectionItem.command = 'vibelang.toggleConnection';
        this._connectionItem.tooltip = 'VibeLang Runtime Connection';
        // Play/Stop button
        this._playStopItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 99);
        this._playStopItem.command = 'vibelang.toggleTransport';
        this._playStopItem.tooltip = 'Play/Stop Transport';
        // BPM display/control
        this._bpmItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 98);
        this._bpmItem.command = 'vibelang.setBpm';
        this._bpmItem.tooltip = 'Click to set BPM';
        // Beat position display
        this._beatItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 97);
        this._beatItem.command = 'vibelang.seekBeat';
        this._beatItem.tooltip = 'Beat position (click to seek)';
        // Bar:Beat:Subdivision display
        this._timeItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 96);
        this._timeItem.tooltip = 'Bar : Beat : Subdivision';
        // Subscribe to state updates
        this._disposables.push(this._store.onStatusChange((status) => this.updateConnection(status)));
        this._disposables.push(this._store.onTransportUpdate((transport) => this.updateTransport(transport)));
        // Initial update
        this.updateConnection(this._store.status);
        if (this._store.transport) {
            this.updateTransport(this._store.transport);
        }
        // Show all items
        this._connectionItem.show();
        this._playStopItem.show();
        this._bpmItem.show();
        this._beatItem.show();
        this._timeItem.show();
    }
    // ==========================================================================
    // Updates
    // ==========================================================================
    updateConnection(status) {
        switch (status) {
            case 'connected':
                this._connectionItem.text = '$(plug) VibeLang';
                this._connectionItem.backgroundColor = undefined;
                this._connectionItem.color = '#9bbb59'; // Ableton green
                this.setControlsEnabled(true);
                break;
            case 'connecting':
                this._connectionItem.text = '$(sync~spin) VibeLang';
                this._connectionItem.backgroundColor = undefined;
                this._connectionItem.color = '#d19a66'; // Orange
                this.setControlsEnabled(false);
                break;
            case 'error':
                this._connectionItem.text = '$(warning) VibeLang';
                this._connectionItem.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
                this._connectionItem.color = undefined;
                this.setControlsEnabled(false);
                break;
            case 'disconnected':
            default:
                this._connectionItem.text = '$(debug-disconnect) VibeLang';
                this._connectionItem.backgroundColor = undefined;
                this._connectionItem.color = '#6b6b6b'; // Dim gray
                this.setControlsEnabled(false);
                break;
        }
    }
    updateTransport(transport) {
        // Play/Stop button
        if (transport.running) {
            this._playStopItem.text = '$(debug-pause)';
            this._playStopItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
        }
        else {
            this._playStopItem.text = '$(play)';
            this._playStopItem.backgroundColor = undefined;
        }
        // BPM display
        this._bpmItem.text = `${transport.bpm.toFixed(1)} BPM`;
        // Beat position - use loop_beat when available for looping display
        const displayBeat = transport.loop_beat ?? transport.current_beat;
        this._beatItem.text = `$(pulse) ${displayBeat.toFixed(2)}`;
        // Bar:Beat:Sub display
        const { bar, beatInBar, subdivision } = this.beatsToBarTime(displayBeat, transport.time_signature.numerator);
        // Show loop indicator if looping
        const loopIndicator = transport.loop_beats ? '$(sync) ' : '';
        this._timeItem.text = `${loopIndicator}${bar}:${beatInBar}:${subdivision}`;
    }
    setControlsEnabled(enabled) {
        if (enabled) {
            this._playStopItem.show();
            this._bpmItem.show();
            this._beatItem.show();
            this._timeItem.show();
        }
        else {
            // Show dimmed versions
            this._playStopItem.text = '$(play)';
            this._playStopItem.backgroundColor = undefined;
            this._bpmItem.text = '--- BPM';
            this._beatItem.text = '$(pulse) ---';
            this._timeItem.text = '-:-:-';
        }
    }
    // ==========================================================================
    // Helpers
    // ==========================================================================
    /**
     * Convert beat position to bar:beat:subdivision format.
     */
    beatsToBarTime(totalBeats, beatsPerBar) {
        const bar = Math.floor(totalBeats / beatsPerBar) + 1;
        const beatInBar = Math.floor(totalBeats % beatsPerBar) + 1;
        const subdivision = Math.floor((totalBeats % 1) * 4) + 1; // 16th note subdivision
        return { bar, beatInBar, subdivision };
    }
    // ==========================================================================
    // Commands
    // ==========================================================================
    /**
     * Register commands for the transport bar.
     */
    static registerCommands(context, store) {
        // Toggle connection
        context.subscriptions.push(vscode.commands.registerCommand('vibelang.toggleConnection', async () => {
            if (store.status === 'connected') {
                store.disconnect();
            }
            else {
                const connected = await store.connect();
                if (!connected) {
                    const action = await vscode.window.showErrorMessage('Could not connect to VibeLang runtime', 'Configure', 'Retry');
                    if (action === 'Configure') {
                        vscode.commands.executeCommand('vibelang.configureConnection');
                    }
                    else if (action === 'Retry') {
                        store.connect();
                    }
                }
            }
        }));
        // Toggle transport (play/stop)
        context.subscriptions.push(vscode.commands.registerCommand('vibelang.toggleTransport', async () => {
            if (store.transport?.running) {
                await store.runtime.stopTransport();
            }
            else {
                await store.runtime.startTransport();
            }
        }));
        // Start transport
        context.subscriptions.push(vscode.commands.registerCommand('vibelang.startTransport', async () => {
            await store.runtime.startTransport();
        }));
        // Stop transport
        context.subscriptions.push(vscode.commands.registerCommand('vibelang.stopTransport', async () => {
            await store.runtime.stopTransport();
        }));
        // Set BPM
        context.subscriptions.push(vscode.commands.registerCommand('vibelang.setBpm', async () => {
            const currentBpm = store.transport?.bpm ?? 120;
            const input = await vscode.window.showInputBox({
                prompt: 'Enter BPM (20-999)',
                value: currentBpm.toString(),
                validateInput: (value) => {
                    const num = parseFloat(value);
                    if (isNaN(num) || num < 20 || num > 999) {
                        return 'BPM must be between 20 and 999';
                    }
                    return null;
                },
            });
            if (input) {
                await store.runtime.updateTransport({ bpm: parseFloat(input) });
            }
        }));
        // Seek to beat
        context.subscriptions.push(vscode.commands.registerCommand('vibelang.seekBeat', async () => {
            const currentBeat = store.transport?.current_beat ?? 0;
            const input = await vscode.window.showInputBox({
                prompt: 'Enter beat position',
                value: Math.floor(currentBeat).toString(),
                validateInput: (value) => {
                    const num = parseFloat(value);
                    if (isNaN(num) || num < 0) {
                        return 'Beat must be a positive number';
                    }
                    return null;
                },
            });
            if (input) {
                await store.runtime.seekTransport(parseFloat(input));
            }
        }));
        // Configure connection
        context.subscriptions.push(vscode.commands.registerCommand('vibelang.configureConnection', async () => {
            const host = await vscode.window.showInputBox({
                prompt: 'Enter VibeLang runtime host',
                value: 'localhost',
            });
            if (!host)
                return;
            const portStr = await vscode.window.showInputBox({
                prompt: 'Enter VibeLang runtime port',
                value: '1606',
                validateInput: (value) => {
                    const num = parseInt(value);
                    if (isNaN(num) || num < 1 || num > 65535) {
                        return 'Port must be between 1 and 65535';
                    }
                    return null;
                },
            });
            if (!portStr)
                return;
            const port = parseInt(portStr);
            await store.connect(host, port);
        }));
    }
    // ==========================================================================
    // Cleanup
    // ==========================================================================
    dispose() {
        this._connectionItem.dispose();
        this._playStopItem.dispose();
        this._bpmItem.dispose();
        this._beatItem.dispose();
        this._timeItem.dispose();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
exports.TransportBar = TransportBar;
//# sourceMappingURL=transportBar.js.map