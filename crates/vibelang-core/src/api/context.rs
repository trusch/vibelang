//! Script execution context.
//!
//! Manages per-script state like current group path and script directory.

use std::cell::RefCell;
use std::path::PathBuf;

/// Source location information for an entity definition.
#[derive(Clone, Debug, Default)]
pub struct SourceLocation {
    /// The script file path where this entity was defined.
    pub file: Option<String>,
    /// Line number (1-based).
    pub line: Option<u32>,
    /// Column number (1-based).
    pub column: Option<u32>,
}

impl SourceLocation {
    /// Create a new source location.
    pub fn new(file: Option<String>, line: Option<u32>, column: Option<u32>) -> Self {
        Self { file, line, column }
    }

    /// Create an unknown source location (no file, line, or column info).
    pub fn unknown() -> Self {
        Self::default()
    }

    /// Check if this source location has any information.
    pub fn is_empty(&self) -> bool {
        self.file.is_none() && self.line.is_none()
    }
}

/// A callback error captured during script execution.
#[derive(Clone, Debug)]
pub struct CallbackError {
    /// Error message.
    pub message: String,
    /// Context name (e.g., group name for define_group).
    pub context: String,
    /// Line number (1-based).
    pub line: Option<u32>,
    /// Column number (1-based).
    pub column: Option<u32>,
}

thread_local! {
    /// Current group path stack for nested group definitions.
    static GROUP_STACK: RefCell<Vec<String>> = RefCell::new(vec!["main".to_string()]);

    /// Script directory for resolving relative paths.
    static SCRIPT_DIR: RefCell<Option<PathBuf>> = RefCell::new(None);

    /// Additional import paths for file resolution.
    static IMPORT_PATHS: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());

    /// Current script file path (for source location tracking).
    static CURRENT_SCRIPT_FILE: RefCell<Option<String>> = RefCell::new(None);

    /// Callback errors collected during script execution.
    static CALLBACK_ERRORS: RefCell<Vec<CallbackError>> = RefCell::new(Vec::new());
}

/// Push a group onto the context stack.
pub fn push_group(name: &str) {
    GROUP_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        let current = stack.last().cloned().unwrap_or_else(|| "main".to_string());
        let new_path = if current == "main" {
            format!("main/{}", name)
        } else {
            format!("{}/{}", current, name)
        };
        stack.push(new_path);
    });
}

/// Pop a group from the context stack.
pub fn pop_group() {
    GROUP_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        if stack.len() > 1 {
            stack.pop();
        }
    });
}

/// Get the current group path.
pub fn current_group_path() -> String {
    GROUP_STACK.with(|stack| {
        stack
            .borrow()
            .last()
            .cloned()
            .unwrap_or_else(|| "main".to_string())
    })
}

/// Set the script directory.
pub fn set_script_dir(dir: PathBuf) {
    SCRIPT_DIR.with(|d| {
        *d.borrow_mut() = Some(dir);
    });
}

/// Get the script directory.
pub fn get_script_dir() -> Option<PathBuf> {
    SCRIPT_DIR.with(|d| d.borrow().clone())
}

/// Set import paths.
pub fn set_import_paths(paths: Vec<PathBuf>) {
    IMPORT_PATHS.with(|p| {
        *p.borrow_mut() = paths;
    });
}

/// Get import paths.
pub fn get_import_paths() -> Vec<PathBuf> {
    IMPORT_PATHS.with(|p| p.borrow().clone())
}

/// Reset the context to initial state.
pub fn reset() {
    GROUP_STACK.with(|stack| {
        *stack.borrow_mut() = vec!["main".to_string()];
    });
    CURRENT_SCRIPT_FILE.with(|f| {
        *f.borrow_mut() = None;
    });
    CALLBACK_ERRORS.with(|e| {
        e.borrow_mut().clear();
    });
}

/// Record a callback error.
pub fn push_callback_error(error: CallbackError) {
    CALLBACK_ERRORS.with(|e| {
        e.borrow_mut().push(error);
    });
}

/// Get all callback errors and clear them.
pub fn take_callback_errors() -> Vec<CallbackError> {
    CALLBACK_ERRORS.with(|e| {
        std::mem::take(&mut *e.borrow_mut())
    })
}

/// Get callback errors without clearing them.
pub fn get_callback_errors() -> Vec<CallbackError> {
    CALLBACK_ERRORS.with(|e| e.borrow().clone())
}

// === Source Location Tracking ===

/// Set the current script file path.
pub fn set_current_script_file(file: Option<String>) {
    CURRENT_SCRIPT_FILE.with(|f| {
        *f.borrow_mut() = file;
    });
}

/// Get the current script file path.
pub fn get_current_script_file() -> Option<String> {
    CURRENT_SCRIPT_FILE.with(|f| f.borrow().clone())
}

/// Resolve a file path by checking multiple locations.
///
/// The resolution order is:
/// 1. If the path is absolute and exists, use it directly
/// 2. Relative to the current working directory
/// 3. Relative to the script directory
/// 4. Relative to each import path (in order)
///
/// Returns the first path that exists, or None if not found.
pub fn resolve_file(path: &str) -> Option<PathBuf> {
    let path_buf = PathBuf::from(path);

    // If already absolute and exists, use it
    if path_buf.is_absolute() {
        if path_buf.exists() {
            log::debug!("Resolved '{}' as absolute path", path_buf.display());
            return Some(path_buf);
        }
        // Don't try other locations for absolute paths
        log::warn!("Absolute path not found: {}", path_buf.display());
        return None;
    }

    // Try relative to current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join(&path_buf);
        if candidate.exists() {
            log::debug!("Resolved '{}' relative to cwd: {}", path, candidate.display());
            return Some(candidate);
        }
    }

    // Try relative to script directory
    if let Some(script_dir) = get_script_dir() {
        let candidate = script_dir.join(&path_buf);
        if candidate.exists() {
            log::debug!("Resolved '{}' relative to script dir: {}", path, candidate.display());
            return Some(candidate);
        }
    }

    // Try relative to each import path
    for import_path in get_import_paths() {
        let candidate = import_path.join(&path_buf);
        if candidate.exists() {
            log::debug!("Resolved '{}' relative to import path: {}", path, candidate.display());
            return Some(candidate);
        }
    }

    log::warn!(
        "Could not resolve file '{}' (checked cwd, script dir, and {} import paths)",
        path,
        get_import_paths().len()
    );
    None
}

/// Resolve a file path, returning an error message if not found.
///
/// This is a convenience wrapper around `resolve_file` that provides a helpful
/// error message listing all the locations that were checked.
pub fn resolve_file_or_error(path: &str) -> Result<PathBuf, String> {
    if let Some(resolved) = resolve_file(path) {
        return Ok(resolved);
    }

    // Build a helpful error message
    let mut locations = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        locations.push(format!("  - cwd: {}", cwd.display()));
    }

    if let Some(script_dir) = get_script_dir() {
        locations.push(format!("  - script dir: {}", script_dir.display()));
    }

    for import_path in get_import_paths() {
        locations.push(format!("  - import path: {}", import_path.display()));
    }

    let locations_str = if locations.is_empty() {
        "  (no search paths configured)".to_string()
    } else {
        locations.join("\n")
    };

    Err(format!(
        "File not found: '{}'\nSearched in:\n{}",
        path, locations_str
    ))
}
