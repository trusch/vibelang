use crate::{public_api, public_artifacts};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::Visit;
use syn::{Fields, FnArg, ImplItem, Item, ItemFn, ReturnType, Type, Visibility};
use vibelang_api_manifest::canonical::{canonical_sha256_hex, sha256_hex};
use vibelang_api_manifest::compatibility::{
    CompatibilityChange, CompatibilityClass, CompatibilityReport,
};
use vibelang_api_manifest::fragments::{
    parse_authoring_fragment, parse_consumers_fragment, parse_http_fragment,
    parse_runtime_fragment, parse_wasm_fragment, parse_websocket_fragment,
    ConsumerDenominatorBaselineSet, DiscoveredSemanticNode, FragmentSet, SemanticFacet,
};
use vibelang_api_manifest::v2::{
    semantic_id, validate_stable_id, Alias, AliasKind, ApiEntryV2, ApiType, Atomicity,
    AvailabilityStatus, AvailabilityV2, BindingDetails, CancellationContract, Capability,
    CapabilityExpression, CapabilityState, ConsistencyPoint, Consumer, ConsumerExclusion,
    CoverageRecord, Derivation, EffectTiming, Effectiveness, EffectivenessStatus, Eligibility,
    EnumVariant, Event, EventDelivery, EventOrdering, ExclusionReason, Facet, FailureContract,
    FailureDelivery, FailureStage, FallbackPolicy, Field, FieldBinding, FieldDirection, Generator,
    HttpSuccess, Idempotency, LifecycleContract, LifecycleEffect, LifecyclePhase, LifecycleRole,
    LossDetection, MigrationDebt, NodeMetadata, ObservableAt, ObservationContract, Operation,
    OperationKind, Ownership, PackageContract, PanicExposure, ParameterV2, PriorState,
    ProvenanceAnchor, PublicApiManifestV2, RepeatSemantics, RevisionRelation, Stability,
    StabilityLevel, SurfaceBinding, Synchronization, TypeKind, UnavailableBehavior, WasmProgress,
    SCHEMA_URI_V2, SCHEMA_VERSION_V2,
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
    "9a4484900c121e0e3c568ab13bd5609c4bb5ffcf500ede5a72cd164071c89c8e";
const ACCEPTED_V1_HTTP_SHA256: &str =
    "76949f764c09d24c6abb22ae1f3b9d1e16830f0fcd5a6450e8cce3c164b61055";
const ACCEPTED_CONSUMER_DENOMINATOR_BASELINE_SHA256: &str =
    "f7e8d99ba9fd97e3f28eae264a78b0be9116f6e8b9cf449c3fb9cd6588aef003";
const WASM_TYPES_PATH: &str = "crates/vibelang-wasm/types/index.d.ts";
const CORE_MANIFEST_PATH: &str = "crates/vibelang-core/Cargo.toml";
const CORE_WIRE_TEST_PATH: &str = "crates/vibelang-core/tests/mutation_ledger.rs";
const EXPECTED_ENTRIES: usize = 3_630;
const EXPECTED_OVERLOADS: usize = 8_439;
const EXPECTED_HTTP_ROUTES: usize = 103;
const EXPECTED_HTTP_TYPES: usize = 108;
const EXPECTED_HTTP_FIELDS: usize = 423;

pub fn generate(root: &Path, check: bool) -> Result<(), String> {
    let discovery = discover(root)?;
    generate_discovered(root, root, &discovery, check)
}

fn generate_discovered(
    source_root: &Path,
    output_root: &Path,
    discovery: &Discovery,
    check: bool,
) -> Result<(), String> {
    let first = compose(source_root, discovery)?;
    let second = compose(source_root, discovery)?;
    if first != second {
        return Err("effective-contract double generation produced different bytes".into());
    }

    for (path, content) in first {
        write_or_check(output_root, path, &content, check)?;
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct CoreCargoManifest {
    package: CoreCargoPackage,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct CoreCargoPackage {
    metadata: CoreCargoMetadata,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct CoreCargoMetadata {
    vibelang: CoreVibelangMetadata,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct CoreVibelangMetadata {
    #[serde(rename = "api-contract")]
    api_contract: CoreApiContractMetadata,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct CoreApiContractMetadata {
    #[serde(rename = "wire-source")]
    wire_source: String,
    #[serde(rename = "ledger-source")]
    ledger_source: String,
    #[serde(rename = "runtime-fragment")]
    runtime_fragment: String,
    #[serde(rename = "wire-schema-version")]
    wire_schema_version: u16,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CoreWireField {
    host_name: String,
    serialized_name: String,
    rust_type: String,
    required: bool,
    symbol: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CoreWireVariant {
    host_name: String,
    serialized_name: String,
}

#[derive(Debug, Clone, PartialEq)]
struct CoreWireDeclaration {
    name: String,
    kind: TypeKind,
    fields: Vec<CoreWireField>,
    variants: Vec<CoreWireVariant>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CoreLedgerOperation {
    symbol: String,
    request_type: String,
    response_type: String,
    error_type: String,
}

#[derive(Debug, Clone, PartialEq)]
struct CoreContractDiscovery {
    wire_source: String,
    ledger_source: String,
    declarations: Vec<CoreWireDeclaration>,
    operation: CoreLedgerOperation,
}

fn discover_core_contract(root: &Path) -> Result<CoreContractDiscovery, String> {
    let manifest: CoreCargoManifest = toml::from_str(&read(root, CORE_MANIFEST_PATH)?)
        .map_err(|error| format!("{CORE_MANIFEST_PATH}: {error}"))?;
    let metadata = manifest.package.metadata.vibelang.api_contract;
    let crate_root = root.join("crates/vibelang-core");
    let wire_path = contract_metadata_path(root, &crate_root, &metadata.wire_source)?;
    let ledger_path = contract_metadata_path(root, &crate_root, &metadata.ledger_source)?;
    let runtime_fragment = contract_metadata_path(root, &crate_root, &metadata.runtime_fragment)?;
    let expected_runtime_fragment = fs::canonicalize(root.join("api/contract/runtime.toml"))
        .map_err(|error| format!("api/contract/runtime.toml: {error}"))?;
    if runtime_fragment != expected_runtime_fragment {
        return Err(format!(
            "{CORE_MANIFEST_PATH} runtime-fragment must resolve to api/contract/runtime.toml"
        ));
    }
    let wire_source = relative_path(root, &wire_path)?;
    let ledger_source = relative_path(root, &ledger_path)?;
    let wire = syn::parse_file(
        &fs::read_to_string(&wire_path).map_err(|error| format!("{wire_source}: {error}"))?,
    )
    .map_err(|error| format!("{wire_source}: {error}"))?;
    let declared_schema_version = wire
        .items
        .iter()
        .find_map(|item| match item {
            Item::Const(item) if item.ident == "MUTATION_SCHEMA_VERSION" => {
                let syn::Expr::Lit(value) = &*item.expr else {
                    return None;
                };
                let syn::Lit::Int(value) = &value.lit else {
                    return None;
                };
                value.base10_parse::<u16>().ok()
            }
            _ => None,
        })
        .ok_or_else(|| format!("{wire_source} has no literal MUTATION_SCHEMA_VERSION"))?;
    if metadata.wire_schema_version != declared_schema_version {
        return Err(format!(
            "{CORE_MANIFEST_PATH} wire-schema-version {} disagrees with {wire_source} value {declared_schema_version}",
            metadata.wire_schema_version
        ));
    }

    let mut declarations = Vec::new();
    for item in &wire.items {
        match item {
            Item::Struct(item) if matches!(item.vis, Visibility::Public(_)) => {
                require_contract_marker(&item.attrs, &wire_source, &item.ident.to_string())?;
                declarations.push(core_struct_declaration(item)?);
            }
            Item::Enum(item) if matches!(item.vis, Visibility::Public(_)) => {
                require_contract_marker(&item.attrs, &wire_source, &item.ident.to_string())?;
                declarations.push(core_enum_declaration(item)?);
            }
            Item::Type(item) if matches!(item.vis, Visibility::Public(_)) => {
                require_contract_marker(&item.attrs, &wire_source, &item.ident.to_string())?;
                declarations.push(CoreWireDeclaration {
                    name: item.ident.to_string(),
                    kind: TypeKind::Alias,
                    fields: Vec::new(),
                    variants: Vec::new(),
                });
            }
            Item::Macro(item)
                if item.mac.path.is_ident("uuid_v7_id")
                    || item.mac.path.is_ident("decimal_u64") =>
            {
                let ident: syn::Ident = syn::parse2(item.mac.tokens.clone())
                    .map_err(|error| format!("{wire_source}: invalid wire macro: {error}"))?;
                declarations.push(CoreWireDeclaration {
                    name: ident.to_string(),
                    kind: TypeKind::Alias,
                    fields: Vec::new(),
                    variants: Vec::new(),
                });
            }
            _ => {}
        }
    }
    declarations.sort_by(|left, right| left.name.cmp(&right.name));
    if declarations.is_empty() {
        return Err(format!("{wire_source} produced no core wire declarations"));
    }
    for pair in declarations.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(format!("duplicate core wire declaration {}", pair[0].name));
        }
    }

    let ledger = syn::parse_file(
        &fs::read_to_string(&ledger_path).map_err(|error| format!("{ledger_source}: {error}"))?,
    )
    .map_err(|error| format!("{ledger_source}: {error}"))?;
    let mut operations = Vec::new();
    for item in &ledger.items {
        let Item::Impl(item) = item else { continue };
        if type_last_ident(&item.self_ty).as_deref() != Some("MutationLedger") {
            continue;
        }
        for impl_item in &item.items {
            let ImplItem::Fn(function) = impl_item else {
                continue;
            };
            let Some(marker) = contract_doc(&function.attrs, "@vibelang-contract-operation") else {
                continue;
            };
            if !matches!(function.vis, Visibility::Public(_)) {
                return Err(format!(
                    "{ledger_source}: contract operation {} is not public",
                    function.sig.ident
                ));
            }
            let fields = marker
                .split_whitespace()
                .filter_map(|field| field.split_once('='))
                .collect::<BTreeMap<_, _>>();
            let field = |name: &str| {
                fields
                    .get(name)
                    .map(|value| (*value).to_string())
                    .ok_or_else(|| {
                        format!(
                            "{ledger_source}: contract operation {} lacks {name}=...",
                            function.sig.ident
                        )
                    })
            };
            operations.push(CoreLedgerOperation {
                symbol: format!("MutationLedger::{}", function.sig.ident),
                request_type: field("request")?,
                response_type: field("response")?,
                error_type: field("error")?,
            });
        }
    }
    let [operation] = operations.as_slice() else {
        return Err(format!(
            "{ledger_source} must expose exactly one annotated M03 ledger operation, found {}",
            operations.len()
        ));
    };
    for name in [
        &operation.request_type,
        &operation.response_type,
        &operation.error_type,
    ] {
        if !declarations
            .iter()
            .any(|declaration| declaration.name == *name)
        {
            return Err(format!(
                "{ledger_source} operation {} references undiscovered wire type {name}",
                operation.symbol
            ));
        }
    }
    Ok(CoreContractDiscovery {
        wire_source,
        ledger_source,
        declarations,
        operation: operation.clone(),
    })
}

fn contract_metadata_path(root: &Path, crate_root: &Path, value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(format!(
            "core API-contract metadata path must be relative: {value}"
        ));
    }
    let canonical = fs::canonicalize(crate_root.join(path))
        .map_err(|error| format!("core API-contract metadata path {value}: {error}"))?;
    if !canonical.starts_with(root) {
        return Err(format!(
            "core API-contract metadata path escapes the repository: {value}"
        ));
    }
    Ok(canonical)
}

fn require_contract_marker(
    attrs: &[syn::Attribute],
    source: &str,
    name: &str,
) -> Result<(), String> {
    if contract_doc(attrs, "@vibelang-contract-wire").is_some() {
        Ok(())
    } else {
        Err(format!(
            "{source}: public wire declaration {name} lacks @vibelang-contract-wire"
        ))
    }
}

fn contract_doc(attrs: &[syn::Attribute], marker: &str) -> Option<String> {
    attrs.iter().find_map(|attribute| {
        if !attribute.path().is_ident("doc") {
            return None;
        }
        let syn::Meta::NameValue(value) = &attribute.meta else {
            return None;
        };
        let syn::Expr::Lit(value) = &value.value else {
            return None;
        };
        let syn::Lit::Str(value) = &value.lit else {
            return None;
        };
        value
            .value()
            .strip_prefix(marker)
            .map(|tail| tail.trim().to_string())
    })
}

fn core_struct_declaration(item: &syn::ItemStruct) -> Result<CoreWireDeclaration, String> {
    let name = item.ident.to_string();
    let fields = match &item.fields {
        Fields::Named(fields) => fields
            .named
            .iter()
            .map(|field| {
                let ident = field
                    .ident
                    .as_ref()
                    .ok_or_else(|| format!("{name} has an unnamed record field"))?;
                let host_name = ident.to_string();
                Ok(CoreWireField {
                    serialized_name: serde_rename(&field.attrs)
                        .unwrap_or_else(|| host_name.clone()),
                    rust_type: type_text(&field.ty),
                    required: outer_type_argument(&field.ty, "Option").is_none(),
                    symbol: format!("{name}::{host_name}"),
                    host_name,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        Fields::Unnamed(_) | Fields::Unit => Vec::new(),
    };
    Ok(CoreWireDeclaration {
        name,
        kind: if matches!(&item.fields, Fields::Named(_)) {
            TypeKind::Record
        } else {
            TypeKind::Alias
        },
        fields,
        variants: Vec::new(),
    })
}

fn core_enum_declaration(item: &syn::ItemEnum) -> Result<CoreWireDeclaration, String> {
    let name = item.ident.to_string();
    let mut fields = Vec::new();
    let mut variants = Vec::new();
    for variant in &item.variants {
        let host_variant = variant.ident.to_string();
        let serialized_variant =
            serde_rename(&variant.attrs).unwrap_or_else(|| to_snake_case(&host_variant));
        variants.push(CoreWireVariant {
            host_name: host_variant.clone(),
            serialized_name: serialized_variant.clone(),
        });
        match &variant.fields {
            Fields::Named(named) => {
                for field in &named.named {
                    let ident = field
                        .ident
                        .as_ref()
                        .ok_or_else(|| format!("{name}::{host_variant} has unnamed fields"))?;
                    let field_name = ident.to_string();
                    let serialized_field =
                        serde_rename(&field.attrs).unwrap_or_else(|| field_name.clone());
                    fields.push(CoreWireField {
                        host_name: format!("{host_variant}.{field_name}"),
                        serialized_name: format!("{serialized_variant}.details.{serialized_field}"),
                        rust_type: type_text(&field.ty),
                        required: outer_type_argument(&field.ty, "Option").is_none(),
                        symbol: format!("{name}::{host_variant}::{field_name}"),
                    });
                }
            }
            Fields::Unnamed(unnamed) => {
                for (index, field) in unnamed.unnamed.iter().enumerate() {
                    fields.push(CoreWireField {
                        host_name: format!("{host_variant}.{index}"),
                        serialized_name: format!("{serialized_variant}.details[{index}]"),
                        rust_type: type_text(&field.ty),
                        required: outer_type_argument(&field.ty, "Option").is_none(),
                        symbol: format!("{name}::{host_variant}::{index}"),
                    });
                }
            }
            Fields::Unit => {}
        }
    }
    fields.sort_by(|left, right| left.host_name.cmp(&right.host_name));
    variants.sort_by(|left, right| left.host_name.cmp(&right.host_name));
    Ok(CoreWireDeclaration {
        name,
        kind: if serde_attribute_has_key(&item.attrs, "tag") {
            TypeKind::TaggedUnion
        } else {
            TypeKind::Enum
        },
        fields,
        variants,
    })
}

fn serde_rename(attrs: &[syn::Attribute]) -> Option<String> {
    serde_attribute_value(attrs, "rename")
}

fn serde_attribute_has_key(attrs: &[syn::Attribute], key: &str) -> bool {
    attrs.iter().any(|attribute| {
        serde_attribute_items(attribute)
            .is_some_and(|items| items.into_iter().any(|meta| meta.path().is_ident(key)))
    })
}

fn serde_attribute_value(attrs: &[syn::Attribute], key: &str) -> Option<String> {
    attrs.iter().find_map(|attribute| {
        serde_attribute_items(attribute)?
            .into_iter()
            .find_map(|meta| {
                let syn::Meta::NameValue(value) = meta else {
                    return None;
                };
                if !value.path.is_ident(key) {
                    return None;
                }
                let syn::Expr::Lit(value) = value.value else {
                    return None;
                };
                let syn::Lit::Str(value) = value.lit else {
                    return None;
                };
                Some(value.value())
            })
    })
}

fn serde_attribute_items(attribute: &syn::Attribute) -> Option<Vec<syn::Meta>> {
    use syn::parse::Parser as _;
    if !attribute.path().is_ident("serde") {
        return None;
    }
    let syn::Meta::List(list) = &attribute.meta else {
        return None;
    };
    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
        .parse2(list.tokens.clone())
        .ok()
        .map(|items| items.into_iter().collect())
}

fn to_snake_case(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let mut output = String::new();
    for (index, character) in characters.iter().copied().enumerate() {
        let previous = index.checked_sub(1).and_then(|index| characters.get(index));
        let next = characters.get(index + 1);
        if character.is_ascii_uppercase()
            && !output.is_empty()
            && (previous.is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
                || (previous.is_some_and(|value| value.is_ascii_uppercase())
                    && next.is_some_and(|value| value.is_ascii_lowercase())))
        {
            output.push('_');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
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
            "class" | "method" | "function" | "compatibility_shim" | "interface" | "type_alias"
            | "type_member" | "host_bridge" | "host_bridge_method",
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
        .filter_map(|line| {
            line.trim()
                .split_once("js_sys::Reflect::get(&bridge, &JsValue::from_str(\"")
                .and_then(|(_, tail)| tail.split_once("\")"))
                .map(|(name, _)| name.to_string())
        })
        .filter(|name| !name.is_empty())
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
    let generated_types = read(root, WASM_TYPES_PATH)?;
    let generated_shapes = discover_generated_typescript_shapes(&generated_types, WASM_TYPES_PATH)?;
    let missing_shapes = missing_generated_typescript_shapes(&generated_types, &generated_shapes)?;
    if !missing_shapes.is_empty() {
        return Err(format!(
            "{} exported generated-TypeScript WASM shape(s) were not discovered: {}",
            missing_shapes.len(),
            missing_shapes.join(", ")
        ));
    }
    declarations.extend(generated_shapes);
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
                WASM_TYPES_PATH,
                format!("generated wasm-bindgen module export {name}"),
            ));
        }
    }
    Ok(())
}

fn discover_generated_typescript_shapes(
    source: &str,
    path: &str,
) -> Result<Vec<RawMechanicalDeclaration>, String> {
    let mut declarations = Vec::new();
    let mut cursor = 0;
    while cursor < source.len() {
        let line_end = source[cursor..]
            .find('\n')
            .map_or(source.len(), |offset| cursor + offset);
        let line = source[cursor..line_end].trim();
        if let Some(tail) = line.strip_prefix("export interface ") {
            let name = typescript_declaration_name(tail)
                .ok_or_else(|| format!("{path}: exported interface has no name"))?;
            let open = source[cursor..]
                .find('{')
                .map(|offset| cursor + offset)
                .ok_or_else(|| format!("{path}: exported interface {name} has no body"))?;
            let close = matching_typescript_delimiter(source, open, b'{', b'}')
                .ok_or_else(|| format!("{path}: exported interface {name} has no closing brace"))?;
            declarations.push(raw_declaration(
                "wasm",
                "interface",
                name.clone(),
                path,
                name.clone(),
            ));
            for member in typescript_members(&source[open + 1..close]) {
                declarations.push(raw_declaration(
                    "wasm",
                    "type_member",
                    format!("{name}.{member}"),
                    path,
                    format!("{name}::{member}"),
                ));
            }
            cursor = close + 1;
            continue;
        }
        if let Some(tail) = line.strip_prefix("export type ") {
            let name = typescript_declaration_name(tail)
                .ok_or_else(|| format!("{path}: exported type alias has no name"))?;
            let equals = source[cursor..]
                .find('=')
                .map(|offset| cursor + offset)
                .ok_or_else(|| format!("{path}: exported type alias {name} has no value"))?;
            let end = typescript_statement_end(source, equals + 1)
                .ok_or_else(|| format!("{path}: exported type alias {name} has no terminator"))?;
            declarations.push(raw_declaration(
                "wasm",
                "type_alias",
                name.clone(),
                path,
                name.clone(),
            ));
            let value = source[equals + 1..end].trim();
            if value.starts_with('{') {
                let open = equals + 1 + source[equals + 1..end].find('{').unwrap();
                let close =
                    matching_typescript_delimiter(source, open, b'{', b'}').ok_or_else(|| {
                        format!("{path}: exported type alias {name} has no closing brace")
                    })?;
                for member in typescript_members(&source[open + 1..close]) {
                    declarations.push(raw_declaration(
                        "wasm",
                        "type_member",
                        format!("{name}.{member}"),
                        path,
                        format!("{name}::{member}"),
                    ));
                }
            }
            cursor = end + 1;
            continue;
        }
        cursor = line_end.saturating_add(1);
    }
    Ok(declarations)
}

fn typescript_declaration_name(tail: &str) -> Option<String> {
    let name = tail
        .trim_start()
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '$'))
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

fn matching_typescript_delimiter(
    source: &str,
    open: usize,
    opening: u8,
    closing: u8,
) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0_u32;
    let mut quote = None;
    let mut escaped = false;
    let mut index = open;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
        } else if byte == opening {
            depth += 1;
        } else if byte == closing {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

fn typescript_statement_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut braces = 0_i32;
    let mut brackets = 0_i32;
    let mut parentheses = 0_i32;
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate().skip(start) {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' | b'`' => quote = Some(byte),
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            b'(' => parentheses += 1,
            b')' => parentheses -= 1,
            b';' if braces == 0 && brackets == 0 && parentheses == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn typescript_members(body: &str) -> BTreeSet<String> {
    let bytes = body.as_bytes();
    let mut members = BTreeSet::new();
    let mut start = 0;
    let mut braces = 0_i32;
    let mut brackets = 0_i32;
    let mut parentheses = 0_i32;
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' | b'`' => quote = Some(byte),
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            b'(' => parentheses += 1,
            b')' => parentheses -= 1,
            b';' if braces == 0 && brackets == 0 && parentheses == 0 => {
                if let Some(member) = typescript_member_name(&body[start..index]) {
                    members.insert(member);
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    if let Some(member) = typescript_member_name(&body[start..]) {
        members.insert(member);
    }
    members
}

fn typescript_member_name(statement: &str) -> Option<String> {
    let mut statement = statement.trim();
    while let Some(comment) = statement.strip_prefix("/*") {
        let end = comment.find("*/")?;
        statement = comment[end + 2..].trim_start();
    }
    for modifier in [
        "readonly ",
        "public ",
        "static ",
        "abstract ",
        "get ",
        "set ",
    ] {
        if let Some(tail) = statement.strip_prefix(modifier) {
            statement = tail.trim_start();
        }
    }
    if statement.starts_with('[') {
        let end = statement.find(']')?;
        return Some(
            statement[..=end]
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect(),
        );
    }
    if matches!(statement.as_bytes().first(), Some(b'\'' | b'"')) {
        let quote = statement.as_bytes()[0];
        let end = statement.as_bytes()[1..]
            .iter()
            .position(|byte| *byte == quote)?
            + 1;
        return Some(statement[..=end].into());
    }
    let name = statement
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '$'))
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

fn expected_generated_typescript_shapes(source: &str) -> BTreeSet<(String, String)> {
    let mut expected = BTreeSet::new();
    let mut interface = None;
    for line in source.lines() {
        let line = line.trim();
        if let Some(tail) = line.strip_prefix("export interface ") {
            interface = typescript_declaration_name(tail);
            if let Some(name) = &interface {
                expected.insert(("interface".into(), name.clone()));
            }
            continue;
        }
        if let Some(tail) = line.strip_prefix("export type ") {
            if let Some(name) = typescript_declaration_name(tail) {
                expected.insert(("type_alias".into(), name));
            }
            continue;
        }
        let Some(name) = &interface else { continue };
        if line.starts_with('}') {
            interface = None;
        } else if let Some(member) = typescript_member_name(line.trim_end_matches(';')) {
            expected.insert(("type_member".into(), format!("{name}.{member}")));
        }
    }
    expected
}

fn missing_generated_typescript_shapes(
    source: &str,
    discovered: &[RawMechanicalDeclaration],
) -> Result<Vec<String>, String> {
    let discovered = discovered
        .iter()
        .map(|declaration| (declaration.kind.clone(), declaration.name.clone()))
        .collect::<BTreeSet<_>>();
    Ok(expected_generated_typescript_shapes(source)
        .difference(&discovered)
        .map(|(kind, name)| format!("wasm:{kind}:{name}"))
        .collect())
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

#[derive(Debug, Clone, Eq, PartialEq)]
struct HttpHandlerContract {
    source: String,
    path_types: Vec<String>,
    query_types: Vec<String>,
    header_types: Vec<String>,
    body_type: Option<String>,
    success_statuses: Vec<u16>,
    success_type: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct HttpTypeAlias {
    source: String,
    target: String,
}

fn discover_http_type_aliases(root: &Path) -> Result<BTreeMap<String, HttpTypeAlias>, String> {
    let mut aliases = BTreeMap::new();
    for entry in WalkDir::new(root.join("crates/vibelang-http/src"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("rs")
        })
    {
        let path = entry.path();
        let source = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let file = syn::parse_file(&fs::read_to_string(path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("{source}: {error}"))?;
        for item in file.items {
            let Item::Type(alias) = item else { continue };
            if !matches!(alias.vis, Visibility::Public(_)) {
                continue;
            }
            let name = alias.ident.to_string();
            let value = HttpTypeAlias {
                source: source.clone(),
                target: type_text(&alias.ty),
            };
            if aliases.insert(name.clone(), value).is_some() {
                return Err(format!("duplicate public HTTP type alias {name}"));
            }
        }
    }
    Ok(aliases)
}

fn add_http_alias_type_ids(
    aliases: &BTreeMap<String, HttpTypeAlias>,
    type_ids_by_name: &mut BTreeMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    for (name, alias) in aliases {
        if type_ids_by_name.contains_key(name) {
            return Err(format!(
                "HTTP type alias {name} collides with a serialized DTO declaration"
            ));
        }
        if let Some(type_id) = referenced_http_type(&alias.target, &alias.source, type_ids_by_name)?
        {
            type_ids_by_name.insert(name.clone(), vec![(alias.source.clone(), type_id)]);
        }
    }
    Ok(())
}

fn add_core_http_type_ids(
    core: &CoreContractDiscovery,
    type_ids_by_name: &mut BTreeMap<String, Vec<(String, String)>>,
) {
    for declaration in &core.declarations {
        type_ids_by_name
            .entry(declaration.name.clone())
            .or_insert_with(|| {
                vec![(
                    core.wire_source.clone(),
                    core_wire_type_id(&declaration.name),
                )]
            });
    }
}

fn discover_http_handler_contracts(
    root: &Path,
    snapshot: &HttpSnapshot,
) -> Result<BTreeMap<String, HttpHandlerContract>, String> {
    let mut contracts = BTreeMap::new();
    for route in &snapshot.routes {
        if contracts.contains_key(&route.handler) {
            continue;
        }
        let source = http_handler_source(&route.handler)?;
        let file =
            syn::parse_file(&read(root, &source)?).map_err(|error| format!("{source}: {error}"))?;
        let name = route
            .handler
            .rsplit("::")
            .next()
            .ok_or_else(|| format!("HTTP route has empty handler {}", route.handler))?;
        let function = file
            .items
            .iter()
            .find_map(|item| match item {
                Item::Fn(function)
                    if function.sig.ident == name
                        && matches!(function.vis, Visibility::Public(_)) =>
                {
                    Some(function)
                }
                _ => None,
            })
            .ok_or_else(|| format!("{} does not define public handler {name}", source))?;
        contracts.insert(
            route.handler.clone(),
            http_handler_contract(&route.handler, source, function)?,
        );
    }
    let routed_handlers = snapshot
        .routes
        .iter()
        .map(|route| route.handler.as_str())
        .collect::<BTreeSet<_>>();
    if contracts.len() != routed_handlers.len() {
        return Err("HTTP handler discovery did not cover the exact routed handler set".into());
    }
    Ok(contracts)
}

fn http_handler_source(handler: &str) -> Result<String, String> {
    let segments = handler.split("::").collect::<Vec<_>>();
    match segments.as_slice() {
        ["routes", module, _] => Ok(format!("crates/vibelang-http/src/routes/{module}.rs")),
        ["websocket", _] => Ok("crates/vibelang-http/src/websocket.rs".into()),
        _ => Err(format!("unsupported HTTP handler path {handler}")),
    }
}

fn http_handler_contract(
    handler: &str,
    source: String,
    function: &ItemFn,
) -> Result<HttpHandlerContract, String> {
    let mut path_types = Vec::new();
    let mut query_types = Vec::new();
    let mut header_types = Vec::new();
    let mut body_type = None;
    for input in &function.sig.inputs {
        let FnArg::Typed(input) = input else {
            return Err(format!("HTTP handler {handler} has a receiver"));
        };
        if outer_type_argument(&input.ty, "State").is_some() {
            continue;
        }
        if type_last_ident(&input.ty).is_some_and(|name| name == "OriginalUri") {
            continue;
        }
        if let Some(ty) = outer_type_argument(&input.ty, "Path") {
            path_types.push(type_text(ty));
            continue;
        }
        if let Some(ty) = outer_type_argument(&input.ty, "Query") {
            query_types.push(type_text(ty));
            continue;
        }
        if let Some(ty) = outer_type_argument(&input.ty, "Json") {
            if body_type.replace(type_text(ty)).is_some() {
                return Err(format!("HTTP handler {handler} has multiple JSON bodies"));
            }
            continue;
        }
        if let Some(ty) = outer_type_argument(&input.ty, "TypedHeader") {
            header_types.push(type_text(ty));
            continue;
        }
        if type_last_ident(&input.ty)
            .is_some_and(|name| matches!(name.as_str(), "HeaderMap" | "WebSocketUpgrade"))
        {
            header_types.push(type_text(&input.ty));
            continue;
        }
        return Err(format!(
            "HTTP handler {handler} has unclassified extractor {}",
            type_text(&input.ty)
        ));
    }

    let success_type = match &function.sig.output {
        ReturnType::Default => "()".into(),
        ReturnType::Type(_, ty) => http_success_payload_type(handler, ty, &header_types)?,
    };
    let mut statuses = SuccessStatusVisitor::default();
    statuses.visit_block(&function.block);
    let mut success_statuses = statuses
        .values
        .into_iter()
        .filter(|status| (200..300).contains(status))
        .collect::<Vec<_>>();
    if success_type == "WebSocketUpgrade" {
        success_statuses = vec![101];
    } else if success_statuses.is_empty() {
        success_statuses.push(200);
    }

    Ok(HttpHandlerContract {
        source,
        path_types,
        query_types,
        header_types,
        body_type,
        success_statuses,
        success_type,
    })
}

fn outer_type_argument<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != wrapper {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    arguments.args.iter().find_map(|argument| match argument {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })
}

fn type_last_ident(ty: &Type) -> Option<String> {
    let Type::Path(path) = ty else {
        return None;
    };
    Some(path.path.segments.last()?.ident.to_string())
}

fn type_text(ty: &Type) -> String {
    ty.to_token_stream().to_string()
}

fn http_success_payload_type(
    handler: &str,
    ty: &Type,
    header_types: &[String],
) -> Result<String, String> {
    if let Some(ok) = outer_type_argument(ty, "Result") {
        return http_success_payload_type(handler, ok, header_types);
    }
    if let Some(payload) = outer_type_argument(ty, "Json") {
        return Ok(type_text(payload));
    }
    if type_last_ident(ty).as_deref() == Some("StatusCode") {
        return Ok("()".into());
    }
    if let Type::Tuple(tuple) = ty {
        if let Some(payload) = tuple
            .elems
            .iter()
            .find_map(|element| outer_type_argument(element, "Json"))
        {
            return Ok(type_text(payload));
        }
        if tuple.elems.is_empty() {
            return Ok("()".into());
        }
    }
    if matches!(ty, Type::ImplTrait(_))
        && header_types
            .iter()
            .any(|header| header == "WebSocketUpgrade")
    {
        return Ok("WebSocketUpgrade".into());
    }
    if matches!(ty, Type::Path(_)) {
        return Ok(type_text(ty));
    }
    Err(format!(
        "HTTP handler {handler} has unsupported success shape {}",
        type_text(ty)
    ))
}

#[derive(Default)]
struct SuccessStatusVisitor {
    values: BTreeSet<u16>,
}

impl<'ast> Visit<'ast> for SuccessStatusVisitor {
    fn visit_expr_path(&mut self, expression: &'ast syn::ExprPath) {
        let segments = &expression.path.segments;
        if segments.len() >= 2 && segments[segments.len() - 2].ident == "StatusCode" {
            if let Some(status) = http_status_value(&segments[segments.len() - 1].ident.to_string())
            {
                self.values.insert(status);
            }
        }
        syn::visit::visit_expr_path(self, expression);
    }
}

fn http_status_value(name: &str) -> Option<u16> {
    Some(match name {
        "SWITCHING_PROTOCOLS" => 101,
        "OK" => 200,
        "CREATED" => 201,
        "ACCEPTED" => 202,
        "NO_CONTENT" => 204,
        "PARTIAL_CONTENT" => 206,
        _ => return None,
    })
}

struct Discovery {
    v1: PublicApiManifest,
    v1_json: String,
    http: HttpSnapshot,
    http_handlers: BTreeMap<String, HttpHandlerContract>,
    http_type_aliases: BTreeMap<String, HttpTypeAlias>,
    baseline: ArtifactBaseline,
    fragments: FragmentSet,
    mechanical: Vec<MechanicalDeclaration>,
    core_contract: CoreContractDiscovery,
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
    let http_handlers = discover_http_handler_contracts(root, &http)?;
    let http_type_aliases = discover_http_type_aliases(root)?;

    validate_v1_inventory(&v1, &http)?;
    let baseline: ArtifactBaseline = serde_json::from_str(&read(root, BASELINE_PATH)?)
        .map_err(|error| format!("{BASELINE_PATH}: {error}"))?;
    validate_accepted_projections(root, &baseline)?;
    let fragments = load_fragments(root)?;
    let mechanical = discover_mechanical_declarations(root, &baseline)?;
    let core_contract = discover_core_contract(root)?;

    Ok(Discovery {
        v1,
        v1_json,
        http,
        http_handlers,
        http_type_aliases,
        baseline,
        fragments,
        mechanical,
        core_contract,
    })
}

fn compose(root: &Path, discovery: &Discovery) -> Result<BTreeMap<&'static str, String>, String> {
    let mut manifest = build_v2(discovery)?;
    let composition = validate_fragment_join(discovery, &manifest, &discovery.fragments)?;
    apply_fragments(&mut manifest, &discovery.fragments)?;
    validate_core_contract_graph(discovery, &manifest)?;
    validate_http_graph(discovery, &manifest)?;
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
    let conventions = crate::conventions::build(&discovery.v1)?;
    crate::conventions::attach(&mut manifest, conventions)?;

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
    validate_http_debt(&debt, &manifest)?;
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
    let midi_capability_id = semantic_id("capability", "http|feature.midi");
    let native_capability_id = semantic_id("capability", "http|target.native");
    let mut capabilities = vec![
        contract_capability(
            conditional_capability_id.clone(),
            "legacy declared condition",
            BASELINE_PATH,
            "api-unification M02 capability bridge",
            "v1 cfg/target/feature/plugin/runtime-condition evidence",
            "M02 preserves the v1 condition as evidence; runtime evaluation lands at M05",
        ),
        contract_capability(
            midi_capability_id.clone(),
            "MIDI HTTP feature",
            "crates/vibelang-http/src/lib.rs",
            "cfg(feature = \"midi\") route chain",
            "Cargo feature midi and the Axum cfg-gated route chain",
            "Expose the 19 MIDI routes only when feature.midi is available",
        ),
        contract_capability(
            native_capability_id.clone(),
            "native HTTP recording target",
            "crates/vibelang-http/src/lib.rs",
            "cfg(not(target_arch = \"wasm32\")) route chain",
            "Rust target architecture and the Axum cfg-gated route chain",
            "Expose the 4 recording routes only on non-wasm32 targets",
        ),
    ];
    capabilities.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));

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
    add_http_alias_type_ids(&discovery.http_type_aliases, &mut http_type_ids_by_name)?;
    add_core_http_type_ids(&discovery.core_contract, &mut http_type_ids_by_name);
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
    for handler in discovery.http_handlers.values() {
        for shape in handler
            .path_types
            .iter()
            .chain(&handler.query_types)
            .chain(&handler.header_types)
            .chain(handler.body_type.iter())
            .chain(std::iter::once(&handler.success_type))
        {
            if referenced_http_type(shape, &handler.source, &http_type_ids_by_name)?.is_none() {
                scalar_shapes.insert(shape.clone());
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
    types.extend(core_wire_manifest_types(&discovery.core_contract)?);
    types.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));

    let default_error_type_id = http_type_ids
        .get("crates/vibelang-http/src/models.rs|ErrorResponse")
        .ok_or_else(|| "HTTP snapshot has no models::ErrorResponse type".to_string())?;
    let midi_error_type_id = http_type_ids
        .get("crates/vibelang-http/src/routes/midi.rs|ErrorResponse")
        .ok_or_else(|| "HTTP snapshot has no midi::ErrorResponse type".to_string())?;
    let mut operations = Vec::new();
    let mut legacy_result_type_ids = BTreeMap::new();
    for route in &discovery.http.routes {
        let error_type_id = if route.handler.starts_with("routes::midi::") {
            midi_error_type_id
        } else {
            default_error_type_id
        };
        let handler = discovery
            .http_handlers
            .get(&route.handler)
            .ok_or_else(|| format!("missing HTTP handler contract for {}", route.handler))?;
        let resolved = resolve_http_route_contract(
            route,
            handler,
            error_type_id,
            &http_type_ids_by_name,
            &scalar_type_ids,
        )?;
        let operation = http_operation(
            route,
            handler,
            &resolved,
            &midi_capability_id,
            &native_capability_id,
        );
        if let Some(type_id) = resolved.legacy_result_type_id {
            legacy_result_type_ids.insert(operation.metadata.id.clone(), type_id);
        }
        operations.push(operation);
    }
    operations.push(core_ledger_operation(&discovery.core_contract)?);
    operations.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    connect_http_field_bindings(
        &mut types,
        &operations,
        &http_type_ids,
        &legacy_result_type_ids,
        &discovery.fragments,
    )?;

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
    let coverage = build_manifest_coverage(
        &consumer_ids,
        &consumer_accounting,
        &discovery.fragments.consumers.denominator_baseline,
    )?;

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
    stats.insert(
        "core_wire_types".into(),
        discovery.core_contract.declarations.len() as u64,
    );
    stats.insert(
        "core_wire_fields".into(),
        discovery
            .core_contract
            .declarations
            .iter()
            .map(|declaration| declaration.fields.len() as u64)
            .sum(),
    );
    stats.insert(
        "core_wire_variants".into(),
        discovery
            .core_contract
            .declarations
            .iter()
            .map(|declaration| declaration.variants.len() as u64)
            .sum(),
    );
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
        conventions: None,
    })
}

fn core_wire_type_id(name: &str) -> String {
    semantic_id("type", &format!("core-wire|{name}"))
}

fn core_ledger_operation_id(operation: &CoreLedgerOperation) -> String {
    semantic_id("operation", &format!("core-ledger|{}", operation.symbol))
}

fn core_wire_manifest_types(core: &CoreContractDiscovery) -> Result<Vec<ApiType>, String> {
    let type_ids = core
        .declarations
        .iter()
        .map(|declaration| {
            (
                declaration.name.clone(),
                core_wire_type_id(&declaration.name),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut scalar_shapes = BTreeSet::new();
    for declaration in &core.declarations {
        for field in &declaration.fields {
            if referenced_core_wire_type(&field.rust_type, &type_ids).is_none() {
                scalar_shapes.insert(field.rust_type.clone());
            }
        }
    }
    let scalar_type_ids = scalar_shapes
        .iter()
        .map(|shape| {
            (
                shape.clone(),
                semantic_id("type", &format!("core-wire-scalar|{shape}")),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut types = core
        .declarations
        .iter()
        .map(|declaration| {
            let mut fields = declaration
                .fields
                .iter()
                .map(|field| {
                    let type_id = referenced_core_wire_type(&field.rust_type, &type_ids)
                        .or_else(|| scalar_type_ids.get(&field.rust_type).cloned())
                        .ok_or_else(|| {
                            format!(
                                "core wire field {}::{} has no discovered type for {}",
                                declaration.name, field.host_name, field.rust_type
                            )
                        })?;
                    Ok(Field {
                        metadata: metadata(
                            semantic_id(
                                "field",
                                &format!(
                                    "core-wire|{}|{}",
                                    declaration.name, field.host_name
                                ),
                            ),
                            format!("{}::{}", declaration.name, field.host_name),
                            "vibelang-core",
                            &core.wire_source,
                            field.symbol.clone(),
                            Derivation::RustAst,
                            available(),
                            Vec::new(),
                        ),
                        serialized_name: field.serialized_name.clone(),
                        host_name: field.host_name.clone(),
                        direction: FieldDirection::Bidirectional,
                        required: field.required,
                        type_id,
                        default: None,
                        value_contract: Facet::NotApplicable {
                            reason: "M03 discovers canonical core wire shape; value semantics land at the owning domain milestone".into(),
                        },
                        operation_applicability: Facet::NotApplicable {
                            reason: "canonical nested wire fields are projected through their owning receipt operation rather than rebound independently".into(),
                        },
                        bindings: Vec::new(),
                        observation: Facet::NotApplicable {
                            reason: "M03 defines lossless carriers; live observation semantics land with runtime instrumentation".into(),
                        },
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            fields.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
            let mut variants = declaration
                .variants
                .iter()
                .map(|variant| EnumVariant {
                    id: semantic_id(
                        "variant",
                        &format!(
                            "core-wire|{}|{}",
                            declaration.name, variant.host_name
                        ),
                    ),
                    serialized_name: variant.serialized_name.clone(),
                })
                .collect::<Vec<_>>();
            variants.sort_by(|left, right| left.id.cmp(&right.id));
            Ok(ApiType {
                metadata: metadata(
                    type_ids[&declaration.name].clone(),
                    declaration.name.clone(),
                    "vibelang-core",
                    &core.wire_source,
                    declaration.name.clone(),
                    Derivation::RustAst,
                    available(),
                    Vec::new(),
                ),
                kind: declaration.kind,
                fields,
                variants,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    for shape in scalar_shapes {
        types.push(ApiType {
            metadata: metadata(
                scalar_type_ids[&shape].clone(),
                shape.clone(),
                "vibelang-core",
                &core.wire_source,
                format!("wire scalar {shape}"),
                Derivation::RustAst,
                available(),
                Vec::new(),
            ),
            kind: TypeKind::Alias,
            fields: Vec::new(),
            variants: Vec::new(),
        });
    }
    types.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    Ok(types)
}

fn referenced_core_wire_type(
    rust_type: &str,
    type_ids: &BTreeMap<String, String>,
) -> Option<String> {
    type_ids
        .iter()
        .filter(|(name, _)| {
            rust_type == name.as_str()
                || rust_type
                    .split(|character: char| !character.is_alphanumeric() && character != '_')
                    .any(|token| token == name.as_str())
        })
        .max_by_key(|(name, _)| name.len())
        .map(|(_, id)| id.clone())
}

fn core_ledger_operation(core: &CoreContractDiscovery) -> Result<Operation, String> {
    let operation = &core.operation;
    let type_id = |name: &str| {
        core.declarations
            .iter()
            .any(|declaration| declaration.name == name)
            .then(|| core_wire_type_id(name))
            .ok_or_else(|| format!("core ledger operation references missing type {name}"))
    };
    Ok(Operation {
        metadata: metadata(
            core_ledger_operation_id(operation),
            operation.symbol.clone(),
            "vibelang-core",
            &core.ledger_source,
            operation.symbol.clone(),
            Derivation::RustAst,
            available(),
            vec![ProvenanceAnchor {
                path: CORE_WIRE_TEST_PATH.into(),
                symbol: "canonical_receipt_status_error_capability_and_event_wires_round_trip"
                    .into(),
                line: None,
                derivation: Derivation::BehavioralFixture,
            }],
        ),
        kind: OperationKind::Mutation,
        request_type_id: Some(type_id(&operation.request_type)?),
        response_type_ids: vec![type_id(&operation.response_type)?],
        error_type_id: type_id(&operation.error_type)?,
        effects: Vec::new(),
        idempotency: Idempotency::Conditional,
        effect_timing: Facet::NotApplicable {
            reason: "runtime.toml owns the M03 ledger timing contract".into(),
        },
        atomicity: Facet::NotApplicable {
            reason: "runtime.toml owns the M03 ledger atomicity contract".into(),
        },
        revision: Facet::NotApplicable {
            reason: "runtime.toml owns the M03 revision-allocation contract".into(),
        },
        receipt: Facet::NotApplicable {
            reason: "runtime.toml owns the M03 canonical receipt contract".into(),
        },
        consistency: ConsistencyPoint::ResponseSnapshot,
        security_capability_ids: Vec::new(),
        bindings: Vec::new(),
    })
}

fn mechanical_type(declaration: &MechanicalDeclaration) -> ApiType {
    let derivation = if declaration
        .source_anchors
        .iter()
        .any(|(path, _)| path == WASM_TYPES_PATH)
    {
        Derivation::GeneratedProjection
    } else {
        match declaration.surface.as_str() {
            "fixtures" => Derivation::BehavioralFixture,
            "docs" | "packages" | "vscode" | "emacs" => Derivation::Catalog,
            _ => Derivation::RustAst,
        }
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
                operation_applicability: Facet::NotApplicable {
                    reason: "source-derived HTTP operation applicability is composed after type discovery"
                        .into(),
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

fn contract_capability(
    id: String,
    name: &str,
    path: &str,
    symbol: &str,
    detection_source: &str,
    projection_rule: &str,
) -> Capability {
    Capability {
        metadata: metadata(
            id,
            name,
            "vibelang-api-manifest",
            path,
            symbol,
            Derivation::ExplicitSemantics,
            available(),
            Vec::new(),
        ),
        detection_source: detection_source.into(),
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
        projection_rules: vec![projection_rule.into()],
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ResolvedHttpRouteContract {
    request_type_id: Option<String>,
    response_type_ids: Vec<String>,
    legacy_result_type_id: Option<String>,
    path_type_ids: Vec<String>,
    query_type_ids: Vec<String>,
    header_type_ids: Vec<String>,
    body_type_id: Option<String>,
    successes: Vec<HttpSuccess>,
    error_type_id: String,
}

fn resolve_http_route_contract(
    route: &HttpRoute,
    handler: &HttpHandlerContract,
    error_type_id: &str,
    type_ids_by_name: &BTreeMap<String, Vec<(String, String)>>,
    scalar_type_ids: &BTreeMap<String, String>,
) -> Result<ResolvedHttpRouteContract, String> {
    let path_type_ids = resolve_http_type_ids(
        &handler.path_types,
        &handler.source,
        type_ids_by_name,
        scalar_type_ids,
    )?;
    let query_type_ids = resolve_http_type_ids(
        &handler.query_types,
        &handler.source,
        type_ids_by_name,
        scalar_type_ids,
    )?;
    let header_type_ids = resolve_http_type_ids(
        &handler.header_types,
        &handler.source,
        type_ids_by_name,
        scalar_type_ids,
    )?;
    let body_type_id = handler
        .body_type
        .as_deref()
        .map(|shape| {
            resolve_http_type_id(shape, &handler.source, type_ids_by_name, scalar_type_ids)
        })
        .transpose()?;
    let legacy_success_type_id = resolve_http_type_id(
        &handler.success_type,
        &handler.source,
        type_ids_by_name,
        scalar_type_ids,
    )?;
    let carrier_type_id = (route.method != "GET" && route.path != "/eval")
        .then(|| {
            resolve_http_type_id(
                "MutationHttpResponse",
                "crates/vibelang-http/src/lib.rs",
                type_ids_by_name,
                scalar_type_ids,
            )
        })
        .transpose()?;
    let success_type_id = carrier_type_id
        .clone()
        .unwrap_or_else(|| legacy_success_type_id.clone());
    let legacy_result_type_id = carrier_type_id
        .as_ref()
        .map(|_| legacy_success_type_id.clone());
    let mut success_statuses = handler.success_statuses.clone();
    if carrier_type_id.is_some() {
        success_statuses.push(202);
        success_statuses.sort_unstable();
        success_statuses.dedup();
    }
    let successes = success_statuses
        .iter()
        .map(|status| HttpSuccess {
            status: *status,
            type_id: success_type_id.clone(),
        })
        .collect::<Vec<_>>();
    let response_type_ids = vec![success_type_id];
    let request_type_id = body_type_id
        .clone()
        .or_else(|| query_type_ids.first().cloned())
        .or_else(|| path_type_ids.first().cloned())
        .or_else(|| header_type_ids.first().cloned());

    let placeholders = route.path.matches('{').count();
    let extracted = handler
        .path_types
        .iter()
        .map(|shape| http_path_shape_arity(shape))
        .sum::<Result<usize, _>>()?;
    if placeholders != extracted {
        return Err(format!(
            "HTTP route {} {} has {placeholders} path placeholders but handler {} extracts {extracted}",
            route.method, route.path, route.handler
        ));
    }

    Ok(ResolvedHttpRouteContract {
        request_type_id,
        response_type_ids,
        legacy_result_type_id,
        path_type_ids,
        query_type_ids,
        header_type_ids,
        body_type_id,
        successes,
        error_type_id: error_type_id.into(),
    })
}

fn resolve_http_type_ids(
    shapes: &[String],
    source: &str,
    type_ids_by_name: &BTreeMap<String, Vec<(String, String)>>,
    scalar_type_ids: &BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    shapes
        .iter()
        .map(|shape| resolve_http_type_id(shape, source, type_ids_by_name, scalar_type_ids))
        .collect()
}

fn resolve_http_type_id(
    shape: &str,
    source: &str,
    type_ids_by_name: &BTreeMap<String, Vec<(String, String)>>,
    scalar_type_ids: &BTreeMap<String, String>,
) -> Result<String, String> {
    referenced_http_type(shape, source, type_ids_by_name)?.map_or_else(
        || {
            scalar_type_ids
                .get(shape)
                .cloned()
                .ok_or_else(|| format!("HTTP wire shape {shape} has no type ID"))
        },
        Ok,
    )
}

fn http_path_shape_arity(shape: &str) -> Result<usize, String> {
    let ty: Type = syn::parse_str(shape)
        .map_err(|error| format!("failed to parse HTTP path shape {shape}: {error}"))?;
    Ok(match ty {
        Type::Tuple(tuple) => tuple.elems.len(),
        _ => 1,
    })
}

fn http_operation(
    route: &HttpRoute,
    handler: &HttpHandlerContract,
    resolved: &ResolvedHttpRouteContract,
    midi_capability_id: &str,
    native_capability_id: &str,
) -> Operation {
    let operation_id = semantic_id(
        "operation",
        &format!("http|{}|{}", route.method, route.path),
    );
    let is_read = route.method == "GET";
    let route_capability_id = match route.availability.as_slice() {
        [] => None,
        [condition] if condition == "feature = \"midi\"" => Some(midi_capability_id),
        [condition] if condition == "not(target_arch = \"wasm32\")" => Some(native_capability_id),
        _ => unreachable!("validated HTTP source snapshot has an unknown route condition"),
    };
    let availability = if let Some(capability_id) = route_capability_id {
        AvailabilityV2 {
            status: AvailabilityStatus::Conditional,
            when: Some(CapabilityExpression::Ref {
                capability_id: capability_id.into(),
            }),
            on_unavailable: UnavailableBehavior::StructuredError,
            evidence: route.availability.clone(),
        }
    } else {
        AvailabilityV2 {
            status: AvailabilityStatus::Available,
            when: None,
            on_unavailable: UnavailableBehavior::StructuredError,
            evidence: vec!["unconditional Axum route registration".into()],
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
            &handler.source,
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
        request_type_id: resolved.request_type_id.clone(),
        response_type_ids: resolved.response_type_ids.clone(),
        error_type_id: resolved.error_type_id.clone(),
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
        security_capability_ids: Vec::new(),
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
                path_type_ids: resolved.path_type_ids.clone(),
                query_type_ids: resolved.query_type_ids.clone(),
                header_type_ids: resolved.header_type_ids.clone(),
                body_type_id: resolved.body_type_id.clone(),
                successes: resolved.successes.clone(),
                error_type_id: resolved.error_type_id.clone(),
                protocol_version: "v1".into(),
                authentication_capability_id: None,
                idempotency_header: None,
                revision_header: None,
            },
        }],
    }
}

fn connect_http_field_bindings(
    types: &mut [ApiType],
    operations: &[Operation],
    http_type_ids: &BTreeMap<String, String>,
    legacy_result_type_ids: &BTreeMap<String, String>,
    fragments: &FragmentSet,
) -> Result<(), String> {
    let raw_type_ids = http_type_ids.values().cloned().collect::<BTreeSet<_>>();
    let operation_ids_by_type = http_operation_ids_by_type(
        types,
        operations,
        &raw_type_ids,
        legacy_result_type_ids,
        fragments,
    )?;

    for api_type in types
        .iter_mut()
        .filter(|api_type| raw_type_ids.contains(&api_type.metadata.id))
    {
        let operation_ids = operation_ids_by_type
            .get(&api_type.metadata.id)
            .cloned()
            .unwrap_or_default();
        for field in &mut api_type.fields {
            field.operation_applicability = if operation_ids.is_empty() {
                Facet::NotApplicable {
                    reason: if api_type.metadata.name == "WebSocketEvent" {
                        "WebSocket envelope fields are event-owned rather than HTTP operation members"
                    } else {
                        "reserved legacy DTO field has no registered HTTP route and is explicit compatibility debt"
                    }
                    .into(),
                }
            } else {
                Facet::Applicable {
                    value: operation_ids.iter().cloned().collect(),
                }
            };
            field.bindings = operation_ids
                .iter()
                .map(|operation_id| FieldBinding {
                    operation_id: operation_id.clone(),
                    effectiveness: http_field_effectiveness(
                        &api_type.metadata.name,
                        &field.host_name,
                        field.direction,
                    ),
                })
                .collect();
        }
    }
    Ok(())
}

fn http_operation_ids_by_type(
    types: &[ApiType],
    operations: &[Operation],
    raw_type_ids: &BTreeSet<String>,
    legacy_result_type_ids: &BTreeMap<String, String>,
    fragments: &FragmentSet,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut nested = BTreeMap::<String, BTreeSet<String>>::new();
    for api_type in types
        .iter()
        .filter(|api_type| raw_type_ids.contains(&api_type.metadata.id))
    {
        nested.insert(
            api_type.metadata.id.clone(),
            api_type
                .fields
                .iter()
                .filter(|field| raw_type_ids.contains(&field.type_id))
                .map(|field| field.type_id.clone())
                .collect(),
        );
    }

    let mut operation_ids_by_type = BTreeMap::<String, BTreeSet<String>>::new();
    for operation in operations {
        let Some(binding) = operation.bindings.first() else {
            continue;
        };
        let BindingDetails::Http {
            path_type_ids,
            query_type_ids,
            header_type_ids,
            body_type_id,
            successes,
            error_type_id,
            ..
        } = &binding.details
        else {
            continue;
        };
        let mut pending = operation
            .response_type_ids
            .iter()
            .chain(path_type_ids)
            .chain(query_type_ids)
            .chain(header_type_ids)
            .chain(body_type_id)
            .chain(successes.iter().map(|success| &success.type_id))
            .chain(std::iter::once(error_type_id))
            .chain(legacy_result_type_ids.get(&operation.metadata.id))
            .filter(|id| raw_type_ids.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        let mut reached = BTreeSet::new();
        while let Some(type_id) = pending.pop() {
            if !reached.insert(type_id.clone()) {
                continue;
            }
            if let Some(children) = nested.get(&type_id) {
                pending.extend(children.iter().cloned());
            }
        }
        for type_id in reached {
            operation_ids_by_type
                .entry(type_id)
                .or_default()
                .insert(operation.metadata.id.clone());
        }
    }

    let http_operation_ids = operations
        .iter()
        .filter(|operation| {
            operation
                .bindings
                .first()
                .is_some_and(|binding| matches!(binding.details, BindingDetails::Http { .. }))
        })
        .map(|operation| operation.metadata.id.as_str())
        .collect::<BTreeSet<_>>();
    let declared_by_field = fragments
        .http
        .records
        .iter()
        .filter(|record| !record.operation_ids.is_empty())
        .map(|record| {
            (
                record.target_id.as_str(),
                record
                    .operation_ids
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut joined_fields = 0usize;
    let mut declared_type_ids = BTreeSet::new();
    for api_type in types
        .iter()
        .filter(|api_type| raw_type_ids.contains(&api_type.metadata.id))
    {
        let declared = api_type
            .fields
            .iter()
            .filter_map(|field| {
                declared_by_field
                    .get(field.metadata.id.as_str())
                    .map(|operation_ids| (field.metadata.name.clone(), operation_ids))
            })
            .collect::<Vec<_>>();
        joined_fields += declared.len();
        if declared.is_empty() {
            continue;
        }
        if operation_ids_by_type.contains_key(&api_type.metadata.id) {
            return Err(format!(
                "HTTP type {} has source-derived operation applicability; fragment records may not restate it",
                api_type.metadata.name
            ));
        }
        if declared.len() != api_type.fields.len() {
            return Err(format!(
                "HTTP type {} has fragment-declared applicability on only {} of {} fields",
                api_type.metadata.name,
                declared.len(),
                api_type.fields.len()
            ));
        }
        let type_operation_ids = declared[0].1.clone();
        for (field_name, operation_ids) in &declared {
            if *operation_ids != &type_operation_ids {
                return Err(format!(
                    "HTTP field {field_name} declares operation applicability diverging from its type {}",
                    api_type.metadata.name
                ));
            }
        }
        if let Some(unknown) = type_operation_ids
            .iter()
            .find(|id| !http_operation_ids.contains(id.as_str()))
        {
            return Err(format!(
                "HTTP type {} declares applicability to {} which is not an HTTP operation",
                api_type.metadata.name, unknown
            ));
        }
        operation_ids_by_type.insert(api_type.metadata.id.clone(), type_operation_ids);
        declared_type_ids.insert(api_type.metadata.id.clone());
    }
    if joined_fields != declared_by_field.len() {
        return Err(format!(
            "{} fragment applicability record(s) target no discovered HTTP field",
            declared_by_field.len() - joined_fields
        ));
    }
    for api_type in types
        .iter()
        .filter(|api_type| declared_type_ids.contains(&api_type.metadata.id))
    {
        let parent_operation_ids = &operation_ids_by_type[&api_type.metadata.id];
        for field in &api_type.fields {
            if !raw_type_ids.contains(&field.type_id) {
                continue;
            }
            if !operation_ids_by_type
                .get(&field.type_id)
                .is_some_and(|child| parent_operation_ids.is_subset(child))
            {
                return Err(format!(
                    "HTTP field {}.{} nests a DTO whose operation applicability does not cover the parent operations",
                    api_type.metadata.name, field.metadata.name
                ));
            }
        }
    }

    Ok(operation_ids_by_type)
}

fn http_field_effectiveness(
    type_name: &str,
    field_name: &str,
    direction: FieldDirection,
) -> Effectiveness {
    let dead = is_dead_http_field(type_name, field_name);
    Effectiveness {
        status: EffectivenessStatus::CompatibilityDebt,
        effect_ids: Vec::new(),
        error_ids: Vec::new(),
        observable_at: if matches!(direction, FieldDirection::Input) {
            ObservableAt::Desired
        } else {
            ObservableAt::ResponseOnly
        },
        migration: Some(MigrationDebt {
            owner: "vibelang-http".into(),
            issue: "M11 HTTP v2 effectiveness binding".into(),
            remove_by: "v2 release-ready gate".into(),
            diagnostic_id: if dead {
                "compat.http.dead_declaration"
            } else {
                "compat.http.operation_binding_pending"
            }
            .into(),
        }),
    }
}

fn validate_core_contract_graph(
    discovery: &Discovery,
    manifest: &PublicApiManifestV2,
) -> Result<(), String> {
    let expected_types = core_wire_manifest_types(&discovery.core_contract)?;
    let actual_types = manifest
        .types
        .iter()
        .filter(|api_type| {
            api_type.metadata.ownership.implementation_owner == "vibelang-core"
                && api_type
                    .metadata
                    .source_anchors
                    .iter()
                    .any(|anchor| anchor.path == discovery.core_contract.wire_source)
        })
        .cloned()
        .collect::<Vec<_>>();
    if actual_types != expected_types {
        return Err(
            "core wire types, fields, variants, serialized names, or type links disagree with the annotated Rust source"
                .into(),
        );
    }

    let operation_id = core_ledger_operation_id(&discovery.core_contract.operation);
    let operation = manifest
        .operations
        .iter()
        .find(|operation| operation.metadata.id == operation_id)
        .ok_or_else(|| {
            "annotated core ledger operation is missing from the manifest".to_string()
        })?;
    let mechanical = core_ledger_operation(&discovery.core_contract)?;
    if operation.metadata != mechanical.metadata
        || operation.request_type_id != mechanical.request_type_id
        || operation.response_type_ids != mechanical.response_type_ids
        || operation.error_type_id != mechanical.error_type_id
        || operation.bindings != mechanical.bindings
    {
        return Err(
            "core ledger operation request/response/error/test-anchor graph disagrees with source"
                .into(),
        );
    }

    let records = discovery
        .fragments
        .runtime
        .records
        .iter()
        .filter(|record| record.target_id == operation_id)
        .collect::<Vec<_>>();
    let [record] = records.as_slice() else {
        return Err(format!(
            "runtime.toml must contain exactly one semantic record for {operation_id}, found {}",
            records.len()
        ));
    };
    let semantics = record.operation.as_ref().ok_or_else(|| {
        format!("runtime.toml operation semantics are missing for {operation_id}")
    })?;
    let revision = record
        .revision
        .as_ref()
        .ok_or_else(|| format!("runtime.toml revision semantics are missing for {operation_id}"))?;
    let receipt = record
        .receipt
        .as_ref()
        .ok_or_else(|| format!("runtime.toml receipt semantics are missing for {operation_id}"))?;
    if record.failure.is_none() || record.effects.is_empty() {
        return Err(format!(
            "runtime.toml failure/effect semantics are incomplete for {operation_id}"
        ));
    }
    if operation.metadata.ownership.implementation_owner != record.owner
        || operation.kind != semantics.kind
        || operation.idempotency != semantics.idempotency
        || operation.consistency != semantics.consistency
        || operation.effect_timing
            != (Facet::Applicable {
                value: semantics.effect_timing,
            })
        || operation.atomicity
            != (Facet::Applicable {
                value: semantics.atomicity,
            })
        || operation.revision
            != (Facet::Applicable {
                value: revision.clone(),
            })
        || operation.receipt
            != (Facet::Applicable {
                value: receipt.clone(),
            })
        || operation.effects != record.effects
    {
        return Err(format!(
            "core ledger manifest semantics disagree with runtime.toml for {operation_id}"
        ));
    }

    let canonical_receipt_types = manifest
        .types
        .iter()
        .filter(|api_type| api_type.metadata.name == "MutationReceipt")
        .collect::<Vec<_>>();
    if canonical_receipt_types.len() != 1
        || canonical_receipt_types[0]
            .metadata
            .ownership
            .implementation_owner
            != "vibelang-core"
    {
        return Err("the canonical core MutationReceipt must resolve exactly once".into());
    }
    let receipt_projections = manifest
        .types
        .iter()
        .filter(|api_type| {
            api_type.metadata.name.ends_with("Receipt")
                && api_type.metadata.name != "MutationReceipt"
                && !api_type
                    .metadata
                    .name
                    .starts_with("mechanical wasm method ")
        })
        .collect::<Vec<_>>();
    if receipt_projections.len() != 1
        || receipt_projections[0].metadata.name
            != "mechanical wasm interface VibelangMutationReceipt"
        || receipt_projections[0]
            .metadata
            .ownership
            .implementation_owner
            != "vibelang-wasm"
        || !receipt_projections[0]
            .metadata
            .source_anchors
            .iter()
            .any(|anchor| anchor.path == WASM_TYPES_PATH)
    {
        let found = receipt_projections
            .iter()
            .map(|api_type| {
                format!(
                    "{} ({}, {})",
                    api_type.metadata.name,
                    api_type.metadata.ownership.implementation_owner,
                    api_type
                        .metadata
                        .source_anchors
                        .first()
                        .map_or("missing anchor", |anchor| anchor.path.as_str())
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "the canonical core MutationReceipt may only have the explicit WASM v1 carrier projection; found [{found}]"
        ));
    }
    Ok(())
}

fn validate_http_graph(
    discovery: &Discovery,
    manifest: &PublicApiManifestV2,
) -> Result<(), String> {
    let http_type_ids = discovery
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
        .collect::<BTreeMap<_, _>>();
    let raw_type_ids = http_type_ids.values().cloned().collect::<BTreeSet<_>>();
    let mut type_ids_by_name = BTreeMap::<String, Vec<(String, String)>>::new();
    for api_type in &discovery.http.types {
        type_ids_by_name
            .entry(api_type.name.clone())
            .or_default()
            .push((
                api_type.source.clone(),
                http_type_ids[&http_type_key(api_type)].clone(),
            ));
    }
    for candidates in type_ids_by_name.values_mut() {
        candidates.sort();
    }
    add_http_alias_type_ids(&discovery.http_type_aliases, &mut type_ids_by_name)?;
    add_core_http_type_ids(&discovery.core_contract, &mut type_ids_by_name);
    let scalar_type_ids = manifest
        .types
        .iter()
        .filter(|api_type| {
            api_type.kind == TypeKind::Alias
                && api_type.metadata.ownership.implementation_owner == "vibelang-http"
        })
        .map(|api_type| (api_type.metadata.name.clone(), api_type.metadata.id.clone()))
        .collect::<BTreeMap<_, _>>();
    let midi_capability_id = semantic_id("capability", "http|feature.midi");
    let native_capability_id = semantic_id("capability", "http|target.native");
    let default_error_type_id = &http_type_ids["crates/vibelang-http/src/models.rs|ErrorResponse"];
    let midi_error_type_id =
        &http_type_ids["crates/vibelang-http/src/routes/midi.rs|ErrorResponse"];

    let actual_operations = manifest
        .operations
        .iter()
        .filter_map(|operation| {
            operation
                .bindings
                .first()
                .and_then(|binding| match &binding.details {
                    BindingDetails::Http { method, path, .. } => {
                        Some(((method.clone(), path.clone()), operation))
                    }
                    _ => None,
                })
        })
        .collect::<BTreeMap<_, _>>();
    let expected_keys = discovery
        .http
        .routes
        .iter()
        .map(|route| (route.method.clone(), route.path.clone()))
        .collect::<BTreeSet<_>>();
    if actual_operations.keys().cloned().collect::<BTreeSet<_>>() != expected_keys {
        return Err("HTTP operation/binding set is not the exact source route set".into());
    }

    let mut legacy_result_type_ids = BTreeMap::new();
    for route in &discovery.http.routes {
        let key = (route.method.clone(), route.path.clone());
        let actual = actual_operations[&key];
        if actual.bindings.len() != 1 {
            return Err(format!(
                "HTTP operation {} {} must have exactly one source binding",
                route.method, route.path
            ));
        }
        let handler = &discovery.http_handlers[&route.handler];
        let error_type_id = if route.handler.starts_with("routes::midi::") {
            midi_error_type_id
        } else {
            default_error_type_id
        };
        let resolved = resolve_http_route_contract(
            route,
            handler,
            error_type_id,
            &type_ids_by_name,
            &scalar_type_ids,
        )?;
        let expected = http_operation(
            route,
            handler,
            &resolved,
            &midi_capability_id,
            &native_capability_id,
        );
        if let Some(type_id) = resolved.legacy_result_type_id {
            legacy_result_type_ids.insert(expected.metadata.id.clone(), type_id);
        }
        if actual.request_type_id != expected.request_type_id
            || actual.response_type_ids != expected.response_type_ids
            || actual.error_type_id != expected.error_type_id
            || actual.security_capability_ids != expected.security_capability_ids
            || actual.metadata.availability != expected.metadata.availability
            || actual.bindings[0].metadata.availability
                != expected.bindings[0].metadata.availability
            || actual.bindings[0].details != expected.bindings[0].details
        {
            return Err(format!(
                "HTTP graph mismatch for {} {}: request/response/extractor/success/condition/security links must match source",
                route.method, route.path
            ));
        }
    }

    let raw_types = manifest
        .types
        .iter()
        .filter(|api_type| raw_type_ids.contains(&api_type.metadata.id))
        .map(|api_type| (api_type.metadata.id.clone(), api_type))
        .collect::<BTreeMap<_, _>>();
    if raw_types.len() != EXPECTED_HTTP_TYPES {
        return Err(format!(
            "HTTP graph has {} source types, expected {EXPECTED_HTTP_TYPES}",
            raw_types.len()
        ));
    }
    for discovered in &discovery.http.types {
        let type_id = &http_type_ids[&http_type_key(discovered)];
        let actual = raw_types
            .get(type_id)
            .ok_or_else(|| format!("HTTP source type {} disappeared", discovered.name))?;
        let expected_fields = discovered
            .fields
            .iter()
            .map(|field| {
                Ok((
                    semantic_id(
                        "field",
                        &format!(
                            "http|{}|{}|{}",
                            discovered.source, discovered.name, field.name
                        ),
                    ),
                    resolve_http_type_id(
                        &field.rust_type,
                        &discovered.source,
                        &type_ids_by_name,
                        &scalar_type_ids,
                    )?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let actual_fields = actual
            .fields
            .iter()
            .map(|field| (field.metadata.id.clone(), field.type_id.clone()))
            .collect::<BTreeMap<_, _>>();
        if actual_fields != expected_fields {
            return Err(format!(
                "HTTP type {} field/type links are not the exact source set",
                discovered.name
            ));
        }
    }

    let expected_operation_ids = http_operation_ids_by_type(
        &manifest.types,
        &manifest.operations,
        &raw_type_ids,
        &legacy_result_type_ids,
        &discovery.fragments,
    )?;
    let event_payload_ids = manifest
        .events
        .iter()
        .map(|event| event.payload_type_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut field_count = 0usize;
    let mut disconnected_types = Vec::new();
    for api_type in raw_types.values() {
        let expected = expected_operation_ids
            .get(&api_type.metadata.id)
            .cloned()
            .unwrap_or_default();
        let explicitly_disconnected = event_payload_ids.contains(api_type.metadata.id.as_str())
            || api_type.metadata.name == "WebSocketEvent"
            || (!api_type.fields.is_empty()
                && api_type
                    .fields
                    .iter()
                    .all(|field| is_dead_http_field(&api_type.metadata.name, &field.host_name)));
        if expected.is_empty() && !explicitly_disconnected {
            disconnected_types.push(api_type.metadata.name.clone());
        }
        for field in &api_type.fields {
            field_count += 1;
            let actual = field
                .bindings
                .iter()
                .map(|binding| binding.operation_id.clone())
                .collect::<BTreeSet<_>>();
            let applicability_matches = match &field.operation_applicability {
                Facet::Applicable { value } => {
                    value.iter().cloned().collect::<BTreeSet<_>>() == expected
                        && value.len() == expected.len()
                }
                Facet::NotApplicable { .. } => expected.is_empty() && explicitly_disconnected,
            };
            if actual.len() != field.bindings.len() || actual != expected || !applicability_matches
            {
                return Err(format!(
                    "HTTP field {} has incomplete or orphan operation applicability",
                    field.metadata.name
                ));
            }
        }
    }
    if field_count != EXPECTED_HTTP_FIELDS {
        return Err(format!(
            "HTTP graph has {field_count} source fields, expected {EXPECTED_HTTP_FIELDS}"
        ));
    }
    if !disconnected_types.is_empty() {
        return Err(format!(
            "HTTP types are disconnected without explicit event/debt semantics: {}",
            disconnected_types.join(", ")
        ));
    }

    let available = actual_operations
        .values()
        .filter(|operation| operation.metadata.availability.status == AvailabilityStatus::Available)
        .count();
    let conditional = actual_operations.len() - available;
    if available != 80 || conditional != 23 {
        return Err(format!(
            "HTTP route conditions must be exactly 80 unconditional and 23 conditional, got {available}/{conditional}"
        ));
    }
    Ok(())
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
        .chain(
            operations
                .iter()
                .filter(|operation| {
                    operation
                        .bindings
                        .iter()
                        .any(|binding| matches!(binding.details, BindingDetails::Http { .. }))
                })
                .flat_map(|operation| {
                    std::iter::once(operation.metadata.id.clone()).chain(
                        operation
                            .bindings
                            .iter()
                            .map(|binding| binding.metadata.id.clone()),
                    )
                }),
        )
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
    baseline: &ConsumerDenominatorBaselineSet,
) -> Result<BTreeMap<String, CoverageRecord>, String> {
    baseline.validate().map_err(|error| error.to_string())?;
    if baseline.sha256 != ACCEPTED_CONSUMER_DENOMINATOR_BASELINE_SHA256 {
        return Err(format!(
            "consumer denominator baseline {} is not accepted; legitimate advancement requires an explicit, separately audited update of the accepted digest {}",
            baseline.sha256, ACCEPTED_CONSUMER_DENOMINATOR_BASELINE_SHA256
        ));
    }
    let expected_consumers = consumer_ids.keys().cloned().collect::<BTreeSet<_>>();
    let accepted_consumers = baseline
        .consumers
        .iter()
        .map(|consumer| consumer.consumer.clone())
        .collect::<BTreeSet<_>>();
    if accepted_consumers != expected_consumers {
        let missing = expected_consumers
            .difference(&accepted_consumers)
            .cloned()
            .collect::<Vec<_>>();
        let orphan = accepted_consumers
            .difference(&expected_consumers)
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!(
            "accepted consumer denominator baseline does not match discovered consumer families: missing={missing:?}, orphan={orphan:?}"
        ));
    }

    let mut coverage = BTreeMap::new();
    let mut shrunk = Vec::new();
    for (category, consumer) in accounting {
        let denominator = consumer.eligible_count() as u64;
        let consumer_id = &consumer_ids[category];
        let accepted = baseline
            .consumers
            .iter()
            .find(|accepted| accepted.consumer == *category)
            .ok_or_else(|| format!("accepted denominator disappeared for consumer {category}"))?;
        if accepted.consumer_id != *consumer_id {
            return Err(format!(
                "accepted denominator for consumer {category} targets {}, expected {consumer_id}",
                accepted.consumer_id
            ));
        }
        if denominator < accepted.accepted_denominator {
            shrunk.push(format!(
                "{category} current={denominator} accepted={}",
                accepted.accepted_denominator
            ));
        }
        let mut exclusions_by_reason = BTreeMap::new();
        for (reason, _) in consumer.exclusions.values() {
            *exclusions_by_reason
                .entry(exclusion_reason_name(*reason).into())
                .or_insert(0) += 1;
        }
        coverage.insert(
            consumer_id.clone(),
            CoverageRecord {
                numerator: consumer.included.len() as u64,
                denominator,
                exclusions_by_reason,
                unresolved_ids: consumer.unresolved.iter().cloned().collect(),
                stale_ids: consumer.stale.iter().cloned().collect(),
                base_denominator: accepted.accepted_denominator,
            },
        );
    }
    if !shrunk.is_empty() {
        return Err(format!(
            "consumer denominator shrink rejected before artifact writes; normal generation cannot reset accepted baselines: {}",
            shrunk.join(", ")
        ));
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
    let core_runtime = (
        core_ledger_operation_id(&discovery.core_contract.operation),
        "vibelang-core".to_string(),
    );
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
            target_id: core_runtime.0.clone(),
            facet: SemanticFacet::Operation,
            owner: core_runtime.1.clone(),
        },
        SemanticRequirement {
            target_id: core_runtime.0.clone(),
            facet: SemanticFacet::Revision,
            owner: core_runtime.1.clone(),
        },
        SemanticRequirement {
            target_id: core_runtime.0.clone(),
            facet: SemanticFacet::Receipt,
            owner: core_runtime.1.clone(),
        },
        SemanticRequirement {
            target_id: core_runtime.0.clone(),
            facet: SemanticFacet::Failure,
            owner: core_runtime.1.clone(),
        },
        SemanticRequirement {
            target_id: core_runtime.0,
            facet: SemanticFacet::Effects,
            owner: core_runtime.1,
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
        if !record.operation_ids.is_empty() {
            facets.insert(SemanticFacet::OperationBinding);
        }
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
            .chain(&record.operation_ids)
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
    let runtime_target = semantic_id("operation", "http|PATCH|/transport");
    if !requirements.iter().any(|requirement| {
        requirement.target_id == runtime_target
            && requirement.facet == SemanticFacet::Operation
            && requirement.owner == "vibelang-core"
    }) {
        return Err(format!(
            "frozen M02 runtime requirement disappeared for {runtime_target}"
        ));
    }
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
        if let Some(revision) = &record.revision {
            operation.revision = Facet::Applicable {
                value: revision.clone(),
            };
        }
        if let Some(receipt) = &record.receipt {
            operation.receipt = Facet::Applicable {
                value: receipt.clone(),
            };
        }
        if !record.effects.is_empty() {
            operation.effects = record.effects.clone();
            operation
                .effects
                .sort_by(|left, right| left.id.cmp(&right.id));
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
        if !record.operation_ids.is_empty() {
            if let Some(effectiveness) = &record.effectiveness {
                for operation_id in &record.operation_ids {
                    let binding = field
                        .bindings
                        .iter_mut()
                        .find(|binding| binding.operation_id == *operation_id)
                        .ok_or_else(|| {
                            format!(
                                "HTTP semantic record {} claims operation {} outside fragment-declared applicability",
                                record.target_id, operation_id
                            )
                        })?;
                    binding.effectiveness = effectiveness.clone();
                }
            }
        } else if let (Some(operation_id), Some(effectiveness)) =
            (&record.operation_id, &record.effectiveness)
        {
            let binding = field
                .bindings
                .iter_mut()
                .find(|binding| binding.operation_id == *operation_id)
                .ok_or_else(|| {
                    format!(
                        "HTTP semantic record {} claims operation {} outside source-derived applicability",
                        record.target_id, operation_id
                    )
                })?;
            binding.effectiveness = effectiveness.clone();
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
    accepted_denominator: u64,
    denominator_baseline_owner: String,
    denominator_baseline_revision: String,
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
            let baseline = discovery
                .fragments
                .consumers
                .denominator_baseline
                .consumers
                .iter()
                .find(|baseline| baseline.consumer_id == consumer.metadata.id)
                .expect("validated consumer denominator baseline");
            ConsumerCoverage {
                id: consumer.metadata.id.clone(),
                projection_paths: consumer.source_projections.clone(),
                included_count: coverage.numerator,
                eligible_count: coverage.denominator,
                accepted_denominator: coverage.base_denominator,
                denominator_baseline_owner: baseline.owner.clone(),
                denominator_baseline_revision: discovery
                    .fragments
                    .consumers
                    .denominator_baseline
                    .accepted_revision
                    .clone(),
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
            let operation_ids = if field.bindings.is_empty() {
                vec![None]
            } else {
                field
                    .bindings
                    .iter()
                    .filter(|binding| {
                        binding.effectiveness.status == EffectivenessStatus::CompatibilityDebt
                    })
                    .map(|binding| Some(binding.operation_id.clone()))
                    .collect()
            };
            for operation_id in operation_ids {
                records.push(DebtRecord {
                    id: semantic_id(
                        "debt",
                        &format!(
                            "http-field|{}|{}",
                            field.metadata.id,
                            operation_id.as_deref().unwrap_or("unbound")
                        ),
                    ),
                    surface: "http".into(),
                    node_id: Some(field.metadata.id.clone()),
                    operation_id,
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
                    exit_gate:
                        "M11 must implement or structurally reject this operation-scoped member"
                            .into(),
                    remove_by: "v2 release-ready gate".into(),
                    source_anchor: field.metadata.source_anchors[0].path.clone(),
                    test_anchor: "tests/fixtures/api-unification/v1/negative/ignored-fields.json"
                        .into(),
                });
            }
        }
    }
    for operation in &manifest.operations {
        let Some(binding) = operation.bindings.first() else {
            continue;
        };
        let BindingDetails::Http {
            method,
            path,
            authentication_capability_id,
            ..
        } = &binding.details
        else {
            continue;
        };
        if authentication_capability_id.is_none() {
            records.push(DebtRecord {
                id: semantic_id("debt", &format!("http-authentication|{method}|{path}")),
                surface: "http".into(),
                node_id: Some(operation.metadata.id.clone()),
                operation_id: Some(operation.metadata.id.clone()),
                member: "authentication".into(),
                legacy_class: "stale".into(),
                owner: "vibelang-http".into(),
                diagnostic_id: "compat.http.no_authentication".into(),
                issue: "M11 HTTP v2 security policy".into(),
                exit_gate: "define and enforce an authentication policy before remote exposure"
                    .into(),
                remove_by: "v2 release-ready gate".into(),
                source_anchor: "crates/vibelang-http/src/lib.rs".into(),
                test_anchor: "crates/vibelang-http/src/lib.rs".into(),
            });
        }
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
            diagnostic_id: "compat.websocket.legacy_v1_projection".into(),
            issue: "M11 versioned WebSocket contract".into(),
            exit_gate:
                "publish versioned payload schemas and ledger catch-up without changing legacy telemetry"
                    .into(),
            remove_by: "v2 release-ready gate".into(),
            source_anchor: "crates/vibelang-http/src/websocket.rs".into(),
            test_anchor: "crates/vibelang-http/src/websocket.rs".into(),
        });
    }
    for api_type in &manifest.types {
        let Some(anchor) = api_type.metadata.source_anchors.iter().find(|anchor| {
            anchor.path == WASM_TYPES_PATH
                && (anchor.symbol == "VibelangError"
                    || anchor.symbol.starts_with("VibelangError::"))
        }) else {
            continue;
        };
        records.push(DebtRecord {
            id: semantic_id(
                "debt",
                &format!("wasm-generated-error|{}", api_type.metadata.id),
            ),
            surface: "wasm".into(),
            node_id: Some(api_type.metadata.id.clone()),
            operation_id: None,
            member: anchor.symbol.replace("::", "."),
            legacy_class: "dead".into(),
            owner: "vibelang-wasm".into(),
            diagnostic_id: "compat.wasm.dead_generated_error_shape".into(),
            issue: "M12 WASM v2 structured error contract".into(),
            exit_gate:
                "remove the unused declaration or return it as the generated structured error type"
                    .into(),
            remove_by: "v2 release-ready gate".into(),
            source_anchor: WASM_TYPES_PATH.into(),
            test_anchor: "xtask/src/effective_contract.rs".into(),
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

fn validate_http_debt(debt: &DebtArtifact, manifest: &PublicApiManifestV2) -> Result<(), String> {
    let http_operation_ids = manifest
        .operations
        .iter()
        .filter(|operation| {
            operation
                .bindings
                .iter()
                .any(|binding| matches!(binding.details, BindingDetails::Http { .. }))
        })
        .map(|operation| operation.metadata.id.clone())
        .collect::<BTreeSet<_>>();
    let authentication_debt = debt
        .records
        .iter()
        .filter(|record| record.diagnostic_id == "compat.http.no_authentication")
        .filter_map(|record| record.operation_id.clone())
        .collect::<BTreeSet<_>>();
    if authentication_debt != http_operation_ids {
        return Err(
            "HTTP routes without authentication require exact operation-scoped compatibility debt"
                .into(),
        );
    }

    let expected_field_debt = manifest
        .types
        .iter()
        .filter(|api_type| {
            api_type
                .metadata
                .source_anchors
                .iter()
                .any(|anchor| anchor.derivation == Derivation::RustAst)
                && api_type
                    .metadata
                    .name
                    .contains(|character: char| character.is_alphabetic())
        })
        .flat_map(|api_type| {
            api_type.fields.iter().flat_map(|field| {
                if field.bindings.is_empty() {
                    vec![(field.metadata.id.clone(), None)]
                } else {
                    field
                        .bindings
                        .iter()
                        .filter(|binding| {
                            binding.effectiveness.status == EffectivenessStatus::CompatibilityDebt
                        })
                        .map(|binding| {
                            (
                                field.metadata.id.clone(),
                                Some(binding.operation_id.clone()),
                            )
                        })
                        .collect()
                }
            })
        })
        .collect::<BTreeSet<_>>();
    let actual_field_debt = debt
        .records
        .iter()
        .filter(|record| {
            matches!(
                record.diagnostic_id.as_str(),
                "compat.http.dead_declaration" | "compat.http.operation_binding_pending"
            )
        })
        .filter_map(|record| {
            record
                .node_id
                .as_ref()
                .map(|node_id| (node_id.clone(), record.operation_id.clone()))
        })
        .collect::<BTreeSet<_>>();
    if actual_field_debt != expected_field_debt {
        return Err(
            "HTTP field compatibility debt is not the exact source-derived applicability set"
                .into(),
        );
    }
    Ok(())
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
            | ("ClockStatusDto", "device_id" | "enabled")
            | ("ClockOutputRequest", "device_id" | "enabled")
            | ("RecordedNoteDto", "beat" | "note" | "velocity" | "duration")
            | (
                "RecordingResultDto",
                "device_id" | "note_count" | "cc_count" | "duration_beats" | "notes"
            )
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
    fn composed_v2_has_one_canonical_conventions_projection() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let root = root();
                let discovery = discover(&root).unwrap();
                let outputs = compose(&root, &discovery).unwrap();
                let manifest =
                    vibelang_api_manifest::v2::parse_v2_manifest(outputs.get(V2_PATH).unwrap())
                        .unwrap();
                let expected = crate::conventions::build(&discovery.v1).unwrap();
                assert_eq!(manifest.conventions.as_ref(), Some(&expected));
                assert_eq!(expected.capabilities.len(), 28);
                assert_eq!(expected.parameter_quantities.len(), 18_797);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn core_wire_operation_and_runtime_fragment_agree_bidirectionally() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let mut manifest = build_v2(&discovery).unwrap();
                validate_fragment_join(&discovery, &manifest, &discovery.fragments).unwrap();
                apply_fragments(&mut manifest, &discovery.fragments).unwrap();
                validate_core_contract_graph(&discovery, &manifest).unwrap();

                let discovered_fields = discovery
                    .core_contract
                    .declarations
                    .iter()
                    .map(|declaration| declaration.fields.len())
                    .sum::<usize>();
                let discovered_variants = discovery
                    .core_contract
                    .declarations
                    .iter()
                    .map(|declaration| declaration.variants.len())
                    .sum::<usize>();
                assert!(discovered_fields > 100);
                assert!(discovered_variants > 50);

                let mut missing_field = manifest.clone();
                missing_field
                    .types
                    .iter_mut()
                    .find(|api_type| api_type.metadata.name == "MutationReceipt")
                    .unwrap()
                    .fields
                    .pop();
                assert!(validate_core_contract_graph(&discovery, &missing_field).is_err());

                let mut missing_variant = manifest.clone();
                missing_variant
                    .types
                    .iter_mut()
                    .find(|api_type| api_type.metadata.name == "TerminalOutcome")
                    .unwrap()
                    .variants
                    .pop();
                assert!(validate_core_contract_graph(&discovery, &missing_variant).is_err());

                let mut missing_semantics = discovery.fragments.clone();
                let operation_id = core_ledger_operation_id(&discovery.core_contract.operation);
                missing_semantics
                    .runtime
                    .records
                    .iter_mut()
                    .find(|record| record.target_id == operation_id)
                    .unwrap()
                    .receipt = None;
                assert!(validate_fragment_join(&discovery, &manifest, &missing_semantics).is_err());
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
    fn typed_http_graph_covers_exact_routes_types_fields_conditions_and_debt() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let mut manifest = build_v2(&discovery).unwrap();
                apply_fragments(&mut manifest, &discovery.fragments).unwrap();
                validate_http_graph(&discovery, &manifest).unwrap();

                let http_operations = manifest
                    .operations
                    .iter()
                    .filter(|operation| {
                        operation
                            .bindings
                            .iter()
                            .any(|binding| matches!(&binding.details, BindingDetails::Http { .. }))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(http_operations.len(), EXPECTED_HTTP_ROUTES);
                assert_eq!(
                    discovery
                        .http
                        .types
                        .iter()
                        .map(|api_type| api_type.fields.len())
                        .sum::<usize>(),
                    EXPECTED_HTTP_FIELDS
                );
                assert_eq!(
                    http_operations
                        .iter()
                        .filter(|operation| operation.request_type_id.is_some())
                        .count(),
                    79
                );
                assert!(http_operations.iter().all(|operation| {
                    !operation.response_type_ids.is_empty()
                        && operation.security_capability_ids.is_empty()
                        && matches!(
                            &operation.bindings[0].details,
                            BindingDetails::Http {
                                successes,
                                authentication_capability_id: None,
                                ..
                            } if !successes.is_empty()
                        )
                }));
                let available = http_operations
                    .iter()
                    .filter(|operation| {
                        operation.metadata.availability.status == AvailabilityStatus::Available
                    })
                    .count();
                assert_eq!((available, http_operations.len() - available), (80, 23));

                let carrier_id = semantic_id(
                    "type",
                    "http|crates/vibelang-http/src/lib.rs|MutationHttpResponse",
                );
                let start = http_operations
                    .iter()
                    .find(|operation| operation.metadata.name == "POST /transport/start")
                    .unwrap();
                assert_eq!(
                    start
                        .bindings
                        .first()
                        .and_then(|binding| match &binding.details {
                            BindingDetails::Http { successes, .. } => Some(successes),
                            _ => None,
                        })
                        .unwrap()
                        .iter()
                        .map(|success| success.type_id.as_str())
                        .collect::<BTreeSet<_>>(),
                    [carrier_id.as_str()].into_iter().collect()
                );
                assert!(start.response_type_ids.contains(&carrier_id));

                let keyboard_operation_id =
                    semantic_id("operation", "http|POST|/midi/route/keyboard");
                let keyboard_message = manifest
                    .types
                    .iter()
                    .find(|api_type| api_type.metadata.name == "AddKeyboardRouteResponse")
                    .unwrap()
                    .fields
                    .iter()
                    .find(|field| field.host_name == "message")
                    .unwrap();
                assert_eq!(
                    keyboard_message.operation_applicability,
                    Facet::Applicable {
                        value: vec![keyboard_operation_id.clone()]
                    }
                );
                assert_eq!(
                    keyboard_message
                        .bindings
                        .iter()
                        .map(|binding| binding.operation_id.as_str())
                        .collect::<BTreeSet<_>>(),
                    [keyboard_operation_id.as_str()].into_iter().collect()
                );

                let receipt_lookup = http_operations
                    .iter()
                    .find(|operation| operation.metadata.name == "GET /receipts/{attempt_id}")
                    .unwrap();
                assert_eq!(
                    receipt_lookup.response_type_ids,
                    [core_wire_type_id("MutationReceipt")]
                );

                let raw_type_ids = discovery
                    .http
                    .types
                    .iter()
                    .map(|api_type| {
                        semantic_id(
                            "type",
                            &format!("http|{}|{}", api_type.source, api_type.name),
                        )
                    })
                    .collect::<BTreeSet<_>>();
                let raw_fields = manifest
                    .types
                    .iter()
                    .filter(|api_type| raw_type_ids.contains(&api_type.metadata.id))
                    .flat_map(|api_type| &api_type.fields)
                    .collect::<Vec<_>>();
                assert_eq!(raw_fields.len(), EXPECTED_HTTP_FIELDS);
                assert!(raw_fields
                    .iter()
                    .all(|field| match &field.operation_applicability {
                        Facet::Applicable { value } =>
                            !value.is_empty() && !field.bindings.is_empty(),
                        Facet::NotApplicable { reason } => {
                            !reason.is_empty() && field.bindings.is_empty()
                        }
                    }));

                let accounting = semantic_join_accounting(
                    &contract_ids(&manifest),
                    &derive_semantic_requirements(&discovery).unwrap(),
                    &semantic_fragment_records(&discovery.fragments),
                );
                let debt = build_debt(&root(), &manifest, "fixture", &accounting).unwrap();
                validate_http_debt(&debt, &manifest).unwrap();
                assert_eq!(
                    debt.records
                        .iter()
                        .filter(|record| record.diagnostic_id == "compat.http.no_authentication")
                        .count(),
                    EXPECTED_HTTP_ROUTES
                );
                assert!(!debt.records.iter().any(|record| {
                    matches!(
                        record.diagnostic_id.as_str(),
                        "compat.http.stale_success" | "compat.wasm.false_success"
                    )
                }));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn disconnected_or_orphan_http_links_and_false_security_fail_closed() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let mut manifest = build_v2(&discovery).unwrap();
                apply_fragments(&mut manifest, &discovery.fragments).unwrap();

                let mut missing_request = manifest.clone();
                missing_request
                    .operations
                    .iter_mut()
                    .find(|operation| operation.metadata.name == "POST /voices")
                    .unwrap()
                    .request_type_id = None;
                assert!(validate_http_graph(&discovery, &missing_request).is_err());

                let mut missing_success = manifest.clone();
                let operation = missing_success
                    .operations
                    .iter_mut()
                    .find(|operation| operation.metadata.name == "GET /voices")
                    .unwrap();
                operation.response_type_ids.clear();
                let BindingDetails::Http { successes, .. } = &mut operation.bindings[0].details
                else {
                    unreachable!()
                };
                successes.clear();
                assert!(validate_http_graph(&discovery, &missing_success).is_err());

                let mut disconnected_field = manifest.clone();
                let field = disconnected_field
                    .types
                    .iter_mut()
                    .find(|api_type| api_type.metadata.name == "VoiceCreate")
                    .unwrap()
                    .fields
                    .iter_mut()
                    .find(|field| !field.bindings.is_empty())
                    .unwrap();
                field.bindings.pop();
                assert!(validate_http_graph(&discovery, &disconnected_field).is_err());

                let mut orphan_field = manifest.clone();
                let wrong_operation_id = semantic_id("operation", "http|GET|/ws");
                let field = orphan_field
                    .types
                    .iter_mut()
                    .find(|api_type| api_type.metadata.name == "VoiceCreate")
                    .unwrap()
                    .fields
                    .iter_mut()
                    .find(|field| !field.bindings.is_empty())
                    .unwrap();
                field.bindings[0].operation_id = wrong_operation_id;
                assert!(validate_http_graph(&discovery, &orphan_field).is_err());

                let mut false_security = manifest.clone();
                let generic_capability_id = semantic_id("capability", "legacy|declared-condition");
                let operation = false_security
                    .operations
                    .iter_mut()
                    .find(|operation| operation.metadata.name == "GET /voices")
                    .unwrap();
                operation.security_capability_ids = vec![generic_capability_id.clone()];
                let BindingDetails::Http {
                    authentication_capability_id,
                    ..
                } = &mut operation.bindings[0].details
                else {
                    unreachable!()
                };
                *authentication_capability_id = Some(generic_capability_id);
                assert!(validate_http_graph(&discovery, &false_security).is_err());

                let mut false_condition = manifest;
                let operation = false_condition
                    .operations
                    .iter_mut()
                    .find(|operation| operation.metadata.name == "GET /recordings")
                    .unwrap();
                operation.metadata.availability = available();
                operation.bindings[0].metadata.availability = available();
                assert!(validate_http_graph(&discovery, &false_condition).is_err());
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
                    9
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
    fn generated_typescript_wasm_shapes_and_dead_error_debt_are_complete() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let discovery = discover(&root()).unwrap();
                let generated = discovery
                    .mechanical
                    .iter()
                    .filter(|declaration| {
                        declaration
                            .source_anchors
                            .iter()
                            .any(|(path, _)| path == WASM_TYPES_PATH)
                    })
                    .map(|declaration| (declaration.kind.as_str(), declaration.name.as_str()))
                    .collect::<BTreeSet<_>>();
                for expected in [
                    ("interface", "VibelangResult"),
                    ("type_member", "VibelangResult.success"),
                    ("interface", "VibelangCompiledSynthdef"),
                    ("type_member", "VibelangCompiledSynthdef.data"),
                    ("interface", "VibelangError"),
                    ("type_member", "VibelangError.message"),
                    ("type_member", "VibelangError.name"),
                    ("type_member", "VibelangError.stack"),
                    ("interface", "VibelangBridge"),
                    ("type_member", "VibelangBridge.loadSynthdef"),
                    ("type_alias", "InitInput"),
                    ("type_alias", "SyncInitInput"),
                    ("interface", "InitOutput"),
                    ("type_member", "InitOutput.memory"),
                    ("type_member", "InitOutput.[exportName:string]"),
                    ("compatibility_shim", "initSync"),
                    ("compatibility_shim", "__wbg_init"),
                ] {
                    assert!(generated.contains(&expected), "missing {expected:?}");
                }
                assert!(!generated.iter().any(|(_, name)| {
                    name.starts_with("ExecutionResult") || name.starts_with("CompiledSynthdef")
                }));
                assert!(discovery.mechanical.iter().any(|declaration| {
                    declaration.kind == "host_bridge_method"
                        && declaration.name == "globalThis.vibelangBridge.loadSynthdef"
                }));

                let manifest = build_v2(&discovery).unwrap();
                let accounting = semantic_join_accounting(
                    &contract_ids(&manifest),
                    &derive_semantic_requirements(&discovery).unwrap(),
                    &semantic_fragment_records(&discovery.fragments),
                );
                let debt = build_debt(&root(), &manifest, "fixture", &accounting).unwrap();
                let error_nodes = manifest
                    .types
                    .iter()
                    .filter(|api_type| {
                        api_type.metadata.source_anchors.iter().any(|anchor| {
                            anchor.path == WASM_TYPES_PATH
                                && (anchor.symbol == "VibelangError"
                                    || anchor.symbol.starts_with("VibelangError::"))
                        })
                    })
                    .map(|api_type| api_type.metadata.id.as_str())
                    .collect::<BTreeSet<_>>();
                assert_eq!(error_nodes.len(), 4);
                let error_debt = debt
                    .records
                    .iter()
                    .filter(|record| {
                        record.diagnostic_id == "compat.wasm.dead_generated_error_shape"
                    })
                    .collect::<Vec<_>>();
                assert_eq!(error_debt.len(), 4);
                assert!(error_debt.iter().all(|record| {
                    record.legacy_class == "dead"
                        && record
                            .node_id
                            .as_deref()
                            .is_some_and(|id| error_nodes.contains(id))
                }));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn independent_typescript_census_rejects_new_omitted_interfaces_and_members() {
        let source = r#"
export interface ArbitraryEnvelope {
  nonce: string;
  /** @deprecated compatibility field */
  payload?: Uint8Array;
  commit(value: number): Promise<void>;
}

export type ArbitraryUnion = string | number;
"#;
        let discovered = discover_generated_typescript_shapes(source, WASM_TYPES_PATH).unwrap();
        let names = discovered
            .iter()
            .map(|declaration| (declaration.kind.as_str(), declaration.name.as_str()))
            .collect::<BTreeSet<_>>();
        for expected in [
            ("interface", "ArbitraryEnvelope"),
            ("type_member", "ArbitraryEnvelope.nonce"),
            ("type_member", "ArbitraryEnvelope.payload"),
            ("type_member", "ArbitraryEnvelope.commit"),
            ("type_alias", "ArbitraryUnion"),
        ] {
            assert!(names.contains(&expected), "missing {expected:?}");
        }

        let omitted_interface = discovered
            .iter()
            .filter(|declaration| declaration.name != "ArbitraryEnvelope")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            missing_generated_typescript_shapes(source, &omitted_interface).unwrap(),
            ["wasm:interface:ArbitraryEnvelope"]
        );

        let omitted_member = discovered
            .iter()
            .filter(|declaration| declaration.name != "ArbitraryEnvelope.payload")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            missing_generated_typescript_shapes(source, &omitted_member).unwrap(),
            ["wasm:type_member:ArbitraryEnvelope.payload"]
        );
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
    fn persisted_denominators_accept_unchanged_and_growing_discovery() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let mut discovery = discover(&root()).unwrap();
                let manifest = build_v2(&discovery).unwrap();
                let expected = BTreeMap::from([
                    ("cli", 33),
                    ("docs", 14),
                    ("fixtures", 26),
                    ("http", 620),
                    ("manifest", 13_001),
                    ("packages", 21),
                    ("rhai_editor", 3_141),
                    ("wasm", 51),
                ]);
                for consumer in &manifest.consumers {
                    let family = consumer.eligibility.surfaces[0].as_str();
                    let coverage = &manifest.coverage[&consumer.metadata.id];
                    assert_eq!(coverage.base_denominator, expected[family], "{family}");
                    assert!(coverage.denominator >= expected[family], "{family}");
                }
                let current_manifest_denominator = manifest
                    .consumers
                    .iter()
                    .find(|consumer| consumer.eligibility.surfaces == ["manifest"])
                    .map(|consumer| manifest.coverage[&consumer.metadata.id].denominator)
                    .unwrap();

                discovery.mechanical.push(
                    classify_mechanical_declaration(raw_declaration(
                        "cli",
                        "command",
                        "m02-denominator-growth-probe",
                        "crates/vibelang-cli/src/main.rs",
                        "Commands::M02DenominatorGrowthProbe",
                    ))
                    .unwrap(),
                );
                discovery
                    .mechanical
                    .sort_by(|left, right| left.id.cmp(&right.id));
                let grown = build_v2(&discovery).unwrap();
                for (family, denominator, accepted) in [
                    ("cli", 34, 33),
                    ("manifest", current_manifest_denominator + 1, 13_001),
                ] {
                    let consumer = grown
                        .consumers
                        .iter()
                        .find(|consumer| consumer.eligibility.surfaces == [family])
                        .unwrap();
                    let coverage = &grown.coverage[&consumer.metadata.id];
                    assert_eq!(coverage.denominator, denominator, "{family}");
                    assert_eq!(coverage.base_denominator, accepted, "{family}");
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn wasm_and_cli_source_loss_fail_before_regeneration_writes() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let source_root = root();
                for (surface, name) in [("wasm", Some("VibelangError.stack")), ("cli", None)] {
                    let mut discovery = discover(&source_root).unwrap();
                    let accepted_denominator = discovery
                        .fragments
                        .consumers
                        .denominator_baseline
                        .consumers
                        .iter()
                        .find(|consumer| consumer.consumer == surface)
                        .unwrap()
                        .accepted_denominator
                        as usize;
                    let current_denominator = discovery
                        .mechanical
                        .iter()
                        .filter(|declaration| declaration.consumers.contains_key(surface))
                        .count();
                    let removal_count = current_denominator - accepted_denominator + 1;
                    let mut indexes = discovery
                        .mechanical
                        .iter()
                        .enumerate()
                        .filter(|(_, declaration)| {
                            declaration.surface == surface
                                && (surface != "wasm" || declaration.kind == "type_member")
                                && declaration.consumers.get(surface)
                                    == Some(&MechanicalDisposition::Included)
                        })
                        .map(|(index, declaration)| {
                            (
                                name.is_none_or(|name| declaration.name != name),
                                declaration.id.clone(),
                                index,
                            )
                        })
                        .collect::<Vec<_>>();
                    indexes.sort();
                    let mut indexes = indexes
                        .into_iter()
                        .take(removal_count)
                        .map(|(_, _, index)| index)
                        .collect::<Vec<_>>();
                    assert_eq!(indexes.len(), removal_count);
                    indexes.sort_unstable_by(|left, right| right.cmp(left));
                    for index in indexes {
                        discovery.mechanical.remove(index);
                    }

                    let output_root = source_root.join("target").join(format!(
                        "vibelang-m02-denominator-source-loss-{surface}-{}",
                        std::process::id()
                    ));
                    let sentinels = [
                        V2_PATH,
                        COVERAGE_PATH,
                        DEBT_PATH,
                        DIFF_PATH,
                        PACKAGE_INDEX_PATH,
                    ]
                    .into_iter()
                    .map(|path| (path, format!("sentinel:{surface}:{path}\n")))
                    .collect::<Vec<_>>();
                    for (path, content) in &sentinels {
                        let path = output_root.join(path);
                        fs::create_dir_all(path.parent().unwrap()).unwrap();
                        fs::write(path, content).unwrap();
                    }

                    let error = generate_discovered(&source_root, &output_root, &discovery, false)
                        .unwrap_err();
                    assert!(error.contains(surface), "{error}");
                    assert!(error.contains("before artifact writes"), "{error}");
                    for (path, content) in &sentinels {
                        assert_eq!(
                            fs::read_to_string(output_root.join(path)).unwrap(),
                            *content
                        );
                    }
                    fs::remove_dir_all(output_root).unwrap();
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn denominator_baseline_tampering_fails_closed() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let mut discovery = discover(&root()).unwrap();
                discovery
                    .fragments
                    .consumers
                    .denominator_baseline
                    .consumers
                    .iter_mut()
                    .find(|consumer| consumer.consumer == "wasm")
                    .unwrap()
                    .accepted_denominator = 50;
                let error = build_v2(&discovery).unwrap_err();
                assert!(error.contains("checksum mismatch"), "{error}");

                let baseline = &mut discovery.fragments.consumers.denominator_baseline;
                baseline.sha256 =
                    vibelang_api_manifest::fragments::consumer_denominator_baseline_sha256(
                        baseline,
                    )
                    .unwrap();
                let error = build_v2(&discovery).unwrap_err();
                assert!(error.contains("is not accepted"), "{error}");
                assert!(error.contains("separately audited update"), "{error}");
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

                let quantization_field_id = semantic_id(
                    "field",
                    "http|crates/vibelang-http/src/models.rs|TransportUpdate|quantization_beats",
                );
                fn quantization_record<'a>(
                    fragments: &'a mut FragmentSet,
                    target_id: &str,
                ) -> &'a mut vibelang_api_manifest::fragments::HttpRecord {
                    fragments
                        .http
                        .records
                        .iter_mut()
                        .find(|record| record.target_id == target_id)
                        .unwrap()
                }
                let mut mutant = discovery.fragments.clone();
                quantization_record(&mut mutant, &quantization_field_id).operation_id = None;
                assert_missing(&mutant, "OperationBinding");

                let mut mutant = discovery.fragments.clone();
                quantization_record(&mut mutant, &quantization_field_id).effectiveness = None;
                assert_missing(&mutant, "Effectiveness");

                let mut mutant = discovery.fragments.clone();
                mutant
                    .websocket
                    .records
                    .iter_mut()
                    .find(|record| record.target_id == semantic_id("event", "websocket|hello"))
                    .unwrap()
                    .event = None;
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
                assert_eq!(composition.accounting.semantic_records, 125);
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

                let receipt_updated = manifest
                    .events
                    .iter()
                    .find(|event| event.metadata.name == "receipt.updated")
                    .unwrap();
                assert_eq!(receipt_updated.ordering, EventOrdering::ObservationSequence);
                assert_eq!(
                    receipt_updated.revision_relation,
                    RevisionRelation::AcceptedRevision
                );
                assert_eq!(receipt_updated.delivery, EventDelivery::BestEffort);
                assert_eq!(receipt_updated.loss_detection, LossDetection::ResetRequired);
                assert_eq!(
                    receipt_updated.resync_operation_id,
                    Some(semantic_id("operation", "http|GET|/receipts/{attempt_id}"))
                );

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
