//! Go-to-definition provider for VibeLang.
//!
//! Provides go-to-definition for:
//! - Import statements (opens the imported file)
//! - Local variables (jumps to definition site)

use std::path::PathBuf;
use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

use crate::analysis::{ImportInfo, VariableDef};

/// Get definition location for an import.
pub fn get_import_definition(
    imports: &[ImportInfo],
    position: Position,
) -> Option<GotoDefinitionResponse> {
    // Find if the position is within an import statement
    for import in imports {
        if position.line == import.range.start.line
            && position.character >= import.range.start.character
            && position.character <= import.range.end.character
        {
            // Position is within this import
            if let Some(ref resolved_path) = import.resolved_path {
                let uri = path_to_uri(resolved_path)?;
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 0,
                        },
                    },
                }));
            }
        }
    }

    None
}

/// Get definition location for a variable at the given position.
pub fn get_variable_definition(
    variable_defs: &[VariableDef],
    document_uri: &Url,
    word: &str,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    // Find the variable definition that matches the word
    // and appears before the current position (definitions must come before usage)
    let mut best_match: Option<&VariableDef> = None;

    for def in variable_defs {
        if def.name == word {
            // Skip if this is the definition itself (user is on the definition)
            if def.range.start.line == position.line
                && position.character >= def.range.start.character
                && position.character <= def.range.end.character
            {
                // User is on the definition itself, don't jump anywhere
                continue;
            }

            // Definition must come before or at the usage position
            // (for simplicity, we take the most recent definition that appears before)
            if def.range.start.line < position.line
                || (def.range.start.line == position.line
                    && def.range.start.character < position.character)
            {
                // Take the closest definition before the usage
                match best_match {
                    Some(existing) if def.range.start.line > existing.range.start.line => {
                        best_match = Some(def);
                    }
                    Some(existing)
                        if def.range.start.line == existing.range.start.line
                            && def.range.start.character > existing.range.start.character =>
                    {
                        best_match = Some(def);
                    }
                    None => {
                        best_match = Some(def);
                    }
                    _ => {}
                }
            }
        }
    }

    best_match.map(|def| {
        GotoDefinitionResponse::Scalar(Location {
            uri: document_uri.clone(),
            range: def.range,
        })
    })
}

/// Convert a file path to a URI.
fn path_to_uri(path: &PathBuf) -> Option<Url> {
    Url::from_file_path(path).ok()
}
