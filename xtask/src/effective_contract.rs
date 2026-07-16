use crate::{public_api, public_artifacts};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use syn::{Fields, ImplItem, Item, Visibility};
use vibelang_api_manifest::canonical::{canonical_sha256_hex, sha256_hex};
use vibelang_api_manifest::compatibility::{
    CompatibilityChange, CompatibilityClass, CompatibilityReport,
};
use vibelang_api_manifest::fragments::{
    parse_authoring_fragment, parse_consumers_fragment, parse_http_fragment,
    parse_runtime_fragment, parse_wasm_fragment, parse_websocket_fragment, DiscoveredSemanticNode,
    FragmentSet, SemanticFacet,
};
use vibelang_api_manifest::v2::{
    semantic_id, validate_stable_id, Alias, AliasKind, ApiEntryV2, ApiType, Atomicity,
    AvailabilityStatus, AvailabilityV2, BindingDetails, CancellationContract, Capability,
    CapabilityExpression, CapabilityState, ConsistencyPoint, Consumer, ConsumerExclusion,
    CoverageRecord, Derivation, EffectTiming, Eligibility, EnumVariant, Event, EventDelivery,
    EventOrdering, ExclusionReason, Facet, FailureContract, FailureDelivery, FailureStage,
    FallbackPolicy, Field, FieldBinding, FieldDirection, Generator, HttpSuccess, Idempotency,
    LifecycleContract, LifecycleEffect, LifecyclePhase, LifecycleRole, LossDetection, NodeMetadata,
    ObservationContract, Operation, OperationKind, Ownership, PackageContract, PanicExposure,
    ParameterV2, PriorState, ProvenanceAnchor, PublicApiManifestV2, RepeatSemantics,
    RevisionRelation, Stability, StabilityLevel, SurfaceBinding, Synchronization, TypeKind,
    UnavailableBehavior, WasmProgress, SCHEMA_URI_V2, SCHEMA_VERSION_V2,
};
use vibelang_api_manifest::{
    to_pretty_json, Anchor, ApiEntry, Availability, BoundarySemantics, PublicApiManifest,
};
use walkdir::WalkDir;

const V2_PATH: &str = "api/public-api-manifest-v2.json";
const COVERAGE_PATH: &str = "api/public-api-coverage-v2.json";
const DIFF_PATH: &str = "api/public-api-compatibility-diff-v1-to-v2.json";
const PACKAGE_INDEX_PATH: &str = "api/public-api-package-index-v1.json";
const DEBT_PATH: &str = "api/public-api-compatibility-debt-v1.json";
const V1_MANIFEST_PATH: &str = "api/public-api-manifest-v1.json";
const V1_HTTP_PATH: &str = "api/http-api-snapshot-v1.json";
const BASELINE_PATH: &str = "api/baselines/public-artifacts-v1.json";
const FRAGMENT_DIR: &str = "api/contract";
const GENERATOR_NAME: &str = "vibelang-xtask-effective-contract";
const API_VERSION: &str = "0.4.0";
const ACCEPTED_V1_MANIFEST_SHA256: &str =
    "1dea4d106f11ebc916b9bd8bdade70973df0166d8040deb60aa1f2e60f244e05";
const ACCEPTED_V1_HTTP_SHA256: &str =
    "6f8a1de4d29e424715ffe1622f681312408fcda16234e39d859c3ec1f458cb2a";
const EXPECTED_ENTRIES: usize = 3_626;
const EXPECTED_OVERLOADS: usize = 8_431;
const EXPECTED_HTTP_ROUTES: usize = 96;
const EXPECTED_HTTP_TYPES: usize = 75;
const EXPECTED_HTTP_FIELDS: usize = 297;

pub fn generate(root: &Path, check: bool) -> Result<(), String> {
    let discovery = discover(root)?;
    let first = compose(root, &discovery)?;
    let second = compose(root, &discovery)?;
    if first != second {
        return Err("effective-contract double generation produced different bytes".into());
    }

    for (path, content) in first {
        write_or_check(root, path, &content, check)?;
    }
    println!(
        "effective contract: {EXPECTED_ENTRIES} entries, {EXPECTED_OVERLOADS} overloads, {EXPECTED_HTTP_ROUTES} routes, {EXPECTED_HTTP_TYPES} HTTP types, {EXPECTED_HTTP_FIELDS} HTTP fields, zero orphan/unclassified records"
    );
    println!("effective-contract double generation is byte-identical");
    Ok(())
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum MechanicalDisposition {
    Included,
    Excluded(ExclusionReason),
}

#[derive(Debug, Clone)]
struct RawMechanicalDeclaration {
    surface: String,
    kind: String,
    name: String,
    source_anchors: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct MechanicalDeclaration {
    id: String,
    surface: String,
    kind: String,
    name: String,
    owner: String,
    source_anchors: Vec<(String, String)>,
    contract_node: bool,
    consumers: BTreeMap<String, MechanicalDisposition>,
}

fn discover_mechanical_declarations(
    root: &Path,
    baseline: &ArtifactBaseline,
) -> Result<Vec<MechanicalDeclaration>, String> {
    let mut raw = Vec::new();
    discover_cli_declarations(root, &mut raw)?;
    discover_wasm_declarations(root, &mut raw)?;
    discover_websocket_declarations(root, &mut raw)?;
    discover_lsp_declarations(root, &mut raw)?;
    discover_vscode_declarations(root, &mut raw)?;
    discover_emacs_declarations(root, &mut raw)?;
    discover_markdown_declarations(root, baseline, &mut raw)?;
    discover_baseline_file_declarations(baseline, &mut raw)?;

    let failures = mechanical_classification_failures(&raw);
    if !failures.is_empty() {
        return Err(format!(
            "{} discovered mechanical declaration(s) lack ownership/classification: {}",
            failures.len(),
            failures.join(", ")
        ));
    }
    let mut declarations = raw
        .iter()
        .cloned()
        .map(classify_mechanical_declaration)
        .collect::<Result<Vec<_>, _>>()?;
    let missing = missing_mechanical_classifications(&raw, &declarations)?;
    if !missing.is_empty() {
        return Err(format!(
            "{} discovered eligible mechanical declaration(s) have no ownership/classification: {}",
            missing.len(),
            missing.join(", ")
        ));
    }
    declarations.sort_by(|left, right| left.id.cmp(&right.id));
    for pair in declarations.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(format!(
                "duplicate source-derived mechanical declaration ID {}",
                pair[0].id
            ));
        }
    }
    Ok(declarations)
}

fn missing_mechanical_classifications(
    raw: &[RawMechanicalDeclaration],
    classified: &[MechanicalDeclaration],
) -> Result<Vec<String>, String> {
    let classified_ids = classified
        .iter()
        .map(|declaration| declaration.id.as_str())
        .collect::<BTreeSet<_>>();
    raw.iter()
        .map(|declaration| {
            let classified = classify_mechanical_declaration(declaration.clone())?;
            Ok((
                classified.id,
                format!(
                    "{}:{}:{}",
                    declaration.surface, declaration.kind, declaration.name
                ),
            ))
        })
        .filter_map(|result| match result {
            Ok((id, _)) if classified_ids.contains(id.as_str()) => None,
            Ok((_, name)) => Some(Ok(name)),
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn mechanical_classification_failures(raw: &[RawMechanicalDeclaration]) -> Vec<String> {
    raw.iter()
        .filter(|declaration| mechanical_class(&declaration.surface, &declaration.kind).is_none())
        .map(|declaration| {
            format!(
                "{}:{}:{}",
                declaration.surface, declaration.kind, declaration.name
            )
        })
        .collect()
}

fn mechanical_class(
    surface: &str,
    kind: &str,
) -> Option<(
    &'static str,
    Option<&'static str>,
    bool,
    Option<ExclusionReason>,
)> {
    match (surface, kind) {
        ("cli", "command" | "argument") => Some(("vibelang-cli", Some("cli"), true, None)),
        (
            "wasm",
            "class" | "method" | "function" | "compatibility_shim" | "interface" | "result_member"
            | "host_bridge" | "host_bridge_method",
        ) => Some(("vibelang-wasm", Some("wasm"), true, None)),
        ("wasm", "start_hook") => Some((
            "vibelang-wasm",
            Some("wasm"),
            false,
            Some(ExclusionReason::NotApplicable),
        )),
        ("websocket", "event" | "action") => Some(("vibelang-http", None, true, None)),
        ("lsp", "diagnostic_rule" | "token_type" | "token_modifier") => {
            Some(("vibelang-lsp", Some("rhai_editor"), true, None))
        }
        ("vscode", "command" | "setting") => {
            Some(("vibelang-vscode", Some("rhai_editor"), true, None))
        }
        ("emacs", "command" | "setting") => {
            Some(("vibelang-emacs", Some("rhai_editor"), true, None))
        }
        ("docs", "contract_block") => Some(("vibelang-docs", Some("docs"), true, None)),
        ("docs", "non_contract_block") => Some((
            "vibelang-docs",
            Some("docs"),
            false,
            Some(ExclusionReason::NotApplicable),
        )),
        ("fixtures", "fixture") => Some(("vibelang-tests", Some("fixtures"), true, None)),
        ("packages", "manifest") => Some(("vibelang-packaging", Some("packages"), true, None)),
        ("packages", "lockfile") => Some((
            "vibelang-packaging",
            Some("packages"),
            false,
            Some(ExclusionReason::NotApplicable),
        )),
        _ => None,
    }
}

fn classify_mechanical_declaration(
    raw: RawMechanicalDeclaration,
) -> Result<MechanicalDeclaration, String> {
    let (owner, consumer, contract_node, exclusion) = mechanical_class(&raw.surface, &raw.kind)
        .ok_or_else(|| format!("unclassified mechanical declaration {}", raw.name))?;
    let id = if raw.surface == "websocket" && raw.kind == "event" {
        semantic_id("event", &format!("websocket|{}", raw.name))
    } else {
        semantic_id(
            "type",
            &format!(
                "mechanical|{}|{}|{}|{}",
                raw.surface, raw.kind, raw.source_anchors[0].0, raw.name
            ),
        )
    };
    let mut consumers = BTreeMap::new();
    if contract_node {
        consumers.insert("manifest".into(), MechanicalDisposition::Included);
    }
    if let Some(consumer) = consumer {
        consumers.insert(
            consumer.into(),
            exclusion.map_or(
                MechanicalDisposition::Included,
                MechanicalDisposition::Excluded,
            ),
        );
    }
    Ok(MechanicalDeclaration {
        id,
        surface: raw.surface,
        kind: raw.kind,
        name: raw.name,
        owner: owner.into(),
        source_anchors: raw.source_anchors,
        contract_node,
        consumers,
    })
}

fn raw_declaration(
    surface: &str,
    kind: &str,
    name: impl Into<String>,
    path: &str,
    symbol: impl Into<String>,
) -> RawMechanicalDeclaration {
    RawMechanicalDeclaration {
        surface: surface.into(),
        kind: kind.into(),
        name: name.into(),
        source_anchors: vec![(path.into(), symbol.into())],
    }
}

fn discover_cli_declarations(
    root: &Path,
    declarations: &mut Vec<RawMechanicalDeclaration>,
) -> Result<(), String> {
    const PATH: &str = "crates/vibelang-cli/src/main.rs";
    let source = read(root, PATH)?;
    let file = syn::parse_file(&source).map_err(|error| format!("{PATH}: {error}"))?;
    for item in file.items {
        match item {
            Item::Struct(item) if item.ident == "Cli" => {
                let Fields::Named(fields) = item.fields else {
                    continue;
                };
                for field in fields.named {
                    let Some(name) = field.ident.map(|ident| ident.to_string()) else {
                        continue;
                    };
                    if name != "command" {
                        declarations.push(raw_declaration(
                            "cli",
                            "argument",
                            name.clone(),
                            PATH,
                            format!("Cli::{name}"),
                        ));
                    }
                }
            }
            Item::Enum(item) if item.ident == "Commands" => {
                for variant in item.variants {
                    let command = to_kebab_case(&variant.ident.to_string());
                    declarations.push(raw_declaration(
                        "cli",
                        "command",
                        command,
                        PATH,
                        format!("Commands::{}", variant.ident),
                    ));
                    let Fields::Named(fields) = variant.fields else {
                        continue;
                    };
                    for field in fields.named {
                        let Some(name) = field.ident.map(|ident| ident.to_string()) else {
                            continue;
                        };
                        declarations.push(raw_declaration(
                            "cli",
                            "argument",
                            format!("{}::{name}", variant.ident),
                            PATH,
                            format!("Commands::{}::{name}", variant.ident),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    declarations.push(raw_declaration(
        "cli",
        "command",
        "help",
        PATH,
        "Clap Parser implicit help subcommand",
    ));
    Ok(())
}

fn discover_wasm_declarations(
    root: &Path,
    declarations: &mut Vec<RawMechanicalDeclaration>,
) -> Result<(), String> {
    const PATH: &str = "crates/vibelang-wasm/src/lib.rs";
    let source = read(root, PATH)?;
    let file = syn::parse_file(&source).map_err(|error| format!("{PATH}: {error}"))?;
    for item in &file.items {
        match item {
            Item::Struct(item) if has_attribute(&item.attrs, "wasm_bindgen") => {
                declarations.push(raw_declaration(
                    "wasm",
                    "class",
                    item.ident.to_string(),
                    PATH,
                    item.ident.to_string(),
                ));
            }
            Item::Struct(item)
                if item.ident == "ExecutionResult" || item.ident == "CompiledSynthdef" =>
            {
                let interface = item.ident.to_string();
                declarations.push(raw_declaration(
                    "wasm",
                    "interface",
                    interface.clone(),
                    PATH,
                    interface.clone(),
                ));
                for field in &item.fields {
                    let Some(field) = &field.ident else { continue };
                    declarations.push(raw_declaration(
                        "wasm",
                        "result_member",
                        format!("{interface}.{field}"),
                        PATH,
                        format!("{interface}::{field}"),
                    ));
                }
            }
            Item::Impl(item) if has_attribute(&item.attrs, "wasm_bindgen") => {
                let class = item.self_ty.to_token_stream().to_string().replace(' ', "");
                for impl_item in &item.items {
                    let ImplItem::Fn(function) = impl_item else {
                        continue;
                    };
                    if !matches!(function.vis, Visibility::Public(_)) {
                        continue;
                    }
                    let name = wasm_export_name(&function.sig.ident.to_string(), &function.attrs);
                    declarations.push(raw_declaration(
                        "wasm",
                        "method",
                        format!("{class}.{name}"),
                        PATH,
                        format!("{class}::{}", function.sig.ident),
                    ));
                }
            }
            Item::Fn(function)
                if has_attribute(&function.attrs, "wasm_bindgen")
                    && matches!(function.vis, Visibility::Public(_)) =>
            {
                let attributes = attributes_text(&function.attrs);
                let kind = if attributes.contains("start") {
                    "start_hook"
                } else {
                    "function"
                };
                declarations.push(raw_declaration(
                    "wasm",
                    kind,
                    wasm_export_name(&function.sig.ident.to_string(), &function.attrs),
                    PATH,
                    function.sig.ident.to_string(),
                ));
            }
            _ => {}
        }
    }
    if !source.contains("JsValue::from_str(\"vibelangBridge\")") {
        return Err("WASM host bridge global is no longer mechanically discoverable".into());
    }
    declarations.push(raw_declaration(
        "wasm",
        "host_bridge",
        "globalThis.vibelangBridge",
        PATH,
        "JsValue::from_str(\"vibelangBridge\")",
    ));
    let bridge_methods = source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("async fn "))
        .filter_map(|tail| {
            tail.split_once('(')
                .map(|(name, _)| name.trim().to_string())
        })
        .filter(|name| name.chars().any(char::is_uppercase))
        .collect::<BTreeSet<_>>();
    if bridge_methods.is_empty() {
        return Err("WASM host bridge has no mechanically discovered methods".into());
    }
    for method in bridge_methods {
        declarations.push(raw_declaration(
            "wasm",
            "host_bridge_method",
            format!("globalThis.vibelangBridge.{method}"),
            PATH,
            method,
        ));
    }
    const TYPES_PATH: &str = "crates/vibelang-wasm/types/index.d.ts";
    let generated_types = read(root, TYPES_PATH)?;
    let source_exports = declarations
        .iter()
        .filter(|declaration| declaration.surface == "wasm" && declaration.kind == "function")
        .map(|declaration| declaration.name.clone())
        .collect::<BTreeSet<_>>();
    for line in generated_types.lines() {
        let line = line.trim();
        let signature = line
            .strip_prefix("export function ")
            .or_else(|| line.strip_prefix("export default function "));
        let Some(signature) = signature else {
            continue;
        };
        let Some((name, _)) = signature.split_once('(') else {
            continue;
        };
        if !source_exports.contains(name) {
            declarations.push(raw_declaration(
                "wasm",
                "compatibility_shim",
                name,
                TYPES_PATH,
                format!("generated wasm-bindgen module export {name}"),
            ));
        }
    }
    Ok(())
}

fn discover_websocket_declarations(
    root: &Path,
    declarations: &mut Vec<RawMechanicalDeclaration>,
) -> Result<(), String> {
    const PATH: &str = "crates/vibelang-http/src/websocket.rs";
    let source = read(root, PATH)?;
    for event in string_array_after(&source, "\"events\":")? {
        declarations.push(raw_declaration(
            "websocket",
            "event",
            event.clone(),
            PATH,
            format!("advertised event {event}"),
        ));
    }
    for action in string_array_after(&source, "\"commands\":")? {
        declarations.push(raw_declaration(
            "websocket",
            "action",
            action.clone(),
            PATH,
            format!("advertised command {action}"),
        ));
    }
    Ok(())
}

fn discover_lsp_declarations(
    root: &Path,
    declarations: &mut Vec<RawMechanicalDeclaration>,
) -> Result<(), String> {
    const TOKENS: &str = "crates/vibelang-lsp/src/features/semantic_tokens.rs";
    let source = read(root, TOKENS)?;
    for (constant, kind, prefix) in [
        ("TOKEN_TYPES", "token_type", "SemanticTokenType::"),
        (
            "TOKEN_MODIFIERS",
            "token_modifier",
            "SemanticTokenModifier::",
        ),
    ] {
        let body = const_array_body(&source, constant)?;
        for line in body.lines() {
            let Some((_, tail)) = line.split_once(prefix) else {
                continue;
            };
            let name = tail
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .next()
                .unwrap_or_default();
            if !name.is_empty() {
                declarations.push(raw_declaration(
                    "lsp",
                    kind,
                    format!("{constant}.{name}"),
                    TOKENS,
                    format!("{constant}::{name}"),
                ));
            }
        }
    }

    const ANALYSIS: &str = "crates/vibelang-lsp/src/analysis/mod.rs";
    let source = read(root, ANALYSIS)?;
    let lint_block = source
        .split_once("// Run linting passes")
        .and_then(|(_, tail)| tail.split_once("result\n"))
        .map(|(block, _)| block)
        .ok_or_else(|| "LSP diagnostic rule invocation block disappeared".to_string())?;
    for line in lint_block.lines() {
        let line = line.trim();
        if let Some((name, _)) = line.split_once('(') {
            if name.starts_with("lint_") {
                declarations.push(raw_declaration(
                    "lsp",
                    "diagnostic_rule",
                    name,
                    ANALYSIS,
                    name,
                ));
            }
        }
    }
    Ok(())
}

fn discover_vscode_declarations(
    root: &Path,
    declarations: &mut Vec<RawMechanicalDeclaration>,
) -> Result<(), String> {
    const PACKAGE: &str = "vscode-extension/package.json";
    let package: Value = serde_json::from_str(&read(root, PACKAGE)?)
        .map_err(|error| format!("{PACKAGE}: {error}"))?;
    let contributed = package["contributes"]["commands"]
        .as_array()
        .ok_or_else(|| "VS Code package has no contributed commands".to_string())?
        .iter()
        .filter_map(|command| command["command"].as_str().map(str::to_string))
        .collect::<BTreeSet<_>>();
    let mut registered: BTreeMap<String, String> = BTreeMap::new();
    for entry in WalkDir::new(root.join("vscode-extension/src")) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("ts")
        {
            continue;
        }
        let relative = relative_path(root, entry.path())?;
        let source = fs::read_to_string(entry.path()).map_err(|error| error.to_string())?;
        for command in quoted_arguments_after(&source, "registerCommand") {
            registered.insert(command, relative.clone());
        }
    }
    let registered_names = registered.keys().cloned().collect::<BTreeSet<_>>();
    if contributed != registered_names {
        return Err(format!(
            "VS Code contributed/registered command drift: contributed-only={:?}, registered-only={:?}",
            contributed.difference(&registered_names).collect::<Vec<_>>(),
            registered_names.difference(&contributed).collect::<Vec<_>>()
        ));
    }
    for command in contributed {
        let mut declaration = raw_declaration(
            "vscode",
            "command",
            command.clone(),
            PACKAGE,
            format!("contributes.commands.{command}"),
        );
        declaration.source_anchors.push((
            registered[&command].clone(),
            format!("registerCommand({command})"),
        ));
        declarations.push(declaration);
    }
    let settings = package["contributes"]["configuration"]["properties"]
        .as_object()
        .ok_or_else(|| "VS Code package has no contributed settings".to_string())?;
    for setting in settings.keys() {
        declarations.push(raw_declaration(
            "vscode",
            "setting",
            setting,
            PACKAGE,
            format!("contributes.configuration.properties.{setting}"),
        ));
    }
    Ok(())
}

fn discover_emacs_declarations(
    root: &Path,
    declarations: &mut Vec<RawMechanicalDeclaration>,
) -> Result<(), String> {
    for entry in WalkDir::new(root.join("emacs")).max_depth(1) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("el")
        {
            continue;
        }
        let relative = relative_path(root, entry.path())?;
        let source = fs::read_to_string(entry.path()).map_err(|error| error.to_string())?;
        let lines = source.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if let Some(name) = lisp_form_name(trimmed, "(defcustom ") {
                declarations.push(raw_declaration(
                    "emacs",
                    "setting",
                    name,
                    &relative,
                    format!("defcustom {name}"),
                ));
            }
            if !trimmed.starts_with("(interactive") {
                continue;
            }
            let command = lines[..=index].iter().rev().find_map(|candidate| {
                let candidate = candidate.trim_start();
                ["(defun ", "(define-minor-mode ", "(define-derived-mode "]
                    .iter()
                    .find_map(|prefix| lisp_form_name(candidate, prefix))
            });
            if let Some(command) = command {
                declarations.push(raw_declaration(
                    "emacs",
                    "command",
                    command,
                    &relative,
                    format!("interactive {command}"),
                ));
            }
        }
    }
    Ok(())
}

fn discover_markdown_declarations(
    root: &Path,
    baseline: &ArtifactBaseline,
    declarations: &mut Vec<RawMechanicalDeclaration>,
) -> Result<(), String> {
    let docs = baseline
        .categories
        .get("docs")
        .ok_or_else(|| "M00 baseline has no docs category".to_string())?;
    for file in &docs.files {
        let source = read(root, &file.path)?;
        let mut fence_index = 0usize;
        for (line_index, line) in source.lines().enumerate() {
            let Some(language) = line.trim().strip_prefix("```") else {
                continue;
            };
            if language.is_empty() {
                continue;
            }
            fence_index += 1;
            let language = language.split_whitespace().next().unwrap_or(language);
            let kind = if matches!(
                language,
                "rhai" | "vibe" | "bash" | "sh" | "shell" | "console"
            ) {
                "contract_block"
            } else {
                "non_contract_block"
            };
            declarations.push(raw_declaration(
                "docs",
                kind,
                format!("{}#{fence_index}:{language}", file.path),
                &file.path,
                format!("fenced block at line {}", line_index + 1),
            ));
        }
    }
    Ok(())
}

fn discover_baseline_file_declarations(
    baseline: &ArtifactBaseline,
    declarations: &mut Vec<RawMechanicalDeclaration>,
) -> Result<(), String> {
    let fixtures = baseline
        .categories
        .get("fixtures")
        .ok_or_else(|| "M00 baseline has no fixtures category".to_string())?;
    for file in &fixtures.files {
        declarations.push(raw_declaration(
            "fixtures",
            "fixture",
            &file.path,
            &file.path,
            "M00 executable/negative fixture",
        ));
    }
    let packages = baseline
        .categories
        .get("packages")
        .ok_or_else(|| "M00 baseline has no packages category".to_string())?;
    for file in &packages.files {
        let kind = if file.path.ends_with("Cargo.lock") || file.path.ends_with("package-lock.json")
        {
            "lockfile"
        } else if file.path.ends_with("Cargo.toml") || file.path.ends_with("package.json") {
            "manifest"
        } else {
            return Err(format!("unclassified package inventory path {}", file.path));
        };
        declarations.push(raw_declaration(
            "packages",
            kind,
            &file.path,
            &file.path,
            format!("package inventory {}", file.path),
        ));
    }
    Ok(())
}

fn has_attribute(attributes: &[syn::Attribute], name: &str) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.path().is_ident(name))
}

fn attributes_text(attributes: &[syn::Attribute]) -> String {
    attributes
        .iter()
        .map(|attribute| attribute.meta.to_token_stream().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn wasm_export_name(rust_name: &str, attributes: &[syn::Attribute]) -> String {
    let text = attributes_text(attributes);
    let marker = "js_name =";
    if let Some(value) = text.split_once(marker).map(|(_, tail)| tail) {
        return value
            .split(|character: char| character == ',' || character == ')')
            .next()
            .unwrap_or(rust_name)
            .trim()
            .trim_matches('"')
            .to_string();
    }
    to_lower_camel(rust_name)
}

fn to_lower_camel(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase = false;
    for character in value.chars() {
        if character == '_' {
            uppercase = true;
        } else if uppercase {
            output.push(character.to_ascii_uppercase());
            uppercase = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn to_kebab_case(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index != 0 {
                output.push('-');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn string_array_after(source: &str, marker: &str) -> Result<Vec<String>, String> {
    let tail = source
        .split_once(marker)
        .map(|(_, tail)| tail)
        .ok_or_else(|| format!("source has no {marker} declaration"))?;
    let body = tail
        .split_once('[')
        .and_then(|(_, tail)| tail.split_once(']'))
        .map(|(body, _)| body)
        .ok_or_else(|| format!("source has malformed {marker} array"))?;
    let mut values = BTreeSet::new();
    for part in body.split(',') {
        let value = part.trim().trim_matches('"');
        if !value.is_empty() {
            values.insert(value.to_string());
        }
    }
    if values.is_empty() {
        return Err(format!("source has empty {marker} array"));
    }
    Ok(values.into_iter().collect())
}

fn const_array_body<'a>(source: &'a str, constant: &str) -> Result<&'a str, String> {
    source
        .split_once(&format!("const {constant}:"))
        .and_then(|(_, tail)| tail.split_once("&["))
        .and_then(|(_, tail)| tail.split_once("];"))
        .map(|(body, _)| body)
        .ok_or_else(|| format!("LSP constant {constant} is not a source array"))
}

fn quoted_arguments_after(source: &str, marker: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut remainder = source;
    while let Some((_, tail)) = remainder.split_once(marker) {
        let Some(open) = tail.find('(') else { break };
        let tail = tail[open + 1..].trim_start();
        let Some(quote) = tail
            .chars()
            .next()
            .filter(|character| *character == '\'' || *character == '"')
        else {
            remainder = &tail[tail.len().min(1)..];
            continue;
        };
        let tail = &tail[quote.len_utf8()..];
        let Some(end) = tail.find(quote) else { break };
        values.push(tail[..end].to_string());
        remainder = &tail[end + quote.len_utf8()..];
    }
    values
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map_err(|error| error.to_string())
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn lisp_form_name<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix)?.split_whitespace().next()
}

struct Discovery {
    v1: PublicApiManifest,
    v1_json: String,
    http: HttpSnapshot,
    baseline: ArtifactBaseline,
    fragments: FragmentSet,
    mechanical: Vec<MechanicalDeclaration>,
}

fn discover(root: &Path) -> Result<Discovery, String> {
    let v1 = public_api::build_manifest(root)?;
    let v1_json = to_pretty_json(&v1).map_err(|error| error.to_string())?;
    let committed_v1 = read(root, V1_MANIFEST_PATH)?;
    require_bytes("generated v1 manifest", &v1_json, &committed_v1)?;
    require_hash(
        V1_MANIFEST_PATH,
        committed_v1.as_bytes(),
        ACCEPTED_V1_MANIFEST_SHA256,
    )?;

    let http_json = public_artifacts::current_http_snapshot_json(root)?;
    let committed_http = read(root, V1_HTTP_PATH)?;
    require_bytes("generated HTTP v1 snapshot", &http_json, &committed_http)?;
    require_hash(
        V1_HTTP_PATH,
        committed_http.as_bytes(),
        ACCEPTED_V1_HTTP_SHA256,
    )?;
    let http: HttpSnapshot =
        serde_json::from_str(&http_json).map_err(|error| format!("{V1_HTTP_PATH}: {error}"))?;

    validate_v1_inventory(&v1, &http)?;
    let baseline: ArtifactBaseline = serde_json::from_str(&read(root, BASELINE_PATH)?)
        .map_err(|error| format!("{BASELINE_PATH}: {error}"))?;
    validate_accepted_projections(root, &baseline)?;
    let fragments = load_fragments(root)?;
    let mechanical = discover_mechanical_declarations(root, &baseline)?;

    Ok(Discovery {
        v1,
        v1_json,
        http,
        baseline,
        fragments,
        mechanical,
    })
}

fn compose(root: &Path, discovery: &Discovery) -> Result<BTreeMap<&'static str, String>, String> {
    let mut manifest = build_v2(discovery)?;
    let composition = validate_fragment_join(discovery, &manifest, &discovery.fragments)?;
    apply_fragments(&mut manifest, &discovery.fragments)?;
    manifest.stats.insert(
        "semantic_fragments".into(),
        composition.accounting.semantic_records,
    );
    manifest.stats.insert(
        "orphan_records".into(),
        composition.accounting.orphan_records,
    );
    manifest.stats.insert(
        "unclassified_records".into(),
        composition.accounting.unclassified_records,
    );
    manifest.validate().map_err(|error| error.to_string())?;

    let v2_json = vibelang_api_manifest::v2::to_pretty_json_v2(&manifest)
        .map_err(|error| error.to_string())?;
    let round_trip = vibelang_api_manifest::v2::parse_v2_manifest(&v2_json)
        .map_err(|error| error.to_string())?;
    if round_trip != manifest {
        return Err("schema-v2 serialization did not round-trip exactly".into());
    }
    let digest = canonical_sha256_hex(&manifest).map_err(|error| error.to_string())?;

    let coverage = build_coverage(root, discovery, &manifest, &digest, &composition.accounting)?;
    let debt = build_debt(root, &manifest, &digest, &composition.accounting)?;
    debt.validate(&contract_ids(&manifest))?;
    let diff = build_diff(discovery, &digest)?;
    diff.report.validate().map_err(|error| error.to_string())?;
    let packages =
        build_package_index(root, discovery, &manifest, &digest, &composition.accounting)?;

    let mut outputs = BTreeMap::new();
    outputs.insert(V2_PATH, v2_json);
    outputs.insert(COVERAGE_PATH, pretty(&coverage)?);
    outputs.insert(DEBT_PATH, pretty(&debt)?);
    outputs.insert(DIFF_PATH, pretty(&diff)?);
    outputs.insert(PACKAGE_INDEX_PATH, pretty(&packages)?);
    Ok(outputs)
}

fn build_v2(discovery: &Discovery) -> Result<PublicApiManifestV2, String> {
    let websocket_events = discovery
        .mechanical
        .iter()
        .filter(|declaration| declaration.surface == "websocket" && declaration.kind == "event")
        .collect::<Vec<_>>();
    if websocket_events.is_empty() {
        return Err("WebSocket mechanical discovery produced no events".into());
    }
    let conditional_capability_id = semantic_id("capability", "legacy|declared-condition");
    let capabilities = vec![Capability {
        metadata: metadata(
            conditional_capability_id.clone(),
            "legacy declared condition",
            "vibelang-api-manifest",
            BASELINE_PATH,
            "api-unification M02 capability bridge",
            Derivation::ExplicitSemantics,
            available(),
            Vec::new(),
        ),
        detection_source: "v1 cfg/target/feature/plugin/runtime-condition evidence".into(),
        dependencies: Vec::new(),
        conflicts: Vec::new(),
        runtime_states: [
            CapabilityState::Available,
            CapabilityState::Degraded,
            CapabilityState::Unavailable,
            CapabilityState::Unknown,
        ]
        .into_iter()
        .collect(),
        projection_rules: vec![
            "M02 preserves the v1 condition as evidence; runtime evaluation lands at M05".into(),
        ],
    }];

    let consumer_ids: BTreeMap<_, _> = discovery
        .baseline
        .categories
        .keys()
        .map(|category| {
            (
                category.clone(),
                semantic_id("consumer", &format!("baseline|{category}")),
            )
        })
        .collect();
    let manifest_consumer_id = consumer_ids
        .get("manifest")
        .ok_or_else(|| "M00 baseline has no manifest consumer category".to_string())?
        .clone();

    let mut entries = discovery
        .v1
        .entries
        .iter()
        .map(|entry| entry_v2(entry, &conditional_capability_id, &manifest_consumer_id))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));

    let http_type_ids: BTreeMap<String, String> = discovery
        .http
        .types
        .iter()
        .map(|api_type| {
            (
                http_type_key(api_type),
                semantic_id(
                    "type",
                    &format!("http|{}|{}", api_type.source, api_type.name),
                ),
            )
        })
        .collect();
    let mut http_type_ids_by_name: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for api_type in &discovery.http.types {
        http_type_ids_by_name
            .entry(api_type.name.clone())
            .or_default()
            .push((
                api_type.source.clone(),
                http_type_ids[&http_type_key(api_type)].clone(),
            ));
    }
    for candidates in http_type_ids_by_name.values_mut() {
        candidates.sort();
    }
    let mut scalar_shapes = BTreeSet::new();
    for api_type in &discovery.http.types {
        for field in &api_type.fields {
            if referenced_http_type(&field.rust_type, &api_type.source, &http_type_ids_by_name)?
                .is_none()
            {
                scalar_shapes.insert(field.rust_type.clone());
            }
        }
    }
    let scalar_type_ids = scalar_shapes
        .iter()
        .map(|scalar| {
            (
                scalar.clone(),
                semantic_id("type", &format!("http-scalar|{scalar}")),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let event_type_ids = websocket_events
        .iter()
        .map(|event| {
            let shape = format!("websocket payload {}", event.name);
            (shape.clone(), semantic_id("type", &format!("{shape}|v1")))
        })
        .collect::<BTreeMap<_, _>>();

    let mut types = discovery
        .http
        .types
        .iter()
        .map(|api_type| {
            http_type_v2(
                api_type,
                &http_type_ids,
                &http_type_ids_by_name,
                &scalar_type_ids,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    for scalar in scalar_shapes {
        let id = scalar_type_ids[&scalar].clone();
        types.push(ApiType {
            metadata: metadata(
                id,
                scalar.clone(),
                "vibelang-http",
                V1_HTTP_PATH,
                format!("wire shape {scalar}"),
                Derivation::GeneratedProjection,
                available(),
                Vec::new(),
            ),
            kind: TypeKind::Alias,
            fields: Vec::new(),
            variants: Vec::new(),
        });
    }
    for event in &websocket_events {
        let shape = format!("websocket payload {}", event.name);
        types.push(ApiType {
            metadata: metadata(
                event_type_ids[&shape].clone(),
                shape.clone(),
                "vibelang-http",
                "crates/vibelang-http/src/websocket.rs",
                format!("WebSocketEvent::{}", event.name),
                Derivation::RustAst,
                available(),
                Vec::new(),
            ),
            kind: TypeKind::Record,
            fields: Vec::new(),
            variants: Vec::new(),
        });
    }
    for declaration in discovery
        .mechanical
        .iter()
        .filter(|declaration| declaration.contract_node && declaration.kind != "event")
    {
        types.push(mechanical_type(declaration));
    }
    types.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));

    let default_error_type_id = http_type_ids
        .get("crates/vibelang-http/src/models.rs|ErrorResponse")
        .ok_or_else(|| "HTTP snapshot has no models::ErrorResponse type".to_string())?;
    let midi_error_type_id = http_type_ids
        .get("crates/vibelang-http/src/routes/midi.rs|ErrorResponse")
        .ok_or_else(|| "HTTP snapshot has no midi::ErrorResponse type".to_string())?;
    let mut operations = discovery
        .http
        .routes
        .iter()
        .map(|route| {
            let error_type_id = if route.handler.starts_with("routes::midi::") {
                midi_error_type_id
            } else {
                default_error_type_id
            };
            http_operation(route, error_type_id, &conditional_capability_id)
        })
        .collect::<Vec<_>>();
    operations.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));

    let mut events = websocket_events
        .iter()
        .map(|event| Event {
            metadata: metadata(
                event.id.clone(),
                event.name.clone(),
                "vibelang-http",
                "crates/vibelang-http/src/websocket.rs",
                format!("WebSocketEvent::{}", event.name),
                Derivation::RustAst,
                available(),
                Vec::new(),
            ),
            payload_type_id: event_type_ids[&format!("websocket payload {}", event.name)].clone(),
            producer_operation_id: None,
            protocol_version: "v1".into(),
            ordering: EventOrdering::Unordered,
            revision_relation: RevisionRelation::TelemetryOnly,
            delivery: EventDelivery::PollDerived,
            loss_detection: LossDetection::NotApplicable,
            resync_operation_id: None,
        })
        .collect::<Vec<_>>();
    events.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));

    let (consumers, consumer_accounting) = build_consumers(
        discovery,
        &consumer_ids,
        &entries,
        &types,
        &operations,
        &events,
    )?;
    let coverage = build_manifest_coverage(&consumer_ids, &consumer_accounting)?;

    let mut stats = discovery.v1.stats.clone();
    stats.insert("http_routes".into(), discovery.http.routes.len() as u64);
    stats.insert("http_types".into(), discovery.http.types.len() as u64);
    stats.insert(
        "http_fields".into(),
        discovery
            .http
            .types
            .iter()
            .map(|api_type| api_type.fields.len() as u64)
            .sum(),
    );
    stats.insert("websocket_events".into(), events.len() as u64);
    Ok(PublicApiManifestV2 {
        schema: SCHEMA_URI_V2.into(),
        schema_version: SCHEMA_VERSION_V2,
        api_version: discovery.v1.api_version.clone(),
        generator: Generator {
            name: GENERATOR_NAME.into(),
            format_version: 1,
        },
        entries,
        types,
        operations,
        events,
        capabilities,
        consumers,
        coverage,
        stats,
    })
}

fn mechanical_type(declaration: &MechanicalDeclaration) -> ApiType {
    let derivation = match declaration.surface.as_str() {
        "fixtures" => Derivation::BehavioralFixture,
        "docs" | "packages" | "vscode" | "emacs" => Derivation::Catalog,
        _ => Derivation::RustAst,
    };
    ApiType {
        metadata: NodeMetadata {
            id: declaration.id.clone(),
            name: format!(
                "mechanical {} {} {}",
                declaration.surface, declaration.kind, declaration.name
            ),
            aliases: Vec::new(),
            stability: stable(),
            availability: available(),
            ownership: ownership(&declaration.owner),
            source_anchors: declaration
                .source_anchors
                .iter()
                .map(|(path, symbol)| ProvenanceAnchor {
                    path: path.clone(),
                    symbol: symbol.clone(),
                    line: None,
                    derivation,
                })
                .collect(),
            test_anchors: Vec::new(),
        },
        kind: TypeKind::Alias,
        fields: Vec::new(),
        variants: Vec::new(),
    }
}

fn entry_v2(
    entry: &ApiEntry,
    conditional_capability_id: &str,
    manifest_consumer_id: &str,
) -> Result<ApiEntryV2, String> {
    let lifecycle = lifecycle(entry);
    let entry_owner = entry_owner(entry);
    let mut overloads = entry
        .overloads
        .iter()
        .map(|overload| {
            let mut parameters = overload
                .parameters
                .iter()
                .map(|parameter| ParameterV2 {
                    position: parameter.position,
                    name: parameter.name.clone(),
                    accepted_types: parameter.accepted_types.clone(),
                    optional: parameter.optional,
                    default: parameter.default.clone(),
                    value_contract: Facet::NotApplicable {
                        reason: "quantity/value semantics land at M05 without changing v1 input"
                            .into(),
                    },
                })
                .collect::<Vec<_>>();
            parameters.sort_by_key(|parameter| parameter.position);
            Ok(vibelang_api_manifest::v2::OverloadV2 {
                metadata: NodeMetadata {
                    id: overload.id.clone(),
                    name: overload.signature.clone(),
                    aliases: aliases(&overload.id, &overload.aliases),
                    stability: stable(),
                    availability: availability_v2(
                        &overload.availability,
                        conditional_capability_id,
                    ),
                    ownership: ownership(entry_owner),
                    source_anchors: anchors(&overload.source_anchors, source_derivation(entry)),
                    test_anchors: Vec::new(),
                },
                signature: overload.signature.clone(),
                parameters,
                return_type: overload.return_type.clone(),
                returns_receiver: overload.returns_receiver,
                lifecycle: Facet::Applicable {
                    value: lifecycle.clone(),
                },
                value_contract: Facet::NotApplicable {
                    reason: "overload-wide value semantics land at M05".into(),
                },
                failure: failure(&overload.boundary),
                operation_ids: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    overloads.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));

    Ok(ApiEntryV2 {
        metadata: NodeMetadata {
            id: entry.id.clone(),
            name: entry.registered_name.clone(),
            aliases: aliases(&entry.id, &entry.aliases),
            stability: match entry.availability.status.as_str() {
                "importable" => Stability {
                    level: StabilityLevel::UnsupportedImportable,
                    since: None,
                    deprecated_since: None,
                    replacement_id: None,
                    reason: Some("import-callable internal v1 declaration".into()),
                    removal_not_before: None,
                },
                _ => stable(),
            },
            availability: availability_v2(&entry.availability, conditional_capability_id),
            ownership: ownership(entry_owner),
            source_anchors: anchors(&entry.source_anchors, source_derivation(entry)),
            test_anchors: anchors(&entry.test_anchors, Derivation::BehavioralFixture),
        },
        surface: entry.surface.clone(),
        kind: entry.kind.clone(),
        registered_name: entry.registered_name.clone(),
        receiver: entry.receiver.clone(),
        overloads,
        details: entry.details.clone(),
        lifecycle: Facet::Applicable { value: lifecycle },
        operation_ids: Vec::new(),
        consumer_ids: vec![manifest_consumer_id.into()],
    })
}

fn http_type_v2(
    api_type: &HttpType,
    type_ids: &BTreeMap<String, String>,
    type_ids_by_name: &BTreeMap<String, Vec<(String, String)>>,
    scalar_type_ids: &BTreeMap<String, String>,
) -> Result<ApiType, String> {
    let type_id = type_ids
        .get(&http_type_key(api_type))
        .ok_or_else(|| format!("missing HTTP type ID for {}", api_type.name))?
        .clone();
    let direction = field_direction(&api_type.derives);
    let mut fields = api_type
        .fields
        .iter()
        .map(|field| {
            let referenced =
                referenced_http_type(&field.rust_type, &api_type.source, type_ids_by_name)?
                    .or_else(|| scalar_type_ids.get(&field.rust_type).cloned())
                    .ok_or_else(|| {
                        format!(
                            "HTTP field {}.{} has no discovered type for {}",
                            api_type.name, field.name, field.rust_type
                        )
                    })?;
            Ok(Field {
                metadata: metadata(
                    semantic_id(
                        "field",
                        &format!("http|{}|{}|{}", api_type.source, api_type.name, field.name),
                    ),
                    format!("{}.{}", api_type.name, field.name),
                    "vibelang-http",
                    &api_type.source,
                    format!("{}::{}", api_type.name, field.name),
                    Derivation::RustAst,
                    available(),
                    Vec::new(),
                ),
                serialized_name: serialized_field_name(field),
                host_name: field.name.clone(),
                direction,
                required: !field.rust_type.starts_with("Option <"),
                type_id: referenced,
                default: None,
                value_contract: Facet::NotApplicable {
                    reason: "field value semantics land at M05/M11".into(),
                },
                bindings: Vec::new(),
                observation: if matches!(
                    direction,
                    FieldDirection::Output | FieldDirection::Bidirectional
                ) {
                    Facet::Applicable {
                        value: ObservationContract {
                            authoritative_source: "current HTTP serializer projection".into(),
                            consistency: ConsistencyPoint::ResponseSnapshot,
                            absent_behavior: UnavailableBehavior::StructuredError,
                        },
                    }
                } else {
                    Facet::NotApplicable {
                        reason: "input-only field".into(),
                    }
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    fields.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    let mut variants = api_type
        .variants
        .iter()
        .map(|variant| EnumVariant {
            id: semantic_id(
                "variant",
                &format!("http|{}|{}|{variant}", api_type.source, api_type.name),
            ),
            serialized_name: variant.clone(),
        })
        .collect::<Vec<_>>();
    variants.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(ApiType {
        metadata: metadata(
            type_id,
            api_type.name.clone(),
            "vibelang-http",
            &api_type.source,
            api_type.name.clone(),
            Derivation::RustAst,
            available(),
            Vec::new(),
        ),
        kind: if api_type.kind == "enum" {
            TypeKind::Enum
        } else if api_type.name == "ErrorResponse" {
            TypeKind::ErrorEnvelope
        } else {
            TypeKind::Record
        },
        fields,
        variants,
    })
}

fn http_operation(
    route: &HttpRoute,
    error_type_id: &str,
    conditional_capability_id: &str,
) -> Operation {
    let operation_id = semantic_id(
        "operation",
        &format!("http|{}|{}", route.method, route.path),
    );
    let is_read = route.method == "GET";
    let availability = if route.availability.is_empty() {
        available()
    } else {
        AvailabilityV2 {
            status: AvailabilityStatus::Conditional,
            when: Some(CapabilityExpression::Ref {
                capability_id: conditional_capability_id.into(),
            }),
            on_unavailable: UnavailableBehavior::StructuredError,
            evidence: route.availability.clone(),
        }
    };
    let tests = if is_read {
        Vec::new()
    } else {
        vec![ProvenanceAnchor {
            path: "tests/fixtures/api-unification/v1/negative/stale-success.json".into(),
            symbol: format!("{} {}", route.method, route.path),
            line: None,
            derivation: Derivation::BehavioralFixture,
        }]
    };
    let binding_id = semantic_id("binding", &format!("http|{}|{}", route.method, route.path));
    Operation {
        metadata: metadata(
            operation_id,
            format!("{} {}", route.method, route.path),
            "vibelang-http",
            "crates/vibelang-http/src/lib.rs",
            route.handler.clone(),
            Derivation::RustAst,
            availability.clone(),
            tests,
        ),
        kind: if is_read {
            OperationKind::Read
        } else {
            OperationKind::Mutation
        },
        request_type_id: None,
        response_type_ids: Vec::new(),
        error_type_id: error_type_id.into(),
        effects: Vec::new(),
        idempotency: if is_read {
            Idempotency::Yes
        } else {
            Idempotency::Conditional
        },
        effect_timing: Facet::NotApplicable {
            reason: "runtime timing is semantic-fragment owned".into(),
        },
        atomicity: Facet::NotApplicable {
            reason: "runtime atomicity is semantic-fragment owned".into(),
        },
        revision: Facet::NotApplicable {
            reason: "M02 records current v1 declaration truth; canonical revisions land at M03"
                .into(),
        },
        receipt: Facet::NotApplicable {
            reason: "M02 records current stale v1 carriers as compatibility debt".into(),
        },
        consistency: ConsistencyPoint::ResponseSnapshot,
        security_capability_ids: vec![conditional_capability_id.into()],
        bindings: vec![SurfaceBinding {
            metadata: metadata(
                binding_id,
                format!("{} {}", route.method, route.path),
                "vibelang-http",
                "crates/vibelang-http/src/lib.rs",
                route.handler.clone(),
                Derivation::RustAst,
                availability,
                Vec::new(),
            ),
            details: BindingDetails::Http {
                method: route.method.clone(),
                path: route.path.clone(),
                path_type_ids: Vec::new(),
                query_type_ids: Vec::new(),
                header_type_ids: Vec::new(),
                body_type_id: None,
                successes: Vec::<HttpSuccess>::new(),
                error_type_id: error_type_id.into(),
                protocol_version: "v1".into(),
                authentication_capability_id: conditional_capability_id.into(),
                idempotency_header: None,
                revision_header: None,
            },
        }],
    }
}

fn build_consumers(
    discovery: &Discovery,
    consumer_ids: &BTreeMap<String, String>,
    entries: &[ApiEntryV2],
    types: &[ApiType],
    operations: &[Operation],
    events: &[Event],
) -> Result<(Vec<Consumer>, BTreeMap<String, ConsumerAccounting>), String> {
    let manifest_ids = entries
        .iter()
        .flat_map(|entry| {
            std::iter::once(entry.metadata.id.clone()).chain(
                entry
                    .overloads
                    .iter()
                    .map(|overload| overload.metadata.id.clone()),
            )
        })
        .chain(types.iter().flat_map(|api_type| {
            std::iter::once(api_type.metadata.id.clone())
                .chain(
                    api_type
                        .fields
                        .iter()
                        .map(|field| field.metadata.id.clone()),
                )
                .chain(api_type.variants.iter().map(|variant| variant.id.clone()))
        }))
        .chain(operations.iter().flat_map(|operation| {
            std::iter::once(operation.metadata.id.clone()).chain(
                operation
                    .bindings
                    .iter()
                    .map(|binding| binding.metadata.id.clone()),
            )
        }))
        .chain(events.iter().map(|event| event.metadata.id.clone()))
        .collect::<BTreeSet<_>>();
    let http_ids = types
        .iter()
        .filter(|api_type| {
            api_type.metadata.source_anchors.iter().any(|anchor| {
                anchor.path == V1_HTTP_PATH
                    || discovery
                        .http
                        .types
                        .iter()
                        .any(|raw| raw.source == anchor.path)
            })
        })
        .flat_map(|api_type| {
            std::iter::once(api_type.metadata.id.clone())
                .chain(
                    api_type
                        .fields
                        .iter()
                        .map(|field| field.metadata.id.clone()),
                )
                .chain(api_type.variants.iter().map(|variant| variant.id.clone()))
        })
        .chain(operations.iter().flat_map(|operation| {
            std::iter::once(operation.metadata.id.clone()).chain(
                operation
                    .bindings
                    .iter()
                    .map(|binding| binding.metadata.id.clone()),
            )
        }))
        .collect::<BTreeSet<_>>();
    let editor = editor_consumer_accounting(discovery)?;
    let mut consumers = Vec::new();
    let mut accounting = BTreeMap::new();
    for (category, baseline) in &discovery.baseline.categories {
        let id = consumer_ids[category].clone();
        let mut current = match category.as_str() {
            "manifest" => ConsumerAccounting::included(manifest_ids.clone()),
            "http" => ConsumerAccounting::included(http_ids.clone()),
            "rhai_editor" => editor.clone(),
            _ => ConsumerAccounting::default(),
        };
        for declaration in &discovery.mechanical {
            let Some(disposition) = declaration.consumers.get(category) else {
                continue;
            };
            match disposition {
                MechanicalDisposition::Included => {
                    current.included.insert(declaration.id.clone());
                }
                MechanicalDisposition::Excluded(reason) => {
                    current
                        .exclusions
                        .insert(declaration.id.clone(), (*reason, declaration.owner.clone()));
                }
            }
        }
        if current.included.is_empty() && current.exclusions.is_empty() {
            return Err(format!(
                "consumer {category} has no source-derived eligible declarations or exclusions"
            ));
        }
        let included_ids = current.included.iter().cloned().collect::<Vec<_>>();
        let exclusions = current
            .exclusions
            .iter()
            .map(|(id, (reason, owner))| ConsumerExclusion {
                id: id.clone(),
                reason: *reason,
                owner: owner.clone(),
            })
            .collect();
        let package = if category == "packages" {
            Facet::Applicable {
                value: PackageContract {
                    name: "vibelang workspace package inventory".into(),
                    version_owner: "workspace package manifests".into(),
                    required_members: baseline
                        .files
                        .iter()
                        .map(|file| file.path.clone())
                        .collect(),
                },
            }
        } else {
            Facet::NotApplicable {
                reason: "consumer is a checked projection, not an archive owner".into(),
            }
        };
        consumers.push(Consumer {
            metadata: metadata(
                id,
                format!("M00 {category} projection"),
                "xtask",
                BASELINE_PATH,
                format!("categories.{category}"),
                Derivation::ExplicitSemantics,
                available(),
                Vec::new(),
            ),
            source_projections: baseline
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect(),
            eligibility: Eligibility {
                surfaces: vec![category.clone()],
                kinds: vec!["source_derived_mechanical_declaration".into()],
                stability_levels: [StabilityLevel::Stable].into_iter().collect(),
                capability_ids: Vec::new(),
            },
            included_ids,
            exclusions,
            host: Facet::NotApplicable {
                reason: "consumer has no WASM host contract".into(),
            },
            coverage_policy: Facet::NotApplicable {
                reason: "consumer has no explicit M02 coverage-policy refinement".into(),
            },
            package,
        });
        accounting.insert(category.clone(), current);
    }
    consumers.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    Ok((consumers, accounting))
}

fn build_manifest_coverage(
    consumer_ids: &BTreeMap<String, String>,
    accounting: &BTreeMap<String, ConsumerAccounting>,
) -> Result<BTreeMap<String, CoverageRecord>, String> {
    let mut coverage = BTreeMap::new();
    for (category, consumer) in accounting {
        let denominator = consumer.eligible_count() as u64;
        let mut exclusions_by_reason = BTreeMap::new();
        for (reason, _) in consumer.exclusions.values() {
            *exclusions_by_reason
                .entry(exclusion_reason_name(*reason).into())
                .or_insert(0) += 1;
        }
        coverage.insert(
            consumer_ids[category].clone(),
            CoverageRecord {
                numerator: consumer.included.len() as u64,
                denominator,
                exclusions_by_reason,
                unresolved_ids: consumer.unresolved.iter().cloned().collect(),
                stale_ids: consumer.stale.iter().cloned().collect(),
                base_denominator: Some(denominator),
            },
        );
    }
    Ok(coverage)
}

#[derive(Debug, Clone, Default)]
struct ConsumerAccounting {
    included: BTreeSet<String>,
    exclusions: BTreeMap<String, (ExclusionReason, String)>,
    unresolved: BTreeSet<String>,
    stale: BTreeSet<String>,
}

impl ConsumerAccounting {
    fn included(included: BTreeSet<String>) -> Self {
        Self {
            included,
            ..Self::default()
        }
    }

    fn eligible_count(&self) -> usize {
        self.included.len() + self.exclusions.len() + self.unresolved.len()
    }
}

fn exclusion_reason_name(reason: ExclusionReason) -> &'static str {
    match reason {
        ExclusionReason::IntentionalCuration => "intentional_curation",
        ExclusionReason::UnsupportedHost => "unsupported_host",
        ExclusionReason::Deprecated => "deprecated",
        ExclusionReason::NotApplicable => "not_applicable",
    }
}

fn editor_consumer_accounting(discovery: &Discovery) -> Result<ConsumerAccounting, String> {
    let mut accounting = ConsumerAccounting::default();
    for entry in discovery.v1.entries.iter().filter(|entry| {
        matches!(
            entry.surface.as_str(),
            "rhai" | "dsp_rhai" | "rhai_extension"
        ) && entry.kind == "function"
            && !matches!(
                entry.availability.status.as_str(),
                "quarantined" | "documentation_only"
            )
    }) {
        accounting
            .included
            .extend(entry.overloads.iter().map(|overload| overload.id.clone()));
    }

    let stdlib: Value = serde_json::from_str(&read(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
        "vscode-extension/src/data/stdlib.json",
    )?)
    .map_err(|error| error.to_string())?;
    let stdlib_names = stdlib["synthdefs"]
        .as_array()
        .ok_or_else(|| "editor stdlib projection has no synthdefs".to_string())?
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect::<BTreeSet<_>>();
    for entry in discovery
        .v1
        .entries
        .iter()
        .filter(|entry| entry.surface == "stdlib")
    {
        if stdlib_names.contains(entry.registered_name.as_str()) {
            accounting.included.insert(entry.id.clone());
        }
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut projected_ugens = BTreeSet::new();
    for entry in WalkDir::new(root.join("vscode-extension/ugen_manifests")) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("json")
        {
            continue;
        }
        let value: Value = serde_json::from_str(
            &fs::read_to_string(entry.path()).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        for function in value
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|ugen| ugen["functions"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
        {
            projected_ugens.insert(function.to_string());
        }
    }
    let callable_ugens = discovery
        .v1
        .entries
        .iter()
        .filter(|entry| {
            entry.surface == "dsp_ugen"
                && matches!(
                    &entry.details,
                    vibelang_api_manifest::EntryDetails::Ugen { callable: true, .. }
                )
        })
        .map(|entry| (entry.registered_name.as_str(), entry.id.as_str()))
        .collect::<BTreeMap<_, _>>();
    for (name, id) in &callable_ugens {
        if projected_ugens.contains(*name) {
            accounting.included.insert((*id).into());
        } else {
            accounting.exclusions.insert(
                (*id).into(),
                (
                    ExclusionReason::IntentionalCuration,
                    "vibelang-tools".into(),
                ),
            );
        }
    }
    let known_stale: Value = serde_json::from_str(&read(
        root,
        "tests/fixtures/api-unification/v1/negative/invalid-ugen-labels.json",
    )?)
    .map_err(|error| error.to_string())?;
    let known_stale = known_stale["labels"]
        .as_array()
        .ok_or_else(|| "invalid-UGen negative fixture has no labels".to_string())?
        .iter()
        .filter_map(|label| label["completion"].as_str())
        .collect::<BTreeSet<_>>();
    let non_callable_ugens = discovery
        .v1
        .entries
        .iter()
        .filter(|entry| {
            entry.surface == "dsp_ugen"
                && matches!(
                    &entry.details,
                    vibelang_api_manifest::EntryDetails::Ugen {
                        callable: false,
                        ..
                    }
                )
        })
        .map(|entry| entry.registered_name.as_str())
        .collect::<BTreeSet<_>>();
    for label in projected_ugens
        .iter()
        .filter(|label| !callable_ugens.contains_key(label.as_str()))
    {
        if known_stale.contains(label.as_str()) {
            accounting.stale.insert(label.clone());
            continue;
        }
        let stem = ["_ar", "_kr", "_ir", "_tr", "_channel"]
            .iter()
            .find_map(|suffix| label.strip_suffix(suffix))
            .unwrap_or(label);
        if non_callable_ugens.contains(stem) {
            accounting.exclusions.insert(
                semantic_id("type", &format!("editor|unsupported-ugen-label|{label}")),
                (ExclusionReason::UnsupportedHost, "vibelang-tools".into()),
            );
        } else {
            return Err(format!(
                "projected UGen label {label} is neither callable, explicitly unavailable, nor known-stale"
            ));
        }
    }
    accounting.unresolved.extend(
        discovery
            .v1
            .entries
            .iter()
            .filter(|entry| matches!(entry.kind.as_str(), "property_get" | "property_set"))
            .map(|entry| entry.id.clone()),
    );
    Ok(accounting)
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SemanticRequirement {
    target_id: String,
    facet: SemanticFacet,
    owner: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SemanticFragmentRecord {
    key: String,
    target_id: String,
    owner: String,
    facets: BTreeSet<SemanticFacet>,
    references: BTreeSet<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SemanticJoinAccounting {
    semantic_records: u64,
    orphan_records: u64,
    unclassified_records: u64,
    unclassified_targets: BTreeSet<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SemanticComposition {
    accounting: SemanticJoinAccounting,
}

fn validate_fragment_join(
    discovery: &Discovery,
    manifest: &PublicApiManifestV2,
    fragments: &FragmentSet,
) -> Result<SemanticComposition, String> {
    let requirements = derive_semantic_requirements(discovery)?;
    let records = semantic_fragment_records(fragments);
    let ids = contract_ids(manifest);
    let accounting = semantic_join_accounting(&ids, &requirements, &records);

    let mut required: BTreeMap<String, BTreeSet<SemanticFacet>> =
        ids.iter().map(|id| (id.clone(), BTreeSet::new())).collect();
    for requirement in &requirements {
        required
            .get_mut(&requirement.target_id)
            .ok_or_else(|| {
                format!(
                    "derived semantic requirement target {} is not a discovered contract node",
                    requirement.target_id
                )
            })?
            .insert(requirement.facet);
    }
    let discovered = required
        .into_iter()
        .map(|(id, required_facets)| DiscoveredSemanticNode {
            id,
            required_facets,
        })
        .collect::<Vec<_>>();
    fragments
        .validate(&discovered)
        .map_err(|error| error.to_string())?;

    let claims = records
        .iter()
        .flat_map(|record| {
            record
                .facets
                .iter()
                .map(|facet| ((record.target_id.clone(), *facet), record.owner.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    for requirement in &requirements {
        let owner = claims
            .get(&(requirement.target_id.clone(), requirement.facet))
            .ok_or_else(|| {
                format!(
                    "required semantic facet {:?} is missing for {}",
                    requirement.facet, requirement.target_id
                )
            })?;
        if owner != &requirement.owner {
            return Err(format!(
                "semantic facet {:?} for {} must be owned by {:?}, found {:?}",
                requirement.facet, requirement.target_id, requirement.owner, owner
            ));
        }
    }
    validate_frozen_semantic_refinements(&requirements, fragments)?;
    if accounting.orphan_records != 0 || accounting.unclassified_records != 0 {
        return Err(format!(
            "semantic join found {} orphan record(s) and {} unclassified discovered node(s)",
            accounting.orphan_records, accounting.unclassified_records
        ));
    }
    Ok(SemanticComposition { accounting })
}

fn derive_semantic_requirements(discovery: &Discovery) -> Result<Vec<SemanticRequirement>, String> {
    let authoring = unique_semantic_target(
        "current stdlib authoring stability anchor",
        discovery
            .v1
            .entries
            .iter()
            .filter(|entry| {
                entry.surface == "stdlib"
                    && entry.kind == "synthdef"
                    && entry.registered_name == "dmx_kick"
            })
            .map(|entry| (entry.id.clone(), entry_owner(entry).to_string()))
            .collect(),
    )?;
    let runtime = unique_semantic_target(
        "current queued transport mutation",
        discovery
            .http
            .routes
            .iter()
            .filter(|route| route.method == "PATCH" && route.path == "/transport")
            .map(|route| {
                (
                    semantic_id(
                        "operation",
                        &format!("http|{}|{}", route.method, route.path),
                    ),
                    "vibelang-core".into(),
                )
            })
            .collect(),
    )?;
    let http = unique_semantic_target(
        "current ignored HTTP quantization field",
        discovery
            .http
            .types
            .iter()
            .filter(|api_type| {
                api_type.name == "TransportUpdate"
                    && api_type.source == "crates/vibelang-http/src/models.rs"
            })
            .flat_map(|api_type| {
                api_type
                    .fields
                    .iter()
                    .filter(|field| field.name == "quantization_beats")
                    .map(|field| {
                        (
                            semantic_id(
                                "field",
                                &format!(
                                    "http|{}|{}|{}",
                                    api_type.source, api_type.name, field.name
                                ),
                            ),
                            "vibelang-http".into(),
                        )
                    })
            })
            .collect(),
    )?;
    let websocket = unique_semantic_target(
        "current WebSocket hello event",
        discovery
            .mechanical
            .iter()
            .filter(|declaration| {
                declaration.surface == "websocket"
                    && declaration.kind == "event"
                    && declaration.name == "hello"
            })
            .map(|declaration| (declaration.id.clone(), declaration.owner.clone()))
            .collect(),
    )?;
    unique_semantic_target(
        "current WASM globalThis.vibelangBridge declaration",
        discovery
            .mechanical
            .iter()
            .filter(|declaration| {
                declaration.surface == "wasm"
                    && declaration.kind == "host_bridge"
                    && declaration.name == "globalThis.vibelangBridge"
            })
            .map(|declaration| (declaration.id.clone(), declaration.owner.clone()))
            .collect(),
    )?;
    unique_semantic_target(
        "current canonical WASM package manifest",
        discovery
            .mechanical
            .iter()
            .filter(|declaration| {
                declaration.surface == "packages"
                    && declaration.kind == "manifest"
                    && declaration.name == "crates/vibelang-wasm/package.json"
            })
            .map(|declaration| (declaration.id.clone(), declaration.owner.clone()))
            .collect(),
    )?;
    if !discovery.baseline.categories.contains_key("wasm")
        || !discovery.baseline.categories.contains_key("rhai_editor")
    {
        return Err("M00 baseline is missing the WASM or editor semantic consumer".into());
    }
    let wasm = unique_semantic_target(
        "current WASM projection consumer",
        vec![(
            semantic_id("consumer", "baseline|wasm"),
            "vibelang-wasm".into(),
        )],
    )?;
    let editor = unique_semantic_target(
        "current editor projection consumer",
        vec![(
            semantic_id("consumer", "baseline|rhai_editor"),
            "vibelang-tools".into(),
        )],
    )?;

    let mut requirements = vec![
        SemanticRequirement {
            target_id: authoring.0,
            facet: SemanticFacet::Stability,
            owner: authoring.1,
        },
        SemanticRequirement {
            target_id: runtime.0,
            facet: SemanticFacet::Operation,
            owner: runtime.1,
        },
        SemanticRequirement {
            target_id: http.0.clone(),
            facet: SemanticFacet::OperationBinding,
            owner: http.1.clone(),
        },
        SemanticRequirement {
            target_id: http.0,
            facet: SemanticFacet::Effectiveness,
            owner: http.1,
        },
        SemanticRequirement {
            target_id: websocket.0,
            facet: SemanticFacet::Event,
            owner: websocket.1,
        },
        SemanticRequirement {
            target_id: wasm.0,
            facet: SemanticFacet::WasmHost,
            owner: wasm.1,
        },
        SemanticRequirement {
            target_id: editor.0.clone(),
            facet: SemanticFacet::ConsumerPolicy,
            owner: editor.1.clone(),
        },
        SemanticRequirement {
            target_id: editor.0,
            facet: SemanticFacet::Coverage,
            owner: editor.1,
        },
    ];
    requirements
        .sort_by(|left, right| (&left.target_id, left.facet).cmp(&(&right.target_id, right.facet)));
    Ok(requirements)
}

fn unique_semantic_target(
    label: &str,
    mut candidates: Vec<(String, String)>,
) -> Result<(String, String), String> {
    candidates.sort();
    match candidates.as_slice() {
        [candidate] => Ok(candidate.clone()),
        _ => Err(format!(
            "{label} must discover exactly one mechanical node, found {}: {}",
            candidates.len(),
            candidates
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn semantic_fragment_records(fragments: &FragmentSet) -> Vec<SemanticFragmentRecord> {
    let mut records = Vec::new();
    for (index, record) in fragments.authoring.records.iter().enumerate() {
        let mut facets = BTreeSet::new();
        if !record.aliases.is_empty() {
            facets.insert(SemanticFacet::Aliases);
        }
        insert_optional_facet(&mut facets, &record.stability, SemanticFacet::Stability);
        insert_optional_facet(
            &mut facets,
            &record.availability,
            SemanticFacet::Availability,
        );
        insert_optional_facet(&mut facets, &record.lifecycle, SemanticFacet::Lifecycle);
        insert_optional_facet(
            &mut facets,
            &record.value_contract,
            SemanticFacet::ValueContract,
        );
        insert_optional_facet(&mut facets, &record.failure, SemanticFacet::Failure);
        if !record.operation_ids.is_empty() {
            facets.insert(SemanticFacet::OperationBinding);
        }
        records.push(fragment_record(
            "authoring",
            index,
            &record.target_id,
            &record.owner,
            facets,
            record.operation_ids.iter().cloned().collect(),
        ));
    }
    for (index, record) in fragments.runtime.records.iter().enumerate() {
        let mut facets = BTreeSet::new();
        insert_optional_facet(&mut facets, &record.operation, SemanticFacet::Operation);
        insert_optional_facet(&mut facets, &record.revision, SemanticFacet::Revision);
        insert_optional_facet(&mut facets, &record.receipt, SemanticFacet::Receipt);
        insert_optional_facet(&mut facets, &record.failure, SemanticFacet::Failure);
        if !record.effects.is_empty() {
            facets.insert(SemanticFacet::Effects);
        }
        records.push(fragment_record(
            "runtime",
            index,
            &record.target_id,
            &record.owner,
            facets,
            BTreeSet::new(),
        ));
    }
    for (index, record) in fragments.http.records.iter().enumerate() {
        let mut facets = BTreeSet::new();
        insert_optional_facet(
            &mut facets,
            &record.operation_id,
            SemanticFacet::OperationBinding,
        );
        insert_optional_facet(
            &mut facets,
            &record.effectiveness,
            SemanticFacet::Effectiveness,
        );
        insert_optional_facet(&mut facets, &record.consistency, SemanticFacet::Consistency);
        insert_optional_facet(&mut facets, &record.failure, SemanticFacet::Failure);
        if !record.security_capability_ids.is_empty() {
            facets.insert(SemanticFacet::Security);
        }
        let references = record
            .operation_id
            .iter()
            .chain(&record.security_capability_ids)
            .cloned()
            .collect();
        records.push(fragment_record(
            "http",
            index,
            &record.target_id,
            &record.owner,
            facets,
            references,
        ));
    }
    for (index, record) in fragments.websocket.records.iter().enumerate() {
        let mut facets = BTreeSet::new();
        insert_optional_facet(
            &mut facets,
            &record.operation_id,
            SemanticFacet::OperationBinding,
        );
        insert_optional_facet(&mut facets, &record.event, SemanticFacet::Event);
        insert_optional_facet(&mut facets, &record.failure, SemanticFacet::Failure);
        let references = record
            .operation_id
            .iter()
            .chain(
                record
                    .event
                    .iter()
                    .flat_map(|event| &event.resync_operation_id),
            )
            .cloned()
            .collect();
        records.push(fragment_record(
            "websocket",
            index,
            &record.target_id,
            &record.owner,
            facets,
            references,
        ));
    }
    for (index, record) in fragments.wasm.records.iter().enumerate() {
        let mut facets = BTreeSet::new();
        insert_optional_facet(
            &mut facets,
            &record.operation_id,
            SemanticFacet::OperationBinding,
        );
        insert_optional_facet(&mut facets, &record.host, SemanticFacet::WasmHost);
        insert_optional_facet(&mut facets, &record.stability, SemanticFacet::Stability);
        insert_optional_facet(&mut facets, &record.failure, SemanticFacet::Failure);
        let references = record
            .operation_id
            .iter()
            .chain(record.host.iter().flat_map(|host| &host.capability_ids))
            .cloned()
            .collect();
        records.push(fragment_record(
            "wasm",
            index,
            &record.target_id,
            &record.owner,
            facets,
            references,
        ));
    }
    for (index, record) in fragments.consumers.records.iter().enumerate() {
        let mut facets = BTreeSet::new();
        insert_optional_facet(&mut facets, &record.policy, SemanticFacet::ConsumerPolicy);
        insert_optional_facet(&mut facets, &record.coverage, SemanticFacet::Coverage);
        if !record.exclusions.is_empty() {
            facets.insert(SemanticFacet::Exclusions);
        }
        let references = record
            .policy
            .iter()
            .flat_map(|policy| &policy.capability_ids)
            .chain(
                record
                    .exclusions
                    .iter()
                    .map(|exclusion| &exclusion.target_id),
            )
            .cloned()
            .collect();
        records.push(fragment_record(
            "consumers",
            index,
            &record.target_id,
            &record.owner,
            facets,
            references,
        ));
    }
    records.sort_by(|left, right| left.key.cmp(&right.key));
    records
}

fn insert_optional_facet<T>(
    facets: &mut BTreeSet<SemanticFacet>,
    value: &Option<T>,
    facet: SemanticFacet,
) {
    if value.is_some() {
        facets.insert(facet);
    }
}

fn fragment_record(
    domain: &str,
    index: usize,
    target_id: &str,
    owner: &str,
    facets: BTreeSet<SemanticFacet>,
    references: BTreeSet<String>,
) -> SemanticFragmentRecord {
    SemanticFragmentRecord {
        key: format!("{domain}:{index:08}:{target_id}"),
        target_id: target_id.into(),
        owner: owner.into(),
        facets,
        references,
    }
}

fn semantic_join_accounting(
    ids: &BTreeSet<String>,
    requirements: &[SemanticRequirement],
    records: &[SemanticFragmentRecord],
) -> SemanticJoinAccounting {
    let orphan_records = records
        .iter()
        .filter(|record| {
            !ids.contains(&record.target_id) || record.references.iter().any(|id| !ids.contains(id))
        })
        .count() as u64;
    let claims = records
        .iter()
        .flat_map(|record| {
            record
                .facets
                .iter()
                .map(|facet| ((record.target_id.as_str(), *facet), record.owner.as_str()))
        })
        .collect::<BTreeMap<_, _>>();
    let unclassified_targets = requirements
        .iter()
        .filter(|requirement| {
            claims
                .get(&(requirement.target_id.as_str(), requirement.facet))
                .is_none_or(|owner| *owner != requirement.owner)
        })
        .map(|requirement| requirement.target_id.clone())
        .collect::<BTreeSet<_>>();
    SemanticJoinAccounting {
        semantic_records: records.len() as u64,
        orphan_records,
        unclassified_records: unclassified_targets.len() as u64,
        unclassified_targets,
    }
}

fn required_semantic_target(
    requirements: &[SemanticRequirement],
    facet: SemanticFacet,
) -> Result<&str, String> {
    let targets = requirements
        .iter()
        .filter(|requirement| requirement.facet == facet)
        .map(|requirement| requirement.target_id.as_str())
        .collect::<BTreeSet<_>>();
    match targets.iter().copied().collect::<Vec<_>>().as_slice() {
        [target] => Ok(target),
        _ => Err(format!(
            "frozen M02 semantic facet {facet:?} must resolve to exactly one target"
        )),
    }
}

fn validate_frozen_semantic_refinements(
    requirements: &[SemanticRequirement],
    fragments: &FragmentSet,
) -> Result<(), String> {
    let runtime_target = required_semantic_target(requirements, SemanticFacet::Operation)?;
    let runtime = fragments
        .runtime
        .records
        .iter()
        .find(|record| record.target_id == runtime_target)
        .and_then(|record| record.operation.as_ref())
        .ok_or_else(|| format!("runtime semantics disappeared for {runtime_target}"))?;
    if runtime.effect_timing != EffectTiming::RuntimeQueued
        || runtime.atomicity != Atomicity::BestEffort
    {
        return Err(format!(
            "runtime semantics for {runtime_target} must preserve effect_timing=runtime_queued and atomicity=best_effort"
        ));
    }

    let wasm_target = required_semantic_target(requirements, SemanticFacet::WasmHost)?;
    let wasm = fragments
        .wasm
        .records
        .iter()
        .find(|record| record.target_id == wasm_target)
        .and_then(|record| record.host.as_ref())
        .ok_or_else(|| format!("WASM host semantics disappeared for {wasm_target}"))?;
    if !wasm.capability_ids.is_empty()
        || wasm.required_globals != ["globalThis.vibelangBridge"]
        || wasm.progress != WasmProgress::HostTick
        || wasm.canonical_package_owner != "crates/vibelang-wasm"
    {
        return Err(format!(
            "WASM semantics for {wasm_target} must preserve globalThis.vibelangBridge, host_tick, and crates/vibelang-wasm ownership"
        ));
    }

    let policy_target = required_semantic_target(requirements, SemanticFacet::ConsumerPolicy)?;
    let consumer = fragments
        .consumers
        .records
        .iter()
        .find(|record| record.target_id == policy_target)
        .ok_or_else(|| format!("consumer semantics disappeared for {policy_target}"))?;
    let policy = consumer
        .policy
        .as_ref()
        .ok_or_else(|| format!("consumer policy disappeared for {policy_target}"))?;
    if policy.surfaces != ["rhai_editor"]
        || policy.kinds != ["v1_compatibility_projection"]
        || !policy.capability_ids.is_empty()
        || policy.include_preview
    {
        return Err(format!(
            "consumer policy for {policy_target} no longer matches the frozen M02 editor projection"
        ));
    }
    let coverage = consumer
        .coverage
        .as_ref()
        .ok_or_else(|| format!("coverage policy disappeared for {policy_target}"))?;
    if !coverage.require_complete_eligibility
        || !coverage.allow_curated_exclusions
        || !coverage.forbid_denominator_shrink
    {
        return Err(format!(
            "coverage policy for {policy_target} must require complete eligibility, owned curation, and denominator preservation"
        ));
    }
    Ok(())
}

fn apply_fragments(
    manifest: &mut PublicApiManifestV2,
    fragments: &FragmentSet,
) -> Result<(), String> {
    for record in &fragments.authoring.records {
        let metadata = find_metadata_mut(manifest, &record.target_id)
            .ok_or_else(|| format!("authoring target {} disappeared", record.target_id))?;
        metadata.ownership.implementation_owner = record.owner.clone();
        if let Some(stability) = &record.stability {
            metadata.stability = stability.clone();
        }
        if let Some(availability) = &record.availability {
            metadata.availability = availability.clone();
        }
    }
    for record in &fragments.runtime.records {
        let operation = manifest
            .operations
            .iter_mut()
            .find(|operation| operation.metadata.id == record.target_id)
            .ok_or_else(|| format!("runtime target {} is not an operation", record.target_id))?;
        operation.metadata.ownership.implementation_owner = record.owner.clone();
        if let Some(semantics) = &record.operation {
            operation.kind = semantics.kind;
            operation.idempotency = semantics.idempotency;
            operation.consistency = semantics.consistency;
            operation.effect_timing = Facet::Applicable {
                value: semantics.effect_timing,
            };
            operation.atomicity = Facet::Applicable {
                value: semantics.atomicity,
            };
        }
    }
    for record in &fragments.http.records {
        let field = manifest
            .types
            .iter_mut()
            .flat_map(|api_type| api_type.fields.iter_mut())
            .find(|field| field.metadata.id == record.target_id)
            .ok_or_else(|| format!("HTTP target {} is not a field", record.target_id))?;
        field.metadata.ownership.implementation_owner = record.owner.clone();
        if let (Some(operation_id), Some(effectiveness)) =
            (&record.operation_id, &record.effectiveness)
        {
            field.bindings.push(FieldBinding {
                operation_id: operation_id.clone(),
                effectiveness: effectiveness.clone(),
            });
        } else if record.operation_id.is_some() || record.effectiveness.is_some() {
            return Err(format!(
                "HTTP target {} must join operation and effectiveness together",
                record.target_id
            ));
        }
    }
    for record in &fragments.websocket.records {
        let event = manifest
            .events
            .iter_mut()
            .find(|event| event.metadata.id == record.target_id)
            .ok_or_else(|| format!("WebSocket target {} is not an event", record.target_id))?;
        event.metadata.ownership.implementation_owner = record.owner.clone();
        if let Some(semantics) = &record.event {
            event.ordering = semantics.ordering;
            event.revision_relation = semantics.revision_relation;
            event.delivery = semantics.delivery;
            event.loss_detection = semantics.loss_detection;
            event.resync_operation_id = semantics.resync_operation_id.clone();
        }
    }
    for record in &fragments.wasm.records {
        let consumer = manifest
            .consumers
            .iter_mut()
            .find(|consumer| consumer.metadata.id == record.target_id)
            .ok_or_else(|| format!("WASM target {} is not a consumer", record.target_id))?;
        consumer.metadata.ownership.implementation_owner = record.owner.clone();
        if let Some(host) = &record.host {
            consumer.host = Facet::Applicable {
                value: host.clone(),
            };
        }
    }
    for record in &fragments.consumers.records {
        let consumer = manifest
            .consumers
            .iter_mut()
            .find(|consumer| consumer.metadata.id == record.target_id)
            .ok_or_else(|| format!("consumer target {} disappeared", record.target_id))?;
        consumer.metadata.ownership.implementation_owner = record.owner.clone();
        if let Some(policy) = &record.policy {
            consumer.eligibility.surfaces = policy.surfaces.clone();
            consumer.eligibility.kinds = policy.kinds.clone();
            consumer.eligibility.capability_ids = policy.capability_ids.clone();
            if policy.include_preview {
                consumer
                    .eligibility
                    .stability_levels
                    .insert(StabilityLevel::Preview);
            }
        }
        if let Some(coverage) = &record.coverage {
            consumer.coverage_policy = Facet::Applicable {
                value: coverage.clone(),
            };
        }
    }
    Ok(())
}

fn find_metadata_mut<'a>(
    manifest: &'a mut PublicApiManifestV2,
    id: &str,
) -> Option<&'a mut NodeMetadata> {
    for entry in &mut manifest.entries {
        if entry.metadata.id == id {
            return Some(&mut entry.metadata);
        }
        for overload in &mut entry.overloads {
            if overload.metadata.id == id {
                return Some(&mut overload.metadata);
            }
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverageArtifact {
    schema: String,
    schema_version: u32,
    contract_digest: String,
    totals: CoverageTotals,
    sources: Vec<SourceCoverage>,
    consumers: Vec<ConsumerCoverage>,
    semantic_fragments: Vec<FragmentCoverage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverageTotals {
    entries: u64,
    overloads: u64,
    routes: u64,
    http_types: u64,
    http_fields: u64,
    source_nodes: u64,
    consumer_nodes: u64,
    orphan_records: u64,
    unclassified_records: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceCoverage {
    id: String,
    path: String,
    kind: String,
    node_count: u64,
    node_ids: Vec<String>,
    node_ids_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsumerCoverage {
    id: String,
    projection_paths: Vec<String>,
    included_count: u64,
    eligible_count: u64,
    excluded_count: u64,
    unresolved_count: u64,
    unclassified_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FragmentCoverage {
    path: String,
    domain: String,
    record_count: u64,
    sha256: String,
}

fn build_coverage(
    root: &Path,
    discovery: &Discovery,
    manifest: &PublicApiManifestV2,
    digest: &str,
    accounting: &SemanticJoinAccounting,
) -> Result<CoverageArtifact, String> {
    let mut source_nodes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    visit_metadata(manifest, |metadata| {
        for anchor in &metadata.source_anchors {
            source_nodes
                .entry(anchor.path.clone())
                .or_default()
                .insert(metadata.id.clone());
        }
    });
    for declaration in &discovery.mechanical {
        for (path, _) in &declaration.source_anchors {
            source_nodes
                .entry(path.clone())
                .or_default()
                .insert(declaration.id.clone());
        }
    }
    for (domain, path, targets) in fragment_targets(&discovery.fragments) {
        source_nodes.entry(path.into()).or_default().extend(targets);
        if domain.is_empty() {
            return Err("semantic fragment domain cannot be empty".into());
        }
    }
    let mut sources = Vec::new();
    for (path, ids) in source_nodes {
        if !root.join(&path).is_file() {
            return Err(format!("discovered source path does not exist: {path}"));
        }
        let node_ids = ids.into_iter().collect::<Vec<_>>();
        let joined = node_ids.join("\0");
        sources.push(SourceCoverage {
            id: semantic_id("source", &path),
            kind: if path.starts_with(FRAGMENT_DIR) {
                "semantic_fragment".into()
            } else {
                "mechanical_declaration".into()
            },
            path,
            node_count: node_ids.len() as u64,
            node_ids,
            node_ids_sha256: sha256_hex(joined.as_bytes()),
        });
    }
    sources.sort_by(|left, right| left.id.cmp(&right.id));

    let consumers = manifest
        .consumers
        .iter()
        .map(|consumer| {
            let coverage = &manifest.coverage[&consumer.metadata.id];
            ConsumerCoverage {
                id: consumer.metadata.id.clone(),
                projection_paths: consumer.source_projections.clone(),
                included_count: coverage.numerator,
                eligible_count: coverage.denominator,
                excluded_count: coverage.exclusions_by_reason.values().sum(),
                unresolved_count: coverage.unresolved_ids.len() as u64,
                unclassified_count: u64::from(
                    accounting
                        .unclassified_targets
                        .contains(&consumer.metadata.id),
                ),
            }
        })
        .collect::<Vec<_>>();
    let semantic_fragments = fragment_targets(&discovery.fragments)
        .into_iter()
        .map(|(domain, path, targets)| {
            let bytes = fs::read(root.join(path)).map_err(|error| error.to_string())?;
            Ok(FragmentCoverage {
                path: path.into(),
                domain: domain.into(),
                record_count: targets.len() as u64,
                sha256: sha256_hex(&bytes),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(CoverageArtifact {
        schema: "https://vibelang.org/schemas/public-api-coverage/v2".into(),
        schema_version: 2,
        contract_digest: digest.into(),
        totals: CoverageTotals {
            entries: manifest.entries.len() as u64,
            overloads: manifest
                .entries
                .iter()
                .map(|entry| entry.overloads.len() as u64)
                .sum(),
            routes: discovery.http.routes.len() as u64,
            http_types: discovery.http.types.len() as u64,
            http_fields: discovery
                .http
                .types
                .iter()
                .map(|api_type| api_type.fields.len() as u64)
                .sum(),
            source_nodes: sources.len() as u64,
            consumer_nodes: consumers.len() as u64,
            orphan_records: accounting.orphan_records,
            unclassified_records: accounting.unclassified_records,
        },
        sources,
        consumers,
        semantic_fragments,
    })
}

fn fragment_targets(fragments: &FragmentSet) -> Vec<(&'static str, &'static str, Vec<String>)> {
    vec![
        (
            "authoring",
            "api/contract/authoring.toml",
            fragments
                .authoring
                .records
                .iter()
                .map(|record| record.target_id.clone())
                .collect(),
        ),
        (
            "runtime",
            "api/contract/runtime.toml",
            fragments
                .runtime
                .records
                .iter()
                .map(|record| record.target_id.clone())
                .collect(),
        ),
        (
            "http",
            "api/contract/http.toml",
            fragments
                .http
                .records
                .iter()
                .map(|record| record.target_id.clone())
                .collect(),
        ),
        (
            "websocket",
            "api/contract/websocket.toml",
            fragments
                .websocket
                .records
                .iter()
                .map(|record| record.target_id.clone())
                .collect(),
        ),
        (
            "wasm",
            "api/contract/wasm.toml",
            fragments
                .wasm
                .records
                .iter()
                .map(|record| record.target_id.clone())
                .collect(),
        ),
        (
            "consumers",
            "api/contract/consumers.toml",
            fragments
                .consumers
                .records
                .iter()
                .map(|record| record.target_id.clone())
                .collect(),
        ),
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DebtArtifact {
    schema: String,
    schema_version: u32,
    contract_digest: String,
    records: Vec<DebtRecord>,
    counts: BTreeMap<String, u64>,
    orphan_count: u64,
    unclassified_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DebtRecord {
    id: String,
    surface: String,
    node_id: Option<String>,
    operation_id: Option<String>,
    member: String,
    legacy_class: String,
    owner: String,
    diagnostic_id: String,
    issue: String,
    exit_gate: String,
    remove_by: String,
    source_anchor: String,
    test_anchor: String,
}

impl DebtArtifact {
    fn validate(&self, ids: &BTreeSet<String>) -> Result<(), String> {
        if self.orphan_count != 0 || self.unclassified_count != 0 {
            return Err("compatibility debt reports orphan or unclassified records".into());
        }
        let allowed = ["ignored", "log_only", "stale", "dead"];
        let mut previous = None;
        for record in &self.records {
            validate_stable_id(&record.id, Some("debt")).map_err(|error| error.to_string())?;
            if previous.is_some_and(|value: &str| value >= record.id.as_str()) {
                return Err(format!(
                    "compatibility-debt record {} is duplicated or out of order",
                    record.id
                ));
            }
            previous = Some(record.id.as_str());
            if !allowed.contains(&record.legacy_class.as_str())
                || record.owner.trim().is_empty()
                || record.diagnostic_id.trim().is_empty()
                || record.issue.trim().is_empty()
                || record.exit_gate.trim().is_empty()
                || record.remove_by.trim().is_empty()
                || record.source_anchor.trim().is_empty()
                || record.test_anchor.trim().is_empty()
            {
                return Err(format!("incomplete compatibility debt {}", record.id));
            }
            if record
                .node_id
                .as_ref()
                .is_some_and(|node_id| !ids.contains(node_id))
                || record
                    .operation_id
                    .as_ref()
                    .is_some_and(|operation_id| !ids.contains(operation_id))
            {
                return Err(format!("orphan compatibility debt {}", record.id));
            }
        }
        Ok(())
    }
}

fn build_debt(
    root: &Path,
    manifest: &PublicApiManifestV2,
    digest: &str,
    accounting: &SemanticJoinAccounting,
) -> Result<DebtArtifact, String> {
    let mut records = Vec::new();
    for api_type in &manifest.types {
        if !api_type
            .metadata
            .source_anchors
            .iter()
            .any(|anchor| anchor.derivation == Derivation::RustAst)
            || !api_type
                .metadata
                .name
                .contains(|character: char| character.is_alphabetic())
        {
            continue;
        }
        for field in &api_type.fields {
            let dead = is_dead_http_field(&api_type.metadata.name, &field.host_name);
            records.push(DebtRecord {
                id: semantic_id("debt", &format!("http-field|{}", field.metadata.id)),
                surface: "http".into(),
                node_id: Some(field.metadata.id.clone()),
                operation_id: field
                    .bindings
                    .first()
                    .map(|binding| binding.operation_id.clone()),
                member: field.serialized_name.clone(),
                legacy_class: if dead { "dead" } else { "stale" }.into(),
                owner: "vibelang-http".into(),
                diagnostic_id: if dead {
                    "compat.http.dead_declaration"
                } else {
                    "compat.http.operation_binding_pending"
                }
                .into(),
                issue: "M11 HTTP v2 effectiveness binding".into(),
                exit_gate: "M11 must implement or structurally reject this operation-scoped member"
                    .into(),
                remove_by: "v2 release-ready gate".into(),
                source_anchor: field.metadata.source_anchors[0].path.clone(),
                test_anchor: "tests/fixtures/api-unification/v1/negative/ignored-fields.json"
                    .into(),
            });
        }
    }
    for operation in &manifest.operations {
        let BindingDetails::Http { method, path, .. } = &operation.bindings[0].details else {
            continue;
        };
        if method == "GET" {
            continue;
        }
        records.push(DebtRecord {
            id: semantic_id("debt", &format!("http-success|{method}|{path}")),
            surface: "http".into(),
            node_id: Some(operation.metadata.id.clone()),
            operation_id: Some(operation.metadata.id.clone()),
            member: "success_response".into(),
            legacy_class: "stale".into(),
            owner: "vibelang-http".into(),
            diagnostic_id: "compat.http.stale_success".into(),
            issue: "M04 honest v1 receipts and M11 HTTP v2".into(),
            exit_gate: "queue/evaluation acceptance must not claim applied state".into(),
            remove_by: "v2 release-ready gate".into(),
            source_anchor: "crates/vibelang-http/src/lib.rs".into(),
            test_anchor: "tests/fixtures/api-unification/v1/negative/stale-success.json".into(),
        });
    }
    for event in &manifest.events {
        records.push(DebtRecord {
            id: semantic_id("debt", &format!("websocket-event|{}", event.metadata.id)),
            surface: "websocket".into(),
            node_id: Some(event.metadata.id.clone()),
            operation_id: None,
            member: event.metadata.name.clone(),
            legacy_class: "stale".into(),
            owner: "vibelang-http".into(),
            diagnostic_id: "compat.websocket.unrevisioned_poll".into(),
            issue: "M11 typed revisioned WebSocket projection".into(),
            exit_gate: "typed payload, ordering, loss detection, and resync must land".into(),
            remove_by: "v2 release-ready gate".into(),
            source_anchor: "crates/vibelang-http/src/websocket.rs".into(),
            test_anchor: "tests/fixtures/api-unification/v1/negative/stale-success.json".into(),
        });
    }
    add_fixture_cases(
        root,
        &mut records,
        "tests/fixtures/api-unification/v1/negative/ignored-fields.json",
        "cases",
        "mixed",
        "ignored",
        "compat.v1.ignored_input",
        "M08-M12 domain effectiveness migrations",
    )?;
    add_fixture_cases(
        root,
        &mut records,
        "tests/fixtures/api-unification/v1/negative/wasm-bridge-false-success.json",
        "cases",
        "wasm",
        "log_only",
        "compat.wasm.false_success",
        "M12 WASM v2 runtime contract",
    )?;
    add_fixture_cases(
        root,
        &mut records,
        "tests/fixtures/api-unification/v1/negative/stale-commands.json",
        "commands",
        "consumers",
        "stale",
        "compat.consumer.stale_command",
        "M13 consumer migration",
    )?;

    let invalid_labels: Value = serde_json::from_str(&read(
        root,
        "tests/fixtures/api-unification/v1/negative/invalid-ugen-labels.json",
    )?)
    .map_err(|error| error.to_string())?;
    for label in invalid_labels["labels"]
        .as_array()
        .ok_or_else(|| "invalid UGen label fixture has no labels".to_string())?
    {
        let completion = label["completion"]
            .as_str()
            .ok_or_else(|| "invalid UGen label lacks completion".to_string())?;
        let runtime = label["runtime"]
            .as_str()
            .ok_or_else(|| "invalid UGen label lacks runtime".to_string())?;
        let node_id = manifest
            .entries
            .iter()
            .find(|entry| entry.registered_name == runtime)
            .map(|entry| entry.metadata.id.clone());
        records.push(DebtRecord {
            id: semantic_id("debt", &format!("ugen-label|{completion}")),
            surface: "editor".into(),
            node_id,
            operation_id: None,
            member: completion.into(),
            legacy_class: "stale".into(),
            owner: "vibelang-editors".into(),
            diagnostic_id: "compat.editor.invalid_ugen_label".into(),
            issue: "M13 exact editor projection".into(),
            exit_gate: format!("completion {completion} must resolve to {runtime}"),
            remove_by: "v2 release-ready gate".into(),
            source_anchor: "vscode-extension/ugen_manifests".into(),
            test_anchor: "tests/fixtures/api-unification/v1/negative/invalid-ugen-labels.json"
                .into(),
        });
    }
    for (name, fixture, diagnostic) in [
        (
            "push_pull_diagnostic_mismatch",
            "tests/fixtures/api-unification/v1/negative/push-pull-diagnostic-mismatch.json",
            "compat.lsp.diagnostic_rule_mismatch",
        ),
        (
            "semantic_token_mismatch",
            "tests/fixtures/api-unification/v1/negative/semantic-token-mismatch.json",
            "compat.lsp.semantic_legend_mismatch",
        ),
    ] {
        records.push(DebtRecord {
            id: semantic_id("debt", &format!("lsp|{name}")),
            surface: "lsp".into(),
            node_id: None,
            operation_id: None,
            member: name.into(),
            legacy_class: "stale".into(),
            owner: "vibelang-lsp".into(),
            diagnostic_id: diagnostic.into(),
            issue: "M13 LSP projection convergence".into(),
            exit_gate: "one generated rule/legend source must feed both producer and consumer"
                .into(),
            remove_by: "v2 release-ready gate".into(),
            source_anchor: "crates/vibelang-lsp/src".into(),
            test_anchor: fixture.into(),
        });
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let mut counts = BTreeMap::new();
    for record in &records {
        *counts.entry(record.legacy_class.clone()).or_insert(0) += 1;
    }
    Ok(DebtArtifact {
        schema: "https://vibelang.org/schemas/public-api-compatibility-debt/v1".into(),
        schema_version: 1,
        contract_digest: digest.into(),
        records,
        counts,
        orphan_count: accounting.orphan_records,
        unclassified_count: accounting.unclassified_records,
    })
}

#[allow(clippy::too_many_arguments)]
fn add_fixture_cases(
    root: &Path,
    records: &mut Vec<DebtRecord>,
    fixture: &str,
    array: &str,
    surface: &str,
    legacy_class: &str,
    diagnostic: &str,
    issue: &str,
) -> Result<(), String> {
    let value: Value =
        serde_json::from_str(&read(root, fixture)?).map_err(|error| error.to_string())?;
    for (index, item) in value[array]
        .as_array()
        .ok_or_else(|| format!("{fixture} has no {array} array"))?
        .iter()
        .enumerate()
    {
        let member = item.as_str().map(str::to_string).unwrap_or_else(|| {
            let path = item
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("fixture");
            let stale = item
                .get("stale")
                .and_then(Value::as_str)
                .unwrap_or("structured fixture case");
            format!("{path}: {stale}")
        });
        records.push(DebtRecord {
            id: semantic_id("debt", &format!("{fixture}|{index}|{member}")),
            surface: surface.into(),
            node_id: None,
            operation_id: None,
            member,
            legacy_class: legacy_class.into(),
            owner: "api-unification".into(),
            diagnostic_id: diagnostic.into(),
            issue: issue.into(),
            exit_gate: "replace legacy behavior with an effective binding or structured rejection"
                .into(),
            remove_by: "v2 release-ready gate".into(),
            source_anchor: fixture.into(),
            test_anchor: fixture.into(),
        });
    }
    Ok(())
}

fn is_dead_http_field(type_name: &str, field_name: &str) -> bool {
    matches!(
        (type_name, field_name),
        ("SourceLocation", "file" | "line" | "column")
            | ("GroupCreate", "name" | "parent_path" | "params")
            | (
                "EffectCreate",
                "id" | "synthdef_name" | "group_path" | "params" | "position"
            )
            | ("ClockOutputRequest", "device_id" | "enabled")
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiffArtifact {
    schema: String,
    schema_version: u32,
    base: ArtifactIdentity,
    candidate_v1_projection: ArtifactIdentity,
    candidate_v2_digest: String,
    unchanged_entry_ids: u64,
    unchanged_overload_ids: u64,
    unclassified_count: u64,
    report: CompatibilityReport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactIdentity {
    path: String,
    sha256: String,
    bytes: u64,
}

fn build_diff(discovery: &Discovery, digest: &str) -> Result<DiffArtifact, String> {
    let identity = ArtifactIdentity {
        path: V1_MANIFEST_PATH.into(),
        sha256: sha256_hex(discovery.v1_json.as_bytes()),
        bytes: discovery.v1_json.len() as u64,
    };
    let report = CompatibilityReport {
        changes: Vec::<CompatibilityChange>::new(),
    };
    let unclassified_count = report
        .changes
        .iter()
        .filter(|change| {
            change.classes.is_empty() || change.classes.contains(&CompatibilityClass::Unclassified)
        })
        .count() as u64;
    Ok(DiffArtifact {
        schema: "https://vibelang.org/schemas/public-api-compatibility-diff/v1".into(),
        schema_version: 1,
        base: identity.clone(),
        candidate_v1_projection: identity,
        candidate_v2_digest: digest.into(),
        unchanged_entry_ids: EXPECTED_ENTRIES as u64,
        unchanged_overload_ids: EXPECTED_OVERLOADS as u64,
        unclassified_count,
        report,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageIndex {
    schema: String,
    schema_version: u32,
    contract_digest: String,
    files: Vec<PackageFile>,
    wasm_package_owners: Vec<WasmPackageOwner>,
    vscode_required_members: Vec<String>,
    wasm_required_members: Vec<String>,
    orphan_count: u64,
    unclassified_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageFile {
    path: String,
    kind: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WasmPackageOwner {
    path: String,
    name: String,
    version: String,
    canonical: bool,
    compatibility_debt: bool,
}

fn build_package_index(
    root: &Path,
    discovery: &Discovery,
    manifest: &PublicApiManifestV2,
    digest: &str,
    accounting: &SemanticJoinAccounting,
) -> Result<PackageIndex, String> {
    let packages = discovery
        .baseline
        .categories
        .get("packages")
        .ok_or_else(|| "M00 baseline has no package category".to_string())?;
    let mut files = Vec::new();
    let mut wasm_package_owners = Vec::new();
    let wasm_hosts = manifest
        .consumers
        .iter()
        .filter_map(|consumer| match &consumer.host {
            Facet::Applicable { value } => Some(value),
            Facet::NotApplicable { .. } => None,
        })
        .collect::<Vec<_>>();
    let [wasm_host] = wasm_hosts.as_slice() else {
        return Err(format!(
            "composed contract must have exactly one WASM host/package owner, found {}",
            wasm_hosts.len()
        ));
    };
    let canonical_wasm_manifest = format!(
        "{}/package.json",
        wasm_host.canonical_package_owner.trim_end_matches('/')
    );
    for baseline in &packages.files {
        let bytes = fs::read(root.join(&baseline.path))
            .map_err(|error| format!("{}: {error}", baseline.path))?;
        let kind = if baseline.path.ends_with("Cargo.toml") {
            "cargo_manifest"
        } else if baseline.path.ends_with("package.json") {
            "node_manifest"
        } else {
            "lockfile"
        };
        files.push(PackageFile {
            path: baseline.path.clone(),
            kind: kind.into(),
            bytes: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
        });
        if baseline.path.ends_with("package.json") {
            let package: Value =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            if package["name"].as_str() == Some("vibelang-wasm") {
                let canonical = baseline.path == canonical_wasm_manifest;
                wasm_package_owners.push(WasmPackageOwner {
                    path: baseline.path.clone(),
                    name: "vibelang-wasm".into(),
                    version: package["version"].as_str().unwrap_or("unknown").into(),
                    canonical,
                    compatibility_debt: !canonical,
                });
            }
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    wasm_package_owners.sort_by(|left, right| left.path.cmp(&right.path));
    if wasm_package_owners
        .iter()
        .filter(|owner| owner.canonical)
        .count()
        != 1
    {
        return Err(format!(
            "composed canonical WASM package {} did not resolve exactly once",
            canonical_wasm_manifest
        ));
    }
    let vscode: Value = serde_json::from_str(&read(root, "vscode-extension/package.json")?)
        .map_err(|error| error.to_string())?;
    let main = vscode["main"]
        .as_str()
        .ok_or_else(|| "VS Code package has no main".to_string())?;
    Ok(PackageIndex {
        schema: "https://vibelang.org/schemas/public-api-package-index/v1".into(),
        schema_version: 1,
        contract_digest: digest.into(),
        files,
        wasm_package_owners,
        vscode_required_members: vec![
            "vscode-extension/package.json".into(),
            format!("vscode-extension/{}", main.trim_start_matches("./")),
            "vscode-extension/src/data/rhai-api.json".into(),
            "vscode-extension/src/data/stdlib.json".into(),
            "vscode-extension/ugen_manifests".into(),
        ],
        wasm_required_members: vec![
            "crates/vibelang-wasm/package.json".into(),
            "crates/vibelang-wasm/types/index.d.ts".into(),
            "crates/vibelang-wasm/pkg/*.js".into(),
            "crates/vibelang-wasm/pkg/*.wasm".into(),
        ],
        orphan_count: accounting.orphan_records,
        unclassified_count: accounting.unclassified_records,
    })
}

fn validate_v1_inventory(v1: &PublicApiManifest, http: &HttpSnapshot) -> Result<(), String> {
    let overloads = v1
        .entries
        .iter()
        .map(|entry| entry.overloads.len())
        .sum::<usize>();
    let fields = http
        .types
        .iter()
        .map(|api_type| api_type.fields.len())
        .sum::<usize>();
    if v1.entries.len() != EXPECTED_ENTRIES
        || overloads != EXPECTED_OVERLOADS
        || http.routes.len() != EXPECTED_HTTP_ROUTES
        || http.types.len() != EXPECTED_HTTP_TYPES
        || fields != EXPECTED_HTTP_FIELDS
    {
        return Err(format!(
            "declaration denominator drift: entries={}/{EXPECTED_ENTRIES}, overloads={overloads}/{EXPECTED_OVERLOADS}, routes={}/{EXPECTED_HTTP_ROUTES}, types={}/{EXPECTED_HTTP_TYPES}, fields={fields}/{EXPECTED_HTTP_FIELDS}",
            v1.entries.len(),
            http.routes.len(),
            http.types.len()
        ));
    }
    let mut ids = BTreeSet::new();
    for entry in &v1.entries {
        validate_stable_id(&entry.id, Some("entry")).map_err(|error| error.to_string())?;
        if !ids.insert(entry.id.as_str()) {
            return Err(format!("duplicate v1 entry ID {}", entry.id));
        }
        for overload in &entry.overloads {
            validate_stable_id(&overload.id, Some("overload"))
                .map_err(|error| error.to_string())?;
            if !ids.insert(overload.id.as_str()) {
                return Err(format!("duplicate v1 overload ID {}", overload.id));
            }
        }
    }
    let routes = http
        .routes
        .iter()
        .map(|route| (&route.method, &route.path))
        .collect::<BTreeSet<_>>();
    let types = http
        .types
        .iter()
        .map(|api_type| (&api_type.source, &api_type.name))
        .collect::<BTreeSet<_>>();
    if routes.len() != EXPECTED_HTTP_ROUTES || types.len() != EXPECTED_HTTP_TYPES {
        return Err("HTTP bidirectional discovery contains duplicate route/type nodes".into());
    }
    let discovered_fields = http
        .types
        .iter()
        .flat_map(|api_type| {
            api_type
                .fields
                .iter()
                .map(|field| (&api_type.source, &api_type.name, &field.name))
        })
        .collect::<BTreeSet<_>>();
    if discovered_fields.len() != EXPECTED_HTTP_FIELDS {
        return Err("HTTP bidirectional discovery contains duplicate field nodes".into());
    }
    Ok(())
}

fn validate_accepted_projections(root: &Path, baseline: &ArtifactBaseline) -> Result<(), String> {
    for category in [
        "manifest",
        "http",
        "rhai_editor",
        "wasm",
        "cli",
        "docs",
        "fixtures",
    ] {
        let group = baseline
            .categories
            .get(category)
            .ok_or_else(|| format!("M00 baseline has no {category} category"))?;
        for file in &group.files {
            let bytes = fs::read(root.join(&file.path))
                .map_err(|error| format!("{}: {error}", file.path))?;
            if bytes.len() as u64 != file.bytes || sha256_hex(&bytes) != file.sha256 {
                return Err(format!(
                    "accepted M00 projection changed: {} (expected {} bytes {})",
                    file.path, file.bytes, file.sha256
                ));
            }
        }
    }
    Ok(())
}

fn load_fragments(root: &Path) -> Result<FragmentSet, String> {
    Ok(FragmentSet {
        authoring: parse_authoring_fragment(&read(root, "api/contract/authoring.toml")?)
            .map_err(|error| error.to_string())?,
        runtime: parse_runtime_fragment(&read(root, "api/contract/runtime.toml")?)
            .map_err(|error| error.to_string())?,
        http: parse_http_fragment(&read(root, "api/contract/http.toml")?)
            .map_err(|error| error.to_string())?,
        websocket: parse_websocket_fragment(&read(root, "api/contract/websocket.toml")?)
            .map_err(|error| error.to_string())?,
        wasm: parse_wasm_fragment(&read(root, "api/contract/wasm.toml")?)
            .map_err(|error| error.to_string())?,
        consumers: parse_consumers_fragment(&read(root, "api/contract/consumers.toml")?)
            .map_err(|error| error.to_string())?,
    })
}

fn contract_ids(manifest: &PublicApiManifestV2) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    visit_metadata(manifest, |metadata| {
        ids.insert(metadata.id.clone());
    });
    for api_type in &manifest.types {
        ids.extend(api_type.variants.iter().map(|variant| variant.id.clone()));
    }
    for operation in &manifest.operations {
        ids.extend(operation.effects.iter().map(|effect| effect.id.clone()));
    }
    ids
}

fn visit_metadata<F>(manifest: &PublicApiManifestV2, mut visit: F)
where
    F: FnMut(&NodeMetadata),
{
    for entry in &manifest.entries {
        visit(&entry.metadata);
        for overload in &entry.overloads {
            visit(&overload.metadata);
        }
    }
    for api_type in &manifest.types {
        visit(&api_type.metadata);
        for field in &api_type.fields {
            visit(&field.metadata);
        }
    }
    for operation in &manifest.operations {
        visit(&operation.metadata);
        for binding in &operation.bindings {
            visit(&binding.metadata);
        }
    }
    for event in &manifest.events {
        visit(&event.metadata);
    }
    for capability in &manifest.capabilities {
        visit(&capability.metadata);
    }
    for consumer in &manifest.consumers {
        visit(&consumer.metadata);
    }
}

fn entry_owner(entry: &ApiEntry) -> &'static str {
    match entry.surface.as_str() {
        "dsp_rhai" | "dsp_ugen" => "vibelang-dsp",
        "stdlib" => "vibelang-std",
        "rhai_extension" | "rhai" => "vibelang-rhai",
        _ => "vibelang-rhai",
    }
}

fn source_derivation(entry: &ApiEntry) -> Derivation {
    match entry.surface.as_str() {
        "dsp_ugen" => Derivation::Catalog,
        "stdlib" => Derivation::StdlibParse,
        _ => Derivation::RustAst,
    }
}

fn aliases(canonical_id: &str, values: &[String]) -> Vec<Alias> {
    let namespace = canonical_id.split(':').nth(1).unwrap_or("entry");
    let mut aliases = values
        .iter()
        .map(|value| Alias {
            id: semantic_id(namespace, &format!("alias|{canonical_id}|{value}")),
            canonical_id: canonical_id.into(),
            kind: AliasKind::LegacySpelling,
            since: API_VERSION.into(),
            deprecated_since: API_VERSION.into(),
            removal_not_before: "1.0.0".into(),
            warning: format!("{value} is a frozen v1 compatibility spelling"),
            behavior_fixture: V1_MANIFEST_PATH.into(),
            compatibility_classes: [CompatibilityClass::MetadataOnly].into_iter().collect(),
        })
        .collect::<Vec<_>>();
    aliases.sort_by(|left, right| left.id.cmp(&right.id));
    aliases
}

fn lifecycle(entry: &ApiEntry) -> LifecycleContract {
    let (role, phase, effects, timing, repeat) = match entry.lifecycle.terminal.as_str() {
        "type_registration" | "not_applicable" if entry.kind == "type" => (
            LifecycleRole::TypeRegistration,
            LifecyclePhase::Register,
            vec![LifecycleEffect::Register],
            EffectTiming::EvaluationLocal,
            RepeatSemantics::Idempotent,
        ),
        "property_get" => (
            LifecycleRole::Observation,
            LifecyclePhase::Observe,
            vec![LifecycleEffect::Observe],
            EffectTiming::EvaluationLocal,
            RepeatSemantics::Pure,
        ),
        "property_set" | "non_terminal_chain" => (
            LifecycleRole::LegacyHandle,
            LifecyclePhase::Configure,
            vec![LifecycleEffect::Configure],
            EffectTiming::EvaluationLocal,
            RepeatSemantics::Replace,
        ),
        "named_terminal" => (
            LifecycleRole::LegacyHandle,
            LifecyclePhase::Commit,
            terminal_effects(&entry.registered_name),
            EffectTiming::RuntimeQueued,
            RepeatSemantics::AdditionalEffect,
        ),
        "definition_registration" => (
            LifecycleRole::ModuleDefinition,
            LifecyclePhase::Register,
            vec![LifecycleEffect::Register],
            EffectTiming::EvaluationLocal,
            RepeatSemantics::Idempotent,
        ),
        _ => (
            LifecycleRole::Value,
            LifecyclePhase::PureCall,
            vec![LifecycleEffect::Construct],
            EffectTiming::EvaluationLocal,
            RepeatSemantics::Pure,
        ),
    };
    LifecycleContract {
        role,
        phase,
        effects,
        effect_timing: timing,
        synchronization: Synchronization::None,
        repeat,
        cancellation: Facet::<CancellationContract>::NotApplicable {
            reason: "current v1 cancellation semantics are preserved as compatibility debt".into(),
        },
    }
}

fn terminal_effects(name: &str) -> Vec<LifecycleEffect> {
    match name {
        "start" | "start_now" | "launch" | "now" | "run" | "restart" => {
            vec![LifecycleEffect::Start]
        }
        "stop" => vec![LifecycleEffect::Stop],
        "cancel" | "remove" => vec![LifecycleEffect::Cancel],
        _ => vec![LifecycleEffect::Register],
    }
}

fn failure(boundary: &BoundarySemantics) -> FailureContract {
    FailureContract {
        stages: vec![FailureStage::Validate],
        error_ids: Vec::new(),
        retryable: false,
        prior_state: PriorState::Unchanged,
        fallback: if boundary.fallbacks.status == "present" {
            FallbackPolicy::LegacyDefault
        } else {
            FallbackPolicy::None
        },
        panic_exposure: if boundary.panic_exposure.status == "present" {
            PanicExposure::Present
        } else {
            PanicExposure::None
        },
        delivery: [FailureDelivery::Return, FailureDelivery::Diagnostic]
            .into_iter()
            .collect(),
        cleanup_owner: None,
    }
}

fn availability_v2(value: &Availability, conditional_capability_id: &str) -> AvailabilityV2 {
    let mut evidence = Vec::new();
    evidence.extend(value.cfg.iter().map(|item| format!("cfg:{item}")));
    evidence.extend(value.targets.iter().map(|item| format!("target:{item}")));
    evidence.extend(value.features.iter().map(|item| format!("feature:{item}")));
    evidence.extend(value.plugins.iter().map(|item| format!("plugin:{item}")));
    evidence.extend(
        value
            .runtime_conditions
            .iter()
            .map(|item| format!("runtime:{item}")),
    );
    let status = match value.status.as_str() {
        "available" => AvailabilityStatus::Available,
        "conditional" => AvailabilityStatus::Conditional,
        "importable" => AvailabilityStatus::Importable,
        "quarantined" => AvailabilityStatus::Quarantined,
        "documentation_only" => AvailabilityStatus::DocumentationOnly,
        "unavailable" => AvailabilityStatus::Unavailable,
        _ => AvailabilityStatus::Unavailable,
    };
    AvailabilityV2 {
        status,
        when: (status == AvailabilityStatus::Conditional).then(|| CapabilityExpression::Ref {
            capability_id: conditional_capability_id.into(),
        }),
        on_unavailable: match status {
            AvailabilityStatus::Importable => UnavailableBehavior::LoadError,
            AvailabilityStatus::DocumentationOnly | AvailabilityStatus::Quarantined => {
                UnavailableBehavior::CompletionLabelOnly
            }
            _ => UnavailableBehavior::StructuredError,
        },
        evidence,
    }
}

fn available() -> AvailabilityV2 {
    AvailabilityV2 {
        status: AvailabilityStatus::Available,
        when: None,
        on_unavailable: UnavailableBehavior::StructuredError,
        evidence: vec!["mechanically present at accepted M01 lineage".into()],
    }
}

fn stable() -> Stability {
    Stability {
        level: StabilityLevel::Stable,
        since: Some(API_VERSION.into()),
        deprecated_since: None,
        replacement_id: None,
        reason: None,
        removal_not_before: None,
    }
}

fn ownership(implementation_owner: &str) -> Ownership {
    Ownership {
        contract_owner: "vibelang-api-manifest".into(),
        implementation_owner: implementation_owner.into(),
        consumer_owner: Some("xtask".into()),
    }
}

#[allow(clippy::too_many_arguments)]
fn metadata(
    id: String,
    name: impl Into<String>,
    owner: &str,
    path: &str,
    symbol: impl Into<String>,
    derivation: Derivation,
    availability: AvailabilityV2,
    test_anchors: Vec<ProvenanceAnchor>,
) -> NodeMetadata {
    NodeMetadata {
        id,
        name: name.into(),
        aliases: Vec::new(),
        stability: stable(),
        availability,
        ownership: ownership(owner),
        source_anchors: vec![ProvenanceAnchor {
            path: path.into(),
            symbol: symbol.into(),
            line: None,
            derivation,
        }],
        test_anchors,
    }
}

fn anchors(values: &[Anchor], derivation: Derivation) -> Vec<ProvenanceAnchor> {
    values
        .iter()
        .map(|anchor| ProvenanceAnchor {
            path: anchor.path.clone(),
            symbol: anchor.symbol.clone(),
            line: anchor.line,
            derivation,
        })
        .collect()
}

fn field_direction(derives: &[String]) -> FieldDirection {
    let input = derives.iter().any(|derive| derive == "Deserialize");
    let output = derives.iter().any(|derive| derive == "Serialize");
    match (input, output) {
        (true, true) => FieldDirection::Bidirectional,
        (true, false) => FieldDirection::Input,
        _ => FieldDirection::Output,
    }
}

fn serialized_field_name(field: &HttpField) -> String {
    for attribute in &field.serde {
        if let Some((_, tail)) = attribute.split_once("rename = \"") {
            if let Some((name, _)) = tail.split_once('"') {
                return name.into();
            }
        }
    }
    field.name.clone()
}

fn http_type_key(api_type: &HttpType) -> String {
    format!("{}|{}", api_type.source, api_type.name)
}

fn referenced_http_type(
    rust_type: &str,
    source: &str,
    type_ids_by_name: &BTreeMap<String, Vec<(String, String)>>,
) -> Result<Option<String>, String> {
    let Some((name, candidates)) = type_ids_by_name
        .iter()
        .filter(|(name, _)| {
            rust_type == name.as_str()
                || rust_type
                    .split(|character: char| !character.is_alphanumeric() && character != '_')
                    .any(|token| token == name.as_str())
        })
        .max_by_key(|(name, _)| name.len())
    else {
        return Ok(None);
    };
    if candidates.len() == 1 {
        return Ok(Some(candidates[0].1.clone()));
    }
    let local = candidates
        .iter()
        .filter(|(candidate_source, _)| candidate_source == source)
        .collect::<Vec<_>>();
    if local.len() == 1 {
        return Ok(Some(local[0].1.clone()));
    }
    Err(format!(
        "HTTP field type {rust_type} ambiguously references {name} from {} declarations",
        candidates.len()
    ))
}

fn require_hash(path: &str, bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = sha256_hex(bytes);
    if actual != expected {
        return Err(format!(
            "{path} changed from accepted SHA-256 {expected} to {actual}"
        ));
    }
    Ok(())
}

fn require_bytes(label: &str, generated: &str, committed: &str) -> Result<(), String> {
    if generated != committed {
        return Err(format!(
            "{label} is not byte-identical to the committed v1 projection"
        ));
    }
    Ok(())
}

fn read(root: &Path, relative: &str) -> Result<String, String> {
    fs::read_to_string(root.join(relative)).map_err(|error| format!("{relative}: {error}"))
}

fn pretty<T: Serialize>(value: &T) -> Result<String, String> {
    let mut json = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    json.push('\n');
    Ok(json)
}

fn write_or_check(root: &Path, relative: &str, generated: &str, check: bool) -> Result<(), String> {
    let path = root.join(relative);
    if check {
        let committed = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if committed != generated {
            return Err(format!(
                "{relative} is stale; run `cargo run -p xtask -- effective-contract generate`"
            ));
        }
        println!("{relative} is current");
    } else {
        fs::write(&path, generated)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        println!("generated {relative}");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactBaseline {
    schema: String,
    schema_version: u32,
    accepted_base: String,
    policy: Value,
    categories: BTreeMap<String, BaselineCategory>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineCategory {
    counts: Value,
    files: Vec<BaselineFile>,
    tree_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineFile {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HttpSnapshot {
    schema: String,
    schema_version: u32,
    routes: Vec<HttpRoute>,
    types: Vec<HttpType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HttpRoute {
    method: String,
    path: String,
    handler: String,
    availability: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HttpType {
    name: String,
    kind: String,
    source: String,
    derives: Vec<String>,
    availability: Vec<String>,
    fields: Vec<HttpField>,
    variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HttpField {
    name: String,
    rust_type: String,
    serde: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn generated_contract_matches_all_committed_m02_outputs() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let root = root();
                let discovery = discover(&root).unwrap();
                let first = compose(&root, &discovery).unwrap();
                let second = compose(&root, &discovery).unwrap();
                assert_eq!(first, second);
                for (path, content) in first {
                    assert_eq!(content, fs::read_to_string(root.join(path)).unwrap());
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn unclassified_and_orphan_debt_fail_closed() {
        let record = DebtRecord {
            id: semantic_id("debt", "probe"),
            surface: "probe".into(),
            node_id: None,
            operation_id: None,
            member: "probe".into(),
            legacy_class: "unclassified".into(),
            owner: "probe".into(),
            diagnostic_id: "probe".into(),
            issue: "probe".into(),
            exit_gate: "probe".into(),
            remove_by: "probe".into(),
            source_anchor: "probe".into(),
            test_anchor: "probe".into(),
        };
        let mut artifact = DebtArtifact {
            schema: "probe".into(),
            schema_version: 1,
            contract_digest: "probe".into(),
            records: vec![record],
            counts: BTreeMap::new(),
            orphan_count: 0,
            unclassified_count: 0,
        };
        assert!(artifact.validate(&BTreeSet::new()).is_err());
        artifact.records[0].legacy_class = "stale".into();
        artifact.records[0].node_id = Some(semantic_id("field", "missing"));
        assert!(artifact.validate(&BTreeSet::new()).is_err());
    }

    #[test]
    fn duplicate_http_type_names_remain_source_qualified() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let manifest = build_v2(&discovery).unwrap();
                let error_types = manifest
                    .types
                    .iter()
                    .filter(|api_type| api_type.metadata.name == "ErrorResponse")
                    .collect::<Vec<_>>();
                assert_eq!(error_types.len(), 2);
                assert_ne!(error_types[0].metadata.id, error_types[1].metadata.id);

                let model_error = error_types
                    .iter()
                    .find(|api_type| {
                        api_type.metadata.source_anchors[0].path
                            == "crates/vibelang-http/src/models.rs"
                    })
                    .unwrap();
                let midi_error = error_types
                    .iter()
                    .find(|api_type| {
                        api_type.metadata.source_anchors[0].path
                            == "crates/vibelang-http/src/routes/midi.rs"
                    })
                    .unwrap();
                assert_eq!(model_error.fields.len(), 2);
                assert_eq!(midi_error.fields.len(), 1);

                let transport = manifest
                    .operations
                    .iter()
                    .find(|operation| operation.metadata.name == "PATCH /transport")
                    .unwrap();
                let midi = manifest
                    .operations
                    .iter()
                    .find(|operation| operation.metadata.name == "POST /midi/note/on")
                    .unwrap();
                assert_eq!(transport.error_type_id, model_error.metadata.id);
                assert_eq!(midi.error_type_id, midi_error.metadata.id);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn source_discovery_covers_every_m02_surface_and_wasm_is_not_websocket() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let surfaces = discovery
                    .mechanical
                    .iter()
                    .map(|declaration| declaration.surface.as_str())
                    .collect::<BTreeSet<_>>();
                assert_eq!(
                    surfaces,
                    [
                        "cli",
                        "docs",
                        "emacs",
                        "fixtures",
                        "lsp",
                        "packages",
                        "vscode",
                        "wasm",
                        "websocket",
                    ]
                    .into_iter()
                    .collect()
                );
                assert_eq!(
                    discovery
                        .mechanical
                        .iter()
                        .filter(|declaration| {
                            declaration.surface == "websocket" && declaration.kind == "event"
                        })
                        .count(),
                    7
                );

                let manifest = build_v2(&discovery).unwrap();
                let wasm = manifest
                    .consumers
                    .iter()
                    .find(|consumer| consumer.eligibility.surfaces == ["wasm"])
                    .unwrap();
                let event_ids = manifest
                    .events
                    .iter()
                    .map(|event| event.metadata.id.as_str())
                    .collect::<BTreeSet<_>>();
                assert!(!wasm.included_ids.is_empty());
                assert!(wasm
                    .included_ids
                    .iter()
                    .all(|id| !event_ids.contains(id.as_str())));
                for surface in ["cli", "docs", "fixtures", "packages", "rhai_editor"] {
                    let consumer = manifest
                        .consumers
                        .iter()
                        .find(|consumer| consumer.eligibility.surfaces == [surface])
                        .unwrap();
                    assert!(!consumer.included_ids.is_empty(), "{surface}");
                    let coverage = &manifest.coverage[&consumer.metadata.id];
                    assert_eq!(coverage.numerator, consumer.included_ids.len() as u64);
                    assert!(coverage.denominator >= coverage.numerator, "{surface}");
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn one_new_discovered_item_without_classification_is_exactly_one_failure() {
        let discovered = vec![
            raw_declaration(
                "cli",
                "command",
                "known",
                "crates/vibelang-cli/src/main.rs",
                "Commands::Known",
            ),
            raw_declaration(
                "cli",
                "command",
                "newly_discovered",
                "crates/vibelang-cli/src/main.rs",
                "Commands::NewlyDiscovered",
            ),
        ];
        let classified = vec![classify_mechanical_declaration(discovered[0].clone()).unwrap()];
        let failures = missing_mechanical_classifications(&discovered, &classified).unwrap();
        assert_eq!(failures, ["cli:command:newly_discovered"]);
    }

    #[test]
    fn source_derived_consumer_coverage_counts_inclusions_exclusions_and_unresolved() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let manifest = build_v2(&discovery).unwrap();
                for consumer in &manifest.consumers {
                    let coverage = &manifest.coverage[&consumer.metadata.id];
                    assert_eq!(coverage.numerator, consumer.included_ids.len() as u64);
                    assert_eq!(
                        coverage.denominator,
                        coverage.numerator
                            + consumer.exclusions.len() as u64
                            + coverage.unresolved_ids.len() as u64
                    );
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn orphan_fragment_targets_fail_the_composer_join() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let mut discovery = discover(&root()).unwrap();
                discovery.fragments.authoring.records[0].target_id =
                    semantic_id("entry", "missing-composer-target");
                let manifest = build_v2(&discovery).unwrap();
                let error = validate_fragment_join(&discovery, &manifest, &discovery.fragments)
                    .unwrap_err();
                assert!(error.to_lowercase().contains("orphan"), "{error}");
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn semantic_join_accounting_counts_orphans_and_unclassified_nodes() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let manifest = build_v2(&discovery).unwrap();
                let ids = contract_ids(&manifest);
                let requirements = derive_semantic_requirements(&discovery).unwrap();

                let mut orphan = discovery.fragments.clone();
                orphan.authoring.records[0].target_id =
                    semantic_id("entry", "missing-composer-target");
                let accounting = semantic_join_accounting(
                    &ids,
                    &requirements,
                    &semantic_fragment_records(&orphan),
                );
                assert_eq!(accounting.orphan_records, 1);

                let mut unclassified = discovery.fragments.clone();
                unclassified.runtime.records[0].operation = None;
                let accounting = semantic_join_accounting(
                    &ids,
                    &requirements,
                    &semantic_fragment_records(&unclassified),
                );
                assert_eq!(accounting.orphan_records, 0);
                assert_eq!(accounting.unclassified_records, 1);
                assert_eq!(
                    accounting.unclassified_targets,
                    [semantic_id("operation", "http|PATCH|/transport")]
                        .into_iter()
                        .collect()
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn every_independently_required_semantic_facet_has_a_deletion_mutant() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let manifest = build_v2(&discovery).unwrap();
                let assert_missing = |fragments: &FragmentSet, facet: &str| {
                    let error =
                        validate_fragment_join(&discovery, &manifest, fragments).expect_err(facet);
                    assert!(error.contains(facet), "{facet}: {error}");
                };

                let mut mutant = discovery.fragments.clone();
                mutant.authoring.records[0].stability = None;
                assert_missing(&mutant, "Stability");

                let mut mutant = discovery.fragments.clone();
                mutant.runtime.records[0].operation = None;
                assert_missing(&mutant, "Operation");

                let mut mutant = discovery.fragments.clone();
                mutant.http.records[0].operation_id = None;
                assert_missing(&mutant, "OperationBinding");

                let mut mutant = discovery.fragments.clone();
                mutant.http.records[0].effectiveness = None;
                assert_missing(&mutant, "Effectiveness");

                let mut mutant = discovery.fragments.clone();
                mutant.websocket.records[0].event = None;
                assert_missing(&mutant, "Event");

                let mut mutant = discovery.fragments.clone();
                mutant.wasm.records[0].host = None;
                assert_missing(&mutant, "WasmHost");

                let mut mutant = discovery.fragments.clone();
                mutant.consumers.records[0].policy = None;
                assert_missing(&mutant, "ConsumerPolicy");

                let mut mutant = discovery.fragments.clone();
                mutant.consumers.records[0].coverage = None;
                assert_missing(&mutant, "Coverage");
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn required_runtime_wasm_and_consumer_owners_fail_closed() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let manifest = build_v2(&discovery).unwrap();
                for (domain, mut mutant) in [
                    ("runtime", discovery.fragments.clone()),
                    ("WASM", discovery.fragments.clone()),
                    ("consumer", discovery.fragments.clone()),
                ] {
                    match domain {
                        "runtime" => mutant.runtime.records[0].owner = "wrong-owner".into(),
                        "WASM" => mutant.wasm.records[0].owner = "wrong-owner".into(),
                        "consumer" => mutant.consumers.records[0].owner = "wrong-owner".into(),
                        _ => unreachable!(),
                    }
                    let error =
                        validate_fragment_join(&discovery, &manifest, &mutant).expect_err(domain);
                    assert!(error.contains("must be owned"), "{domain}: {error}");
                }

                for (domain, mut mutant) in [
                    ("runtime", discovery.fragments.clone()),
                    ("WASM", discovery.fragments.clone()),
                    ("consumer", discovery.fragments.clone()),
                ] {
                    match domain {
                        "runtime" => mutant.runtime.records[0].owner.clear(),
                        "WASM" => mutant.wasm.records[0].owner.clear(),
                        "consumer" => mutant.consumers.records[0].owner.clear(),
                        _ => unreachable!(),
                    }
                    let error =
                        validate_fragment_join(&discovery, &manifest, &mutant).expect_err(domain);
                    assert!(error.contains("must be owned"), "{domain}: {error}");
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn frozen_runtime_wasm_and_coverage_refinements_have_mutants() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let manifest = build_v2(&discovery).unwrap();

                let mut mutant = discovery.fragments.clone();
                mutant.runtime.records[0]
                    .operation
                    .as_mut()
                    .unwrap()
                    .effect_timing = EffectTiming::RuntimeApplied;
                let error = validate_fragment_join(&discovery, &manifest, &mutant).unwrap_err();
                assert!(error.contains("effect_timing=runtime_queued"), "{error}");

                let mut mutant = discovery.fragments.clone();
                mutant.runtime.records[0]
                    .operation
                    .as_mut()
                    .unwrap()
                    .atomicity = Atomicity::Required;
                let error = validate_fragment_join(&discovery, &manifest, &mutant).unwrap_err();
                assert!(error.contains("atomicity=best_effort"), "{error}");

                let mut mutant = discovery.fragments.clone();
                mutant.wasm.records[0]
                    .host
                    .as_mut()
                    .unwrap()
                    .required_globals
                    .clear();
                let error = validate_fragment_join(&discovery, &manifest, &mutant).unwrap_err();
                assert!(error.contains("globalThis.vibelangBridge"), "{error}");

                let mut mutant = discovery.fragments.clone();
                mutant.wasm.records[0].host.as_mut().unwrap().progress = WasmProgress::Synchronous;
                let error = validate_fragment_join(&discovery, &manifest, &mutant).unwrap_err();
                assert!(error.contains("host_tick"), "{error}");

                let mut mutant = discovery.fragments.clone();
                mutant.wasm.records[0]
                    .host
                    .as_mut()
                    .unwrap()
                    .canonical_package_owner = "landing-page/src/audio".into();
                let error = validate_fragment_join(&discovery, &manifest, &mutant).unwrap_err();
                assert!(error.contains("crates/vibelang-wasm ownership"), "{error}");

                let mut mutant = discovery.fragments.clone();
                mutant.consumers.records[0]
                    .coverage
                    .as_mut()
                    .unwrap()
                    .require_complete_eligibility = false;
                let error = validate_fragment_join(&discovery, &manifest, &mutant).unwrap_err();
                assert!(error.contains("complete eligibility"), "{error}");

                let mut mutant = discovery.fragments.clone();
                mutant.consumers.records[0]
                    .coverage
                    .as_mut()
                    .unwrap()
                    .allow_curated_exclusions = false;
                let error = validate_fragment_join(&discovery, &manifest, &mutant).unwrap_err();
                assert!(error.contains("owned curation"), "{error}");

                let mut mutant = discovery.fragments.clone();
                mutant.consumers.records[0]
                    .coverage
                    .as_mut()
                    .unwrap()
                    .forbid_denominator_shrink = false;
                let error = validate_fragment_join(&discovery, &manifest, &mutant).unwrap_err();
                assert!(error.contains("denominator preservation"), "{error}");
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn valid_semantic_join_composes_facets_owners_and_computed_zero_counts() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let mut manifest = build_v2(&discovery).unwrap();
                let composition =
                    validate_fragment_join(&discovery, &manifest, &discovery.fragments).unwrap();
                assert_eq!(composition.accounting.semantic_records, 6);
                assert_eq!(composition.accounting.orphan_records, 0);
                assert_eq!(composition.accounting.unclassified_records, 0);
                assert!(composition.accounting.unclassified_targets.is_empty());

                apply_fragments(&mut manifest, &discovery.fragments).unwrap();
                let runtime = manifest
                    .operations
                    .iter()
                    .find(|operation| operation.metadata.name == "PATCH /transport")
                    .unwrap();
                assert_eq!(
                    runtime.metadata.ownership.implementation_owner,
                    "vibelang-core"
                );
                assert_eq!(
                    runtime.effect_timing,
                    Facet::Applicable {
                        value: EffectTiming::RuntimeQueued
                    }
                );
                assert_eq!(
                    runtime.atomicity,
                    Facet::Applicable {
                        value: Atomicity::BestEffort
                    }
                );

                let wasm = manifest
                    .consumers
                    .iter()
                    .find(|consumer| consumer.eligibility.surfaces == ["wasm"])
                    .unwrap();
                assert_eq!(
                    wasm.metadata.ownership.implementation_owner,
                    "vibelang-wasm"
                );
                let Facet::Applicable { value: host } = &wasm.host else {
                    panic!("WASM host semantics were not composed");
                };
                assert_eq!(host.required_globals, ["globalThis.vibelangBridge"]);
                assert_eq!(host.progress, WasmProgress::HostTick);
                assert_eq!(host.canonical_package_owner, "crates/vibelang-wasm");

                let editor = manifest
                    .consumers
                    .iter()
                    .find(|consumer| consumer.eligibility.surfaces == ["rhai_editor"])
                    .unwrap();
                assert_eq!(
                    editor.metadata.ownership.implementation_owner,
                    "vibelang-tools"
                );
                assert!(matches!(editor.coverage_policy, Facet::Applicable { .. }));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn a_stale_checked_output_is_rejected() {
        let directory = root()
            .join("target")
            .join(format!("vibelang-m02-stale-output-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("output.json"), "old\n").unwrap();
        assert!(write_or_check(&directory, "output.json", "new\n", true).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
