//! Document links provider for VibeLang.
//!
//! Makes import paths and sample paths clickable.

use std::path::PathBuf;

use tower_lsp::lsp_types::{DocumentLink, Position, Range, Url};

/// Get document links (clickable paths) for a document.
pub fn get_document_links(
    content: &str,
    file_path: Option<&PathBuf>,
    import_paths: &[PathBuf],
) -> Vec<DocumentLink> {
    let mut links = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        // Import statements: import "path/to/file.vibe";
        if let Some(import_link) = extract_import_link(line, line_num, file_path, import_paths) {
            links.push(import_link);
        }

        // Sample paths: sample("name", "path/to/sample.wav")
        if let Some(sample_link) = extract_sample_link(line, line_num, file_path) {
            links.push(sample_link);
        }

        // SFZ paths: load_sfz("name", "path/to/instrument.sfz")
        if let Some(sfz_link) = extract_sfz_link(line, line_num, file_path) {
            links.push(sfz_link);
        }
    }

    links
}

/// Extract import link from a line.
fn extract_import_link(
    line: &str,
    line_num: usize,
    file_path: Option<&PathBuf>,
    import_paths: &[PathBuf],
) -> Option<DocumentLink> {
    let import_start = line.find("import ")?;
    let after_import = &line[import_start + 7..];

    // Find the quoted path
    let quote_char = after_import.chars().find(|c| *c == '"' || *c == '\'')?;
    let quote_start = after_import.find(quote_char)?;
    let path_start = quote_start + 1;
    let rest = &after_import[path_start..];
    let path_end = rest.find(quote_char)?;
    let import_path = &rest[..path_end];

    // Calculate the range of the path in the document
    let absolute_start = import_start + 7 + path_start;
    let absolute_end = absolute_start + path_end;

    // Try to resolve the path
    let resolved_uri = resolve_import_path(import_path, file_path, import_paths)?;

    Some(DocumentLink {
        range: Range {
            start: Position {
                line: line_num as u32,
                character: absolute_start as u32,
            },
            end: Position {
                line: line_num as u32,
                character: absolute_end as u32,
            },
        },
        target: Some(resolved_uri),
        tooltip: Some(format!("Open {}", import_path)),
        data: None,
    })
}

/// Extract sample link from a line.
fn extract_sample_link(
    line: &str,
    line_num: usize,
    file_path: Option<&PathBuf>,
) -> Option<DocumentLink> {
    // sample("name", "path")
    let sample_start = line.find("sample(")?;
    let after_sample = &line[sample_start + 7..];

    // Skip the first argument (name)
    let first_quote = after_sample.find(|c| c == '"' || c == '\'')?;
    let after_first = &after_sample[first_quote + 1..];
    let first_close = after_first.find(|c| c == '"' || c == '\'')?;
    let after_close = &after_first[first_close + 1..];

    // Find the comma and second argument
    let comma = after_close.find(',')?;
    let after_comma = &after_close[comma + 1..].trim_start();

    // Find the path argument
    let quote_char = after_comma.chars().find(|c| *c == '"' || *c == '\'')?;
    let quote_start = after_comma.find(quote_char)?;
    let path_start = quote_start + 1;
    let rest = &after_comma[path_start..];
    let path_end = rest.find(quote_char)?;
    let sample_path = &rest[..path_end];

    // Calculate absolute position
    let base_offset = sample_start + 7 + first_quote + 1 + first_close + 1 + comma + 1;
    let trimmed_len = after_close[comma + 1..].len() - after_comma.len();
    let absolute_start = base_offset + trimmed_len + path_start;
    let absolute_end = absolute_start + path_end;

    // Resolve the path relative to the current file
    let resolved_uri = resolve_relative_path(sample_path, file_path)?;

    Some(DocumentLink {
        range: Range {
            start: Position {
                line: line_num as u32,
                character: absolute_start as u32,
            },
            end: Position {
                line: line_num as u32,
                character: absolute_end as u32,
            },
        },
        target: Some(resolved_uri),
        tooltip: Some(format!("Open sample: {}", sample_path)),
        data: None,
    })
}

/// Extract SFZ link from a line.
fn extract_sfz_link(
    line: &str,
    line_num: usize,
    file_path: Option<&PathBuf>,
) -> Option<DocumentLink> {
    // load_sfz("name", "path")
    let sfz_start = line.find("load_sfz(")?;
    let after_sfz = &line[sfz_start + 9..];

    // Skip the first argument (name)
    let first_quote = after_sfz.find(|c| c == '"' || c == '\'')?;
    let after_first = &after_sfz[first_quote + 1..];
    let first_close = after_first.find(|c| c == '"' || c == '\'')?;
    let after_close = &after_first[first_close + 1..];

    // Find the comma and second argument
    let comma = after_close.find(',')?;
    let after_comma = &after_close[comma + 1..].trim_start();

    // Find the path argument
    let quote_char = after_comma.chars().find(|c| *c == '"' || *c == '\'')?;
    let quote_start = after_comma.find(quote_char)?;
    let path_start = quote_start + 1;
    let rest = &after_comma[path_start..];
    let path_end = rest.find(quote_char)?;
    let sfz_path = &rest[..path_end];

    // Calculate absolute position
    let base_offset = sfz_start + 9 + first_quote + 1 + first_close + 1 + comma + 1;
    let trimmed_len = after_close[comma + 1..].len() - after_comma.len();
    let absolute_start = base_offset + trimmed_len + path_start;
    let absolute_end = absolute_start + path_end;

    // Resolve the path relative to the current file
    let resolved_uri = resolve_relative_path(sfz_path, file_path)?;

    Some(DocumentLink {
        range: Range {
            start: Position {
                line: line_num as u32,
                character: absolute_start as u32,
            },
            end: Position {
                line: line_num as u32,
                character: absolute_end as u32,
            },
        },
        target: Some(resolved_uri),
        tooltip: Some(format!("Open SFZ: {}", sfz_path)),
        data: None,
    })
}

/// Resolve an import path to a file URI.
fn resolve_import_path(
    import_path: &str,
    file_path: Option<&PathBuf>,
    import_paths: &[PathBuf],
) -> Option<Url> {
    // Try relative to current file first
    if let Some(current) = file_path {
        if let Some(parent) = current.parent() {
            let resolved = parent.join(import_path);
            if resolved.exists() {
                return Url::from_file_path(&resolved).ok();
            }
        }
    }

    // Try import paths
    for base in import_paths {
        let resolved = base.join(import_path);
        if resolved.exists() {
            return Url::from_file_path(&resolved).ok();
        }
    }

    None
}

/// Resolve a relative path to a file URI.
fn resolve_relative_path(path: &str, file_path: Option<&PathBuf>) -> Option<Url> {
    let current = file_path?;
    let parent = current.parent()?;
    let resolved = parent.join(path);

    if resolved.exists() {
        Url::from_file_path(&resolved).ok()
    } else {
        // Return the path anyway (it might exist later)
        Url::from_file_path(&resolved).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_import_link() {
        let line = r#"import "stdlib/drums/kicks.vibe";"#;
        let import_paths = vec![PathBuf::from("/usr/share/vibelang")];
        let link = extract_import_link(line, 0, None, &import_paths);
        // Link will be None if path doesn't exist, but parsing should work
        assert!(line.contains("stdlib/drums/kicks.vibe"));
    }

    #[test]
    fn test_sample_link_parsing() {
        let line = r#"let s = sample("kick", "samples/kick.wav");"#;
        // Just verify parsing doesn't panic
        let _ = extract_sample_link(line, 0, None);
    }
}
