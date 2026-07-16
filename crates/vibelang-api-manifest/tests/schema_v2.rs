use serde_json::{json, Value};
use std::collections::BTreeMap;
use vibelang_api_manifest::{
    canonical::{canonical_json, canonical_sha256_hex, sha256_hex, DecimalCounter},
    compatibility::{
        classify_change_kind, ChangeKind, CompatibilityChange, CompatibilityClass,
        CompatibilityReport,
    },
    fragments::{
        consumer_denominator_baseline_sha256, parse_authoring_fragment, parse_consumers_fragment,
        parse_http_fragment, parse_runtime_fragment, parse_wasm_fragment, parse_websocket_fragment,
        DiscoveredSemanticNode, FragmentSet, SemanticFacet,
    },
    v2::{
        parse_manifest, parse_v2_manifest, to_pretty_json_v2, Alias, AliasKind, ApiEntryV2,
        ApiType, AvailabilityStatus, AvailabilityV2, BindingDetails, Capability,
        CapabilityExpression, ConsistencyPoint, Consumer, CoverageRecord, Derivation, Eligibility,
        Event, EventDelivery, EventOrdering, Facet, FailureContract, FailureDelivery, FailureStage,
        FallbackPolicy, Field, FieldDirection, Generator, HttpSuccess, Idempotency, LossDetection,
        NodeMetadata, Operation, OperationKind, OverloadV2, PanicExposure, PriorState,
        ProvenanceAnchor, PublicApiManifestV2, RevisionRelation, Stability, StabilityLevel,
        SurfaceBinding, TypeKind, UnavailableBehavior, VersionedPublicApiManifest, SCHEMA_URI_V2,
        SCHEMA_VERSION_V2,
    },
    Anchor, DuplicateNameHandling, EntryDetails, ErrorCode, PublicApiManifest, StdlibDeclaration,
    UgenInput,
};

const AUTHORING: &str = include_str!("fixtures/authoring.toml");
const RUNTIME: &str = include_str!("fixtures/runtime.toml");
const HTTP: &str = include_str!("fixtures/http.toml");
const WEBSOCKET: &str = include_str!("fixtures/websocket.toml");
const WASM: &str = include_str!("fixtures/wasm.toml");
const CONSUMERS: &str = include_str!("fixtures/consumers.toml");

fn fragment_set() -> FragmentSet {
    FragmentSet {
        authoring: parse_authoring_fragment(AUTHORING).unwrap(),
        runtime: parse_runtime_fragment(RUNTIME).unwrap(),
        http: parse_http_fragment(HTTP).unwrap(),
        websocket: parse_websocket_fragment(WEBSOCKET).unwrap(),
        wasm: parse_wasm_fragment(WASM).unwrap(),
        consumers: parse_consumers_fragment(CONSUMERS).unwrap(),
    }
}

fn discovered_nodes() -> Vec<DiscoveredSemanticNode> {
    [
        ("v1:entry:0000000000000001", SemanticFacet::Stability),
        ("v1:operation:0000000000000002", SemanticFacet::Operation),
        ("v1:field:0000000000000003", SemanticFacet::Consistency),
        ("v1:event:0000000000000004", SemanticFacet::Event),
        ("v1:consumer:0000000000000005", SemanticFacet::WasmHost),
        ("v1:consumer:0000000000000006", SemanticFacet::Coverage),
    ]
    .into_iter()
    .map(|(id, facet)| DiscoveredSemanticNode {
        id: id.into(),
        required_facets: [facet].into_iter().collect(),
    })
    .collect()
}

fn fixture_metadata(id: &str, name: &str) -> NodeMetadata {
    NodeMetadata {
        id: id.into(),
        name: name.into(),
        aliases: Vec::new(),
        stability: Stability {
            level: StabilityLevel::Stable,
            since: Some("0.4.0".into()),
            deprecated_since: None,
            replacement_id: None,
            reason: None,
            removal_not_before: None,
        },
        availability: AvailabilityV2 {
            status: AvailabilityStatus::Available,
            when: None,
            on_unavailable: UnavailableBehavior::Hidden,
            evidence: vec!["fixture".into()],
        },
        ownership: vibelang_api_manifest::v2::Ownership {
            contract_owner: "manifest".into(),
            implementation_owner: "core".into(),
            consumer_owner: None,
        },
        source_anchors: vec![ProvenanceAnchor {
            path: "crates/vibelang-api-manifest/tests/schema_v2.rs".into(),
            symbol: "fixture_metadata".into(),
            line: None,
            derivation: Derivation::BehavioralFixture,
        }],
        test_anchors: Vec::new(),
    }
}

fn fixture_manifest() -> PublicApiManifestV2 {
    let capability_id = "v1:capability:0000000000000001".to_string();
    let mut capability_metadata = fixture_metadata(&capability_id, "target.native");
    capability_metadata.aliases.push(Alias {
        id: "v1:capability:0000000000000002".into(),
        canonical_id: capability_id,
        kind: AliasKind::Rename,
        since: "0.4.0".into(),
        deprecated_since: "0.5.0".into(),
        removal_not_before: "1.0.0".into(),
        warning: "use target.native".into(),
        behavior_fixture: "schema_v2::stable_alias".into(),
        compatibility_classes: [CompatibilityClass::BehavioralChange].into_iter().collect(),
    });
    PublicApiManifestV2 {
        schema: SCHEMA_URI_V2.into(),
        schema_version: SCHEMA_VERSION_V2,
        api_version: "0.4.0".into(),
        generator: Generator {
            name: "fixture-generator".into(),
            format_version: 1,
        },
        entries: Vec::new(),
        types: Vec::new(),
        operations: Vec::new(),
        events: Vec::new(),
        capabilities: vec![Capability {
            metadata: capability_metadata,
            detection_source: "fixture".into(),
            dependencies: Vec::new(),
            conflicts: Vec::new(),
            runtime_states: vibelang_api_manifest::fragments::capability_states(),
            projection_rules: vec!["fixture".into()],
        }],
        consumers: Vec::new(),
        coverage: BTreeMap::new(),
        stats: BTreeMap::new(),
    }
}

fn manifest_with_entry_details(details: EntryDetails) -> PublicApiManifestV2 {
    let mut manifest = fixture_manifest();
    manifest.entries.push(ApiEntryV2 {
        metadata: fixture_metadata("v1:entry:0000000000000001", "fixture.entry"),
        surface: "rhai".into(),
        kind: "type".into(),
        registered_name: "FixtureEntry".into(),
        receiver: None,
        overloads: Vec::new(),
        details,
        lifecycle: Facet::NotApplicable {
            reason: "fixture entry".into(),
        },
        operation_ids: Vec::new(),
        consumer_ids: Vec::new(),
    });
    manifest
}

fn fixture_failure() -> FailureContract {
    FailureContract {
        stages: vec![FailureStage::Validate],
        error_ids: Vec::new(),
        retryable: false,
        prior_state: PriorState::Unchanged,
        fallback: FallbackPolicy::Reject,
        panic_exposure: PanicExposure::None,
        delivery: [FailureDelivery::Return].into_iter().collect(),
        cleanup_owner: None,
    }
}

fn manifest_with_all_metadata_owners() -> PublicApiManifestV2 {
    let mut manifest = manifest_with_entry_details(EntryDetails::RhaiType {
        display_name: "FixtureEntry".into(),
    });
    manifest.entries[0].overloads.push(OverloadV2 {
        metadata: fixture_metadata("v1:overload:0000000000000001", "fixture.overload"),
        signature: "FixtureEntry()".into(),
        parameters: Vec::new(),
        return_type: "FixtureEntry".into(),
        returns_receiver: None,
        lifecycle: Facet::NotApplicable {
            reason: "fixture overload".into(),
        },
        value_contract: Facet::NotApplicable {
            reason: "fixture overload".into(),
        },
        failure: fixture_failure(),
        operation_ids: Vec::new(),
    });

    let type_id = "v1:type:0000000000000001";
    manifest.types.push(ApiType {
        metadata: fixture_metadata(type_id, "fixture.type"),
        kind: TypeKind::Record,
        fields: vec![Field {
            metadata: fixture_metadata("v1:field:0000000000000001", "fixture.field"),
            serialized_name: "value".into(),
            host_name: "value".into(),
            direction: FieldDirection::Output,
            required: true,
            type_id: type_id.into(),
            default: None,
            value_contract: Facet::NotApplicable {
                reason: "fixture field".into(),
            },
            operation_applicability: Facet::NotApplicable {
                reason: "fixture field".into(),
            },
            bindings: Vec::new(),
            observation: Facet::NotApplicable {
                reason: "fixture field".into(),
            },
        }],
        variants: Vec::new(),
    });

    manifest.operations.push(Operation {
        metadata: fixture_metadata("v1:operation:0000000000000001", "fixture.operation"),
        kind: OperationKind::Read,
        request_type_id: None,
        response_type_ids: Vec::new(),
        error_type_id: type_id.into(),
        effects: Vec::new(),
        idempotency: Idempotency::Yes,
        effect_timing: Facet::NotApplicable {
            reason: "fixture read".into(),
        },
        atomicity: Facet::NotApplicable {
            reason: "fixture read".into(),
        },
        revision: Facet::NotApplicable {
            reason: "fixture read".into(),
        },
        receipt: Facet::NotApplicable {
            reason: "fixture read".into(),
        },
        consistency: ConsistencyPoint::ResponseSnapshot,
        security_capability_ids: Vec::new(),
        bindings: vec![SurfaceBinding {
            metadata: fixture_metadata("v1:binding:0000000000000001", "fixture.binding"),
            details: BindingDetails::Cli {
                binary: "vibe".into(),
                command: vec!["fixture".into()],
                defaults: BTreeMap::new(),
                environment_sources: Vec::new(),
                exit_contract: "fixture".into(),
            },
        }],
    });

    manifest.events.push(Event {
        metadata: fixture_metadata("v1:event:0000000000000001", "fixture.event"),
        payload_type_id: type_id.into(),
        producer_operation_id: None,
        protocol_version: "v2".into(),
        ordering: EventOrdering::ObservationSequence,
        revision_relation: RevisionRelation::None,
        delivery: EventDelivery::AtLeastOnce,
        loss_detection: LossDetection::NotApplicable,
        resync_operation_id: None,
    });

    manifest.consumers.push(Consumer {
        metadata: fixture_metadata("v1:consumer:0000000000000001", "fixture.consumer"),
        source_projections: Vec::new(),
        eligibility: Eligibility {
            surfaces: Vec::new(),
            kinds: Vec::new(),
            stability_levels: [StabilityLevel::Stable].into_iter().collect(),
            capability_ids: Vec::new(),
        },
        included_ids: Vec::new(),
        exclusions: Vec::new(),
        host: Facet::NotApplicable {
            reason: "fixture consumer".into(),
        },
        coverage_policy: Facet::NotApplicable {
            reason: "fixture consumer".into(),
        },
        package: Facet::NotApplicable {
            reason: "fixture consumer".into(),
        },
    });
    manifest
}

fn manifest_with_http_binding() -> PublicApiManifestV2 {
    let mut manifest = manifest_with_all_metadata_owners();
    let type_id = manifest.types[0].metadata.id.clone();
    let operation = &mut manifest.operations[0];
    operation.response_type_ids = vec![type_id.clone()];
    operation.bindings[0].details = BindingDetails::Http {
        method: "GET".into(),
        path: "/fixture".into(),
        path_type_ids: Vec::new(),
        query_type_ids: Vec::new(),
        header_type_ids: Vec::new(),
        body_type_id: None,
        successes: vec![HttpSuccess {
            status: 200,
            type_id: type_id.clone(),
        }],
        error_type_id: type_id,
        protocol_version: "v1".into(),
        authentication_capability_id: None,
        idempotency_header: None,
        revision_header: None,
    };
    manifest
}

fn assert_v2_details_reject_unknown_field(details: EntryDetails, object_pointer: &str) {
    let mut value = serde_json::to_value(manifest_with_entry_details(details)).unwrap();
    parse_v2_manifest(&serde_json::to_string(&value).unwrap()).unwrap();
    value
        .pointer_mut(object_pointer)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("unknown_v2_field".into(), Value::Bool(true));
    assert_eq!(
        parse_v2_manifest(&serde_json::to_string(&value).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::UnknownField,
        "unknown field at {object_pointer} was accepted"
    );
}

#[test]
fn schema_v2_round_trips_deterministically() {
    let manifest = fixture_manifest();
    let first = to_pretty_json_v2(&manifest).unwrap();
    let parsed = parse_v2_manifest(&first).unwrap();
    let second = to_pretty_json_v2(&parsed).unwrap();
    assert_eq!(first, second);
    assert_eq!(manifest, parsed);
    assert!(first.ends_with("}\n"));
    assert!(!first.ends_with("\n\n"));
}

#[test]
fn checked_in_v1_manifest_still_parses_without_projection_changes() {
    let input = include_str!("../../../api/public-api-manifest-v1.json");
    let direct: PublicApiManifest = serde_json::from_str(input).unwrap();
    let versioned = parse_manifest(input).unwrap();
    match versioned {
        VersionedPublicApiManifest::V1(parsed) => assert_eq!(parsed, direct),
        VersionedPublicApiManifest::V2(_) => panic!("v1 fixture parsed as v2"),
    }
    assert_eq!(direct.schema_version, 1);
    assert_eq!(direct.entries.len(), 3_626);
}

#[test]
fn v2_rejects_unknown_fields_in_every_reused_v1_details_boundary() {
    assert_v2_details_reject_unknown_field(
        EntryDetails::RhaiType {
            display_name: "FixtureEntry".into(),
        },
        "/entries/0/details",
    );
    assert_v2_details_reject_unknown_field(
        EntryDetails::Ugen {
            class: "FixtureUgen".into(),
            description: "fixture".into(),
            rate: "ar".into(),
            runtime_rate: "audio".into(),
            category: "fixture".into(),
            inputs: vec![UgenInput {
                name: "input".into(),
                input_type: "f64".into(),
                default: None,
                description: "fixture".into(),
            }],
            outputs: 1,
            emitted_class: "FixtureUgen".into(),
            special_index: 0,
            pseudo: false,
            callable: true,
            requires_plugin: None,
            unavailable_reason: None,
        },
        "/entries/0/details/inputs/0",
    );

    let stdlib_details = EntryDetails::StdlibDefinition {
        definition_kind: "synthdef".into(),
        import_paths: vec!["fixture".into()],
        declarations: vec![StdlibDeclaration {
            import_path: "fixture".into(),
            definition_kind: "synthdef".into(),
            callable_signature: None,
            access: "public".into(),
            export_classification: "exported".into(),
            support_classification: "supported".into(),
            source_anchor: Anchor {
                path: "fixture".into(),
                symbol: "fixture".into(),
                line: None,
            },
        }],
        duplicate_name: DuplicateNameHandling {
            status: "unique".into(),
            declaration_count: 1,
            import_paths: vec!["fixture".into()],
            resolution: "exact".into(),
        },
        export_classification: "exported".into(),
        support_classification: "supported".into(),
    };
    assert_v2_details_reject_unknown_field(
        stdlib_details.clone(),
        "/entries/0/details/declarations/0",
    );
    assert_v2_details_reject_unknown_field(
        stdlib_details.clone(),
        "/entries/0/details/declarations/0/source_anchor",
    );
    assert_v2_details_reject_unknown_field(stdlib_details, "/entries/0/details/duplicate_name");
}

#[test]
fn v1_details_remain_permissive_while_the_3626_entry_baseline_parses() {
    let mut value: Value =
        serde_json::from_str(include_str!("../../../api/public-api-manifest-v1.json")).unwrap();
    value["entries"][0]["details"]
        .as_object_mut()
        .unwrap()
        .insert("unknown_v2_field".into(), Value::Bool(true));
    match parse_manifest(&serde_json::to_string(&value).unwrap()).unwrap() {
        VersionedPublicApiManifest::V1(parsed) => assert_eq!(parsed.entries.len(), 3_626),
        VersionedPublicApiManifest::V2(_) => panic!("v1 fixture parsed as v2"),
    }
}

#[test]
fn full_manifest_validates_conditional_availability_for_every_metadata_owner() {
    let capability_id = "v1:capability:0000000000000001";
    let conditional = AvailabilityV2 {
        status: AvailabilityStatus::Conditional,
        when: Some(CapabilityExpression::All {
            expressions: vec![
                CapabilityExpression::Ref {
                    capability_id: capability_id.into(),
                },
                CapabilityExpression::Not {
                    expression: Box::new(CapabilityExpression::Any {
                        expressions: vec![CapabilityExpression::Ref {
                            capability_id: capability_id.into(),
                        }],
                    }),
                },
            ],
        }),
        on_unavailable: UnavailableBehavior::StructuredError,
        evidence: vec!["fixture".into()],
    };
    let mut manifest = manifest_with_all_metadata_owners();
    manifest.entries[0].metadata.availability = conditional.clone();
    manifest.entries[0].overloads[0].metadata.availability = conditional.clone();
    manifest.types[0].metadata.availability = conditional.clone();
    manifest.types[0].fields[0].metadata.availability = conditional.clone();
    manifest.operations[0].metadata.availability = conditional.clone();
    manifest.operations[0].bindings[0].metadata.availability = conditional.clone();
    manifest.events[0].metadata.availability = conditional.clone();
    manifest.capabilities[0].metadata.availability = conditional.clone();
    manifest.consumers[0].metadata.availability = conditional;
    let valid_json = to_pretty_json_v2(&manifest).unwrap();
    parse_v2_manifest(&valid_json).unwrap();

    let owners = [
        ("/entries/0", "v1:entry:0000000000000001"),
        ("/entries/0/overloads/0", "v1:overload:0000000000000001"),
        ("/types/0", "v1:type:0000000000000001"),
        ("/types/0/fields/0", "v1:field:0000000000000001"),
        ("/operations/0", "v1:operation:0000000000000001"),
        ("/operations/0/bindings/0", "v1:binding:0000000000000001"),
        ("/events/0", "v1:event:0000000000000001"),
        ("/capabilities/0", "v1:capability:0000000000000001"),
        ("/consumers/0", "v1:consumer:0000000000000001"),
    ];
    let base_value: Value = serde_json::from_str(&valid_json).unwrap();
    let mut unknown_binding = base_value.clone();
    unknown_binding["operations"][0]["bindings"][0]
        .as_object_mut()
        .unwrap()
        .insert("unknown_v2_field".into(), Value::Bool(true));
    assert_eq!(
        parse_v2_manifest(&serde_json::to_string(&unknown_binding).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::UnknownField
    );
    for (owner_pointer, owner_id) in owners {
        let mut value = base_value.clone();
        *value
            .pointer_mut(&format!("{owner_pointer}/availability"))
            .unwrap() = json!({
            "status": "conditional",
            "when": {
                "kind": "ref",
                "capability_id": "v1:capability:ffffffffffffffff"
            },
            "on_unavailable": "structured_error",
            "evidence": ["fixture"]
        });
        let error = parse_v2_manifest(&serde_json::to_string(&value).unwrap()).unwrap_err();
        assert_eq!(
            error.code,
            ErrorCode::InvalidReference,
            "owner {owner_pointer}"
        );
        assert_eq!(error.path, owner_id, "owner {owner_pointer}");
    }
}

#[test]
fn stable_aliases_require_a_new_id_and_exact_canonical_target() {
    let mut manifest = fixture_manifest();
    manifest.capabilities[0].metadata.aliases[0].canonical_id =
        "v1:capability:0000000000000099".into();
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::AliasConflict
    );

    let mut manifest = fixture_manifest();
    manifest.capabilities[0].metadata.aliases[0].id = manifest.capabilities[0].metadata.id.clone();
    assert_eq!(
        manifest.validate().unwrap_err().code,
        ErrorCode::AliasConflict
    );
}

#[test]
#[allow(clippy::excessive_precision)]
fn rfc_8785_and_sha256_vectors_are_pinned() {
    let value = json!({
        "numbers": [333333333.33333329_f64, 1E30_f64, 4.50_f64, 2e-3_f64, 1e-27_f64],
        "string": "\u{20ac}$\u{000f}\nA'B\"\\\"/",
        "literals": [Value::Null, Value::Bool(true), Value::Bool(false)]
    });
    let expected = concat!(
        "{\"literals\":[null,true,false],",
        "\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27],",
        "\"string\":\"€$\\u000f\\nA'B\\\"\\\\\\\"/\"}"
    );
    let canonical = canonical_json(&value).unwrap();
    assert_eq!(canonical, expected.as_bytes());
    assert_eq!(
        sha256_hex(&canonical),
        "6d77565c0fe51d7346bd5debb08f2eebbe9bde01eade30b34e2011f360f91b0e"
    );
    assert_eq!(
        canonical_sha256_hex(&value).unwrap(),
        "6d77565c0fe51d7346bd5debb08f2eebbe9bde01eade30b34e2011f360f91b0e"
    );
}

#[test]
fn decimal_counter_uses_lossless_javascript_wire_strings() {
    let counter = DecimalCounter::new(u64::MAX);
    assert_eq!(
        serde_json::to_string(&counter).unwrap(),
        "\"18446744073709551615\""
    );
    assert_eq!(
        serde_json::from_str::<DecimalCounter>("\"18446744073709551615\"")
            .unwrap()
            .get(),
        u64::MAX
    );
    for invalid in ["\"01\"", "\"+1\"", "18446744073709551615"] {
        assert!(
            serde_json::from_str::<DecimalCounter>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn all_six_fragment_fixtures_parse_and_validate_completeness() {
    let first = fragment_set();
    let second = fragment_set();
    assert_eq!(first, second);
    first.validate(&discovered_nodes()).unwrap();
}

#[test]
fn consumer_denominator_baseline_is_checksum_protected() {
    let parsed = parse_consumers_fragment(CONSUMERS).unwrap();
    assert_eq!(
        consumer_denominator_baseline_sha256(&parsed.denominator_baseline).unwrap(),
        parsed.denominator_baseline.sha256
    );
    let tampered = CONSUMERS.replace("accepted_denominator = 1", "accepted_denominator = 2");
    assert_eq!(
        parse_consumers_fragment(&tampered).unwrap_err().code,
        ErrorCode::InvalidValue
    );
}

#[test]
fn coverage_requires_a_nonshrinking_accepted_denominator_and_exact_equation() {
    let mut manifest = manifest_with_all_metadata_owners();
    let consumer_id = manifest.consumers[0].metadata.id.clone();
    let included_id = manifest.types[0].metadata.id.clone();
    manifest.consumers[0].included_ids.push(included_id);
    manifest.coverage.insert(
        consumer_id.clone(),
        CoverageRecord {
            numerator: 1,
            denominator: 1,
            exclusions_by_reason: BTreeMap::new(),
            unresolved_ids: Vec::new(),
            stale_ids: Vec::new(),
            base_denominator: 1,
        },
    );
    manifest.validate().unwrap();

    let mut shrunken = manifest.clone();
    shrunken
        .coverage
        .get_mut(&consumer_id)
        .unwrap()
        .base_denominator = 2;
    assert_eq!(
        shrunken.validate().unwrap_err().code,
        ErrorCode::InvalidValue
    );

    let mut inconsistent = manifest.clone();
    inconsistent
        .coverage
        .get_mut(&consumer_id)
        .unwrap()
        .denominator = 2;
    assert_eq!(
        inconsistent.validate().unwrap_err().code,
        ErrorCode::InvalidValue
    );

    let mut missing_baseline = serde_json::to_value(manifest).unwrap();
    missing_baseline["coverage"][&consumer_id]
        .as_object_mut()
        .unwrap()
        .remove("base_denominator");
    assert_eq!(
        parse_v2_manifest(&serde_json::to_string(&missing_baseline).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::MissingFacet
    );
}

#[test]
fn fragment_set_rejects_orphan_availability_capabilities_and_accepts_valid_references() {
    let fragment_with_availability = |capability_id: &str| {
        format!(
            "{AUTHORING}\n[records.availability]\nstatus = \"conditional\"\non_unavailable = \"structured_error\"\nevidence = [\"fixture\"]\n\n[records.availability.when]\nkind = \"ref\"\ncapability_id = {capability_id:?}\n"
        )
    };

    let mut fragments = fragment_set();
    fragments.authoring = parse_authoring_fragment(&fragment_with_availability(
        "v1:capability:ffffffffffffffff",
    ))
    .unwrap();
    assert_eq!(
        fragments.validate(&discovered_nodes()).unwrap_err().code,
        ErrorCode::InvalidReference
    );

    let capability_id = "v1:capability:0000000000000001";
    fragments.authoring =
        parse_authoring_fragment(&fragment_with_availability(capability_id)).unwrap();
    let mut discovered = discovered_nodes();
    discovered.push(DiscoveredSemanticNode {
        id: capability_id.into(),
        required_facets: Default::default(),
    });
    fragments.validate(&discovered).unwrap();
}

#[test]
fn fragment_mutations_reject_missing_unknown_duplicate_orphan_and_mechanical_data() {
    let missing = AUTHORING.replace("level = \"stable\"\n", "");
    assert_eq!(
        parse_authoring_fragment(&missing).unwrap_err().code,
        ErrorCode::MissingFacet
    );

    let missing_applicable_detail = AUTHORING.replace("since = \"0.4.0\"\n", "");
    assert_eq!(
        parse_authoring_fragment(&missing_applicable_detail)
            .unwrap_err()
            .code,
        ErrorCode::MissingFacet
    );

    let unknown = AUTHORING.replace("level = \"stable\"", "level = \"future\"");
    assert_eq!(
        parse_authoring_fragment(&unknown).unwrap_err().code,
        ErrorCode::UnknownEnum
    );

    let unknown_field = AUTHORING.replace(
        "owner = \"vibelang-rhai\"",
        "owner = \"vibelang-rhai\"\nsemantic_guess = true",
    );
    assert_eq!(
        parse_authoring_fragment(&unknown_field).unwrap_err().code,
        ErrorCode::UnknownField
    );

    let record = AUTHORING.split("[[records]]").nth(1).unwrap();
    let duplicate = format!("{AUTHORING}[[records]]{record}");
    assert_eq!(
        parse_authoring_fragment(&duplicate).unwrap_err().code,
        ErrorCode::DuplicateId
    );

    let mechanical = AUTHORING.replace(
        "owner = \"vibelang-rhai\"",
        "owner = \"vibelang-rhai\"\nregistered_name = \"voice\"",
    );
    assert_eq!(
        parse_authoring_fragment(&mechanical).unwrap_err().code,
        ErrorCode::MechanicalFactRestatement
    );

    let mut discovered = discovered_nodes();
    discovered.retain(|node| node.id != "v1:event:0000000000000004");
    assert_eq!(
        fragment_set().validate(&discovered).unwrap_err().code,
        ErrorCode::OrphanId
    );
}

#[test]
fn duplicate_semantic_ownership_and_missing_required_facets_fail() {
    let mut fragments = fragment_set();
    fragments.wasm.records[0].target_id = fragments.authoring.records[0].target_id.clone();
    fragments.wasm.records[0].stability = fragments.authoring.records[0].stability.clone();
    let mut discovered = discovered_nodes();
    discovered.retain(|node| node.id != "v1:consumer:0000000000000005");
    assert_eq!(
        fragments.validate(&discovered).unwrap_err().code,
        ErrorCode::DuplicateOwner
    );

    let mut required = discovered_nodes();
    required[0].required_facets.insert(SemanticFacet::Lifecycle);
    assert_eq!(
        fragment_set().validate(&required).unwrap_err().code,
        ErrorCode::MissingFacet
    );
}

#[test]
fn http_bindings_require_connected_request_success_and_authentication_links() {
    let manifest = manifest_with_http_binding();
    manifest.validate().unwrap();

    let mut missing_success = manifest.clone();
    let BindingDetails::Http { successes, .. } =
        &mut missing_success.operations[0].bindings[0].details
    else {
        unreachable!()
    };
    successes.clear();
    assert_eq!(
        missing_success.validate().unwrap_err().code,
        ErrorCode::MissingFacet
    );

    let mut missing_request = manifest.clone();
    let type_id = missing_request.types[0].metadata.id.clone();
    let BindingDetails::Http { path_type_ids, .. } =
        &mut missing_request.operations[0].bindings[0].details
    else {
        unreachable!()
    };
    path_type_ids.push(type_id);
    assert_eq!(
        missing_request.validate().unwrap_err().code,
        ErrorCode::MissingFacet
    );

    let mut false_authentication = manifest.clone();
    let capability_id = false_authentication.capabilities[0].metadata.id.clone();
    let BindingDetails::Http {
        authentication_capability_id,
        ..
    } = &mut false_authentication.operations[0].bindings[0].details
    else {
        unreachable!()
    };
    *authentication_capability_id = Some(capability_id.clone());
    assert_eq!(
        false_authentication.validate().unwrap_err().code,
        ErrorCode::InvalidValue
    );

    let mut false_security = manifest.clone();
    false_security.operations[0]
        .security_capability_ids
        .push(capability_id.clone());
    assert_eq!(
        false_security.validate().unwrap_err().code,
        ErrorCode::InvalidValue
    );

    let mut authenticated = manifest;
    authenticated.operations[0]
        .security_capability_ids
        .push(capability_id.clone());
    let BindingDetails::Http {
        authentication_capability_id,
        ..
    } = &mut authenticated.operations[0].bindings[0].details
    else {
        unreachable!()
    };
    *authentication_capability_id = Some(capability_id);
    authenticated.validate().unwrap();
}

#[test]
fn compatibility_vectors_cover_every_frozen_class_and_reject_unclassified() {
    use CompatibilityClass::*;
    let vectors = [
        (ChangeKind::OwnershipOrAnchor, MetadataOnly),
        (ChangeKind::OptionalNodeAddition, CompatibleAddition),
        (ChangeKind::RangeWidening, CompatibleRelaxation),
        (ChangeKind::DefaultOrFallback, BehavioralChange),
        (ChangeKind::CallableRemovalOrRename, SourceBreaking),
        (ChangeKind::HttpMethodPathStatus, WireBreaking),
        (ChangeKind::AvailabilityNarrowing, AvailabilityBreaking),
        (ChangeKind::PackagePathOrEntrypoint, ConsumerBreaking),
        (ChangeKind::SecurityPolicy, SecurityOperational),
    ];
    for (kind, expected) in vectors {
        assert!(classify_change_kind(kind).contains(&expected));
    }

    let report = CompatibilityReport {
        changes: vec![CompatibilityChange {
            pointer: "/operations/0/unknown_future_facet".into(),
            before: Some(json!(true)),
            after: Some(json!(false)),
            classes: [Unclassified].into_iter().collect(),
            rationale: "fixture".into(),
            impacted_consumers: Vec::new(),
            required_action: "add a schema-aware classifier".into(),
        }],
    };
    assert_eq!(
        report.validate().unwrap_err().code,
        ErrorCode::UnclassifiedDiff
    );
}

#[test]
fn unknown_v2_enum_values_fail_closed() {
    let json = to_pretty_json_v2(&fixture_manifest()).unwrap();
    let mutated = json.replace("\"level\": \"stable\"", "\"level\": \"future\"");
    assert_eq!(
        parse_v2_manifest(&mutated).unwrap_err().code,
        ErrorCode::UnknownEnum
    );
}
