/**
 * Bar Utilities
 *
 * Shared bar preprocessing for pattern and melody parsers.
 * Handles bar separator normalization while preserving content within bars.
 */

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
export function splitIntoBars(input: string): string[] {
    return input
        .split('|')
        .map(s => s.trim())           // Trim whitespace at bar boundaries
        .filter(s => s.length > 0);   // Remove empty bars
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
export function normalizeBars(input: string): string {
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
export function countBars(input: string): number {
    return splitIntoBars(input).length;
}

/**
 * Parse bars from raw input (alias for splitIntoBars).
 */
export function parseBars(input: string): string[] {
    return splitIntoBars(input);
}
