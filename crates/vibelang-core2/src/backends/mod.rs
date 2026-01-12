//! Backend implementations for different synthesis engines.
//!
//! This module contains concrete implementations of the [`Backend`] trait:
//!
//! - [`scsynth::ScsynthBackend`] - SuperCollider synthesis server (native only)
//! - [`scsynth_process::ScsynthProcess`] - Manages the scsynth child process
//! - [`web_scsynth::WebScsynthBackend`] - SuperSonic (scsynth WASM) backend
//!
//! # Example (Native)
//!
//! ```ignore
//! use vibelang_core2::backends::{ScsynthBackend, ScsynthProcess, ScsynthConfig};
//! use vibelang_core2::Runtime;
//!
//! // Start scsynth process
//! let process = ScsynthProcess::spawn(ScsynthConfig::default())?;
//! process.wait_startup(Duration::from_secs(2)).await;
//!
//! // Connect to it
//! let backend = ScsynthBackend::connect(&process.addr()).await?;
//! let runtime = Runtime::new(backend);
//! ```
//!
//! # Example (WASM)
//!
//! ```ignore
//! use vibelang_core2::backends::WebScsynthBackend;
//! use vibelang_core2::Runtime;
//!
//! // Create web backend (requires JavaScript SuperSonic setup)
//! let backend = WebScsynthBackend::new();
//! backend.init().await?;
//! let runtime = Runtime::new(backend);
//! ```

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod scsynth;

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod scsynth_process;

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub use scsynth::{
    setup_metering, setup_node_tracking, OscCallback, OscResponse, ScsynthBackend, ScsynthError,
};

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub use scsynth_process::{ProcessError, ScsynthConfig, ScsynthProcess};

// WASM backend
#[cfg(target_arch = "wasm32")]
pub mod web_scsynth;

#[cfg(target_arch = "wasm32")]
pub use web_scsynth::{WebScsynthBackend, WebScsynthError};
