use quote::ToTokens;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use syn::{Expr, FnArg, ImplItem, Item, ReturnType, Type, Visibility};
use vibelang_api_manifest::{
    ApiEntry, Availability, EntryDetails, Lifecycle, PublicApiManifest, StdlibDeclaration,
    UgenInput,
};
use walkdir::WalkDir;

const CLI_HELP_PATH: &str = "docs/reference/generated/cli-help.txt";
const UGEN_REFERENCE_PATH: &str = "docs/reference/generated/ugens.md";
const WASM_TYPES_PATH: &str = "crates/vibelang-wasm/types/index.d.ts";
const EDITOR_RHAI_PATH: &str = "vscode-extension/src/data/rhai-api.json";
const LSP_RHAI_PATH: &str = "crates/vibelang-lsp/src/data/rhai-api.json";
const EDITOR_STDLIB_PATH: &str = "vscode-extension/src/data/stdlib.json";
const HTTP_SNAPSHOT_PATH: &str = "api/http-api-snapshot-v1.json";
const HTTP_REFERENCE_PATH: &str = "docs/reference/generated/http-routes.md";
const MANIFEST_PATH: &str = "api/public-api-manifest-v1.json";
const UGEN_CANONICAL_DIR: &str = "crates/vibelang-dsp/ugen_manifests";
const UGEN_PROJECTION_DIRS: &[&str] = &[
    "crates/vibelang-lsp/src/data/ugen_manifests",
    "vscode-extension/ugen_manifests",
];

pub fn generate(root: &Path, cli_help_path: &Path, check: bool) -> Result<(), String> {
    let manifest: PublicApiManifest = serde_json::from_str(
        &fs::read_to_string(root.join(MANIFEST_PATH)).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    validate_manifest_availability(&manifest)?;
    validate_example_imports(root)?;
    validate_editor_consumers(root, &manifest)?;

    let cli_help = normalize_cli_help(
        &fs::read_to_string(cli_help_path)
            .map_err(|error| format!("failed to read {}: {error}", cli_help_path.display()))?,
    );
    let http = build_http_snapshot(root)?;
    let artifacts = [
        (CLI_HELP_PATH, cli_help),
        (UGEN_REFERENCE_PATH, render_ugen_reference(&manifest)?),
        (WASM_TYPES_PATH, render_wasm_types(root)?),
        (EDITOR_RHAI_PATH, render_editor_rhai(&manifest)?),
        (LSP_RHAI_PATH, render_editor_rhai(&manifest)?),
        (EDITOR_STDLIB_PATH, render_editor_stdlib(root, &manifest)?),
        (HTTP_SNAPSHOT_PATH, pretty_json(&http)?),
        (HTTP_REFERENCE_PATH, render_http_reference(&http)),
    ];
    for (path, content) in artifacts {
        write_or_check(root, path, &content, check)?;
    }
    sync_ugen_projections(root, check)?;

    println!(
        "intentional exclusion: wasm-bindgen start hooks are module lifecycle functions rather than callable JS exports, while private InitOutput ABI fields and generated JS/wasm binaries are build outputs; types/index.d.ts regenerates the callable annotated Rust export surface and retains only the stable module-init compatibility shim"
    );
    Ok(())
}

fn write_or_check(root: &Path, relative: &str, generated: &str, check: bool) -> Result<(), String> {
    let path = root.join(relative);
    if check {
        let committed = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if committed != generated {
            return Err(format!(
                "{relative} is stale; run `scripts/public-artifacts.sh generate`"
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

fn pretty_json<T: Serialize>(value: &T) -> Result<String, String> {
    let mut json = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    json.push('\n');
    Ok(json)
}

fn normalize_cli_help(value: &str) -> String {
    let mut normalized = value.replace("\r\n", "\n");
    while normalized.ends_with("\n\n") {
        normalized.pop();
    }
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn validate_manifest_availability(manifest: &PublicApiManifest) -> Result<(), String> {
    for entry in &manifest.entries {
        validate_availability(&entry.id, &entry.availability)?;
        for overload in &entry.overloads {
            validate_availability(&overload.id, &overload.availability)?;
        }
        if matches!(
            &entry.details,
            EntryDetails::Ugen {
                callable: false,
                ..
            }
        ) && !entry.overloads.is_empty()
        {
            return Err(format!(
                "non-callable UGen {} exposes overloads",
                entry.registered_name
            ));
        }
    }
    Ok(())
}

fn validate_availability(id: &str, availability: &Availability) -> Result<(), String> {
    const STATUSES: &[&str] = &[
        "available",
        "conditional",
        "importable",
        "quarantined",
        "documentation_only",
    ];
    if !STATUSES.contains(&availability.status.as_str()) {
        return Err(format!(
            "{id} has unknown availability status `{}`",
            availability.status
        ));
    }
    if availability.status == "conditional"
        && availability.cfg.is_empty()
        && availability.targets.is_empty()
        && availability.features.is_empty()
        && availability.plugins.is_empty()
        && availability.runtime_conditions.is_empty()
    {
        return Err(format!("{id} is conditional without a condition"));
    }
    if availability.status == "quarantined" && availability.runtime_conditions.is_empty() {
        return Err(format!("{id} is quarantined without a reason"));
    }
    Ok(())
}

fn render_ugen_reference(manifest: &PublicApiManifest) -> Result<String, String> {
    let mut by_file: BTreeMap<String, Vec<&ApiEntry>> = BTreeMap::new();
    for entry in manifest
        .entries
        .iter()
        .filter(|entry| matches!(entry.details, EntryDetails::Ugen { .. }))
    {
        let source = entry
            .source_anchors
            .iter()
            .find(|anchor| anchor.path.contains("/ugen_manifests/"))
            .ok_or_else(|| format!("UGen {} has no manifest anchor", entry.registered_name))?;
        by_file.entry(source.path.clone()).or_default().push(entry);
    }

    let callable = manifest
        .entries
        .iter()
        .filter(|entry| matches!(entry.details, EntryDetails::Ugen { callable: true, .. }))
        .count();
    let quarantined = manifest
        .entries
        .iter()
        .filter(|entry| entry.availability.status == "quarantined")
        .count();
    let builders = manifest
        .entries
        .iter()
        .filter(|entry| entry.kind == "ugen_builder_model")
        .count();

    let mut output = String::new();
    output.push_str("# Generated UGen function index\n\n");
    output.push_str(
        "> Generated from `api/public-api-manifest-v1.json`; edit canonical DSP manifests and regenerate instead of editing this file.\n\n",
    );
    output.push_str(&format!(
        "This index contains **{callable} runtime-callable identities**, **{quarantined} quarantined identities**, and **{builders} documentation-only builder records**. Availability is copied from the registration manifest, so an unregistered demand identity cannot appear as callable.\n\n"
    ));
    output.push_str("| Availability | Meaning |\n|---|---|\n");
    output.push_str("| `available` | Registered without a host feature condition; backend plugins may still be required |\n");
    output.push_str("| `conditional` | Registration or execution depends on the listed feature/target/plugin condition |\n");
    output.push_str(
        "| `quarantined` | Canonical source record retained but no runtime overload registered |\n",
    );
    output.push_str(
        "| `documentation_only` | Builder model, not a generated rate-suffixed callable |\n",
    );

    for (path, mut entries) in by_file {
        entries.sort_by(|left, right| {
            let left_key = ugen_sort_key(left);
            let right_key = ugen_sort_key(right);
            left_key.cmp(&right_key)
        });
        let file = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&path);
        output.push_str(&format!(
            "\n## `{file}`\n\nSource: [`{path}`](../../../{path})\n\n"
        ));
        output.push_str(
            "| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |\n",
        );
        output.push_str("|---|---|---|---|---:|---|\n");
        for entry in entries {
            let EntryDetails::Ugen {
                class,
                rate,
                runtime_rate,
                inputs,
                outputs,
                requires_plugin,
                unavailable_reason,
                ..
            } = &entry.details
            else {
                unreachable!()
            };
            output.push_str(&format!(
                "| `{}` | `{}` | `{}` / `{}` | {} | {} | {} |\n",
                markdown(class),
                markdown(&entry.registered_name),
                markdown(rate),
                markdown(runtime_rate),
                render_ugen_inputs(inputs),
                outputs,
                render_availability(
                    &entry.availability,
                    requires_plugin.as_deref(),
                    unavailable_reason.as_deref()
                ),
            ));
        }
    }
    Ok(output)
}

fn ugen_sort_key(entry: &ApiEntry) -> (String, String) {
    let class = match &entry.details {
        EntryDetails::Ugen { class, .. } => class.clone(),
        _ => String::new(),
    };
    (class, entry.registered_name.clone())
}

fn render_ugen_inputs(inputs: &[UgenInput]) -> String {
    if inputs.is_empty() {
        return "none".into();
    }
    inputs
        .iter()
        .map(|input| {
            let default = input
                .default
                .as_ref()
                .map(serde_json::Value::to_string)
                .unwrap_or_else(|| "0".into());
            format!(
                "`{}` (`{}`; default `{}`)",
                markdown(&input.name),
                markdown(&input.input_type),
                markdown(&default)
            )
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

fn render_availability(
    availability: &Availability,
    plugin: Option<&str>,
    unavailable_reason: Option<&str>,
) -> String {
    let mut details = Vec::new();
    details.extend(availability.cfg.iter().cloned());
    details.extend(availability.targets.iter().cloned());
    details.extend(availability.features.iter().cloned());
    details.extend(availability.runtime_conditions.iter().cloned());
    if let Some(plugin) = plugin {
        details.push(format!("plugin: {plugin}"));
    }
    if let Some(reason) = unavailable_reason {
        details.push(reason.into());
    }
    if details.is_empty() {
        format!("`{}`", markdown(&availability.status))
    } else {
        format!(
            "`{}` — {}",
            markdown(&availability.status),
            markdown(&details.join("; "))
        )
    }
}

fn markdown(value: &str) -> String {
    value.replace('|', "\\|").replace('`', "\\`")
}

#[derive(Serialize)]
struct EditorRhaiEntry<'a> {
    name: &'a str,
    description: String,
    signature: &'a str,
    example: &'static str,
    receiver: Option<&'a str>,
    lifecycle: &'a Lifecycle,
    availability: &'a Availability,
}

fn render_editor_rhai(manifest: &PublicApiManifest) -> Result<String, String> {
    let mut rows = Vec::new();
    for entry in manifest.entries.iter().filter(|entry| {
        matches!(
            entry.surface.as_str(),
            "rhai" | "dsp_rhai" | "rhai_extension"
        ) && entry.kind == "function"
            && !matches!(
                entry.availability.status.as_str(),
                "quarantined" | "documentation_only"
            )
    }) {
        for overload in &entry.overloads {
            let receiver = entry
                .receiver
                .as_deref()
                .map(|receiver| format!(" method on {receiver}"))
                .unwrap_or_default();
            rows.push(EditorRhaiEntry {
                name: &entry.registered_name,
                description: format!(
                    "Source-backed {}{} registration; availability: {}.",
                    entry.surface, receiver, overload.availability.status
                ),
                signature: &overload.signature,
                example: "",
                receiver: entry.receiver.as_deref(),
                lifecycle: &entry.lifecycle,
                availability: &overload.availability,
            });
        }
    }
    pretty_json(&rows)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EditorStdlib<'a> {
    version: String,
    synthdefs: Vec<EditorStdlibEntry<'a>>,
    categories: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EditorStdlibEntry<'a> {
    name: &'a str,
    #[serde(rename = "type")]
    item_type: &'static str,
    description: String,
    category: String,
    subcategory: Option<String>,
    import_path: &'a str,
    source_path: String,
    params: Vec<serde_json::Value>,
    availability: &'a Availability,
}

fn render_editor_stdlib(root: &Path, manifest: &PublicApiManifest) -> Result<String, String> {
    let mut rows = Vec::new();
    let mut categories = BTreeSet::new();
    for entry in &manifest.entries {
        let EntryDetails::StdlibDefinition {
            definition_kind,
            declarations,
            ..
        } = &entry.details
        else {
            continue;
        };
        for declaration in declarations {
            let (category, subcategory) = stdlib_categories(&declaration.import_path);
            categories.insert(category.clone());
            rows.push(EditorStdlibEntry {
                name: &entry.registered_name,
                item_type: if definition_kind == "effect" {
                    "effect"
                } else {
                    "instrument"
                },
                description: source_description(root, declaration),
                category,
                subcategory,
                import_path: &declaration.import_path,
                source_path: declaration
                    .import_path
                    .strip_prefix("stdlib/")
                    .unwrap_or(&declaration.import_path)
                    .into(),
                params: Vec::new(),
                availability: &entry.availability,
            });
        }
    }
    let output = EditorStdlib {
        version: format!(
            "public-api-manifest-v{}:{}",
            manifest.schema_version, manifest.api_version
        ),
        synthdefs: rows,
        categories: categories.into_iter().collect(),
    };
    pretty_json(&output)
}

fn stdlib_categories(import_path: &str) -> (String, Option<String>) {
    let mut parts = import_path
        .strip_prefix("stdlib/")
        .unwrap_or(import_path)
        .split('/');
    let category = parts.next().unwrap_or("stdlib").to_string();
    let subcategory = parts.next().and_then(|part| {
        if part.ends_with(".vibe") {
            None
        } else {
            Some(part.to_string())
        }
    });
    (category, subcategory)
}

fn source_description(root: &Path, declaration: &StdlibDeclaration) -> String {
    fs::read_to_string(root.join(&declaration.source_anchor.path))
        .ok()
        .and_then(|source| {
            source.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("//")
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
            })
        })
        .unwrap_or_else(|| {
            format!(
                "{} standard-library definition",
                declaration.definition_kind
            )
        })
}

#[derive(Debug)]
struct WasmClass {
    name: String,
    methods: Vec<WasmMethod>,
}

#[derive(Debug)]
struct WasmMethod {
    name: String,
    constructor: bool,
    is_static: bool,
    is_async: bool,
    parameters: Vec<(String, String)>,
    return_type: String,
}

fn render_wasm_types(root: &Path) -> Result<String, String> {
    let source = fs::read_to_string(root.join("crates/vibelang-wasm/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let file = syn::parse_file(&source).map_err(|error| error.to_string())?;
    let interfaces = wasm_value_interfaces(&file)?;
    let (classes, functions) = wasm_exports(&file)?;

    let mut output = String::new();
    output.push_str(
        "/* Generated by scripts/public-artifacts.sh from crates/vibelang-wasm/src/lib.rs. */\n",
    );
    output.push_str("/* wasm-bindgen start hooks, raw JS/wasm, and private InitOutput ABI fields are intentionally excluded. */\n\n");
    output.push_str(&interfaces);
    output.push_str("export interface VibelangError {\n  message: string;\n  name?: string;\n  stack?: string;\n}\n\n");
    output.push_str("export interface VibelangBridge {\n  loadSynthdef(name: string, data: Uint8Array): Promise<unknown>;\n}\n\n");
    output.push_str(
        "declare global {\n  interface Window {\n    vibelangBridge?: VibelangBridge;\n  }\n}\n\n",
    );
    for class in classes {
        output.push_str(&format!("export class {} {{\n", class.name));
        output.push_str("  free(): void;\n  [Symbol.dispose](): void;\n");
        for method in class.methods {
            if method.constructor {
                output.push_str(&format!(
                    "  constructor({});\n",
                    render_ts_parameters(&method.parameters)
                ));
                continue;
            }
            let prefix = if method.is_static { "static " } else { "" };
            let mut return_type = wasm_method_return(&class.name, &method);
            if method.is_async {
                return_type = format!("Promise<{return_type}>");
            }
            output.push_str(&format!(
                "  {prefix}{}({}): {};\n",
                method.name,
                render_ts_parameters(&method.parameters),
                return_type
            ));
        }
        output.push_str("}\n\n");
    }
    for function in functions {
        output.push_str(&format!(
            "export function {}({}): {};\n",
            function.name,
            render_ts_parameters(&function.parameters),
            wasm_method_return("", &function)
        ));
    }
    output.push_str("\nexport type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;\n");
    output.push_str("export type SyncInitInput = BufferSource | WebAssembly.Module;\n");
    output.push_str("export interface InitOutput {\n  readonly memory: WebAssembly.Memory;\n  readonly [exportName: string]: unknown;\n}\n");
    output.push_str("export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;\n");
    output.push_str("export default function __wbg_init(module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;\n");
    Ok(output)
}

fn wasm_value_interfaces(file: &syn::File) -> Result<String, String> {
    let names = [
        ("ExecutionResult", "VibelangResult"),
        ("CompiledSynthdef", "VibelangCompiledSynthdef"),
    ];
    let mut output = String::new();
    for (rust_name, ts_name) in names {
        let item = file
            .items
            .iter()
            .find_map(|item| match item {
                Item::Struct(item) if item.ident == rust_name => Some(item),
                _ => None,
            })
            .ok_or_else(|| format!("missing WASM value struct {rust_name}"))?;
        output.push_str(&format!("export interface {ts_name} {{\n"));
        for field in &item.fields {
            let name = field
                .ident
                .as_ref()
                .ok_or_else(|| format!("{rust_name} has unnamed field"))?;
            let ty = if rust_name == "CompiledSynthdef" && name == "data" {
                "Uint8Array | number[]".into()
            } else {
                rust_type_to_ts(&field.ty)
            };
            output.push_str(&format!("  {name}: {ty};\n"));
        }
        output.push_str("}\n\n");
    }
    Ok(output)
}

fn wasm_exports(file: &syn::File) -> Result<(Vec<WasmClass>, Vec<WasmMethod>), String> {
    let mut classes: BTreeMap<String, Vec<WasmMethod>> = BTreeMap::new();
    let mut functions = Vec::new();
    for item in &file.items {
        match item {
            Item::Impl(item) if has_wasm_bindgen(&item.attrs) => {
                let Type::Path(path) = item.self_ty.as_ref() else {
                    return Err("wasm_bindgen impl has non-path self type".into());
                };
                let class = path
                    .path
                    .segments
                    .last()
                    .ok_or_else(|| "wasm_bindgen impl has empty path".to_string())?
                    .ident
                    .to_string();
                let methods = classes.entry(class).or_default();
                for impl_item in &item.items {
                    let ImplItem::Fn(function) = impl_item else {
                        continue;
                    };
                    if !matches!(function.vis, Visibility::Public(_)) {
                        continue;
                    }
                    methods.push(parse_wasm_signature(
                        &function.sig,
                        &function.attrs,
                        function.sig.receiver().is_none(),
                    )?);
                }
            }
            Item::Fn(function)
                if has_wasm_bindgen(&function.attrs)
                    && matches!(function.vis, Visibility::Public(_)) =>
            {
                if has_wasm_bindgen_flag(&function.attrs, "start")? {
                    continue;
                }
                functions.push(parse_wasm_signature(&function.sig, &function.attrs, true)?);
            }
            _ => {}
        }
    }
    let classes = classes
        .into_iter()
        .map(|(name, methods)| WasmClass { name, methods })
        .collect();
    Ok((classes, functions))
}

fn parse_wasm_signature(
    signature: &syn::Signature,
    attrs: &[syn::Attribute],
    is_static: bool,
) -> Result<WasmMethod, String> {
    let attributes = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("wasm_bindgen"))
        .map(|attr| attr.meta.to_token_stream().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let constructor = attributes.contains("constructor");
    let name =
        wasm_js_name(&attributes).unwrap_or_else(|| to_lower_camel(&signature.ident.to_string()));
    let mut parameters = Vec::new();
    for input in &signature.inputs {
        let FnArg::Typed(argument) = input else {
            continue;
        };
        let name = match argument.pat.as_ref() {
            syn::Pat::Ident(ident) => ident.ident.to_string(),
            _ => "value".into(),
        };
        parameters.push((name, rust_type_to_ts(&argument.ty)));
    }
    let return_type = match &signature.output {
        ReturnType::Default => "void".into(),
        ReturnType::Type(_, ty) => rust_type_to_ts(ty),
    };
    Ok(WasmMethod {
        name,
        constructor,
        is_static,
        is_async: signature.asyncness.is_some(),
        parameters,
        return_type,
    })
}

fn wasm_js_name(attributes: &str) -> Option<String> {
    let marker = "js_name =";
    let start = attributes.find(marker)? + marker.len();
    let value = attributes[start..].trim_start();
    Some(
        value
            .split(|character: char| character == ',' || character == ')')
            .next()?
            .trim()
            .trim_matches('"')
            .to_string(),
    )
}

fn has_wasm_bindgen(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("wasm_bindgen"))
}

fn has_wasm_bindgen_flag(attrs: &[syn::Attribute], flag: &str) -> Result<bool, String> {
    for attr in attrs
        .iter()
        .filter(|attr| attr.path().is_ident("wasm_bindgen"))
    {
        let syn::Meta::List(_) = &attr.meta else {
            continue;
        };
        let arguments = attr
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .map_err(|error| format!("invalid wasm_bindgen attribute: {error}"))?;
        if arguments
            .iter()
            .any(|argument| matches!(argument, syn::Meta::Path(path) if path.is_ident(flag)))
        {
            return Ok(true);
        }
    }
    Ok(false)
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

fn rust_type_to_ts(ty: &Type) -> String {
    match ty {
        Type::Reference(reference) => rust_type_to_ts(&reference.elem),
        Type::Tuple(tuple) if tuple.elems.is_empty() => "void".into(),
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return "unknown".into();
            };
            let name = segment.ident.to_string();
            match name.as_str() {
                "String" | "str" => "string".into(),
                "bool" => "boolean".into(),
                "f32" | "f64" | "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32"
                | "u64" | "usize" => "number".into(),
                "JsValue" => "unknown".into(),
                "Option" => {
                    let inner = first_type_argument(segment)
                        .map(rust_type_to_ts)
                        .unwrap_or_else(|| "unknown".into());
                    format!("{inner} | null")
                }
                "Vec" => {
                    let inner = first_type_argument(segment)
                        .map(rust_type_to_ts)
                        .unwrap_or_else(|| "unknown".into());
                    format!("{inner}[]")
                }
                "Result" => first_type_argument(segment)
                    .map(rust_type_to_ts)
                    .unwrap_or_else(|| "void".into()),
                _ => name,
            }
        }
        _ => "unknown".into(),
    }
}

fn first_type_argument(segment: &syn::PathSegment) -> Option<&Type> {
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    arguments.args.iter().find_map(|argument| match argument {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })
}

fn wasm_method_return(_class: &str, method: &WasmMethod) -> String {
    match method.name.as_str() {
        "execute" => "VibelangResult".into(),
        "getSynthdefs" | "getSystemSynthdefs" => "VibelangCompiledSynthdef[]".into(),
        _ => method.return_type.clone(),
    }
}

fn render_ts_parameters(parameters: &[(String, String)]) -> String {
    parameters
        .iter()
        .map(|(name, ty)| format!("{name}: {ty}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Serialize)]
struct HttpSnapshot {
    schema: &'static str,
    schema_version: u32,
    routes: Vec<HttpRoute>,
    types: Vec<HttpType>,
}

#[derive(Debug, Serialize)]
struct HttpRoute {
    method: String,
    path: String,
    handler: String,
    availability: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HttpType {
    name: String,
    kind: String,
    source: String,
    derives: Vec<String>,
    availability: Vec<String>,
    fields: Vec<HttpField>,
    variants: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HttpField {
    name: String,
    rust_type: String,
    serde: Vec<String>,
}

fn build_http_snapshot(root: &Path) -> Result<HttpSnapshot, String> {
    let lib_path = root.join("crates/vibelang-http/src/lib.rs");
    let source = fs::read_to_string(&lib_path).map_err(|error| error.to_string())?;
    let cfg_regions = cfg_route_regions(&source)?;
    let mut routes = extract_routes(&source, &cfg_regions)?;
    let handler_names = http_handler_names(root)?;
    for route in &routes {
        let name = route.handler.rsplit("::").next().unwrap_or(&route.handler);
        if !handler_names.contains(name) {
            return Err(format!(
                "route {} {} names missing handler {}",
                route.method, route.path, route.handler
            ));
        }
    }
    routes.sort_by(|left, right| {
        (&left.path, &left.method, &left.handler).cmp(&(&right.path, &right.method, &right.handler))
    });
    let mut seen = BTreeSet::new();
    for route in &routes {
        if !seen.insert((route.method.clone(), route.path.clone())) {
            return Err(format!(
                "duplicate route registration {} {}",
                route.method, route.path
            ));
        }
    }
    Ok(HttpSnapshot {
        schema: "https://vibelang.org/schemas/http-source-snapshot/v1",
        schema_version: 1,
        routes,
        types: extract_http_types(root)?,
    })
}

fn cfg_route_regions(source: &str) -> Result<Vec<(usize, usize, String)>, String> {
    let mut regions = Vec::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("#[cfg(") {
        let start = offset + relative;
        let cfg_end = matching_delimiter(source, start + "#[cfg".len(), '(', ')')?;
        let after = &source[cfg_end + 1..];
        let Some(let_relative) = after.find("let app = app") else {
            offset = cfg_end + 1;
            continue;
        };
        if let_relative > 80 {
            offset = cfg_end + 1;
            continue;
        }
        let chain_start = cfg_end + 1 + let_relative;
        let end = source[chain_start..]
            .find(';')
            .map(|relative| chain_start + relative)
            .ok_or_else(|| "cfg-gated route chain has no terminator".to_string())?;
        let cfg = source[start + "#[cfg(".len()..cfg_end]
            .trim_end_matches(']')
            .trim()
            .to_string();
        regions.push((chain_start, end, cfg));
        offset = end + 1;
    }
    Ok(regions)
}

fn extract_routes(
    source: &str,
    cfg_regions: &[(usize, usize, String)],
) -> Result<Vec<HttpRoute>, String> {
    let mut routes = Vec::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find(".route(") {
        let start = offset + relative;
        let open = start + ".route".len();
        let close = matching_delimiter(source, open, '(', ')')?;
        let content = &source[open + 1..close];
        let expression: syn::ExprTuple = syn::parse_str(&format!("({content})"))
            .map_err(|error| format!("failed to parse route `{content}`: {error}"))?;
        if expression.elems.len() != 2 {
            return Err(format!("route must have two arguments: {content}"));
        }
        let path = match &expression.elems[0] {
            Expr::Lit(literal) => match &literal.lit {
                syn::Lit::Str(path) => path.value(),
                _ => return Err("route path is not a string".into()),
            },
            _ => return Err("route path is not a literal".into()),
        };
        let (method, handler) = match &expression.elems[1] {
            Expr::Call(call) => {
                let Expr::Path(method) = call.func.as_ref() else {
                    return Err(format!("route method is not a path: {content}"));
                };
                let method = method
                    .path
                    .segments
                    .last()
                    .ok_or_else(|| "empty route method".to_string())?
                    .ident
                    .to_string()
                    .to_uppercase();
                let handler = call
                    .args
                    .first()
                    .ok_or_else(|| "route has no handler".to_string())?
                    .to_token_stream()
                    .to_string()
                    .replace(' ', "");
                (method, handler)
            }
            _ => return Err(format!("route method is not a call: {content}")),
        };
        let availability = cfg_regions
            .iter()
            .filter(|(region_start, region_end, _)| start >= *region_start && start <= *region_end)
            .map(|(_, _, cfg)| cfg.clone())
            .collect();
        routes.push(HttpRoute {
            method,
            path,
            handler,
            availability,
        });
        offset = close + 1;
    }
    Ok(routes)
}

fn matching_delimiter(
    source: &str,
    open: usize,
    open_char: char,
    close_char: char,
) -> Result<usize, String> {
    let mut depth = 0usize;
    let mut string = false;
    let mut escaped = false;
    for (relative, character) in source[open..].char_indices() {
        if string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                string = false;
            }
            continue;
        }
        if character == '"' {
            string = true;
        } else if character == open_char {
            depth += 1;
        } else if character == close_char {
            depth = depth
                .checked_sub(1)
                .ok_or_else(|| "unbalanced delimiter".to_string())?;
            if depth == 0 {
                return Ok(open + relative);
            }
        }
    }
    Err("unterminated delimiter".into())
}

fn http_handler_names(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    for path in rust_files(&root.join("crates/vibelang-http/src"))? {
        let file = syn::parse_file(&fs::read_to_string(&path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        for item in file.items {
            if let Item::Fn(function) = item {
                names.insert(function.sig.ident.to_string());
            }
        }
    }
    Ok(names)
}

fn extract_http_types(root: &Path) -> Result<Vec<HttpType>, String> {
    let source_root = root.join("crates/vibelang-http/src");
    let mut types = Vec::new();
    for path in rust_files(&source_root)? {
        let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let file =
            syn::parse_file(&source).map_err(|error| format!("{}: {error}", path.display()))?;
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        for item in file.items {
            match item {
                Item::Struct(item)
                    if matches!(item.vis, Visibility::Public(_))
                        && has_serde_derive(&item.attrs) =>
                {
                    let fields = item
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(index, field)| HttpField {
                            name: field
                                .ident
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_else(|| index.to_string()),
                            rust_type: field.ty.to_token_stream().to_string(),
                            serde: serde_attrs(&field.attrs),
                        })
                        .collect();
                    types.push(HttpType {
                        name: item.ident.to_string(),
                        kind: "struct".into(),
                        source: relative.clone(),
                        derives: derive_names(&item.attrs),
                        availability: cfg_attrs(&item.attrs),
                        fields,
                        variants: Vec::new(),
                    });
                }
                Item::Enum(item)
                    if matches!(item.vis, Visibility::Public(_))
                        && has_serde_derive(&item.attrs) =>
                {
                    types.push(HttpType {
                        name: item.ident.to_string(),
                        kind: "enum".into(),
                        source: relative.clone(),
                        derives: derive_names(&item.attrs),
                        availability: cfg_attrs(&item.attrs),
                        fields: Vec::new(),
                        variants: item
                            .variants
                            .iter()
                            .map(|variant| variant.ident.to_string())
                            .collect(),
                    });
                }
                _ => {}
            }
        }
    }
    types.sort_by(|left, right| (&left.source, &left.name).cmp(&(&right.source, &right.name)));
    Ok(types)
}

fn rust_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = WalkDir::new(directory)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("rs")
        })
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn has_serde_derive(attrs: &[syn::Attribute]) -> bool {
    let derives = derive_names(attrs);
    derives
        .iter()
        .any(|name| name == "Serialize" || name == "Deserialize")
}

fn derive_names(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut names = BTreeSet::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("derive")) {
        let tokens = attr.meta.to_token_stream().to_string();
        for candidate in ["Serialize", "Deserialize"] {
            if tokens
                .split(|character: char| !character.is_alphanumeric() && character != '_')
                .any(|part| part == candidate)
            {
                names.insert(candidate.into());
            }
        }
    }
    names.into_iter().collect()
}

fn serde_attrs(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("serde"))
        .map(|attr| attr.meta.to_token_stream().to_string())
        .collect()
}

fn cfg_attrs(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .map(|attr| attr.meta.to_token_stream().to_string())
        .collect()
}

fn render_http_reference(snapshot: &HttpSnapshot) -> String {
    let mut output = String::new();
    output.push_str("# Generated HTTP route and schema index\n\n");
    output.push_str(
        "> Generated from `api/http-api-snapshot-v1.json`; edit Axum routes or Serde DTOs and regenerate instead of editing this file.\n\n",
    );
    output.push_str(&format!(
        "The source snapshot contains **{} method/path registrations** and **{} public serialized/deserialized Rust types**. It records declarations and feature gates, not handler effectiveness or runtime status semantics.\n\n",
        snapshot.routes.len(),
        snapshot.types.len()
    ));
    output.push_str("## Routes\n\n| Method | Path | Handler | Availability |\n|---|---|---|---|\n");
    for route in &snapshot.routes {
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} |\n",
            route.method,
            route.path,
            route.handler,
            if route.availability.is_empty() {
                "always".into()
            } else {
                route
                    .availability
                    .iter()
                    .map(|cfg| format!("`{}`", markdown(cfg)))
                    .collect::<Vec<_>>()
                    .join("<br>")
            }
        ));
    }
    output
        .push_str("\n## Serde types\n\n| Type | Kind | Source | Direction |\n|---|---|---|---|\n");
    for ty in &snapshot.types {
        output.push_str(&format!(
            "| `{}` | `{}` | [`{}`](../../../{}) | `{}` |\n",
            ty.name,
            ty.kind,
            ty.source,
            ty.source,
            ty.derives.join(" + ")
        ));
    }
    output
}

fn sync_ugen_projections(root: &Path, check: bool) -> Result<(), String> {
    let canonical = root.join(UGEN_CANONICAL_DIR);
    for relative_directory in UGEN_PROJECTION_DIRS {
        let directory = root.join(relative_directory);
        let mut names = fs::read_dir(&directory)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            })
            .map(|entry| entry.file_name())
            .collect::<Vec<_>>();
        names.sort();
        for name in &names {
            let source = canonical.join(&name);
            let target = directory.join(&name);
            if !source.is_file() {
                return Err(format!(
                    "stale editor UGen projection {} has no canonical source",
                    target.display()
                ));
            }
            let generated = fs::read(&source).map_err(|error| error.to_string())?;
            if check {
                let committed = fs::read(&target).map_err(|error| error.to_string())?;
                if committed != generated {
                    return Err(format!(
                        "{} is stale; run `scripts/public-artifacts.sh generate`",
                        target.strip_prefix(root).unwrap_or(&target).display()
                    ));
                }
            } else {
                fs::write(&target, generated).map_err(|error| error.to_string())?;
            }
        }
        println!(
            "{} UGen projection files are current; unbundled canonical categories remain an intentional editor packaging exclusion",
            names.len()
        );
    }
    Ok(())
}

fn validate_example_imports(root: &Path) -> Result<(), String> {
    const FICTIONAL_CALLS: &[&str] = &[
        "load_sample",
        "at_bar",
        "every",
        "after",
        "fade_in",
        "fade_out",
        "midi_out",
        "midi_in",
        "export_audio",
    ];
    let examples = root.join("examples");
    let mut checked = 0usize;
    for entry in WalkDir::new(&examples) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("vibe")
        {
            continue;
        }
        checked += 1;
        let source = fs::read_to_string(entry.path()).map_err(|error| error.to_string())?;
        for import in imports(&source) {
            let resolved = if let Some(path) = import.strip_prefix("stdlib/") {
                root.join("crates/vibelang-std/stdlib").join(path)
            } else {
                entry.path().parent().unwrap_or(&examples).join(import)
            };
            if !resolved.is_file() {
                return Err(format!(
                    "{} imports missing file {}",
                    entry.path().display(),
                    resolved.display()
                ));
            }
        }
        let code = strip_comments_and_strings(&source);
        for name in FICTIONAL_CALLS {
            if contains_call(&code, name) {
                return Err(format!(
                    "{} contains fictional public call `{name}`",
                    entry.path().display()
                ));
            }
        }
    }
    println!("validated imports and fictional-call denylist in {checked} example files");
    Ok(())
}

fn validate_editor_consumers(root: &Path, manifest: &PublicApiManifest) -> Result<(), String> {
    const EDITOR_ROOTS: &[&str] = &["crates/vibelang-lsp/src", "vscode-extension/src"];
    const DOCUMENTED_CONSUMERS: &[&str] = &[
        "docs/interfaces/lsp-and-editors.md",
        "landing-page/src/components/CodeDemo.jsx",
        "landing-page/src/components/Documentation.jsx",
        "landing-page/src/data/autocompleteData.js",
    ];

    let supported = manifest
        .entries
        .iter()
        .filter(|entry| {
            entry.kind == "function"
                && matches!(
                    entry.surface.as_str(),
                    "rhai" | "dsp_rhai" | "rhai_extension"
                )
                && !matches!(
                    entry.availability.status.as_str(),
                    "quarantined" | "documentation_only"
                )
                && !entry.overloads.is_empty()
        })
        .map(|entry| entry.registered_name.as_str())
        .collect::<BTreeSet<_>>();
    for required in ["sample", "set_quantization"] {
        if !supported.contains(required) {
            return Err(format!(
                "editor replacement `{required}` is not backed by an available manifest function"
            ));
        }
    }

    let mut checked = 0usize;
    for relative_root in EDITOR_ROOTS {
        for entry in WalkDir::new(root.join(relative_root)) {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry.file_type().is_file()
                || !matches!(
                    entry.path().extension().and_then(|value| value.to_str()),
                    Some("rs" | "ts")
                )
            {
                continue;
            }
            let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let source = fs::read_to_string(entry.path()).map_err(|error| error.to_string())?;
            validate_editor_source(relative, &source, true)?;
            checked += 1;
        }
    }
    for relative in DOCUMENTED_CONSUMERS {
        let source = fs::read_to_string(root.join(relative)).map_err(|error| error.to_string())?;
        validate_editor_source(Path::new(relative), &source, false)?;
        checked += 1;
    }

    println!(
        "validated manifest-backed editor metadata and rejected-call fixtures in {checked} consumer files"
    );
    Ok(())
}

fn validate_editor_source(
    path: &Path,
    source: &str,
    reject_stale_rows: bool,
) -> Result<(), String> {
    const FICTIONAL_CALLS: &[&str] = &[
        "load_sample",
        "at_bar",
        "after",
        "fade_in",
        "fade_out",
        "midi_out",
        "midi_in",
        "export_audio",
    ];
    const STALE_ROWS: &[&str] = &["load_sample", "at_bar", "midi_out", "midi_in"];

    for name in FICTIONAL_CALLS {
        if contains_unqualified_call(source, name) {
            return Err(format!(
                "{} contains fictional public editor call `{name}`",
                path.display()
            ));
        }
    }
    if contains_string_first_argument_call(source, "set_quantization") {
        return Err(format!(
            "{} emits string-valued `set_quantization`; the manifest accepts only numeric overloads",
            path.display()
        ));
    }
    if reject_stale_rows {
        for name in STALE_ROWS {
            if contains_identifier(source, name) {
                return Err(format!(
                    "{} retains stale editor metadata row `{name}` without a manifest source",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn contains_unqualified_call(source: &str, name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find(name) {
        let start = offset + relative;
        let end = start + name.len();
        let before_ok = start == 0
            || (!bytes[start - 1].is_ascii_alphanumeric()
                && bytes[start - 1] != b'_'
                && bytes[start - 1] != b'.');
        let mut after = end;
        while after < bytes.len() && bytes[after].is_ascii_whitespace() {
            after += 1;
        }
        if before_ok && after < bytes.len() && bytes[after] == b'(' {
            return true;
        }
        offset = end;
    }
    false
}

fn contains_identifier(source: &str, name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find(name) {
        let start = offset + relative;
        let end = start + name.len();
        let before_ok =
            start == 0 || (!bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_');
        let after_ok =
            end == bytes.len() || (!bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_');
        if before_ok && after_ok {
            return true;
        }
        offset = end;
    }
    false
}

fn contains_string_first_argument_call(source: &str, name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find(name) {
        let start = offset + relative;
        let end = start + name.len();
        let before_ok = start == 0
            || (!bytes[start - 1].is_ascii_alphanumeric()
                && bytes[start - 1] != b'_'
                && bytes[start - 1] != b'.');
        let mut after = end;
        while after < bytes.len() && bytes[after].is_ascii_whitespace() {
            after += 1;
        }
        if before_ok && after < bytes.len() && bytes[after] == b'(' {
            after += 1;
            while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                after += 1;
            }
            if after < bytes.len() && matches!(bytes[after], b'"' | b'\'') {
                return true;
            }
            if after + 1 < bytes.len()
                && bytes[after] == b'\\'
                && matches!(bytes[after + 1], b'"' | b'\'')
            {
                return true;
            }
        }
        offset = end;
    }
    false
}

fn imports(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let rest = line.strip_prefix("import")?.trim_start();
            let rest = rest.strip_prefix('"')?;
            Some(rest.split('"').next()?.to_string())
        })
        .collect()
}

fn strip_comments_and_strings(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut characters = source.chars().peekable();
    let mut string = false;
    let mut escaped = false;
    while let Some(character) = characters.next() {
        if string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                string = false;
            }
            output.push(' ');
        } else if character == '"' {
            string = true;
            output.push(' ');
        } else if character == '/' && characters.peek() == Some(&'/') {
            output.push(' ');
            output.push(' ');
            characters.next();
            for comment in characters.by_ref() {
                if comment == '\n' {
                    output.push('\n');
                    break;
                }
                output.push(' ');
            }
        } else {
            output.push(character);
        }
    }
    output
}

fn contains_call(source: &str, name: &str) -> bool {
    let bytes = source.as_bytes();
    let needle = name.as_bytes();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find(name) {
        let start = offset + relative;
        let end = start + needle.len();
        let before_ok =
            start == 0 || !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_';
        let mut after = end;
        while after < bytes.len() && bytes[after].is_ascii_whitespace() {
            after += 1;
        }
        if before_ok && after < bytes.len() && bytes[after] == b'(' {
            return true;
        }
        offset = end;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn static_artifacts_match_committed_snapshots() {
        let root = root();
        let manifest: PublicApiManifest =
            serde_json::from_str(&fs::read_to_string(root.join(MANIFEST_PATH)).unwrap()).unwrap();
        validate_manifest_availability(&manifest).unwrap();
        validate_example_imports(&root).unwrap();
        validate_editor_consumers(&root, &manifest).unwrap();
        assert_eq!(
            render_ugen_reference(&manifest).unwrap(),
            fs::read_to_string(root.join(UGEN_REFERENCE_PATH)).unwrap()
        );
        assert_eq!(
            render_wasm_types(&root).unwrap(),
            fs::read_to_string(root.join(WASM_TYPES_PATH)).unwrap()
        );
        assert_eq!(
            render_editor_rhai(&manifest).unwrap(),
            fs::read_to_string(root.join(EDITOR_RHAI_PATH)).unwrap()
        );
        assert_eq!(
            render_editor_rhai(&manifest).unwrap(),
            fs::read_to_string(root.join(LSP_RHAI_PATH)).unwrap()
        );
        assert_eq!(
            render_editor_stdlib(&root, &manifest).unwrap(),
            fs::read_to_string(root.join(EDITOR_STDLIB_PATH)).unwrap()
        );
        let http = build_http_snapshot(&root).unwrap();
        assert_eq!(
            pretty_json(&http).unwrap(),
            fs::read_to_string(root.join(HTTP_SNAPSHOT_PATH)).unwrap()
        );
        assert_eq!(
            render_http_reference(&http),
            fs::read_to_string(root.join(HTTP_REFERENCE_PATH)).unwrap()
        );
        sync_ugen_projections(&root, true).unwrap();
    }

    #[test]
    fn route_extraction_preserves_cfg_and_rejects_shape_drift() {
        let source = r#"
            let app = Router::new().route("/always", get(routes::always));
            #[cfg(feature = "midi")]
            let app = app.route("/midi", post(routes::midi));
        "#;
        let regions = cfg_route_regions(source).unwrap();
        let routes = extract_routes(source, &regions).unwrap();
        assert_eq!(routes.len(), 2);
        assert!(routes[0].availability.is_empty());
        assert_eq!(routes[1].availability, vec!["feature = \"midi\""]);
        assert_eq!(routes[1].handler, "routes::midi");
    }

    #[test]
    fn fictional_example_calls_ignore_comments_and_strings_but_not_code() {
        let source = "// load_sample()\nlet text = \"after()\";\nexport_audio();";
        let code = strip_comments_and_strings(source);
        assert!(!contains_call(&code, "load_sample"));
        assert!(!contains_call(&code, "after"));
        assert!(contains_call(&code, "export_audio"));
    }

    #[test]
    fn editor_consumer_rejects_fictional_snippets_and_stale_rows() {
        let error = validate_editor_source(
            Path::new("sampleBrowser.ts"),
            r#"const snippet = `let kick = load_sample("kick", "kick.wav");`;"#,
            true,
        )
        .unwrap_err();
        assert!(error.contains("fictional public editor call `load_sample`"));

        for name in ["load_sample", "at_bar", "midi_out", "midi_in"] {
            let source = format!(r#"let row = ("{name}", "legacy signature");"#);
            let error =
                validate_editor_source(Path::new("signature_help.rs"), &source, true).unwrap_err();
            assert!(error.contains(&format!("stale editor metadata row `{name}`")));
        }
    }

    #[test]
    fn editor_consumer_rejects_string_quantization_snippets() {
        for source in [
            r#"const snippet = 'set_quantization("bar")';"#,
            r#""set_quantization(\"$1\")$0".to_string()"#,
        ] {
            let error = validate_editor_source(Path::new("completion"), source, true).unwrap_err();
            assert!(error.contains("emits string-valued `set_quantization`"));
        }
        validate_editor_source(
            Path::new("completion"),
            "const snippet = 'set_quantization(4.0)';",
            true,
        )
        .unwrap();
    }

    #[test]
    fn cli_help_normalization_is_platform_stable() {
        assert_eq!(normalize_cli_help("a\r\n\r\n"), "a\n");
    }

    #[test]
    fn wasm_start_hooks_are_not_projected_as_callable_exports() {
        let file = syn::parse_file(
            r#"
                #[wasm_bindgen(start)]
                pub fn init_panic_hook() {}

                #[wasm_bindgen]
                pub fn version() -> String {
                    String::new()
                }
            "#,
        )
        .unwrap();
        let (_, functions) = wasm_exports(&file).unwrap();
        assert_eq!(
            functions
                .iter()
                .map(|function| function.name.as_str())
                .collect::<Vec<_>>(),
            vec!["version"]
        );
    }
}
