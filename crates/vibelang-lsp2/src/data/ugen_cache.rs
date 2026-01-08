//! UGen manifest cache for hover documentation and completion.
//!
//! Embeds UGen manifests directly in the binary for reliable availability.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation,
    InsertTextFormat, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel,
    SignatureHelp, SignatureInformation,
};

/// UGen input definition from manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct UGenInput {
    pub name: String,
    #[serde(rename = "type")]
    pub input_type: String,
    #[serde(default)]
    pub default: f64,
    #[serde(default)]
    pub description: String,
}

/// UGen definition from manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct UGenDefinition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub rates: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<UGenInput>,
    #[serde(default)]
    pub outputs: i32,
    #[serde(default)]
    pub category: String,
}

/// Static cache for loaded UGen definitions.
static UGEN_CACHE: OnceLock<HashMap<String, UGenDefinition>> = OnceLock::new();

/// Embedded UGen manifests - compiled into the binary.
const EMBEDDED_MANIFESTS: &[(&str, &str)] = &[
    ("oscillators", include_str!("../../../vibelang-dsp/ugen_manifests/oscillators.json")),
    ("filters", include_str!("../../../vibelang-dsp/ugen_manifests/filters.json")),
    ("envelopes", include_str!("../../../vibelang-dsp/ugen_manifests/envelopes.json")),
    ("delays", include_str!("../../../vibelang-dsp/ugen_manifests/delays.json")),
    ("reverb", include_str!("../../../vibelang-dsp/ugen_manifests/reverb.json")),
    ("noise", include_str!("../../../vibelang-dsp/ugen_manifests/noise.json")),
    ("dynamics", include_str!("../../../vibelang-dsp/ugen_manifests/dynamics.json")),
    ("panning", include_str!("../../../vibelang-dsp/ugen_manifests/panning.json")),
    ("buffers", include_str!("../../../vibelang-dsp/ugen_manifests/buffers.json")),
    ("bufdelays", include_str!("../../../vibelang-dsp/ugen_manifests/bufdelays.json")),
    ("triggers", include_str!("../../../vibelang-dsp/ugen_manifests/triggers.json")),
    ("math", include_str!("../../../vibelang-dsp/ugen_manifests/math.json")),
    ("control", include_str!("../../../vibelang-dsp/ugen_manifests/control.json")),
    ("inout", include_str!("../../../vibelang-dsp/ugen_manifests/inout.json")),
    ("multichannel", include_str!("../../../vibelang-dsp/ugen_manifests/multichannel.json")),
    ("analysis", include_str!("../../../vibelang-dsp/ugen_manifests/analysis.json")),
    ("conversion", include_str!("../../../vibelang-dsp/ugen_manifests/conversion.json")),
    ("demand", include_str!("../../../vibelang-dsp/ugen_manifests/demand.json")),
    ("fft", include_str!("../../../vibelang-dsp/ugen_manifests/fft.json")),
    ("granular", include_str!("../../../vibelang-dsp/ugen_manifests/granular.json")),
    ("info", include_str!("../../../vibelang-dsp/ugen_manifests/info.json")),
    ("physical", include_str!("../../../vibelang-dsp/ugen_manifests/physical.json")),
    ("pitchtime", include_str!("../../../vibelang-dsp/ugen_manifests/pitchtime.json")),
    ("random", include_str!("../../../vibelang-dsp/ugen_manifests/random.json")),
];

/// Load UGen definitions from embedded manifests.
fn load_embedded_manifests() -> HashMap<String, UGenDefinition> {
    let mut ugens = HashMap::new();

    for (_category, content) in EMBEDDED_MANIFESTS {
        if let Ok(defs) = serde_json::from_str::<Vec<UGenDefinition>>(content) {
            for def in defs {
                // Skip documentation-only entries (builder rate)
                if def.rates.iter().all(|r| r == "builder") {
                    continue;
                }

                // Map both the original name and snake_case + rate variants
                for rate in &def.rates {
                    if rate == "builder" {
                        continue;
                    }
                    let func_name = format!("{}_{}", to_snake_case(&def.name), rate);
                    ugens.insert(func_name, def.clone());
                }
                // Also insert by original name (e.g., "SinOsc")
                ugens.insert(def.name.clone(), def.clone());
            }
        }
    }

    ugens
}

/// Convert PascalCase to snake_case.
pub fn to_snake_case(s: &str) -> String {
    if s == "DC" {
        return "dc".to_string();
    }
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                let prev_lower = chars[i - 1].is_lowercase();
                let next_lower = chars.get(i + 1).map(|ch| ch.is_lowercase()).unwrap_or(false);
                if prev_lower || next_lower {
                    result.push('_');
                }
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Initialize UGen cache (now uses embedded manifests).
pub fn init_ugen_cache(_workspace_root: Option<&std::path::Path>) {
    UGEN_CACHE.get_or_init(load_embedded_manifests);
}

/// Get the cached UGen definitions.
pub fn get_ugen_cache() -> &'static HashMap<String, UGenDefinition> {
    UGEN_CACHE.get_or_init(load_embedded_manifests)
}

/// Get documentation for a specific UGen.
pub fn get_ugen_doc(name: &str) -> Option<&'static UGenDefinition> {
    get_ugen_cache().get(name)
}

/// Get all UGen function names (e.g., sin_osc_ar, lpf_kr, etc.)
pub fn get_all_ugen_functions() -> Vec<&'static str> {
    get_ugen_cache()
        .keys()
        .filter(|k| k.ends_with("_ar") || k.ends_with("_kr") || k.ends_with("_ir"))
        .map(|s| s.as_str())
        .collect()
}

/// Get UGen completions for DSP body context.
pub fn get_ugen_completions() -> Vec<CompletionItem> {
    let cache = get_ugen_cache();
    let mut completions = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (func_name, ugen) in cache.iter() {
        // Only include rate-specific functions (skip PascalCase names)
        if !func_name.ends_with("_ar") && !func_name.ends_with("_kr") && !func_name.ends_with("_ir")
        {
            continue;
        }

        // Skip duplicates
        if !seen.insert(func_name.clone()) {
            continue;
        }

        let rate = if func_name.ends_with("_ar") {
            "audio rate"
        } else if func_name.ends_with("_kr") {
            "control rate"
        } else {
            "init rate"
        };

        // Build signature
        let params: Vec<String> = ugen
            .inputs
            .iter()
            .map(|i| format!("{}: {}", i.name, i.input_type))
            .collect();
        let signature = format!("{}({})", func_name, params.join(", "));

        // Build parameter snippet
        let snippet_params: Vec<String> = ugen
            .inputs
            .iter()
            .enumerate()
            .map(|(i, input)| format!("${{{}:{}}}", i + 1, input.default))
            .collect();
        let snippet = format!("{}({})$0", func_name, snippet_params.join(", "));

        // Build documentation
        let mut doc = format!("**{}** - {}\n\n", ugen.name, ugen.description);
        doc.push_str(&format!("**Category:** {} | **Rate:** {}\n\n", ugen.category, rate));

        if !ugen.inputs.is_empty() {
            doc.push_str("**Parameters:**\n");
            for input in &ugen.inputs {
                doc.push_str(&format!(
                    "- `{}` (default: {}) - {}\n",
                    input.name, input.default, input.description
                ));
            }
        }

        completions.push(CompletionItem {
            label: func_name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(format!(" → {} output(s)", ugen.outputs)),
                description: Some(ugen.category.clone()),
            }),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc,
            })),
            insert_text: Some(snippet),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            sort_text: Some(format!("0_{}", func_name)), // Sort UGens first
            ..Default::default()
        });
    }

    // Sort by name
    completions.sort_by(|a, b| a.label.cmp(&b.label));
    completions
}

/// Get signature help for a UGen function.
pub fn get_ugen_signature_help(func_name: &str, active_param: usize) -> Option<SignatureHelp> {
    let ugen = get_ugen_doc(func_name)?;

    // Build parameter information
    let parameters: Vec<ParameterInformation> = ugen
        .inputs
        .iter()
        .map(|input| ParameterInformation {
            label: ParameterLabel::Simple(input.name.clone()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(format!(
                "{} (default: {})",
                input.description, input.default
            ))),
        })
        .collect();

    // Build signature label
    let param_labels: Vec<String> = ugen.inputs.iter().map(|i| i.name.clone()).collect();
    let label = format!("{}({})", func_name, param_labels.join(", "));

    let signature = SignatureInformation {
        label,
        documentation: Some(tower_lsp::lsp_types::Documentation::MarkupContent(
            MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "**{}** ({})\n\n{}",
                    ugen.name, ugen.category, ugen.description
                ),
            },
        )),
        parameters: Some(parameters),
        active_parameter: Some(active_param as u32),
    };

    Some(SignatureHelp {
        signatures: vec![signature],
        active_signature: Some(0),
        active_parameter: Some(active_param as u32),
    })
}

/// Format UGen documentation for hover.
pub fn format_ugen_hover(name: &str) -> Option<String> {
    let ugen = get_ugen_doc(name)?;

    // Determine the rate suffix if present
    let rate = if name.ends_with("_ar") {
        Some("ar")
    } else if name.ends_with("_kr") {
        Some("kr")
    } else if name.ends_with("_ir") {
        Some("ir")
    } else {
        None
    };

    let mut content = String::new();

    // Title with rate info
    if let Some(r) = rate {
        let rate_name = match r {
            "ar" => "audio rate",
            "kr" => "control rate",
            "ir" => "init rate",
            _ => r,
        };
        content.push_str(&format!("### UGen: `{}` ({})\n\n", ugen.name, rate_name));
    } else {
        content.push_str(&format!("### UGen: `{}`\n\n", ugen.name));
    }

    // Description
    content.push_str(&ugen.description);
    content.push_str("\n\n");

    // Category
    content.push_str(&format!("**Category:** {}\n\n", ugen.category));

    // Available rates
    let rates_str = ugen
        .rates
        .iter()
        .filter(|r| *r != "builder")
        .map(|r| format!("`{}_{}`", to_snake_case(&ugen.name), r))
        .collect::<Vec<_>>()
        .join(", ");
    if !rates_str.is_empty() {
        content.push_str(&format!("**Available as:** {}\n\n", rates_str));
    }

    // Inputs/parameters
    if !ugen.inputs.is_empty() {
        content.push_str("**Parameters:**\n\n");
        for input in &ugen.inputs {
            content.push_str(&format!(
                "- `{}` ({}) - {} (default: {})\n",
                input.name, input.input_type, input.description, input.default
            ));
        }
        content.push('\n');
    }

    // Outputs
    content.push_str(&format!("**Outputs:** {}\n", ugen.outputs));

    // Example usage
    let func_name = if let Some(r) = rate {
        format!("{}_{}", to_snake_case(&ugen.name), r)
    } else if ugen.rates.contains(&"ar".to_string()) {
        format!("{}_ar", to_snake_case(&ugen.name))
    } else {
        format!(
            "{}_{}",
            to_snake_case(&ugen.name),
            ugen.rates.first().unwrap_or(&"ar".to_string())
        )
    };

    let args = ugen
        .inputs
        .iter()
        .map(|i| format!("{}", i.default))
        .collect::<Vec<_>>()
        .join(", ");

    content.push_str(&format!(
        "\n**Example:**\n```rhai\nlet sig = {}({});\n```",
        func_name, args
    ));

    Some(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ugen_cache_loads() {
        let cache = get_ugen_cache();
        assert!(!cache.is_empty(), "UGen cache should not be empty");
        assert!(cache.contains_key("sin_osc_ar"), "Should have sin_osc_ar");
        assert!(cache.contains_key("SinOsc"), "Should have SinOsc");
    }

    #[test]
    fn test_ugen_completions() {
        let completions = get_ugen_completions();
        assert!(!completions.is_empty(), "Should have UGen completions");

        let sin_osc = completions.iter().find(|c| c.label == "sin_osc_ar");
        assert!(sin_osc.is_some(), "Should have sin_osc_ar completion");
    }

    #[test]
    fn test_ugen_signature_help() {
        let help = get_ugen_signature_help("sin_osc_ar", 0);
        assert!(help.is_some(), "Should have signature help for sin_osc_ar");

        let help = help.unwrap();
        assert_eq!(help.signatures.len(), 1);
        assert!(help.signatures[0].label.contains("sin_osc_ar"));
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("SinOsc"), "sin_osc");
        assert_eq!(to_snake_case("LPF"), "lpf");
        assert_eq!(to_snake_case("DC"), "dc");
        assert_eq!(to_snake_case("BPeakEQ"), "b_peak_eq");
    }
}
