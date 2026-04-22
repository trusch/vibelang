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

use crate::audio::{AudioConnector, PortDirection};

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

    /// Whether to automatically connect JACK/PipeWire ports to system output.
    /// Default: true (auto-connect for best out-of-box experience).
    pub auto_connect_jack: bool,

    /// Manual JACK output connection targets (one per output channel).
    /// If set, these ports are used instead of auto-discovery.
    /// Example: vec!["Headphones:playback_FL", "Headphones:playback_FR"]
    pub jack_connect_outputs: Option<Vec<String>>,

    /// Manual JACK input connection sources (one per input channel).
    /// If set, these ports are connected to SuperCollider's inputs.
    /// Example: vec!["system:capture_1", "system:capture_2", "system:capture_3", "system:capture_4"]
    pub jack_connect_inputs: Option<Vec<String>>,
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
            auto_connect_jack: true, // Auto-connect by default for best UX
            jack_connect_outputs: None,
            jack_connect_inputs: None,
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

    /// Enable or disable automatic JACK/PipeWire port connection.
    ///
    /// When enabled (default), vibelang will automatically connect
    /// SuperCollider's audio outputs to the system's default audio output.
    pub fn auto_connect_jack(mut self, auto_connect: bool) -> Self {
        self.auto_connect_jack = auto_connect;
        self
    }

    /// Set manual JACK output connection targets.
    ///
    /// When set, these ports will be used instead of auto-discovering
    /// the system audio output. One port per output channel.
    ///
    /// Example: `jack_connect_outputs(vec!["Headphones:playback_FL", "Headphones:playback_FR"])`
    pub fn jack_connect_outputs(mut self, ports: Vec<String>) -> Self {
        self.jack_connect_outputs = Some(ports);
        self
    }

    /// Set manual JACK input connection sources.
    ///
    /// When set, these ports will be connected to SuperCollider's audio inputs.
    /// One port per input channel.
    ///
    /// Example: `jack_connect_inputs(vec!["system:capture_1", "system:capture_2"])`
    pub fn jack_connect_inputs(mut self, ports: Vec<String>) -> Self {
        self.jack_connect_inputs = Some(ports);
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

    /// Kill any existing scsynth process that might be using the same port.
    ///
    /// This prevents "duplicate node ID" errors caused by connecting to a stale
    /// scsynth instance from a previous run.
    fn kill_stale_scsynth(port: u16) {
        // Try to find and kill any scsynth using this port
        // Use lsof on Unix to find processes using the UDP port
        #[cfg(unix)]
        {
            // Try lsof first (more reliable for finding UDP ports)
            if let Ok(output) = Command::new("lsof")
                .args(["-i", &format!("UDP:{}", port), "-t"])
                .output()
            {
                if output.status.success() {
                    let pids = String::from_utf8_lossy(&output.stdout);
                    for pid in pids.split_whitespace() {
                        if let Ok(pid) = pid.parse::<u32>() {
                            tracing::debug!("Killing stale process on port {}: PID {}", port, pid);
                            let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
                        }
                    }
                    // Give the OS time to clean up
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    /// Spawn a new scsynth process with the given configuration.
    pub fn spawn(config: ScsynthConfig) -> Result<Self, ProcessError> {
        // Kill any stale scsynth that might be using the same port
        Self::kill_stale_scsynth(config.port);

        let executable = Self::find_executable(&config)?;
        let args = config.build_args();

        tracing::info!(
            "Spawning scsynth: {} {}",
            executable.display(),
            args.join(" ")
        );

        let mut cmd = Command::new(&executable);
        cmd.args(&args);

        // Control JACK auto-connection behavior.
        // If manual ports are specified, we want to disable scsynth's auto-connect
        // so we can make our own connections afterward.
        let has_manual_outputs = config.jack_connect_outputs.is_some();
        let has_manual_inputs = config.jack_connect_inputs.is_some();

        if config.auto_connect_jack && !has_manual_outputs {
            // When auto-connect is enabled and no manual outputs specified,
            // hint which ports to connect to
            cmd.env("SC_JACK_DEFAULT_OUTPUTS", "system");
        } else {
            // Disable auto-connect for outputs
            cmd.env("SC_JACK_DEFAULT_OUTPUTS", "");
        }

        if config.auto_connect_jack && !has_manual_inputs {
            // When auto-connect is enabled and no manual inputs specified,
            // hint which ports to connect to
            cmd.env("SC_JACK_DEFAULT_INPUTS", "system");
        } else {
            // Disable auto-connect for inputs
            cmd.env("SC_JACK_DEFAULT_INPUTS", "");
        }

        cmd.stdin(Stdio::null());
        cmd.stdout(if config.verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        });
        cmd.stderr(if config.verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        });

        let child = cmd.spawn().map_err(ProcessError::SpawnFailed)?;

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
            Ok(None) => true,     // Still running
            Ok(Some(_)) => false, // Exited
            Err(_) => false,      // Error checking status
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

    /// Check if auto JACK connection is enabled.
    pub fn should_auto_connect(&self) -> bool {
        self.config.auto_connect_jack
    }

    /// Disconnect all audio ports from SuperCollider.
    ///
    /// This is called when `--no-jack-connect` is used to undo any automatic
    /// connections that scsynth's JACK driver may have made.
    ///
    /// Uses AudioConnector which supports both PipeWire and JACK backends.
    #[cfg(unix)]
    pub fn disconnect_all_jack_ports(&self) {
        tracing::debug!("Disconnecting all audio ports from SuperCollider");

        // Get the client name (defaults to "SuperCollider" but can be custom via -H flag)
        let client_name = self.config.device.as_deref().unwrap_or("SuperCollider");

        let connector = match AudioConnector::detect() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("No audio backend available for disconnect: {}", e);
                return;
            }
        };

        match connector.disconnect_client(client_name) {
            Ok(()) => {
                tracing::info!("Disconnected all audio ports from {}", client_name);
            }
            Err(e) => {
                tracing::warn!("Failed to disconnect some ports: {}", e);
            }
        }
    }

    #[cfg(not(unix))]
    pub fn disconnect_all_jack_ports(&self) {
        // No-op on non-Unix platforms
    }

    /// Automatically connect SuperCollider's JACK/PipeWire ports to system output.
    ///
    /// This function discovers the default audio output and connects SuperCollider's
    /// output ports to it. Works with both PipeWire (using pw-link) and JACK.
    ///
    /// If manual targets are specified via `jack_connect_targets`, those are used
    /// instead of auto-discovery.
    ///
    /// Call this after scsynth has started and JACK ports are available.
    pub fn auto_connect_jack_ports(&self) {
        if !self.config.auto_connect_jack {
            tracing::debug!("JACK auto-connect disabled by configuration");
            return;
        }

        #[cfg(unix)]
        {
            // Give JACK a moment to register the ports
            std::thread::sleep(Duration::from_millis(200));

            // Get the JACK client name
            let client_name = self.config.device.as_deref().unwrap_or("SuperCollider");

            // Handle output connections
            let outputs_connected = if let Some(ref ports) = self.config.jack_connect_outputs {
                tracing::info!("Connecting outputs to specified JACK ports: {:?}", ports);
                self.try_connect_outputs_to_ports(client_name, ports)
            } else {
                // Auto-discover outputs
                self.try_pipewire_connect() || self.try_jack_connect()
            };

            if !outputs_connected {
                tracing::warn!(
                    "Could not connect JACK output ports. \
                     Audio may not be audible. \
                     Try connecting SuperCollider outputs manually using a tool like qjackctl or helvum."
                );
            }

            // Handle input connections
            if let Some(ref ports) = self.config.jack_connect_inputs {
                tracing::info!("Connecting inputs from specified JACK ports: {:?}", ports);
                if !self.try_connect_inputs_from_ports(client_name, ports) {
                    tracing::warn!(
                        "Could not connect from specified input ports: {:?}. \
                         Check that the port names are correct (use `pw-link -o` or `jack_lsp` to list available ports).",
                        ports
                    );
                }
            }
        }

        #[cfg(not(unix))]
        {
            tracing::debug!("JACK auto-connect not supported on this platform");
        }
    }

    /// Try to connect SuperCollider outputs to specific ports.
    ///
    /// Uses AudioConnector with fuzzy matching support for better UX.
    /// Supports both PipeWire (pw-link) and JACK (jack_connect) backends.
    #[cfg(unix)]
    fn try_connect_outputs_to_ports(&self, client_name: &str, ports: &[String]) -> bool {
        let connector = match AudioConnector::detect() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("No audio backend available: {}", e);
                return false;
            }
        };

        tracing::debug!("Using audio backend: {}", connector.backend_name());

        let mut all_success = true;

        for (i, pattern) in ports.iter().enumerate() {
            let src_port = format!("{}:out_{}", client_name, i + 1);

            // Use the connector's fuzzy matching to resolve the port pattern
            match connector.resolve_port(pattern, PortDirection::Input) {
                Ok(resolved_port) => match connector.connect(&src_port, &resolved_port) {
                    Ok(()) => {
                        tracing::debug!("Connected {} -> {}", src_port, resolved_port);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to connect {} -> {}: {}",
                            src_port,
                            resolved_port,
                            e
                        );
                        all_success = false;
                    }
                },
                Err(e) => {
                    tracing::warn!("Could not resolve port pattern '{}': {}", pattern, e);
                    all_success = false;
                }
            }
        }

        if all_success {
            tracing::info!(
                "Output ports connected successfully via {}",
                connector.backend_name()
            );
        }
        all_success
    }

    /// Try to connect specific ports to SuperCollider inputs.
    ///
    /// Uses AudioConnector with fuzzy matching support for better UX.
    /// Supports both PipeWire (pw-link) and JACK (jack_connect) backends.
    #[cfg(unix)]
    fn try_connect_inputs_from_ports(&self, client_name: &str, ports: &[String]) -> bool {
        let connector = match AudioConnector::detect() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("No audio backend available: {}", e);
                return false;
            }
        };

        tracing::debug!("Using audio backend: {}", connector.backend_name());

        let mut all_success = true;

        for (i, pattern) in ports.iter().enumerate() {
            let dst_port = format!("{}:in_{}", client_name, i + 1);

            // Use the connector's fuzzy matching to resolve the port pattern
            match connector.resolve_port(pattern, PortDirection::Output) {
                Ok(resolved_port) => match connector.connect(&resolved_port, &dst_port) {
                    Ok(()) => {
                        tracing::debug!("Connected {} -> {}", resolved_port, dst_port);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to connect {} -> {}: {}",
                            resolved_port,
                            dst_port,
                            e
                        );
                        all_success = false;
                    }
                },
                Err(e) => {
                    tracing::warn!("Could not resolve port pattern '{}': {}", pattern, e);
                    all_success = false;
                }
            }
        }

        if all_success {
            tracing::info!(
                "Input ports connected successfully via {}",
                connector.backend_name()
            );
        }
        all_success
    }

    /// Try to connect using PipeWire's pw-link command.
    #[cfg(unix)]
    fn try_pipewire_connect(&self) -> bool {
        // First, discover available output ports using pw-link -o
        let output = match Command::new("pw-link").arg("-o").output() {
            Ok(o) if o.status.success() => o,
            _ => return false,
        };

        let outputs = String::from_utf8_lossy(&output.stdout);

        // Find SuperCollider output ports
        let sc_out_1 = outputs.lines().find(|l| l.contains("SuperCollider:out_1"));
        let sc_out_2 = outputs.lines().find(|l| l.contains("SuperCollider:out_2"));

        if sc_out_1.is_none() || sc_out_2.is_none() {
            tracing::debug!("SuperCollider JACK ports not found yet");
            return false;
        }

        // Discover input (playback) ports using pw-link -i
        let input = match Command::new("pw-link").arg("-i").output() {
            Ok(o) if o.status.success() => o,
            _ => return false,
        };

        let inputs = String::from_utf8_lossy(&input.stdout);

        // Look for a suitable audio output - prefer headphones, then speakers, then any sink
        let (playback_l, playback_r) = self.find_best_audio_output(&inputs);

        if let (Some(left), Some(right)) = (playback_l, playback_r) {
            tracing::info!(
                "Auto-connecting SuperCollider to: {}",
                left.split(':').next().unwrap_or(&left)
            );

            // Connect left channel
            let result_l = Command::new("pw-link")
                .args(["SuperCollider:out_1", &left])
                .output();

            // Connect right channel
            let result_r = Command::new("pw-link")
                .args(["SuperCollider:out_2", &right])
                .output();

            match (result_l, result_r) {
                (Ok(l), Ok(r)) if l.status.success() && r.status.success() => {
                    tracing::info!("JACK audio ports connected successfully");
                    return true;
                }
                (Ok(l), Ok(r)) => {
                    // Check if already connected (not an error)
                    let l_err = String::from_utf8_lossy(&l.stderr);
                    let r_err = String::from_utf8_lossy(&r.stderr);
                    if l_err.contains("already") || r_err.contains("already") {
                        tracing::debug!("JACK ports already connected");
                        return true;
                    }
                    tracing::debug!("pw-link failed: L={} R={}", l_err.trim(), r_err.trim());
                }
                _ => {
                    tracing::debug!("pw-link command failed to execute");
                }
            }
        }

        false
    }

    /// Find the best audio output from available PipeWire/JACK inputs.
    ///
    /// Priority: Headphones > Speakers > HDMI > Any sink
    #[cfg(unix)]
    fn find_best_audio_output(&self, inputs: &str) -> (Option<String>, Option<String>) {
        let lines: Vec<&str> = inputs.lines().collect();

        // Priority patterns to search for (in order)
        let patterns = [
            ("Headphones", "playback_FL", "playback_FR"),
            ("Speaker", "playback_FL", "playback_FR"),
            ("Analog", "playback_FL", "playback_FR"),
            ("HDMI", "playback_FL", "playback_FR"),
            ("sink", "playback_FL", "playback_FR"),
        ];

        for (device_pattern, left_suffix, right_suffix) in patterns {
            // Find matching device
            for line in &lines {
                if line.contains(device_pattern) && line.contains(left_suffix) {
                    // Found left channel, look for matching right channel
                    let base = line.trim_end_matches(left_suffix);
                    let right_port = format!("{}{}", base, right_suffix);

                    if lines.iter().any(|l| l.trim() == right_port) {
                        return (Some(line.trim().to_string()), Some(right_port));
                    }
                }
            }
        }

        // Fallback: find any playback_FL/FR pair
        for line in &lines {
            if line.contains("playback_FL") && !line.contains("SuperCollider") {
                let left = line.trim().to_string();
                let right = left.replace("playback_FL", "playback_FR");
                if lines.iter().any(|l| l.trim() == right) {
                    return (Some(left), Some(right));
                }
            }
        }

        (None, None)
    }

    /// Try to connect using JACK's jack_connect command.
    #[cfg(unix)]
    fn try_jack_connect(&self) -> bool {
        // List JACK ports
        let output = match Command::new("jack_lsp").output() {
            Ok(o) if o.status.success() => o,
            _ => return false,
        };

        let ports = String::from_utf8_lossy(&output.stdout);

        // Check if SuperCollider ports exist
        if !ports.contains("SuperCollider:out_1") {
            return false;
        }

        // Find system playback ports (standard JACK naming)
        let system_l = ports
            .lines()
            .find(|l| l.contains("system:playback_1") || l.contains("playback_FL"));
        let system_r = ports
            .lines()
            .find(|l| l.contains("system:playback_2") || l.contains("playback_FR"));

        if let (Some(left), Some(right)) = (system_l, system_r) {
            tracing::info!("Auto-connecting SuperCollider via JACK to: {}", left);

            let result_l = Command::new("jack_connect")
                .args(["SuperCollider:out_1", left.trim()])
                .output();

            let result_r = Command::new("jack_connect")
                .args(["SuperCollider:out_2", right.trim()])
                .output();

            match (result_l, result_r) {
                (Ok(l), Ok(r)) if l.status.success() && r.status.success() => {
                    tracing::info!("JACK audio ports connected successfully");
                    return true;
                }
                _ => {
                    tracing::debug!("jack_connect failed");
                }
            }
        }

        false
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
