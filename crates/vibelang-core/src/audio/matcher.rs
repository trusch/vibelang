//! Port name matching with fuzzy/glob support.

/// Port name matcher with support for various matching strategies.
pub struct PortMatcher;

impl PortMatcher {
    /// Check if a port name matches a pattern.
    ///
    /// Supports:
    /// - Exact match (case-insensitive)
    /// - Glob patterns with `*` wildcard
    /// - Substring match if no wildcards present
    ///
    /// # Examples
    ///
    /// ```
    /// use vibelang_core::audio::PortMatcher;
    ///
    /// // Exact match (case-insensitive)
    /// assert!(PortMatcher::matches("Vibelang:out_1", "vibelang:out_1"));
    ///
    /// // Glob patterns
    /// assert!(PortMatcher::matches("UMC404HD 192k Line A:playback_FL", "UMC404*:playback_FL"));
    /// assert!(PortMatcher::matches("Headphones:playback_FL", "*Headphones*"));
    ///
    /// // Substring match
    /// assert!(PortMatcher::matches("UMC404HD 192k Line A:playback_FL", "UMC404"));
    /// ```
    pub fn matches(port_name: &str, pattern: &str) -> bool {
        let port_lower = port_name.to_lowercase();
        let pattern_lower = pattern.to_lowercase();

        // Exact match (case-insensitive)
        if port_lower == pattern_lower {
            return true;
        }

        // Glob pattern matching
        if pattern.contains('*') {
            return Self::glob_match(&port_lower, &pattern_lower);
        }

        // Substring match
        port_lower.contains(&pattern_lower)
    }

    /// Simple glob matching with `*` wildcard.
    fn glob_match(text: &str, pattern: &str) -> bool {
        let parts: Vec<&str> = pattern.split('*').collect();

        if parts.is_empty() {
            return true;
        }

        let mut pos = 0;
        let mut first = true;

        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }

            if let Some(found_pos) = text[pos..].find(part) {
                // First part must match at the start if pattern doesn't start with *
                if first && !pattern.starts_with('*') && found_pos != 0 {
                    return false;
                }
                pos += found_pos + part.len();
                first = false;
            } else {
                return false;
            }

            // Last part must match at the end if pattern doesn't end with *
            if i == parts.len() - 1 && !pattern.ends_with('*')
                && !text.ends_with(part) {
                return false;
            }
        }

        true
    }

    /// Calculate a match score (higher is better match).
    /// Used for sorting suggestions.
    pub fn score(port_name: &str, pattern: &str) -> u32 {
        let port_lower = port_name.to_lowercase();
        let pattern_lower = pattern.to_lowercase();

        // Exact match gets highest score
        if port_lower == pattern_lower {
            return 1000;
        }

        // Starts with pattern
        if port_lower.starts_with(&pattern_lower) {
            return 500 + (100 - pattern.len().min(100)) as u32;
        }

        // Contains pattern
        if port_lower.contains(&pattern_lower) {
            return 200 + (100 - pattern.len().min(100)) as u32;
        }

        // Word-level matching (e.g., "UMC" matches "UMC404HD")
        let pattern_words: Vec<&str> = pattern_lower.split_whitespace().collect();
        let port_words: Vec<&str> = port_lower.split(|c: char| !c.is_alphanumeric()).collect();

        let mut word_matches = 0;
        for pw in &pattern_words {
            for portw in &port_words {
                if portw.contains(pw) {
                    word_matches += 1;
                    break;
                }
            }
        }

        if word_matches > 0 {
            return 100 + word_matches * 20;
        }

        // Levenshtein-like fuzzy match for typo tolerance
        let edit_distance = Self::levenshtein(&port_lower, &pattern_lower);
        if edit_distance <= 3 {
            return 50 - edit_distance as u32;
        }

        0
    }

    /// Find the best matching ports from a list.
    pub fn find_matches<'a>(ports: &'a [String], pattern: &str) -> Vec<(&'a str, u32)> {
        let mut matches: Vec<(&str, u32)> = ports
            .iter()
            .map(|p| (p.as_str(), Self::score(p, pattern)))
            .filter(|(_, score)| *score > 0)
            .collect();

        matches.sort_by(|a, b| b.1.cmp(&a.1));
        matches
    }

    /// Simple Levenshtein distance for typo tolerance.
    fn levenshtein(a: &str, b: &str) -> usize {
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();

        let m = a_chars.len();
        let n = b_chars.len();

        if m == 0 {
            return n;
        }
        if n == 0 {
            return m;
        }

        // Use two rows instead of full matrix for memory efficiency
        let mut prev = (0..=n).collect::<Vec<_>>();
        let mut curr = vec![0; n + 1];

        for i in 1..=m {
            curr[0] = i;
            for j in 1..=n {
                let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
                curr[j] = (prev[j] + 1)
                    .min(curr[j - 1] + 1)
                    .min(prev[j - 1] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }

        prev[n]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(PortMatcher::matches("Vibelang:out_1", "Vibelang:out_1"));
        assert!(PortMatcher::matches("Vibelang:out_1", "vibelang:out_1"));
    }

    #[test]
    fn test_glob_match() {
        assert!(PortMatcher::matches(
            "UMC404HD 192k Line A:playback_FL",
            "UMC404*:playback_FL"
        ));
        assert!(PortMatcher::matches(
            "UMC404HD 192k Line A:playback_FL",
            "*playback_FL"
        ));
        assert!(PortMatcher::matches("Headphones:playback_FL", "*Headphones*"));
        assert!(PortMatcher::matches(
            "UMC404HD 192k Input 1:capture_MONO",
            "UMC404*Input 1*"
        ));
    }

    #[test]
    fn test_substring_match() {
        assert!(PortMatcher::matches(
            "UMC404HD 192k Line A:playback_FL",
            "UMC404"
        ));
        assert!(PortMatcher::matches(
            "UMC404HD 192k Line A:playback_FL",
            "playback_FL"
        ));
    }

    #[test]
    fn test_no_match() {
        assert!(!PortMatcher::matches("Vibelang:out_1", "Focusrite"));
        assert!(!PortMatcher::matches(
            "UMC404HD 192k Line A:playback_FL",
            "Headphones"
        ));
    }

    #[test]
    fn test_score() {
        // Exact match scores highest
        assert!(PortMatcher::score("Vibelang:out_1", "Vibelang:out_1") > 900);

        // Starts with scores high
        assert!(PortMatcher::score("Vibelang:out_1", "Vibelang") > 400);

        // Contains scores medium
        assert!(PortMatcher::score("UMC404HD:playback_FL", "404") > 100);

        // No match scores 0
        assert_eq!(PortMatcher::score("Vibelang:out_1", "Focusrite"), 0);
    }
}
