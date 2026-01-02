//! SuperCollider (scsynth) process management.
//!
//! This module handles starting, monitoring, and stopping the scsynth server process.
//! It automatically detects the audio backend (JACK, ALSA, PulseAudio) and attempts
//! to configure appropriate audio connections.

use crate::audio_device::AudioConfig;
use anyhow::{anyhow, Result};
use rosc::{OscPacket, OscType};
use std::net::UdpSocket;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// Wait for scsynth to be ready by polling `/status` until we get a response.
///
/// This function sends OSC `/status` messages to scsynth and waits for `/status.reply`.
/// It uses exponential backoff starting at 50ms, up to a maximum total timeout.
///
/// # Arguments
/// * `port` - The UDP port scsynth is listening on
/// * `timeout` - Maximum time to wait for scsynth to become ready
///
/// # Returns
/// `Ok(())` if scsynth responded, or an error if the timeout was exceeded.
pub fn wait_for_scsynth_ready(port: u16, timeout: Duration) -> Result<()> {
    use rosc::{encoder, OscMessage};

    let start = Instant::now();
    let addr = format!("127.0.0.1:{}", port);

    // Create a socket for sending/receiving status
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    // Set a short timeout for receiving so we can retry
    sock.set_read_timeout(Some(Duration::from_millis(100)))?;

    // Build the /status message
    let status_msg = OscMessage {
        addr: "/status".to_string(),
        args: vec![],
    };
    let status_packet = OscPacket::Message(status_msg);
    let status_bytes = encoder::encode(&status_packet)?;

    let mut attempt = 0;
    let mut delay = Duration::from_millis(50);
    let max_delay = Duration::from_millis(1000);

    log::info!("Waiting for scsynth to become ready on port {}...", port);

    loop {
        // Check if we've exceeded the timeout
        if start.elapsed() > timeout {
            return Err(anyhow!(
                "Timeout waiting for scsynth to start ({}s). \
                 SuperCollider may not be installed correctly, or the audio device may be unavailable.",
                timeout.as_secs()
            ));
        }

        attempt += 1;
        log::debug!("Sending /status to scsynth (attempt {})", attempt);

        // Send status request
        if let Err(e) = sock.send_to(&status_bytes, &addr) {
            log::debug!("Failed to send /status: {}", e);
            std::thread::sleep(delay);
            delay = std::cmp::min(delay * 2, max_delay);
            continue;
        }

        // Try to receive a response
        let mut buf = [0u8; 4096];
        match sock.recv_from(&mut buf) {
            Ok((size, _)) => {
                // Try to decode the response
                if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                    if let OscPacket::Message(msg) = packet {
                        if msg.addr == "/status.reply" {
                            log::info!(
                                "scsynth is ready (responded after {} attempts, {:.1}s)",
                                attempt,
                                start.elapsed().as_secs_f32()
                            );
                            // Log some server info if available
                            if msg.args.len() >= 7 {
                                if let (
                                    Some(OscType::Int(_unused)),
                                    Some(OscType::Int(ugens)),
                                    Some(OscType::Int(synths)),
                                    Some(OscType::Int(groups)),
                                    Some(OscType::Int(synthdefs)),
                                ) = (
                                    msg.args.get(0),
                                    msg.args.get(1),
                                    msg.args.get(2),
                                    msg.args.get(3),
                                    msg.args.get(4),
                                ) {
                                    log::debug!(
                                        "Server status: {} UGens, {} synths, {} groups, {} synthdefs",
                                        ugens, synths, groups, synthdefs
                                    );
                                }
                            }
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock
                    && e.kind() != std::io::ErrorKind::TimedOut
                {
                    log::debug!("Error receiving from scsynth: {}", e);
                }
            }
        }

        // Wait before next attempt with exponential backoff
        std::thread::sleep(delay);
        delay = std::cmp::min(delay * 2, max_delay);
    }
}

impl ScsynthProcess {
    /// Start scsynth on the specified UDP port with default audio configuration.
    ///
    /// This is a convenience wrapper around [`start_with_config`] that uses default
    /// stereo (2 in, 2 out) audio settings.
    ///
    /// # Errors
    ///
    /// Returns an error if scsynth cannot be started or exits immediately.
    pub fn start(port: u16) -> Result<Self> {
        Self::start_with_config(port, &AudioConfig::default())
    }

    /// Start scsynth on the specified UDP port with custom audio configuration.
    ///
    /// This function will:
    /// 1. Spawn the scsynth process with the specified audio device and channel settings
    /// 2. Detect and configure the audio backend (JACK, ALSA, or PulseAudio)
    /// 3. Attempt to auto-connect JACK ports if JACK is running
    ///
    /// # Arguments
    ///
    /// * `port` - UDP port for OSC communication
    /// * `config` - Audio configuration (devices, channels, sample rate)
    ///
    /// # Errors
    ///
    /// Returns an error if scsynth cannot be started or exits immediately.
    pub fn start_with_config(port: u16, config: &AudioConfig) -> Result<Self> {
        log::info!("Starting scsynth on port {}...", port);
        log::info!(
            "Audio config: {} inputs, {} outputs{}{}",
            config.input_channels,
            config.output_channels,
            config.output_device.as_ref().map(|d| format!(", device: {}", d)).unwrap_or_default(),
            config.sample_rate.map(|r| format!(", sample rate: {}", r)).unwrap_or_default()
        );

        // Find scsynth binary
        let scsynth_path = find_scsynth()?;

        // Check if JACK is running (only on Unix-like systems)
        #[cfg(not(target_os = "windows"))]
        let jack_running = Command::new("jack_lsp")
            .output()
            .is_ok_and(|output| output.status.success());

        #[cfg(target_os = "windows")]
        let jack_running = false;

        // Build scsynth command
        let mut cmd = Command::new(&scsynth_path);
        cmd.arg("-u").arg(port.to_string());

        // Device selection (use output device as the main device for scsynth)
        // scsynth's -H flag selects the audio device
        if let Some(ref device) = config.output_device {
            cmd.arg("-H").arg(device);
        }

        // Channel configuration
        cmd.arg("-i").arg(config.input_channels.to_string());
        cmd.arg("-o").arg(config.output_channels.to_string());

        // Sample rate
        if let Some(rate) = config.sample_rate {
            cmd.arg("-S").arg(rate.to_string());
        }

        // Set JACK connection hints
        cmd.env("SC_JACK_DEFAULT_INPUTS", "system");
        cmd.env("SC_JACK_DEFAULT_OUTPUTS", "system");

        // Capture output
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
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

        // Give scsynth a brief moment to start the process before polling
        std::thread::sleep(Duration::from_millis(100));

        // Wait for scsynth to be ready (up to 30 seconds for slow machines)
        wait_for_scsynth_ready(port, Duration::from_secs(30))?;

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
