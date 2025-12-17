//! Language Server Protocol implementation for VibeLang.
//!
//! This module provides a full-featured LSP server that can be used by
//! any editor supporting the Language Server Protocol.
//!
//! Features:
//! - Syntax error diagnostics (via Rhai compilation)
//! - Unknown synthdef/effect validation
//! - Auto-completion for API functions, synthdefs, and stdlib imports
//! - Hover documentation
//! - Go-to-definition for imports
//! - Auto-import suggestions

mod backend;
mod completion;
mod diagnostics;
mod document;
mod hover;
mod definition;
mod analysis;

pub use backend::VibeLangServer;

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
