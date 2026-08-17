use crate::{ErrorCode, ManifestError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityClass {
    MetadataOnly,
    CompatibleAddition,
    CompatibleRelaxation,
    BehavioralChange,
    SourceBreaking,
    WireBreaking,
    AvailabilityBreaking,
    ConsumerBreaking,
    SecurityOperational,
    Unclassified,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChangeKind {
    OwnershipOrAnchor,
    OptionalNodeAddition,
    OptionalResponseFieldAddition,
    SemanticTokenAppend,
    AcceptedInputRelaxation,
    RangeWidening,
    CapabilityExpansion,
    DefaultOrFallback,
    ErrorOrDiagnostic,
    TimingOrLifecycle,
    ParserOrConsistency,
    Deprecation,
    CallableRemovalOrRename,
    ReceiverOrSignature,
    RequiredParameterAddition,
    TypeNarrowing,
    HttpMethodPathStatus,
    SerializedFieldShape,
    EventPayloadMeaning,
    SemanticTokenReorder,
    ErrorEnvelope,
    AvailabilityNarrowing,
    ConsumerEligibilityRemoval,
    PackagePathOrEntrypoint,
    CoverageDenominatorShrink,
    SecurityPolicy,
}

pub fn classify_change_kind(kind: ChangeKind) -> BTreeSet<CompatibilityClass> {
    use ChangeKind as Kind;
    use CompatibilityClass as Class;

    match kind {
        Kind::OwnershipOrAnchor => set([Class::MetadataOnly]),
        Kind::OptionalNodeAddition
        | Kind::OptionalResponseFieldAddition
        | Kind::SemanticTokenAppend => set([Class::CompatibleAddition]),
        Kind::AcceptedInputRelaxation | Kind::RangeWidening | Kind::CapabilityExpansion => {
            set([Class::CompatibleRelaxation])
        }
        Kind::DefaultOrFallback
        | Kind::ErrorOrDiagnostic
        | Kind::TimingOrLifecycle
        | Kind::ParserOrConsistency
        | Kind::Deprecation => set([Class::BehavioralChange]),
        Kind::CallableRemovalOrRename
        | Kind::ReceiverOrSignature
        | Kind::RequiredParameterAddition
        | Kind::TypeNarrowing => set([Class::SourceBreaking]),
        Kind::HttpMethodPathStatus
        | Kind::SerializedFieldShape
        | Kind::EventPayloadMeaning
        | Kind::SemanticTokenReorder
        | Kind::ErrorEnvelope => set([Class::WireBreaking]),
        Kind::AvailabilityNarrowing => set([Class::AvailabilityBreaking]),
        Kind::ConsumerEligibilityRemoval
        | Kind::PackagePathOrEntrypoint
        | Kind::CoverageDenominatorShrink => set([Class::ConsumerBreaking]),
        Kind::SecurityPolicy => set([Class::SecurityOperational]),
    }
}

pub fn classify_json_pointer(
    pointer: &str,
    before: Option<&Value>,
    after: Option<&Value>,
) -> BTreeSet<CompatibilityClass> {
    use CompatibilityClass as Class;

    let normalized = pointer.to_ascii_lowercase();
    if after.is_some() && before.is_none() {
        if normalized.contains("security") {
            return set([Class::CompatibleAddition, Class::SecurityOperational]);
        }
        return set([Class::CompatibleAddition]);
    }
    if before.is_some() && after.is_none() {
        let mut classes = BTreeSet::new();
        if is_wire_path(&normalized) {
            classes.insert(Class::WireBreaking);
        }
        if is_consumer_path(&normalized) {
            classes.insert(Class::ConsumerBreaking);
        }
        if normalized.contains("availability") || normalized.contains("capabilit") {
            classes.insert(Class::AvailabilityBreaking);
        }
        if classes.is_empty() {
            classes.insert(Class::SourceBreaking);
        }
        return classes;
    }
    if normalized.contains("ownership")
        || normalized.contains("source_anchors")
        || normalized.contains("test_anchors")
        || normalized.ends_with("/description")
    {
        return set([Class::MetadataOnly]);
    }
    if normalized.contains("security")
        || normalized.contains("authentication")
        || normalized.contains("origin")
        || normalized.contains("rate_limit")
    {
        return set([Class::SecurityOperational]);
    }
    if is_wire_path(&normalized) {
        return set([Class::WireBreaking]);
    }
    if normalized.contains("registered_name")
        || normalized.contains("signature")
        || normalized.contains("receiver")
        || normalized.contains("required_parameter")
    {
        return set([Class::SourceBreaking]);
    }
    if normalized.contains("availability") || normalized.contains("capabilit") {
        return set([Class::AvailabilityBreaking]);
    }
    if is_consumer_path(&normalized) {
        return set([Class::ConsumerBreaking]);
    }
    if normalized.contains("default")
        || normalized.contains("fallback")
        || normalized.contains("error")
        || normalized.contains("diagnostic")
        || normalized.contains("timing")
        || normalized.contains("lifecycle")
        || normalized.contains("consistency")
        || normalized.contains("parser")
        || normalized.contains("idempotency")
        || normalized.contains("stability/level")
    {
        return set([Class::BehavioralChange]);
    }
    set([Class::Unclassified])
}

fn is_wire_path(pointer: &str) -> bool {
    pointer.contains("http/method")
        || pointer.contains("http/path")
        || pointer.contains("success_status")
        || pointer.contains("serialized_name")
        || pointer.contains("event/payload")
        || pointer.contains("token_legend")
        || pointer.contains("error_envelope")
}

fn is_consumer_path(pointer: &str) -> bool {
    pointer.contains("consumer")
        || pointer.contains("package")
        || pointer.contains("coverage/denominator")
}

fn set<const N: usize>(classes: [CompatibilityClass; N]) -> BTreeSet<CompatibilityClass> {
    classes.into_iter().collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityChange {
    pub pointer: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub classes: BTreeSet<CompatibilityClass>,
    pub rationale: String,
    pub impacted_consumers: Vec<String>,
    pub required_action: String,
}

impl CompatibilityChange {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.pointer.is_empty() || !self.pointer.starts_with('/') {
            return Err(ManifestError::new(
                ErrorCode::InvalidValue,
                "compatibility.pointer",
                "a compatibility pointer must be an absolute JSON pointer",
            ));
        }
        if self.classes.is_empty() || self.classes.contains(&CompatibilityClass::Unclassified) {
            return Err(ManifestError::new(
                ErrorCode::UnclassifiedDiff,
                self.pointer.clone(),
                "every changed pointer must have one or more concrete compatibility classes",
            ));
        }
        if self.rationale.trim().is_empty() || self.required_action.trim().is_empty() {
            return Err(ManifestError::new(
                ErrorCode::MissingFacet,
                self.pointer.clone(),
                "classified changes require a rationale and release action",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityReport {
    pub changes: Vec<CompatibilityChange>,
}

impl CompatibilityReport {
    pub fn validate(&self) -> Result<(), ManifestError> {
        let mut pointers = BTreeSet::new();
        for change in &self.changes {
            change.validate()?;
            if !pointers.insert(change.pointer.as_str()) {
                return Err(ManifestError::new(
                    ErrorCode::DuplicateId,
                    change.pointer.clone(),
                    "a changed pointer may appear only once; attach every applicable class to it",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_changes_receive_the_frozen_classes() {
        assert_eq!(
            classify_change_kind(ChangeKind::HttpMethodPathStatus),
            set([CompatibilityClass::WireBreaking])
        );
        assert_eq!(
            classify_change_kind(ChangeKind::DefaultOrFallback),
            set([CompatibilityClass::BehavioralChange])
        );
        assert_eq!(
            classify_change_kind(ChangeKind::SecurityPolicy),
            set([CompatibilityClass::SecurityOperational])
        );
    }
}
