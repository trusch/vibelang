//! Language Server Protocol implementation for VibeLang (core2-based).
//!
//! This crate provides a full-featured LSP server for VibeLang, built on top of
//! vibelang-core2 for modern, async validation.
//!
//! # Features
//!
//! - Syntax error diagnostics (via Rhai compilation)
//! - Semantic diagnostics (undefined synthdefs, voices, etc.)
//! - Auto-completion for API functions, synthdefs, and stdlib imports
//! - Hover documentation for API, UGens, and synthdefs
//! - Go-to-definition for imports and variables
//! - Signature help for function parameters
//! - Document symbols (outline view)
//! - Code actions (quick fixes)
//! - Semantic tokens (syntax highlighting)
//! - Inlay hints (inline annotations)
//!
//! # Usage
//!
//! ```ignore
//! use vibelang_lsp::run_lsp_server;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     run_lsp_server().await
//! }
//! ```

pub mod analysis;
pub mod data;
pub mod features;
pub mod server;
pub mod utils;
pub mod validation;

pub use server::VibeLangServer;

use tower_lsp::{LspService, Server};

/// Run the LSP server over stdio.
///
/// This function blocks until the client disconnects.
pub async fn run_lsp_server() -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(VibeLangServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
