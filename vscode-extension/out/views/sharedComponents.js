"use strict";
/**
 * Shared Webview Components
 *
 * Reusable HTML, CSS, and JS components for Pattern Editor and Melody Editor.
 * Professional styling with smooth animations, tooltips, and great UX.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.getEditorBaseStyles = getEditorBaseStyles;
exports.getRecordingPanelStyles = getRecordingPanelStyles;
exports.getCodePanelStyles = getCodePanelStyles;
exports.getTimingControlStyles = getTimingControlStyles;
exports.renderKeyAssignmentPanel = renderKeyAssignmentPanel;
exports.renderCodePanel = renderCodePanel;
exports.renderTimingSlider = renderTimingSlider;
exports.renderRecordButton = renderRecordButton;
exports.renderHelpTip = renderHelpTip;
exports.renderToastContainer = renderToastContainer;
exports.getTransportUtilsScript = getTransportUtilsScript;
exports.getNumpadUtilsScript = getNumpadUtilsScript;
exports.getRecordButtonScript = getRecordButtonScript;
exports.getToastScript = getToastScript;
exports.getCollapsiblePanelScript = getCollapsiblePanelScript;
exports.getColorForIndex = getColorForIndex;
exports.midiToNoteName = midiToNoteName;
exports.isBlackKey = isBlackKey;
// ============================================================================
// CSS Styles
// ============================================================================
/**
 * Shared CSS for editor panels - Professional dark theme
 */
function getEditorBaseStyles() {
    return `
        :root {
            /* Background colors - using VSCode theme variables */
            --bg-primary: var(--vscode-editor-background);
            --bg-secondary: var(--vscode-sideBar-background, var(--vscode-editor-background));
            --bg-tertiary: var(--vscode-editorWidget-background, var(--vscode-sideBar-background));
            --bg-elevated: var(--vscode-dropdown-background, var(--vscode-input-background));
            --bg-lane: var(--vscode-list-hoverBackground, var(--vscode-editor-background));

            /* Text colors - using VSCode theme variables */
            --text-primary: var(--vscode-editor-foreground);
            --text-secondary: var(--vscode-descriptionForeground, var(--vscode-foreground));
            --text-muted: var(--vscode-disabledForeground, var(--vscode-descriptionForeground));
            --text-link: var(--vscode-textLink-foreground);

            /* Accent colors - using VSCode theme variables where possible */
            --accent-green: var(--vscode-charts-green, var(--vscode-terminal-ansiGreen, #3fb950));
            --accent-green-dim: color-mix(in srgb, var(--accent-green) 15%, transparent);
            --accent-orange: var(--vscode-charts-orange, var(--vscode-terminal-ansiYellow, #d29922));
            --accent-orange-dim: color-mix(in srgb, var(--accent-orange) 15%, transparent);
            --accent-blue: var(--vscode-textLink-foreground, var(--vscode-charts-blue, #58a6ff));
            --accent-blue-dim: color-mix(in srgb, var(--accent-blue) 15%, transparent);
            --accent-purple: var(--vscode-charts-purple, var(--vscode-terminal-ansiMagenta, #a371f7));
            --accent-purple-dim: color-mix(in srgb, var(--accent-purple) 15%, transparent);
            --accent-red: var(--vscode-errorForeground, var(--vscode-charts-red, #f85149));
            --accent-red-dim: color-mix(in srgb, var(--accent-red) 15%, transparent);
            --accent-cyan: var(--vscode-charts-cyan, var(--vscode-terminal-ansiCyan, #39c5cf));
            --accent-pink: var(--vscode-terminal-ansiMagenta, #db61a2);

            /* UI colors - using VSCode theme variables */
            --border: var(--vscode-panel-border, var(--vscode-widget-border, var(--vscode-editorWidget-border)));
            --border-muted: var(--vscode-editorGroup-border, var(--border));
            --shadow: var(--vscode-widget-shadow, rgba(0, 0, 0, 0.4));
            --playhead: var(--vscode-errorForeground, #f85149);
            --beat-line: color-mix(in srgb, var(--border) 60%, transparent);
            --bar-line: color-mix(in srgb, var(--text-secondary) 40%, transparent);

            /* Transitions */
            --transition-fast: 0.1s ease;
            --transition-normal: 0.2s ease;
            --transition-slow: 0.3s ease;
        }

        * { box-sizing: border-box; margin: 0; padding: 0; }

        body {
            font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', Helvetica, Arial, sans-serif);
            background: var(--bg-primary);
            color: var(--text-primary);
            font-size: var(--vscode-font-size, 13px);
            line-height: 1.5;
            overflow: hidden;
            height: 100vh;
            display: flex;
            flex-direction: column;
        }

        /* ========== Scrollbar ========== */
        ::-webkit-scrollbar {
            width: 8px;
            height: 8px;
        }
        ::-webkit-scrollbar-track {
            background: var(--vscode-scrollbarSlider-background, var(--bg-primary));
        }
        ::-webkit-scrollbar-thumb {
            background: var(--vscode-scrollbarSlider-background, var(--bg-elevated));
            border-radius: 4px;
        }
        ::-webkit-scrollbar-thumb:hover {
            background: var(--vscode-scrollbarSlider-hoverBackground, var(--text-muted));
        }

        /* ========== Toolbar ========== */
        .toolbar {
            display: flex;
            align-items: center;
            gap: 10px;
            padding: 6px 8px;
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border);
            flex-wrap: wrap;
        }

        .toolbar-group {
            display: flex;
            align-items: center;
            gap: 6px;
        }

        .toolbar-divider {
            width: 1px;
            height: 24px;
            background: var(--border);
            margin: 0 4px;
        }

        .toolbar-label {
            font-size: 12px;
            font-weight: 500;
            color: var(--text-secondary);
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        /* ========== Form Controls ========== */
        select, input[type="text"], input[type="number"] {
            padding: 4px 8px;
            border: 1px solid var(--vscode-input-border, var(--border));
            border-radius: 4px;
            background: var(--vscode-input-background);
            color: var(--vscode-input-foreground);
            font-size: var(--vscode-font-size, 12px);
            font-family: inherit;
            transition: all var(--transition-fast);
            cursor: pointer;
        }

        /* Fix dropdown option styling for dark themes */
        select option {
            background: var(--vscode-dropdown-background, var(--vscode-input-background));
            color: var(--vscode-dropdown-foreground, var(--vscode-input-foreground));
        }

        select:hover, input:hover {
            border-color: var(--vscode-focusBorder, var(--accent-blue));
        }

        select:focus, input:focus {
            outline: none;
            border-color: var(--vscode-focusBorder, var(--accent-blue));
            box-shadow: 0 0 0 1px var(--vscode-focusBorder, var(--accent-blue));
        }

        /* ========== Buttons ========== */
        .btn {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            gap: 6px;
            padding: 4px 10px;
            border: 1px solid var(--vscode-button-border, transparent);
            border-radius: 4px;
            background: var(--vscode-button-secondaryBackground, var(--bg-tertiary));
            color: var(--vscode-button-secondaryForeground, var(--text-primary));
            cursor: pointer;
            font-size: var(--vscode-font-size, 12px);
            font-family: inherit;
            font-weight: 500;
            transition: all var(--transition-fast);
            white-space: nowrap;
        }

        .btn:hover {
            background: var(--vscode-button-secondaryHoverBackground, var(--bg-elevated));
        }

        .btn:active {
            background: var(--vscode-button-secondaryBackground, var(--bg-primary));
        }

        .btn:disabled {
            opacity: 0.5;
            cursor: not-allowed;
        }

        .btn.active {
            background: var(--vscode-button-background, var(--accent-green));
            color: var(--vscode-button-foreground, #fff);
            border-color: var(--vscode-button-background, var(--accent-green));
        }

        .btn-icon {
            width: 28px;
            height: 28px;
            padding: 0;
            font-size: 14px;
            border-radius: 4px;
        }

        .btn-small {
            padding: 2px 6px;
            font-size: 11px;
            border-radius: 3px;
        }

        .btn-primary {
            background: var(--vscode-button-background, var(--accent-blue));
            border-color: var(--vscode-button-background, var(--accent-blue));
            color: var(--vscode-button-foreground, #fff);
        }

        .btn-primary:hover {
            background: var(--vscode-button-hoverBackground, var(--accent-blue));
        }

        .btn-danger {
            color: var(--accent-red);
        }

        .btn-danger:hover {
            background: var(--accent-red-dim);
            border-color: var(--accent-red);
        }

        /* ========== Zoom Display ========== */
        .zoom-display {
            min-width: 42px;
            padding: 4px 8px;
            font-size: 11px;
            font-weight: 500;
            color: var(--text-secondary);
            text-align: center;
            background: var(--bg-primary);
            border: 1px solid var(--border);
            border-radius: 4px;
        }

        /* ========== Playhead ========== */
        .playhead {
            position: absolute;
            top: 0;
            bottom: 0;
            width: 2px;
            background: linear-gradient(180deg, var(--playhead) 0%, transparent 100%);
            pointer-events: none;
            z-index: 100;
            opacity: 0;
            transition: opacity var(--transition-fast);
        }

        .playhead::before {
            content: '';
            position: absolute;
            top: 0;
            left: -4px;
            width: 10px;
            height: 10px;
            background: var(--playhead);
            border-radius: 50%;
            box-shadow: 0 0 8px var(--playhead);
        }

        .playhead.visible {
            opacity: 1;
        }

        /* ========== Info Bar ========== */
        .info-bar {
            display: flex;
            align-items: center;
            gap: 16px;
            padding: 4px 8px;
            background: var(--vscode-statusBar-background, var(--bg-secondary));
            border-top: 1px solid var(--border);
            font-size: 11px;
        }

        .info-item {
            display: flex;
            align-items: center;
            gap: 6px;
        }

        .info-label {
            color: var(--text-muted);
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        .info-value {
            color: var(--text-secondary);
            font-family: var(--vscode-editor-font-family, 'SF Mono', 'Cascadia Code', Consolas, monospace);
            font-weight: 500;
        }

        /* ========== Empty State ========== */
        .empty-state {
            flex: 1;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            color: var(--text-secondary);
            text-align: center;
            padding: 40px;
            background: radial-gradient(ellipse at center, var(--bg-secondary) 0%, var(--bg-primary) 100%);
        }

        .empty-icon {
            font-size: 56px;
            margin-bottom: 20px;
            opacity: 0.4;
            animation: float 3s ease-in-out infinite;
        }

        @keyframes float {
            0%, 100% { transform: translateY(0); }
            50% { transform: translateY(-8px); }
        }

        .empty-state h3 {
            font-size: 18px;
            font-weight: 600;
            margin-bottom: 8px;
            color: var(--text-primary);
        }

        .empty-state p {
            max-width: 300px;
            line-height: 1.6;
        }

        /* ========== Tooltips ========== */
        [data-tooltip] {
            position: relative;
        }

        [data-tooltip]::after {
            content: attr(data-tooltip);
            position: absolute;
            bottom: 100%;
            left: 50%;
            transform: translateX(-50%) translateY(-4px);
            padding: 6px 10px;
            background: var(--bg-elevated);
            color: var(--text-primary);
            font-size: 11px;
            font-weight: 400;
            white-space: nowrap;
            border-radius: 6px;
            box-shadow: 0 4px 12px var(--shadow);
            opacity: 0;
            pointer-events: none;
            transition: all var(--transition-normal);
            z-index: 1000;
        }

        [data-tooltip]:hover::after {
            opacity: 1;
            transform: translateX(-50%) translateY(-8px);
        }

        [data-tooltip-bottom]::after {
            bottom: auto;
            top: 100%;
            transform: translateX(-50%) translateY(4px);
        }

        [data-tooltip-bottom]:hover::after {
            transform: translateX(-50%) translateY(8px);
        }

        /* ========== Keyboard Shortcut Hints ========== */
        .kbd {
            display: inline-block;
            padding: 2px 6px;
            font-family: var(--vscode-editor-font-family, 'SF Mono', Consolas, monospace);
            font-size: 10px;
            background: var(--vscode-keybindingLabel-background, var(--bg-primary));
            border: 1px solid var(--vscode-keybindingLabel-border, var(--border));
            border-radius: 3px;
            color: var(--vscode-keybindingLabel-foreground, var(--text-secondary));
            margin-left: 4px;
        }

        /* ========== Status Badge ========== */
        .badge {
            display: inline-flex;
            align-items: center;
            padding: 2px 8px;
            font-size: 10px;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 0.5px;
            border-radius: 10px;
            animation: badge-appear 0.2s ease;
        }

        @keyframes badge-appear {
            from { opacity: 0; transform: scale(0.8); }
            to { opacity: 1; transform: scale(1); }
        }

        .badge-modified {
            background: var(--accent-orange-dim);
            color: var(--accent-orange);
            border: 1px solid var(--accent-orange);
        }

        .badge-recording {
            background: var(--accent-red-dim);
            color: var(--accent-red);
            border: 1px solid var(--accent-red);
        }

        .badge-playing {
            background: var(--accent-green-dim);
            color: var(--accent-green);
            border: 1px solid var(--accent-green);
        }

        /* ========== Toast Notifications ========== */
        .toast {
            position: fixed;
            bottom: 80px;
            left: 50%;
            transform: translateX(-50%) translateY(20px);
            padding: 10px 20px;
            background: var(--bg-elevated);
            color: var(--text-primary);
            border-radius: 8px;
            box-shadow: 0 4px 20px var(--shadow);
            font-size: 13px;
            font-weight: 500;
            opacity: 0;
            pointer-events: none;
            transition: all var(--transition-normal);
            z-index: 9999;
        }

        .toast.visible {
            opacity: 1;
            transform: translateX(-50%) translateY(0);
        }

        .toast.success {
            border-left: 3px solid var(--accent-green);
        }

        .toast.error {
            border-left: 3px solid var(--accent-red);
        }

        /* ========== Help Tip ========== */
        .help-tip {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 8px 12px;
            background: var(--accent-blue-dim);
            border: 1px solid rgba(88, 166, 255, 0.3);
            border-radius: 6px;
            font-size: 11px;
            color: var(--accent-blue);
            margin: 8px 0;
        }

        .help-tip-icon {
            font-size: 14px;
            flex-shrink: 0;
        }

        .help-tip-close {
            margin-left: auto;
            cursor: pointer;
            opacity: 0.6;
        }

        .help-tip-close:hover {
            opacity: 1;
        }
    `;
}
/**
 * CSS for the recording panel (numpad key assignments) - Polished version
 */
function getRecordingPanelStyles() {
    return `
        /* ========== Record Button ========== */
        .record-btn {
            display: flex;
            align-items: center;
            gap: 6px;
            min-width: 70px;
        }

        .record-indicator {
            width: 10px;
            height: 10px;
            border-radius: 50%;
            background: var(--text-muted);
            transition: all var(--transition-fast);
        }

        .record-btn.active {
            background: var(--accent-red);
            color: white;
            border-color: var(--accent-red);
            animation: record-glow 1.5s ease-in-out infinite;
        }

        .record-btn.active .record-indicator {
            background: white;
            animation: record-pulse 1s ease-in-out infinite;
        }

        @keyframes record-glow {
            0%, 100% { box-shadow: 0 0 0 0 rgba(248, 81, 73, 0.4); }
            50% { box-shadow: 0 0 0 8px rgba(248, 81, 73, 0); }
        }

        @keyframes record-pulse {
            0%, 100% { opacity: 1; transform: scale(1); }
            50% { opacity: 0.5; transform: scale(0.7); }
        }

        /* ========== Key Assignment Panel ========== */
        .key-assignment-panel {
            background: linear-gradient(180deg, var(--bg-secondary) 0%, var(--bg-tertiary) 100%);
            border-bottom: 1px solid var(--border);
            overflow: hidden;
            transition: max-height var(--transition-slow);
        }

        .key-assignment-panel.collapsed {
            max-height: 44px;
        }

        .panel-header {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 8px;
        }

        .panel-icon {
            font-size: 16px;
            opacity: 0.7;
        }

        .panel-title {
            font-weight: 600;
            font-size: 12px;
            color: var(--text-secondary);
            flex: 1;
        }

        .panel-subtitle {
            font-size: 11px;
            color: var(--text-muted);
            margin-left: 8px;
            font-weight: 400;
        }

        .key-grid-container {
            padding: 0 8px 8px;
        }

        .key-assignment-panel.collapsed .key-grid-container {
            display: none;
        }

        .key-grid {
            display: flex;
            flex-direction: column;
            gap: 6px;
            max-width: 340px;
        }

        .key-row {
            display: flex;
            gap: 6px;
        }

        .key-slot {
            flex: 1;
            padding: 10px 8px;
            background: var(--bg-primary);
            border: 1px solid var(--border);
            border-radius: 8px;
            cursor: pointer;
            text-align: center;
            transition: all var(--transition-fast);
            min-width: 90px;
            position: relative;
            overflow: hidden;
        }

        .key-slot::before {
            content: '';
            position: absolute;
            inset: 0;
            background: linear-gradient(180deg, rgba(255,255,255,0.05) 0%, transparent 100%);
            opacity: 0;
            transition: opacity var(--transition-fast);
        }

        .key-slot:hover {
            border-color: var(--accent-blue);
            transform: translateY(-2px);
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
        }

        .key-slot:hover::before {
            opacity: 1;
        }

        .key-slot.flash {
            background: var(--accent-green);
            border-color: var(--accent-green);
            transform: scale(1.05);
            box-shadow: 0 0 20px var(--accent-green-dim);
        }

        .key-slot.flash .key-number,
        .key-slot.flash .key-label {
            color: #000;
        }

        .key-slot.held {
            background: var(--accent-orange);
            border-color: var(--accent-orange);
            box-shadow: 0 0 15px rgba(227, 160, 57, 0.5);
        }

        .key-slot.held .key-number,
        .key-slot.held .key-label {
            color: #000;
        }

        .key-number {
            font-weight: 700;
            font-size: 16px;
            color: var(--text-muted);
            display: block;
            margin-bottom: 4px;
            font-family: var(--vscode-editor-font-family, 'SF Mono', Consolas, monospace);
        }

        .key-label {
            color: var(--text-primary);
            font-size: 11px;
            font-weight: 500;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            display: block;
        }

        .key-label.empty {
            color: var(--text-muted);
            font-style: italic;
            font-weight: 400;
        }

        /* ========== Key Picker Dropdown ========== */
        .key-picker {
            position: fixed;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 8px;
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
            z-index: 1001;
            max-height: 240px;
            width: 160px;
            display: none;
            overflow-y: auto;
            animation: picker-appear 0.15s ease;
        }

        @keyframes picker-appear {
            from { opacity: 0; transform: translateY(-8px); }
            to { opacity: 1; transform: translateY(0); }
        }

        .key-picker.visible {
            display: block;
        }

        .key-picker-header {
            padding: 8px 12px;
            font-size: 10px;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 0.5px;
            color: var(--text-muted);
            background: var(--bg-tertiary);
            border-bottom: 1px solid var(--border);
            position: sticky;
            top: 0;
        }

        .key-picker-item {
            padding: 8px 12px;
            cursor: pointer;
            font-size: 12px;
            transition: all var(--transition-fast);
            border-left: 2px solid transparent;
        }

        .key-picker-item:hover {
            background: var(--bg-tertiary);
            border-left-color: var(--accent-blue);
        }

        .key-picker-item.selected {
            background: var(--accent-blue-dim);
            color: var(--accent-blue);
            border-left-color: var(--accent-blue);
        }

        /* ========== Recording Flash Effect ========== */
        .just-recorded {
            animation: item-flash 0.4s ease-out;
        }

        @keyframes item-flash {
            0% {
                transform: scale(1.15);
                box-shadow: 0 0 20px var(--accent-green);
                filter: brightness(1.3);
            }
            100% {
                transform: scale(1);
                box-shadow: none;
                filter: brightness(1);
            }
        }
    `;
}
/**
 * CSS for the code output panel - Polished version
 */
function getCodePanelStyles() {
    return `
        /* ========== Code Panel ========== */
        .code-panel {
            background: var(--bg-secondary);
            border-top: 1px solid var(--border);
            display: flex;
            flex-direction: column;
            transition: max-height var(--transition-slow);
        }

        .code-panel.collapsed {
            max-height: 44px;
        }

        .code-panel-header {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 8px;
        }

        .code-panel-icon {
            font-size: 14px;
            opacity: 0.7;
        }

        .code-panel-title {
            font-weight: 600;
            font-size: 12px;
            color: var(--text-secondary);
            flex: 1;
        }

        .code-panel-controls {
            display: flex;
            gap: 8px;
            align-items: center;
        }

        .code-output-container {
            padding: 0 8px 8px;
        }

        .code-panel.collapsed .code-output-container {
            display: none;
        }

        .code-output {
            overflow: auto;
            padding: 10px 12px;
            font-family: var(--vscode-editor-font-family, 'SF Mono', 'Cascadia Code', Consolas, monospace);
            font-size: var(--vscode-editor-font-size, 12px);
            line-height: 1.6;
            white-space: pre-wrap;
            color: var(--vscode-textPreformat-foreground, var(--accent-green));
            background: var(--vscode-textCodeBlock-background, var(--bg-primary));
            border: 1px solid var(--border);
            border-radius: 4px;
            margin: 0;
            min-height: 80px;
            max-height: 180px;
        }

        .code-output.empty {
            color: var(--text-muted);
            font-style: italic;
        }

        /* Syntax-like coloring for code output */
        .code-output .fn-name { color: var(--accent-blue); }
        .code-output .string { color: var(--accent-green); }
        .code-output .method { color: var(--accent-purple); }

        .btn-copy {
            background: var(--bg-tertiary);
        }

        .btn-copy:hover {
            background: var(--accent-blue-dim);
            border-color: var(--accent-blue);
            color: var(--accent-blue);
        }

        .btn-save {
            background: var(--accent-orange-dim);
            color: var(--accent-orange);
            border-color: var(--accent-orange);
        }

        .btn-save:hover {
            background: var(--accent-orange);
            color: #000;
        }

        .btn-save:disabled {
            background: var(--bg-tertiary);
            color: var(--text-muted);
            border-color: var(--border);
        }

        .btn-save.disabled-dynamic {
            background: var(--bg-tertiary);
            color: var(--text-muted);
            border-color: var(--border);
            cursor: not-allowed;
            opacity: 0.6;
        }

        .btn-save.disabled-dynamic:hover {
            background: var(--bg-tertiary);
            color: var(--text-muted);
        }

        /* ========== Toggle Button ========== */
        .btn-toggle {
            width: 24px;
            height: 24px;
            padding: 0;
            font-size: 10px;
            background: transparent;
            border: none;
            color: var(--text-muted);
            transition: transform var(--transition-fast);
        }

        .btn-toggle:hover {
            color: var(--text-primary);
            background: transparent;
            border: none;
            transform: scale(1.1);
        }

        .collapsed .btn-toggle {
            transform: rotate(-90deg);
        }
    `;
}
/**
 * CSS for timing controls
 */
function getTimingControlStyles() {
    return `
        /* ========== Timing Slider ========== */
        .timing-control {
            display: flex;
            align-items: center;
            gap: 8px;
        }

        .timing-slider {
            -webkit-appearance: none;
            width: 100px;
            height: 4px;
            background: var(--bg-elevated);
            border-radius: 2px;
            cursor: pointer;
        }

        .timing-slider::-webkit-slider-thumb {
            -webkit-appearance: none;
            width: 14px;
            height: 14px;
            background: var(--accent-blue);
            border-radius: 50%;
            cursor: pointer;
            transition: all var(--transition-fast);
            box-shadow: 0 2px 8px rgba(88, 166, 255, 0.3);
        }

        .timing-slider::-webkit-slider-thumb:hover {
            transform: scale(1.2);
            box-shadow: 0 2px 12px rgba(88, 166, 255, 0.5);
        }

        .timing-value {
            font-family: var(--vscode-editor-font-family, 'SF Mono', Consolas, monospace);
            font-size: 11px;
            color: var(--text-secondary);
            min-width: 50px;
            text-align: right;
        }
    `;
}
// ============================================================================
// HTML Components
// ============================================================================
/**
 * Generate HTML for the recording panel with numpad key assignments - Polished version
 */
function renderKeyAssignmentPanel(panelId, title, subtitle, showAutoAssign = true) {
    const autoAssignBtn = showAutoAssign
        ? `<button class="btn btn-small" id="${panelId}AutoAssign" data-tooltip="Auto-fill keys with available items">Auto-assign</button>`
        : '';
    return `
    <div class="key-assignment-panel" id="${panelId}Panel">
        <div class="panel-header">
            <span class="panel-icon">⌨️</span>
            <span class="panel-title">${title}<span class="panel-subtitle">${subtitle}</span></span>
            ${autoAssignBtn}
            <button class="btn btn-small btn-toggle" id="${panelId}Toggle" data-tooltip="Toggle panel">▼</button>
        </div>
        <div class="key-grid-container" id="${panelId}KeyGridContainer">
            <div class="key-grid" id="${panelId}KeyGrid">
                <!-- Numpad layout: 7-8-9 on top row -->
                <div class="key-row">
                    <div class="key-slot" data-key="6" data-tooltip="Press 7 to trigger"><span class="key-number">7</span><span class="key-label empty">click to assign</span></div>
                    <div class="key-slot" data-key="7" data-tooltip="Press 8 to trigger"><span class="key-number">8</span><span class="key-label empty">click to assign</span></div>
                    <div class="key-slot" data-key="8" data-tooltip="Press 9 to trigger"><span class="key-number">9</span><span class="key-label empty">click to assign</span></div>
                </div>
                <div class="key-row">
                    <div class="key-slot" data-key="3" data-tooltip="Press 4 to trigger"><span class="key-number">4</span><span class="key-label empty">click to assign</span></div>
                    <div class="key-slot" data-key="4" data-tooltip="Press 5 to trigger"><span class="key-number">5</span><span class="key-label empty">click to assign</span></div>
                    <div class="key-slot" data-key="5" data-tooltip="Press 6 to trigger"><span class="key-number">6</span><span class="key-label empty">click to assign</span></div>
                </div>
                <div class="key-row">
                    <div class="key-slot" data-key="0" data-tooltip="Press 1 to trigger"><span class="key-number">1</span><span class="key-label empty">click to assign</span></div>
                    <div class="key-slot" data-key="1" data-tooltip="Press 2 to trigger"><span class="key-number">2</span><span class="key-label empty">click to assign</span></div>
                    <div class="key-slot" data-key="2" data-tooltip="Press 3 to trigger"><span class="key-number">3</span><span class="key-label empty">click to assign</span></div>
                </div>
            </div>
        </div>
    </div>
    <div class="key-picker" id="${panelId}Picker"></div>
    `;
}
/**
 * Generate HTML for the code output panel - Polished version
 */
function renderCodePanel(panelId, title, placeholder) {
    return `
    <div class="code-panel" id="${panelId}Panel">
        <div class="code-panel-header">
            <span class="code-panel-icon">📝</span>
            <span class="code-panel-title">${title}</span>
            <div class="code-panel-controls">
                <button class="btn btn-small btn-copy" id="${panelId}Copy" data-tooltip="Copy code to clipboard">
                    <span>📋</span> Copy
                </button>
                <button class="btn btn-small btn-save" id="${panelId}Save" data-tooltip="Save changes back to source file">
                    <span>💾</span> Save to File
                </button>
                <button class="btn btn-small btn-toggle" id="${panelId}Toggle">▼</button>
            </div>
        </div>
        <div class="code-output-container">
            <pre class="code-output empty" id="${panelId}Output" data-placeholder="${placeholder}">${placeholder}</pre>
        </div>
    </div>
    `;
}
/**
 * Generate HTML for timing offset slider - Polished version
 */
function renderTimingSlider(defaultOffset = -50) {
    return `
    <div class="timing-control" data-tooltip="Adjust timing to compensate for latency. Negative = earlier.">
        <span class="toolbar-label">Offset</span>
        <input type="range" id="timingOffset" class="timing-slider"
               min="-200" max="100" value="${defaultOffset}">
        <span id="timingOffsetValue" class="timing-value">${defaultOffset}ms</span>
    </div>
    `;
}
/**
 * Generate HTML for record button - Polished version
 */
function renderRecordButton() {
    return `
    <button class="btn record-btn" id="recordBtn"
            data-tooltip="Enable recording mode. Numpad keys will record to the grid.">
        <span class="record-indicator"></span>
        <span>REC</span>
    </button>
    `;
}
/**
 * Generate HTML for a help tip box
 */
function renderHelpTip(id, icon, message) {
    return `
    <div class="help-tip" id="${id}">
        <span class="help-tip-icon">${icon}</span>
        <span>${message}</span>
        <span class="help-tip-close" onclick="this.parentElement.style.display='none'">✕</span>
    </div>
    `;
}
/**
 * Generate HTML for a toast notification container
 */
function renderToastContainer() {
    return `<div class="toast" id="toast"></div>`;
}
// ============================================================================
// JavaScript Components
// ============================================================================
/**
 * JavaScript utilities for transport/playhead handling.
 */
function getTransportUtilsScript() {
    return `
    // Transport state
    let transportState = {
        current_beat: 0,
        bpm: 120,
        running: false,
        lastUpdate: performance.now()
    };

    function getInterpolatedBeat(timingOffsetMs = 0) {
        const { current_beat, bpm, running, lastUpdate } = transportState;
        if (!running) return current_beat;

        const now = performance.now();
        const elapsedMs = now - lastUpdate;
        const adjustedElapsedMs = elapsedMs + timingOffsetMs;
        const elapsedBeats = (adjustedElapsedMs / 1000) * (bpm / 60);
        return current_beat + elapsedBeats;
    }

    function updateTransport(data) {
        transportState.current_beat = data.current_beat;
        transportState.bpm = data.bpm;
        transportState.running = data.running;
        transportState.lastUpdate = performance.now();
    }
    `;
}
/**
 * JavaScript utilities for numpad key handling.
 */
function getNumpadUtilsScript() {
    return `
    const heldKeys = new Set();
    const keyPressStartTimes = new Map(); // Track when each key was pressed (for held duration)
    const keyPressStartBeats = new Map(); // Track the beat when each key was pressed
    const keyPressData = new Map(); // Store keyIndex and assignment for release

    function numpadKeyToIndex(code) {
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

    function flashKeySlot(keyIndex, panelId) {
        const slot = document.querySelector('#' + panelId + 'KeyGrid .key-slot[data-key="' + keyIndex + '"]');
        if (slot) {
            slot.classList.add('flash');
            setTimeout(() => slot.classList.remove('flash'), 200);
        }
    }

    function setKeySlotHeld(keyIndex, panelId, held) {
        const slot = document.querySelector('#' + panelId + 'KeyGrid .key-slot[data-key="' + keyIndex + '"]');
        if (slot) {
            slot.classList.toggle('held', held);
        }
    }

    // Extended keydown handler with options for held-note support
    function handleNumpadKeydown(e, keyAssignments, onKeyPress, panelId, options = {}) {
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT') {
            return;
        }

        const keyIndex = numpadKeyToIndex(e.code);
        if (keyIndex === null) return;

        e.preventDefault();
        if (heldKeys.has(e.code)) return;
        heldKeys.add(e.code);

        const assignment = keyAssignments.get(keyIndex);
        if (!assignment) return;

        // Track press time and beat for held duration calculation
        keyPressStartTimes.set(e.code, performance.now());
        if (options.getCurrentBeat) {
            keyPressStartBeats.set(e.code, options.getCurrentBeat());
        }
        keyPressData.set(e.code, { keyIndex, assignment });

        // Visual feedback - show as held or flash
        if (options.showHeld) {
            setKeySlotHeld(keyIndex, panelId, true);
        } else {
            flashKeySlot(keyIndex, panelId);
        }

        if (onKeyPress) {
            onKeyPress(keyIndex, assignment, { isKeyDown: true });
        }
    }

    // Extended keyup handler with release callback
    function handleNumpadKeyupWithRelease(e, onKeyRelease, panelId, options = {}) {
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT') {
            return;
        }

        const keyIndex = numpadKeyToIndex(e.code);
        if (keyIndex === null) return;

        if (!heldKeys.has(e.code)) return;
        heldKeys.delete(e.code);

        const data = keyPressData.get(e.code);
        const startTime = keyPressStartTimes.get(e.code);
        const startBeat = keyPressStartBeats.get(e.code);

        keyPressData.delete(e.code);
        keyPressStartTimes.delete(e.code);
        keyPressStartBeats.delete(e.code);

        const heldDurationMs = startTime ? performance.now() - startTime : 0;
        const endBeat = options.getCurrentBeat ? options.getCurrentBeat() : null;

        // Clear held visual
        setKeySlotHeld(keyIndex, panelId, false);

        if (onKeyRelease && data) {
            onKeyRelease(data.keyIndex, data.assignment, {
                startBeat,
                endBeat,
                heldDurationMs,
            });
        }
    }

    // Simple keyup handler for backward compatibility
    function handleNumpadKeyup(e) {
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT') {
            return;
        }
        const keyIndex = numpadKeyToIndex(e.code);
        if (keyIndex !== null) {
            setKeySlotHeld(keyIndex, 'voices', false);
            setKeySlotHeld(keyIndex, 'notes', false);
        }
        heldKeys.delete(e.code);
    }
    `;
}
/**
 * JavaScript for record button handling.
 */
function getRecordButtonScript() {
    return `
    let isRecording = false;
    let timingOffsetMs = -50;

    function setupRecordButton() {
        const recordBtn = document.getElementById('recordBtn');
        if (recordBtn) {
            recordBtn.addEventListener('click', () => {
                isRecording = !isRecording;
                recordBtn.classList.toggle('active', isRecording);
                if (isRecording) {
                    showToast('Recording enabled - press numpad keys to record', 'success');
                }
            });
        }

        const timingSlider = document.getElementById('timingOffset');
        const timingValue = document.getElementById('timingOffsetValue');
        if (timingSlider && timingValue) {
            timingSlider.addEventListener('input', () => {
                timingOffsetMs = parseInt(timingSlider.value);
                timingValue.textContent = timingOffsetMs + 'ms';
            });
        }
    }
    `;
}
/**
 * JavaScript for toast notifications.
 */
function getToastScript() {
    return `
    let toastTimeout = null;

    function showToast(message, type = 'success') {
        const toast = document.getElementById('toast');
        if (!toast) return;

        if (toastTimeout) {
            clearTimeout(toastTimeout);
        }

        toast.textContent = message;
        toast.className = 'toast visible ' + type;

        toastTimeout = setTimeout(() => {
            toast.classList.remove('visible');
        }, 2500);
    }
    `;
}
/**
 * JavaScript for collapsible panels.
 */
function getCollapsiblePanelScript() {
    return `
    function setupCollapsiblePanel(panelId) {
        const toggle = document.getElementById(panelId + 'Toggle');
        const panel = document.getElementById(panelId + 'Panel');

        if (toggle && panel) {
            toggle.addEventListener('click', () => {
                panel.classList.toggle('collapsed');
            });
        }
    }
    `;
}
// ============================================================================
// Utility Functions
// ============================================================================
function getColorForIndex(index) {
    const colors = [
        '#3fb950', '#58a6ff', '#a371f7', '#d29922', '#f85149',
        '#39c5cf', '#db61a2', '#8b949e', '#7ee787', '#79c0ff'
    ];
    return colors[index % colors.length];
}
function midiToNoteName(midi) {
    const notes = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
    const octave = Math.floor(midi / 12) - 1;
    return notes[midi % 12] + octave;
}
function isBlackKey(midi) {
    const note = midi % 12;
    return [1, 3, 6, 8, 10].includes(note);
}
//# sourceMappingURL=sharedComponents.js.map