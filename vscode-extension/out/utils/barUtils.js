"use strict";
/**
 * Bar Utilities
 *
 * Shared bar preprocessing for pattern and melody parsers.
 * Handles bar separator normalization while preserving content within bars.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.splitIntoBars = splitIntoBars;
exports.normalizeBars = normalizeBars;
exports.countBars = countBars;
exports.parseBars = parseBars;
/**
 * Split input into bars with normalized bar separators.
 * - Collapses consecutive | (with whitespace between) into single separator
 * - Strips leading and trailing |
 * - Preserves content WITHIN bars (tokenizers handle internal whitespace)
 *
 * @example
 * splitIntoBars('x...|x...') // ['x...', 'x...']
 * splitIntoBars('x...|x...|') // ['x...', 'x...'] - trailing | stripped
 * splitIntoBars('|x...|x...') // ['x...', 'x...'] - leading | stripped
 * splitIntoBars('x...||x...') // ['x...', 'x...'] - consecutive || collapsed
 * splitIntoBars('C4 - - | E4 - -') // ['C4 - -', 'E4 - -'] - internal whitespace preserved
 */
function splitIntoBars(input) {
    return input
        .split('|')
        .map(s => s.trim()) // Trim whitespace at bar boundaries
        .filter(s => s.length > 0); // Remove empty bars
}
/**
 * Normalize a pattern/melody string's bar structure.
 * Returns the string with normalized bar separators.
 *
 * @example
 * normalizeBars('x...|x...|') // 'x...|x...'
 * normalizeBars('|x...|x...') // 'x...|x...'
 * normalizeBars('x...||x...') // 'x...|x...'
 */
function normalizeBars(input) {
    return splitIntoBars(input).join('|');
}
/**
 * Count the number of bars in a pattern/melody string.
 *
 * @example
 * countBars('') // 0
 * countBars('x...') // 1
 * countBars('x...|x...') // 2
 * countBars('|x...|x...|') // 2
 */
function countBars(input) {
    return splitIntoBars(input).length;
}
/**
 * Parse bars from raw input (alias for splitIntoBars).
 */
function parseBars(input) {
    return splitIntoBars(input);
}
//# sourceMappingURL=barUtils.js.map