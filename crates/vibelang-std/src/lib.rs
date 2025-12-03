//! VibeLang Standard Library.
//!
//! This crate provides the standard library of `.vibe` files for VibeLang,
//! including drums, bass, leads, pads, effects, and music theory utilities.
//!
//! # Usage
//!
//! The standard library is typically accessed via import paths in `.vibe` files:
//!
//! ```vibe
//! import "stdlib/drums/kicks/kick_808.vibe";
//! import "stdlib/effects/reverb.vibe";
//! ```
//!
//! # Directory Structure
//!
//! - `stdlib/drums/` - Drum sounds (kicks, snares, hihats, claps, percussion, cymbals)
//! - `stdlib/bass/` - Bass sounds (sub, acid, pluck, reese, wobble)
//! - `stdlib/leads/` - Lead synths (pluck, stabs, synth)
//! - `stdlib/pads/` - Pad textures (ambient, lush)
//! - `stdlib/textures/` - Ambient textures (ambient, drone)
//! - `stdlib/effects/` - Audio effects (delay, reverb, distortion, filters, etc.)
//! - `stdlib/fx/` - Sound design elements (impacts, risers, subdrops, sweeps)
//! - `stdlib/theory/` - Music theory tools (scales, chords, progressions, etc.)
//!
//! # Installation
//!
//! When installed via `cargo install vibelang-cli`, the stdlib is automatically
//! extracted to `~/.local/share/vibelang/stdlib/` (Linux/macOS) or the equivalent
//! user data directory on Windows.

use include_dir::{include_dir, Dir};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Embedded stdlib directory (compiled into the binary)
static STDLIB_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/stdlib");

/// Cached stdlib path
static STDLIB_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Get the path to the stdlib directory.
///
/// On first call, this will extract the embedded stdlib files to the user's
/// data directory if they don't already exist. Subsequent calls return the
/// cached path.
///
/// The stdlib is extracted to:
/// - Linux: `~/.local/share/vibelang/stdlib/`
/// - macOS: `~/Library/Application Support/vibelang/stdlib/`
/// - Windows: `C:\Users\<User>\AppData\Roaming\vibelang\stdlib\`
pub fn stdlib_path() -> &'static str {
    STDLIB_PATH
        .get_or_init(|| {
            let path = get_stdlib_install_path();
            ensure_stdlib_extracted(&path);
            path
        })
        .to_str()
        .unwrap_or(".")
}

/// Get the installation path for the stdlib.
fn get_stdlib_install_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| {
            // Fallback to home directory if data_dir is not available
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .join("vibelang")
        .join("stdlib")
}

/// Ensure the stdlib is extracted to the target path.
fn ensure_stdlib_extracted(target_path: &PathBuf) {
    // Check if stdlib already exists and has content
    if target_path.exists() && target_path.join("index.vibe").exists() {
        // Stdlib already extracted, check version
        let version_file = target_path.join(".version");
        let current_version = env!("CARGO_PKG_VERSION");

        if let Ok(installed_version) = std::fs::read_to_string(&version_file) {
            if installed_version.trim() == current_version {
                // Same version, no need to re-extract
                return;
            }
        }

        // Different version or no version file, re-extract
        eprintln!(
            "Updating stdlib to version {} in {}",
            current_version,
            target_path.display()
        );
    } else {
        eprintln!(
            "Extracting stdlib to {}",
            target_path.display()
        );
    }

    // Extract the embedded stdlib
    if let Err(e) = extract_stdlib(target_path) {
        eprintln!("Warning: Failed to extract stdlib: {}", e);
        eprintln!("You may need to manually specify stdlib path with -I flag");
    }
}

/// Extract the embedded stdlib to the target directory.
fn extract_stdlib(target_path: &PathBuf) -> std::io::Result<()> {
    // Create the target directory
    std::fs::create_dir_all(target_path)?;

    // Extract all files recursively
    extract_dir(&STDLIB_DIR, target_path)?;

    // Write version file
    let version_file = target_path.join(".version");
    std::fs::write(version_file, env!("CARGO_PKG_VERSION"))?;

    eprintln!("Stdlib extracted successfully");
    Ok(())
}

/// Recursively extract a directory.
fn extract_dir(dir: &Dir, target_path: &PathBuf) -> std::io::Result<()> {
    // Extract files in this directory
    for file in dir.files() {
        let file_path = target_path.join(file.path());

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&file_path, file.contents())?;
    }

    // Recursively extract subdirectories
    for subdir in dir.dirs() {
        extract_dir(subdir, target_path)?;
    }

    Ok(())
}

/// List of all available stdlib categories.
pub const CATEGORIES: &[&str] = &[
    "drums",
    "bass",
    "leads",
    "pads",
    "textures",
    "effects",
    "fx",
    "theory",
];

/// Get the embedded stdlib directory for direct access.
///
/// This can be used to list files or read content without extracting.
pub fn embedded_stdlib() -> &'static Dir<'static> {
    &STDLIB_DIR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdlib_path_returns_string() {
        let path = stdlib_path();
        assert!(!path.is_empty());
    }

    #[test]
    fn test_embedded_stdlib_has_content() {
        let dir = embedded_stdlib();
        // Should have at least some files
        assert!(dir.files().count() > 0 || dir.dirs().count() > 0);
    }
}
