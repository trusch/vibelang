pub use crate::v2::{Atomicity, CoveragePolicy, WasmHostSemantics, WasmProgress};
use crate::{
    v2::{
        Alias, AvailabilityStatus, AvailabilityV2, CapabilityState, ConsistencyPoint, Effect,
        EffectTiming, Effectiveness, EventDelivery, EventOrdering, Facet, FailureContract,
        Idempotency, LifecycleContract, LossDetection, OperationKind, ReceiptContract,
        RevisionContract, RevisionRelation, Stability, ValueContract,
    },
    ErrorCode, ManifestError,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const FRAGMENT_SCHEMA_URI: &str =
    "https://vibelang.org/schemas/public-api-semantic-fragment/v1";
pub const FRAGMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FragmentDomain {
    Authoring,
    Runtime,
    Http,
    Websocket,
    Wasm,
    Consumers,
}

impl FragmentDomain {
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Authoring => "authoring.toml",
            Self::Runtime => "runtime.toml",
            Self::Http => "http.toml",
            Self::Websocket => "websocket.toml",
            Self::Wasm => "wasm.toml",
            Self::Consumers => "consumers.toml",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FragmentHeader {
    pub fragment_schema: String,
    pub fragment_version: u32,
    pub domain: FragmentDomain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringFragment {
    #[serde(flatten)]
    pub header: FragmentHeader,
    pub records: Vec<AuthoringRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringRecord {
    pub target_id: String,
    pub owner: String,
    #[serde(default)]
    pub aliases: Vec<Alias>,
    pub stability: Option<Stability>,
    pub availability: Option<AvailabilityV2>,
    pub lifecycle: Option<LifecycleContract>,
    pub value_contract: Option<ValueContract>,
    pub failure: Option<FailureContract>,
    #[serde(default)]
    pub operation_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFragment {
    #[serde(flatten)]
    pub header: FragmentHeader,
    pub records: Vec<RuntimeRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRecord {
    pub target_id: String,
    pub owner: String,
    pub operation: Option<RuntimeOperationSemantics>,
    pub revision: Option<RevisionContract>,
    pub receipt: Option<ReceiptContract>,
    pub failure: Option<FailureContract>,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeOperationSemantics {
    pub kind: OperationKind,
    pub idempotency: Idempotency,
    pub consistency: ConsistencyPoint,
    pub effect_timing: EffectTiming,
    pub atomicity: Atomicity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpFragment {
    #[serde(flatten)]
    pub header: FragmentHeader,
    pub records: Vec<HttpRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpRecord {
    pub target_id: String,
    pub owner: String,
    pub operation_id: Option<String>,
    pub effectiveness: Option<Effectiveness>,
    pub consistency: Option<ConsistencyPoint>,
    pub failure: Option<FailureContract>,
    #[serde(default)]
    pub security_capability_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebsocketFragment {
    #[serde(flatten)]
    pub header: FragmentHeader,
    pub records: Vec<WebsocketRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebsocketRecord {
    pub target_id: String,
    pub owner: String,
    pub operation_id: Option<String>,
    pub event: Option<WebsocketEventSemantics>,
    pub failure: Option<FailureContract>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebsocketEventSemantics {
    pub ordering: EventOrdering,
    pub revision_relation: RevisionRelation,
    pub delivery: EventDelivery,
    pub loss_detection: LossDetection,
    pub resync_operation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmFragment {
    #[serde(flatten)]
    pub header: FragmentHeader,
    pub records: Vec<WasmRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmRecord {
    pub target_id: String,
    pub owner: String,
    pub operation_id: Option<String>,
    pub host: Option<WasmHostSemantics>,
    pub stability: Option<Stability>,
    pub failure: Option<FailureContract>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumersFragment {
    #[serde(flatten)]
    pub header: FragmentHeader,
    pub records: Vec<ConsumerRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerRecord {
    pub target_id: String,
    pub owner: String,
    pub policy: Option<ConsumerPolicy>,
    pub coverage: Option<CoveragePolicy>,
    #[serde(default)]
    pub exclusions: Vec<SemanticExclusion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerPolicy {
    #[serde(default)]
    pub surfaces: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub capability_ids: Vec<String>,
    pub include_preview: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExclusion {
    pub target_id: String,
    pub reason: String,
    pub owner: String,
}

pub fn parse_authoring_fragment(input: &str) -> Result<AuthoringFragment, ManifestError> {
    parse_fragment(input, FragmentDomain::Authoring)
}

pub fn parse_runtime_fragment(input: &str) -> Result<RuntimeFragment, ManifestError> {
    parse_fragment(input, FragmentDomain::Runtime)
}

pub fn parse_http_fragment(input: &str) -> Result<HttpFragment, ManifestError> {
    parse_fragment(input, FragmentDomain::Http)
}

pub fn parse_websocket_fragment(input: &str) -> Result<WebsocketFragment, ManifestError> {
    parse_fragment(input, FragmentDomain::Websocket)
}

pub fn parse_wasm_fragment(input: &str) -> Result<WasmFragment, ManifestError> {
    parse_fragment(input, FragmentDomain::Wasm)
}

pub fn parse_consumers_fragment(input: &str) -> Result<ConsumersFragment, ManifestError> {
    parse_fragment(input, FragmentDomain::Consumers)
}

fn parse_fragment<T>(input: &str, expected: FragmentDomain) -> Result<T, ManifestError>
where
    T: DeserializeOwned + FragmentFile,
{
    let value: toml::Value = toml::from_str(input).map_err(|error| {
        ManifestError::decode(
            ErrorCode::TomlDecode,
            expected.file_name(),
            error.to_string(),
        )
    })?;
    reject_mechanical_facts(&value, expected.file_name())?;
    let fragment: T = value.try_into().map_err(|error: toml::de::Error| {
        ManifestError::decode(
            ErrorCode::TomlDecode,
            expected.file_name(),
            error.to_string(),
        )
    })?;
    fragment.validate_file(expected)?;
    Ok(fragment)
}

trait FragmentFile {
    type Record: SemanticRecord;

    fn header(&self) -> &FragmentHeader;
    fn records(&self) -> &[Self::Record];

    fn validate_file(&self, expected: FragmentDomain) -> Result<(), ManifestError> {
        validate_header(self.header(), expected)?;
        validate_records(self.records(), expected)
    }
}

macro_rules! fragment_file {
    ($fragment:ty, $record:ty) => {
        impl FragmentFile for $fragment {
            type Record = $record;

            fn header(&self) -> &FragmentHeader {
                &self.header
            }

            fn records(&self) -> &[Self::Record] {
                &self.records
            }
        }
    };
}

fragment_file!(AuthoringFragment, AuthoringRecord);
fragment_file!(RuntimeFragment, RuntimeRecord);
fragment_file!(HttpFragment, HttpRecord);
fragment_file!(WebsocketFragment, WebsocketRecord);
fragment_file!(WasmFragment, WasmRecord);
fragment_file!(ConsumersFragment, ConsumerRecord);

fn validate_header(header: &FragmentHeader, expected: FragmentDomain) -> Result<(), ManifestError> {
    if header.fragment_schema != FRAGMENT_SCHEMA_URI
        || header.fragment_version != FRAGMENT_SCHEMA_VERSION
        || header.domain != expected
    {
        return Err(ManifestError::new(
            ErrorCode::InvalidSchema,
            expected.file_name(),
            format!(
                "expected fragment schema {FRAGMENT_SCHEMA_URI:?}, version {FRAGMENT_SCHEMA_VERSION}, domain {expected:?}"
            ),
        ));
    }
    Ok(())
}

fn validate_records<T: SemanticRecord>(
    records: &[T],
    domain: FragmentDomain,
) -> Result<(), ManifestError> {
    let mut previous = None;
    let mut ids = BTreeSet::new();
    for record in records {
        crate::v2::validate_stable_id(record.target_id(), None)?;
        if record.owner().trim().is_empty() {
            return Err(ManifestError::new(
                ErrorCode::MissingFacet,
                record.target_id(),
                "semantic records require an owner",
            ));
        }
        if !ids.insert(record.target_id()) {
            return Err(ManifestError::new(
                ErrorCode::DuplicateId,
                record.target_id(),
                "a fragment may own a target only once",
            ));
        }
        if previous.is_some_and(|value| value >= record.target_id()) {
            return Err(ManifestError::new(
                ErrorCode::NonDeterministicOrder,
                domain.file_name(),
                "semantic records must be strictly sorted by target_id",
            ));
        }
        previous = Some(record.target_id());
        if record.claims().is_empty() {
            return Err(ManifestError::new(
                ErrorCode::MissingFacet,
                record.target_id(),
                "semantic record contributes no behavioral facet",
            ));
        }
        record.validate_semantics()?;
    }
    Ok(())
}

const MECHANICAL_FIELDS: &[&str] = &[
    "name",
    "registered_name",
    "signature",
    "receiver",
    "method",
    "path",
    "serialized_name",
    "host_name",
    "package_path",
    "package_location",
    "js_name",
    "route",
    "wire_type",
    "field_shape",
];

fn reject_mechanical_facts(value: &toml::Value, path: &str) -> Result<(), ManifestError> {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                let nested = format!("{path}.{key}");
                if MECHANICAL_FIELDS.contains(&key.as_str()) {
                    return Err(ManifestError::new(
                        ErrorCode::MechanicalFactRestatement,
                        nested,
                        "semantic fragments may reference discovered IDs but may not restate mechanical declarations",
                    ));
                }
                reject_mechanical_facts(value, &nested)?;
            }
        }
        toml::Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                reject_mechanical_facts(value, &format!("{path}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticFacet {
    Aliases,
    Stability,
    Availability,
    Lifecycle,
    ValueContract,
    Failure,
    OperationBinding,
    Operation,
    Revision,
    Receipt,
    Effects,
    Effectiveness,
    Consistency,
    Security,
    Event,
    WasmHost,
    ConsumerPolicy,
    Coverage,
    Exclusions,
}

trait SemanticRecord {
    fn target_id(&self) -> &str;
    fn owner(&self) -> &str;
    fn claims(&self) -> BTreeSet<SemanticFacet>;
    fn references(&self) -> Vec<&str> {
        Vec::new()
    }
    fn availability(&self) -> Option<&AvailabilityV2> {
        None
    }
    fn validate_semantics(&self) -> Result<(), ManifestError> {
        Ok(())
    }
}

macro_rules! claim_if_some {
    ($set:ident, $value:expr, $facet:expr) => {
        if $value.is_some() {
            $set.insert($facet);
        }
    };
}

impl SemanticRecord for AuthoringRecord {
    fn target_id(&self) -> &str {
        &self.target_id
    }

    fn owner(&self) -> &str {
        &self.owner
    }

    fn claims(&self) -> BTreeSet<SemanticFacet> {
        let mut claims = BTreeSet::new();
        if !self.aliases.is_empty() {
            claims.insert(SemanticFacet::Aliases);
        }
        claim_if_some!(claims, self.stability, SemanticFacet::Stability);
        claim_if_some!(claims, self.availability, SemanticFacet::Availability);
        claim_if_some!(claims, self.lifecycle, SemanticFacet::Lifecycle);
        claim_if_some!(claims, self.value_contract, SemanticFacet::ValueContract);
        claim_if_some!(claims, self.failure, SemanticFacet::Failure);
        if !self.operation_ids.is_empty() {
            claims.insert(SemanticFacet::OperationBinding);
        }
        claims
    }

    fn references(&self) -> Vec<&str> {
        self.operation_ids.iter().map(String::as_str).collect()
    }

    fn availability(&self) -> Option<&AvailabilityV2> {
        self.availability.as_ref()
    }

    fn validate_semantics(&self) -> Result<(), ManifestError> {
        let namespace = self.target_id.split(':').nth(1).ok_or_else(|| {
            ManifestError::new(
                ErrorCode::InvalidStableId,
                &self.target_id,
                "target ID has no namespace",
            )
        })?;
        let mut previous = None;
        let mut alias_ids = BTreeSet::new();
        for alias in &self.aliases {
            if previous.is_some_and(|value| value >= alias.id.as_str())
                || !alias_ids.insert(alias.id.as_str())
            {
                return Err(ManifestError::new(
                    ErrorCode::AliasConflict,
                    &alias.id,
                    "fragment aliases must be unique and strictly sorted",
                ));
            }
            previous = Some(alias.id.as_str());
            crate::v2::validate_alias(alias, &self.target_id, namespace)?;
        }
        if let Some(stability) = &self.stability {
            crate::v2::validate_stability(stability, &self.target_id)?;
        }
        if self.availability.as_ref().is_some_and(|availability| {
            availability.status == AvailabilityStatus::Conditional && availability.when.is_none()
        }) {
            return Err(ManifestError::new(
                ErrorCode::MissingFacet,
                &self.target_id,
                "conditional availability requires a capability expression",
            ));
        }
        if let Some(lifecycle) = &self.lifecycle {
            lifecycle.cancellation.validate(&self.target_id)?;
        }
        if let Some(value) = &self.value_contract {
            value.range.validate(&self.target_id)?;
            value.parser.validate(&self.target_id)?;
            value.default.validate(&self.target_id)?;
            value.collision.validate(&self.target_id)?;
        }
        if let Some(failure) = &self.failure {
            crate::v2::validate_failure(failure, &self.target_id)?;
        }
        Ok(())
    }
}

impl SemanticRecord for RuntimeRecord {
    fn target_id(&self) -> &str {
        &self.target_id
    }

    fn owner(&self) -> &str {
        &self.owner
    }

    fn claims(&self) -> BTreeSet<SemanticFacet> {
        let mut claims = BTreeSet::new();
        claim_if_some!(claims, self.operation, SemanticFacet::Operation);
        claim_if_some!(claims, self.revision, SemanticFacet::Revision);
        claim_if_some!(claims, self.receipt, SemanticFacet::Receipt);
        claim_if_some!(claims, self.failure, SemanticFacet::Failure);
        if !self.effects.is_empty() {
            claims.insert(SemanticFacet::Effects);
        }
        claims
    }

    fn validate_semantics(&self) -> Result<(), ManifestError> {
        if let Some(revision) = &self.revision {
            crate::v2::validate_revision(revision, &self.target_id)?;
        }
        if let Some(receipt) = &self.receipt {
            crate::v2::validate_receipt(receipt, &self.target_id)?;
        }
        if let Some(failure) = &self.failure {
            crate::v2::validate_failure(failure, &self.target_id)?;
        }
        Ok(())
    }
}

impl SemanticRecord for HttpRecord {
    fn target_id(&self) -> &str {
        &self.target_id
    }

    fn owner(&self) -> &str {
        &self.owner
    }

    fn claims(&self) -> BTreeSet<SemanticFacet> {
        let mut claims = BTreeSet::new();
        claim_if_some!(claims, self.operation_id, SemanticFacet::OperationBinding);
        claim_if_some!(claims, self.effectiveness, SemanticFacet::Effectiveness);
        claim_if_some!(claims, self.consistency, SemanticFacet::Consistency);
        claim_if_some!(claims, self.failure, SemanticFacet::Failure);
        if !self.security_capability_ids.is_empty() {
            claims.insert(SemanticFacet::Security);
        }
        claims
    }

    fn references(&self) -> Vec<&str> {
        self.operation_id
            .iter()
            .map(String::as_str)
            .chain(self.security_capability_ids.iter().map(String::as_str))
            .collect()
    }

    fn validate_semantics(&self) -> Result<(), ManifestError> {
        if let Some(effectiveness) = &self.effectiveness {
            crate::v2::validate_effectiveness(effectiveness, &self.target_id)?;
        }
        if let Some(failure) = &self.failure {
            crate::v2::validate_failure(failure, &self.target_id)?;
        }
        Ok(())
    }
}

impl SemanticRecord for WebsocketRecord {
    fn target_id(&self) -> &str {
        &self.target_id
    }

    fn owner(&self) -> &str {
        &self.owner
    }

    fn claims(&self) -> BTreeSet<SemanticFacet> {
        let mut claims = BTreeSet::new();
        claim_if_some!(claims, self.operation_id, SemanticFacet::OperationBinding);
        claim_if_some!(claims, self.event, SemanticFacet::Event);
        claim_if_some!(claims, self.failure, SemanticFacet::Failure);
        claims
    }

    fn references(&self) -> Vec<&str> {
        let mut references: Vec<_> = self.operation_id.iter().map(String::as_str).collect();
        if let Some(event) = &self.event {
            references.extend(event.resync_operation_id.iter().map(String::as_str));
        }
        references
    }

    fn validate_semantics(&self) -> Result<(), ManifestError> {
        if let Some(event) = &self.event {
            if !matches!(event.loss_detection, LossDetection::NotApplicable)
                && event.resync_operation_id.is_none()
            {
                return Err(ManifestError::new(
                    ErrorCode::MissingFacet,
                    &self.target_id,
                    "loss-detecting WebSocket semantics require a resync operation",
                ));
            }
        }
        if let Some(failure) = &self.failure {
            crate::v2::validate_failure(failure, &self.target_id)?;
        }
        Ok(())
    }
}

impl SemanticRecord for WasmRecord {
    fn target_id(&self) -> &str {
        &self.target_id
    }

    fn owner(&self) -> &str {
        &self.owner
    }

    fn claims(&self) -> BTreeSet<SemanticFacet> {
        let mut claims = BTreeSet::new();
        claim_if_some!(claims, self.operation_id, SemanticFacet::OperationBinding);
        claim_if_some!(claims, self.host, SemanticFacet::WasmHost);
        claim_if_some!(claims, self.stability, SemanticFacet::Stability);
        claim_if_some!(claims, self.failure, SemanticFacet::Failure);
        claims
    }

    fn references(&self) -> Vec<&str> {
        let mut references: Vec<_> = self.operation_id.iter().map(String::as_str).collect();
        if let Some(host) = &self.host {
            references.extend(host.capability_ids.iter().map(String::as_str));
        }
        references
    }

    fn validate_semantics(&self) -> Result<(), ManifestError> {
        if let Some(host) = &self.host {
            if host.canonical_package_owner.trim().is_empty() {
                return Err(ManifestError::new(
                    ErrorCode::MissingFacet,
                    &self.target_id,
                    "WASM host semantics require a canonical package owner",
                ));
            }
        }
        if let Some(stability) = &self.stability {
            crate::v2::validate_stability(stability, &self.target_id)?;
        }
        if let Some(failure) = &self.failure {
            crate::v2::validate_failure(failure, &self.target_id)?;
        }
        Ok(())
    }
}

impl SemanticRecord for ConsumerRecord {
    fn target_id(&self) -> &str {
        &self.target_id
    }

    fn owner(&self) -> &str {
        &self.owner
    }

    fn claims(&self) -> BTreeSet<SemanticFacet> {
        let mut claims = BTreeSet::new();
        claim_if_some!(claims, self.policy, SemanticFacet::ConsumerPolicy);
        claim_if_some!(claims, self.coverage, SemanticFacet::Coverage);
        if !self.exclusions.is_empty() {
            claims.insert(SemanticFacet::Exclusions);
        }
        claims
    }

    fn references(&self) -> Vec<&str> {
        let mut references = Vec::new();
        if let Some(policy) = &self.policy {
            references.extend(policy.capability_ids.iter().map(String::as_str));
        }
        references.extend(self.exclusions.iter().map(|value| value.target_id.as_str()));
        references
    }

    fn validate_semantics(&self) -> Result<(), ManifestError> {
        for exclusion in &self.exclusions {
            if exclusion.reason.trim().is_empty() || exclusion.owner.trim().is_empty() {
                return Err(ManifestError::new(
                    ErrorCode::MissingFacet,
                    &self.target_id,
                    "consumer exclusions require a reason and owner",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FragmentSet {
    pub authoring: AuthoringFragment,
    pub runtime: RuntimeFragment,
    pub http: HttpFragment,
    pub websocket: WebsocketFragment,
    pub wasm: WasmFragment,
    pub consumers: ConsumersFragment,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DiscoveredSemanticNode {
    pub id: String,
    pub required_facets: BTreeSet<SemanticFacet>,
}

impl FragmentSet {
    pub fn validate(&self, discovered: &[DiscoveredSemanticNode]) -> Result<(), ManifestError> {
        let discovered_by_id: BTreeMap<_, _> = discovered
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        if discovered_by_id.len() != discovered.len() {
            return Err(ManifestError::new(
                ErrorCode::DuplicateId,
                "discovered",
                "discovered nodes contain duplicate stable IDs",
            ));
        }

        let mut claims: BTreeMap<(&str, SemanticFacet), (FragmentDomain, &str)> = BTreeMap::new();
        let capability_ids: BTreeSet<_> = discovered_by_id
            .keys()
            .copied()
            .filter(|id| id.split(':').nth(1) == Some("capability"))
            .collect();
        self.visit_records(|domain, record| {
            if !discovered_by_id.contains_key(record.target_id()) {
                return Err(ManifestError::new(
                    ErrorCode::OrphanId,
                    record.target_id(),
                    format!("{} references no discovered mechanical node", domain.file_name()),
                ));
            }
            for reference in record.references() {
                if !discovered_by_id.contains_key(reference) {
                    return Err(ManifestError::new(
                        ErrorCode::OrphanId,
                        record.target_id(),
                        format!("semantic reference {reference:?} does not resolve"),
                    ));
                }
            }
            if let Some(expression) = record
                .availability()
                .and_then(|availability| availability.when.as_ref())
            {
                crate::v2::validate_capability_expression(
                    expression,
                    &capability_ids,
                    record.target_id(),
                )?;
            }
            for facet in record.claims() {
                if let Some((prior_domain, prior_owner)) =
                    claims.insert((record.target_id(), facet), (domain, record.owner()))
                {
                    return Err(ManifestError::new(
                        ErrorCode::DuplicateOwner,
                        record.target_id(),
                        format!(
                            "facet {facet:?} is owned by both {prior_owner:?} in {prior_domain:?} and {:?} in {domain:?}",
                            record.owner()
                        ),
                    ));
                }
            }
            Ok(())
        })?;

        for node in discovered {
            for facet in &node.required_facets {
                if !claims.contains_key(&(node.id.as_str(), *facet)) {
                    return Err(ManifestError::new(
                        ErrorCode::MissingFacet,
                        node.id.clone(),
                        format!("required semantic facet {facet:?} is missing"),
                    ));
                }
            }
        }
        Ok(())
    }

    fn visit_records<'a, F>(&'a self, mut visit: F) -> Result<(), ManifestError>
    where
        F: FnMut(FragmentDomain, &'a dyn SemanticRecord) -> Result<(), ManifestError>,
    {
        for record in &self.authoring.records {
            visit(FragmentDomain::Authoring, record)?;
        }
        for record in &self.runtime.records {
            visit(FragmentDomain::Runtime, record)?;
        }
        for record in &self.http.records {
            visit(FragmentDomain::Http, record)?;
        }
        for record in &self.websocket.records {
            visit(FragmentDomain::Websocket, record)?;
        }
        for record in &self.wasm.records {
            visit(FragmentDomain::Wasm, record)?;
        }
        for record in &self.consumers.records {
            visit(FragmentDomain::Consumers, record)?;
        }
        Ok(())
    }
}

pub fn capability_states() -> BTreeSet<CapabilityState> {
    [
        CapabilityState::Available,
        CapabilityState::Degraded,
        CapabilityState::Unavailable,
        CapabilityState::Unknown,
    ]
    .into_iter()
    .collect()
}

pub fn not_applicable<T>(reason: impl Into<String>) -> Facet<T> {
    Facet::NotApplicable {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mechanical_fields_are_rejected_before_typed_decode() {
        let input = format!(
            "fragment_schema = {FRAGMENT_SCHEMA_URI:?}\nfragment_version = 1\ndomain = \"authoring\"\n[[records]]\ntarget_id = \"v1:entry:0000000000000001\"\nowner = \"rhai\"\nregistered_name = \"voice\"\n"
        );
        assert_eq!(
            parse_authoring_fragment(&input).unwrap_err().code,
            ErrorCode::MechanicalFactRestatement
        );
    }
}
