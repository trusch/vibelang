//! Filesystem extension for Rhai.
//!
//! Provides file system access functions for VibeLang scripts.
//!
//! # Available Functions
//!
//! - `read_file(path)` - Read file contents as string
//! - `write_file(path, content)` - Write string to file
//! - `append_file(path, content)` - Append string to file
//! - `file_exists(path)` - Check if file exists
//! - `is_dir(path)` - Check if path is a directory
//! - `is_file(path)` - Check if path is a file
//! - `list_dir(path)` - List directory contents
//! - `create_dir(path)` - Create a directory
//! - `remove_file(path)` - Remove a file
//! - `remove_dir(path)` - Remove an empty directory
//! - `copy_file(src, dst)` - Copy a file
//! - `rename_file(src, dst)` - Rename/move a file
//! - `file_size(path)` - Get file size in bytes
//! - `read_lines(path)` - Read file as array of lines
//! - `glob(pattern)` - Find files matching a glob pattern
//!
//! # Security
//!
//! This extension can be sandboxed to a specific directory by providing
//! a base path during registration. All paths will be resolved relative
//! to this base path.

use rhai::{Array, Dynamic, Engine, EvalAltResult};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static FS_BASE_PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Set the base path for filesystem operations.
pub fn set_base_path(path: Option<&str>) {
    FS_BASE_PATH.with(|bp| {
        *bp.borrow_mut() = path.map(PathBuf::from);
    });
}

/// Resolve a path, applying sandboxing if a base path is set.
fn resolve_path(path: &str) -> Result<PathBuf, Box<EvalAltResult>> {
    FS_BASE_PATH.with(|bp| {
        let bp = bp.borrow();
        let resolved = if let Some(base) = bp.as_ref() {
            let path = Path::new(path);

            // If the path is absolute, reject it in sandbox mode
            if path.is_absolute() {
                return Err(Box::new(EvalAltResult::ErrorRuntime(
                    format!(
                        "Absolute paths not allowed in sandbox mode: {}",
                        path.display()
                    )
                    .into(),
                    rhai::Position::NONE,
                )));
            }

            // Resolve and canonicalize
            let resolved = base.join(path);

            // Make sure resolved path is still under base
            // (prevent path traversal with ..)
            if let Ok(canonical) = resolved.canonicalize() {
                if let Ok(base_canonical) = base.canonicalize() {
                    if !canonical.starts_with(&base_canonical) {
                        return Err(Box::new(EvalAltResult::ErrorRuntime(
                            format!("Path traversal not allowed: {}", path.display()).into(),
                            rhai::Position::NONE,
                        )));
                    }
                }
            }

            resolved
        } else {
            PathBuf::from(path)
        };

        Ok(resolved)
    })
}

/// Register filesystem functions with the Rhai engine.
pub fn register(engine: &mut Engine, base_path: Option<&str>) {
    // Set the base path for sandboxing
    set_base_path(base_path);

    // File reading
    engine.register_fn("read_file", read_file);
    engine.register_fn("read_lines", read_lines);

    // File writing
    engine.register_fn("write_file", write_file);
    engine.register_fn("append_file", append_file);

    // File info
    engine.register_fn("file_exists", file_exists);
    engine.register_fn("is_dir", is_dir);
    engine.register_fn("is_file", is_file);
    engine.register_fn("file_size", file_size);

    // Directory operations
    engine.register_fn("list_dir", list_dir);
    engine.register_fn("create_dir", create_dir);
    engine.register_fn("create_dir_all", create_dir_all);
    engine.register_fn("remove_dir", remove_dir);

    // File operations
    engine.register_fn("remove_file", remove_file);
    engine.register_fn("copy_file", copy_file);
    engine.register_fn("rename_file", rename_file);

    // Glob pattern matching
    engine.register_fn("glob", glob_files);

    // Path utilities
    engine.register_fn("path_join", path_join);
    engine.register_fn("path_parent", path_parent);
    engine.register_fn("path_filename", path_filename);
    engine.register_fn("path_extension", path_extension);
    engine.register_fn("path_stem", path_stem);
}

// ============================================================================
// File Reading
// ============================================================================

/// Read entire file contents as a string.
pub fn read_file(path: &str) -> Result<String, Box<EvalAltResult>> {
    let path = resolve_path(path)?;
    std::fs::read_to_string(&path).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to read file '{}': {}", path.display(), e).into(),
            rhai::Position::NONE,
        ))
    })
}

/// Read file as array of lines.
pub fn read_lines(path: &str) -> Result<Array, Box<EvalAltResult>> {
    let content = read_file(path)?;
    Ok(content
        .lines()
        .map(|line| Dynamic::from(line.to_string()))
        .collect())
}

// ============================================================================
// File Writing
// ============================================================================

/// Write content to a file (overwrites if exists).
pub fn write_file(path: &str, content: &str) -> Result<(), Box<EvalAltResult>> {
    let path = resolve_path(path)?;
    std::fs::write(&path, content).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to write file '{}': {}", path.display(), e).into(),
            rhai::Position::NONE,
        ))
    })
}

/// Append content to a file.
pub fn append_file(path: &str, content: &str) -> Result<(), Box<EvalAltResult>> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let path = resolve_path(path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| {
            Box::new(EvalAltResult::ErrorRuntime(
                format!(
                    "Failed to open file '{}' for appending: {}",
                    path.display(),
                    e
                )
                .into(),
                rhai::Position::NONE,
            ))
        })?;

    file.write_all(content.as_bytes()).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to append to file '{}': {}", path.display(), e).into(),
            rhai::Position::NONE,
        ))
    })
}

// ============================================================================
// File Info
// ============================================================================

/// Check if a file or directory exists.
pub fn file_exists(path: &str) -> bool {
    resolve_path(path).map(|p| p.exists()).unwrap_or(false)
}

/// Check if path is a directory.
pub fn is_dir(path: &str) -> bool {
    resolve_path(path).map(|p| p.is_dir()).unwrap_or(false)
}

/// Check if path is a file.
pub fn is_file(path: &str) -> bool {
    resolve_path(path).map(|p| p.is_file()).unwrap_or(false)
}

/// Get file size in bytes.
pub fn file_size(path: &str) -> Result<i64, Box<EvalAltResult>> {
    let path = resolve_path(path)?;
    std::fs::metadata(&path)
        .map(|m| m.len() as i64)
        .map_err(|e| {
            Box::new(EvalAltResult::ErrorRuntime(
                format!("Failed to get file size for '{}': {}", path.display(), e).into(),
                rhai::Position::NONE,
            ))
        })
}

// ============================================================================
// Directory Operations
// ============================================================================

/// List contents of a directory.
pub fn list_dir(path: &str) -> Result<Array, Box<EvalAltResult>> {
    let path = resolve_path(path)?;
    let entries = std::fs::read_dir(&path).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to read directory '{}': {}", path.display(), e).into(),
            rhai::Position::NONE,
        ))
    })?;

    let mut result = Array::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            result.push(Dynamic::from(name.to_string()));
        }
    }

    Ok(result)
}

/// Create a directory.
pub fn create_dir(path: &str) -> Result<(), Box<EvalAltResult>> {
    let path = resolve_path(path)?;
    std::fs::create_dir(&path).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to create directory '{}': {}", path.display(), e).into(),
            rhai::Position::NONE,
        ))
    })
}

/// Create a directory and all parent directories.
pub fn create_dir_all(path: &str) -> Result<(), Box<EvalAltResult>> {
    let path = resolve_path(path)?;
    std::fs::create_dir_all(&path).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to create directory '{}': {}", path.display(), e).into(),
            rhai::Position::NONE,
        ))
    })
}

/// Remove an empty directory.
pub fn remove_dir(path: &str) -> Result<(), Box<EvalAltResult>> {
    let path = resolve_path(path)?;
    std::fs::remove_dir(&path).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to remove directory '{}': {}", path.display(), e).into(),
            rhai::Position::NONE,
        ))
    })
}

// ============================================================================
// File Operations
// ============================================================================

/// Remove a file.
pub fn remove_file(path: &str) -> Result<(), Box<EvalAltResult>> {
    let path = resolve_path(path)?;
    std::fs::remove_file(&path).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to remove file '{}': {}", path.display(), e).into(),
            rhai::Position::NONE,
        ))
    })
}

/// Copy a file.
pub fn copy_file(src: &str, dst: &str) -> Result<i64, Box<EvalAltResult>> {
    let src = resolve_path(src)?;
    let dst = resolve_path(dst)?;
    std::fs::copy(&src, &dst).map(|n| n as i64).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!(
                "Failed to copy '{}' to '{}': {}",
                src.display(),
                dst.display(),
                e
            )
            .into(),
            rhai::Position::NONE,
        ))
    })
}

/// Rename or move a file.
pub fn rename_file(src: &str, dst: &str) -> Result<(), Box<EvalAltResult>> {
    let src = resolve_path(src)?;
    let dst = resolve_path(dst)?;
    std::fs::rename(&src, &dst).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!(
                "Failed to rename '{}' to '{}': {}",
                src.display(),
                dst.display(),
                e
            )
            .into(),
            rhai::Position::NONE,
        ))
    })
}

// ============================================================================
// Glob Pattern Matching
// ============================================================================

/// Find files matching a glob pattern.
///
/// Note: This is a simple glob implementation supporting:
/// - `*` matches any sequence of characters (not including path separators)
/// - `**` matches any sequence including path separators
/// - `?` matches a single character
pub fn glob_files(pattern: &str) -> Result<Array, Box<EvalAltResult>> {
    // For now, use a simple recursive directory walk with pattern matching
    // In a production environment, consider using the `glob` crate

    let base = FS_BASE_PATH
        .with(|bp| bp.borrow().clone())
        .unwrap_or_else(|| PathBuf::from("."));
    let mut results = Array::new();

    fn walk_dir(
        dir: &Path,
        pattern: &str,
        results: &mut Array,
        base: &Path,
    ) -> std::io::Result<()> {
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                // Get path relative to base for matching
                let rel_path = path.strip_prefix(base).unwrap_or(&path);
                let rel_str = rel_path.to_string_lossy();

                if matches_glob(pattern, &rel_str) {
                    results.push(Dynamic::from(rel_str.to_string()));
                }

                if path.is_dir() {
                    walk_dir(&path, pattern, results, base)?;
                }
            }
        }
        Ok(())
    }

    walk_dir(&base, pattern, &mut results, &base).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Glob error: {}", e).into(),
            rhai::Position::NONE,
        ))
    })?;

    Ok(results)
}

/// Simple glob pattern matcher.
fn matches_glob(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    fn matches(pattern: &[char], text: &[char]) -> bool {
        match (pattern.first(), text.first()) {
            (None, None) => true,
            (Some('*'), _) => {
                if pattern.get(1) == Some(&'*') {
                    // ** matches everything including path separators
                    matches(&pattern[2..], text)
                        || (!text.is_empty() && matches(pattern, &text[1..]))
                } else {
                    // * matches everything except path separators
                    matches(&pattern[1..], text)
                        || (!text.is_empty()
                            && text[0] != '/'
                            && text[0] != '\\'
                            && matches(pattern, &text[1..]))
                }
            }
            (Some('?'), Some(c)) if *c != '/' && *c != '\\' => matches(&pattern[1..], &text[1..]),
            (Some(p), Some(t)) if p == t => matches(&pattern[1..], &text[1..]),
            _ => false,
        }
    }

    matches(&pattern_chars, &text_chars)
}

// ============================================================================
// Path Utilities
// ============================================================================

/// Join path components.
pub fn path_join(base: &str, path: &str) -> String {
    Path::new(base).join(path).to_string_lossy().to_string()
}

/// Get parent directory of a path.
pub fn path_parent(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Get filename from path.
pub fn path_filename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Get file extension.
pub fn path_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Get file stem (filename without extension).
pub fn path_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_matching() {
        assert!(matches_glob("*.txt", "foo.txt"));
        assert!(matches_glob("*.txt", "bar.txt"));
        assert!(!matches_glob("*.txt", "foo.rs"));
        assert!(matches_glob("foo?", "fooX"));
        assert!(!matches_glob("foo?", "foo"));
        assert!(matches_glob("**/*.rs", "src/foo.rs"));
        assert!(matches_glob("src/**", "src/a/b/c"));
    }

    #[test]
    fn test_path_utilities() {
        assert_eq!(path_join("foo", "bar"), "foo/bar");
        assert_eq!(path_filename("foo/bar.txt"), "bar.txt");
        assert_eq!(path_extension("foo/bar.txt"), "txt");
        assert_eq!(path_stem("foo/bar.txt"), "bar");
        assert_eq!(path_parent("foo/bar/baz.txt"), "foo/bar");
    }
}
