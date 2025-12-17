/**
 * VibeLang Mixer Panel (Bottom Panel View)
 *
 * WebviewViewProvider that shows channel strips for all groups with faders and meters.
 * Integrated into VS Code's bottom panel alongside Terminal, Problems, etc.
 * Uses Ableton-style dark theme aesthetics.
 */

import * as vscode from 'vscode';
import { StateStore } from '../state/stateStore';
import { Group } from '../api/types';
import { MixerMessage } from '../api/webviewMessages';

const METER_POLL_INTERVAL = 50; // ms - fast polling for smooth meters

export class MixerViewProvider implements vscode.WebviewViewProvider {
    public static readonly viewType = 'vibelang.mixerView';

    private _view?: vscode.WebviewView;
    private readonly _store: StateStore;
    private _disposables: vscode.Disposable[] = [];
    private _meterTimer: NodeJS.Timeout | null = null;
    private _isInitialized = false;

    constructor(store: StateStore) {
        this._store = store;

        // Listen for state updates - these will only take effect once the view is resolved
        this._disposables.push(
            store.onGroupsUpdate(() => {
                if (this._isInitialized) {
                    this._updateContent();
                }
            })
        );

        this._disposables.push(
            store.onStatusChange(() => {
                if (this._isInitialized) {
                    this._updateContent();
                    this._updateMeterPolling();
                }
            })
        );
    }

    public resolveWebviewView(
        webviewView: vscode.WebviewView,
        _context: vscode.WebviewViewResolveContext,
        _token: vscode.CancellationToken
    ): void {
        this._view = webviewView;

        // Configure webview options
        webviewView.webview.options = {
            enableScripts: true,
        };

        // Mark as initialized before setting content
        this._isInitialized = true;

        // Set initial content immediately
        try {
            const html = this._getHtmlContent();
            webviewView.webview.html = html;
        } catch (err) {
            console.error('MixerViewProvider: Error setting initial HTML:', err);
            webviewView.webview.html = this._getErrorHtml(err);
        }

        // Handle messages from webview
        webviewView.webview.onDidReceiveMessage(
            (message) => this._handleMessage(message),
            null,
            this._disposables
        );

        // Handle visibility changes - pause/resume meter polling
        webviewView.onDidChangeVisibility(() => {
            if (this._view?.visible) {
                // Refresh content when becoming visible
                this._updateContent();
            }
            this._updateMeterPolling();
        });

        // Handle disposal
        webviewView.onDidDispose(() => {
            this._stopMeterPolling();
            this._isInitialized = false;
        });

        // Start meter polling if connected and visible
        this._updateMeterPolling();
    }

    private _getErrorHtml(err: unknown): string {
        const errorMessage = err instanceof Error ? err.message : String(err);
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Mixer Error</title>
    <style>
        body {
            font-family: var(--vscode-font-family, sans-serif);
            background: var(--vscode-panel-background, #1e1e1e);
            color: var(--vscode-errorForeground, #f14c4c);
            padding: 16px;
        }
    </style>
</head>
<body>
    <div>Error loading mixer: ${errorMessage}</div>
</body>
</html>`;
    }

    private _updateMeterPolling(): void {
        // Stop existing timer
        this._stopMeterPolling();

        // Only poll when connected and view is visible
        if (this._store.status === 'connected' && this._view?.visible) {
            this._meterTimer = setInterval(() => this._pollMeters(), METER_POLL_INTERVAL);
        }
    }

    private _stopMeterPolling(): void {
        if (this._meterTimer) {
            clearInterval(this._meterTimer);
            this._meterTimer = null;
        }
    }

    private async _pollMeters(): Promise<void> {
        if (!this._view) return;

        try {
            const meters = await this._store.runtime.getMeters();
            if (meters) {
                this._view.webview.postMessage({
                    type: 'meters',
                    data: meters
                });
            }
        } catch {
            // Ignore meter polling errors
        }
    }

    private _updateContent(): void {
        if (!this._view || !this._isInitialized) {
            return;
        }

        try {
            const html = this._getHtmlContent();
            this._view.webview.html = html;
        } catch (err) {
            console.error('MixerViewProvider: Error updating content:', err);
            this._view.webview.html = this._getErrorHtml(err);
        }
    }

    private async _handleMessage(message: MixerMessage): Promise<void> {
        switch (message.command) {
            case 'setAmp':
                await this._store.runtime.setGroupParam(
                    message.path,
                    'amp',
                    message.value
                );
                break;
            case 'setPan':
                await this._store.runtime.setGroupParam(
                    message.path,
                    'pan',
                    message.value
                );
                break;
            case 'mute':
                const group = this._store.getGroup(message.path);
                if (group) {
                    if (group.muted) {
                        await this._store.runtime.unmuteGroup(message.path);
                    } else {
                        await this._store.runtime.muteGroup(message.path);
                    }
                }
                break;
            case 'solo':
                const g = this._store.getGroup(message.path);
                if (g) {
                    if (g.soloed) {
                        await this._store.runtime.unsoloGroup(message.path);
                    } else {
                        await this._store.runtime.soloGroup(message.path);
                    }
                }
                break;
            case 'select':
                this._store.selectGroup(message.path);
                vscode.commands.executeCommand('vibelang.openInspector');
                break;
        }
    }

    private _getHtmlContent(): string {
        const status = this._store.status;
        const groups = this._store.groups;

        if (status !== 'connected') {
            return this._wrapHtml(`
                <div class="mixer-container">
                    <div class="empty-state">
                        <span class="empty-icon">🔌</span>
                        <span>Not connected to VibeLang runtime</span>
                    </div>
                </div>
            `);
        }

        if (groups.length === 0) {
            return this._wrapHtml(`
                <div class="mixer-container">
                    <div class="empty-state">
                        <span class="empty-icon">🎛️</span>
                        <span>No groups in session</span>
                    </div>
                </div>
            `);
        }

        const channels = groups.map(g => this._renderChannel(g)).join('');

        return this._wrapHtml(`
            <div class="mixer-container">
                <div class="mixer-channels">
                    ${channels}
                </div>
            </div>
        `);
    }

    private _renderChannel(group: Group): string {
        const amp = group.params['amp'] ?? 1.0;
        const isActive = this._store.isGroupActive(group.path);
        const ampDb = this._ampToDb(amp);
        const pan = group.params['pan'] ?? 0;

        return `
            <div class="channel ${group.muted ? 'muted' : ''} ${group.soloed ? 'soloed' : ''} ${isActive ? 'active' : ''}"
                 data-path="${group.path}"
                 onclick="sendMessage({command:'select', path:'${group.path}'})">

                <div class="channel-top">
                    <div class="channel-name" title="${group.name}">${group.name}</div>
                    <div class="channel-controls">
                        <button class="btn-mute ${group.muted ? 'active' : ''}"
                                onclick="event.stopPropagation(); sendMessage({command:'mute', path:'${group.path}'})">M</button>
                        <button class="btn-solo ${group.soloed ? 'active' : ''}"
                                onclick="event.stopPropagation(); sendMessage({command:'solo', path:'${group.path}'})">S</button>
                    </div>
                </div>

                <div class="channel-middle">
                    <div class="meter-section">
                        <div class="meter-scale">
                            <span class="meter-label" style="top: 0%;">+6</span>
                            <span class="meter-label" style="top: 18.75%;">0</span>
                            <span class="meter-label" style="top: 37.5%;">-6</span>
                            <span class="meter-label" style="top: 56.25%;">-12</span>
                            <span class="meter-label" style="top: 75%;">-24</span>
                            <span class="meter-label" style="top: 100%;">-∞</span>
                        </div>
                        <div class="meter-container">
                            <canvas class="meter-canvas meter-left" data-path="${group.path}" data-channel="left" width="8" height="120"></canvas>
                            <canvas class="meter-canvas meter-right" data-path="${group.path}" data-channel="right" width="8" height="120"></canvas>
                        </div>
                    </div>

                    <div class="fader-section">
                        <input type="range" class="fader"
                               data-path="${group.path}"
                               min="0" max="1.5" step="0.01" value="${amp}"
                               orient="vertical"
                               oninput="handleFaderInput(event, '${group.path}')"
                               onclick="event.stopPropagation()"
                               ondblclick="resetFader(event, '${group.path}')">
                    </div>
                </div>

                <div class="channel-bottom">
                    <div class="fader-value">
                        <input type="text" class="fader-value-input"
                               data-path="${group.path}"
                               value="${ampDb.toFixed(1)}"
                               onclick="event.stopPropagation(); this.select()"
                               onkeydown="handleDbInput(event, '${group.path}')"
                               onblur="commitDbValue(this, '${group.path}')">
                        <span class="fader-unit">dB</span>
                    </div>
                    <div class="pan-container">
                        <span class="pan-label">L</span>
                        <input type="range" class="pan-knob"
                               min="-1" max="1" step="0.01" value="${pan}"
                               oninput="event.stopPropagation(); sendMessage({command:'setPan', path:'${group.path}', value:parseFloat(this.value)})"
                               onclick="event.stopPropagation()">
                        <span class="pan-label">R</span>
                    </div>
                </div>
            </div>
        `;
    }

    private _ampToDb(amp: number): number {
        if (amp <= 0) return -Infinity;
        return 20 * Math.log10(amp);
    }

    private _getNonce(): string {
        let text = '';
        const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
        for (let i = 0; i < 32; i++) {
            text += possible.charAt(Math.floor(Math.random() * possible.length));
        }
        return text;
    }

    private _wrapHtml(content: string): string {
        // Generate a nonce for CSP
        const nonce = this._getNonce();

        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:;">
    <title>Mixer</title>
    <style>
        :root {
            --bg-primary: var(--vscode-panel-background, #1e1e1e);
            --bg-secondary: var(--vscode-sideBar-background, #252526);
            --bg-tertiary: var(--vscode-input-background, #3c3c3c);
            --bg-channel: #1a1a1a;
            --text-primary: var(--vscode-foreground, #cccccc);
            --text-secondary: var(--vscode-descriptionForeground, #858585);
            --text-muted: #5a5a5a;
            --accent-green: #9bbb59;
            --accent-orange: #d19a66;
            --accent-red: #d16969;
            --accent-blue: #569cd6;
            --border: var(--vscode-panel-border, #3c3c3c);
        }

        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }

        html, body {
            font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif);
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: 11px;
            overflow-x: auto;
            overflow-y: hidden;
            height: 100%;
            width: 100%;
        }

        .empty-state {
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 8px;
            min-height: 100px;
            color: var(--text-secondary);
            padding: 16px;
        }

        .empty-icon {
            font-size: 24px;
            opacity: 0.6;
        }

        .mixer-container {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            height: 100%;
            min-height: 220px;
        }

        .mixer-channels {
            display: flex;
            justify-content: center;
            align-items: center;
            padding: 8px;
            gap: 6px;
            flex: 1;
            overflow-x: auto;
            overflow-y: hidden;
            width: 100%;
        }

        .channel {
            display: flex;
            flex-direction: column;
            width: 80px;
            min-width: 80px;
            max-width: 80px;
            height: 210px;
            flex-shrink: 0;
            background: linear-gradient(to bottom, var(--bg-channel), #141414);
            border: 1px solid var(--border);
            border-radius: 4px;
            padding: 8px 6px;
            cursor: pointer;
            transition: all 0.15s ease;
        }

        .channel:hover {
            border-color: var(--text-secondary);
            background: linear-gradient(to bottom, #222, #181818);
        }

        .channel.active {
            border-color: var(--accent-green);
            box-shadow: 0 0 8px rgba(155, 187, 89, 0.25);
        }

        .channel.muted {
            opacity: 0.5;
        }

        .channel.soloed {
            border-color: var(--accent-orange);
            box-shadow: 0 0 8px rgba(209, 154, 102, 0.25);
        }

        .channel-top {
            display: flex;
            flex-direction: column;
            gap: 4px;
            margin-bottom: 6px;
        }

        .channel-name {
            font-size: 9px;
            font-weight: 600;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            text-transform: uppercase;
            letter-spacing: 0.3px;
            text-align: center;
            color: var(--text-primary);
        }

        .channel-controls {
            display: flex;
            gap: 3px;
            justify-content: center;
        }

        .btn-mute, .btn-solo {
            width: 20px;
            height: 16px;
            border: none;
            border-radius: 2px;
            font-size: 8px;
            font-weight: 700;
            cursor: pointer;
            background: var(--bg-tertiary);
            color: var(--text-muted);
            transition: all 0.1s ease;
            padding: 0;
        }

        .btn-mute:hover {
            background: var(--accent-red);
            color: #fff;
        }

        .btn-mute.active {
            background: var(--accent-red);
            color: #fff;
        }

        .btn-solo:hover {
            background: var(--accent-orange);
            color: #000;
        }

        .btn-solo.active {
            background: var(--accent-orange);
            color: #000;
        }

        .channel-middle {
            display: flex;
            gap: 4px;
            justify-content: center;
            align-items: flex-start;
            height: 120px;
            flex-shrink: 0;
        }

        .meter-section {
            display: flex;
            gap: 2px;
            flex-shrink: 0;
            height: 120px;
        }

        .meter-scale {
            position: relative;
            width: 14px;
            height: 120px;
            flex-shrink: 0;
        }

        .meter-label {
            position: absolute;
            right: 1px;
            font-size: 6px;
            color: var(--text-muted);
            transform: translateY(-50%);
            font-family: 'SF Mono', Monaco, 'Consolas', monospace;
            white-space: nowrap;
        }

        .meter-container {
            display: flex;
            gap: 2px;
            flex-shrink: 0;
            height: 120px;
        }

        .meter-canvas {
            border-radius: 2px;
            flex-shrink: 0;
        }

        .fader-section {
            display: flex;
            flex-direction: column;
            align-items: center;
            height: 120px;
            flex-shrink: 0;
        }

        /* Vertical fader styling - matches meter height */
        .fader {
            width: 24px;
            height: 120px;
            flex-shrink: 0;
            -webkit-appearance: slider-vertical;
            appearance: slider-vertical;
            writing-mode: vertical-lr;
            direction: rtl;
            background: transparent;
            cursor: pointer;
            outline: none;
        }

        .fader::-webkit-slider-runnable-track {
            width: 6px;
            height: 100%;
            background: linear-gradient(to top, #2a2a2a, #383838, #2a2a2a);
            border-radius: 3px;
        }

        .fader::-webkit-slider-thumb {
            -webkit-appearance: none;
            appearance: none;
            width: 16px;
            height: 10px;
            background: linear-gradient(to bottom, #888, #555);
            border: 1px solid #666;
            border-radius: 2px;
            cursor: pointer;
            margin-left: -5px;
        }

        .fader::-webkit-slider-thumb:hover {
            background: linear-gradient(to bottom, #999, #666);
        }

        .channel-bottom {
            margin-top: auto;
            padding-top: 6px;
            display: flex;
            flex-direction: column;
            gap: 4px;
            align-items: center;
        }

        .fader-value {
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 2px;
        }

        .fader-value-input {
            width: 32px;
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            border-radius: 2px;
            color: var(--accent-green);
            font-size: 9px;
            font-family: 'SF Mono', Monaco, 'Consolas', monospace;
            text-align: right;
            padding: 2px 3px;
        }

        .fader-value-input:focus {
            border-color: var(--accent-green);
            outline: none;
        }

        .fader-unit {
            font-size: 8px;
            color: var(--text-muted);
        }

        .pan-container {
            display: flex;
            align-items: center;
            gap: 3px;
            justify-content: center;
        }

        .pan-label {
            font-size: 7px;
            color: var(--text-muted);
        }

        .pan-knob {
            width: 40px;
            height: 6px;
            -webkit-appearance: none;
            background: var(--bg-tertiary);
            border-radius: 3px;
            cursor: pointer;
        }

        .pan-knob::-webkit-slider-thumb {
            -webkit-appearance: none;
            width: 8px;
            height: 10px;
            background: var(--text-secondary);
            border-radius: 2px;
            cursor: pointer;
        }

        /* Firefox support for vertical sliders */
        @supports (not (-webkit-appearance: slider-vertical)) {
            .fader {
                transform: rotate(-90deg);
                transform-origin: center;
                width: 120px;
                height: 24px;
            }
        }
    </style>
</head>
<body>
    ${content}
    <script>
        const vscode = acquireVsCodeApi();

        function sendMessage(message) {
            vscode.postMessage(message);
        }

        // =====================================================================
        // Fader Control Functions
        // =====================================================================

        function ampToDb(amp) {
            if (amp <= 0) return -Infinity;
            return 20 * Math.log10(amp);
        }

        function dbToAmp(db) {
            return Math.pow(10, db / 20);
        }

        function handleFaderInput(event, path) {
            event.stopPropagation();
            const fader = event.target;
            let value = parseFloat(fader.value);

            if (event.shiftKey) {
                const prevValue = parseFloat(fader.dataset.prevValue || value);
                value = prevValue + (value - prevValue) * 0.1;
                fader.value = value;
            }
            fader.dataset.prevValue = value;

            const dbInput = document.querySelector('.fader-value-input[data-path="' + path + '"]');
            if (dbInput) {
                dbInput.value = ampToDb(value).toFixed(1);
            }

            sendMessage({command:'setAmp', path: path, value: value});
        }

        function resetFader(event, path) {
            event.stopPropagation();
            event.preventDefault();
            const fader = event.target;
            fader.value = 1.0;
            fader.dataset.prevValue = 1.0;

            const dbInput = document.querySelector('.fader-value-input[data-path="' + path + '"]');
            if (dbInput) {
                dbInput.value = '0.0';
            }

            sendMessage({command:'setAmp', path: path, value: 1.0});
        }

        function handleDbInput(event, path) {
            if (event.key === 'Enter') {
                event.preventDefault();
                commitDbValue(event.target, path);
                event.target.blur();
            } else if (event.key === 'Escape') {
                event.target.blur();
            }
        }

        function commitDbValue(input, path) {
            const db = parseFloat(input.value);
            if (!isNaN(db) && db >= -96 && db <= 12) {
                const amp = Math.min(1.5, Math.max(0, dbToAmp(db)));
                const fader = document.querySelector('.fader[data-path="' + path + '"]');
                if (fader) {
                    fader.value = amp;
                    fader.dataset.prevValue = amp;
                }
                input.value = ampToDb(amp).toFixed(1);
                sendMessage({command:'setAmp', path: path, value: amp});
            }
        }

        // =====================================================================
        // Stereo VU Meters
        // =====================================================================

        const meterData = {};
        const realMeterData = {};
        const meterDecay = 0.92;
        const meterAttack = 0.5;

        window.addEventListener('message', event => {
            const message = event.data;
            if (message.type === 'meters') {
                Object.assign(realMeterData, message.data);
            }
        });

        function initMeters() {
            const canvases = document.querySelectorAll('.meter-canvas');
            canvases.forEach(canvas => {
                const path = canvas.dataset.path;
                const channel = canvas.dataset.channel;
                const key = path + '_' + channel;
                meterData[key] = { level: 0, peak: 0, peakHold: 0 };
            });
            animateMeters();
        }

        function animateMeters() {
            const canvases = document.querySelectorAll('.meter-canvas');

            canvases.forEach(canvas => {
                const ctx = canvas.getContext('2d');
                const path = canvas.dataset.path;
                const channelSide = canvas.dataset.channel;
                const channelEl = canvas.closest('.channel');
                const isMuted = channelEl.classList.contains('muted');

                const key = path + '_' + channelSide;
                let data = meterData[key] || { level: 0, peak: 0, peakHold: 0 };

                const realData = realMeterData[path];
                if (realData && !isMuted) {
                    const targetLevel = Math.min(1.0, channelSide === 'left' ? realData.peak_left : realData.peak_right);
                    data.level = data.level + (targetLevel - data.level) * meterAttack;
                    if (data.level > data.peak) {
                        data.peak = data.level;
                        data.peakHold = 30;
                    }
                } else {
                    data.level *= meterDecay;
                }

                if (data.peakHold > 0) {
                    data.peakHold--;
                } else {
                    data.peak *= 0.98;
                }

                meterData[key] = data;

                const width = canvas.width;
                const height = canvas.height;
                ctx.clearRect(0, 0, width, height);

                ctx.fillStyle = '#1a1a1a';
                ctx.fillRect(0, 0, width, height);

                const segments = 32;
                const segmentHeight = height / segments;
                const gapHeight = 1;
                const filledSegments = Math.floor(data.level * segments);
                const peakSegment = Math.floor(data.peak * segments);

                for (let i = 0; i < segments; i++) {
                    const y = height - (i + 1) * segmentHeight + gapHeight / 2;
                    const segH = segmentHeight - gapHeight;

                    let color;
                    if (i >= segments - 2) {
                        color = '#d16969';
                    } else if (i >= segments - 4) {
                        color = '#d19a66';
                    } else if (i >= segments - 8) {
                        color = '#d4d46f';
                    } else {
                        color = '#9bbb59';
                    }

                    if (i < filledSegments) {
                        ctx.fillStyle = color;
                        ctx.fillRect(0, y, width, segH);
                    } else {
                        ctx.fillStyle = '#222';
                        ctx.fillRect(0, y, width, segH);
                    }

                    if (i === peakSegment && data.peak > 0.05) {
                        ctx.fillStyle = '#fff';
                        ctx.fillRect(0, y, width, 1);
                    }
                }

                ctx.strokeStyle = '#333';
                ctx.lineWidth = 1;
                ctx.strokeRect(0.5, 0.5, width - 1, height - 1);
            });

            requestAnimationFrame(animateMeters);
        }

        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', initMeters);
        } else {
            initMeters();
        }
    </script>
</body>
</html>`;
    }

    public dispose(): void {
        this._stopMeterPolling();
        for (const d of this._disposables) {
            d.dispose();
        }
    }
}
