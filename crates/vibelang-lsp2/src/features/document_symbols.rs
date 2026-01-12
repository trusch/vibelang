//! Document symbols provider for VibeLang.
//!
//! Provides outline view with hierarchical symbols.

#![allow(deprecated)] // DocumentSymbol.deprecated field is deprecated

use tower_lsp::lsp_types::{DocumentSymbol, Position, Range, SymbolKind};

use crate::analysis::AnalysisResult;

/// Get document symbols for outline view.
pub fn get_document_symbols(
    content: &str,
    analysis: Option<&AnalysisResult>,
) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    // Transport settings
    let mut transport_children = Vec::new();
    extract_transport_symbols(content, &mut transport_children);
    if !transport_children.is_empty() {
        symbols.push(DocumentSymbol {
            name: "Transport".to_string(),
            detail: None,
            kind: SymbolKind::NAMESPACE,
            tags: None,
            deprecated: None,
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
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            children: Some(transport_children),
        });
    }

    // Groups
    if let Some(analysis) = analysis {
        let mut group_symbols = Vec::new();
        for group in &analysis.group_defs {
            if let Some(range) = find_entity_range(content, "define_group", group) {
                // Find voices and patterns inside this group
                let children = find_group_children(content, range.start.line);

                group_symbols.push(DocumentSymbol {
                    name: group.clone(),
                    detail: Some("group".to_string()),
                    kind: SymbolKind::MODULE,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                });
            }
        }
        if !group_symbols.is_empty() {
            symbols.push(DocumentSymbol {
                name: "Groups".to_string(),
                detail: None,
                kind: SymbolKind::NAMESPACE,
                tags: None,
                deprecated: None,
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
                selection_range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                children: Some(group_symbols),
            });
        }

        // Voices (not inside groups)
        let mut voice_symbols = Vec::new();
        for voice in &analysis.voice_defs {
            if let Some(range) = find_entity_range(content, "voice", voice) {
                voice_symbols.push(DocumentSymbol {
                    name: voice.clone(),
                    detail: Some("voice".to_string()),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                });
            }
        }
        if !voice_symbols.is_empty() {
            symbols.push(DocumentSymbol {
                name: "Voices".to_string(),
                detail: None,
                kind: SymbolKind::NAMESPACE,
                tags: None,
                deprecated: None,
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
                selection_range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                children: Some(voice_symbols),
            });
        }

        // Patterns
        let mut pattern_symbols = Vec::new();
        for pattern in &analysis.pattern_defs {
            if let Some(range) = find_entity_range(content, "pattern", pattern) {
                pattern_symbols.push(DocumentSymbol {
                    name: pattern.clone(),
                    detail: Some("pattern".to_string()),
                    kind: SymbolKind::EVENT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                });
            }
        }
        if !pattern_symbols.is_empty() {
            symbols.push(DocumentSymbol {
                name: "Patterns".to_string(),
                detail: None,
                kind: SymbolKind::NAMESPACE,
                tags: None,
                deprecated: None,
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
                selection_range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                children: Some(pattern_symbols),
            });
        }

        // Melodies
        let mut melody_symbols = Vec::new();
        for melody in &analysis.melody_defs {
            if let Some(range) = find_entity_range(content, "melody", melody) {
                melody_symbols.push(DocumentSymbol {
                    name: melody.clone(),
                    detail: Some("melody".to_string()),
                    kind: SymbolKind::EVENT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                });
            }
        }
        if !melody_symbols.is_empty() {
            symbols.push(DocumentSymbol {
                name: "Melodies".to_string(),
                detail: None,
                kind: SymbolKind::NAMESPACE,
                tags: None,
                deprecated: None,
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
                selection_range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                children: Some(melody_symbols),
            });
        }

        // Sequences
        let mut sequence_symbols = Vec::new();
        for sequence in &analysis.sequence_defs {
            if let Some(range) = find_entity_range(content, "sequence", sequence) {
                sequence_symbols.push(DocumentSymbol {
                    name: sequence.clone(),
                    detail: Some("sequence".to_string()),
                    kind: SymbolKind::STRUCT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                });
            }
        }
        if !sequence_symbols.is_empty() {
            symbols.push(DocumentSymbol {
                name: "Sequences".to_string(),
                detail: None,
                kind: SymbolKind::NAMESPACE,
                tags: None,
                deprecated: None,
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
                selection_range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                children: Some(sequence_symbols),
            });
        }

        // Synthdefs
        let mut synthdef_symbols = Vec::new();
        for synthdef in &analysis.local_synthdefs {
            if let Some(range) = find_entity_range(content, "define_synthdef", synthdef) {
                synthdef_symbols.push(DocumentSymbol {
                    name: synthdef.clone(),
                    detail: Some("synthdef".to_string()),
                    kind: SymbolKind::CLASS,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                });
            }
        }
        if !synthdef_symbols.is_empty() {
            symbols.push(DocumentSymbol {
                name: "Synthdefs".to_string(),
                detail: None,
                kind: SymbolKind::NAMESPACE,
                tags: None,
                deprecated: None,
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
                selection_range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                children: Some(synthdef_symbols),
            });
        }

        // Effects
        let mut effect_symbols = Vec::new();
        for effect in &analysis.local_effects {
            if let Some(range) = find_entity_range(content, "define_fx", effect) {
                effect_symbols.push(DocumentSymbol {
                    name: effect.clone(),
                    detail: Some("effect".to_string()),
                    kind: SymbolKind::CLASS,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                });
            }
        }
        if !effect_symbols.is_empty() {
            symbols.push(DocumentSymbol {
                name: "Effects".to_string(),
                detail: None,
                kind: SymbolKind::NAMESPACE,
                tags: None,
                deprecated: None,
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
                selection_range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                children: Some(effect_symbols),
            });
        }
    }

    symbols
}

/// Extract transport-related symbols (tempo, time signature, etc.)
fn extract_transport_symbols(content: &str, symbols: &mut Vec<DocumentSymbol>) {
    let patterns = [
        ("set_tempo", "tempo"),
        ("set_time_signature", "time signature"),
        ("set_quantization", "quantization"),
    ];

    for (pattern, detail) in patterns {
        let re = regex::Regex::new(&format!(r"{}.*", pattern)).ok();
        if let Some(re) = re {
            for (line_num, line) in content.lines().enumerate() {
                if let Some(m) = re.find(line) {
                    symbols.push(DocumentSymbol {
                        name: line[m.start()..m.end().min(line.len().min(m.start() + 30))]
                            .to_string(),
                        detail: Some(detail.to_string()),
                        kind: SymbolKind::PROPERTY,
                        tags: None,
                        deprecated: None,
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
                        selection_range: Range {
                            start: Position {
                                line: line_num as u32,
                                character: m.start() as u32,
                            },
                            end: Position {
                                line: line_num as u32,
                                character: m.end() as u32,
                            },
                        },
                        children: None,
                    });
                }
            }
        }
    }
}

/// Find the range of an entity definition.
fn find_entity_range(content: &str, entity_type: &str, name: &str) -> Option<Range> {
    let pattern = format!(r#"{}\s*\(\s*["']{}["']"#, entity_type, regex::escape(name));
    let re = regex::Regex::new(&pattern).ok()?;

    for (line_num, line) in content.lines().enumerate() {
        if let Some(m) = re.find(line) {
            // Try to find the end of the statement
            let end_line = find_statement_end(content, line_num);
            return Some(Range {
                start: Position {
                    line: line_num as u32,
                    character: m.start() as u32,
                },
                end: Position {
                    line: end_line as u32,
                    character: content.lines().nth(end_line).map(|l| l.len()).unwrap_or(0) as u32,
                },
            });
        }
    }

    None
}

/// Find the end line of a statement (looking for semicolon or closing brace).
fn find_statement_end(content: &str, start_line: usize) -> usize {
    let lines: Vec<&str> = content.lines().collect();
    let mut brace_depth: i32 = 0;
    let mut paren_depth: i32 = 0;

    for (i, line) in lines.iter().enumerate().skip(start_line) {
        for c in line.chars() {
            match c {
                '{' => brace_depth += 1,
                '}' => brace_depth = brace_depth.saturating_sub(1),
                '(' => paren_depth += 1,
                ')' => paren_depth = paren_depth.saturating_sub(1),
                ';' if brace_depth == 0 && paren_depth == 0 => return i,
                _ => {}
            }
        }

        // If we're back to depth 0 and find a closing brace at the start
        if brace_depth == 0 && paren_depth == 0 && i > start_line && line.trim().starts_with('}') {
            return i;
        }
    }

    start_line
}

/// Find children entities within a group.
fn find_group_children(content: &str, group_start_line: u32) -> Vec<DocumentSymbol> {
    let mut children = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    // Find the closure body of the group
    let mut brace_depth = 0;
    let mut in_group = false;

    for (line_num, line) in lines.iter().enumerate().skip(group_start_line as usize) {
        for c in line.chars() {
            if c == '{' {
                brace_depth += 1;
                in_group = true;
            } else if c == '}' {
                brace_depth -= 1;
                if brace_depth == 0 && in_group {
                    return children;
                }
            }
        }

        if in_group && brace_depth > 0 {
            // Look for voice/pattern/melody definitions
            let entities = [
                ("voice", SymbolKind::FUNCTION),
                ("pattern", SymbolKind::EVENT),
                ("melody", SymbolKind::EVENT),
                ("fx", SymbolKind::FUNCTION),
            ];

            for (entity_type, kind) in entities {
                let pattern = format!(r#"{}\s*\(\s*["']([^"']+)["']"#, entity_type);
                if let Ok(re) = regex::Regex::new(&pattern) {
                    for cap in re.captures_iter(line) {
                        if let Some(name_match) = cap.get(1) {
                            children.push(DocumentSymbol {
                                name: name_match.as_str().to_string(),
                                detail: Some(entity_type.to_string()),
                                kind,
                                tags: None,
                                deprecated: None,
                                range: Range {
                                    start: Position {
                                        line: line_num as u32,
                                        character: 0,
                                    },
                                    end: Position {
                                        line: line_num as u32,
                                        character: line.len() as u32,
                                    },
                                },
                                selection_range: Range {
                                    start: Position {
                                        line: line_num as u32,
                                        character: name_match.start() as u32,
                                    },
                                    end: Position {
                                        line: line_num as u32,
                                        character: name_match.end() as u32,
                                    },
                                },
                                children: None,
                            });
                        }
                    }
                }
            }
        }
    }

    children
}
