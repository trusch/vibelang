//! Build script for vibelang-dsp.
//!
//! Generates UGen wrapper functions from JSON manifests.

use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=ugen_manifests");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated.rs");
    let mut f = File::create(&dest_path).unwrap();

    // Read all manifest files from the ugen_manifests directory
    let mut manifest: Vec<Value> = Vec::new();
    let manifests_dir = Path::new("ugen_manifests");

    if let Ok(entries) = fs::read_dir(manifests_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                println!("cargo:rerun-if-changed={}", path.display());
                let manifest_str = fs::read_to_string(&path).unwrap();
                let mut category_manifest: Vec<Value> = serde_json::from_str(&manifest_str).unwrap();
                manifest.append(&mut category_manifest);
            }
        }
    }

    // Generate the code
    writeln!(f, "// AUTO-GENERATED FILE - DO NOT EDIT").unwrap();
    writeln!(f, "// Generated from ugen_manifests/*.json\n").unwrap();
    writeln!(f, "use crate::errors::*;").unwrap();
    writeln!(f, "use crate::graph::*;").unwrap();
    writeln!(f, "use crate::helpers;").unwrap();
    writeln!(f, "use rhai::Dynamic;\n").unwrap();

    // Generate one function per UGen rate (snake_case_ar, snake_case_kr, etc.)
    for ugen in &manifest {
        let name = ugen["name"].as_str().unwrap();
        let rates = ugen["rates"].as_array().unwrap();

        // Skip documentation-only entries (like fluent builder API docs)
        let has_only_builder_rate = rates.iter().all(|r| r.as_str() == Some("builder"));
        if has_only_builder_rate {
            continue;
        }

        let description = ugen
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let inputs = ugen["inputs"].as_array().unwrap();
        let outputs = ugen["outputs"].as_i64().unwrap();
        let category = ugen
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("General");

        let snake_name = to_snake_case(name);

        // Generate one function for each rate
        for rate in rates {
            let rate_str = rate.as_str().unwrap();
            let rate_enum = match rate_str {
                "ar" => "Rate::Audio",
                "kr" => "Rate::Control",
                "ir" => "Rate::Scalar",
                _ => "Rate::Audio",
            };

            let func_name = format!("{}_{}", snake_name, rate_str);

            // Collect parameter info
            let mut dyn_params = Vec::new();
            let mut param_names = Vec::new();

            for input in inputs.iter() {
                let param_name = input["name"].as_str().unwrap();
                let escaped_name = param_to_snake_case(param_name);
                dyn_params.push(format!("{}: &Dynamic", escaped_name));
                param_names.push(escaped_name);
            }

            // Generate documentation
            writeln!(f, "/// {} - {} ({})", name, description, category).unwrap();
            writeln!(f, "///").unwrap();
            writeln!(f, "/// # Parameters").unwrap();
            for input in inputs.iter() {
                let param_name = input["name"].as_str().unwrap();
                let param_desc = input
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let param_default = input.get("default").and_then(|v| {
                    if v.is_f64() {
                        Some(v.as_f64().unwrap())
                    } else if v.is_i64() {
                        Some(v.as_i64().unwrap() as f64)
                    } else {
                        None
                    }
                });

                if let Some(default) = param_default {
                    writeln!(f, "/// - `{}` (default: {}): {}", param_name, default, param_desc).unwrap();
                } else {
                    writeln!(f, "/// - `{}`: {}", param_name, param_desc).unwrap();
                }
            }
            writeln!(f, "///").unwrap();
            writeln!(f, "/// # Returns").unwrap();
            writeln!(f, "/// {} output channel(s)", outputs).unwrap();

            // Generate function
            if !dyn_params.is_empty() {
                writeln!(
                    f,
                    "pub fn {}({}) -> Result<crate::NodeRef> {{",
                    func_name,
                    dyn_params.join(", ")
                )
                .unwrap();
            } else {
                writeln!(f, "pub fn {}() -> Result<crate::NodeRef> {{", func_name).unwrap();
            }

            writeln!(f, "    let inputs = vec![").unwrap();
            for param_name in &param_names {
                writeln!(f, "        helpers::dynamic_to_input({})?,", param_name).unwrap();
            }
            writeln!(f, "    ];").unwrap();
            writeln!(f, "    with_builder(|builder| {{").unwrap();
            writeln!(
                f,
                "        builder.add_node(\"{}\".to_string(), {}, inputs, {}, 0)",
                name, rate_enum, outputs
            )
            .unwrap();
            writeln!(f, "    }})").unwrap();
            writeln!(f, "}}\n").unwrap();
        }
    }

    // Generate registration function
    writeln!(f, "/// Register all generated UGens with the Rhai engine.").unwrap();
    writeln!(
        f,
        "pub fn register_generated_ugens(engine: &mut rhai::Engine) {{"
    )
    .unwrap();

    for ugen in &manifest {
        let name = ugen["name"].as_str().unwrap();
        let rates = ugen["rates"].as_array().unwrap();

        // Skip documentation-only entries (like fluent builder API docs)
        let has_only_builder_rate = rates.iter().all(|r| r.as_str() == Some("builder"));
        if has_only_builder_rate {
            continue;
        }

        let inputs = ugen["inputs"].as_array().unwrap();
        let snake_name = to_snake_case(name);

        for rate in rates {
            let rate_str = rate.as_str().unwrap();
            let func_name = format!("{}_{}", snake_name, rate_str);

            // Register for all possible arities (0 to N) to support default arguments
            for arity in 0..=inputs.len() {
                let mut closure_params = Vec::new();
                let mut call_args = Vec::new();

                for input in inputs.iter().take(arity) {
                    let param_name = input["name"].as_str().unwrap();
                    let escaped_name = param_to_snake_case(param_name);
                    closure_params.push(format!("{}: Dynamic", escaped_name));
                    call_args.push(format!("&{}", escaped_name));
                }

                for input in inputs.iter().skip(arity) {
                    let default_val = input
                        .get("default")
                        .and_then(|v| {
                            if v.is_f64() {
                                Some(v.as_f64().unwrap())
                            } else if v.is_i64() {
                                Some(v.as_i64().unwrap() as f64)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0.0);
                    call_args.push(format!("&Dynamic::from({}f64)", default_val));
                }

                if !closure_params.is_empty() {
                    writeln!(f, "    engine.register_fn(").unwrap();
                    writeln!(f, "        \"{}\",", func_name).unwrap();
                    writeln!(f, "        |{}| {{", closure_params.join(", ")).unwrap();
                    writeln!(
                        f,
                        "            {}({}).unwrap()",
                        func_name,
                        call_args.join(", ")
                    )
                    .unwrap();
                    writeln!(f, "        }}").unwrap();
                    writeln!(f, "    );").unwrap();
                } else {
                    writeln!(
                        f,
                        "    engine.register_fn(\"{}\", || {}({}).unwrap());",
                        func_name,
                        func_name,
                        call_args.join(", ")
                    )
                    .unwrap();
                }
            }
        }
    }

    writeln!(f, "}}").unwrap();
}

fn to_snake_case(s: &str) -> String {
    if s == "DC" {
        return "dc".to_string();
    }
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                let prev_lower = chars[i - 1].is_lowercase();
                let next_lower = chars.get(i + 1).map(|c| c.is_lowercase()).unwrap_or(false);
                if prev_lower || next_lower {
                    result.push('_');
                }
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

/// Convert a parameter name to snake_case and escape if necessary.
fn param_to_snake_case(s: &str) -> String {
    escape_keyword(&to_snake_case(s))
}

fn escape_keyword(s: &str) -> String {
    match s {
        "as" | "break" | "const" | "continue" | "crate" | "else" | "enum" | "extern" | "false"
        | "fn" | "for" | "if" | "impl" | "in" | "let" | "loop" | "match" | "mod" | "move"
        | "mut" | "pub" | "ref" | "return" | "self" | "Self" | "static" | "struct" | "super"
        | "trait" | "true" | "type" | "unsafe" | "use" | "where" | "while" | "async" | "await"
        | "dyn" | "abstract" | "become" | "box" | "do" | "final" | "macro" | "override"
        | "priv" | "typeof" | "unsized" | "virtual" | "yield" | "try" => format!("r#{}", s),
        _ => s.to_string(),
    }
}
