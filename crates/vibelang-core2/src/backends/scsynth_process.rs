//! scsynth process management.
//!
//! This module provides functionality to spawn and manage the scsynth process.
//! It handles starting the server, waiting for it to become available, and
//! graceful shutdown.

use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Configuration for spawning scsynth.
#[derive(Clone, Debug)]
pub struct ScsynthConfig {
    /// Path to the scsynth executable.
    /// If None, will search in PATH.
    pub executable: Option<PathBuf>,

    /// UDP port to listen on.
    pub port: u16,

    /// Number of audio bus channels.
    pub audio_bus_channels: u32,

    /// Number of control bus channels.
    pub control_bus_channels: u32,

    /// Number of input channels.
    pub input_channels: u32,

    /// Number of output channels.
    pub output_channels: u32,

    /// Block size (samples per control period).
    pub block_size: u32,

    /// Hardware buffer size.
    pub hardware_buffer_size: u32,

    /// Sample rate (0 = use hardware default).
    pub sample_rate: u32,

    /// Number of wire buffers.
    pub wire_buffers: u32,

    /// Number of random seeds.
    pub random_seeds: u32,

    /// Maximum number of nodes.
    pub max_nodes: u32,

    /// Maximum number of synthdefs.
    pub max_synthdefs: u32,

    /// Real-time memory size in bytes.
    pub realtime_memory: u32,

    /// Whether to run in verbose mode.
    pub verbose: bool,

    /// Audio device name (None = default).
    pub device: Option<String>,
}

impl Default for ScsynthConfig {
    fn default() -> Self {
        Self {
            executable: None,
            port: 57110,
            audio_bus_channels: 1024,
            control_bus_channels: 16384,
            input_channels: 2,
            output_channels: 2,
            block_size: 64,
            hardware_buffer_size: 512,
            sample_rate: 0, // Use hardware default
            wire_buffers: 64,
            random_seeds: 64,
            max_nodes: 1024,
            max_synthdefs: 1024,
            realtime_memory: 8192,
            verbose: false,
            device: None,
        }
    }
}

impl ScsynthConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the UDP port.
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Set the number of output channels.
    pub fn output_channels(mut self, channels: u32) -> Self {
        self.output_channels = channels;
        self
    }

    /// Set the number of input channels.
    pub fn input_channels(mut self, channels: u32) -> Self {
        self.input_channels = channels;
        self
    }

    /// Set the sample rate (0 = hardware default).
    pub fn sample_rate(mut self, rate: u32) -> Self {
        self.sample_rate = rate;
        self
    }

    /// Set the audio device name.
    pub fn device(mut self, device: impl Into<String>) -> Self {
        self.device = Some(device.into());
        self
    }

    /// Enable verbose mode.
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Set the path to the scsynth executable.
    pub fn executable(mut self, path: impl Into<PathBuf>) -> Self {
        self.executable = Some(path.into());
        self
    }

    /// Build the command arguments.
    fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            "-u".to_string(),
            self.port.to_string(),
            "-a".to_string(),
            self.audio_bus_channels.to_string(),
            "-c".to_string(),
            self.control_bus_channels.to_string(),
            "-i".to_string(),
            self.input_channels.to_string(),
            "-o".to_string(),
            self.output_channels.to_string(),
            "-z".to_string(),
            self.block_size.to_string(),
            "-Z".to_string(),
            self.hardware_buffer_size.to_string(),
            "-w".to_string(),
            self.wire_buffers.to_string(),
            "-r".to_string(),
            self.random_seeds.to_string(),
            "-n".to_string(),
            self.max_nodes.to_string(),
            "-d".to_string(),
            self.max_synthdefs.to_string(),
            "-m".to_string(),
            self.realtime_memory.to_string(),
        ];

        if self.sample_rate > 0 {
            args.push("-S".to_string());
            args.push(self.sample_rate.to_string());
        }

        if let Some(ref device) = self.device {
            args.push("-H".to_string());
            args.push(device.clone());
        }

        if self.verbose {
            args.push("-V".to_string());
            args.push("1".to_string());
        }

        args
    }
}

/// Error type for process management.
#[derive(Debug)]
pub enum ProcessError {
    /// Failed to spawn the process.
    SpawnFailed(io::Error),
    /// Process exited unexpectedly.
    ProcessExited(Option<i32>),
    /// scsynth executable not found.
    ExecutableNotFound,
    /// Process is not running.
    NotRunning,
}

impl std::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessError::SpawnFailed(e) => write!(f, "Failed to spawn scsynth: {}", e),
            ProcessError::ProcessExited(code) => {
                if let Some(code) = code {
                    write!(f, "scsynth exited with code {}", code)
                } else {
                    write!(f, "scsynth exited without code")
                }
            }
            ProcessError::ExecutableNotFound => write!(f, "scsynth executable not found"),
            ProcessError::NotRunning => write!(f, "scsynth is not running"),
        }
    }
}

impl std::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProcessError::SpawnFailed(e) => Some(e),
            _ => None,
        }
    }
}

/// Manages a scsynth child process.
pub struct ScsynthProcess {
    /// The child process handle.
    child: Child,
    /// Configuration used to spawn the process.
    config: ScsynthConfig,
    /// Flag indicating if the process should be kept running.
    running: Arc<AtomicBool>,
}

impl ScsynthProcess {
    /// Find the scsynth executable.
    fn find_executable(config: &ScsynthConfig) -> Result<PathBuf, ProcessError> {
        // Use explicit path if provided
        if let Some(ref path) = config.executable {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        // Try common locations
        let candidates = [
            // Linux
            "/usr/bin/scsynth",
            "/usr/local/bin/scsynth",
            "/opt/SuperCollider/scsynth",
            // macOS
            "/Applications/SuperCollider.app/Contents/Resources/scsynth",
            "/Applications/SuperCollider/SuperCollider.app/Contents/Resources/scsynth",
            // User-installed on macOS
            "~/Library/Application Support/SuperCollider/scsynth",
        ];

        for candidate in &candidates {
            let path = PathBuf::from(shellexpand::tilde(candidate).as_ref());
            if path.exists() {
                return Ok(path);
            }
        }

        // Try PATH
        if let Ok(output) = Command::new("which").arg("scsynth").output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout);
                let path = PathBuf::from(path_str.trim());
                if path.exists() {
                    return Ok(path);
                }
            }
        }

        Err(ProcessError::ExecutableNotFound)
    }

    /// Spawn a new scsynth process with the given configuration.
    pub fn spawn(config: ScsynthConfig) -> Result<Self, ProcessError> {
        let executable = Self::find_executable(&config)?;
        let args = config.build_args();

        tracing::info!(
            "Spawning scsynth: {} {}",
            executable.display(),
            args.join(" ")
        );

        let child = Command::new(&executable)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(if config.verbose {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .stderr(if config.verbose {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .spawn()
            .map_err(ProcessError::SpawnFailed)?;

        tracing::info!("scsynth started with PID {}", child.id());

        Ok(Self {
            child,
            config,
            running: Arc::new(AtomicBool::new(true)),
        })
    }

    /// Spawn with default configuration.
    pub fn spawn_default() -> Result<Self, ProcessError> {
        Self::spawn(ScsynthConfig::default())
    }

    /// Get the UDP port the server is listening on.
    pub fn port(&self) -> u16 {
        self.config.port
    }

    /// Get the server address string.
    pub fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.config.port)
    }

    /// Check if the process is still running.
    pub fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,       // Still running
            Ok(Some(_)) => false,   // Exited
            Err(_) => false,        // Error checking status
        }
    }

    /// Wait for the process to become ready.
    ///
    /// This simply waits for a short time to allow scsynth to start up.
    /// The actual readiness check is done by the backend via /status.
    pub async fn wait_startup(&self, timeout: Duration) {
        // Give scsynth time to start up
        let startup_delay = Duration::from_millis(500).min(timeout);
        tokio::time::sleep(startup_delay).await;
    }

    /// Stop the scsynth process gracefully.
    pub fn stop(&mut self) -> Result<(), ProcessError> {
        if !self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        self.running.store(false, Ordering::Relaxed);

        tracing::info!("Stopping scsynth (PID {})", self.child.id());

        // Try to kill the process
        #[cfg(unix)]
        {
            // Send SIGTERM first for graceful shutdown
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGTERM);
            }

            // Wait a bit for graceful shutdown
            std::thread::sleep(Duration::from_millis(100));

            // Check if it's still running
            if let Ok(None) = self.child.try_wait() {
                // Still running, force kill
                let _ = self.child.kill();
            }
        }

        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }

        // Wait for the process to fully exit
        let _ = self.child.wait();

        tracing::info!("scsynth stopped");
        Ok(())
    }

    /// Get the process ID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for ScsynthProcess {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Utility module for shell expansion.
mod shellexpand {
    use std::borrow::Cow;
    use std::env;

    /// Expand tilde (~) in paths.
    pub fn tilde(path: &str) -> Cow<'_, str> {
        if path.starts_with("~/") {
            if let Ok(home) = env::var("HOME") {
                return Cow::Owned(format!("{}{}", home, &path[1..]));
            }
        }
        Cow::Borrowed(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ScsynthConfig::default();
        assert_eq!(config.port, 57110);
        assert_eq!(config.output_channels, 2);
        assert_eq!(config.input_channels, 2);
    }

    #[test]
    fn test_config_builder() {
        let config = ScsynthConfig::new()
            .port(57111)
            .output_channels(8)
            .sample_rate(48000)
            .verbose(true);

        assert_eq!(config.port, 57111);
        assert_eq!(config.output_channels, 8);
        assert_eq!(config.sample_rate, 48000);
        assert!(config.verbose);
    }

    #[test]
    fn test_build_args() {
        let config = ScsynthConfig::new().port(57111).sample_rate(48000);
        let args = config.build_args();

        assert!(args.contains(&"-u".to_string()));
        assert!(args.contains(&"57111".to_string()));
        assert!(args.contains(&"-S".to_string()));
        assert!(args.contains(&"48000".to_string()));
    }

    #[test]
    fn test_process_error_display() {
        let err = ProcessError::ExecutableNotFound;
        assert_eq!(format!("{}", err), "scsynth executable not found");

        let err = ProcessError::ProcessExited(Some(1));
        assert_eq!(format!("{}", err), "scsynth exited with code 1");
    }

    #[test]
    fn test_shellexpand_tilde() {
        // This test might be environment-dependent
        let path = "~/test";
        let expanded = shellexpand::tilde(path);

        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(expanded.as_ref(), format!("{}/test", home));
        }

        // Non-tilde paths should pass through unchanged
        let path = "/usr/bin/test";
        let expanded = shellexpand::tilde(path);
        assert_eq!(expanded.as_ref(), "/usr/bin/test");
    }
}
