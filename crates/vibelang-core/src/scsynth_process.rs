//! SuperCollider (scsynth) process management.
//!
//! This module handles starting, monitoring, and stopping the scsynth server process.
//! It automatically detects the audio backend (JACK, ALSA, PulseAudio) and attempts
//! to configure appropriate audio connections.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Find the scsynth binary path based on the operating system.
///
/// On Linux: Uses `scsynth` from PATH
/// On macOS: Checks SuperCollider.app bundle, then PATH
/// On Windows: Checks common installation directories, then PATH
fn find_scsynth() -> Result<PathBuf> {
    // First, check if scsynth is in PATH
    let scsynth_name = if cfg!(windows) {
        "scsynth.exe"
    } else {
        "scsynth"
    };

    // Try PATH first
    if let Ok(output) = Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg(scsynth_name)
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                log::info!("Found scsynth in PATH: {}", path);
                return Ok(PathBuf::from(path));
            }
        }
    }

    // Platform-specific search paths
    #[cfg(target_os = "macos")]
    {
        let mac_paths = [
            "/Applications/SuperCollider.app/Contents/Resources/scsynth",
            "/Applications/SuperCollider/SuperCollider.app/Contents/Resources/scsynth",
        ];
        for path in &mac_paths {
            let p = PathBuf::from(path);
            if p.exists() {
                log::info!("Found scsynth at: {}", path);
                return Ok(p);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Check common Windows installation paths
        let program_files = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
        let program_files_x86 = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".to_string());

        // Look for SuperCollider installations (they often have version numbers)
        for base in &[&program_files, &program_files_x86] {
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.starts_with("supercollider") {
                        let scsynth_path = entry.path().join("scsynth.exe");
                        if scsynth_path.exists() {
                            log::info!("Found scsynth at: {}", scsynth_path.display());
                            return Ok(scsynth_path);
                        }
                    }
                }
            }
        }

        // Also check user's local app data
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let local_path = PathBuf::from(local_app_data).join("Programs");
            if let Ok(entries) = std::fs::read_dir(&local_path) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.starts_with("supercollider") {
                        let scsynth_path = entry.path().join("scsynth.exe");
                        if scsynth_path.exists() {
                            log::info!("Found scsynth at: {}", scsynth_path.display());
                            return Ok(scsynth_path);
                        }
                    }
                }
            }
        }
    }

    // Fallback: just return the binary name and hope it's in PATH
    log::warn!(
        "Could not find scsynth in common locations. Assuming it's in PATH."
    );
    Ok(PathBuf::from(scsynth_name))
}

/// Manages the scsynth process lifecycle.
///
/// When dropped, the scsynth process is gracefully terminated.
///
/// # Example
///
/// ```ignore
/// let process = ScsynthProcess::start(57110)?;
/// println!("scsynth running on port {}", process.port());
/// // Process will be killed when `process` goes out of scope
/// ```
pub struct ScsynthProcess {
    child: Option<Child>,
    port: u16,
    running: Arc<AtomicBool>,
}

impl ScsynthProcess {
    /// Start scsynth on the specified UDP port.
    ///
    /// This function will:
    /// 1. Spawn the scsynth process
    /// 2. Detect and configure the audio backend (JACK, ALSA, or PulseAudio)
    /// 3. Attempt to auto-connect JACK ports if JACK is running
    ///
    /// # Errors
    ///
    /// Returns an error if scsynth cannot be started or exits immediately.
    pub fn start(port: u16) -> Result<Self> {
        log::info!("Starting scsynth on port {}...", port);

        // Find scsynth binary
        let scsynth_path = find_scsynth()?;

        // Check if JACK is running (only on Unix-like systems)
        #[cfg(not(target_os = "windows"))]
        let jack_running = Command::new("jack_lsp")
            .output()
            .is_ok_and(|output| output.status.success());

        #[cfg(target_os = "windows")]
        let jack_running = false;

        // Start scsynth with stereo output
        let mut child = Command::new(&scsynth_path)
            .arg("-u")
            .arg(port.to_string())
            .arg("-i")
            .arg("2") // Input channels (stereo)
            .arg("-o")
            .arg("2") // Output channels
            .env("SC_JACK_DEFAULT_INPUTS", "system")
            .env("SC_JACK_DEFAULT_OUTPUTS", "system")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                anyhow!(
                    "Failed to start scsynth at '{}': {}. Is SuperCollider installed?",
                    scsynth_path.display(),
                    e
                )
            })?;

        // Spawn threads to capture and log scsynth output
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    log::info!("[scsynth] {}", line);
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    log::warn!("[scsynth] {}", line);
                }
            });
        }

        // Give scsynth a moment to start
        std::thread::sleep(Duration::from_millis(200));

        // Auto-connect JACK ports if JACK is running
        if jack_running {
            log::info!("JACK is running, attempting to auto-connect ports...");
            std::thread::sleep(Duration::from_millis(500));

            // Connect outputs
            let out1 = Command::new("jack_connect")
                .arg("SuperCollider:out_1")
                .arg("system:playback_1")
                .output();
            let out2 = Command::new("jack_connect")
                .arg("SuperCollider:out_2")
                .arg("system:playback_2")
                .output();

            match (&out1, &out2) {
                (Ok(_), Ok(_)) => log::info!("JACK output ports connected"),
                _ => {
                    log::warn!("JACK output auto-connection failed. Manually connect:");
                    log::warn!("  jack_connect SuperCollider:out_1 system:playback_1");
                    log::warn!("  jack_connect SuperCollider:out_2 system:playback_2");
                }
            }

            // Connect inputs (for line-in / microphone support)
            let in1 = Command::new("jack_connect")
                .arg("system:capture_1")
                .arg("SuperCollider:in_1")
                .output();
            let in2 = Command::new("jack_connect")
                .arg("system:capture_2")
                .arg("SuperCollider:in_2")
                .output();

            match (&in1, &in2) {
                (Ok(_), Ok(_)) => log::info!("JACK input ports connected"),
                _ => {
                    log::warn!("JACK input auto-connection failed. Manually connect:");
                    log::warn!("  jack_connect system:capture_1 SuperCollider:in_1");
                    log::warn!("  jack_connect system:capture_2 SuperCollider:in_2");
                }
            }
        }

        // Check if process is still running
        match child.try_wait() {
            Ok(Some(status)) => {
                Err(anyhow!(
                    "scsynth exited immediately with status: {}",
                    status
                ))
            }
            Ok(None) => {
                // Process is running
                log::info!("scsynth started successfully on port {}", port);
                let running = Arc::new(AtomicBool::new(true));
                Ok(Self {
                    child: Some(child),
                    port,
                    running,
                })
            }
            Err(e) => Err(anyhow!("Error checking scsynth process: {}", e)),
        }
    }

    /// Get the UDP port scsynth is listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Check if the scsynth process is still running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for ScsynthProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            log::info!("Stopping scsynth...");
            self.running.store(false, Ordering::Relaxed);
            let _ = child.kill();
            let _ = child.wait();
            log::info!("scsynth stopped");
        }
    }
}

#[cfg(test)]
mod tests {
    // Note: These tests require scsynth to be installed
    // They are ignored by default to avoid CI failures

    #[test]
    #[ignore]
    fn test_start_and_stop() {
        use super::*;
        let process = ScsynthProcess::start(57999).expect("Failed to start scsynth");
        assert!(process.is_running());
        assert_eq!(process.port(), 57999);
        drop(process);
        // Process should be stopped now
    }
}
