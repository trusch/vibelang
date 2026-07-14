//! Versioned, deterministic schema for VibeLang's public registration manifest.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_URI: &str = "https://vibelang.org/schemas/public-api-manifest/v1";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicApiManifest {
    pub schema: String,
    pub schema_version: u32,
    pub api_version: String,
    pub entries: Vec<ApiEntry>,
    pub stats: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiEntry {
    pub id: String,
    pub surface: String,
    pub kind: String,
    pub registered_name: String,
    pub aliases: Vec<String>,
    pub receiver: Option<String>,
    pub overloads: Vec<Overload>,
    pub availability: Availability,
    pub lifecycle: Lifecycle,
    pub source_anchors: Vec<Anchor>,
    pub test_anchors: Vec<Anchor>,
    pub details: EntryDetails,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overload {
    pub id: String,
    pub signature: String,
    pub aliases: Vec<String>,
    pub parameters: Vec<Parameter>,
    pub return_type: String,
    pub returns_receiver: Option<bool>,
    pub availability: Availability,
    pub source_anchors: Vec<Anchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub position: u32,
    pub name: Option<String>,
    pub accepted_types: Vec<String>,
    pub optional: bool,
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Availability {
    pub status: String,
    pub cfg: Vec<String>,
    pub targets: Vec<String>,
    pub features: Vec<String>,
    pub plugins: Vec<String>,
    pub runtime_conditions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lifecycle {
    pub phase: String,
    pub terminal: String,
    pub classification: String,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            phase: "unknown".into(),
            terminal: "unknown".into(),
            classification: "pending-p0.4".into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Anchor {
    pub path: String,
    pub symbol: String,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EntryDetails {
    Rhai {
        callable_identities: Vec<String>,
    },
    RhaiType {
        display_name: String,
    },
    Ugen {
        class: String,
        description: String,
        rate: String,
        runtime_rate: String,
        category: String,
        inputs: Vec<UgenInput>,
        outputs: u32,
        emitted_class: String,
        special_index: i16,
        pseudo: bool,
        callable: bool,
        requires_plugin: Option<String>,
        unavailable_reason: Option<String>,
    },
    StdlibDefinition {
        definition_kind: String,
        import_paths: Vec<String>,
        occurrences: Vec<Anchor>,
        export_classification: String,
        support_classification: String,
    },
    StdlibFunction {
        import_paths: Vec<String>,
        access: String,
        documentation: Vec<String>,
        export_classification: String,
        support_classification: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UgenInput {
    pub name: String,
    pub input_type: String,
    pub default: Option<serde_json::Value>,
    pub description: String,
}

pub fn stable_id(namespace: &str, canonical_key: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in canonical_key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("v1:{namespace}:{hash:016x}")
}

pub fn to_pretty_json(manifest: &PublicApiManifest) -> Result<String, serde_json::Error> {
    let mut json = serde_json::to_string_pretty(manifest)?;
    json.push('\n');
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ids_do_not_depend_on_process_state() {
        assert_eq!(
            stable_id("entry", "rhai|function|voice|Voice"),
            "v1:entry:f74e279a0ca9aa9f"
        );
        assert_ne!(stable_id("entry", "a"), stable_id("entry", "b"));
    }

    #[test]
    fn serialization_is_pretty_and_has_one_trailing_newline() {
        let manifest = PublicApiManifest {
            schema: SCHEMA_URI.into(),
            schema_version: SCHEMA_VERSION,
            api_version: "0.4.0".into(),
            entries: Vec::new(),
            stats: BTreeMap::new(),
        };
        let first = to_pretty_json(&manifest).unwrap();
        let second = to_pretty_json(&manifest).unwrap();
        assert_eq!(first, second);
        assert!(first.ends_with("}\n"));
        assert!(!first.ends_with("\n\n"));
    }
}
