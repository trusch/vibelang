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
use std::path::{Component, Path, PathBuf};

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

            if path.is_absolute() {
                return Err(boundary_error(
                    "extension.fs.absolute_path",
                    &path.to_string_lossy(),
                    "relative_sandbox_path",
                ));
            }

            let mut relative = PathBuf::new();
            for component in path.components() {
                match component {
                    Component::CurDir => {}
                    Component::Normal(component) => relative.push(component),
                    Component::ParentDir if relative.pop() => {}
                    Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                        return Err(boundary_error(
                            "extension.fs.path_traversal",
                            &path.to_string_lossy(),
                            "path_within_sandbox",
                        ));
                    }
                }
            }

            let base = base.canonicalize().map_err(|error| {
                boundary_error(
                    "extension.fs.sandbox_base",
                    &base.to_string_lossy(),
                    &format!("existing_sandbox_base error={error}"),
                )
            })?;
            let resolved = base.join(relative);

            // For new targets, canonicalize the nearest existing ancestor so
            // a symlinked parent cannot redirect creation outside the sandbox.
            let mut existing = resolved.as_path();
            while !existing.exists() {
                existing = existing.parent().ok_or_else(|| {
                    boundary_error(
                        "extension.fs.path_traversal",
                        &path.to_string_lossy(),
                        "path_within_sandbox",
                    )
                })?;
            }
            let canonical_existing = existing.canonicalize().map_err(|error| {
                boundary_error(
                    "extension.fs.path_resolution",
                    &path.to_string_lossy(),
                    &format!("resolvable_sandbox_ancestor error={error}"),
                )
            })?;
            if !canonical_existing.starts_with(&base) {
                return Err(boundary_error(
                    "extension.fs.path_traversal",
                    &path.to_string_lossy(),
                    "path_within_sandbox",
                ));
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

fn boundary_error(code: &str, token: &str, expected: &str) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "{code} span=0..{} expected={expected} token={token:?}",
            token.len()
        )
        .into(),
        rhai::Position::NONE,
    ))
}

fn io_boundary_error(
    code: &str,
    token: &str,
    expected: &str,
    error: impl std::fmt::Display,
) -> Box<EvalAltResult> {
    boundary_error(code, token, &format!("{expected} error={error}"))
}

/// Install the complete filesystem inventory with strict vibe-api 2 boundaries.
pub(crate) fn register_v2(engine: &mut Engine, base_path: Option<&str>) {
    register(engine, base_path);
    engine
        .register_fn("read_file", read_file_strict)
        .register_fn("read_lines", read_lines_strict)
        .register_fn("write_file", write_file_strict)
        .register_fn("append_file", append_file_strict)
        .register_fn("file_exists", |path: &str| {
            resolve_path(path).map(|path| path.exists())
        })
        .register_fn("is_dir", |path: &str| {
            resolve_path(path).map(|path| path.is_dir())
        })
        .register_fn("is_file", |path: &str| {
            resolve_path(path).map(|path| path.is_file())
        })
        .register_fn("file_size", file_size_strict)
        .register_fn("list_dir", list_dir_strict)
        .register_fn("create_dir", create_dir_strict)
        .register_fn("create_dir_all", create_dir_all_strict)
        .register_fn("remove_dir", remove_dir_strict)
        .register_fn("remove_file", remove_file_strict)
        .register_fn("copy_file", copy_file_strict)
        .register_fn("rename_file", rename_file_strict)
        .register_fn("glob", glob_files_strict)
        .register_fn("path_join", path_join_strict)
        .register_fn("path_parent", |path: &str| {
            path_component_strict(path, "parent")
        })
        .register_fn("path_filename", |path: &str| {
            path_component_strict(path, "filename")
        })
        .register_fn("path_extension", |path: &str| {
            path_component_strict(path, "extension")
        })
        .register_fn("path_stem", |path: &str| {
            path_component_strict(path, "stem")
        });
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

fn read_file_strict(path: &str) -> Result<String, Box<EvalAltResult>> {
    let resolved = resolve_path(path)?;
    std::fs::read_to_string(&resolved).map_err(|error| {
        io_boundary_error("extension.fs.read_file", path, "readable_utf8_file", error)
    })
}

fn read_lines_strict(path: &str) -> Result<Array, Box<EvalAltResult>> {
    Ok(read_file_strict(path)?
        .lines()
        .map(|line| Dynamic::from(line.to_owned()))
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

fn write_file_strict(path: &str, content: &str) -> Result<(), Box<EvalAltResult>> {
    let resolved = resolve_path(path)?;
    std::fs::write(&resolved, content)
        .map_err(|error| io_boundary_error("extension.fs.write_file", path, "writable_file", error))
}

fn append_file_strict(path: &str, content: &str) -> Result<(), Box<EvalAltResult>> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let resolved = resolve_path(path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&resolved)
        .map_err(|error| {
            io_boundary_error(
                "extension.fs.append_open",
                path,
                "writable_append_file",
                error,
            )
        })?;
    file.write_all(content.as_bytes()).map_err(|error| {
        io_boundary_error("extension.fs.append_write", path, "complete_append", error)
    })
}

// ============================================================================
// File Info
// ============================================================================

/// Check if a file or directory exists.
pub fn file_exists(path: &str) -> bool {
    resolve_path(path).map(|p| p.exists()).unwrap_or_else(|_| {
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=file_exists argument=path input={path:?} recovery=legacy_false effective_value=false replacement=use_valid_sandbox_path"
        );
        false
    })
}

/// Check if path is a directory.
pub fn is_dir(path: &str) -> bool {
    resolve_path(path).map(|p| p.is_dir()).unwrap_or_else(|_| {
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=is_dir argument=path input={path:?} recovery=legacy_false effective_value=false replacement=use_valid_sandbox_path"
        );
        false
    })
}

/// Check if path is a file.
pub fn is_file(path: &str) -> bool {
    resolve_path(path).map(|p| p.is_file()).unwrap_or_else(|_| {
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=is_file argument=path input={path:?} recovery=legacy_false effective_value=false replacement=use_valid_sandbox_path"
        );
        false
    })
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

fn file_size_strict(path: &str) -> Result<i64, Box<EvalAltResult>> {
    let resolved = resolve_path(path)?;
    let size = std::fs::metadata(&resolved)
        .map_err(|error| {
            io_boundary_error("extension.fs.file_size", path, "readable_metadata", error)
        })?
        .len();
    i64::try_from(size).map_err(|_| {
        boundary_error(
            "extension.fs.file_size_range",
            path,
            "file_size_representable_as_i64",
        )
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
    let mut skipped = false;
    for entry in entries {
        match entry {
            Ok(entry) => {
                if let Some(name) = entry.file_name().to_str() {
                    result.push(Dynamic::from(name.to_string()));
                } else {
                    skipped = true;
                }
            }
            Err(_) => {
                skipped = true;
            }
        }
    }
    if skipped {
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=list_dir argument=path input={} recovery=legacy_entry_drop effective_value=partial_listing replacement=use_readable_utf8_directory_entries",
            path.display()
        );
    }

    Ok(result)
}

fn list_dir_strict(path: &str) -> Result<Array, Box<EvalAltResult>> {
    let resolved = resolve_path(path)?;
    let entries = std::fs::read_dir(&resolved).map_err(|error| {
        boundary_error(
            "extension.fs.read_directory",
            path,
            &format!("readable_directory_error_{error}"),
        )
    })?;
    let mut result = Array::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            boundary_error(
                "extension.fs.directory_entry",
                path,
                &format!("readable_directory_entry_error_{error}"),
            )
        })?;
        let name = entry.file_name().into_string().map_err(|name| {
            boundary_error(
                "extension.fs.path_encoding",
                &name.to_string_lossy(),
                "utf8_filename",
            )
        })?;
        result.push(Dynamic::from(name));
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

fn create_dir_strict(path: &str) -> Result<(), Box<EvalAltResult>> {
    let resolved = resolve_path(path)?;
    std::fs::create_dir(&resolved)
        .map_err(|error| io_boundary_error("extension.fs.create_dir", path, "new_directory", error))
}

fn create_dir_all_strict(path: &str) -> Result<(), Box<EvalAltResult>> {
    let resolved = resolve_path(path)?;
    std::fs::create_dir_all(&resolved).map_err(|error| {
        io_boundary_error(
            "extension.fs.create_dir_all",
            path,
            "creatable_directory_tree",
            error,
        )
    })
}

fn remove_dir_strict(path: &str) -> Result<(), Box<EvalAltResult>> {
    let resolved = resolve_path(path)?;
    std::fs::remove_dir(&resolved).map_err(|error| {
        io_boundary_error(
            "extension.fs.remove_dir",
            path,
            "removable_empty_directory",
            error,
        )
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

fn remove_file_strict(path: &str) -> Result<(), Box<EvalAltResult>> {
    let resolved = resolve_path(path)?;
    std::fs::remove_file(&resolved).map_err(|error| {
        io_boundary_error("extension.fs.remove_file", path, "removable_file", error)
    })
}

fn copy_file_strict(src: &str, dst: &str) -> Result<i64, Box<EvalAltResult>> {
    let resolved_src = resolve_path(src)?;
    let resolved_dst = resolve_path(dst)?;
    let copied = std::fs::copy(&resolved_src, &resolved_dst).map_err(|error| {
        io_boundary_error(
            "extension.fs.copy_file",
            &format!("{src:?}->{dst:?}"),
            "readable_source_and_writable_destination",
            error,
        )
    })?;
    i64::try_from(copied).map_err(|_| {
        boundary_error(
            "extension.fs.copy_size_range",
            &format!("{src:?}->{dst:?}"),
            "copied_size_representable_as_i64",
        )
    })
}

fn rename_file_strict(src: &str, dst: &str) -> Result<(), Box<EvalAltResult>> {
    let resolved_src = resolve_path(src)?;
    let resolved_dst = resolve_path(dst)?;
    std::fs::rename(&resolved_src, &resolved_dst).map_err(|error| {
        io_boundary_error(
            "extension.fs.rename_file",
            &format!("{src:?}->{dst:?}"),
            "movable_source_and_destination",
            error,
        )
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
    let base = FS_BASE_PATH
        .with(|bp| bp.borrow().clone())
        .unwrap_or_else(|| PathBuf::from("."));
    let mut results = Array::new();
    let mut used_lossy_path = false;

    fn walk_dir(
        dir: &Path,
        pattern: &str,
        results: &mut Array,
        base: &Path,
        used_lossy_path: &mut bool,
    ) -> std::io::Result<()> {
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                let rel_path = path
                    .strip_prefix(base)
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                let rel_str = rel_path.to_string_lossy();
                *used_lossy_path |= matches!(&rel_str, std::borrow::Cow::Owned(_));

                if matches_glob(pattern, &rel_str) {
                    results.push(Dynamic::from(rel_str.to_string()));
                }

                if entry.file_type()?.is_dir() {
                    walk_dir(&path, pattern, results, base, used_lossy_path)?;
                }
            }
        }
        Ok(())
    }

    walk_dir(&base, pattern, &mut results, &base, &mut used_lossy_path).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Glob error: {}", e).into(),
            rhai::Position::NONE,
        ))
    })?;
    if used_lossy_path {
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=glob argument=pattern input={pattern:?} recovery=legacy_lossy_utf8 effective_value=lossy_paths replacement=use_utf8_paths"
        );
    }

    Ok(results)
}

fn glob_files_strict(pattern: &str) -> Result<Array, Box<EvalAltResult>> {
    let configured_base = FS_BASE_PATH.with(|bp| bp.borrow().clone());
    let base = match configured_base {
        Some(base) => base,
        None => PathBuf::from("."),
    };
    let base = base.canonicalize().map_err(|error| {
        io_boundary_error(
            "extension.fs.glob_base",
            pattern,
            "readable_search_base",
            error,
        )
    })?;
    let mut results = Array::new();

    fn walk_dir(
        dir: &Path,
        pattern: &str,
        results: &mut Array,
        base: &Path,
    ) -> Result<(), Box<EvalAltResult>> {
        let entries = std::fs::read_dir(dir).map_err(|error| {
            io_boundary_error(
                "extension.fs.glob_read_directory",
                pattern,
                "readable_search_directory",
                error,
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                io_boundary_error(
                    "extension.fs.glob_directory_entry",
                    pattern,
                    "readable_search_entry",
                    error,
                )
            })?;
            let path = entry.path();
            let rel_path = path.strip_prefix(base).map_err(|error| {
                io_boundary_error(
                    "extension.fs.glob_path",
                    pattern,
                    "path_within_search_base",
                    error,
                )
            })?;
            let rel_str = rel_path.to_str().ok_or_else(|| {
                boundary_error("extension.fs.path_encoding", pattern, "utf8_glob_result")
            })?;

            if matches_glob(pattern, rel_str) {
                results.push(Dynamic::from(rel_str.to_owned()));
            }

            let file_type = entry.file_type().map_err(|error| {
                io_boundary_error(
                    "extension.fs.glob_file_type",
                    rel_str,
                    "readable_file_type",
                    error,
                )
            })?;
            if file_type.is_dir() {
                walk_dir(&path, pattern, results, base)?;
            }
        }
        Ok(())
    }

    walk_dir(&base, pattern, &mut results, &base)?;
    Ok(results)
}

/// Simple glob pattern matcher.
fn matches_glob(pattern: &str, text: &str) -> bool {
    let text_chars: Vec<char> = text.chars().collect();
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let mut matched = vec![false; text_chars.len() + 1];
    matched[0] = true;
    let mut pattern_index = 0;

    while pattern_index < pattern_chars.len() {
        let character = pattern_chars[pattern_index];
        let globstar = character == '*' && pattern_chars.get(pattern_index + 1) == Some(&'*');
        let mut next = vec![false; text_chars.len() + 1];

        if character == '*' {
            next[0] = matched[0];
            for (text_index, text_character) in text_chars.iter().enumerate() {
                let may_consume = globstar || !matches!(text_character, '/' | '\\');
                next[text_index + 1] = matched[text_index + 1] || (may_consume && next[text_index]);
            }
            pattern_index += if globstar { 2 } else { 1 };
        } else {
            for (text_index, text_character) in text_chars.iter().enumerate() {
                let character_matches = character == *text_character
                    || (character == '?' && !matches!(text_character, '/' | '\\'));
                next[text_index + 1] = matched[text_index] && character_matches;
            }
            pattern_index += 1;
        }

        matched = next;
    }

    matched[text_chars.len()]
}

// ============================================================================
// Path Utilities
// ============================================================================

/// Join path components.
pub fn path_join(base: &str, path: &str) -> String {
    Path::new(base).join(path).to_string_lossy().to_string()
}

fn path_join_strict(base: &str, path: &str) -> Result<String, Box<EvalAltResult>> {
    Path::new(base)
        .join(path)
        .to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            boundary_error(
                "extension.fs.path_encoding",
                &format!("{base:?}+{path:?}"),
                "utf8_joined_path",
            )
        })
}

/// Get parent directory of a path.
pub fn path_parent(path: &str) -> String {
    path_component_compat(path, "parent")
}

/// Get filename from path.
pub fn path_filename(path: &str) -> String {
    path_component_compat(path, "filename")
}

/// Get file extension.
pub fn path_extension(path: &str) -> String {
    path_component_compat(path, "extension")
}

/// Get file stem (filename without extension).
pub fn path_stem(path: &str) -> String {
    path_component_compat(path, "stem")
}

fn path_component_compat(path: &str, component: &str) -> String {
    let path_value = Path::new(path);
    let value = match component {
        "parent" => path_value.parent().map(Path::as_os_str),
        "filename" => path_value.file_name(),
        "extension" => path_value.extension(),
        "stem" => path_value.file_stem(),
        _ => None,
    }
    .filter(|value| !value.is_empty());
    match value {
        Some(value) => value.to_str().map(ToOwned::to_owned).unwrap_or_else(|| {
            let effective = value.to_string_lossy();
            log::warn!(
                "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=path_{component} argument=path input={path:?} recovery=legacy_lossy_utf8 effective_value={effective:?} replacement=use_utf8_path_component"
            );
            effective.into_owned()
        }),
        None => {
            log::warn!(
                "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=path_{component} argument=path input={path:?} recovery=legacy_empty_string effective_value=\"\" replacement=use_path_with_{component}"
            );
            String::new()
        }
    }
}

fn path_component_strict(path: &str, component: &str) -> Result<String, Box<EvalAltResult>> {
    let path_value = Path::new(path);
    let value = match component {
        "parent" => path_value.parent(),
        "filename" => path_value.file_name().map(Path::new),
        "extension" => path_value.extension().map(Path::new),
        "stem" => path_value.file_stem().map(Path::new),
        _ => None,
    }
    .filter(|value| !value.as_os_str().is_empty())
    .ok_or_else(|| {
        boundary_error(
            "extension.fs.path_component_missing",
            path,
            &format!("path_with_{component}"),
        )
    })?;
    value
        .to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| boundary_error("extension.fs.path_encoding", path, "utf8_path_component"))
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
        assert!(!matches_glob("src/*", "src/a/b"));
        assert!(matches_glob(&"*".repeat(10_000), ""));
    }

    #[test]
    fn test_path_utilities() {
        assert_eq!(path_join("foo", "bar"), "foo/bar");
        assert_eq!(path_filename("foo/bar.txt"), "bar.txt");
        assert_eq!(path_extension("foo/bar.txt"), "txt");
        assert_eq!(path_stem("foo/bar.txt"), "bar");
        assert_eq!(path_parent("foo/bar/baz.txt"), "foo/bar");
    }

    #[test]
    fn v2_path_helpers_reject_missing_components() {
        let mut engine = Engine::new();
        register_v2(&mut engine, None);
        let error = engine
            .eval::<Dynamic>(r#"path_extension("README")"#)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("extension.fs.path_component_missing"),
            "{error}"
        );
    }

    #[test]
    fn sandbox_rejects_parent_traversal_for_nonexistent_targets() {
        let base = std::env::temp_dir();
        set_base_path(base.to_str());
        let error = resolve_path("../vibelang-outside/new-file")
            .unwrap_err()
            .to_string();
        set_base_path(None);
        assert!(error.contains("extension.fs.path_traversal"), "{error}");
    }

    #[test]
    fn v2_filesystem_errors_keep_stable_boundary_codes() {
        let mut engine = Engine::new();
        register_v2(&mut engine, None);
        let error = engine
            .eval::<Dynamic>(r#"read_file("/definitely/missing/vibelang-file")"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("extension.fs.read_file"), "{error}");
    }
}
