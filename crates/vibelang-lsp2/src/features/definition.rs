//! Go-to-definition provider for VibeLang.
//!
//! Supports:
//! - Import statements -> jump to imported file
//! - Variable references -> jump to variable definition

use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

use crate::analysis::{ImportInfo, VariableDef};

/// Get definition for an import statement.
pub fn get_import_definition(
    imports: &[ImportInfo],
    position: Position,
) -> Option<GotoDefinitionResponse> {
    for import in imports {
        if position.line == import.range.start.line
            && position.character >= import.range.start.character
            && position.character <= import.range.end.character
        {
            if let Some(ref resolved_path) = import.resolved_path {
                if let Ok(uri) = Url::from_file_path(resolved_path) {
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
    }
    None
}

/// Get definition for a variable reference.
pub fn get_variable_definition(
    variable_defs: &[VariableDef],
    uri: &Url,
    word: &str,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    // Find the closest definition before the current position
    let mut best_match: Option<&VariableDef> = None;

    for def in variable_defs {
        if def.name == word {
            // Only consider definitions before or at the current position
            if def.range.start.line < position.line
                || (def.range.start.line == position.line
                    && def.range.start.character <= position.character)
            {
                // Update best match if this is closer to the current position
                if best_match.is_none()
                    || def.range.start.line > best_match.unwrap().range.start.line
                {
                    best_match = Some(def);
                }
            }
        }
    }

    best_match.map(|def| {
        GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: def.range,
        })
    })
}

/// Get definition for a voice/pattern/melody name.
pub fn get_entity_definition(
    content: &str,
    uri: &Url,
    name: &str,
    entity_type: &str,
) -> Option<GotoDefinitionResponse> {
    let pattern = match entity_type {
        "voice" => format!(r#"voice\s*\(\s*["']{}["']"#, regex::escape(name)),
        "pattern" => format!(r#"pattern\s*\(\s*["']{}["']"#, regex::escape(name)),
        "melody" => format!(r#"melody\s*\(\s*["']{}["']"#, regex::escape(name)),
        "sequence" => format!(r#"sequence\s*\(\s*["']{}["']"#, regex::escape(name)),
        "group" => format!(r#"define_group\s*\(\s*["']{}["']"#, regex::escape(name)),
        _ => return None,
    };

    let re = regex::Regex::new(&pattern).ok()?;

    for (line_num, line) in content.lines().enumerate() {
        if let Some(m) = re.find(line) {
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line: line_num as u32,
                        character: m.start() as u32,
                    },
                    end: Position {
                        line: line_num as u32,
                        character: m.end() as u32,
                    },
                },
            }));
        }
    }

    None
}
