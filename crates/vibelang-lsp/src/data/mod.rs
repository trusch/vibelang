//! Data structures and caching for the LSP.
//!
//! This module provides:
//! - Document store for managing open files
//! - UGen manifest cache
//! - Synthdef information cache

mod document_store;
mod ugen_cache;

pub use document_store::DocumentStore;
pub use ugen_cache::{
    format_ugen_hover, get_all_ugen_functions, get_ugen_cache, get_ugen_completions, get_ugen_doc,
    get_ugen_signature_help, init_ugen_cache, to_snake_case, UGenDefinition, UGenInput,
};

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Synthdef information for hover and completion.
#[derive(Debug, Clone)]
pub struct SynthdefInfo {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Vec<ParamInfo>,
    pub category: Option<String>,
}

/// Parameter information.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub default: f64,
    pub description: Option<String>,
}

/// Manifest-generated API function documentation.
#[derive(Debug, Clone)]
pub struct ApiFunctionDoc {
    pub name: String,
    pub signature: String,
    pub description: String,
    pub example: String,
    pub parameters: Vec<(String, String, String)>,
}

#[derive(Deserialize)]
struct GeneratedApiRow {
    name: String,
    description: String,
    signature: String,
    example: String,
    receiver: Option<String>,
}

/// Static cache for API documentation generated from the canonical manifest.
static API_DOCS: OnceLock<HashMap<String, ApiFunctionDoc>> = OnceLock::new();

/// Get manifest-backed global API function documentation.
pub fn get_api_docs() -> &'static HashMap<String, ApiFunctionDoc> {
    API_DOCS.get_or_init(|| {
        let rows: Vec<GeneratedApiRow> = serde_json::from_str(include_str!("rhai-api.json"))
            .expect("generated LSP Rhai API metadata must be valid JSON");
        let mut docs = HashMap::<String, ApiFunctionDoc>::new();
        for row in rows {
            if row.receiver.is_some() || !is_public_identifier(&row.name) {
                continue;
            }
            let doc = docs
                .entry(row.name.clone())
                .or_insert_with(|| ApiFunctionDoc {
                    name: row.name,
                    signature: String::new(),
                    description: row.description,
                    example: row.example,
                    parameters: Vec::new(),
                });
            if doc.signature.is_empty() {
                doc.signature = row.signature;
            } else if !doc.signature.split(" | ").any(|part| part == row.signature) {
                doc.signature.push_str(" | ");
                doc.signature.push_str(&row.signature);
            }
        }
        docs
    })
}

/// Get documentation for a specific API function.
pub fn get_api_function_doc(name: &str) -> Option<&'static ApiFunctionDoc> {
    get_api_docs().get(name)
}

fn is_public_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    matches!(characters.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_api_docs_expose_numeric_quantization_and_real_sample_constructor() {
        let quantization = get_api_function_doc("set_quantization").unwrap();
        assert!(quantization.signature.contains("f64"));
        assert!(quantization.signature.contains("i64"));
        assert!(!quantization.signature.contains("string"));

        assert!(get_api_function_doc("sample").is_some());
    }
}
