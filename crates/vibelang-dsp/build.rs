//! Build script for vibelang-dsp.
//!
//! Generates UGen wrapper functions from JSON manifests.

use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct UGenManifest {
    name: String,
    description: String,
    rates: Vec<String>,
    inputs: Vec<UGenInput>,
    outputs: u32,
    category: String,
    #[serde(default)]
    #[allow(dead_code)]
    functions: Option<Vec<String>>,
    /// Server-side UGen class to emit, defaults to `name`. Used to expose
    /// BinaryOpUGen / UnaryOpUGen variants under friendlier names (e.g. an
    /// entry named "Hypot" emits `BinaryOpUGen` with `special_index = 23`).
    #[serde(default)]
    ugen_class: Option<String>,
    /// Special index passed to the server (operator selector for
    /// BinaryOpUGen / UnaryOpUGen). Defaults to 0.
    #[serde(default)]
    special_index: Option<i16>,
    /// True when the manifest name is an sclang-side helper, alias, or wrapper
    /// that must not be emitted as a literal server UGen name.
    #[serde(default)]
    pseudo: bool,
    /// SuperCollider plugin package required for this literal server UGen.
    #[serde(default)]
    requires_plugin: Option<String>,
    /// Rationale for entries kept as documentation/unavailable stubs.
    #[serde(default)]
    unavailable_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UGenInput {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    default: Option<serde_json::Value>,
    description: String,
}

// `demand` is the SuperCollider demand-rate (used by Dseq, Dser, Dwhite, …).
// `builder` flags documentation-only fluent-API entries (e.g. `envelope`).
const ALLOWED_RATES: &[&str] = &["ar", "kr", "ir", "demand", "builder"];
const ALLOWED_INPUT_TYPES: &[&str] = &["signal", "float", "int", "method"];
const CONFIRMED_PSEUDO_UGENS: &[&str] = &[
    "AMClip",
    "AbsDif",
    "Atan2",
    "BLowPass4",
    "BHiPass4",
    "Changed",
    "Clip2",
    "DifSqr",
    "Excess",
    "ExpExp",
    "ExpLin",
    "FFTCentroid",
    "FirstArg",
    "Fold2",
    "Greyhole",
    "Hypot",
    "HypotApx",
    "JPverb",
    "LinLin",
    "Mix",
    "OnsetsDS",
    "PMOsc",
    "PV_DiffMags",
    "PanX2D",
    "PulseDPW",
    "Ring1",
    "Ring2",
    "Ring3",
    "Ring4",
    "Rotate",
    "ScaleNeg",
    "SelectX",
    "Silence",
    "SoundIn",
    "Splay",
    "SplayAz",
    "SqrDif",
    "SqrSum",
    "SumSqr",
    "TWChoose",
    "Thresh",
    "Tilt",
    "Tumble",
    "Wrap2",
];
const STALE_NON_BINARY_UGENS: &[&str] = &[
    "AtsFile",
    "BigArity24",
    "CQ_Diff",
    "FFTSubbandFlux",
    "FaustGreyholeRaw",
    "HOAEncLebedev061",
    "HOALibEnc3D1",
    "HOALibEnc3D2",
    "HOALibEnc3D3",
    "HOALibEnc3D4",
    "HOALibEnc3D5",
    "HOAmbiPanner1",
    "HOAmbiPanner2",
    "HOAmbiPanner3",
    "HOAmbiPanner4",
    "HOAmbiPanner5",
    "ITU5001",
    "ITU5002",
    "LinkJump",
    "LinkPhase",
    "LinkTempo",
    "MIDelay",
    "MiBraids",
    "MiClouds",
    "MiElements",
    "MiGrids",
    "MiMu",
    "MiOmi",
    "MiPlaits",
    "MiRings",
    "MiRipples",
    "MiTides",
    "MiVerb",
    "MiWarps",
    "RMAFoodChainL",
    "RosslerResL",
    "SimpleLoopBuf",
    "VBAPSpeaker",
    "VBAPSpeakerArray",
    "envelope",
];

/// `^[A-Z][A-Za-z0-9_]*$` — PascalCase identifier, with optional underscores
/// to accommodate SuperCollider's PV_* family (PV_HainsworthFoote, PV_MagAbove, …).
fn is_valid_ugen_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `^[a-z][a-zA-Z0-9_]*$` — leading lowercase, then alphanumerics or underscore.
/// Relaxed from the strict snake_case rule to accommodate the existing manifests'
/// camelCase convention (`numChannels`, `attackTime`, …). The build script normalises
/// to snake_case in the generated Rust regardless.
fn is_valid_input_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn has_codegen_lowering(ugen: &UGenManifest) -> bool {
    if ugen.ugen_class.is_some() {
        return true;
    }

    ugen.rates
        .iter()
        .filter(|rate| rate.as_str() != "builder")
        .all(|rate| {
            let func_name = format!("{}_{}", to_snake_case(&ugen.name), rate);
            pseudo_lowering_expr(&func_name, rate).is_some()
        })
}

fn validate(file: &Path, ugen: &UGenManifest) -> Result<(), String> {
    if ugen.rates.is_empty() {
        return Err(format!("UGen '{}' has empty rates list", ugen.name));
    }
    for r in &ugen.rates {
        if !ALLOWED_RATES.contains(&r.as_str()) {
            return Err(format!(
                "UGen '{}' has invalid rate '{}' (allowed: {:?})",
                ugen.name, r, ALLOWED_RATES
            ));
        }
    }

    let is_builder_only = ugen.rates.iter().all(|r| r == "builder");
    let has_unavailable_reason = ugen
        .unavailable_reason
        .as_deref()
        .map(str::trim)
        .is_some_and(|reason| !reason.is_empty());

    if ugen
        .requires_plugin
        .as_deref()
        .map(str::trim)
        .is_some_and(str::is_empty)
    {
        return Err(format!("UGen '{}' has empty requires_plugin", ugen.name));
    }
    if ugen.unavailable_reason.is_some() && !has_unavailable_reason {
        return Err(format!("UGen '{}' has empty unavailable_reason", ugen.name));
    }
    if ugen.unavailable_reason.is_some() && !is_builder_only {
        return Err(format!(
            "UGen '{}' has unavailable_reason but is still codegenerated",
            ugen.name
        ));
    }
    if CONFIRMED_PSEUDO_UGENS.contains(&ugen.name.as_str()) {
        if !ugen.pseudo {
            return Err(format!(
                "UGen '{}' is a confirmed pseudo-UGen and must be tagged pseudo",
                ugen.name
            ));
        }
        if !is_builder_only && !has_codegen_lowering(ugen) {
            return Err(format!(
                "UGen '{}' is pseudo but has no ugen_class/special_index or codegen lowering",
                ugen.name
            ));
        }
    }
    if ugen.pseudo && !is_builder_only && !has_codegen_lowering(ugen) {
        return Err(format!(
            "UGen '{}' is tagged pseudo but has no codegen lowering",
            ugen.name
        ));
    }
    if STALE_NON_BINARY_UGENS.contains(&ugen.name.as_str())
        && ugen.requires_plugin.is_none()
        && !(is_builder_only && has_unavailable_reason)
    {
        return Err(format!(
            "UGen '{}' is an audited non-binary name; mark it unavailable or require a plugin",
            ugen.name
        ));
    }

    // UGen name must be PascalCase, except for builder-only entries (which are
    // documentation pseudo-entries — e.g. `envelope` — and follow function-name
    // conventions instead of UGen conventions).
    if !is_builder_only && !is_valid_ugen_name(&ugen.name) {
        return Err(format!(
            "UGen '{}' name does not match ^[A-Z][A-Za-z0-9]*$ (file: {})",
            ugen.name,
            file.display()
        ));
    }

    for inp in &ugen.inputs {
        if !ALLOWED_INPUT_TYPES.contains(&inp.ty.as_str()) {
            return Err(format!(
                "UGen '{}' input '{}' has invalid type '{}' (allowed: {:?})",
                ugen.name, inp.name, inp.ty, ALLOWED_INPUT_TYPES
            ));
        }
        // method-typed inputs are documentation for chainable API calls
        // (e.g. `.attack(time)`); they don't follow identifier rules.
        if inp.ty != "method" && !is_valid_input_name(&inp.name) {
            return Err(format!(
                "UGen '{}' input '{}' does not match ^[a-z][a-zA-Z0-9_]*$",
                ugen.name, inp.name
            ));
        }
        // Touch description so the field is exercised by validation.
        if inp.description.is_empty() {
            return Err(format!(
                "UGen '{}' input '{}' has empty description",
                ugen.name, inp.name
            ));
        }
    }

    if ugen.description.is_empty() {
        return Err(format!("UGen '{}' has empty description", ugen.name));
    }
    if ugen.category.is_empty() {
        return Err(format!("UGen '{}' has empty category", ugen.name));
    }

    Ok(())
}

fn load_and_validate() -> Vec<UGenManifest> {
    let manifests_dir = Path::new("ugen_manifests");
    let mut manifest: Vec<UGenManifest> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    let entries = match fs::read_dir(manifests_dir) {
        Ok(e) => e,
        Err(e) => {
            println!("cargo:warning=failed to read ugen_manifests dir: {}", e);
            panic!("failed to read ugen_manifests dir: {}", e);
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    paths.sort(); // deterministic ordering across builds

    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        let body = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                println!("cargo:warning=failed to read {}: {}", path.display(), e);
                panic!("failed to read {}: {}", path.display(), e);
            }
        };
        let parsed: Vec<UGenManifest> = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                println!(
                    "cargo:warning={}: schema parse error: {}",
                    path.display(),
                    e
                );
                panic!(
                    "Failed to parse manifest {} against UGenManifest schema: {}",
                    path.display(),
                    e
                );
            }
        };
        for ugen in parsed {
            if let Err(msg) = validate(&path, &ugen) {
                println!("cargo:warning={}: {}", path.display(), msg);
                panic!("manifest validation failed in {}: {}", path.display(), msg);
            }
            if !seen_names.insert(ugen.name.clone()) {
                let msg = format!(
                    "duplicate UGen name '{}' (also defined elsewhere)",
                    ugen.name
                );
                println!("cargo:warning={}: {}", path.display(), msg);
                panic!("{} in {}", msg, path.display());
            }
            manifest.push(ugen);
        }
    }

    manifest
}

fn main() {
    println!("cargo:rerun-if-changed=ugen_manifests");

    let manifest = load_and_validate();

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated.rs");
    let mut f = File::create(&dest_path).unwrap();

    writeln!(f, "// AUTO-GENERATED FILE - DO NOT EDIT").unwrap();
    writeln!(f, "// Generated from ugen_manifests/*.json\n").unwrap();
    writeln!(f, "use crate::errors::*;").unwrap();
    writeln!(f, "use crate::graph::*;").unwrap();
    writeln!(f, "use crate::helpers;").unwrap();
    writeln!(f, "use rhai::Dynamic;\n").unwrap();

    // Generate one function per UGen rate (snake_case_ar, snake_case_kr, etc.)
    for ugen in &manifest {
        let name = &ugen.name;
        let rates = &ugen.rates;

        // Skip documentation-only entries (like fluent builder API docs)
        let has_only_builder_rate = rates.iter().all(|r| r == "builder");
        if has_only_builder_rate {
            continue;
        }

        let description = ugen.description.as_str();
        let inputs = &ugen.inputs;
        let outputs = ugen.outputs as i64;
        let category = ugen.category.as_str();

        let snake_name = to_snake_case(name);

        // Generate one function for each rate
        for rate_str in rates {
            let rate_enum = match rate_str.as_str() {
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
                let escaped_name = param_to_snake_case(&input.name);
                dyn_params.push(format!("{}: &Dynamic", escaped_name));
                param_names.push(escaped_name);
            }

            // Generate documentation
            writeln!(f, "/// {} - {} ({})", name, description, category).unwrap();
            writeln!(f, "///").unwrap();
            writeln!(f, "/// # Parameters").unwrap();
            for input in inputs.iter() {
                let param_name = input.name.as_str();
                let param_desc = input.description.as_str();
                let param_default = input.default.as_ref().and_then(|v| {
                    if v.is_f64() {
                        v.as_f64()
                    } else if v.is_i64() {
                        v.as_i64().map(|n| n as f64)
                    } else {
                        None
                    }
                });

                if let Some(default) = param_default {
                    writeln!(
                        f,
                        "/// - `{}` (default: {}): {}",
                        param_name, default, param_desc
                    )
                    .unwrap();
                } else {
                    writeln!(f, "/// - `{}`: {}", param_name, param_desc).unwrap();
                }
            }
            writeln!(f, "///").unwrap();
            writeln!(f, "/// # Returns").unwrap();
            writeln!(f, "/// {} output channel(s)", outputs).unwrap();

            let is_pseudo_lowering = pseudo_lowering_expr(&func_name, rate_str).is_some();

            // Generate function
            if !dyn_params.is_empty() {
                writeln!(
                    f,
                    "pub fn {}({}) -> Result<{}> {{",
                    func_name,
                    dyn_params.join(", "),
                    if is_pseudo_lowering {
                        "Dynamic"
                    } else {
                        "crate::NodeRef"
                    }
                )
                .unwrap();
            } else {
                writeln!(
                    f,
                    "pub fn {}() -> Result<{}> {{",
                    func_name,
                    if is_pseudo_lowering {
                        "Dynamic"
                    } else {
                        "crate::NodeRef"
                    }
                )
                .unwrap();
            }

            if let Some(expr) = pseudo_lowering_expr(&func_name, rate_str) {
                writeln!(f, "    {}", expr).unwrap();
                writeln!(f, "}}\n").unwrap();
                continue;
            }

            writeln!(f, "    let inputs = vec![").unwrap();
            for param_name in &param_names {
                writeln!(f, "        helpers::dynamic_to_input({})?,", param_name).unwrap();
            }
            writeln!(f, "    ];").unwrap();
            writeln!(f, "    with_builder(|builder| {{").unwrap();
            let emitted_class = ugen.ugen_class.as_deref().unwrap_or(name);
            let special_index = ugen.special_index.unwrap_or(0);
            writeln!(
                f,
                "        builder.add_node(\"{}\".to_string(), {}, inputs, {}, {})",
                emitted_class, rate_enum, outputs, special_index
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
        let name = &ugen.name;
        let rates = &ugen.rates;

        // Skip documentation-only entries (like fluent builder API docs)
        let has_only_builder_rate = rates.iter().all(|r| r == "builder");
        if has_only_builder_rate {
            continue;
        }

        let inputs = &ugen.inputs;
        let snake_name = to_snake_case(name);

        for rate_str in rates {
            let func_name = format!("{}_{}", snake_name, rate_str);

            // Register per-arity overloads so default arguments work for
            // calls with fewer than `inputs.len()` positional args. Arities
            // ≤ 20 use `register_fn` (typed-tuple `IntoFuncArgs` / `def_register!`
            // path). UGens with > 20 inputs exceed Rhai's `def_register!(A:20, …)`
            // cap *and* `register_raw_fn`'s exact-TypeId matching defeats a
            // `&[Dynamic; N]` slot for `f64`-typed user calls — so the >20 case
            // falls back to a single `rhai::Array` parameter, validated and
            // unpacked inside the closure. See `kb/synthdef-arity-limits-plan.md`
            // §3.1.
            let positional_max = inputs.len().min(20);
            for arity in 0..=positional_max {
                let mut closure_params = Vec::new();
                let mut call_args = Vec::new();

                for input in inputs.iter().take(arity) {
                    let escaped_name = param_to_snake_case(&input.name);
                    closure_params.push(format!("{}: Dynamic", escaped_name));
                    call_args.push(format!("&{}", escaped_name));
                }

                for input in inputs.iter().skip(arity) {
                    let default_val = input
                        .default
                        .as_ref()
                        .and_then(|v| {
                            if v.is_f64() {
                                v.as_f64()
                            } else if v.is_i64() {
                                v.as_i64().map(|n| n as f64)
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

            if inputs.len() > 20 {
                let arity = inputs.len();
                writeln!(f, "    engine.register_raw_fn(").unwrap();
                writeln!(f, "        \"{}\",", func_name).unwrap();
                writeln!(f, "        &[std::any::TypeId::of::<rhai::Array>()],").unwrap();
                writeln!(f, "        |_ctx, args: &mut [&mut Dynamic]| {{").unwrap();
                writeln!(
                    f,
                    "            let array: rhai::Array = std::mem::take(args[0]).cast();"
                )
                .unwrap();
                writeln!(f, "            if array.len() != {} {{", arity).unwrap();
                writeln!(
                    f,
                    "                return Err(format!(\"{} expects array of length {}, got {{}}\", array.len()).into());",
                    func_name, arity
                )
                .unwrap();
                writeln!(f, "            }}").unwrap();
                let call_args: Vec<String> = (0..arity).map(|i| format!("&array[{}]", i)).collect();
                writeln!(
                    f,
                    "            Ok({}({}).unwrap())",
                    func_name,
                    call_args.join(", ")
                )
                .unwrap();
                writeln!(f, "        }},").unwrap();
                writeln!(f, "    );").unwrap();
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
                let prev = chars[i - 1];
                let prev_lower = prev.is_lowercase();
                let next_lower = chars.get(i + 1).map(|c| c.is_lowercase()).unwrap_or(false);
                if (prev_lower || next_lower) && prev != '_' {
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

fn pseudo_lowering_expr(func_name: &str, rate_str: &str) -> Option<&'static str> {
    let rate = match rate_str {
        "ar" => "Rate::Audio",
        "kr" => "Rate::Control",
        "ir" => "Rate::Scalar",
        _ => return None,
    };

    match func_name {
        "changed_ar" | "changed_kr" => Some(match rate {
            "Rate::Audio" => {
                "helpers::changed_pseudo(Rate::Audio, r#in, threshold).map(Dynamic::from)"
            }
            "Rate::Control" => {
                "helpers::changed_pseudo(Rate::Control, r#in, threshold).map(Dynamic::from)"
            }
            _ => return None,
        }),
        "j_pverb_ar" => Some(
            "helpers::jpverb_pseudo(r#in, t60, damp, size, early_diff, mod_depth, mod_freq, low, mid, high, lowcut, highcut).map(Dynamic::from)",
        ),
        "greyhole_ar" => Some(
            "helpers::greyhole_pseudo(r#in, delay_time, damp, size, diff, feedback, mod_depth, mod_freq).map(Dynamic::from)",
        ),
        "lin_lin_ar" => Some(
            "helpers::lin_lin_pseudo(Rate::Audio, r#in, srclo, srchi, dstlo, dsthi).map(Dynamic::from)",
        ),
        "lin_lin_kr" => Some(
            "helpers::lin_lin_pseudo(Rate::Control, r#in, srclo, srchi, dstlo, dsthi).map(Dynamic::from)",
        ),
        "lin_lin_ir" => Some(
            "helpers::lin_lin_pseudo(Rate::Scalar, r#in, srclo, srchi, dstlo, dsthi).map(Dynamic::from)",
        ),
        "mix_ar" => Some("helpers::mix_pseudo(Rate::Audio, array)"),
        "mix_kr" => Some("helpers::mix_pseudo(Rate::Control, array)"),
        "splay_ar" => Some(
            "helpers::splay_pseudo(Rate::Audio, in_array, spread, level, center, level_comp)",
        ),
        "splay_kr" => Some(
            "helpers::splay_pseudo(Rate::Control, in_array, spread, level, center, level_comp)",
        ),
        "splay_az_ar" => Some(
            "helpers::splay_az_pseudo(Rate::Audio, num_chans, in_array, spread, level, width, center, orientation, level_comp)",
        ),
        "splay_az_kr" => Some(
            "helpers::splay_az_pseudo(Rate::Control, num_chans, in_array, spread, level, width, center, orientation, level_comp)",
        ),
        _ => None,
    }
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
