use proc_macro2::Span;
use quote::ToTokens;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::Visit;
use syn::{Expr, ExprBlock, ExprMethodCall, FnArg, GenericArgument, Item, ItemFn, Lit, Pat, Type};
use vibelang_api_manifest::{
    stable_id, to_pretty_json, Anchor, ApiEntry, Availability, EntryDetails, Lifecycle, Overload,
    Parameter, PublicApiManifest, UgenInput, SCHEMA_URI, SCHEMA_VERSION,
};
use walkdir::WalkDir;

#[path = "../../crates/vibelang-dsp/build_support.rs"]
mod dsp_build_support;
use dsp_build_support::UGenManifest;

const MANIFEST_PATH: &str = "api/public-api-manifest-v1.json";
const MANIFEST_TEST_SYMBOL: &str =
    "public_api::tests::generated_manifest_matches_committed_snapshot";

pub fn generate(root: &Path, check: bool) -> Result<(), String> {
    let manifest = build_manifest(root)?;
    let json = to_pretty_json(&manifest).map_err(|error| error.to_string())?;
    let path = root.join(MANIFEST_PATH);

    if check {
        let committed = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if committed != json {
            return Err(format!(
                "{} is stale; run `CARGO_BUILD_JOBS=1 cargo run -p xtask -- public-api generate`",
                MANIFEST_PATH
            ));
        }
        println!("{} is current", MANIFEST_PATH);
    } else {
        fs::write(&path, json)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        println!("generated {}", MANIFEST_PATH);
    }

    println!(
        "{} entries, {} overloads",
        manifest.entries.len(),
        manifest
            .entries
            .iter()
            .map(|entry| entry.overloads.len())
            .sum::<usize>()
    );
    Ok(())
}

fn build_manifest(root: &Path) -> Result<PublicApiManifest, String> {
    let source_index = SourceIndex::load(root)?;
    let ugen_catalog = UgenCatalog::load(root)?;
    let metadata_json = vibelang_rhai::public_api_metadata_json()?;
    let metadata: RhaiMetadata =
        serde_json::from_str(&metadata_json).map_err(|error| error.to_string())?;

    let mut entries = rhai_entries(&metadata, &source_index, &ugen_catalog, root)?;
    let type_entries = rhai_type_entries(&metadata, &source_index);
    validate_type_declaration_coverage(&type_entries, &source_index)?;
    entries.extend(type_entries);
    entries.extend(ugen_catalog.quarantined_entries());
    entries.extend(ugen_catalog.builder_entries(root));

    let stdlib = scan_stdlib(root)?;
    entries.extend(stdlib.entries);

    canonicalize_entries(&mut entries);
    validate_entries(&entries)?;

    let mut stats = BTreeMap::new();
    stats.insert(
        "effective_rhai_functions".into(),
        metadata.functions.len() as u64,
    );
    stats.insert(
        "effective_rhai_types".into(),
        metadata.custom_types.len() as u64,
    );
    stats.insert("manifest_entries".into(), entries.len() as u64);
    stats.insert(
        "manifest_overloads".into(),
        entries
            .iter()
            .map(|entry| entry.overloads.len() as u64)
            .sum(),
    );
    stats.insert(
        "manifest_ugen_callable_entries".into(),
        entries
            .iter()
            .filter(|entry| matches!(&entry.details, EntryDetails::Ugen { callable: true, .. }))
            .count() as u64,
    );
    stats.insert(
        "manifest_ugen_callable_overloads".into(),
        entries
            .iter()
            .filter(|entry| matches!(&entry.details, EntryDetails::Ugen { callable: true, .. }))
            .map(|entry| entry.overloads.len() as u64)
            .sum(),
    );
    stats.insert(
        "registration_declarations".into(),
        source_index.registrations.len() as u64,
    );
    stats.insert(
        "registered_type_declarations".into(),
        source_index.types.len() as u64,
    );
    stats.insert("stdlib_definition_occurrences".into(), stdlib.definitions);
    stats.insert("stdlib_files".into(), stdlib.files);
    stats.insert("stdlib_function_declarations".into(), stdlib.functions);
    stats.insert(
        "ugen_callable_records".into(),
        ugen_catalog.callable_records,
    );
    stats.insert("ugen_demand_names".into(), ugen_catalog.demand_names);
    stats.insert(
        "ugen_quarantined_names".into(),
        ugen_catalog.quarantined_names(),
    );
    stats.insert(
        "ugen_generated_names".into(),
        ugen_catalog.generated_names(),
    );
    stats.insert(
        "ugen_generated_overloads".into(),
        ugen_catalog.generated_overloads(),
    );
    stats.insert("ugen_records".into(), ugen_catalog.records.len() as u64);

    validate_baseline(&stats)?;

    Ok(PublicApiManifest {
        schema: SCHEMA_URI.into(),
        schema_version: SCHEMA_VERSION,
        api_version: workspace_api_version(root)?,
        entries,
        stats,
    })
}

fn validate_baseline(stats: &BTreeMap<String, u64>) -> Result<(), String> {
    let expected = [
        ("registration_declarations", 638),
        ("registered_type_declarations", 34),
        ("manifest_ugen_callable_entries", 1_174),
        ("manifest_ugen_callable_overloads", 5_962),
        ("stdlib_definition_occurrences", 890),
        ("stdlib_files", 829),
        ("stdlib_function_declarations", 707),
        ("ugen_callable_records", 802),
        ("ugen_demand_names", 25),
        ("ugen_quarantined_names", 25),
        ("ugen_generated_names", 1_174),
        ("ugen_generated_overloads", 5_962),
        ("ugen_records", 875),
    ];
    for (name, expected_value) in expected {
        let actual = stats.get(name).copied().unwrap_or_default();
        if actual != expected_value {
            return Err(format!(
                "baseline changed for {name}: expected {expected_value}, got {actual}"
            ));
        }
    }
    Ok(())
}

fn workspace_api_version(root: &Path) -> Result<String, String> {
    let cargo_toml = fs::read_to_string(root.join("crates/vibelang-rhai/Cargo.toml"))
        .map_err(|error| error.to_string())?;
    cargo_toml
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("version = \"")
                .and_then(|version| version.strip_suffix('"'))
                .map(str::to_owned)
        })
        .ok_or_else(|| "vibelang-rhai package version not found".into())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RhaiMetadata {
    #[serde(default)]
    custom_types: Vec<RhaiCustomType>,
    #[serde(default)]
    functions: Vec<RhaiFunction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RhaiCustomType {
    type_name: String,
    display_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RhaiFunction {
    name: String,
    #[serde(default)]
    this_type: Option<String>,
    num_params: usize,
    #[serde(default)]
    params: Vec<RhaiParameter>,
    #[serde(default)]
    return_type: String,
    signature: String,
}

#[derive(Debug, Deserialize)]
struct RhaiParameter {
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type", default)]
    parameter_type: Option<String>,
}

fn rhai_entries(
    metadata: &RhaiMetadata,
    sources: &SourceIndex,
    ugens: &UgenCatalog,
    root: &Path,
) -> Result<Vec<ApiEntry>, String> {
    let mut grouped: BTreeMap<(String, String, String, Option<String>), ApiEntry> = BTreeMap::new();

    for function in &metadata.functions {
        let (kind, registered_name) = if let Some(name) = function.name.strip_prefix("get$") {
            ("property_get", name)
        } else if let Some(name) = function.name.strip_prefix("set$") {
            ("property_set", name)
        } else {
            ("function", function.name.as_str())
        };
        let ugen = ugens
            .generated
            .get(registered_name)
            .filter(|ugen| ugen.matches_metadata(function));
        let source_matches = if ugen.is_some() {
            Vec::new()
        } else {
            sources.matches(function, registered_name, kind)
        };
        let receiver = infer_receiver(function, kind, &source_matches);
        let property_type_sources = if source_matches.is_empty() && kind.starts_with("property_") {
            receiver
                .as_deref()
                .map(|receiver| sources.types_for_receiver(receiver))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let surface = if ugen.is_some() {
            "dsp_ugen"
        } else {
            source_surface(&source_matches)
        };
        let key = (
            surface.into(),
            kind.into(),
            registered_name.into(),
            receiver.clone(),
        );

        let source_anchors =
            entry_source_anchors(&source_matches, &property_type_sources, ugen, root)?;
        let aliases = sources.aliases(registered_name, &source_matches);
        let details = if let Some(ugen) = ugen {
            ugen.details(true)
        } else {
            EntryDetails::Rhai {
                callable_identities: source_matches
                    .iter()
                    .map(|source| source.callable.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            }
        };

        let overload_availability = availability(&source_matches, &property_type_sources, ugen);
        let callable_identities = source_matches
            .iter()
            .map(|source| source.callable.clone())
            .collect::<BTreeSet<_>>();
        let entry = grouped.entry(key).or_insert_with(|| {
            let canonical = format!(
                "{surface}|{kind}|{registered_name}|{}",
                receiver.as_deref().unwrap_or("")
            );
            ApiEntry {
                id: stable_id("entry", &canonical),
                surface: surface.into(),
                kind: kind.into(),
                registered_name: registered_name.into(),
                aliases: aliases.clone(),
                receiver: receiver.clone(),
                overloads: Vec::new(),
                availability: overload_availability.clone(),
                lifecycle: Lifecycle::default(),
                source_anchors: source_anchors.clone(),
                test_anchors: test_anchors(surface),
                details,
            }
        });
        entry.aliases.extend(aliases.iter().cloned());
        entry.source_anchors.extend(source_anchors.iter().cloned());
        merge_availability(&mut entry.availability, &overload_availability);
        if let EntryDetails::Rhai {
            callable_identities: existing,
        } = &mut entry.details
        {
            existing.extend(callable_identities);
            existing.sort();
            existing.dedup();
        }

        let receiver_parameter = receiver.as_ref().and_then(|receiver| {
            function.params.first().and_then(|parameter| {
                parameter
                    .parameter_type
                    .as_deref()
                    .filter(|parameter_type| same_type(parameter_type, receiver))
                    .map(|_| 1usize)
            })
        });
        let parameters = function
            .params
            .iter()
            .skip(receiver_parameter.unwrap_or(0))
            .enumerate()
            .map(|(position, parameter)| Parameter {
                position: position as u32,
                name: parameter.name.clone(),
                accepted_types: vec![parameter
                    .parameter_type
                    .clone()
                    .unwrap_or_else(|| "Dynamic".into())],
                optional: false,
                default: None,
            })
            .collect();
        let canonical = format!("{}|{}", entry.id, function.signature);
        entry.overloads.push(Overload {
            id: stable_id("overload", &canonical),
            signature: function.signature.clone(),
            aliases,
            parameters,
            return_type: function.return_type.clone(),
            returns_receiver: receiver
                .as_ref()
                .map(|receiver| same_type(&function.return_type, receiver)),
            availability: overload_availability,
            source_anchors,
        });
    }

    let entries: Vec<_> = grouped.into_values().collect();
    validate_registration_coverage(&entries, sources)?;
    Ok(entries)
}

fn validate_registration_coverage(
    entries: &[ApiEntry],
    sources: &SourceIndex,
) -> Result<(), String> {
    let mut counts = BTreeMap::<Anchor, usize>::new();
    for entry in entries {
        for overload in &entry.overloads {
            for anchor in &overload.source_anchors {
                *counts.entry(anchor.clone()).or_default() += 1;
            }
        }
    }

    let mut failures = Vec::new();
    for source in &sources.registrations {
        let count = counts.get(&source.anchor).copied().unwrap_or_default();
        if count != 1 {
            failures.push(format!(
                "{}:{} {} `{}` represented {count} times",
                source.anchor.path,
                source.anchor.line.unwrap_or_default(),
                source.kind,
                source.name
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "registration declaration coverage failed:\n{}",
            failures.into_iter().take(20).collect::<Vec<_>>().join("\n")
        ))
    }
}

fn validate_type_declaration_coverage(
    entries: &[ApiEntry],
    sources: &SourceIndex,
) -> Result<(), String> {
    let counts = entries.iter().flat_map(|entry| &entry.source_anchors).fold(
        BTreeMap::<Anchor, usize>::new(),
        |mut counts, anchor| {
            *counts.entry(anchor.clone()).or_default() += 1;
            counts
        },
    );
    let failures = sources
        .types
        .iter()
        .filter_map(|source| {
            let count = counts.get(&source.anchor).copied().unwrap_or_default();
            (count != 1).then(|| {
                format!(
                    "{}:{} type `{}` represented {count} times",
                    source.anchor.path,
                    source.anchor.line.unwrap_or_default(),
                    source.rust_type
                )
            })
        })
        .collect::<Vec<_>>();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "registered type declaration coverage failed:\n{}",
            failures.into_iter().take(20).collect::<Vec<_>>().join("\n")
        ))
    }
}

fn infer_receiver(
    function: &RhaiFunction,
    kind: &str,
    sources: &[&SourceRegistration],
) -> Option<String> {
    if let Some(this_type) = &function.this_type {
        return Some(clean_type(this_type));
    }
    if kind.starts_with("property_") {
        return function
            .params
            .first()
            .and_then(|parameter| parameter.parameter_type.as_deref())
            .map(clean_type);
    }
    let first_type = function
        .params
        .first()
        .and_then(|parameter| parameter.parameter_type.as_deref());
    sources
        .iter()
        .filter_map(|source| source.receiver.as_deref())
        .find(|receiver| {
            first_type
                .map(|first_type| same_type(first_type, receiver))
                .unwrap_or(false)
        })
        .map(str::to_owned)
}

fn clean_type(value: &str) -> String {
    syn::parse_str::<Type>(value)
        .map(|value| canonical_syn_type(&value))
        .unwrap_or_else(|_| {
            value
                .trim()
                .trim_start_matches("&mut ")
                .trim_start_matches('&')
                .trim()
                .rsplit("::")
                .next()
                .unwrap_or(value)
                .to_owned()
        })
}

fn same_type(left: &str, right: &str) -> bool {
    clean_type(left) == clean_type(right)
}

fn canonical_syn_type(value: &Type) -> String {
    match value {
        Type::Reference(reference) => canonical_syn_type(&reference.elem),
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return value.to_token_stream().to_string().replace(' ', "");
            };
            let raw_identifier = segment.ident.to_string();
            let identifier = match raw_identifier.as_str() {
                "String" | "str" | "ImmutableString" => "string",
                "Array" | "Vec" => "array",
                "Map" | "HashMap" => "map",
                "FnPtr" => "Fn",
                "INT" => "i64",
                "FLOAT" => "f64",
                other => other,
            }
            .to_owned();
            let arguments = match &segment.arguments {
                syn::PathArguments::AngleBracketed(arguments) => arguments
                    .args
                    .iter()
                    .filter_map(|argument| match argument {
                        GenericArgument::Type(argument) => Some(canonical_syn_type(argument)),
                        GenericArgument::Const(argument) => {
                            Some(argument.to_token_stream().to_string().replace(' ', ""))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            if arguments.is_empty() {
                identifier
            } else {
                format!("{identifier}<{}>", arguments.join(","))
            }
        }
        Type::Tuple(tuple) => format!(
            "({})",
            tuple
                .elems
                .iter()
                .map(canonical_syn_type)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Type::Slice(slice) => format!("[{}]", canonical_syn_type(&slice.elem)),
        _ => value.to_token_stream().to_string().replace(' ', ""),
    }
}

fn source_match_score(source: &SourceRegistration, function: &RhaiFunction) -> Option<u32> {
    let parameters = source.parameters.as_ref()?;
    if parameters.len() != function.num_params || function.params.len() != function.num_params {
        return None;
    }

    let mut score = 1;
    for (index, (source_type, parameter)) in parameters.iter().zip(&function.params).enumerate() {
        if source_type == "_" {
            continue;
        }
        let metadata_type = parameter.parameter_type.as_deref().unwrap_or("Dynamic");
        if !same_type(source_type, metadata_type) {
            return None;
        }
        score += 4;
        if index == 0 && source.receiver.is_some() {
            score += 16;
        }
    }
    Some(score)
}

fn rhai_type_entries(metadata: &RhaiMetadata, sources: &SourceIndex) -> Vec<ApiEntry> {
    metadata
        .custom_types
        .iter()
        .map(|custom_type| {
            let source_matches = sources.type_matches(custom_type);
            let surface = source_type_surface(&source_matches);
            let canonical = format!(
                "{surface}|type|{}|{}",
                custom_type.display_name, custom_type.type_name
            );
            ApiEntry {
                id: stable_id("entry", &canonical),
                surface: surface.into(),
                kind: "type".into(),
                registered_name: custom_type.display_name.clone(),
                aliases: Vec::new(),
                receiver: None,
                overloads: Vec::new(),
                availability: availability_for_type(&source_matches),
                lifecycle: Lifecycle::default(),
                source_anchors: source_matches
                    .iter()
                    .map(|source| source.anchor.clone())
                    .collect(),
                test_anchors: test_anchors(surface),
                details: EntryDetails::RhaiType {
                    display_name: custom_type.display_name.clone(),
                },
            }
        })
        .collect()
}

fn source_surface(sources: &[&SourceRegistration]) -> &'static str {
    if sources
        .iter()
        .any(|source| source.anchor.path.contains("/extensions/"))
    {
        "rhai_extension"
    } else if !sources.is_empty()
        && sources
            .iter()
            .all(|source| source.anchor.path.starts_with("crates/vibelang-dsp/"))
    {
        "dsp_rhai"
    } else {
        "rhai"
    }
}

fn source_type_surface(sources: &[&SourceType]) -> &'static str {
    if !sources.is_empty()
        && sources
            .iter()
            .all(|source| source.anchor.path.starts_with("crates/vibelang-dsp/"))
    {
        "dsp_rhai"
    } else {
        "rhai"
    }
}

fn entry_source_anchors(
    sources: &[&SourceRegistration],
    property_types: &[&SourceType],
    ugen: Option<&GeneratedUgen>,
    root: &Path,
) -> Result<Vec<Anchor>, String> {
    let mut anchors: BTreeSet<Anchor> =
        sources.iter().map(|source| source.anchor.clone()).collect();
    anchors.extend(property_types.iter().map(|source| source.anchor.clone()));
    if let Some(ugen) = ugen {
        anchors.insert(ugen.anchor.clone());
        anchors.insert(build_registration_anchor(root)?);
    }
    Ok(anchors.into_iter().collect())
}

fn build_registration_anchor(root: &Path) -> Result<Anchor, String> {
    let path = "crates/vibelang-dsp/build.rs";
    let body = fs::read_to_string(root.join(path)).map_err(|error| error.to_string())?;
    let line = body
        .lines()
        .position(|line| line.contains("Generate registration function"))
        .map(|line| line as u32 + 1);
    Ok(Anchor {
        path: path.into(),
        symbol: "main::generated UGen registrations".into(),
        line,
    })
}

fn test_anchors(surface: &str) -> Vec<Anchor> {
    let mut anchors = vec![Anchor {
        path: "xtask/src/public_api.rs".into(),
        symbol: MANIFEST_TEST_SYMBOL.into(),
        line: None,
    }];
    if surface == "dsp_ugen" {
        anchors.push(Anchor {
            path: "crates/vibelang-dsp/tests/ugen_manifest_round_trip.rs".into(),
            symbol: "canonical UGen manifest round trip".into(),
            line: None,
        });
    }
    anchors
}

fn availability(
    sources: &[&SourceRegistration],
    property_types: &[&SourceType],
    ugen: Option<&GeneratedUgen>,
) -> Availability {
    let cfg: BTreeSet<String> = sources
        .iter()
        .flat_map(|source| source.cfg.iter().cloned())
        .chain(
            property_types
                .iter()
                .flat_map(|source| source.cfg.iter().cloned()),
        )
        .collect();
    let features = cfg_features(&cfg);
    let targets = cfg
        .iter()
        .filter(|condition| condition.contains("target_"))
        .cloned()
        .collect();
    let plugins: Vec<String> = ugen
        .and_then(|ugen| ugen.manifest.requires_plugin.clone())
        .into_iter()
        .collect();
    let runtime_conditions = if sources
        .iter()
        .any(|source| source.anchor.path.contains("/extensions/"))
    {
        vec!["extension enabled in ExtensionConfig".into()]
    } else {
        Vec::new()
    };
    Availability {
        status: if cfg.is_empty() && plugins.is_empty() && runtime_conditions.is_empty() {
            "available".into()
        } else {
            "conditional".into()
        },
        cfg: cfg.into_iter().collect(),
        targets,
        features,
        plugins,
        runtime_conditions,
    }
}

fn availability_for_type(sources: &[&SourceType]) -> Availability {
    let cfg: BTreeSet<String> = sources
        .iter()
        .flat_map(|source| source.cfg.iter().cloned())
        .collect();
    Availability {
        status: if cfg.is_empty() {
            "available".into()
        } else {
            "conditional".into()
        },
        features: cfg_features(&cfg),
        targets: cfg
            .iter()
            .filter(|condition| condition.contains("target_"))
            .cloned()
            .collect(),
        cfg: cfg.into_iter().collect(),
        plugins: Vec::new(),
        runtime_conditions: Vec::new(),
    }
}

fn cfg_features(cfg: &BTreeSet<String>) -> Vec<String> {
    cfg.iter()
        .flat_map(|condition| {
            let mut features = Vec::new();
            let mut remainder = condition.as_str();
            while let Some(index) = remainder.find("feature = \"") {
                remainder = &remainder[index + "feature = \"".len()..];
                if let Some(end) = remainder.find('"') {
                    features.push(remainder[..end].to_owned());
                    remainder = &remainder[end + 1..];
                } else {
                    break;
                }
            }
            features
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn merge_availability(current: &mut Availability, additional: &Availability) {
    if current.status != "available" && additional.status == "available" {
        current.status = "available".into();
    }
    merge_sorted(&mut current.cfg, &additional.cfg);
    merge_sorted(&mut current.targets, &additional.targets);
    merge_sorted(&mut current.features, &additional.features);
    merge_sorted(&mut current.plugins, &additional.plugins);
    merge_sorted(
        &mut current.runtime_conditions,
        &additional.runtime_conditions,
    );
}

fn merge_sorted(current: &mut Vec<String>, additional: &[String]) {
    current.extend(additional.iter().cloned());
    current.sort();
    current.dedup();
}

#[derive(Debug)]
struct SourceIndex {
    registrations: Vec<SourceRegistration>,
    types: Vec<SourceType>,
    aliases: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone)]
struct SourceRegistration {
    name: String,
    kind: String,
    callable: String,
    parameters: Option<Vec<String>>,
    receiver: Option<String>,
    anchor: Anchor,
    cfg: Vec<String>,
}

#[derive(Debug, Clone)]
struct SourceType {
    rust_type: String,
    anchor: Anchor,
    cfg: Vec<String>,
}

impl SourceIndex {
    fn load(root: &Path) -> Result<Self, String> {
        let mut paths = Vec::new();
        for directory in [
            root.join("crates/vibelang-rhai/src/api"),
            root.join("crates/vibelang-rhai/src/extensions"),
        ] {
            for entry in WalkDir::new(directory) {
                let entry = entry.map_err(|error| error.to_string())?;
                if entry.file_type().is_file()
                    && entry.path().extension().and_then(|value| value.to_str()) == Some("rs")
                {
                    paths.push(entry.path().to_path_buf());
                }
            }
        }
        paths.extend([
            root.join("crates/vibelang-dsp/src/api.rs"),
            root.join("crates/vibelang-dsp/src/helpers.rs"),
            root.join("crates/vibelang-dsp/src/rhainodes.rs"),
        ]);
        paths.sort();
        paths.dedup();

        let global_signatures = load_global_signatures(&paths)?;
        let mut registrations = Vec::new();
        let mut types = Vec::new();
        for path in paths {
            scan_registration_file(
                root,
                &path,
                &global_signatures,
                &mut registrations,
                &mut types,
            )?;
        }

        let mut identity_names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for registration in registrations
            .iter()
            .filter(|registration| registration.kind == "function")
        {
            identity_names
                .entry(registration.callable.clone())
                .or_default()
                .insert(registration.name.clone());
        }
        let mut aliases: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for names in identity_names.values().filter(|names| names.len() > 1) {
            for name in names {
                aliases
                    .entry(name.clone())
                    .or_default()
                    .extend(names.iter().filter(|alias| *alias != name).cloned());
            }
        }

        Ok(Self {
            registrations,
            types,
            aliases,
        })
    }

    fn matches(&self, function: &RhaiFunction, name: &str, kind: &str) -> Vec<&SourceRegistration> {
        let candidates: Vec<_> = self
            .registrations
            .iter()
            .filter(|source| source.name == name && source.kind == kind)
            .collect();
        let arity_matches: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|source| {
                source
                    .parameters
                    .as_ref()
                    .is_some_and(|parameters| parameters.len() == function.num_params)
            })
            .collect();
        let candidates = if arity_matches.is_empty() {
            candidates
        } else {
            arity_matches
        };

        let mut scored = candidates
            .into_iter()
            .filter_map(|source| source_match_score(source, function).map(|score| (score, source)))
            .collect::<Vec<_>>();
        let Some(best_score) = scored.iter().map(|(score, _)| *score).max() else {
            return Vec::new();
        };
        scored.retain(|(score, _)| *score == best_score);
        scored.into_iter().map(|(_, source)| source).collect()
    }

    fn aliases(&self, name: &str, sources: &[&SourceRegistration]) -> Vec<String> {
        let source_identities: BTreeSet<_> =
            sources.iter().map(|source| &source.callable).collect();
        self.aliases
            .get(name)
            .into_iter()
            .flatten()
            .filter(|alias| {
                self.registrations.iter().any(|registration| {
                    registration.name == **alias
                        && source_identities.contains(&registration.callable)
                })
            })
            .cloned()
            .collect()
    }

    fn type_matches(&self, custom_type: &RhaiCustomType) -> Vec<&SourceType> {
        self.types
            .iter()
            .filter(|source| {
                same_type(&source.rust_type, &custom_type.display_name)
                    || same_type(&source.rust_type, &custom_type.type_name)
            })
            .collect()
    }

    fn types_for_receiver(&self, receiver: &str) -> Vec<&SourceType> {
        self.types
            .iter()
            .filter(|source| same_type(&source.rust_type, receiver))
            .collect()
    }
}

fn scan_registration_file(
    root: &Path,
    path: &Path,
    global_signatures: &GlobalSignatures,
    registrations: &mut Vec<SourceRegistration>,
    types: &mut Vec<SourceType>,
) -> Result<(), String> {
    let body = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let file = syn::parse_file(&body)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    let relative = relative_path(root, path)?;
    let module_cfg = module_cfg(root, path)?;
    let signatures = SignatureIndex::new(&file, global_signatures);

    let mut registration_functions = Vec::new();
    collect_registration_functions(&file.items, &mut registration_functions);
    for function in registration_functions {
        let mut initial_cfg = module_cfg.clone();
        initial_cfg.extend(cfg_attrs(&function.attrs));
        let mut visitor = RegistrationVisitor {
            relative: &relative,
            function: function.sig.ident.to_string(),
            cfg_stack: initial_cfg,
            signatures: &signatures,
            registrations,
            types,
        };
        visitor.visit_block(&function.block);
    }
    Ok(())
}

fn collect_registration_functions<'a>(items: &'a [Item], output: &mut Vec<&'a ItemFn>) {
    for item in items {
        match item {
            Item::Fn(function)
                if function.sig.ident.to_string().starts_with("register")
                    && function.sig.inputs.iter().any(|input| {
                        matches!(input, FnArg::Typed(input) if input.ty.to_token_stream().to_string().contains("Engine"))
                    }) =>
            {
                output.push(function);
            }
            Item::Mod(module) => {
                if let Some((_, items)) = &module.content {
                    collect_registration_functions(items, output);
                }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct SignatureIndex {
    free: BTreeMap<String, CallableSignature>,
    methods: BTreeMap<(String, String), CallableSignature>,
}

#[derive(Clone, Eq, PartialEq)]
struct CallableSignature {
    parameters: Vec<String>,
    receiver: Option<String>,
}

impl SignatureIndex {
    fn new(file: &syn::File, global_signatures: &GlobalSignatures) -> Self {
        let mut index = Self {
            free: global_signatures.free.clone(),
            methods: global_signatures.methods.clone(),
        };
        for item in &file.items {
            match item {
                Item::Fn(function) => {
                    index.free.insert(
                        function.sig.ident.to_string(),
                        callable_signature(&function.sig, None),
                    );
                }
                Item::Impl(implementation) => {
                    let self_type =
                        clean_type(&implementation.self_ty.to_token_stream().to_string());
                    for item in &implementation.items {
                        if let syn::ImplItem::Fn(method) = item {
                            index.methods.insert(
                                (self_type.clone(), method.sig.ident.to_string()),
                                callable_signature(&method.sig, Some(&self_type)),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        index
    }

    fn callable(&self, expression: &Expr) -> Option<CallableSignature> {
        match expression {
            Expr::Closure(closure) => {
                let parameters = closure
                    .inputs
                    .iter()
                    .filter_map(|input| match input {
                        Pat::Type(input) => {
                            let parameter_type = input.ty.to_token_stream().to_string();
                            (!is_native_call_context(&parameter_type)).then_some(parameter_type)
                        }
                        _ => Some("_".into()),
                    })
                    .collect();
                let receiver = closure.inputs.iter().find_map(|input| match input {
                    Pat::Type(input)
                        if !is_native_call_context(&input.ty.to_token_stream().to_string()) =>
                    {
                        mutable_receiver_type(&input.ty)
                    }
                    _ => None,
                });
                Some(CallableSignature {
                    parameters,
                    receiver,
                })
            }
            Expr::Path(path) => {
                let segments: Vec<_> = path.path.segments.iter().collect();
                if segments.len() == 1 {
                    let name = segments[0].ident.to_string();
                    self.free.get(&name).cloned()
                } else {
                    let method = segments.last().unwrap().ident.to_string();
                    let receiver = segments[segments.len() - 2].ident.to_string();
                    self.methods.get(&(receiver, method)).cloned()
                }
            }
            _ => None,
        }
    }
}

struct GlobalSignatures {
    free: BTreeMap<String, CallableSignature>,
    methods: BTreeMap<(String, String), CallableSignature>,
}

fn load_global_signatures(paths: &[PathBuf]) -> Result<GlobalSignatures, String> {
    let mut free_candidates = BTreeMap::<String, Vec<CallableSignature>>::new();
    let mut methods = BTreeMap::new();
    for path in paths {
        let body = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let file = syn::parse_file(&body)
            .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
        for item in file.items {
            match item {
                Item::Fn(function) => {
                    let signature = callable_signature(&function.sig, None);
                    let candidates = free_candidates
                        .entry(function.sig.ident.to_string())
                        .or_default();
                    if !candidates.contains(&signature) {
                        candidates.push(signature);
                    }
                }
                Item::Impl(implementation) => {
                    let self_type =
                        clean_type(&implementation.self_ty.to_token_stream().to_string());
                    for item in implementation.items {
                        if let syn::ImplItem::Fn(method) = item {
                            methods.insert(
                                (self_type.clone(), method.sig.ident.to_string()),
                                callable_signature(&method.sig, Some(&self_type)),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let free = free_candidates
        .into_iter()
        .filter_map(|(name, mut candidates)| {
            (candidates.len() == 1).then(|| (name, candidates.pop().unwrap()))
        })
        .collect();
    Ok(GlobalSignatures { free, methods })
}

fn callable_signature(signature: &syn::Signature, self_type: Option<&str>) -> CallableSignature {
    let explicit_receiver = signature
        .inputs
        .iter()
        .any(|input| matches!(input, FnArg::Receiver(_)))
        .then(|| self_type.unwrap().to_owned());
    let parameters = signature
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Receiver(_) => explicit_receiver.clone(),
            FnArg::Typed(input) => {
                let mut parameter_type = input.ty.to_token_stream().to_string();
                if let Some(self_type) = self_type {
                    parameter_type = parameter_type.replace("Self", self_type);
                }
                (!is_native_call_context(&parameter_type)).then_some(parameter_type)
            }
        })
        .collect::<Vec<_>>();
    let receiver = explicit_receiver.or_else(|| {
        signature.inputs.iter().find_map(|input| match input {
            FnArg::Typed(input)
                if !is_native_call_context(&input.ty.to_token_stream().to_string()) =>
            {
                mutable_receiver_type(&input.ty)
            }
            _ => None,
        })
    });
    CallableSignature {
        parameters,
        receiver,
    }
}

fn mutable_receiver_type(value: &Type) -> Option<String> {
    match value {
        Type::Reference(reference) if reference.mutability.is_some() => {
            Some(canonical_syn_type(&reference.elem))
        }
        _ => None,
    }
}

fn is_native_call_context(parameter_type: &str) -> bool {
    clean_type(parameter_type) == "NativeCallContext"
}

struct RegistrationVisitor<'a> {
    relative: &'a str,
    function: String,
    cfg_stack: Vec<String>,
    signatures: &'a SignatureIndex,
    registrations: &'a mut Vec<SourceRegistration>,
    types: &'a mut Vec<SourceType>,
}

impl<'ast> Visit<'ast> for RegistrationVisitor<'_> {
    fn visit_expr_block(&mut self, node: &'ast ExprBlock) {
        let added = cfg_attrs(&node.attrs);
        self.cfg_stack.extend(added.iter().cloned());
        syn::visit::visit_expr_block(self, node);
        self.cfg_stack.truncate(self.cfg_stack.len() - added.len());
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let added = cfg_attrs(&node.attrs);
        self.cfg_stack.extend(added.iter().cloned());
        self.record_method_call(node);
        syn::visit::visit_expr_method_call(self, node);
        self.cfg_stack.truncate(self.cfg_stack.len() - added.len());
    }
}

impl RegistrationVisitor<'_> {
    fn record_method_call(&mut self, node: &ExprMethodCall) {
        let method = node.method.to_string();
        if method == "register_fn" || method == "register_get" {
            let mut arguments = node.args.iter();
            let Some(Expr::Lit(name)) = arguments.next() else {
                return;
            };
            let Lit::Str(name) = &name.lit else {
                return;
            };
            let Some(callable) = arguments.next() else {
                return;
            };
            let signature = self.signatures.callable(callable);
            self.registrations.push(SourceRegistration {
                name: name.value(),
                kind: if method == "register_get" {
                    "property_get".into()
                } else {
                    "function".into()
                },
                callable: callable.to_token_stream().to_string(),
                parameters: signature
                    .as_ref()
                    .map(|signature| signature.parameters.clone()),
                receiver: signature.and_then(|signature| signature.receiver),
                anchor: Anchor {
                    path: self.relative.into(),
                    symbol: format!("{}::{method}", self.function),
                    line: span_line(node.method.span()),
                },
                cfg: self
                    .cfg_stack
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            });
            return;
        }

        if method == "register_type" || method == "build_type" {
            let Some(turbofish) = &node.turbofish else {
                return;
            };
            let Some(GenericArgument::Type(rust_type)) = turbofish.args.first() else {
                return;
            };
            self.types.push(SourceType {
                rust_type: rust_type.to_token_stream().to_string(),
                anchor: Anchor {
                    path: self.relative.into(),
                    symbol: format!("{}::{method}", self.function),
                    line: span_line(node.method.span()),
                },
                cfg: self
                    .cfg_stack
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            });
        }
    }
}

fn span_line(span: Span) -> Option<u32> {
    let line = span.start().line;
    (line > 0).then_some(line as u32)
}

fn cfg_attrs(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg"))
        .map(|attribute| attribute.meta.to_token_stream().to_string())
        .collect()
}

fn module_cfg(root: &Path, path: &Path) -> Result<Vec<String>, String> {
    let src = if path.starts_with(root.join("crates/vibelang-rhai/src")) {
        root.join("crates/vibelang-rhai/src")
    } else {
        return Ok(Vec::new());
    };
    let relative = path.strip_prefix(&src).map_err(|error| error.to_string())?;
    let mut segments: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    if segments.last().is_some_and(|name| name == "mod.rs") {
        segments.pop();
    } else if let Some(last) = segments.last_mut() {
        *last = last.trim_end_matches(".rs").to_owned();
    }

    let mut cfg = Vec::new();
    let mut module_file = src.join("lib.rs");
    let mut module_dir = src.clone();
    for segment in segments {
        let body = fs::read_to_string(&module_file).map_err(|error| error.to_string())?;
        let file = syn::parse_file(&body).map_err(|error| error.to_string())?;
        if let Some(module) = file.items.iter().find_map(|item| match item {
            Item::Mod(module) if module.ident == segment => Some(module),
            _ => None,
        }) {
            cfg.extend(cfg_attrs(&module.attrs));
        }
        let direct = module_dir.join(format!("{segment}.rs"));
        let nested = module_dir.join(&segment).join("mod.rs");
        if direct.exists() {
            module_file = direct;
        } else {
            module_file = nested;
            module_dir = module_dir.join(segment);
        }
    }
    Ok(cfg
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn relative_path<'a>(root: &Path, path: &'a Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone)]
struct GeneratedUgen {
    manifest: UGenManifest,
    rate: String,
    anchor: Anchor,
}

impl GeneratedUgen {
    fn matches_metadata(&self, function: &RhaiFunction) -> bool {
        metadata_matches_generated_ugen(self.manifest.inputs.len(), function)
    }

    fn details(&self, callable: bool) -> EntryDetails {
        let quarantined = dsp_build_support::is_quarantined_rate(&self.rate);
        let runtime_rate = if quarantined {
            "unavailable"
        } else if self.rate == "builder" {
            // Preserve the v1 documentation-only snapshot until its schema is revised.
            "audio"
        } else {
            dsp_build_support::runtime_rate_manifest(&self.rate)
                .expect("generated UGen rate must have a runtime encoding")
        };
        EntryDetails::Ugen {
            class: self.manifest.name.clone(),
            description: self.manifest.description.clone(),
            rate: self.rate.clone(),
            runtime_rate: runtime_rate.into(),
            category: self.manifest.category.clone(),
            inputs: self
                .manifest
                .inputs
                .iter()
                .map(|input| UgenInput {
                    name: input.name.clone(),
                    input_type: input.ty.clone(),
                    default: input.default.clone(),
                    description: input.description.clone(),
                })
                .collect(),
            outputs: self.manifest.outputs,
            emitted_class: self
                .manifest
                .ugen_class
                .clone()
                .unwrap_or_else(|| self.manifest.name.clone()),
            special_index: self.manifest.special_index.unwrap_or(0),
            pseudo: self.manifest.pseudo,
            callable,
            requires_plugin: self.manifest.requires_plugin.clone(),
            unavailable_reason: if quarantined {
                Some(dsp_build_support::DEMAND_QUARANTINE_REASON.into())
            } else {
                self.manifest.unavailable_reason.clone()
            },
        }
    }
}

fn metadata_matches_generated_ugen(input_count: usize, function: &RhaiFunction) -> bool {
    if function.num_params == 0 {
        return true;
    }

    if dsp_build_support::has_array_overload(input_count)
        && function.num_params == 1
        && function.params.is_empty()
        && function.signature == format!("{}(_)", function.name)
    {
        return true;
    }

    function.num_params <= dsp_build_support::positional_arity_max(input_count)
        && function.params.len() == function.num_params
        && function.params.iter().all(|parameter| {
            parameter
                .parameter_type
                .as_deref()
                .map(|parameter_type| clean_type(parameter_type) == "Dynamic")
                .unwrap_or(false)
        })
}

#[derive(Debug)]
struct UgenCatalog {
    records: Vec<(UGenManifest, Anchor)>,
    generated: BTreeMap<String, GeneratedUgen>,
    quarantined: BTreeMap<String, GeneratedUgen>,
    callable_records: u64,
    demand_names: u64,
}

impl UgenCatalog {
    fn load(root: &Path) -> Result<Self, String> {
        let directory = root.join("crates/vibelang-dsp/ugen_manifests");
        let mut paths: Vec<PathBuf> = fs::read_dir(&directory)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .collect();
        paths.sort();

        let mut records = Vec::new();
        let mut generated = BTreeMap::new();
        let mut quarantined = BTreeMap::new();
        let mut callable_records = 0;
        let mut demand_names = BTreeSet::new();
        for path in paths {
            let body = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let parsed: Vec<UGenManifest> = serde_json::from_str(&body)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let relative = relative_path(root, &path)?;
            for manifest in parsed {
                let line = body
                    .lines()
                    .position(|line| line.contains(&format!("\"name\": \"{}\"", manifest.name)))
                    .map(|line| line as u32 + 1);
                let anchor = Anchor {
                    path: relative.clone(),
                    symbol: manifest.name.clone(),
                    line,
                };
                let callable = manifest
                    .rates
                    .iter()
                    .any(|rate| dsp_build_support::runtime_rate_manifest(rate).is_some());
                if callable {
                    callable_records += 1;
                }
                for rate in manifest
                    .rates
                    .iter()
                    .filter(|rate| rate.as_str() != "builder")
                {
                    let registered_name = format!(
                        "{}_{}",
                        dsp_build_support::to_snake_case(&manifest.name),
                        rate
                    );
                    if rate == "demand" {
                        demand_names.insert(registered_name.clone());
                    }
                    let generated_ugen = GeneratedUgen {
                        manifest: manifest.clone(),
                        rate: rate.clone(),
                        anchor: anchor.clone(),
                    };
                    let destination = if dsp_build_support::is_quarantined_rate(rate) {
                        &mut quarantined
                    } else if dsp_build_support::runtime_rate_manifest(rate).is_some() {
                        &mut generated
                    } else {
                        return Err(format!(
                            "UGen {} has unsupported runtime rate {rate}",
                            manifest.name
                        ));
                    };
                    if destination
                        .insert(registered_name.clone(), generated_ugen)
                        .is_some()
                    {
                        return Err(format!("duplicate generated UGen name {registered_name}"));
                    }
                }
                records.push((manifest, anchor));
            }
        }

        Ok(Self {
            records,
            generated,
            quarantined,
            callable_records,
            demand_names: demand_names.len() as u64,
        })
    }

    fn generated_names(&self) -> u64 {
        self.generated.len() as u64
    }

    fn generated_overloads(&self) -> u64 {
        self.generated
            .values()
            .map(|ugen| {
                let input_count = ugen.manifest.inputs.len();
                let positional = dsp_build_support::positional_arity_max(input_count) + 1;
                positional as u64 + u64::from(dsp_build_support::has_array_overload(input_count))
            })
            .sum()
    }

    fn quarantined_names(&self) -> u64 {
        self.quarantined.len() as u64
    }

    fn quarantined_entries(&self) -> Vec<ApiEntry> {
        self.quarantined
            .iter()
            .map(|(registered_name, ugen)| {
                let canonical = format!("dsp_ugen|ugen_quarantined|{registered_name}|");
                let availability = Availability {
                    status: "quarantined".into(),
                    cfg: Vec::new(),
                    targets: Vec::new(),
                    features: Vec::new(),
                    plugins: ugen.manifest.requires_plugin.clone().into_iter().collect(),
                    runtime_conditions: vec![dsp_build_support::DEMAND_QUARANTINE_REASON.into()],
                };
                ApiEntry {
                    id: stable_id("entry", &canonical),
                    surface: "dsp_ugen".into(),
                    kind: "ugen_quarantined".into(),
                    registered_name: registered_name.clone(),
                    aliases: Vec::new(),
                    receiver: None,
                    overloads: Vec::new(),
                    availability,
                    lifecycle: Lifecycle::default(),
                    source_anchors: vec![
                        Anchor {
                            path: "crates/vibelang-dsp/build_support.rs".into(),
                            symbol: "DEMAND_QUARANTINE_REASON".into(),
                            line: None,
                        },
                        ugen.anchor.clone(),
                    ],
                    test_anchors: test_anchors("dsp_ugen"),
                    details: ugen.details(false),
                }
            })
            .collect()
    }

    fn builder_entries(&self, _root: &Path) -> Vec<ApiEntry> {
        self.records
            .iter()
            .filter(|(manifest, _)| manifest.rates.iter().all(|rate| rate == "builder"))
            .map(|(manifest, anchor)| {
                let generated = GeneratedUgen {
                    manifest: manifest.clone(),
                    rate: "builder".into(),
                    anchor: anchor.clone(),
                };
                let registered_name = dsp_build_support::to_snake_case(&manifest.name);
                let canonical = format!("dsp_ugen|ugen_builder_model|{registered_name}|");
                ApiEntry {
                    id: stable_id("entry", &canonical),
                    surface: "dsp_ugen".into(),
                    kind: "ugen_builder_model".into(),
                    registered_name,
                    aliases: Vec::new(),
                    receiver: None,
                    overloads: Vec::new(),
                    availability: Availability {
                        status: "documentation_only".into(),
                        cfg: Vec::new(),
                        targets: Vec::new(),
                        features: Vec::new(),
                        plugins: manifest.requires_plugin.clone().into_iter().collect(),
                        runtime_conditions: Vec::new(),
                    },
                    lifecycle: Lifecycle::default(),
                    source_anchors: vec![anchor.clone()],
                    test_anchors: test_anchors("dsp_ugen"),
                    details: generated.details(false),
                }
            })
            .collect()
    }
}

struct StdlibScan {
    entries: Vec<ApiEntry>,
    files: u64,
    definitions: u64,
    functions: u64,
}

fn stdlib_availability() -> Availability {
    Availability {
        status: "importable".into(),
        cfg: Vec::new(),
        targets: Vec::new(),
        features: Vec::new(),
        plugins: Vec::new(),
        runtime_conditions: vec!["defining stdlib module imported".into()],
    }
}

fn scan_stdlib(root: &Path) -> Result<StdlibScan, String> {
    let stdlib_root = root.join("crates/vibelang-std/stdlib");
    let mut paths = Vec::new();
    for entry in WalkDir::new(&stdlib_root) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|value| value.to_str()) == Some("vibe")
        {
            paths.push(entry.path().to_path_buf());
        }
    }
    paths.sort();

    let mut engine = rhai::Engine::new_raw();
    engine.set_max_expr_depths(4096, 4096);
    engine.set_max_call_levels(4096);
    let mut definitions: BTreeMap<(String, String), Vec<(String, Anchor)>> = BTreeMap::new();
    let mut functions: BTreeMap<(String, Option<String>), StdlibFunctionGroup> = BTreeMap::new();
    let mut definition_count = 0;
    let mut function_count = 0;

    for path in &paths {
        let body = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let relative = relative_path(root, path)?;
        let import_path = stdlib_import_path(&stdlib_root, path)?;
        let tokens = lex_rhai(&body)?;

        for definition in scan_definitions(&tokens) {
            definition_count += 1;
            definitions
                .entry((definition.kind, definition.name.clone()))
                .or_default()
                .push((
                    import_path.clone(),
                    Anchor {
                        path: relative.clone(),
                        symbol: definition.name,
                        line: Some(definition.line),
                    },
                ));
        }

        let mut lines = scan_function_lines(&tokens);
        let ast = engine
            .compile(&body)
            .map_err(|error| format!("failed to compile {}: {error}", path.display()))?;
        for function in ast.iter_functions() {
            if function.name.starts_with("anon$") {
                continue;
            }
            function_count += 1;
            let signature = function.to_string();
            let queue = lines
                .get_mut(&(function.name.to_owned(), function.params.len()))
                .ok_or_else(|| {
                    format!(
                        "source anchor not found for {} in {}",
                        signature,
                        path.display()
                    )
                })?;
            let line = queue.pop_front().ok_or_else(|| {
                format!(
                    "source anchor exhausted for {} in {}",
                    signature,
                    path.display()
                )
            })?;
            let receiver = function.this_type.map(str::to_owned);
            let group = functions
                .entry((function.name.to_owned(), receiver.clone()))
                .or_insert_with(|| StdlibFunctionGroup {
                    name: function.name.to_owned(),
                    receiver,
                    overloads: BTreeMap::new(),
                    import_paths: BTreeSet::new(),
                    access: BTreeSet::new(),
                    documentation: BTreeSet::new(),
                    anchors: BTreeSet::new(),
                });
            let anchor = Anchor {
                path: relative.clone(),
                symbol: signature.clone(),
                line: Some(line),
            };
            group.import_paths.insert(import_path.clone());
            group
                .access
                .insert(format!("{:?}", function.access).to_lowercase());
            group
                .documentation
                .extend(function.comments.iter().map(|comment| comment.to_string()));
            group.anchors.insert(anchor.clone());
            group
                .overloads
                .entry(signature.clone())
                .or_insert_with(|| Overload {
                    id: String::new(),
                    signature,
                    aliases: Vec::new(),
                    parameters: function
                        .params
                        .iter()
                        .enumerate()
                        .map(|(position, name)| Parameter {
                            position: position as u32,
                            name: Some((*name).into()),
                            accepted_types: vec!["Dynamic".into()],
                            optional: false,
                            default: None,
                        })
                        .collect(),
                    return_type: "Dynamic".into(),
                    returns_receiver: None,
                    availability: stdlib_availability(),
                    source_anchors: Vec::new(),
                })
                .source_anchors
                .push(anchor);
        }
        if lines.values().any(|queue| !queue.is_empty()) {
            return Err(format!(
                "Rhai AST did not expose every function declaration in {}",
                path.display()
            ));
        }
    }

    let mut entries = Vec::new();
    for ((kind, name), occurrences) in definitions {
        let anchors: Vec<_> = occurrences
            .iter()
            .map(|(_, anchor)| anchor.clone())
            .collect();
        let import_paths: Vec<_> = occurrences
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let canonical = format!("stdlib|{kind}|{name}|");
        let entry_id = stable_id("entry", &canonical);
        entries.push(ApiEntry {
            id: entry_id.clone(),
            surface: "stdlib".into(),
            kind: kind.clone(),
            registered_name: name.clone(),
            aliases: Vec::new(),
            receiver: None,
            overloads: vec![Overload {
                id: stable_id("overload", &format!("{entry_id}|{name}()")),
                signature: format!("{name}()"),
                aliases: Vec::new(),
                parameters: Vec::new(),
                return_type: "definition".into(),
                returns_receiver: None,
                availability: stdlib_availability(),
                source_anchors: anchors.clone(),
            }],
            availability: stdlib_availability(),
            lifecycle: Lifecycle::default(),
            source_anchors: anchors.clone(),
            test_anchors: test_anchors("stdlib"),
            details: EntryDetails::StdlibDefinition {
                definition_kind: kind,
                import_paths,
                occurrences: anchors,
                export_classification: "unknown".into(),
                support_classification: "unknown".into(),
            },
        });
    }
    for (_, group) in functions {
        entries.push(group.into_entry());
    }

    Ok(StdlibScan {
        entries,
        files: paths.len() as u64,
        definitions: definition_count,
        functions: function_count,
    })
}

struct StdlibFunctionGroup {
    name: String,
    receiver: Option<String>,
    overloads: BTreeMap<String, Overload>,
    import_paths: BTreeSet<String>,
    access: BTreeSet<String>,
    documentation: BTreeSet<String>,
    anchors: BTreeSet<Anchor>,
}

impl StdlibFunctionGroup {
    fn into_entry(mut self) -> ApiEntry {
        let canonical = format!(
            "stdlib|script_function|{}|{}",
            self.name,
            self.receiver.as_deref().unwrap_or("")
        );
        let entry_id = stable_id("entry", &canonical);
        for overload in self.overloads.values_mut() {
            overload.id = stable_id("overload", &format!("{}|{}", entry_id, overload.signature));
        }
        ApiEntry {
            id: entry_id,
            surface: "stdlib".into(),
            kind: "script_function".into(),
            registered_name: self.name,
            aliases: Vec::new(),
            receiver: self.receiver,
            overloads: self.overloads.into_values().collect(),
            availability: stdlib_availability(),
            lifecycle: Lifecycle::default(),
            source_anchors: self.anchors.into_iter().collect(),
            test_anchors: test_anchors("stdlib"),
            details: EntryDetails::StdlibFunction {
                import_paths: self.import_paths.into_iter().collect(),
                access: self.access.into_iter().collect::<Vec<_>>().join("|"),
                documentation: self.documentation.into_iter().collect(),
                export_classification: "unknown".into(),
                support_classification: "unknown".into(),
            },
        }
    }
}

fn stdlib_import_path(stdlib_root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(stdlib_root)
        .map_err(|error| error.to_string())?;
    let mut import = relative.to_string_lossy().replace('\\', "/");
    if let Some(without_extension) = import.strip_suffix(".vibe") {
        import = without_extension.into();
    }
    Ok(format!("stdlib/{import}"))
}

#[derive(Clone)]
enum RhaiTokenKind {
    Ident(String),
    String(String),
    Symbol(char),
}

#[derive(Clone)]
struct RhaiToken {
    kind: RhaiTokenKind,
    line: u32,
}

fn lex_rhai(source: &str) -> Result<Vec<RhaiToken>, String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1u32;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                line += 1;
                index += 1;
            }
            byte if byte.is_ascii_whitespace() => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                let mut depth = 1;
                while index < bytes.len() && depth > 0 {
                    if bytes[index] == b'\n' {
                        line += 1;
                        index += 1;
                    } else if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                        depth += 1;
                        index += 2;
                    } else if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                        depth -= 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                if depth != 0 {
                    return Err("unterminated block comment in stdlib source".into());
                }
            }
            b'"' => {
                let token_line = line;
                index += 1;
                let mut value = String::new();
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' if index + 1 < bytes.len() => {
                            value.push(bytes[index + 1] as char);
                            index += 2;
                        }
                        b'"' => {
                            index += 1;
                            break;
                        }
                        b'\n' => {
                            line += 1;
                            value.push('\n');
                            index += 1;
                        }
                        byte => {
                            value.push(byte as char);
                            index += 1;
                        }
                    }
                }
                tokens.push(RhaiToken {
                    kind: RhaiTokenKind::String(value),
                    line: token_line,
                });
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                tokens.push(RhaiToken {
                    kind: RhaiTokenKind::Ident(source[start..index].into()),
                    line,
                });
            }
            byte => {
                tokens.push(RhaiToken {
                    kind: RhaiTokenKind::Symbol(byte as char),
                    line,
                });
                index += 1;
            }
        }
    }
    Ok(tokens)
}

struct DefinitionOccurrence {
    kind: String,
    name: String,
    line: u32,
}

fn scan_definitions(tokens: &[RhaiToken]) -> Vec<DefinitionOccurrence> {
    let mut definitions = Vec::new();
    for window in tokens.windows(3) {
        let RhaiTokenKind::Ident(function) = &window[0].kind else {
            continue;
        };
        if function != "define_synthdef" && function != "define_fx" {
            continue;
        }
        if !matches!(window[1].kind, RhaiTokenKind::Symbol('(')) {
            continue;
        }
        let RhaiTokenKind::String(name) = &window[2].kind else {
            continue;
        };
        definitions.push(DefinitionOccurrence {
            kind: if function == "define_synthdef" {
                "synthdef".into()
            } else {
                "effect".into()
            },
            name: name.clone(),
            line: window[0].line,
        });
    }
    definitions
}

fn scan_function_lines(tokens: &[RhaiToken]) -> BTreeMap<(String, usize), VecDeque<u32>> {
    let mut lines: BTreeMap<(String, usize), VecDeque<u32>> = BTreeMap::new();
    let mut index = 0;
    while index + 2 < tokens.len() {
        let RhaiTokenKind::Ident(keyword) = &tokens[index].kind else {
            index += 1;
            continue;
        };
        if keyword != "fn" {
            index += 1;
            continue;
        }
        let RhaiTokenKind::Ident(name) = &tokens[index + 1].kind else {
            index += 1;
            continue;
        };
        if !matches!(tokens[index + 2].kind, RhaiTokenKind::Symbol('(')) {
            index += 1;
            continue;
        }
        let mut cursor = index + 3;
        let mut depth = 1usize;
        let mut arity = 0usize;
        let mut has_parameter = false;
        while cursor < tokens.len() && depth > 0 {
            match &tokens[cursor].kind {
                RhaiTokenKind::Symbol('(') => depth += 1,
                RhaiTokenKind::Symbol(')') => depth -= 1,
                RhaiTokenKind::Symbol(',') if depth == 1 => arity += 1,
                RhaiTokenKind::Ident(_) if depth == 1 => has_parameter = true,
                _ => {}
            }
            cursor += 1;
        }
        if has_parameter {
            arity += 1;
        }
        lines
            .entry((name.clone(), arity))
            .or_default()
            .push_back(tokens[index].line);
        index = cursor;
    }
    lines
}

fn canonicalize_entries(entries: &mut Vec<ApiEntry>) {
    for entry in entries.iter_mut() {
        entry.aliases.sort();
        entry.aliases.dedup();
        entry.source_anchors.sort();
        entry.source_anchors.dedup();
        entry.test_anchors.sort();
        entry.test_anchors.dedup();
        entry
            .overloads
            .sort_by(|left, right| (&left.signature, &left.id).cmp(&(&right.signature, &right.id)));
        for overload in &mut entry.overloads {
            overload.aliases.sort();
            overload.aliases.dedup();
            overload.source_anchors.sort();
            overload.source_anchors.dedup();
        }
    }
    entries.sort_by(|left, right| {
        (
            &left.surface,
            &left.kind,
            &left.registered_name,
            &left.receiver,
            &left.id,
        )
            .cmp(&(
                &right.surface,
                &right.kind,
                &right.registered_name,
                &right.receiver,
                &right.id,
            ))
    });
}

fn validate_entries(entries: &[ApiEntry]) -> Result<(), String> {
    let mut entry_ids = BTreeSet::new();
    let mut overload_ids = BTreeSet::new();
    for entry in entries {
        if !entry_ids.insert(&entry.id) {
            return Err(format!("duplicate entry id {}", entry.id));
        }
        if entry.source_anchors.is_empty() {
            return Err(format!(
                "{} {} {} has no source anchor",
                entry.id, entry.surface, entry.registered_name
            ));
        }
        if entry.test_anchors.is_empty() {
            return Err(format!("{} has no test anchor", entry.id));
        }
        for anchor in entry.source_anchors.iter().chain(&entry.test_anchors) {
            validate_anchor(anchor)?;
        }
        for overload in &entry.overloads {
            if !overload_ids.insert(&overload.id) {
                return Err(format!("duplicate overload id {}", overload.id));
            }
            if overload.source_anchors.is_empty() {
                return Err(format!("{} has no source anchor", overload.id));
            }
            for anchor in &overload.source_anchors {
                validate_anchor(anchor)?;
            }
        }
    }
    Ok(())
}

fn validate_anchor(anchor: &Anchor) -> Result<(), String> {
    if Path::new(&anchor.path).is_absolute()
        || anchor.path.contains('\\')
        || anchor.path.split('/').any(|component| component == "..")
    {
        Err(format!(
            "anchor path is not repository-relative: {}",
            anchor.path
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUARANTINED_DEMAND_FUNCTIONS: &[&str] = &[
        "d_noise_ring_demand",
        "dbrown2_demand",
        "dbrown_demand",
        "dbufrd_demand",
        "dbufwr_demand",
        "dconst_demand",
        "ddup_demand",
        "deta_blocker_buf_demand",
        "dgauss_demand",
        "dgeom_demand",
        "dibrown_demand",
        "diwhite_demand",
        "dpoll_demand",
        "drand_demand",
        "dreset_demand",
        "dseq_demand",
        "dser_demand",
        "dseries_demand",
        "dshuf_demand",
        "dstutter_demand",
        "dswitch1_demand",
        "dswitch_demand",
        "dwhite_demand",
        "dwrand_demand",
        "dxrand_demand",
    ];

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn generated_manifest_matches_committed_snapshot() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let root = root();
                let manifest = build_manifest(&root).unwrap();
                assert_eq!(manifest.schema_version, 1);
                let global_group = manifest
                    .entries
                    .iter()
                    .find(|entry| {
                        entry.surface == "rhai"
                            && entry.registered_name == "group"
                            && entry.receiver.is_none()
                    })
                    .unwrap();
                assert_eq!(global_group.availability.status, "available");
                assert!(global_group.availability.features.is_empty());
                assert_eq!(global_group.overloads.len(), 1);
                assert_eq!(
                    global_group.overloads[0].source_anchors[0].path,
                    "crates/vibelang-rhai/src/api/group.rs"
                );
                let midi_group = manifest
                    .entries
                    .iter()
                    .find(|entry| {
                        entry.registered_name == "group"
                            && entry.receiver.as_deref() == Some("MidiDevice")
                    })
                    .unwrap();
                assert_eq!(midi_group.availability.status, "conditional");
                assert_eq!(midi_group.availability.features, ["midi"]);
                let generated = to_pretty_json(&manifest).unwrap();
                assert_eq!(generated, to_pretty_json(&manifest).unwrap());
                let committed = fs::read_to_string(root.join(MANIFEST_PATH)).unwrap();
                assert_eq!(generated, committed);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn demand_registrations_are_quarantined() {
        let catalog = UgenCatalog::load(&root()).unwrap();
        let demand_entries = catalog.quarantined_entries();

        assert_eq!(catalog.demand_names, 25);
        assert_eq!(catalog.quarantined_names(), 25);
        assert!(catalog.generated.values().all(|ugen| ugen.rate != "demand"));
        assert_eq!(demand_entries.len(), 25);
        assert_eq!(
            demand_entries
                .iter()
                .map(|entry| entry.registered_name.as_str())
                .collect::<Vec<_>>(),
            QUARANTINED_DEMAND_FUNCTIONS
        );
        assert!(demand_entries.iter().all(|entry| {
            entry.kind == "ugen_quarantined"
                && entry.overloads.is_empty()
                && entry.availability.status == "quarantined"
                && matches!(
                    &entry.details,
                    EntryDetails::Ugen {
                        runtime_rate,
                        callable: false,
                        unavailable_reason: Some(reason),
                        ..
                    } if runtime_rate == "unavailable"
                        && reason == dsp_build_support::DEMAND_QUARANTINE_REASON
                )
        }));
    }

    #[test]
    fn generated_ugen_matching_excludes_handwritten_name_collisions() {
        let generated = RhaiFunction {
            name: "in_ar".into(),
            this_type: None,
            num_params: 2,
            params: vec!["types::dynamic::Dynamic", "types::dynamic::Dynamic"]
                .into_iter()
                .map(|parameter_type| RhaiParameter {
                    name: None,
                    parameter_type: Some(parameter_type.into()),
                })
                .collect(),
            return_type: "NodeRef".into(),
            signature: "in_ar(_: Dynamic, _: Dynamic) -> NodeRef".into(),
        };
        assert!(metadata_matches_generated_ugen(2, &generated));

        let handwritten = RhaiFunction {
            params: vec!["f64", "i64"]
                .into_iter()
                .map(|parameter_type| RhaiParameter {
                    name: None,
                    parameter_type: Some(parameter_type.into()),
                })
                .collect(),
            ..generated
        };

        assert!(!metadata_matches_generated_ugen(2, &handwritten));
    }

    #[test]
    fn lexical_function_anchors_ignore_comments_and_strings() {
        let tokens = lex_rhai(
            r#"// fn fake(a)
               let text = "fn also_fake(a)";
               fn real(a, b) { a + b }
            "#,
        )
        .unwrap();
        let lines = scan_function_lines(&tokens);
        assert_eq!(lines.get(&("real".into(), 2)).unwrap().len(), 1);
        assert!(!lines.keys().any(|(name, _)| name.contains("fake")));
    }

    #[test]
    fn rust_and_rhai_signature_types_have_one_canonical_form() {
        assert!(same_type("String", "string"));
        assert!(same_type("rhai::FnPtr", "Fn"));
        assert!(same_type(
            "Vec<vibelang_dsp::rhainodes::NodeRef>",
            "alloc::vec::Vec<vibelang_dsp::rhainodes::NodeRef>"
        ));
        assert!(is_native_call_context("rhai::NativeCallContext"));
    }
}
