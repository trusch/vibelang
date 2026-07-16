use crate::{
    compatibility::CompatibilityClass, stable_id, Anchor, DuplicateNameHandling, EntryDetails,
    ErrorCode, ManifestError, PublicApiManifest, StdlibDeclaration, UgenInput,
};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_URI_V2: &str = "https://vibelang.org/schemas/public-api-manifest/v2";
pub const SCHEMA_VERSION_V2: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub enum VersionedPublicApiManifest {
    V1(PublicApiManifest),
    V2(PublicApiManifestV2),
}

pub fn parse_manifest(input: &str) -> Result<VersionedPublicApiManifest, ManifestError> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| ManifestError::decode(ErrorCode::JsonDecode, "$", error.to_string()))?;
    match value.get("schema_version").and_then(Value::as_u64) {
        Some(1) => serde_json::from_value(value)
            .map(VersionedPublicApiManifest::V1)
            .map_err(|error| ManifestError::decode(ErrorCode::JsonDecode, "$", error.to_string())),
        Some(2) => {
            let manifest: PublicApiManifestV2 = serde_json::from_value(value).map_err(|error| {
                ManifestError::decode(ErrorCode::JsonDecode, "$", error.to_string())
            })?;
            manifest.validate()?;
            Ok(VersionedPublicApiManifest::V2(manifest))
        }
        Some(version) => Err(ManifestError::new(
            ErrorCode::UnsupportedSchemaVersion,
            "schema_version",
            format!("unsupported public API manifest schema version {version}"),
        )),
        None => Err(ManifestError::new(
            ErrorCode::MissingFacet,
            "schema_version",
            "manifest has no integer schema_version",
        )),
    }
}

pub fn parse_v2_manifest(input: &str) -> Result<PublicApiManifestV2, ManifestError> {
    match parse_manifest(input)? {
        VersionedPublicApiManifest::V2(manifest) => Ok(manifest),
        VersionedPublicApiManifest::V1(_) => Err(ManifestError::new(
            ErrorCode::UnsupportedSchemaVersion,
            "schema_version",
            "expected schema v2 but received schema v1",
        )),
    }
}

pub fn to_pretty_json_v2(manifest: &PublicApiManifestV2) -> Result<String, ManifestError> {
    manifest.validate()?;
    let mut json = serde_json::to_string_pretty(manifest)
        .map_err(|error| ManifestError::new(ErrorCode::JsonDecode, "$", error.to_string()))?;
    json.push('\n');
    Ok(json)
}

pub fn semantic_id(namespace: &str, canonical_key: &str) -> String {
    stable_id(namespace, canonical_key)
}

pub fn validate_stable_id(id: &str, expected_namespace: Option<&str>) -> Result<(), ManifestError> {
    let mut parts = id.split(':');
    let algorithm = parts.next();
    let namespace = parts.next();
    let digest = parts.next();
    let well_formed = algorithm == Some("v1")
        && namespace.is_some_and(|value| {
            !value.is_empty()
                && value.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
                })
        })
        && digest.is_some_and(|value| {
            value.len() == 16
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        && parts.next().is_none();
    if !well_formed || expected_namespace.is_some_and(|expected| namespace != Some(expected)) {
        return Err(ManifestError::new(
            ErrorCode::InvalidStableId,
            id,
            expected_namespace.map_or_else(
                || "stable IDs must be v1:<namespace>:<16 lowercase hex digits>".into(),
                |expected| format!("stable ID must use namespace {expected:?}"),
            ),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicApiManifestV2 {
    pub schema: String,
    pub schema_version: u32,
    pub api_version: String,
    pub generator: Generator,
    pub entries: Vec<ApiEntryV2>,
    pub types: Vec<ApiType>,
    pub operations: Vec<Operation>,
    pub events: Vec<Event>,
    pub capabilities: Vec<Capability>,
    pub consumers: Vec<Consumer>,
    pub coverage: BTreeMap<String, CoverageRecord>,
    pub stats: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generator {
    pub name: String,
    pub format_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeMetadata {
    pub id: String,
    pub name: String,
    pub aliases: Vec<Alias>,
    pub stability: Stability,
    pub availability: AvailabilityV2,
    pub ownership: Ownership,
    pub source_anchors: Vec<ProvenanceAnchor>,
    pub test_anchors: Vec<ProvenanceAnchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alias {
    pub id: String,
    pub canonical_id: String,
    pub kind: AliasKind,
    pub since: String,
    pub deprecated_since: String,
    pub removal_not_before: String,
    pub warning: String,
    pub behavior_fixture: String,
    pub compatibility_classes: BTreeSet<CompatibilityClass>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasKind {
    Rename,
    LegacySpelling,
    Replacement,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ownership {
    pub contract_owner: String,
    pub implementation_owner: String,
    pub consumer_owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stability {
    pub level: StabilityLevel,
    pub since: Option<String>,
    pub deprecated_since: Option<String>,
    pub replacement_id: Option<String>,
    pub reason: Option<String>,
    pub removal_not_before: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StabilityLevel {
    Stable,
    Preview,
    Experimental,
    Deprecated,
    UnsupportedImportable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvailabilityV2 {
    pub status: AvailabilityStatus,
    pub when: Option<CapabilityExpression>,
    pub on_unavailable: UnavailableBehavior,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityStatus {
    Available,
    Conditional,
    Importable,
    Quarantined,
    DocumentationOnly,
    Unavailable,
    Removed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableBehavior {
    Hidden,
    StructuredError,
    LoadError,
    CompletionLabelOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CapabilityExpression {
    All {
        expressions: Vec<CapabilityExpression>,
    },
    Any {
        expressions: Vec<CapabilityExpression>,
    },
    Not {
        expression: Box<CapabilityExpression>,
    },
    Ref {
        capability_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceAnchor {
    pub path: String,
    pub symbol: String,
    pub line: Option<u32>,
    pub derivation: Derivation,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Derivation {
    CompiledMetadata,
    RustAst,
    Catalog,
    StdlibParse,
    ExplicitSemantics,
    GeneratedProjection,
    BehavioralFixture,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "applicability", rename_all = "snake_case", deny_unknown_fields)]
pub enum Facet<T> {
    Applicable { value: T },
    NotApplicable { reason: String },
}

impl<T> Facet<T> {
    pub fn validate(&self, path: &str) -> Result<(), ManifestError> {
        if let Self::NotApplicable { reason } = self {
            if reason.trim().is_empty() {
                return Err(ManifestError::new(
                    ErrorCode::MissingFacet,
                    path,
                    "not_applicable requires a nonempty reason",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiEntryV2 {
    #[serde(flatten)]
    pub metadata: NodeMetadata,
    pub surface: String,
    pub kind: String,
    pub registered_name: String,
    pub receiver: Option<String>,
    pub overloads: Vec<OverloadV2>,
    #[serde(deserialize_with = "deserialize_entry_details_v2")]
    pub details: EntryDetails,
    pub lifecycle: Facet<LifecycleContract>,
    pub operation_ids: Vec<String>,
    pub consumer_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum EntryDetailsV2Wire {
    Rhai {
        callable_identities: Vec<String>,
    },
    RhaiType {
        display_name: String,
    },
    Ugen {
        class: String,
        description: String,
        rate: String,
        runtime_rate: String,
        category: String,
        inputs: Vec<UgenInputV2Wire>,
        outputs: u32,
        emitted_class: String,
        special_index: i16,
        pseudo: bool,
        callable: bool,
        requires_plugin: Option<String>,
        unavailable_reason: Option<String>,
    },
    StdlibDefinition {
        definition_kind: String,
        import_paths: Vec<String>,
        declarations: Vec<StdlibDeclarationV2Wire>,
        duplicate_name: DuplicateNameHandlingV2Wire,
        export_classification: String,
        support_classification: String,
    },
    StdlibFunction {
        import_paths: Vec<String>,
        access: String,
        documentation: Vec<String>,
        declarations: Vec<StdlibDeclarationV2Wire>,
        duplicate_name: DuplicateNameHandlingV2Wire,
        export_classification: String,
        support_classification: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UgenInputV2Wire {
    name: String,
    input_type: String,
    default: Option<Value>,
    description: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StdlibDeclarationV2Wire {
    import_path: String,
    definition_kind: String,
    callable_signature: Option<String>,
    access: String,
    export_classification: String,
    support_classification: String,
    source_anchor: AnchorV2Wire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DuplicateNameHandlingV2Wire {
    status: String,
    declaration_count: u32,
    import_paths: Vec<String>,
    resolution: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnchorV2Wire {
    path: String,
    symbol: String,
    line: Option<u32>,
}

fn deserialize_entry_details_v2<'de, D>(deserializer: D) -> Result<EntryDetails, D::Error>
where
    D: Deserializer<'de>,
{
    EntryDetailsV2Wire::deserialize(deserializer).map(Into::into)
}

impl From<EntryDetailsV2Wire> for EntryDetails {
    fn from(details: EntryDetailsV2Wire) -> Self {
        match details {
            EntryDetailsV2Wire::Rhai {
                callable_identities,
            } => Self::Rhai {
                callable_identities,
            },
            EntryDetailsV2Wire::RhaiType { display_name } => Self::RhaiType { display_name },
            EntryDetailsV2Wire::Ugen {
                class,
                description,
                rate,
                runtime_rate,
                category,
                inputs,
                outputs,
                emitted_class,
                special_index,
                pseudo,
                callable,
                requires_plugin,
                unavailable_reason,
            } => Self::Ugen {
                class,
                description,
                rate,
                runtime_rate,
                category,
                inputs: inputs.into_iter().map(Into::into).collect(),
                outputs,
                emitted_class,
                special_index,
                pseudo,
                callable,
                requires_plugin,
                unavailable_reason,
            },
            EntryDetailsV2Wire::StdlibDefinition {
                definition_kind,
                import_paths,
                declarations,
                duplicate_name,
                export_classification,
                support_classification,
            } => Self::StdlibDefinition {
                definition_kind,
                import_paths,
                declarations: declarations.into_iter().map(Into::into).collect(),
                duplicate_name: duplicate_name.into(),
                export_classification,
                support_classification,
            },
            EntryDetailsV2Wire::StdlibFunction {
                import_paths,
                access,
                documentation,
                declarations,
                duplicate_name,
                export_classification,
                support_classification,
            } => Self::StdlibFunction {
                import_paths,
                access,
                documentation,
                declarations: declarations.into_iter().map(Into::into).collect(),
                duplicate_name: duplicate_name.into(),
                export_classification,
                support_classification,
            },
        }
    }
}

impl From<UgenInputV2Wire> for UgenInput {
    fn from(input: UgenInputV2Wire) -> Self {
        Self {
            name: input.name,
            input_type: input.input_type,
            default: input.default,
            description: input.description,
        }
    }
}

impl From<StdlibDeclarationV2Wire> for StdlibDeclaration {
    fn from(declaration: StdlibDeclarationV2Wire) -> Self {
        Self {
            import_path: declaration.import_path,
            definition_kind: declaration.definition_kind,
            callable_signature: declaration.callable_signature,
            access: declaration.access,
            export_classification: declaration.export_classification,
            support_classification: declaration.support_classification,
            source_anchor: declaration.source_anchor.into(),
        }
    }
}

impl From<DuplicateNameHandlingV2Wire> for DuplicateNameHandling {
    fn from(duplicate: DuplicateNameHandlingV2Wire) -> Self {
        Self {
            status: duplicate.status,
            declaration_count: duplicate.declaration_count,
            import_paths: duplicate.import_paths,
            resolution: duplicate.resolution,
        }
    }
}

impl From<AnchorV2Wire> for Anchor {
    fn from(anchor: AnchorV2Wire) -> Self {
        Self {
            path: anchor.path,
            symbol: anchor.symbol,
            line: anchor.line,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverloadV2 {
    #[serde(flatten)]
    pub metadata: NodeMetadata,
    pub signature: String,
    pub parameters: Vec<ParameterV2>,
    pub return_type: String,
    pub returns_receiver: Option<bool>,
    pub lifecycle: Facet<LifecycleContract>,
    pub value_contract: Facet<ValueContract>,
    pub failure: FailureContract,
    pub operation_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterV2 {
    pub position: u32,
    pub name: Option<String>,
    pub accepted_types: Vec<String>,
    pub optional: bool,
    pub default: Option<Value>,
    pub value_contract: Facet<ValueContract>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleContract {
    pub role: LifecycleRole,
    pub phase: LifecyclePhase,
    pub effects: Vec<LifecycleEffect>,
    pub effect_timing: EffectTiming,
    pub synchronization: Synchronization,
    pub repeat: RepeatSemantics,
    pub cancellation: Facet<CancellationContract>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRole {
    Value,
    Builder,
    Ref,
    Observation,
    LegacyHandle,
    Operation,
    TypeRegistration,
    ModuleDefinition,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhase {
    Construct,
    Configure,
    Validate,
    Register,
    Enqueue,
    Plan,
    Stage,
    Commit,
    Observe,
    Release,
    PureCall,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleEffect {
    Construct,
    Configure,
    Register,
    Start,
    Stop,
    Synchronize,
    Cancel,
    Observe,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectTiming {
    None,
    EvaluationLocal,
    CandidateAcceptance,
    RuntimeQueued,
    RuntimeApplied,
    ImmediateLive,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Atomicity {
    Required,
    BestEffort,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Synchronization {
    None,
    SyncToCandidate,
    RevisionReceipt,
    BackendBarrier,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatSemantics {
    Pure,
    Idempotent,
    Replace,
    DuplicateError,
    AdditionalEffect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancellationContract {
    pub operation_id: String,
    pub latest_phase: LifecyclePhase,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueContract {
    pub semantic_type: String,
    pub unit_id: String,
    pub range: Facet<NumericRange>,
    pub non_finite: NonFinitePolicy,
    pub coercion: Vec<CoercionSource>,
    pub parser: Facet<ParserContract>,
    pub default: Facet<DefaultContract>,
    pub collision: Facet<CollisionContract>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumericRange {
    pub minimum: Option<f64>,
    pub minimum_inclusive: bool,
    pub maximum: Option<f64>,
    pub maximum_inclusive: bool,
    pub provenance: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NonFinitePolicy {
    Reject,
    Allow,
    Clamp,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoercionSource {
    pub source_type: String,
    pub loss: CoercionLoss,
    pub rounding: RoundingPolicy,
    pub wrapping: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoercionLoss {
    Lossless,
    Checked,
    Lossy,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingPolicy {
    None,
    RejectFraction,
    Nearest,
    Floor,
    Ceiling,
    Truncate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParserContract {
    pub mode: ParserMode,
    pub grammar_id: String,
    pub location_behavior: String,
    pub fallback: FallbackPolicy,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserMode {
    Strict,
    Permissive,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackPolicy {
    Reject,
    StructuredDiagnostic,
    LegacyDefault,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultContract {
    pub value: Value,
    pub owner: DefaultOwner,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultOwner {
    Wire,
    Language,
    Handler,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollisionContract {
    pub namespace: String,
    pub duplicate_policy: DuplicatePolicy,
    pub deterministic_resolution: String,
    pub diagnostic_id: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    Reject,
    IdempotentIfEqual,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiType {
    #[serde(flatten)]
    pub metadata: NodeMetadata,
    pub kind: TypeKind,
    pub fields: Vec<Field>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    Record,
    Enum,
    Alias,
    TaggedUnion,
    ErrorEnvelope,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    #[serde(flatten)]
    pub metadata: NodeMetadata,
    pub serialized_name: String,
    pub host_name: String,
    pub direction: FieldDirection,
    pub required: bool,
    pub type_id: String,
    pub default: Option<Value>,
    pub value_contract: Facet<ValueContract>,
    pub operation_applicability: Facet<Vec<String>>,
    pub bindings: Vec<FieldBinding>,
    pub observation: Facet<ObservationContract>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldDirection {
    Input,
    Output,
    Bidirectional,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldBinding {
    pub operation_id: String,
    pub effectiveness: Effectiveness,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effectiveness {
    pub status: EffectivenessStatus,
    pub effect_ids: Vec<String>,
    pub error_ids: Vec<String>,
    pub observable_at: ObservableAt,
    pub migration: Option<MigrationDebt>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectivenessStatus {
    Effective,
    StructuredRejection,
    CompatibilityDebt,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservableAt {
    Desired,
    Applied,
    Telemetry,
    ResponseOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationDebt {
    pub owner: String,
    pub issue: String,
    pub remove_by: String,
    pub diagnostic_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationContract {
    pub authoritative_source: String,
    pub consistency: ConsistencyPoint,
    pub absent_behavior: UnavailableBehavior,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumVariant {
    pub id: String,
    pub serialized_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    #[serde(flatten)]
    pub metadata: NodeMetadata,
    pub kind: OperationKind,
    pub request_type_id: Option<String>,
    pub response_type_ids: Vec<String>,
    pub error_type_id: String,
    pub effects: Vec<Effect>,
    pub idempotency: Idempotency,
    pub effect_timing: Facet<EffectTiming>,
    pub atomicity: Facet<Atomicity>,
    pub revision: Facet<RevisionContract>,
    pub receipt: Facet<ReceiptContract>,
    pub consistency: ConsistencyPoint,
    pub security_capability_ids: Vec<String>,
    pub bindings: Vec<SurfaceBinding>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Read,
    Mutation,
    Evaluation,
    Subscription,
    Telemetry,
    LifecycleControl,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SurfaceBinding {
    #[serde(flatten)]
    pub metadata: NodeMetadata,
    #[serde(flatten)]
    pub details: BindingDetails,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "surface", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingDetails {
    Http {
        method: String,
        path: String,
        path_type_ids: Vec<String>,
        query_type_ids: Vec<String>,
        header_type_ids: Vec<String>,
        body_type_id: Option<String>,
        successes: Vec<HttpSuccess>,
        error_type_id: String,
        protocol_version: String,
        authentication_capability_id: Option<String>,
        idempotency_header: Option<String>,
        revision_header: Option<String>,
    },
    Rhai {
        entry_id: String,
        overload_id: String,
    },
    Wasm {
        class_or_module: String,
        js_name: String,
        asynchronous: bool,
        required_capability_ids: Vec<String>,
        error_type_id: String,
        canonical_package: String,
    },
    Cli {
        binary: String,
        command: Vec<String>,
        defaults: BTreeMap<String, Value>,
        environment_sources: Vec<String>,
        exit_contract: String,
    },
    Websocket {
        action_type: String,
        payload_type_id: String,
        protocol_version: String,
    },
    Editor {
        consumer_id: String,
        command_id: String,
        package_location: String,
    },
}

const NODE_METADATA_FIELDS: &[&str] = &[
    "id",
    "name",
    "aliases",
    "stability",
    "availability",
    "ownership",
    "source_anchors",
    "test_anchors",
];

const SURFACE_BINDING_FIELDS: &[&str] = &[
    "id",
    "name",
    "aliases",
    "stability",
    "availability",
    "ownership",
    "source_anchors",
    "test_anchors",
    "surface",
    "method",
    "path",
    "path_type_ids",
    "query_type_ids",
    "header_type_ids",
    "body_type_id",
    "successes",
    "error_type_id",
    "protocol_version",
    "authentication_capability_id",
    "idempotency_header",
    "revision_header",
    "entry_id",
    "overload_id",
    "class_or_module",
    "js_name",
    "asynchronous",
    "required_capability_ids",
    "canonical_package",
    "binary",
    "command",
    "defaults",
    "environment_sources",
    "exit_contract",
    "action_type",
    "payload_type_id",
    "consumer_id",
    "command_id",
    "package_location",
];

impl<'de> Deserialize<'de> for SurfaceBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut fields = BTreeMap::<String, Value>::deserialize(deserializer)?;
        if let Some(field) = fields
            .keys()
            .find(|field| !SURFACE_BINDING_FIELDS.contains(&field.as_str()))
        {
            return Err(D::Error::unknown_field(field, SURFACE_BINDING_FIELDS));
        }

        let mut metadata_fields = serde_json::Map::new();
        for field in NODE_METADATA_FIELDS {
            if let Some(value) = fields.remove(*field) {
                metadata_fields.insert((*field).into(), value);
            }
        }
        let metadata =
            serde_json::from_value(Value::Object(metadata_fields)).map_err(D::Error::custom)?;
        let details = serde_json::from_value(Value::Object(fields.into_iter().collect()))
            .map_err(D::Error::custom)?;
        Ok(Self { metadata, details })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpSuccess {
    pub status: u16,
    pub type_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub id: String,
    pub kind: LifecycleEffect,
    pub target_id: String,
    pub behavior: EffectBehavior,
    pub observable_result: String,
    pub preconditions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectBehavior {
    Merge,
    Replace,
    Add,
    Remove,
    External,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Idempotency {
    Yes,
    No,
    Conditional,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionContract {
    pub mode: RevisionMode,
    pub acceptance: Vec<AcceptanceState>,
    pub terminals: BTreeSet<TerminalOutcome>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionMode {
    None,
    Allocates,
    Observes,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceState {
    Rejected,
    Accepted,
    Terminal,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcome {
    Rejected,
    Superseded,
    Applied,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptContract {
    pub attempt_id_type: String,
    pub runtime_epoch_type: String,
    pub revision_id_type: String,
    pub event_sequence_type: String,
    pub terminal_outcomes: BTreeSet<TerminalOutcome>,
    pub component_outcomes: Vec<ComponentOutcome>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentOutcome {
    Applied,
    Failed,
    Uncertain,
    NotStarted,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencyPoint {
    ResponseSnapshot,
    DesiredState,
    AppliedState,
    TelemetrySample,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureContract {
    pub stages: Vec<FailureStage>,
    pub error_ids: Vec<String>,
    pub retryable: bool,
    pub prior_state: PriorState,
    pub fallback: FallbackPolicy,
    pub panic_exposure: PanicExposure,
    pub delivery: BTreeSet<FailureDelivery>,
    pub cleanup_owner: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureStage {
    Decode,
    Parse,
    Validate,
    Admission,
    Plan,
    Stage,
    Commit,
    Observe,
    Cleanup,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorState {
    Unchanged,
    PartiallyChanged,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanicExposure {
    None,
    Present,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureDelivery {
    Return,
    Receipt,
    Event,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    #[serde(flatten)]
    pub metadata: NodeMetadata,
    pub payload_type_id: String,
    pub producer_operation_id: Option<String>,
    pub protocol_version: String,
    pub ordering: EventOrdering,
    pub revision_relation: RevisionRelation,
    pub delivery: EventDelivery,
    pub loss_detection: LossDetection,
    pub resync_operation_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventOrdering {
    ObservationSequence,
    Unordered,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionRelation {
    AppliedRevision,
    AcceptedRevision,
    TelemetryOnly,
    None,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventDelivery {
    AtLeastOnce,
    BestEffort,
    PollDerived,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LossDetection {
    SequenceGap,
    ResetRequired,
    NotSupported,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    #[serde(flatten)]
    pub metadata: NodeMetadata,
    pub detection_source: String,
    pub dependencies: Vec<String>,
    pub conflicts: Vec<String>,
    pub runtime_states: BTreeSet<CapabilityState>,
    pub projection_rules: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Available,
    Degraded,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consumer {
    #[serde(flatten)]
    pub metadata: NodeMetadata,
    pub source_projections: Vec<String>,
    pub eligibility: Eligibility,
    pub included_ids: Vec<String>,
    pub exclusions: Vec<ConsumerExclusion>,
    pub host: Facet<WasmHostSemantics>,
    pub coverage_policy: Facet<CoveragePolicy>,
    pub package: Facet<PackageContract>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmHostSemantics {
    #[serde(default)]
    pub capability_ids: Vec<String>,
    #[serde(default)]
    pub required_globals: Vec<String>,
    pub progress: WasmProgress,
    pub canonical_package_owner: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WasmProgress {
    HostTick,
    WorkletAcknowledgement,
    Synchronous,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoveragePolicy {
    pub require_complete_eligibility: bool,
    pub allow_curated_exclusions: bool,
    pub forbid_denominator_shrink: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Eligibility {
    pub surfaces: Vec<String>,
    pub kinds: Vec<String>,
    pub stability_levels: BTreeSet<StabilityLevel>,
    pub capability_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerExclusion {
    pub id: String,
    pub reason: ExclusionReason,
    pub owner: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    IntentionalCuration,
    UnsupportedHost,
    Deprecated,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageContract {
    pub name: String,
    pub version_owner: String,
    pub required_members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageRecord {
    pub numerator: u64,
    pub denominator: u64,
    pub exclusions_by_reason: BTreeMap<String, u64>,
    pub unresolved_ids: Vec<String>,
    pub stale_ids: Vec<String>,
    pub base_denominator: u64,
}

impl PublicApiManifestV2 {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema != SCHEMA_URI_V2 || self.schema_version != SCHEMA_VERSION_V2 {
            return Err(ManifestError::new(
                ErrorCode::InvalidSchema,
                "schema",
                format!(
                    "schema v2 requires {SCHEMA_URI_V2:?} and schema_version {SCHEMA_VERSION_V2}"
                ),
            ));
        }
        if self.api_version.trim().is_empty()
            || self.generator.name.trim().is_empty()
            || self.generator.format_version == 0
        {
            return Err(ManifestError::new(
                ErrorCode::MissingFacet,
                "generator",
                "api_version, generator name, and nonzero format_version are required",
            ));
        }

        ensure_sorted("entries", &self.entries, |entry| &entry.metadata.id)?;
        ensure_sorted("types", &self.types, |value| &value.metadata.id)?;
        ensure_sorted("operations", &self.operations, |value| &value.metadata.id)?;
        ensure_sorted("events", &self.events, |value| &value.metadata.id)?;
        ensure_sorted("capabilities", &self.capabilities, |value| {
            &value.metadata.id
        })?;
        ensure_sorted("consumers", &self.consumers, |value| &value.metadata.id)?;

        let mut ids = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        for entry in &self.entries {
            validate_metadata(&entry.metadata, "entry", &mut ids, &mut aliases)?;
            entry.lifecycle.validate(&entry.metadata.id)?;
            ensure_sorted("overloads", &entry.overloads, |value| &value.metadata.id)?;
            for overload in &entry.overloads {
                validate_metadata(&overload.metadata, "overload", &mut ids, &mut aliases)?;
                overload.lifecycle.validate(&overload.metadata.id)?;
                overload.value_contract.validate(&overload.metadata.id)?;
                validate_failure(&overload.failure, &overload.metadata.id)?;
                for (position, parameter) in overload.parameters.iter().enumerate() {
                    if parameter.position as usize != position {
                        return Err(ManifestError::new(
                            ErrorCode::NonDeterministicOrder,
                            overload.metadata.id.clone(),
                            "overload parameters must be in exact positional order",
                        ));
                    }
                    parameter.value_contract.validate(&overload.metadata.id)?;
                }
            }
        }
        for api_type in &self.types {
            validate_metadata(&api_type.metadata, "type", &mut ids, &mut aliases)?;
            ensure_sorted("fields", &api_type.fields, |value| &value.metadata.id)?;
            for field in &api_type.fields {
                validate_metadata(&field.metadata, "field", &mut ids, &mut aliases)?;
                field.value_contract.validate(&field.metadata.id)?;
                field.operation_applicability.validate(&field.metadata.id)?;
                field.observation.validate(&field.metadata.id)?;
                ensure_sorted("field bindings", &field.bindings, |binding| {
                    &binding.operation_id
                })?;
                let mut operation_ids = BTreeSet::new();
                for binding in &field.bindings {
                    if !operation_ids.insert(&binding.operation_id) {
                        return Err(ManifestError::new(
                            ErrorCode::DuplicateId,
                            field.metadata.id.clone(),
                            "a field may bind to an operation only once",
                        ));
                    }
                    validate_effectiveness(&binding.effectiveness, &field.metadata.id)?;
                }
                match &field.operation_applicability {
                    Facet::Applicable { value }
                        if value.is_empty()
                            || value.iter().collect::<BTreeSet<_>>().len() != value.len()
                            || value
                                != &field
                                    .bindings
                                    .iter()
                                    .map(|binding| binding.operation_id.clone())
                                    .collect::<Vec<_>>() =>
                    {
                        return Err(ManifestError::new(
                            ErrorCode::InvalidValue,
                            field.metadata.id.clone(),
                            "applicable field operation IDs must be nonempty, unique, sorted, and match bindings exactly",
                        ));
                    }
                    Facet::NotApplicable { .. } if !field.bindings.is_empty() => {
                        return Err(ManifestError::new(
                            ErrorCode::InvalidValue,
                            field.metadata.id.clone(),
                            "not-applicable fields cannot carry operation bindings",
                        ));
                    }
                    _ => {}
                }
            }
            ensure_sorted("variants", &api_type.variants, |value| &value.id)?;
            for variant in &api_type.variants {
                validate_unique_id(&variant.id, "variant", &mut ids)?;
            }
        }
        for operation in &self.operations {
            validate_metadata(&operation.metadata, "operation", &mut ids, &mut aliases)?;
            operation.effect_timing.validate(&operation.metadata.id)?;
            operation.atomicity.validate(&operation.metadata.id)?;
            operation.revision.validate(&operation.metadata.id)?;
            operation.receipt.validate(&operation.metadata.id)?;
            if let Facet::Applicable { value } = &operation.revision {
                validate_revision(value, &operation.metadata.id)?;
            }
            if let Facet::Applicable { value } = &operation.receipt {
                validate_receipt(value, &operation.metadata.id)?;
            }
            if matches!(
                operation.kind,
                OperationKind::Mutation
                    | OperationKind::Evaluation
                    | OperationKind::LifecycleControl
            ) && operation.metadata.test_anchors.is_empty()
            {
                return Err(ManifestError::new(
                    ErrorCode::MissingFacet,
                    operation.metadata.id.clone(),
                    "effective mutations require a behavioral test anchor",
                ));
            }
            for effect in &operation.effects {
                validate_unique_id(&effect.id, "effect", &mut ids)?;
            }
            ensure_sorted("bindings", &operation.bindings, |value| &value.metadata.id)?;
            let mut has_http_binding = false;
            let mut http_authentication_capability_ids = BTreeSet::new();
            for binding in &operation.bindings {
                validate_metadata(&binding.metadata, "binding", &mut ids, &mut aliases)?;
                if let BindingDetails::Http {
                    path_type_ids,
                    query_type_ids,
                    header_type_ids,
                    body_type_id,
                    successes,
                    error_type_id,
                    authentication_capability_id,
                    ..
                } = &binding.details
                {
                    has_http_binding = true;
                    if let Some(id) = authentication_capability_id {
                        http_authentication_capability_ids.insert(id);
                    }
                    if successes.is_empty() {
                        return Err(ManifestError::new(
                            ErrorCode::MissingFacet,
                            binding.metadata.id.clone(),
                            "HTTP bindings require at least one success status and shape",
                        ));
                    }
                    let request_ids = path_type_ids
                        .iter()
                        .chain(query_type_ids)
                        .chain(header_type_ids)
                        .chain(body_type_id)
                        .collect::<BTreeSet<_>>();
                    if request_ids.is_empty() != operation.request_type_id.is_none()
                        || operation
                            .request_type_id
                            .as_ref()
                            .is_some_and(|id| !request_ids.contains(id))
                    {
                        return Err(ManifestError::new(
                            ErrorCode::MissingFacet,
                            binding.metadata.id.clone(),
                            "HTTP request extractors and operation request_type_id must be connected",
                        ));
                    }
                    let success_ids = successes
                        .iter()
                        .map(|success| &success.type_id)
                        .collect::<BTreeSet<_>>();
                    let success_pairs = successes
                        .iter()
                        .map(|success| (success.status, &success.type_id))
                        .collect::<BTreeSet<_>>();
                    if success_pairs.len() != successes.len()
                        || operation.response_type_ids.iter().collect::<BTreeSet<_>>()
                            != success_ids
                    {
                        return Err(ManifestError::new(
                            ErrorCode::MissingFacet,
                            binding.metadata.id.clone(),
                            "HTTP success shapes and operation response_type_ids must match exactly",
                        ));
                    }
                    if error_type_id != &operation.error_type_id {
                        return Err(ManifestError::new(
                            ErrorCode::InvalidValue,
                            binding.metadata.id.clone(),
                            "HTTP binding and operation error types must match",
                        ));
                    }
                    if authentication_capability_id.as_ref().is_some_and(|id| {
                        !operation
                            .security_capability_ids
                            .iter()
                            .any(|security| security == id)
                    }) {
                        return Err(ManifestError::new(
                            ErrorCode::InvalidValue,
                            binding.metadata.id.clone(),
                            "HTTP authentication capability must be an operation security capability",
                        ));
                    }
                }
            }
            let security_capability_ids = operation
                .security_capability_ids
                .iter()
                .collect::<BTreeSet<_>>();
            if has_http_binding
                && (security_capability_ids.len() != operation.security_capability_ids.len()
                    || security_capability_ids != http_authentication_capability_ids)
            {
                return Err(ManifestError::new(
                    ErrorCode::InvalidValue,
                    operation.metadata.id.clone(),
                    "HTTP operation security capabilities must match binding authentication capabilities exactly",
                ));
            }
        }
        for event in &self.events {
            validate_metadata(&event.metadata, "event", &mut ids, &mut aliases)?;
            if !matches!(event.loss_detection, LossDetection::NotApplicable)
                && event.resync_operation_id.is_none()
            {
                return Err(ManifestError::new(
                    ErrorCode::MissingFacet,
                    event.metadata.id.clone(),
                    "loss-detecting events require a resync operation",
                ));
            }
        }
        for capability in &self.capabilities {
            validate_metadata(&capability.metadata, "capability", &mut ids, &mut aliases)?;
            if capability.runtime_states.is_empty() {
                return Err(ManifestError::new(
                    ErrorCode::MissingFacet,
                    capability.metadata.id.clone(),
                    "capabilities require explicit runtime states",
                ));
            }
        }
        for consumer in &self.consumers {
            validate_metadata(&consumer.metadata, "consumer", &mut ids, &mut aliases)?;
            consumer.host.validate(&consumer.metadata.id)?;
            consumer.coverage_policy.validate(&consumer.metadata.id)?;
            consumer.package.validate(&consumer.metadata.id)?;
            if let Facet::Applicable { value } = &consumer.host {
                if value.required_globals.is_empty()
                    || value.canonical_package_owner.trim().is_empty()
                {
                    return Err(ManifestError::new(
                        ErrorCode::MissingFacet,
                        consumer.metadata.id.clone(),
                        "applicable WASM host semantics require a global and canonical package owner",
                    ));
                }
            }
            for exclusion in &consumer.exclusions {
                if exclusion.owner.trim().is_empty() {
                    return Err(ManifestError::new(
                        ErrorCode::MissingFacet,
                        consumer.metadata.id.clone(),
                        "consumer exclusions require an owner",
                    ));
                }
            }
        }
        for alias in aliases {
            if ids.contains(alias) {
                return Err(ManifestError::new(
                    ErrorCode::AliasConflict,
                    alias,
                    "an alias ID may not be the canonical ID of another node",
                ));
            }
        }

        self.validate_references(&ids)?;
        for (consumer_id, coverage) in &self.coverage {
            if !ids.contains(consumer_id.as_str()) {
                return Err(invalid_reference("coverage", consumer_id));
            }
            let classified_denominator = coverage
                .exclusions_by_reason
                .values()
                .try_fold(coverage.numerator, |total, count| total.checked_add(*count))
                .and_then(|value| value.checked_add(coverage.unresolved_ids.len() as u64));
            if coverage.numerator > coverage.denominator
                || coverage.base_denominator == 0
                || coverage.denominator < coverage.base_denominator
                || classified_denominator != Some(coverage.denominator)
            {
                return Err(ManifestError::new(
                    ErrorCode::InvalidValue,
                    consumer_id,
                    "coverage must satisfy numerator + exclusions + unresolved = denominator and cannot silently shrink its nonzero accepted base denominator",
                ));
            }
        }
        Ok(())
    }

    fn validate_references(&self, ids: &BTreeSet<&str>) -> Result<(), ManifestError> {
        let require = |path: &str, id: &str| {
            if ids.contains(id) {
                Ok(())
            } else {
                Err(invalid_reference(path, id))
            }
        };
        let capability_ids: BTreeSet<_> = self
            .capabilities
            .iter()
            .map(|capability| capability.metadata.id.as_str())
            .collect();
        self.visit_metadata(|metadata| {
            if let Some(expression) = &metadata.availability.when {
                validate_capability_expression(expression, &capability_ids, &metadata.id)?;
            }
            Ok(())
        })?;
        for entry in &self.entries {
            for id in entry.operation_ids.iter().chain(&entry.consumer_ids) {
                require(&entry.metadata.id, id)?;
            }
            for overload in &entry.overloads {
                for id in &overload.operation_ids {
                    require(&overload.metadata.id, id)?;
                }
            }
        }
        for api_type in &self.types {
            for field in &api_type.fields {
                require(&field.metadata.id, &field.type_id)?;
                for binding in &field.bindings {
                    require(&field.metadata.id, &binding.operation_id)?;
                }
            }
        }
        for operation in &self.operations {
            if let Some(id) = &operation.request_type_id {
                require(&operation.metadata.id, id)?;
            }
            for id in &operation.response_type_ids {
                require(&operation.metadata.id, id)?;
            }
            require(&operation.metadata.id, &operation.error_type_id)?;
            for id in &operation.security_capability_ids {
                require(&operation.metadata.id, id)?;
            }
            for effect in &operation.effects {
                require(&operation.metadata.id, &effect.target_id)?;
            }
            for binding in &operation.bindings {
                match &binding.details {
                    BindingDetails::Http {
                        path_type_ids,
                        query_type_ids,
                        header_type_ids,
                        body_type_id,
                        successes,
                        error_type_id,
                        authentication_capability_id,
                        ..
                    } => {
                        for id in path_type_ids
                            .iter()
                            .chain(query_type_ids)
                            .chain(header_type_ids)
                            .chain(body_type_id)
                            .chain(successes.iter().map(|value| &value.type_id))
                            .chain(std::iter::once(error_type_id))
                            .chain(authentication_capability_id)
                        {
                            require(&binding.metadata.id, id)?;
                        }
                    }
                    BindingDetails::Rhai {
                        entry_id,
                        overload_id,
                    } => {
                        require(&binding.metadata.id, entry_id)?;
                        require(&binding.metadata.id, overload_id)?;
                    }
                    BindingDetails::Wasm {
                        required_capability_ids,
                        error_type_id,
                        ..
                    } => {
                        for id in required_capability_ids.iter().chain([error_type_id]) {
                            require(&binding.metadata.id, id)?;
                        }
                    }
                    BindingDetails::Websocket {
                        payload_type_id, ..
                    } => require(&binding.metadata.id, payload_type_id)?,
                    BindingDetails::Editor { consumer_id, .. } => {
                        require(&binding.metadata.id, consumer_id)?;
                    }
                    BindingDetails::Cli { .. } => {}
                }
            }
        }
        for event in &self.events {
            require(&event.metadata.id, &event.payload_type_id)?;
            if let Some(id) = &event.producer_operation_id {
                require(&event.metadata.id, id)?;
            }
            if let Some(id) = &event.resync_operation_id {
                require(&event.metadata.id, id)?;
            }
        }
        for capability in &self.capabilities {
            for id in capability.dependencies.iter().chain(&capability.conflicts) {
                require(&capability.metadata.id, id)?;
            }
        }
        for consumer in &self.consumers {
            for id in consumer
                .included_ids
                .iter()
                .chain(&consumer.eligibility.capability_ids)
            {
                require(&consumer.metadata.id, id)?;
            }
        }
        Ok(())
    }

    fn visit_metadata<F>(&self, mut visit: F) -> Result<(), ManifestError>
    where
        F: FnMut(&NodeMetadata) -> Result<(), ManifestError>,
    {
        for entry in &self.entries {
            visit(&entry.metadata)?;
            for overload in &entry.overloads {
                visit(&overload.metadata)?;
            }
        }
        for api_type in &self.types {
            visit(&api_type.metadata)?;
            for field in &api_type.fields {
                visit(&field.metadata)?;
            }
        }
        for operation in &self.operations {
            visit(&operation.metadata)?;
            for binding in &operation.bindings {
                visit(&binding.metadata)?;
            }
        }
        for event in &self.events {
            visit(&event.metadata)?;
        }
        for capability in &self.capabilities {
            visit(&capability.metadata)?;
        }
        for consumer in &self.consumers {
            visit(&consumer.metadata)?;
        }
        Ok(())
    }
}

fn validate_metadata<'a>(
    metadata: &'a NodeMetadata,
    namespace: &str,
    ids: &mut BTreeSet<&'a str>,
    aliases: &mut BTreeSet<&'a str>,
) -> Result<(), ManifestError> {
    validate_unique_id(&metadata.id, namespace, ids)?;
    if metadata.name.trim().is_empty()
        || metadata.ownership.contract_owner.trim().is_empty()
        || metadata.ownership.implementation_owner.trim().is_empty()
        || metadata.source_anchors.is_empty()
    {
        return Err(ManifestError::new(
            ErrorCode::MissingFacet,
            metadata.id.clone(),
            "name, ownership, and source provenance are required",
        ));
    }
    validate_stability(&metadata.stability, &metadata.id)?;
    if metadata.availability.status == AvailabilityStatus::Conditional
        && metadata.availability.when.is_none()
    {
        return Err(ManifestError::new(
            ErrorCode::MissingFacet,
            metadata.id.clone(),
            "conditional availability requires a capability expression",
        ));
    }
    ensure_sorted("aliases", &metadata.aliases, |alias| &alias.id)?;
    for alias in &metadata.aliases {
        validate_alias(alias, &metadata.id, namespace)?;
        if !aliases.insert(&alias.id) {
            return Err(ManifestError::new(
                ErrorCode::AliasConflict,
                alias.id.clone(),
                "alias ID is claimed more than once",
            ));
        }
    }
    Ok(())
}

fn validate_unique_id<'a>(
    id: &'a str,
    namespace: &str,
    ids: &mut BTreeSet<&'a str>,
) -> Result<(), ManifestError> {
    validate_stable_id(id, Some(namespace))?;
    if !ids.insert(id) {
        return Err(ManifestError::new(
            ErrorCode::DuplicateId,
            id,
            "stable ID is used by more than one node",
        ));
    }
    Ok(())
}

pub(crate) fn validate_alias(
    alias: &Alias,
    canonical_id: &str,
    namespace: &str,
) -> Result<(), ManifestError> {
    validate_stable_id(&alias.id, Some(namespace))?;
    if alias.canonical_id != canonical_id
        || alias.id == canonical_id
        || alias.since.trim().is_empty()
        || alias.deprecated_since.trim().is_empty()
        || alias.removal_not_before.trim().is_empty()
        || alias.warning.trim().is_empty()
        || alias.behavior_fixture.trim().is_empty()
        || alias.compatibility_classes.is_empty()
        || alias
            .compatibility_classes
            .contains(&CompatibilityClass::Unclassified)
    {
        return Err(ManifestError::new(
            ErrorCode::AliasConflict,
            alias.id.clone(),
            "aliases require a distinct ID, matching canonical target, migration metadata, fixture, and classified compatibility",
        ));
    }
    Ok(())
}

pub(crate) fn validate_stability(stability: &Stability, path: &str) -> Result<(), ManifestError> {
    match stability.level {
        StabilityLevel::Stable | StabilityLevel::Preview | StabilityLevel::Experimental
            if stability.since.as_deref().is_none_or(str::is_empty) =>
        {
            Err(ManifestError::new(
                ErrorCode::MissingFacet,
                path,
                "stable, preview, and experimental nodes require since",
            ))
        }
        StabilityLevel::Deprecated
            if stability
                .deprecated_since
                .as_deref()
                .is_none_or(str::is_empty)
                || stability
                    .removal_not_before
                    .as_deref()
                    .is_none_or(str::is_empty)
                || (stability.replacement_id.is_none()
                    && stability.reason.as_deref().is_none_or(str::is_empty)) =>
        {
            Err(ManifestError::new(
                ErrorCode::MissingFacet,
                path,
                "deprecated nodes require deprecation/removal metadata and a replacement or reason",
            ))
        }
        _ => Ok(()),
    }
}

pub(crate) fn validate_failure(failure: &FailureContract, path: &str) -> Result<(), ManifestError> {
    if failure.stages.is_empty() || failure.delivery.is_empty() {
        return Err(ManifestError::new(
            ErrorCode::MissingFacet,
            path,
            "failure stage and caller-visible delivery are required",
        ));
    }
    Ok(())
}

pub(crate) fn validate_effectiveness(
    effectiveness: &Effectiveness,
    path: &str,
) -> Result<(), ManifestError> {
    match effectiveness.status {
        EffectivenessStatus::Effective if effectiveness.effect_ids.is_empty() => {
            Err(ManifestError::new(
                ErrorCode::MissingFacet,
                path,
                "effective bindings require an observable effect ID",
            ))
        }
        EffectivenessStatus::StructuredRejection if effectiveness.error_ids.is_empty() => {
            Err(ManifestError::new(
                ErrorCode::MissingFacet,
                path,
                "structured rejection requires a reachable error ID",
            ))
        }
        EffectivenessStatus::CompatibilityDebt if effectiveness.migration.is_none() => {
            Err(ManifestError::new(
                ErrorCode::MissingFacet,
                path,
                "compatibility debt requires an owner, issue, diagnostic, and removal gate",
            ))
        }
        EffectivenessStatus::Effective | EffectivenessStatus::StructuredRejection
            if effectiveness.migration.is_some() =>
        {
            Err(ManifestError::new(
                ErrorCode::InvalidValue,
                path,
                "only compatibility debt may carry migration metadata",
            ))
        }
        _ => Ok(()),
    }
}

pub(crate) fn validate_revision(
    revision: &RevisionContract,
    path: &str,
) -> Result<(), ManifestError> {
    match revision.mode {
        RevisionMode::Allocates
            if revision.acceptance.is_empty()
                || revision.terminals != expected_terminal_outcomes() =>
        {
            Err(ManifestError::new(
                ErrorCode::MissingFacet,
                path,
                "revision-allocating operations require acceptance states and exactly rejected/superseded/applied/partial terminals",
            ))
        }
        RevisionMode::None if !revision.terminals.is_empty() => Err(ManifestError::new(
            ErrorCode::InvalidValue,
            path,
            "revision.mode=none cannot declare revision terminals",
        )),
        _ => Ok(()),
    }
}

pub(crate) fn validate_receipt(receipt: &ReceiptContract, path: &str) -> Result<(), ManifestError> {
    if receipt.attempt_id_type.trim().is_empty()
        || receipt.runtime_epoch_type.trim().is_empty()
        || receipt.revision_id_type.trim().is_empty()
        || receipt.event_sequence_type.trim().is_empty()
        || receipt.terminal_outcomes != expected_terminal_outcomes()
    {
        return Err(ManifestError::new(
            ErrorCode::MissingFacet,
            path,
            "receipts require all canonical ID/counter types and exactly rejected/superseded/applied/partial terminals",
        ));
    }
    Ok(())
}

fn expected_terminal_outcomes() -> BTreeSet<TerminalOutcome> {
    [
        TerminalOutcome::Rejected,
        TerminalOutcome::Superseded,
        TerminalOutcome::Applied,
        TerminalOutcome::Partial,
    ]
    .into_iter()
    .collect()
}

pub(crate) fn validate_capability_expression(
    expression: &CapabilityExpression,
    ids: &BTreeSet<&str>,
    path: &str,
) -> Result<(), ManifestError> {
    match expression {
        CapabilityExpression::All { expressions } | CapabilityExpression::Any { expressions } => {
            if expressions.is_empty() {
                return Err(ManifestError::new(
                    ErrorCode::MissingFacet,
                    path,
                    "all/any capability expressions require operands",
                ));
            }
            for expression in expressions {
                validate_capability_expression(expression, ids, path)?;
            }
        }
        CapabilityExpression::Not { expression } => {
            validate_capability_expression(expression, ids, path)?;
        }
        CapabilityExpression::Ref { capability_id } if !ids.contains(capability_id.as_str()) => {
            return Err(invalid_reference(path, capability_id));
        }
        CapabilityExpression::Ref { .. } => {}
    }
    Ok(())
}

fn invalid_reference(path: &str, id: &str) -> ManifestError {
    ManifestError::new(
        ErrorCode::InvalidReference,
        path,
        format!("reference {id:?} does not resolve to a contract node"),
    )
}

fn ensure_sorted<T, F>(path: &str, values: &[T], id: F) -> Result<(), ManifestError>
where
    F: Fn(&T) -> &str,
{
    if values
        .windows(2)
        .any(|window| id(&window[0]) >= id(&window[1]))
    {
        return Err(ManifestError::new(
            ErrorCode::NonDeterministicOrder,
            path,
            "contract arrays must be strictly sorted by stable ID",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_semantic_ids_use_the_frozen_algorithm_namespace() {
        assert_eq!(
            semantic_id("operation", "runtime|group.apply"),
            "v1:operation:01c53cb2a68de66e"
        );
    }

    #[test]
    fn stable_id_shape_is_strict() {
        assert!(validate_stable_id("v1:field:0123456789abcdef", Some("field")).is_ok());
        for invalid in [
            "v2:field:0123456789abcdef",
            "v1:Field:0123456789abcdef",
            "v1:field:0123456789ABCDEf",
            "v1:field:abc",
        ] {
            assert!(validate_stable_id(invalid, None).is_err(), "{invalid}");
        }
    }
}
