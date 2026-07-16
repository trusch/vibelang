use crate::{public_api, public_artifacts};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
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
    semantic_id, validate_stable_id, Alias, AliasKind, ApiEntryV2, ApiType, AvailabilityStatus,
    AvailabilityV2, BindingDetails, CancellationContract, Capability, CapabilityExpression,
    CapabilityState, ConsistencyPoint, Consumer, ConsumerExclusion, CoverageRecord, Derivation,
    EffectTiming, Eligibility, EnumVariant, Event, EventDelivery, EventOrdering, Facet,
    FailureContract, FailureDelivery, FailureStage, FallbackPolicy, Field, FieldBinding,
    FieldDirection, Generator, HttpSuccess, Idempotency, LifecycleContract, LifecycleEffect,
    LifecyclePhase, LifecycleRole, LossDetection, NodeMetadata, ObservationContract, Operation,
    OperationKind, Ownership, PackageContract, PanicExposure, ParameterV2, PriorState,
    ProvenanceAnchor, PublicApiManifestV2, RepeatSemantics, RevisionRelation, Stability,
    StabilityLevel, SurfaceBinding, Synchronization, TypeKind, UnavailableBehavior, SCHEMA_URI_V2,
    SCHEMA_VERSION_V2,
};
use vibelang_api_manifest::{
    to_pretty_json, Anchor, ApiEntry, Availability, BoundarySemantics, PublicApiManifest,
};

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

const WEBSOCKET_EVENTS: &[&str] = &[
    "hello",
    "playback.bar",
    "playback.tick",
    "transport.beat",
    "transport.bpm",
    "transport.started",
    "transport.stopped",
];

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

struct Discovery {
    v1: PublicApiManifest,
    v1_json: String,
    http: HttpSnapshot,
    baseline: ArtifactBaseline,
    fragments: FragmentSet,
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

    Ok(Discovery {
        v1,
        v1_json,
        http,
        baseline,
        fragments,
    })
}

fn compose(root: &Path, discovery: &Discovery) -> Result<BTreeMap<&'static str, String>, String> {
    let mut manifest = build_v2(discovery)?;
    validate_fragment_join(&manifest, &discovery.fragments)?;
    apply_fragments(&mut manifest, &discovery.fragments)?;
    manifest.validate().map_err(|error| error.to_string())?;

    let v2_json = vibelang_api_manifest::v2::to_pretty_json_v2(&manifest)
        .map_err(|error| error.to_string())?;
    let round_trip = vibelang_api_manifest::v2::parse_v2_manifest(&v2_json)
        .map_err(|error| error.to_string())?;
    if round_trip != manifest {
        return Err("schema-v2 serialization did not round-trip exactly".into());
    }
    let digest = canonical_sha256_hex(&manifest).map_err(|error| error.to_string())?;

    let coverage = build_coverage(root, discovery, &manifest, &digest)?;
    let debt = build_debt(root, &manifest, &digest)?;
    debt.validate(&contract_ids(&manifest))?;
    let diff = build_diff(discovery, &digest)?;
    diff.report.validate().map_err(|error| error.to_string())?;
    let packages = build_package_index(root, discovery, &digest)?;

    let mut outputs = BTreeMap::new();
    outputs.insert(V2_PATH, v2_json);
    outputs.insert(COVERAGE_PATH, pretty(&coverage)?);
    outputs.insert(DEBT_PATH, pretty(&debt)?);
    outputs.insert(DIFF_PATH, pretty(&diff)?);
    outputs.insert(PACKAGE_INDEX_PATH, pretty(&packages)?);
    Ok(outputs)
}

fn build_v2(discovery: &Discovery) -> Result<PublicApiManifestV2, String> {
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
    let event_type_ids = WEBSOCKET_EVENTS
        .iter()
        .map(|event| {
            let shape = format!("websocket payload {event}");
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
    for event in WEBSOCKET_EVENTS {
        let shape = format!("websocket payload {event}");
        types.push(ApiType {
            metadata: metadata(
                event_type_ids[&shape].clone(),
                shape.clone(),
                "vibelang-http",
                "crates/vibelang-http/src/websocket.rs",
                format!("WebSocketEvent::{event}"),
                Derivation::RustAst,
                available(),
                Vec::new(),
            ),
            kind: TypeKind::Record,
            fields: Vec::new(),
            variants: Vec::new(),
        });
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

    let mut events = WEBSOCKET_EVENTS
        .iter()
        .map(|event| Event {
            metadata: metadata(
                semantic_id("event", &format!("websocket|{event}")),
                (*event).to_string(),
                "vibelang-http",
                "crates/vibelang-http/src/websocket.rs",
                format!("WebSocketEvent::{event}"),
                Derivation::RustAst,
                available(),
                Vec::new(),
            ),
            payload_type_id: event_type_ids[&format!("websocket payload {event}")].clone(),
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

    let consumers = build_consumers(
        discovery,
        &consumer_ids,
        &entries,
        &types,
        &operations,
        &events,
    )?;
    let coverage = build_manifest_coverage(discovery, &consumer_ids)?;

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
    stats.insert("semantic_fragments".into(), 6);
    stats.insert("orphan_records".into(), 0);
    stats.insert("unclassified_records".into(), 0);

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
) -> Result<Vec<Consumer>, String> {
    let mut consumers = Vec::new();
    for (category, baseline) in &discovery.baseline.categories {
        let id = consumer_ids[category].clone();
        let mut included_ids = match category.as_str() {
            "manifest" => entries
                .iter()
                .flat_map(|entry| {
                    std::iter::once(entry.metadata.id.clone()).chain(
                        entry
                            .overloads
                            .iter()
                            .map(|value| value.metadata.id.clone()),
                    )
                })
                .collect(),
            "http" => types
                .iter()
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
                .collect(),
            "wasm" => events
                .iter()
                .map(|event| event.metadata.id.clone())
                .collect(),
            _ => Vec::new(),
        };
        included_ids.sort();
        included_ids.dedup();
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
                kinds: vec!["v1_compatibility_projection".into()],
                stability_levels: [StabilityLevel::Stable].into_iter().collect(),
                capability_ids: Vec::new(),
            },
            included_ids,
            exclusions: Vec::<ConsumerExclusion>::new(),
            package,
        });
    }
    consumers.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    Ok(consumers)
}

fn build_manifest_coverage(
    discovery: &Discovery,
    consumer_ids: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, CoverageRecord>, String> {
    let mut coverage = BTreeMap::new();
    for (category, baseline) in &discovery.baseline.categories {
        let denominator = match category.as_str() {
            "manifest" => (EXPECTED_ENTRIES + EXPECTED_OVERLOADS) as u64,
            "http" => (EXPECTED_HTTP_ROUTES + EXPECTED_HTTP_TYPES + EXPECTED_HTTP_FIELDS) as u64,
            _ => baseline.files.len() as u64,
        };
        let stale_ids = match category.as_str() {
            "rhai_editor" => vec![
                "a2_k_kr",
                "k2_a_ar",
                "lag2_ud_ar",
                "lag2_ud_kr",
                "lag3_ud_ar",
                "lag3_ud_kr",
                "t2_a_ar",
                "t2_k_kr",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            _ => Vec::new(),
        };
        coverage.insert(
            consumer_ids[category].clone(),
            CoverageRecord {
                numerator: denominator,
                denominator,
                exclusions_by_reason: BTreeMap::new(),
                unresolved_ids: Vec::new(),
                stale_ids,
                base_denominator: Some(denominator),
            },
        );
    }
    Ok(coverage)
}

fn validate_fragment_join(
    manifest: &PublicApiManifestV2,
    fragments: &FragmentSet,
) -> Result<(), String> {
    let ids = contract_ids(manifest);
    let mut required: BTreeMap<String, BTreeSet<SemanticFacet>> =
        ids.iter().map(|id| (id.clone(), BTreeSet::new())).collect();
    for record in &fragments.authoring.records {
        if record.stability.is_some() {
            require_facet(&mut required, &record.target_id, SemanticFacet::Stability)?;
        }
        if record.availability.is_some() {
            require_facet(
                &mut required,
                &record.target_id,
                SemanticFacet::Availability,
            )?;
        }
        if record.lifecycle.is_some() {
            require_facet(&mut required, &record.target_id, SemanticFacet::Lifecycle)?;
        }
    }
    for record in &fragments.runtime.records {
        if record.operation.is_some() {
            require_facet(&mut required, &record.target_id, SemanticFacet::Operation)?;
        }
    }
    for record in &fragments.http.records {
        if record.operation_id.is_some() {
            require_facet(
                &mut required,
                &record.target_id,
                SemanticFacet::OperationBinding,
            )?;
        }
        if record.effectiveness.is_some() {
            require_facet(
                &mut required,
                &record.target_id,
                SemanticFacet::Effectiveness,
            )?;
        }
    }
    for record in &fragments.websocket.records {
        if record.event.is_some() {
            require_facet(&mut required, &record.target_id, SemanticFacet::Event)?;
        }
    }
    for record in &fragments.wasm.records {
        if record.host.is_some() {
            require_facet(&mut required, &record.target_id, SemanticFacet::WasmHost)?;
        }
    }
    for record in &fragments.consumers.records {
        if record.policy.is_some() {
            require_facet(
                &mut required,
                &record.target_id,
                SemanticFacet::ConsumerPolicy,
            )?;
        }
        if record.coverage.is_some() {
            require_facet(&mut required, &record.target_id, SemanticFacet::Coverage)?;
        }
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
        .map_err(|error| error.to_string())
}

fn require_facet(
    required: &mut BTreeMap<String, BTreeSet<SemanticFacet>>,
    id: &str,
    facet: SemanticFacet,
) -> Result<(), String> {
    required
        .get_mut(id)
        .ok_or_else(|| format!("semantic fragment target {id} is orphaned"))?
        .insert(facet);
    Ok(())
}

fn apply_fragments(
    manifest: &mut PublicApiManifestV2,
    fragments: &FragmentSet,
) -> Result<(), String> {
    for record in &fragments.authoring.records {
        let metadata = find_metadata_mut(manifest, &record.target_id)
            .ok_or_else(|| format!("authoring target {} disappeared", record.target_id))?;
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
        if let Some(semantics) = &record.operation {
            operation.kind = semantics.kind;
            operation.idempotency = semantics.idempotency;
            operation.consistency = semantics.consistency;
        }
    }
    for record in &fragments.http.records {
        let field = manifest
            .types
            .iter_mut()
            .flat_map(|api_type| api_type.fields.iter_mut())
            .find(|field| field.metadata.id == record.target_id)
            .ok_or_else(|| format!("HTTP target {} is not a field", record.target_id))?;
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
        if let Some(semantics) = &record.event {
            event.ordering = semantics.ordering;
            event.revision_relation = semantics.revision_relation;
            event.delivery = semantics.delivery;
            event.loss_detection = semantics.loss_detection;
            event.resync_operation_id = semantics.resync_operation_id.clone();
        }
    }
    for record in &fragments.consumers.records {
        let consumer = manifest
            .consumers
            .iter_mut()
            .find(|consumer| consumer.metadata.id == record.target_id)
            .ok_or_else(|| format!("consumer target {} disappeared", record.target_id))?;
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
        .map(|consumer| ConsumerCoverage {
            id: consumer.metadata.id.clone(),
            projection_paths: consumer.source_projections.clone(),
            included_count: consumer.included_ids.len() as u64,
            unresolved_count: 0,
            unclassified_count: 0,
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
            orphan_records: 0,
            unclassified_records: 0,
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
        orphan_count: 0,
        unclassified_count: 0,
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
    Ok(DiffArtifact {
        schema: "https://vibelang.org/schemas/public-api-compatibility-diff/v1".into(),
        schema_version: 1,
        base: identity.clone(),
        candidate_v1_projection: identity,
        candidate_v2_digest: digest.into(),
        unchanged_entry_ids: EXPECTED_ENTRIES as u64,
        unchanged_overload_ids: EXPECTED_OVERLOADS as u64,
        unclassified_count: 0,
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
    digest: &str,
) -> Result<PackageIndex, String> {
    let packages = discovery
        .baseline
        .categories
        .get("packages")
        .ok_or_else(|| "M00 baseline has no package category".to_string())?;
    let mut files = Vec::new();
    let mut wasm_package_owners = Vec::new();
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
                wasm_package_owners.push(WasmPackageOwner {
                    path: baseline.path.clone(),
                    name: "vibelang-wasm".into(),
                    version: package["version"].as_str().unwrap_or("unknown").into(),
                    canonical: baseline.path == "crates/vibelang-wasm/package.json",
                    compatibility_debt: baseline.path == "landing-page/src/audio/package.json",
                });
            }
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    wasm_package_owners.sort_by(|left, right| left.path.cmp(&right.path));
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
        orphan_count: 0,
        unclassified_count: 0,
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
    fn orphan_fragment_targets_fail_the_composer_join() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let mut discovery = discover(&root()).unwrap();
                discovery.fragments.authoring.records[0].target_id =
                    semantic_id("entry", "missing-composer-target");
                let manifest = build_v2(&discovery).unwrap();
                let error = validate_fragment_join(&manifest, &discovery.fragments).unwrap_err();
                assert!(error.contains("orphaned"), "{error}");
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
