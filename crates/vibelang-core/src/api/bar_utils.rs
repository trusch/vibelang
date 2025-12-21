//! Bar preprocessing utilities for patterns and melodies.
//!
//! This module provides shared bar separator handling for both pattern and melody parsers.
//! It normalizes bar separators (`|`) while preserving content within bars.

/// Split input into bars with normalized bar separators.
///
/// This function handles:
/// - Collapsing consecutive `|` (with optional whitespace between) into single separator
/// - Stripping leading and trailing `|`
/// - Preserving whitespace WITHIN bars (tokenizers handle that)
///
/// # Examples
///
/// ```
/// use vibelang_core::api::split_into_bars;
///
/// // Simple bars
/// assert_eq!(split_into_bars("x...|x..."), vec!["x...", "x..."]);
///
/// // Trailing pipe is stripped
/// assert_eq!(split_into_bars("x...|x...|"), vec!["x...", "x..."]);
///
/// // Leading pipe is stripped
/// assert_eq!(split_into_bars("|x...|x..."), vec!["x...", "x..."]);
///
/// // Consecutive pipes are collapsed
/// assert_eq!(split_into_bars("x...||x..."), vec!["x...", "x..."]);
///
/// // Multiline with pipes
/// assert_eq!(split_into_bars("x...\n|x..."), vec!["x...", "x..."]);
///
/// // Whitespace within bars is preserved
/// assert_eq!(split_into_bars("C4 - - | E4 - -"), vec!["C4 - -", "E4 - -"]);
/// ```
pub fn split_into_bars(input: &str) -> Vec<String> {
    input
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Normalize a pattern/melody string's bar structure.
///
/// Returns the string with normalized bar separators:
/// - Collapses consecutive `|` into single `|`
/// - Strips leading and trailing `|`
/// - Preserves internal bar content (including internal whitespace)
///
/// # Examples
///
/// ```
/// use vibelang_core::api::normalize_bars;
///
/// assert_eq!(normalize_bars("x...|x...|"), "x...|x...");
/// assert_eq!(normalize_bars("|x...|x..."), "x...|x...");
/// assert_eq!(normalize_bars("x...||x..."), "x...|x...");
/// assert_eq!(normalize_bars("C4 - - | E4 - -"), "C4 - -|E4 - -");
/// ```
pub fn normalize_bars(input: &str) -> String {
    let bars = split_into_bars(input);
    bars.join("|")
}

/// Count the number of bars in a pattern/melody string.
///
/// # Examples
///
/// ```
/// use vibelang_core::api::count_bars;
///
/// assert_eq!(count_bars(""), 0);
/// assert_eq!(count_bars("x..."), 1);
/// assert_eq!(count_bars("x...|x..."), 2);
/// assert_eq!(count_bars("|x...|x...|"), 2);
/// assert_eq!(count_bars("x...||x..."), 2);
/// ```
pub fn count_bars(input: &str) -> usize {
    split_into_bars(input).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    // === split_into_bars tests ===

    #[test]
    fn test_split_simple_bars() {
        assert_eq!(split_into_bars("x...|x..."), vec!["x...", "x..."]);
    }

    #[test]
    fn test_split_trailing_pipe() {
        assert_eq!(split_into_bars("x...|x...|"), vec!["x...", "x..."]);
    }

    #[test]
    fn test_split_leading_pipe() {
        assert_eq!(split_into_bars("|x...|x..."), vec!["x...", "x..."]);
    }

    #[test]
    fn test_split_leading_and_trailing_pipe() {
        assert_eq!(split_into_bars("|x...|x...|"), vec!["x...", "x..."]);
    }

    #[test]
    fn test_split_consecutive_pipes() {
        assert_eq!(split_into_bars("x...||x..."), vec!["x...", "x..."]);
    }

    #[test]
    fn test_split_multiple_consecutive_pipes() {
        assert_eq!(split_into_bars("x...|||x..."), vec!["x...", "x..."]);
    }

    #[test]
    fn test_split_multiline_with_pipes() {
        assert_eq!(split_into_bars("x...\n|x..."), vec!["x...", "x..."]);
        assert_eq!(split_into_bars("|x...\n|x...|"), vec!["x...", "x..."]);
    }

    #[test]
    fn test_split_preserves_internal_whitespace() {
        // Whitespace WITHIN bars should be preserved
        assert_eq!(
            split_into_bars("x . . . | y . . ."),
            vec!["x . . .", "y . . ."]
        );
    }

    #[test]
    fn test_split_multiline_melody_format() {
        let input = r#"
            D3:m7 - - - | - - - - |
            A3:m7 - - - | - - - -
        "#;
        let bars = split_into_bars(input);
        assert_eq!(bars.len(), 4);
        assert_eq!(bars[0], "D3:m7 - - -");
        assert_eq!(bars[1], "- - - -");
        assert_eq!(bars[2], "A3:m7 - - -");
        assert_eq!(bars[3], "- - - -");
    }

    #[test]
    fn test_split_empty_input() {
        assert!(split_into_bars("").is_empty());
        assert!(split_into_bars("|").is_empty());
        assert!(split_into_bars("||").is_empty());
        assert!(split_into_bars("|||").is_empty());
        assert!(split_into_bars("   ").is_empty());
        assert!(split_into_bars("  |  |  ").is_empty());
    }

    #[test]
    fn test_split_single_bar() {
        assert_eq!(split_into_bars("x..."), vec!["x..."]);
        assert_eq!(split_into_bars("|x...|"), vec!["x..."]);
    }

    #[test]
    fn test_split_four_bars() {
        assert_eq!(split_into_bars("a|b|c|d"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_split_whitespace_only_bars() {
        // Bars with only whitespace should be filtered out
        assert_eq!(split_into_bars("x...|   |y..."), vec!["x...", "y..."]);
    }

    #[test]
    fn test_split_real_lofi_pattern() {
        // From examples/lofi_beat.vibe
        let pattern = "x... ..x. x... .... | x... ..x. ..x. ....";
        let bars = split_into_bars(pattern);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0], "x... ..x. x... ....");
        assert_eq!(bars[1], "x... ..x. ..x. ....");
    }

    #[test]
    fn test_split_compact_melody_notation() {
        // Compact notation without spaces - should preserve internal content
        assert_eq!(split_into_bars("D3:m7---|----"), vec!["D3:m7---", "----"]);
        assert_eq!(
            split_into_bars("C4..E4..|G4..B4.."),
            vec!["C4..E4..", "G4..B4.."]
        );
    }

    // === normalize_bars tests ===

    #[test]
    fn test_normalize_simple() {
        assert_eq!(normalize_bars("x...|x..."), "x...|x...");
    }

    #[test]
    fn test_normalize_with_trailing() {
        assert_eq!(normalize_bars("x...|x...|"), "x...|x...");
    }

    #[test]
    fn test_normalize_with_leading() {
        assert_eq!(normalize_bars("|x...|x..."), "x...|x...");
    }

    #[test]
    fn test_normalize_consecutive_pipes() {
        assert_eq!(normalize_bars("x...||x..."), "x...|x...");
    }

    #[test]
    fn test_normalize_preserves_internal_whitespace() {
        assert_eq!(normalize_bars("C4 - - | E4 - -"), "C4 - -|E4 - -");
    }

    #[test]
    fn test_normalize_empty() {
        assert_eq!(normalize_bars(""), "");
        assert_eq!(normalize_bars("|"), "");
        assert_eq!(normalize_bars("||"), "");
    }

    // === count_bars tests ===

    #[test]
    fn test_count_bars() {
        assert_eq!(count_bars(""), 0);
        assert_eq!(count_bars("x..."), 1);
        assert_eq!(count_bars("x...|x..."), 2);
        assert_eq!(count_bars("|x...|x...|"), 2);
        assert_eq!(count_bars("x...||x..."), 2);
        assert_eq!(count_bars("a|b|c|d"), 4);
    }

    #[test]
    fn test_count_bars_multiline() {
        let input = r#"
            D3:m7 - - - | - - - - |
            A3:m7 - - - | - - - -
        "#;
        assert_eq!(count_bars(input), 4);
    }
}
