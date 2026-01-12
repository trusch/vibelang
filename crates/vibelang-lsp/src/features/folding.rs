//! Folding ranges provider for VibeLang.
//!
//! Provides code folding for:
//! - Group definitions (define_group)
//! - Synthdef bodies (define_synthdef)
//! - Effect bodies (define_fx)
//! - Closure blocks
//! - Multiline patterns/melodies

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

/// Get folding ranges for a document.
pub fn get_folding_ranges(content: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    // Track brace/paren depths for folding
    let mut brace_stack: Vec<usize> = Vec::new();
    let mut paren_stack: Vec<usize> = Vec::new();

    // Track special constructs
    let mut in_multiline_string = false;
    let mut string_start_line = 0;

    for (line_num, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() {
            continue;
        }

        // Track multiline strings (backtick strings)
        let backtick_count = line.chars().filter(|c| *c == '`').count();
        if backtick_count % 2 == 1 {
            if !in_multiline_string {
                in_multiline_string = true;
                string_start_line = line_num;
            } else {
                in_multiline_string = false;
                if line_num > string_start_line {
                    ranges.push(FoldingRange {
                        start_line: string_start_line as u32,
                        start_character: None,
                        end_line: line_num as u32,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: Some("...".to_string()),
                    });
                }
            }
        }

        if in_multiline_string {
            continue;
        }

        // Check for comment blocks
        if trimmed.starts_with("//") {
            // Look for consecutive comment lines
            let mut end_line = line_num;
            for (i, next_line) in lines.iter().enumerate().skip(line_num + 1) {
                if next_line.trim().starts_with("//") {
                    end_line = i;
                } else {
                    break;
                }
            }
            if end_line > line_num + 1 {
                ranges.push(FoldingRange {
                    start_line: line_num as u32,
                    start_character: None,
                    end_line: end_line as u32,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Comment),
                    collapsed_text: Some("// ...".to_string()),
                });
            }
        }

        // Track braces for block folding
        for c in line.chars() {
            match c {
                '{' => brace_stack.push(line_num),
                '}' => {
                    if let Some(start) = brace_stack.pop() {
                        if line_num > start {
                            ranges.push(FoldingRange {
                                start_line: start as u32,
                                start_character: None,
                                end_line: line_num as u32,
                                end_character: None,
                                kind: Some(FoldingRangeKind::Region),
                                collapsed_text: Some("{ ... }".to_string()),
                            });
                        }
                    }
                }
                '(' => {
                    // Only track multiline parens (for method chains)
                    if line.ends_with('(') || (line.contains('(') && !line.contains(')')) {
                        paren_stack.push(line_num);
                    }
                }
                ')' => {
                    if let Some(start) = paren_stack.pop() {
                        if line_num > start {
                            ranges.push(FoldingRange {
                                start_line: start as u32,
                                start_character: None,
                                end_line: line_num as u32,
                                end_character: None,
                                kind: Some(FoldingRangeKind::Region),
                                collapsed_text: Some("(...)".to_string()),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        // Check for define_group blocks
        if trimmed.starts_with("define_group(") {
            // Find the end of the group (matching closing paren or brace)
            if let Some(end) = find_block_end(&lines, line_num) {
                if end > line_num {
                    ranges.push(FoldingRange {
                        start_line: line_num as u32,
                        start_character: None,
                        end_line: end as u32,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: Some("define_group(...) { ... }".to_string()),
                    });
                }
            }
        }

        // Check for define_synthdef blocks
        if trimmed.starts_with("define_synthdef(") {
            if let Some(end) = find_block_end(&lines, line_num) {
                if end > line_num {
                    ranges.push(FoldingRange {
                        start_line: line_num as u32,
                        start_character: None,
                        end_line: end as u32,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: Some("define_synthdef(...) { ... }".to_string()),
                    });
                }
            }
        }

        // Check for define_fx blocks
        if trimmed.starts_with("define_fx(") {
            if let Some(end) = find_block_end(&lines, line_num) {
                if end > line_num {
                    ranges.push(FoldingRange {
                        start_line: line_num as u32,
                        start_character: None,
                        end_line: end as u32,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: Some("define_fx(...) { ... }".to_string()),
                    });
                }
            }
        }

        // Check for import blocks (consecutive imports)
        if trimmed.starts_with("import ") {
            let mut end_line = line_num;
            for (i, next_line) in lines.iter().enumerate().skip(line_num + 1) {
                if next_line.trim().starts_with("import ") {
                    end_line = i;
                } else {
                    break;
                }
            }
            if end_line > line_num + 1 {
                ranges.push(FoldingRange {
                    start_line: line_num as u32,
                    start_character: None,
                    end_line: end_line as u32,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Imports),
                    collapsed_text: Some("imports...".to_string()),
                });
            }
        }
    }

    // Deduplicate ranges (keep the most specific ones)
    ranges.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then(b.end_line.cmp(&a.end_line))
    });

    // Remove duplicates with same start line
    let mut seen_starts = std::collections::HashSet::new();
    ranges.retain(|r| seen_starts.insert((r.start_line, r.end_line)));

    ranges
}

/// Find the end of a block starting at the given line.
fn find_block_end(lines: &[&str], start: usize) -> Option<usize> {
    let mut depth = 0;
    let mut found_open = false;

    for (i, line) in lines.iter().enumerate().skip(start) {
        for c in line.chars() {
            match c {
                '{' | '(' => {
                    depth += 1;
                    found_open = true;
                }
                '}' | ')' => {
                    depth -= 1;
                    if found_open && depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_folding_group() {
        let content = r#"
define_group("Drums", || {
    let kick = voice("kick");
    let snare = voice("snare");
});
"#;
        let ranges = get_folding_ranges(content);
        assert!(!ranges.is_empty());
    }

    #[test]
    fn test_folding_synthdef() {
        let content = r#"
define_synthdef("my_synth")
    .param("freq", 440)
    .body(|freq| {
        let sig = sin_osc_ar(freq);
        sig * 0.5
    });
"#;
        let ranges = get_folding_ranges(content);
        assert!(!ranges.is_empty());
    }

    #[test]
    fn test_folding_imports() {
        let content = r#"
import "stdlib/drums/kicks.vibe";
import "stdlib/drums/snares.vibe";
import "stdlib/drums/hihats.vibe";
import "stdlib/bass/sub.vibe";

let kick = voice("kick");
"#;
        let ranges = get_folding_ranges(content);
        let import_range = ranges
            .iter()
            .find(|r| r.kind == Some(FoldingRangeKind::Imports));
        assert!(import_range.is_some());
    }

    #[test]
    fn test_folding_comments() {
        let content = r#"
// This is a multi-line
// comment block that
// should be foldable
let x = 42;
"#;
        let ranges = get_folding_ranges(content);
        let comment_range = ranges
            .iter()
            .find(|r| r.kind == Some(FoldingRangeKind::Comment));
        assert!(comment_range.is_some());
    }
}
