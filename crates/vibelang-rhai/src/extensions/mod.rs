//! Optional Rhai extensions for VibeLang.
//!
//! These extensions provide additional functionality that users can opt-in to.
//! Each extension is feature-gated and must be explicitly enabled.
//!
//! # Available Extensions
//!
//! - `fs` - Filesystem access (read_file, write_file, etc.)
//! - `exec` - Shell command execution
//! - `net` - Networking (HTTP fetch)
//!
//! # Security Considerations
//!
//! These extensions provide powerful capabilities that can affect the system.
//! They are disabled by default and should only be enabled in trusted environments.
//!
//! # Example Usage
//!
//! Enable extensions in Cargo.toml:
//! ```toml
//! [dependencies]
//! vibelang-rhai = { version = "0.1", features = ["ext-fs", "ext-exec", "ext-net"] }
//! ```
//!
//! Or enable all extensions:
//! ```toml
//! [dependencies]
//! vibelang-rhai = { version = "0.1", features = ["extensions"] }
//! ```

use rhai::Engine;

// Feature-gated extension modules
#[cfg(feature = "ext-fs")]
pub mod fs;

#[cfg(feature = "ext-exec")]
pub mod exec;

#[cfg(feature = "ext-net")]
pub mod net;

/// Extension configuration for the script engine.
#[derive(Debug, Clone, Default)]
pub struct ExtensionConfig {
    /// Enable filesystem extension (read_file, write_file, etc.)
    pub filesystem: bool,
    /// Enable exec extension (run shell commands)
    pub exec: bool,
    /// Enable networking extension (HTTP fetch)
    pub networking: bool,
    /// Base directory for filesystem operations (sandboxing).
    /// If set, all file operations are restricted to this directory.
    pub fs_base_path: Option<String>,
}

impl ExtensionConfig {
    /// Create a new configuration with all extensions disabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable all available extensions.
    pub fn enable_all() -> Self {
        Self {
            filesystem: true,
            exec: true,
            networking: true,
            fs_base_path: None,
        }
    }

    /// Enable filesystem extension.
    pub fn with_filesystem(mut self) -> Self {
        self.filesystem = true;
        self
    }

    /// Enable exec extension.
    pub fn with_exec(mut self) -> Self {
        self.exec = true;
        self
    }

    /// Enable networking extension.
    pub fn with_networking(mut self) -> Self {
        self.networking = true;
        self
    }

    /// Set base path for filesystem sandboxing.
    pub fn with_fs_base_path(mut self, path: impl Into<String>) -> Self {
        self.fs_base_path = Some(path.into());
        self
    }
}

/// Register all enabled extensions with the Rhai engine.
pub fn register_extensions(engine: &mut Engine, config: &ExtensionConfig) {
    #[cfg(feature = "ext-fs")]
    if config.filesystem {
        fs::register(engine, config.fs_base_path.as_deref());
    }

    #[cfg(feature = "ext-exec")]
    if config.exec {
        exec::register(engine);
    }

    #[cfg(feature = "ext-net")]
    if config.networking {
        net::register(engine);
    }

    // Log which extensions are enabled
    let mut enabled = Vec::new();

    #[cfg(feature = "ext-fs")]
    if config.filesystem {
        enabled.push("fs");
    }

    #[cfg(feature = "ext-exec")]
    if config.exec {
        enabled.push("exec");
    }

    #[cfg(feature = "ext-net")]
    if config.networking {
        enabled.push("net");
    }

    if !enabled.is_empty() {
        tracing::info!("Registered Rhai extensions: {}", enabled.join(", "));
    }
}

/// Check if any extensions are available (compiled in).
pub fn has_extensions() -> bool {
    cfg!(any(
        feature = "ext-fs",
        feature = "ext-exec",
        feature = "ext-net"
    ))
}

/// List available (compiled-in) extensions.
#[allow(clippy::vec_init_then_push)] // Conditional cfg attributes require this pattern
pub fn list_available_extensions() -> Vec<&'static str> {
    let mut list = Vec::new();

    #[cfg(feature = "ext-fs")]
    list.push("fs");

    #[cfg(feature = "ext-exec")]
    list.push("exec");

    #[cfg(feature = "ext-net")]
    list.push("net");

    list
}
