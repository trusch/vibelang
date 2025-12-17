"use strict";
/**
 * Type-safe message protocol for webview communication.
 *
 * All webview panels should use these types instead of loose { command: string } objects.
 * This provides compile-time type checking and autocomplete for message handling.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.isMixerMessage = isMixerMessage;
exports.isGoToSourceMessage = isGoToSourceMessage;
// =============================================================================
// Type Guards
// =============================================================================
function isMixerMessage(message) {
    if (typeof message !== 'object' || message === null)
        return false;
    const cmd = message.command;
    return ['setAmp', 'setPan', 'mute', 'solo', 'select'].includes(cmd ?? '');
}
function isGoToSourceMessage(message) {
    if (typeof message !== 'object' || message === null)
        return false;
    const m = message;
    return m.command === 'goToSource' && typeof m.file === 'string' && typeof m.line === 'number';
}
//# sourceMappingURL=webviewMessages.js.map