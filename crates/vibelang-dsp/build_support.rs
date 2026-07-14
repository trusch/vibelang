//! Registration-shape rules shared by DSP code generation and API extraction.

use serde::Deserialize;

pub const MAX_POSITIONAL_ARITY: usize = 20;

#[derive(Debug, Clone, Deserialize)]
pub struct UGenManifest {
    pub name: String,
    pub description: String,
    pub rates: Vec<String>,
    pub inputs: Vec<UGenInput>,
    pub outputs: u32,
    pub category: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub functions: Option<Vec<String>>,
    /// Server-side UGen class to emit instead of `name`.
    #[serde(default)]
    pub ugen_class: Option<String>,
    /// Operator selector passed to the server. Defaults to zero.
    #[serde(default)]
    pub special_index: Option<i16>,
    /// Public shape argument removed from the encoded server inputs.
    #[serde(default)]
    #[allow(dead_code)]
    pub channel_count_input: Option<String>,
    /// Whether the name is an sclang helper, alias, or wrapper.
    #[serde(default)]
    pub pseudo: bool,
    /// SuperCollider plugin package required by this UGen.
    #[serde(default)]
    pub requires_plugin: Option<String>,
    /// Why a documentation-only entry cannot be generated.
    #[serde(default)]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UGenInput {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    pub description: String,
}

pub fn positional_arity_max(input_count: usize) -> usize {
    input_count.min(MAX_POSITIONAL_ARITY)
}

pub fn has_array_overload(input_count: usize) -> bool {
    input_count > MAX_POSITIONAL_ARITY
}

#[allow(dead_code)]
pub fn runtime_rate_rust(rate: &str) -> &'static str {
    match rate {
        "ar" => "Rate::Audio",
        "kr" => "Rate::Control",
        "ir" => "Rate::Scalar",
        _ => "Rate::Audio",
    }
}

#[allow(dead_code)]
pub fn runtime_rate_manifest(rate: &str) -> &'static str {
    match rate {
        "ar" => "audio",
        "kr" => "control",
        "ir" => "scalar",
        _ => "audio",
    }
}

pub fn to_snake_case(value: &str) -> String {
    if value == "DC" {
        return "dc".to_string();
    }
    let mut result = String::new();
    let chars: Vec<char> = value.chars().collect();
    for (index, &character) in chars.iter().enumerate() {
        if character.is_uppercase() {
            if index > 0 {
                let previous = chars[index - 1];
                let previous_lower = previous.is_lowercase();
                let next_lower = chars
                    .get(index + 1)
                    .map(|next| next.is_lowercase())
                    .unwrap_or(false);
                if (previous_lower || next_lower) && previous != '_' {
                    result.push('_');
                }
            }
            result.push(character.to_lowercase().next().unwrap());
        } else {
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_demand_fallback_remains_audio() {
        assert_eq!(runtime_rate_rust("demand"), "Rate::Audio");
        assert_eq!(runtime_rate_manifest("demand"), "audio");
    }

    #[test]
    fn arity_policy_matches_rhai_limit() {
        assert_eq!(positional_arity_max(24), 20);
        assert!(has_array_overload(24));
        assert!(!has_array_overload(20));
    }
}
