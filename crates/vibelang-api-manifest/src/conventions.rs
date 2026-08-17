use crate::{ErrorCode, ManifestError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CONVENTIONS_SCHEMA_ID: &str = "schema.vibelang.conventions.v1";
pub const CONVENTIONS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConventionsMetadata {
    pub schema_id: String,
    pub schema_version: u32,
    pub units: Vec<UnitDefinition>,
    pub ranges: Vec<RangeDefinition>,
    pub invalid_value_policies: Vec<InvalidValuePolicyDefinition>,
    pub parser_policies: Vec<ParserPolicyDefinition>,
    pub collision_policies: Vec<CollisionPolicyDefinition>,
    pub diagnostics: Vec<DiagnosticDefinition>,
    pub availability_states: Vec<AvailabilityStateDefinition>,
    pub availability_reasons: Vec<AvailabilityReasonDefinition>,
    pub security_modes: Vec<SecurityModeDefinition>,
    pub capabilities: Vec<CapabilityDefinition>,
    pub classification_rules: Vec<ClassificationRuleDefinition>,
    pub parameter_quantities: Vec<QuantityOccurrence>,
    pub ugen_input_quantities: Vec<QuantityOccurrence>,
    pub parser_bindings: Vec<ParserBinding>,
    pub collision_bindings: Vec<CollisionBinding>,
    pub availability_bindings: Vec<AvailabilityBinding>,
    pub security_bindings: Vec<SecurityBinding>,
    pub stats: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDefinition {
    pub unit_id: String,
    pub wire_type: String,
    pub meaning: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeDefinition {
    pub range_id: String,
    pub minimum: Option<f64>,
    pub minimum_inclusive: bool,
    pub maximum: Option<f64>,
    pub maximum_inclusive: bool,
    pub finite: bool,
    pub integer: bool,
    pub allowed_values: Vec<f64>,
    pub unbounded: bool,
    pub meaning: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvalidValuePolicyDefinition {
    pub policy_id: String,
    pub canonical: bool,
    pub diagnostic_id: Option<String>,
    pub behavior: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserStrictness {
    Strict,
    Permissive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParserPolicyDefinition {
    pub parser_id: String,
    pub strictness: ParserStrictness,
    pub grammar_id: String,
    pub consumes_full_input: bool,
    pub fallback_policy_id: String,
    pub fallback_rationale: String,
    pub diagnostic_id: Option<String>,
    pub source_anchor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollisionPolicyDefinition {
    pub policy_id: String,
    pub namespace: String,
    pub duplicate_behavior: String,
    pub deterministic_resolution: String,
    pub diagnostic_id: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticDefinition {
    pub diagnostic_id: String,
    pub severity_id: String,
    pub category_id: String,
    pub meaning: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvailabilityStateDefinition {
    pub state_id: String,
    pub terminal: bool,
    pub meaning: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvailabilityReasonDefinition {
    pub reason_id: String,
    pub gate: AvailabilityGate,
    pub meaning: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityModeDefinition {
    pub mode_id: String,
    pub remote_allowed: bool,
    pub authentication_required: bool,
    pub origin_policy_id: String,
    pub degraded_reason_id: Option<String>,
    pub meaning: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityGate {
    Declaration,
    Target,
    BuildFeature,
    OperatorPolicy,
    RuntimeProbe,
    BackendSemantic,
    ConsumerProjection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDefinition {
    pub capability_id: String,
    pub required_gates: Vec<AvailabilityGate>,
    pub detection_source: String,
    pub dependencies: Vec<String>,
    pub conflicts: Vec<String>,
    pub meaning: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationRuleDefinition {
    pub rule_id: String,
    pub rationale: String,
    pub source_anchor: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationBasis {
    ReviewedRule,
    ExplicitOverride,
    HeuristicOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityOccurrence {
    pub occurrence_id: String,
    pub target_id: String,
    pub position: u32,
    pub name: Option<String>,
    pub source_type: String,
    pub classification: QuantityClassification,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "applicability", rename_all = "snake_case", deny_unknown_fields)]
pub enum QuantityClassification {
    Applicable {
        semantic_type_id: String,
        unit_id: String,
        range_id: String,
        canonical_invalid_value_policy_id: String,
        legacy_invalid_value_policy_id: Option<String>,
        rule_id: String,
        basis: ClassificationBasis,
        provenance: Vec<String>,
    },
    NotApplicable {
        reason: String,
        rule_id: String,
        basis: ClassificationBasis,
        provenance: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParserBinding {
    pub target_id: String,
    pub canonical_parser_id: String,
    pub legacy_parser_id: String,
    pub fallback_rationale: String,
    pub diagnostic_id: Option<String>,
    pub source_anchors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollisionBinding {
    pub target_id: String,
    pub canonical_policy_id: String,
    pub legacy_policy_id: String,
    pub candidates: Vec<String>,
    pub source_anchors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvailabilityBinding {
    pub target_id: String,
    pub declared_status: String,
    pub predicate_capability_ids: Vec<String>,
    pub unavailable_reason_ids: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityBinding {
    pub target_id: String,
    pub mode_id: String,
    pub state_id: String,
    pub reason_ids: Vec<String>,
    pub source_anchors: Vec<String>,
}

impl ConventionsMetadata {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_id != CONVENTIONS_SCHEMA_ID
            || self.schema_version != CONVENTIONS_SCHEMA_VERSION
        {
            return Err(invalid("schema_id", "invalid conventions schema identity"));
        }

        let units = sorted_ids("units", &self.units, |value| &value.unit_id)?;
        let ranges = sorted_ids("ranges", &self.ranges, |value| &value.range_id)?;
        let invalid_policies = sorted_ids(
            "invalid_value_policies",
            &self.invalid_value_policies,
            |value| &value.policy_id,
        )?;
        let parsers = sorted_ids("parser_policies", &self.parser_policies, |value| {
            &value.parser_id
        })?;
        let collisions = sorted_ids("collision_policies", &self.collision_policies, |value| {
            &value.policy_id
        })?;
        let diagnostics = sorted_ids("diagnostics", &self.diagnostics, |value| {
            &value.diagnostic_id
        })?;
        let states = sorted_ids("availability_states", &self.availability_states, |value| {
            &value.state_id
        })?;
        let reasons = sorted_ids(
            "availability_reasons",
            &self.availability_reasons,
            |value| &value.reason_id,
        )?;
        let security_modes = sorted_ids("security_modes", &self.security_modes, |value| {
            &value.mode_id
        })?;
        let capabilities = sorted_ids("capabilities", &self.capabilities, |value| {
            &value.capability_id
        })?;
        let rules = sorted_ids(
            "classification_rules",
            &self.classification_rules,
            |value| &value.rule_id,
        )?;

        for range in &self.ranges {
            if range.minimum.is_some_and(|value| !value.is_finite())
                || range.maximum.is_some_and(|value| !value.is_finite())
                || range.allowed_values.iter().any(|value| !value.is_finite())
                || (range.unbounded
                    && (range.minimum.is_some()
                        || range.maximum.is_some()
                        || !range.allowed_values.is_empty()))
                || (!range.unbounded
                    && range.minimum.is_none()
                    && range.maximum.is_none()
                    && range.allowed_values.is_empty())
            {
                return Err(invalid(
                    &range.range_id,
                    "range bounds and unbounded marker disagree",
                ));
            }
        }

        for policy in &self.invalid_value_policies {
            if let Some(id) = &policy.diagnostic_id {
                require(&diagnostics, &policy.policy_id, id)?;
            }
        }
        for parser in &self.parser_policies {
            if parser.grammar_id.trim().is_empty()
                || parser.fallback_policy_id.trim().is_empty()
                || parser.fallback_rationale.trim().is_empty()
            {
                return Err(invalid(&parser.parser_id, "parser policy is incomplete"));
            }
            if let Some(id) = &parser.diagnostic_id {
                require(&diagnostics, &parser.parser_id, id)?;
            }
        }
        for collision in &self.collision_policies {
            require(&diagnostics, &collision.policy_id, &collision.diagnostic_id)?;
        }
        for mode in &self.security_modes {
            if let Some(id) = &mode.degraded_reason_id {
                require(&reasons, &mode.mode_id, id)?;
            }
        }
        for capability in &self.capabilities {
            if capability.required_gates.is_empty() {
                return Err(invalid(
                    &capability.capability_id,
                    "capability has no evaluation gates",
                ));
            }
            for id in capability.dependencies.iter().chain(&capability.conflicts) {
                require(&capabilities, &capability.capability_id, id)?;
            }
        }

        validate_occurrences(
            "parameter_quantities",
            &self.parameter_quantities,
            &units,
            &ranges,
            &invalid_policies,
            &rules,
        )?;
        validate_occurrences(
            "ugen_input_quantities",
            &self.ugen_input_quantities,
            &units,
            &ranges,
            &invalid_policies,
            &rules,
        )?;

        ensure_sorted("parser_bindings", &self.parser_bindings, |value| {
            &value.target_id
        })?;
        for binding in &self.parser_bindings {
            require(&parsers, &binding.target_id, &binding.canonical_parser_id)?;
            require(&parsers, &binding.target_id, &binding.legacy_parser_id)?;
            if binding.fallback_rationale.trim().is_empty() || binding.source_anchors.is_empty() {
                return Err(invalid(
                    &binding.target_id,
                    "parser binding lacks rationale or provenance",
                ));
            }
            if let Some(id) = &binding.diagnostic_id {
                require(&diagnostics, &binding.target_id, id)?;
            }
        }

        ensure_sorted("collision_bindings", &self.collision_bindings, |value| {
            &value.target_id
        })?;
        for binding in &self.collision_bindings {
            require(
                &collisions,
                &binding.target_id,
                &binding.canonical_policy_id,
            )?;
            require(&collisions, &binding.target_id, &binding.legacy_policy_id)?;
            if binding.candidates.len() < 2 || binding.source_anchors.is_empty() {
                return Err(invalid(
                    &binding.target_id,
                    "collision binding lacks candidates or provenance",
                ));
            }
        }

        ensure_sorted(
            "availability_bindings",
            &self.availability_bindings,
            |value| &value.target_id,
        )?;
        for binding in &self.availability_bindings {
            for id in &binding.predicate_capability_ids {
                require(&capabilities, &binding.target_id, id)?;
            }
            for id in &binding.unavailable_reason_ids {
                require(&reasons, &binding.target_id, id)?;
            }
            if binding.declared_status == "conditional"
                && binding.predicate_capability_ids.is_empty()
            {
                return Err(invalid(
                    &binding.target_id,
                    "conditional binding has no capability predicate",
                ));
            }
        }

        ensure_sorted("security_bindings", &self.security_bindings, |value| {
            &value.target_id
        })?;
        for binding in &self.security_bindings {
            require(&security_modes, &binding.target_id, &binding.mode_id)?;
            require(&states, &binding.target_id, &binding.state_id)?;
            for id in &binding.reason_ids {
                require(&reasons, &binding.target_id, id)?;
            }
            if binding.source_anchors.is_empty() {
                return Err(invalid(
                    &binding.target_id,
                    "security binding lacks provenance",
                ));
            }
        }

        let expected = [
            (
                "parameter_occurrences",
                self.parameter_quantities.len() as u64,
            ),
            (
                "ugen_input_occurrences",
                self.ugen_input_quantities.len() as u64,
            ),
            ("unknown_occurrences", 0),
            ("heuristic_only_occurrences", 0),
            ("stale_occurrences", 0),
        ];
        for (key, value) in expected {
            if self.stats.get(key) != Some(&value) {
                return Err(invalid("stats", &format!("{key} must equal {value}")));
            }
        }
        Ok(())
    }
}

fn validate_occurrences(
    path: &str,
    occurrences: &[QuantityOccurrence],
    units: &BTreeSet<&str>,
    ranges: &BTreeSet<&str>,
    invalid_policies: &BTreeSet<&str>,
    rules: &BTreeSet<&str>,
) -> Result<(), ManifestError> {
    ensure_sorted(path, occurrences, |value| &value.occurrence_id)?;
    for occurrence in occurrences {
        if occurrence.target_id.trim().is_empty() || occurrence.source_type.trim().is_empty() {
            return Err(invalid(
                &occurrence.occurrence_id,
                "occurrence identity is incomplete",
            ));
        }
        match &occurrence.classification {
            QuantityClassification::Applicable {
                semantic_type_id,
                unit_id,
                range_id,
                canonical_invalid_value_policy_id,
                legacy_invalid_value_policy_id,
                rule_id,
                basis,
                provenance,
            } => {
                validate_classification_basis(&occurrence.occurrence_id, *basis)?;
                validate_semantic_id(semantic_type_id)?;
                require_named(units, &occurrence.occurrence_id, "unit_id", unit_id)?;
                require_named(ranges, &occurrence.occurrence_id, "range_id", range_id)?;
                require_named(
                    invalid_policies,
                    &occurrence.occurrence_id,
                    "canonical_invalid_value_policy_id",
                    canonical_invalid_value_policy_id,
                )?;
                if let Some(id) = legacy_invalid_value_policy_id {
                    require_named(
                        invalid_policies,
                        &occurrence.occurrence_id,
                        "legacy_invalid_value_policy_id",
                        id,
                    )?;
                }
                require_named(rules, &occurrence.occurrence_id, "rule_id", rule_id)?;
                if provenance.is_empty() {
                    return Err(invalid(
                        &occurrence.occurrence_id,
                        "quantity lacks provenance",
                    ));
                }
            }
            QuantityClassification::NotApplicable {
                reason,
                rule_id,
                basis,
                provenance,
            } => {
                validate_classification_basis(&occurrence.occurrence_id, *basis)?;
                require_named(rules, &occurrence.occurrence_id, "rule_id", rule_id)?;
                if reason.trim().is_empty() || provenance.is_empty() {
                    return Err(invalid(
                        &occurrence.occurrence_id,
                        "not-applicable quantity lacks reason or provenance",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_classification_basis(
    path: &str,
    basis: ClassificationBasis,
) -> Result<(), ManifestError> {
    if basis == ClassificationBasis::HeuristicOnly {
        Err(invalid(path, "must use a reviewed classification basis"))
    } else {
        Ok(())
    }
}

fn sorted_ids<'a, T, F>(
    path: &str,
    values: &'a [T],
    id: F,
) -> Result<BTreeSet<&'a str>, ManifestError>
where
    F: Fn(&'a T) -> &'a str,
{
    let ordered = values.iter().map(&id).collect::<Vec<_>>();
    if ordered.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(ManifestError::new(
            ErrorCode::NonDeterministicOrder,
            path,
            "convention arrays must be strictly sorted by stable ID",
        ));
    }
    let mut ids = BTreeSet::new();
    for value in ordered {
        validate_semantic_id(value)?;
        if !ids.insert(value) {
            return Err(invalid(path, "registry contains duplicate IDs"));
        }
    }
    Ok(ids)
}

fn ensure_sorted<T, F>(path: &str, values: &[T], id: F) -> Result<(), ManifestError>
where
    F: Fn(&T) -> &str,
{
    if values.windows(2).any(|pair| id(&pair[0]) >= id(&pair[1])) {
        return Err(ManifestError::new(
            ErrorCode::NonDeterministicOrder,
            path,
            "convention arrays must be strictly sorted by stable ID",
        ));
    }
    Ok(())
}

fn validate_semantic_id(id: &str) -> Result<(), ManifestError> {
    let valid = !id.is_empty()
        && id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && id.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        });
    if valid {
        Ok(())
    } else {
        Err(ManifestError::new(
            ErrorCode::InvalidStableId,
            id,
            "semantic IDs must contain lowercase dotted ASCII segments",
        ))
    }
}

fn require(ids: &BTreeSet<&str>, path: &str, id: &str) -> Result<(), ManifestError> {
    if ids.contains(id) {
        Ok(())
    } else {
        Err(ManifestError::new(
            ErrorCode::InvalidReference,
            path,
            format!("convention reference {id:?} does not resolve"),
        ))
    }
}

fn require_named(
    ids: &BTreeSet<&str>,
    path: &str,
    field: &str,
    id: &str,
) -> Result<(), ManifestError> {
    if ids.contains(id) {
        Ok(())
    } else {
        Err(ManifestError::new(
            ErrorCode::InvalidReference,
            path,
            format!("unknown {field} {id}"),
        ))
    }
}

fn invalid(path: &str, message: &str) -> ManifestError {
    ManifestError::new(ErrorCode::InvalidValue, path, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_ids_reject_uppercase_empty_and_punctuation_segments() {
        for id in ["Unit.frequency.hz", "unit..hz", "unit.frequency-hz"] {
            assert!(validate_semantic_id(id).is_err(), "{id}");
        }
        assert!(validate_semantic_id("unit.frequency.hz").is_ok());
    }
}
