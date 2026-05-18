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
//! - `stdlib/processors/` - Patchable named-input processors
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
///
/// If the data directory is unavailable, falls back to `~/vibelang/stdlib/`.
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
        .or_else(|| {
            // Fallback to home directory if data_dir is not available
            // (avoids hardcoding platform-specific paths like .local/share)
            dirs::home_dir()
        })
        .unwrap_or_else(|| PathBuf::from("."))
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
        eprintln!("Extracting stdlib to {}", target_path.display());
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
    "processors",
    "fx",
    "theory",
    "cv",
];

/// Get the embedded stdlib directory for direct access.
///
/// This can be used to list files or read content without extracting.
pub fn embedded_stdlib() -> &'static Dir<'static> {
    &STDLIB_DIR
}

/// Get all stdlib files as a HashMap of path -> content.
///
/// This is useful for WASM where filesystem access is not available.
/// The paths are relative to the stdlib root (e.g., "drums/kicks/kick_808.vibe").
pub fn get_stdlib_files() -> std::collections::HashMap<String, String> {
    let mut files = std::collections::HashMap::new();
    collect_files_recursive(&STDLIB_DIR, &mut files);
    files
}

/// Recursively collect all files from a directory.
fn collect_files_recursive(dir: &Dir, files: &mut std::collections::HashMap<String, String>) {
    // Collect files in this directory
    for file in dir.files() {
        if let Some(path) = file.path().to_str() {
            // Only include .vibe files
            if path.ends_with(".vibe") {
                if let Some(content) = file.contents_utf8() {
                    files.insert(path.to_string(), content.to_string());
                }
            }
        }
    }

    // Recursively collect from subdirectories
    for subdir in dir.dirs() {
        collect_files_recursive(subdir, files);
    }
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

    #[test]
    fn test_get_stdlib_files_paths() {
        let files = get_stdlib_files();
        println!("Found {} stdlib files:", files.len());
        for path in files.keys().take(20) {
            println!("  - {}", path);
        }
        assert!(!files.is_empty(), "Should have stdlib files");
    }

    /// Regression: stdlib CV synthdefs that are meant to be driven by
    /// vibelang's pattern/melody trigger protocol must declare their
    /// trigger/gate input as `gate` — matching the param name fired by
    /// `pattern().on(voice).step(...)` and `melody().on(voice).notes(...)`.
    /// Previously these used `gate_in` / `trig_in` (designed for explicit
    /// `.modulate_by(src, "out")` wiring), which silently no-ops a pattern.
    #[test]
    fn cv_trigger_gate_synthdefs_use_gate_param() {
        let files = get_stdlib_files();
        let cases: &[(&str, &[&str])] = &[
            ("cv/triggers/cv_gate.vibe", &[".param(\"gate\","]),
            ("cv/triggers/cv_trigger.vibe", &[".param(\"gate\","]),
            ("cv/triggers/cv_burst.vibe", &[".param(\"gate\","]),
            ("cv/util/cv_sample_hold.vibe", &[".param(\"gate\","]),
            ("cv/envelopes/cv_env_ad.vibe", &[".param(\"gate\","]),
            ("cv/envelopes/cv_env_adsr.vibe", &[".param(\"gate\","]),
            ("cv/envelopes/cv_env_complex.vibe", &[".param(\"gate\","]),
        ];

        for (path, must_contain) in cases {
            let body = files
                .get(*path)
                .unwrap_or_else(|| panic!("stdlib missing {}", path));
            for needle in *must_contain {
                assert!(
                    body.contains(needle),
                    "{} should declare {} (vibelang gate convention)",
                    path,
                    needle
                );
            }
            assert!(
                !body.contains(".param(\"gate_in\","),
                "{} still declares legacy `gate_in` param",
                path
            );
            assert!(
                !body.contains(".param(\"trig_in\","),
                "{} still declares legacy `trig_in` param",
                path
            );
        }

        // cv_maths' rising-edge trigger input is also renamed to `gate` —
        // patterns/melodies fire `gate` and a Maths trigger IS a gate edge.
        let maths = files
            .get("cv/eurorack/cv_maths.vibe")
            .expect("stdlib missing cv/eurorack/cv_maths.vibe");
        assert!(
            maths.contains(".param(\"gate\","),
            "cv_maths should declare `gate` (vibelang trigger convention)"
        );
        assert!(
            !maths.contains(".param(\"trig_in\","),
            "cv_maths still declares legacy `trig_in`"
        );
    }

    /// kr-output LFO siblings:
    /// each `cv_lfo_*_kr.vibe` declares `.output_kr("out")` (the kr
    /// CV-source port), uses `a2k_kr` to decimate the internal ar
    /// computation back to kr at the output, and does NOT carry a
    /// bare ar `.output(...)` declaration that would shadow the kr port.
    #[test]
    fn cv_lfo_kr_synthdefs_declare_kr_output_and_a2k() {
        let files = get_stdlib_files();
        let kr_lfos: &[&str] = &[
            "cv/lfo/cv_lfo_sine_kr.vibe",
            "cv/lfo/cv_lfo_tri_kr.vibe",
            "cv/lfo/cv_lfo_square_kr.vibe",
            "cv/lfo/cv_lfo_random_kr.vibe",
            "cv/lfo/cv_lfo_smooth_random_kr.vibe",
            "cv/lfo/cv_lfo_chaos_kr.vibe",
        ];

        for path in kr_lfos {
            let body = files
                .get(*path)
                .unwrap_or_else(|| panic!("stdlib missing {}", path));
            assert!(
                body.contains(".output_kr(\"out\")"),
                "{} should declare `.output_kr(\"out\")` (kr CV-source port)",
                path,
            );
            assert!(
                body.contains("a2k_kr("),
                "{} should call `a2k_kr(...)` to decimate ar → kr at the output",
                path,
            );
            // Sanity: must not also declare a bare ar `.output("out")` /
            // `.output("out", ...)`.  `.output_kr(...)` is the only port.
            assert!(
                !body.contains(".output(\"out\""),
                "{} declares a bare ar `.output(\"out\"...)` — the kr-only port \
                 must not be shadowed by an ar output",
                path,
            );
        }
    }

    /// Regression for the ReSynthesizer rack: Spectraphon's public synthdef
    /// owns both the FFT/BinData analyzer path and the SAM/SAO additive bank.
    /// The old split analyzer/oscillator and Rhai helper files must not return
    /// as separate user-facing surfaces.
    #[test]
    fn spectraphon_is_one_public_synthdef_with_analyzer_and_bank() {
        let files = get_stdlib_files();
        let spectraphon = files
            .get("instruments/spectral/spectraphon.vibe")
            .expect("stdlib missing instruments/spectral/spectraphon.vibe");

        assert!(
            files
                .get("instruments/spectral/spectraphon_analyzer.vibe")
                .is_none()
                && files
                    .get("instruments/spectral/spectraphon_oscillator.vibe")
                    .is_none()
                && files
                .get("instruments/spectral/spectraphon_sam_oscillator.vibe")
                .is_none()
                && files
                    .get("instruments/spectral/spectraphon_sao_oscillator.vibe")
                    .is_none()
                && files
                    .get("instruments/spectral/spectraphon_side.vibe")
                    .is_none()
                && files
                    .get("instruments/spectral/spectraphon_dual.vibe")
                    .is_none(),
            "split-era Spectraphon files must be deleted",
        );

        assert!(
            spectraphon.contains("define_synthdef(\"spectraphon\""),
            "spectraphon should be the single public synthdef"
        );
        assert!(
            spectraphon.contains(".input(\"analyze\", 1)")
                && spectraphon.contains("fft_kr(")
                && spectraphon.contains("bin_data_kr("),
            "spectraphon should own the real FFT/BinData analyzer path"
        );
        assert!(
            spectraphon.contains(".param(\"fft_buf\", 0.0)")
                && spectraphon.contains("let chain = fft_kr(fft_mem"),
            "spectraphon should support script-allocated FFT chain buffers"
        );
        assert!(
            spectraphon.contains(".param(\"mode\", 0.0)"),
            "spectraphon should expose a `mode` param (0 sam_live / 1 sam_capture / 2 sao)"
        );
        assert!(
            spectraphon.contains("buf_rd_kr(1, mag_mem")
                && spectraphon.contains("buf_wr_kr(")
                && spectraphon.contains("base00"),
            "spectraphon should retain SAM live/capture mag reads + SAO bilinear array reads"
        );
    }

    #[test]
    fn spectraphon_normalizes_analyzer_magnitude_bank() {
        let files = get_stdlib_files();
        let spectraphon = files
            .get("instruments/spectral/spectraphon.vibe")
            .expect("stdlib missing instruments/spectral/spectraphon.vibe");

        assert!(
            spectraphon.contains("let mag_scale = 0.001953125"),
            "spectraphon should retain FFT-size magnitude scaling before writing mag_buf"
        );
        assert!(
            spectraphon.contains("fn _spectraphon_mag_norm(mag_buf)")
                && spectraphon.contains("return 3.5 / max(mag_sum, 1.0);"),
            "spectraphon should derive a bounded unit-spectrum normalizer from mag_buf"
        );

        let normalized_reads = spectraphon
            .matches("buf_rd_kr(1, mag_mem, k - 1.0, 0, 1) * mag_norm")
            .count();
        assert!(
            normalized_reads >= 4,
            "all SAM partial-bank reads (odd seed/loop + even seed/loop) should apply mag_norm; found {normalized_reads}"
        );
    }
}
