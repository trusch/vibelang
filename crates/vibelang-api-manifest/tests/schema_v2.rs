use serde_json::{json, Value};
use std::collections::BTreeMap;
use vibelang_api_manifest::{
    canonical::{canonical_json, canonical_sha256_hex, sha256_hex, DecimalCounter},
    compatibility::{
        classify_change_kind, ChangeKind, CompatibilityChange, CompatibilityClass,
        CompatibilityReport,
    },
    fragments::{
        parse_authoring_fragment, parse_consumers_fragment, parse_http_fragment,
        parse_runtime_fragment, parse_wasm_fragment, parse_websocket_fragment,
        DiscoveredSemanticNode, FragmentSet, SemanticFacet,
    },
    v2::{
        parse_manifest, parse_v2_manifest, to_pretty_json_v2, Alias, AliasKind, AvailabilityStatus,
        AvailabilityV2, Capability, Derivation, Generator, NodeMetadata, ProvenanceAnchor,
        PublicApiManifestV2, Stability, StabilityLevel, UnavailableBehavior,
        VersionedPublicApiManifest, SCHEMA_URI_V2, SCHEMA_VERSION_V2,
    },
    ErrorCode, PublicApiManifest,
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

fn fixture_manifest() -> PublicApiManifestV2 {
    let capability_id = "v1:capability:0000000000000001".to_string();
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
            metadata: NodeMetadata {
                id: capability_id.clone(),
                name: "target.native".into(),
                aliases: vec![Alias {
                    id: "v1:capability:0000000000000002".into(),
                    canonical_id: capability_id,
                    kind: AliasKind::Rename,
                    since: "0.4.0".into(),
                    deprecated_since: "0.5.0".into(),
                    removal_not_before: "1.0.0".into(),
                    warning: "use target.native".into(),
                    behavior_fixture: "schema_v2::stable_alias".into(),
                    compatibility_classes: [CompatibilityClass::BehavioralChange]
                        .into_iter()
                        .collect(),
                }],
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
                    symbol: "fixture_manifest".into(),
                    line: None,
                    derivation: Derivation::BehavioralFixture,
                }],
                test_anchors: Vec::new(),
            },
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
