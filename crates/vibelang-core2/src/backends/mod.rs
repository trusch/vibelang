//! Backend implementations for different synthesis engines.
//!
//! This module contains concrete implementations of the [`Backend`] trait:
//!
//! - [`scsynth::ScsynthBackend`] - SuperCollider synthesis server (native only)
//! - [`scsynth_process::ScsynthProcess`] - Manages the scsynth child process
//!
//! # Example
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

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod scsynth;

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod scsynth_process;

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub use scsynth::{OscCallback, OscResponse, ScsynthBackend, ScsynthError};

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub use scsynth_process::{ProcessError, ScsynthConfig, ScsynthProcess};
