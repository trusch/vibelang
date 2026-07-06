//! SFZ v2 preprocessor: `#include` and `#define`.
//!
//! Runs before tokenization. Semantics follow the de-facto SFZ v2 standard
//! (ARIA/sfizz):
//!
//! - `#include "relative/path.sfz"` splices the referenced file in place.
//!   Paths are resolved relative to the *including* file. Includes nest
//!   recursively up to [`MAX_INCLUDE_DEPTH`]; include cycles are detected
//!   and reported as errors.
//! - `#define $VAR value` registers a textual macro. Every later occurrence
//!   of `$VAR` (in opcodes, headers, values, and even `#include` paths) is
//!   replaced by `value`. Later definitions override earlier ones. Defines
//!   are global: a define made inside an included file is visible after the
//!   include point of the parent file.
//!
//! Substitution is purely textual and longest-name-first, so `$FOO` never
//! clobbers the prefix of `$FOOBAR`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::parser::error::Error;

type Result<T> = std::result::Result<T, Error>;

/// Maximum `#include` nesting depth before we assume something is wrong.
pub const MAX_INCLUDE_DEPTH: usize = 32;

/// Preprocess an SFZ file on disk, resolving `#include`s relative to the
/// file and applying `#define` substitution. Returns the flattened text.
pub fn preprocess_file(path: &Path) -> Result<String> {
    let mut pp = Preprocessor::new();
    let mut out = String::new();
    pp.process_file(path, &mut out)?;
    Ok(out)
}

/// Preprocess in-memory SFZ content.
///
/// `base_dir` is used to resolve `#include` directives; when it is `None`
/// an `#include` is an error (there is nothing to resolve against).
/// `#define` substitution works either way.
pub fn preprocess_str(content: &str, base_dir: Option<&Path>) -> Result<String> {
    let mut pp = Preprocessor::new();
    let mut out = String::new();
    pp.process_content(content, base_dir, &mut out)?;
    Ok(out)
}

/// Stateful preprocessor: carries defines and the include stack.
struct Preprocessor {
    /// Ordered `(name, value)` pairs; names are stored *with* the leading
    /// `$`. Redefinitions update in place so ordering stays stable.
    defines: Vec<(String, String)>,
    /// Canonicalized paths of files currently being included (cycle check).
    include_stack: Vec<PathBuf>,
}

impl Preprocessor {
    fn new() -> Self {
        Self {
            defines: Vec::new(),
            include_stack: Vec::new(),
        }
    }

    fn process_file(&mut self, path: &Path, out: &mut String) -> Result<()> {
        if self.include_stack.len() >= MAX_INCLUDE_DEPTH {
            return Err(Error::Parse(format!(
                "#include depth limit ({}) exceeded at {}",
                MAX_INCLUDE_DEPTH,
                path.display()
            )));
        }

        let canonical = fs::canonicalize(path).map_err(|_| Error::FileNotFound(path.into()))?;
        if self.include_stack.contains(&canonical) {
            let cycle: Vec<String> = self
                .include_stack
                .iter()
                .chain(std::iter::once(&canonical))
                .map(|p| p.display().to_string())
                .collect();
            return Err(Error::Parse(format!(
                "#include cycle detected: {}",
                cycle.join(" -> ")
            )));
        }

        let content = fs::read_to_string(&canonical)?;
        self.include_stack.push(canonical.clone());
        let result = self.process_content(&content, canonical.parent(), out);
        self.include_stack.pop();
        result
    }

    fn process_content(
        &mut self,
        content: &str,
        base_dir: Option<&Path>,
        out: &mut String,
    ) -> Result<()> {
        for (line_no, raw_line) in content.lines().enumerate() {
            let trimmed = raw_line.trim_start();

            if let Some(rest) = trimmed.strip_prefix("#define") {
                self.handle_define(rest, line_no + 1)?;
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("#include") {
                self.handle_include(rest, base_dir, line_no + 1, out)?;
                continue;
            }

            if trimmed.starts_with('#') {
                // Unknown directive — keep the parser from choking on it,
                // but don't silently swallow a typo'd #inlcude.
                return Err(Error::Parse(format!(
                    "unknown preprocessor directive at line {}: {}",
                    line_no + 1,
                    trimmed
                )));
            }

            out.push_str(&self.substitute(raw_line));
            out.push('\n');
        }
        Ok(())
    }

    fn handle_define(&mut self, rest: &str, line_no: usize) -> Result<()> {
        // Grammar: #define $NAME value-until-end-of-line
        let rest = rest.trim_start();
        if !rest.starts_with('$') {
            return Err(Error::Parse(format!(
                "malformed #define at line {}: expected `#define $NAME value`",
                line_no
            )));
        }
        let mut parts = rest.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("").trim();
        let value = parts.next().unwrap_or("").trim();
        if name.len() < 2 {
            return Err(Error::Parse(format!(
                "malformed #define at line {}: empty variable name",
                line_no
            )));
        }
        // Allow defines to reference earlier defines in their value.
        let value = self.substitute(value);

        if let Some(entry) = self.defines.iter_mut().find(|(n, _)| n == name) {
            entry.1 = value; // later definition overrides
        } else {
            self.defines.push((name.to_string(), value));
        }
        Ok(())
    }

    fn handle_include(
        &mut self,
        rest: &str,
        base_dir: Option<&Path>,
        line_no: usize,
        out: &mut String,
    ) -> Result<()> {
        // Grammar: #include "path" — variables in the path are substituted.
        let rest = self.substitute(rest.trim());
        let path_str = rest
            .strip_prefix('"')
            .and_then(|s| s.split('"').next())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                Error::Parse(format!(
                    "malformed #include at line {}: expected `#include \"path\"`",
                    line_no
                ))
            })?;

        let base_dir = base_dir.ok_or_else(|| {
            Error::Parse(format!(
                "#include at line {} cannot be resolved: no base directory \
                 (parsing from a string, not a file)",
                line_no
            ))
        })?;

        let normalized = crate::parser::path_utils::normalize_path(path_str);
        let include_path = {
            let p = Path::new(&normalized);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                base_dir.join(p)
            }
        };

        self.process_file(&include_path, out)
    }

    /// Replace all defined `$VAR`s in `line`, longest name first.
    fn substitute(&self, line: &str) -> String {
        if !line.contains('$') || self.defines.is_empty() {
            return line.to_string();
        }
        let mut names: Vec<usize> = (0..self.defines.len()).collect();
        names.sort_by_key(|&i| std::cmp::Reverse(self.defines[i].0.len()));
        let mut result = line.to_string();
        for i in names {
            let (name, value) = &self.defines[i];
            if result.contains(name.as_str()) {
                result = result.replace(name.as_str(), value);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_substitutes_in_later_lines() {
        let src = "#define $KEY 60\n<region>\nkey=$KEY\n";
        let out = preprocess_str(src, None).unwrap();
        assert!(out.contains("key=60"));
        assert!(!out.contains("#define"));
    }

    #[test]
    fn later_define_overrides() {
        let src = "#define $V 1\n#define $V 2\nvolume=$V\n";
        let out = preprocess_str(src, None).unwrap();
        assert!(out.contains("volume=2"));
    }

    #[test]
    fn longest_name_wins() {
        let src = "#define $AB 1\n#define $ABC 2\nx=$ABC y=$AB\n";
        let out = preprocess_str(src, None).unwrap();
        assert!(out.contains("x=2 y=1"), "got: {}", out);
    }

    #[test]
    fn include_without_base_dir_errors() {
        let src = "#include \"other.sfz\"\n";
        let err = preprocess_str(src, None).unwrap_err();
        assert!(err.to_string().contains("no base directory"));
    }

    #[test]
    fn malformed_define_errors() {
        let err = preprocess_str("#define NOVAR 1\n", None).unwrap_err();
        assert!(err.to_string().contains("#define"));
    }

    #[test]
    fn unknown_directive_errors() {
        let err = preprocess_str("#inlcude \"foo.sfz\"\n", None).unwrap_err();
        assert!(err.to_string().contains("unknown preprocessor directive"));
    }
}
