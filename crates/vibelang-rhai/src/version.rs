//! Language-major selection for VibeLang source and imports.

use thiserror::Error;

pub const V2_DIRECTIVE: &str = "// vibe-api: 2";

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LanguageVersion {
    V1,
    V2,
}

impl LanguageVersion {
    #[must_use]
    pub const fn major(self) -> u16 {
        match self {
            Self::V1 => 1,
            Self::V2 => 2,
        }
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LanguageSelectionError {
    #[error("invalid VibeLang API directive on line {line}: expected exact `{V2_DIRECTIVE}`")]
    InvalidDirective { line: usize },
    #[error("the VibeLang API directive must appear before executable source (line {line})")]
    MisplacedDirective { line: usize },
    #[error("the VibeLang API directive may appear only once (line {line})")]
    DuplicateDirective { line: usize },
    #[error(
        "cross-major import rejected: importer uses vibe-api {importer}, module declares vibe-api {module}"
    )]
    CrossMajorImport { importer: u16, module: u16 },
    #[error("vibe-api 2 support is disabled for this ScriptEngine")]
    V2Disabled,
    #[error("vibe-api 2 source requires the versioned evaluate or evaluate_file entry point")]
    V2RequiresVersionedEntryPoint,
}

pub fn select_language(source: &str) -> Result<LanguageVersion, LanguageSelectionError> {
    Ok(explicit_language(source)?.unwrap_or(LanguageVersion::V1))
}

pub fn select_import_language(
    source: &str,
    importer: LanguageVersion,
) -> Result<LanguageVersion, LanguageSelectionError> {
    let selected = explicit_language(source)?.unwrap_or(importer);
    if selected != importer {
        return Err(LanguageSelectionError::CrossMajorImport {
            importer: importer.major(),
            module: selected.major(),
        });
    }
    Ok(selected)
}

fn explicit_language(source: &str) -> Result<Option<LanguageVersion>, LanguageSelectionError> {
    let mut selected = None;
    let mut executable_source_seen = false;

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let raw_line = if index == 0 {
            raw_line.strip_prefix('\u{feff}').unwrap_or(raw_line)
        } else {
            raw_line
        };
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("//") && line.contains("vibe-api") {
            if line != V2_DIRECTIVE {
                return Err(LanguageSelectionError::InvalidDirective { line: line_number });
            }
            if executable_source_seen {
                return Err(LanguageSelectionError::MisplacedDirective { line: line_number });
            }
            if selected.replace(LanguageVersion::V2).is_some() {
                return Err(LanguageSelectionError::DuplicateDirective { line: line_number });
            }
            continue;
        }

        if !line.starts_with("//") {
            executable_source_seen = true;
        }
    }

    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unversioned_source_is_v1_and_exact_directive_selects_v2() {
        assert_eq!(
            select_language("set_tempo(120);").unwrap(),
            LanguageVersion::V1
        );
        assert_eq!(
            select_language("\n// comment\n  // vibe-api: 2\nlet x = 1;").unwrap(),
            LanguageVersion::V2
        );
    }

    #[test]
    fn near_miss_misplaced_and_duplicate_directives_are_rejected() {
        assert!(matches!(
            select_language("// vibe-api:2\nlet x = 1;"),
            Err(LanguageSelectionError::InvalidDirective { .. })
        ));
        assert!(matches!(
            select_language("let x = 1;\n// vibe-api: 2"),
            Err(LanguageSelectionError::MisplacedDirective { .. })
        ));
        assert!(matches!(
            select_language("// vibe-api: 2\n// vibe-api: 2"),
            Err(LanguageSelectionError::DuplicateDirective { .. })
        ));
    }

    #[test]
    fn imports_inherit_the_importer_and_reject_explicit_cross_major_source() {
        assert_eq!(
            select_import_language("fn helper() {}", LanguageVersion::V2).unwrap(),
            LanguageVersion::V2
        );
        assert!(matches!(
            select_import_language(V2_DIRECTIVE, LanguageVersion::V1),
            Err(LanguageSelectionError::CrossMajorImport {
                importer: 1,
                module: 2,
            })
        ));
    }
}
