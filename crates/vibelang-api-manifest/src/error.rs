use std::fmt;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorCode {
    InvalidSchema,
    UnsupportedSchemaVersion,
    JsonDecode,
    TomlDecode,
    UnknownEnum,
    UnknownField,
    MissingFacet,
    DuplicateId,
    DuplicateOwner,
    OrphanId,
    MechanicalFactRestatement,
    InvalidStableId,
    AliasConflict,
    InvalidCanonicalJson,
    InvalidCounter,
    UnclassifiedDiff,
    InvalidReference,
    NonDeterministicOrder,
    InvalidValue,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ManifestError {
    pub code: ErrorCode,
    pub path: String,
    pub message: String,
}

impl ManifestError {
    pub fn new(code: ErrorCode, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
        }
    }

    pub(crate) fn decode(
        default_code: ErrorCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let message = message.into();
        let code = if message.contains("unknown variant") {
            ErrorCode::UnknownEnum
        } else if message.contains("unknown field") {
            ErrorCode::UnknownField
        } else if message.contains("missing field") {
            ErrorCode::MissingFacet
        } else {
            default_code
        };
        Self::new(code, path, message)
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} at {}: {}", self.code, self.path, self.message)
    }
}

impl std::error::Error for ManifestError {}
