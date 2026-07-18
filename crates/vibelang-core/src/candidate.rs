//! Immutable v2 candidate and logical-identity foundations.
//!
//! These types deliberately stop before runtime planning or activation. They
//! let authoring layers build and validate a side-effect-free candidate while
//! keeping language-contract, engine-instance, and runtime-epoch boundaries
//! explicit.

use crate::mutation::{CandidateOrigin, RuntimeEpoch, SourceSpan};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

pub const V2_LANGUAGE_MAJOR: u16 = 2;
pub const V2_MANIFEST_SCHEMA_VERSION: u16 = 2;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContractDigest(String);

impl ContractDigest {
    pub fn parse(value: impl Into<String>) -> Result<Self, CandidateError> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(CandidateError::InvalidIdentity(
                "contract digest must use sha256:<hex>".into(),
            ));
        };
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(CandidateError::InvalidIdentity(
                "contract digest must contain 64 lowercase hex digits".into(),
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(format!("sha256:{:x}", Sha256::digest(bytes)))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContractDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LanguageContract {
    language_major: u16,
    manifest_schema_version: u16,
    manifest_digest: ContractDigest,
}

impl LanguageContract {
    pub fn new(
        language_major: u16,
        manifest_schema_version: u16,
        manifest_digest: ContractDigest,
    ) -> Result<Self, CandidateError> {
        if language_major == 0 || manifest_schema_version == 0 {
            return Err(CandidateError::InvalidIdentity(
                "language major and manifest schema version must be non-zero".into(),
            ));
        }
        Ok(Self {
            language_major,
            manifest_schema_version,
            manifest_digest,
        })
    }

    pub fn v2(manifest_digest: ContractDigest) -> Self {
        Self {
            language_major: V2_LANGUAGE_MAJOR,
            manifest_schema_version: V2_MANIFEST_SCHEMA_VERSION,
            manifest_digest,
        }
    }

    #[must_use]
    pub const fn language_major(&self) -> u16 {
        self.language_major
    }

    #[must_use]
    pub const fn manifest_schema_version(&self) -> u16 {
        self.manifest_schema_version
    }

    #[must_use]
    pub fn manifest_digest(&self) -> &ContractDigest {
        &self.manifest_digest
    }
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EngineInstanceId(Uuid);

impl EngineInstanceId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn parse(value: &str) -> Result<Self, CandidateError> {
        let uuid = Uuid::parse_str(value)
            .map_err(|error| CandidateError::InvalidIdentity(error.to_string()))?;
        if uuid.get_version_num() != 7 || uuid.hyphenated().to_string() != value {
            return Err(CandidateError::InvalidIdentity(
                "engine instance id must use canonical lowercase UUIDv7".into(),
            ));
        }
        Ok(Self(uuid))
    }
}

impl Default for EngineInstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for EngineInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("EngineInstanceId")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for EngineInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationIdentity {
    language: LanguageContract,
    engine_instance: EngineInstanceId,
    runtime_epoch: RuntimeEpoch,
}

impl EvaluationIdentity {
    #[must_use]
    pub fn new(
        language: LanguageContract,
        engine_instance: EngineInstanceId,
        runtime_epoch: RuntimeEpoch,
    ) -> Self {
        Self {
            language,
            engine_instance,
            runtime_epoch,
        }
    }

    #[must_use]
    pub fn language(&self) -> &LanguageContract {
        &self.language
    }

    #[must_use]
    pub const fn engine_instance(&self) -> EngineInstanceId {
        self.engine_instance
    }

    #[must_use]
    pub const fn runtime_epoch(&self) -> RuntimeEpoch {
        self.runtime_epoch
    }

    pub fn ensure_compatible(&self, other: &Self) -> Result<(), CompatibilityError> {
        if self.language != other.language {
            return Err(CompatibilityError::Contract {
                expected: self.language.clone(),
                actual: other.language.clone(),
            });
        }
        if self.engine_instance != other.engine_instance {
            return Err(CompatibilityError::Engine {
                expected: self.engine_instance,
                actual: other.engine_instance,
            });
        }
        if self.runtime_epoch != other.runtime_epoch {
            return Err(CompatibilityError::Epoch {
                expected: self.runtime_epoch,
                actual: other.runtime_epoch,
            });
        }
        Ok(())
    }
}

fn validate_component(value: &str, label: &'static str) -> Result<(), CandidateError> {
    if value.is_empty() || value.len() > 255 {
        return Err(CandidateError::InvalidAddress(format!(
            "{label} must contain 1..=255 bytes"
        )));
    }
    if value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(CandidateError::InvalidAddress(format!(
            "{label} contains a non-canonical component: {value}"
        )));
    }
    Ok(())
}

macro_rules! component_type {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, CandidateError> {
                let value = value.into();
                validate_component(&value, $label)?;
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

component_type!(ProjectNamespace, "project namespace");
component_type!(DeclarationKey, "declaration key");
component_type!(GroupComponent, "group component");

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModulePath(String);

impl ModulePath {
    pub fn new(value: impl Into<String>) -> Result<Self, CandidateError> {
        let value = value.into();
        if value.starts_with('/') || value.ends_with('/') || value.contains('\\') {
            return Err(CandidateError::InvalidAddress(format!(
                "module path must be canonical and project-relative: {value}"
            )));
        }
        for component in value.split('/') {
            validate_component(component, "module path")?;
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModulePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GroupScope(Vec<GroupComponent>);

impl GroupScope {
    #[must_use]
    pub fn root() -> Self {
        Self::default()
    }

    pub fn new(components: impl IntoIterator<Item = String>) -> Result<Self, CandidateError> {
        components
            .into_iter()
            .map(GroupComponent::new)
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    #[must_use]
    pub fn components(&self) -> &[GroupComponent] {
        &self.0
    }
}

impl fmt::Display for GroupScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, component) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("/")?;
            }
            component.fmt(formatter)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EntityKind {
    Group,
    Voice,
    Pattern,
    Melody,
    Sequence,
    Fade,
    Effect,
    SynthDef,
    EffectDef,
    Sample,
    Buffer,
    Sfz,
    Recording,
    Route,
    MidiDevice,
    MidiRoute,
    Callback,
}

impl fmt::Display for EntityKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", format!("{self:?}").to_ascii_lowercase())
    }
}

pub trait RefKind: Clone + fmt::Debug + Eq + Ord + Send + Sync + 'static {
    const KIND: EntityKind;
}

macro_rules! ref_kinds {
    ($(($name:ident, $kind:ident)),+ $(,)?) => {
        $(
            #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
            pub struct $name;

            impl RefKind for $name {
                const KIND: EntityKind = EntityKind::$kind;
            }
        )+
    };
}

ref_kinds!(
    (GroupKind, Group),
    (VoiceKind, Voice),
    (PatternKind, Pattern),
    (MelodyKind, Melody),
    (SequenceKind, Sequence),
    (FadeKind, Fade),
    (EffectKind, Effect),
    (SynthDefKind, SynthDef),
    (EffectDefKind, EffectDef),
    (SampleKind, Sample),
    (BufferKind, Buffer),
    (SfzKind, Sfz),
    (RecordingKind, Recording),
    (RouteKind, Route),
    (MidiDeviceKind, MidiDevice),
    (MidiRouteKind, MidiRoute),
    (CallbackKind, Callback),
);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LogicalAddress {
    project: ProjectNamespace,
    module: ModulePath,
    kind: EntityKind,
    group_scope: GroupScope,
    key: DeclarationKey,
}

impl LogicalAddress {
    #[must_use]
    pub fn new(
        project: ProjectNamespace,
        module: ModulePath,
        kind: EntityKind,
        group_scope: GroupScope,
        key: DeclarationKey,
    ) -> Self {
        Self {
            project,
            module,
            kind,
            group_scope,
            key,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        self.kind
    }

    #[must_use]
    pub fn project(&self) -> &ProjectNamespace {
        &self.project
    }

    #[must_use]
    pub fn module(&self) -> &ModulePath {
        &self.module
    }

    #[must_use]
    pub fn group_scope(&self) -> &GroupScope {
        &self.group_scope
    }

    #[must_use]
    pub fn key(&self) -> &DeclarationKey {
        &self.key
    }
}

impl fmt::Display for LogicalAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}::{}::{}::",
            self.project, self.module, self.kind
        )?;
        if !self.group_scope.components().is_empty() {
            write!(formatter, "{}/", self.group_scope)?;
        }
        self.key.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TypedAddress<K: RefKind> {
    address: LogicalAddress,
    kind: PhantomData<K>,
}

impl<K: RefKind> TypedAddress<K> {
    pub fn new(
        project: ProjectNamespace,
        module: ModulePath,
        group_scope: GroupScope,
        key: DeclarationKey,
    ) -> Self {
        Self {
            address: LogicalAddress::new(project, module, K::KIND, group_scope, key),
            kind: PhantomData,
        }
    }

    pub fn from_untyped(address: LogicalAddress) -> Result<Self, CandidateError> {
        if address.kind != K::KIND {
            return Err(CandidateError::WrongRefKind {
                expected: K::KIND,
                actual: address.kind,
            });
        }
        Ok(Self {
            address,
            kind: PhantomData,
        })
    }

    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        K::KIND
    }

    #[must_use]
    pub fn as_untyped(&self) -> &LogicalAddress {
        &self.address
    }

    #[must_use]
    pub fn erase(&self) -> LogicalAddress {
        self.address.clone()
    }
}

impl<K: RefKind> fmt::Display for TypedAddress<K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.address.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedRef<K: RefKind> {
    identity: EvaluationIdentity,
    address: TypedAddress<K>,
}

impl<K: RefKind> TypedRef<K> {
    #[must_use]
    pub fn new(identity: EvaluationIdentity, address: TypedAddress<K>) -> Self {
        Self { identity, address }
    }

    #[must_use]
    pub fn identity(&self) -> &EvaluationIdentity {
        &self.identity
    }

    #[must_use]
    pub fn address(&self) -> &TypedAddress<K> {
        &self.address
    }

    #[must_use]
    pub fn erase(&self) -> ErasedRef {
        ErasedRef {
            identity: self.identity.clone(),
            address: self.address.erase(),
        }
    }

    pub fn validate(&self, identity: &EvaluationIdentity) -> Result<(), CompatibilityError> {
        identity.ensure_compatible(&self.identity)
    }

    #[must_use]
    pub fn persisted_address(&self) -> RefAddress<K> {
        RefAddress {
            language: self.identity.language.clone(),
            address: self.address.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErasedRef {
    identity: EvaluationIdentity,
    address: LogicalAddress,
}

impl ErasedRef {
    #[must_use]
    pub fn identity(&self) -> &EvaluationIdentity {
        &self.identity
    }

    #[must_use]
    pub fn address(&self) -> &LogicalAddress {
        &self.address
    }

    pub fn try_typed<K: RefKind>(&self) -> Result<TypedRef<K>, CandidateError> {
        Ok(TypedRef::new(
            self.identity.clone(),
            TypedAddress::from_untyped(self.address.clone())?,
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefAddress<K: RefKind> {
    language: LanguageContract,
    address: TypedAddress<K>,
}

impl<K: RefKind> RefAddress<K> {
    #[must_use]
    pub fn language(&self) -> &LanguageContract {
        &self.language
    }

    #[must_use]
    pub fn address(&self) -> &TypedAddress<K> {
        &self.address
    }

    pub fn resolve(
        &self,
        identity: &EvaluationIdentity,
    ) -> Result<TypedRef<K>, CompatibilityError> {
        if self.language != identity.language {
            return Err(CompatibilityError::Contract {
                expected: identity.language.clone(),
                actual: self.language.clone(),
            });
        }
        Ok(TypedRef::new(identity.clone(), self.address.clone()))
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SyntaxKey {
    Explicit(DeclarationKey),
    Generated(String),
}

impl SyntaxKey {
    pub fn deterministic(
        module: &ModulePath,
        syntax_path: &[u32],
        role: &str,
    ) -> Result<Self, CandidateError> {
        validate_component(role, "syntax role")?;
        if syntax_path.is_empty() {
            return Err(CandidateError::InvalidAddress(
                "a generated syntax key needs a non-empty AST path".into(),
            ));
        }
        let mut hasher = Sha256::new();
        hasher.update(b"vibelang.syntax-key.v2\0");
        hasher.update(module.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(role.as_bytes());
        for component in syntax_path {
            hasher.update(component.to_be_bytes());
        }
        Ok(Self::Generated(format!("{:x}", hasher.finalize())))
    }
}

impl fmt::Display for SyntaxKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Explicit(key) => write!(formatter, "explicit:{key}"),
            Self::Generated(key) => write!(formatter, "syntax:{key}"),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContributionId {
    module: ModulePath,
    key: SyntaxKey,
}

impl ContributionId {
    #[must_use]
    pub fn new(module: ModulePath, key: SyntaxKey) -> Self {
        Self { module, key }
    }
}

impl fmt::Display for ContributionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}::{}", self.module, self.key)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OverrideId(ContributionId);

impl OverrideId {
    #[must_use]
    pub fn new(id: ContributionId) -> Self {
        Self(id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceAnchor {
    module: ModulePath,
    syntax_key: SyntaxKey,
    span: Option<SourceSpan>,
}

impl SourceAnchor {
    #[must_use]
    pub fn new(module: ModulePath, syntax_key: SyntaxKey, span: Option<SourceSpan>) -> Self {
        Self {
            module,
            syntax_key,
            span,
        }
    }

    #[must_use]
    pub fn module(&self) -> &ModulePath {
        &self.module
    }

    #[must_use]
    pub fn syntax_key(&self) -> &SyntaxKey {
        &self.syntax_key
    }

    #[must_use]
    pub fn span(&self) -> Option<&SourceSpan> {
        self.span.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AuthoringRole {
    Value,
    Builder,
    Ref,
    Observation,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TerminalEffect {
    Register,
    Start,
    Stop,
    Synchronize,
    Cancel,
    Observe,
}

impl TerminalEffect {
    const fn lifecycle_effect(self) -> LifecycleEffect {
        match self {
            Self::Register => LifecycleEffect::Register,
            Self::Start => LifecycleEffect::Start,
            Self::Stop => LifecycleEffect::Stop,
            Self::Synchronize => LifecycleEffect::Synchronize,
            Self::Cancel => LifecycleEffect::Cancel,
            Self::Observe => LifecycleEffect::Observe,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Synchronization {
    None,
    CandidateLocal,
    RevisionReceipt,
    BackendBarrier,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Cancellation {
    NotCancellable,
    BeforePlanning,
    RemoveDeclaration,
    DisconnectEdge,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Composition {
    Standalone,
    Reference,
    Contribution,
    Override,
    ParentOwned,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EffectDomain {
    Managed,
    External,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleMetadata {
    pub role: AuthoringRole,
    pub phase: LifecyclePhase,
    pub effects: BTreeSet<LifecycleEffect>,
    pub terminal_effect: TerminalEffect,
    pub synchronization: Synchronization,
    pub cancellation: Cancellation,
    pub composition: Composition,
    pub effect_domain: EffectDomain,
}

impl LifecycleMetadata {
    #[must_use]
    pub fn register(composition: Composition) -> Self {
        Self {
            role: AuthoringRole::Builder,
            phase: LifecyclePhase::Register,
            effects: BTreeSet::from([LifecycleEffect::Register]),
            terminal_effect: TerminalEffect::Register,
            synchronization: Synchronization::CandidateLocal,
            cancellation: Cancellation::RemoveDeclaration,
            composition,
            effect_domain: EffectDomain::Managed,
        }
    }

    pub fn validate(&self) -> Result<(), CandidateError> {
        if !self
            .effects
            .contains(&self.terminal_effect.lifecycle_effect())
        {
            return Err(CandidateError::InvalidLifecycle(
                "terminal effect is absent from the lifecycle effect set".into(),
            ));
        }
        if self.terminal_effect == TerminalEffect::Synchronize
            && self.synchronization == Synchronization::None
        {
            return Err(CandidateError::InvalidLifecycle(
                "a synchronize terminal needs an explicit synchronization boundary".into(),
            ));
        }
        if self.terminal_effect == TerminalEffect::Cancel
            && self.cancellation == Cancellation::NotCancellable
        {
            return Err(CandidateError::InvalidLifecycle(
                "a cancel terminal needs an explicit cancellation mode".into(),
            ));
        }
        if self.effect_domain == EffectDomain::External {
            return Err(CandidateError::ExternalEffect);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeclarationOwner {
    Structural(SyntaxKey),
    Contribution(ContributionId),
    Parent(LogicalAddress),
    Override(OverrideId),
}

impl DeclarationOwner {
    fn composition(&self) -> Composition {
        match self {
            Self::Structural(_) => Composition::Standalone,
            Self::Contribution(_) => Composition::Contribution,
            Self::Parent(_) => Composition::ParentOwned,
            Self::Override(_) => Composition::Override,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeclarationPayload {
    Empty,
    Opaque {
        type_id: String,
        canonical_bytes: Arc<[u8]>,
    },
}

impl DeclarationPayload {
    pub fn opaque(
        type_id: impl Into<String>,
        canonical_bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self, CandidateError> {
        let type_id = type_id.into();
        validate_component(&type_id, "payload type id")?;
        Ok(Self::Opaque {
            type_id,
            canonical_bytes: canonical_bytes.into(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationIr {
    address: LogicalAddress,
    owner: DeclarationOwner,
    source: SourceAnchor,
    lifecycle: LifecycleMetadata,
    payload: DeclarationPayload,
}

impl DeclarationIr {
    #[must_use]
    pub fn new<K: RefKind>(
        address: TypedAddress<K>,
        owner: DeclarationOwner,
        source: SourceAnchor,
        lifecycle: LifecycleMetadata,
        payload: DeclarationPayload,
    ) -> Self {
        Self {
            address: address.erase(),
            owner,
            source,
            lifecycle,
            payload,
        }
    }

    #[must_use]
    pub fn new_erased(
        address: LogicalAddress,
        owner: DeclarationOwner,
        source: SourceAnchor,
        lifecycle: LifecycleMetadata,
        payload: DeclarationPayload,
    ) -> Self {
        Self {
            address,
            owner,
            source,
            lifecycle,
            payload,
        }
    }

    #[must_use]
    pub fn address(&self) -> &LogicalAddress {
        &self.address
    }

    #[must_use]
    pub fn owner(&self) -> &DeclarationOwner {
        &self.owner
    }

    #[must_use]
    pub fn source(&self) -> &SourceAnchor {
        &self.source
    }

    #[must_use]
    pub fn lifecycle(&self) -> &LifecycleMetadata {
        &self.lifecycle
    }

    #[must_use]
    pub fn payload(&self) -> &DeclarationPayload {
        &self.payload
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceUse {
    reference: ErasedRef,
    source: SourceAnchor,
}

impl ReferenceUse {
    #[must_use]
    pub fn reference(&self) -> &ErasedRef {
        &self.reference
    }

    #[must_use]
    pub fn source(&self) -> &SourceAnchor {
        &self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionIr {
    id: ContributionId,
    target_group: ErasedRef,
    explicit_order: Option<i32>,
    source: SourceAnchor,
    owned_declarations: BTreeSet<LogicalAddress>,
}

impl ContributionIr {
    #[must_use]
    pub fn new(
        id: ContributionId,
        target_group: TypedRef<GroupKind>,
        explicit_order: Option<i32>,
        source: SourceAnchor,
    ) -> Self {
        Self {
            id,
            target_group: target_group.erase(),
            explicit_order,
            source,
            owned_declarations: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn id(&self) -> &ContributionId {
        &self.id
    }

    #[must_use]
    pub fn target_group(&self) -> &ErasedRef {
        &self.target_group
    }

    #[must_use]
    pub const fn explicit_order(&self) -> Option<i32> {
        self.explicit_order
    }

    #[must_use]
    pub fn source(&self) -> &SourceAnchor {
        &self.source
    }

    #[must_use]
    pub fn owned_declarations(&self) -> &BTreeSet<LogicalAddress> {
        &self.owned_declarations
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverrideIr {
    id: OverrideId,
    target: ErasedRef,
    fields: BTreeSet<String>,
    precedence: i32,
    source: SourceAnchor,
}

impl OverrideIr {
    pub fn new<K: RefKind>(
        id: OverrideId,
        target: TypedRef<K>,
        fields: impl IntoIterator<Item = String>,
        precedence: i32,
        source: SourceAnchor,
    ) -> Result<Self, CandidateError> {
        let fields = fields
            .into_iter()
            .map(|field| {
                validate_component(&field, "override field")?;
                Ok(field)
            })
            .collect::<Result<BTreeSet<_>, CandidateError>>()?;
        if fields.is_empty() {
            return Err(CandidateError::InvalidOverride(
                "an override must name at least one field".into(),
            ));
        }
        Ok(Self {
            id,
            target: target.erase(),
            fields,
            precedence,
            source,
        })
    }

    #[must_use]
    pub fn id(&self) -> &OverrideId {
        &self.id
    }

    #[must_use]
    pub fn target(&self) -> &ErasedRef {
        &self.target
    }

    #[must_use]
    pub fn fields(&self) -> &BTreeSet<String> {
        &self.fields
    }

    #[must_use]
    pub const fn precedence(&self) -> i32 {
        self.precedence
    }

    #[must_use]
    pub fn source(&self) -> &SourceAnchor {
        &self.source
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReferenceCatalog {
    addresses: BTreeSet<LogicalAddress>,
}

impl ReferenceCatalog {
    pub fn insert<K: RefKind>(
        &mut self,
        reference: &TypedRef<K>,
        identity: &EvaluationIdentity,
    ) -> Result<(), CandidateError> {
        reference.validate(identity)?;
        self.addresses.insert(reference.address.erase());
        Ok(())
    }

    fn contains(&self, address: &LogicalAddress) -> bool {
        self.addresses.contains(address)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateIr {
    identity: EvaluationIdentity,
    origin: CandidateOrigin,
    declarations: Vec<DeclarationIr>,
    references: Vec<ReferenceUse>,
    contributions: Vec<ContributionIr>,
    overrides: Vec<OverrideIr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate(Arc<CandidateIr>);

impl Candidate {
    #[must_use]
    pub fn identity(&self) -> &EvaluationIdentity {
        &self.0.identity
    }

    #[must_use]
    pub fn origin(&self) -> CandidateOrigin {
        self.0.origin
    }

    #[must_use]
    pub fn declarations(&self) -> &[DeclarationIr] {
        &self.0.declarations
    }

    #[must_use]
    pub fn references(&self) -> &[ReferenceUse] {
        &self.0.references
    }

    #[must_use]
    pub fn contributions(&self) -> &[ContributionIr] {
        &self.0.contributions
    }

    #[must_use]
    pub fn overrides(&self) -> &[OverrideIr] {
        &self.0.overrides
    }
}

#[derive(Debug)]
pub struct CandidateDraft {
    identity: EvaluationIdentity,
    origin: CandidateOrigin,
    declarations: BTreeMap<LogicalAddress, DeclarationIr>,
    references: BTreeMap<LogicalAddress, ReferenceUse>,
    contributions: BTreeMap<ContributionId, ContributionIr>,
    overrides: BTreeMap<OverrideId, OverrideIr>,
}

impl CandidateDraft {
    #[must_use]
    pub fn new(identity: EvaluationIdentity, origin: CandidateOrigin) -> Self {
        Self {
            identity,
            origin,
            declarations: BTreeMap::new(),
            references: BTreeMap::new(),
            contributions: BTreeMap::new(),
            overrides: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn identity(&self) -> &EvaluationIdentity {
        &self.identity
    }

    #[must_use]
    pub fn declaration_count(&self) -> usize {
        self.declarations.len()
    }

    pub fn declare<K: RefKind>(
        &mut self,
        declaration: DeclarationIr,
    ) -> Result<TypedRef<K>, CandidateError> {
        let typed = TypedAddress::<K>::from_untyped(declaration.address.clone())?;
        self.declare_erased(declaration)?;
        Ok(TypedRef::new(self.identity.clone(), typed))
    }

    pub fn declare_erased(
        &mut self,
        declaration: DeclarationIr,
    ) -> Result<ErasedRef, CandidateError> {
        declaration.lifecycle.validate()?;
        if declaration.lifecycle.composition != declaration.owner.composition() {
            return Err(CandidateError::InvalidLifecycle(format!(
                "composition {:?} does not match owner {:?}",
                declaration.lifecycle.composition, declaration.owner
            )));
        }
        if let Some(first) = self.declarations.get(&declaration.address) {
            return Err(CandidateError::DuplicateDeclaration {
                address: Box::new(declaration.address.clone()),
                first: Box::new(first.source.clone()),
                duplicate: Box::new(declaration.source.clone()),
            });
        }
        if let DeclarationOwner::Contribution(id) = &declaration.owner {
            let contribution = self
                .contributions
                .get_mut(id)
                .ok_or_else(|| CandidateError::UnknownContribution(id.clone()))?;
            contribution
                .owned_declarations
                .insert(declaration.address.clone());
        }
        let address = declaration.address.clone();
        self.declarations.insert(address.clone(), declaration);
        Ok(ErasedRef {
            identity: self.identity.clone(),
            address,
        })
    }

    pub fn add_reference<K: RefKind>(
        &mut self,
        reference: &TypedRef<K>,
        source: SourceAnchor,
    ) -> Result<(), CandidateError> {
        reference.validate(&self.identity)?;
        self.references
            .entry(reference.address.erase())
            .or_insert_with(|| ReferenceUse {
                reference: reference.erase(),
                source,
            });
        Ok(())
    }

    pub fn add_contribution(&mut self, contribution: ContributionIr) -> Result<(), CandidateError> {
        contribution
            .target_group
            .identity
            .ensure_compatible(&self.identity)?;
        if self.contributions.contains_key(&contribution.id) {
            return Err(CandidateError::DuplicateContribution(contribution.id));
        }
        self.contributions
            .insert(contribution.id.clone(), contribution);
        Ok(())
    }

    pub fn add_override(&mut self, override_ir: OverrideIr) -> Result<(), CandidateError> {
        override_ir
            .target
            .identity
            .ensure_compatible(&self.identity)?;
        if self.overrides.contains_key(&override_ir.id) {
            return Err(CandidateError::DuplicateOverride(override_ir.id));
        }
        for existing in self.overrides.values() {
            if existing.target.address == override_ir.target.address
                && existing.precedence == override_ir.precedence
                && !existing.fields.is_disjoint(&override_ir.fields)
            {
                return Err(CandidateError::OverrideConflict {
                    target: override_ir.target.address.clone(),
                    precedence: override_ir.precedence,
                });
            }
        }
        self.overrides.insert(override_ir.id.clone(), override_ir);
        Ok(())
    }

    pub fn finish(self, catalog: &ReferenceCatalog) -> Result<Candidate, CandidateError> {
        let target_exists = |address: &LogicalAddress| {
            self.declarations.contains_key(address) || catalog.contains(address)
        };
        for reference in self.references.values() {
            if !target_exists(reference.reference.address()) {
                return Err(CandidateError::UnresolvedReference(
                    reference.reference.address().clone(),
                ));
            }
        }
        for contribution in self.contributions.values() {
            if !target_exists(contribution.target_group.address()) {
                return Err(CandidateError::UnresolvedReference(
                    contribution.target_group.address().clone(),
                ));
            }
        }
        for override_ir in self.overrides.values() {
            if !target_exists(override_ir.target.address()) {
                return Err(CandidateError::UnresolvedReference(
                    override_ir.target.address().clone(),
                ));
            }
        }
        for declaration in self.declarations.values() {
            match &declaration.owner {
                DeclarationOwner::Parent(parent) if !target_exists(parent) => {
                    return Err(CandidateError::UnresolvedReference(parent.clone()));
                }
                DeclarationOwner::Override(id) if !self.overrides.contains_key(id) => {
                    return Err(CandidateError::UnknownOverride(id.clone()));
                }
                _ => {}
            }
        }

        let mut contributions: Vec<_> = self.contributions.into_values().collect();
        contributions.sort_by(|left, right| {
            right
                .explicit_order
                .is_some()
                .cmp(&left.explicit_order.is_some())
                .then_with(|| left.explicit_order.cmp(&right.explicit_order))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(Candidate(Arc::new(CandidateIr {
            identity: self.identity,
            origin: self.origin,
            declarations: self.declarations.into_values().collect(),
            references: self.references.into_values().collect(),
            contributions,
            overrides: self.overrides.into_values().collect(),
        })))
    }
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum CompatibilityError {
    #[error("language contract mismatch: expected {expected:?}, got {actual:?}")]
    Contract {
        expected: LanguageContract,
        actual: LanguageContract,
    },
    #[error("engine instance mismatch: expected {expected}, got {actual}")]
    Engine {
        expected: EngineInstanceId,
        actual: EngineInstanceId,
    },
    #[error("runtime epoch mismatch: expected {expected}, got {actual}")]
    Epoch {
        expected: RuntimeEpoch,
        actual: RuntimeEpoch,
    },
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum CandidateError {
    #[error("invalid candidate identity: {0}")]
    InvalidIdentity(String),
    #[error("invalid logical address: {0}")]
    InvalidAddress(String),
    #[error("typed Ref kind mismatch: expected {expected}, got {actual}")]
    WrongRefKind {
        expected: EntityKind,
        actual: EntityKind,
    },
    #[error("duplicate declaration: {address}")]
    DuplicateDeclaration {
        address: Box<LogicalAddress>,
        first: Box<SourceAnchor>,
        duplicate: Box<SourceAnchor>,
    },
    #[error("unresolved reference: {0}")]
    UnresolvedReference(LogicalAddress),
    #[error("duplicate contribution: {0}")]
    DuplicateContribution(ContributionId),
    #[error("unknown contribution: {0}")]
    UnknownContribution(ContributionId),
    #[error("duplicate override: {0:?}")]
    DuplicateOverride(OverrideId),
    #[error("unknown override owner: {0:?}")]
    UnknownOverride(OverrideId),
    #[error("override conflict on {target} at precedence {precedence}")]
    OverrideConflict {
        target: LogicalAddress,
        precedence: i32,
    },
    #[error("invalid override: {0}")]
    InvalidOverride(String),
    #[error("invalid lifecycle metadata: {0}")]
    InvalidLifecycle(String),
    #[error("external effects cannot be part of a required-atomic Candidate")]
    ExternalEffect,
    #[error(transparent)]
    Compatibility(#[from] CompatibilityError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(seed: u8) -> LanguageContract {
        LanguageContract::v2(ContractDigest::from_bytes(&[seed]))
    }

    fn identity(seed: u8) -> EvaluationIdentity {
        EvaluationIdentity::new(contract(seed), EngineInstanceId::new(), RuntimeEpoch::new())
    }

    fn module() -> ModulePath {
        ModulePath::new("song/main").unwrap()
    }

    fn address<K: RefKind>(key: &str) -> TypedAddress<K> {
        TypedAddress::new(
            ProjectNamespace::new("test-project").unwrap(),
            module(),
            GroupScope::root(),
            DeclarationKey::new(key).unwrap(),
        )
    }

    fn source(index: u32) -> SourceAnchor {
        SourceAnchor::new(
            module(),
            SyntaxKey::deterministic(&module(), &[index], "declaration").unwrap(),
            None,
        )
    }

    fn declaration<K: RefKind>(key: &str) -> DeclarationIr {
        DeclarationIr::new(
            address::<K>(key),
            DeclarationOwner::Structural(
                SyntaxKey::deterministic(&module(), &[1], "owner").unwrap(),
            ),
            source(1),
            LifecycleMetadata::register(Composition::Standalone),
            DeclarationPayload::Empty,
        )
    }

    #[test]
    fn candidate_is_immutable_and_clone_shares_only_frozen_ir() {
        let identity = identity(1);
        let mut draft = CandidateDraft::new(identity, CandidateOrigin::RhaiHost);
        draft
            .declare::<VoiceKind>(declaration::<VoiceKind>("lead"))
            .unwrap();
        let candidate = draft.finish(&ReferenceCatalog::default()).unwrap();
        let cloned = candidate.clone();

        assert_eq!(candidate.declarations(), cloned.declarations());
        assert_eq!(candidate.declarations().len(), 1);
    }

    #[test]
    fn exact_duplicate_rejects_without_overwriting_the_first_declaration() {
        let mut draft = CandidateDraft::new(identity(1), CandidateOrigin::RhaiHost);
        draft
            .declare::<VoiceKind>(declaration::<VoiceKind>("lead"))
            .unwrap();
        let error = draft
            .declare::<VoiceKind>(declaration::<VoiceKind>("lead"))
            .unwrap_err();

        assert!(matches!(error, CandidateError::DuplicateDeclaration { .. }));
        assert_eq!(draft.declaration_count(), 1);
    }

    #[test]
    fn wrong_kind_rejects_before_declaration_or_owner_residue() {
        let identity = identity(1);
        let contribution_id = ContributionId::new(
            module(),
            SyntaxKey::deterministic(&module(), &[8], "body").unwrap(),
        );
        let group = TypedRef::new(identity.clone(), address::<GroupKind>("root"));
        let mut draft = CandidateDraft::new(identity, CandidateOrigin::RhaiHost);
        draft
            .add_contribution(ContributionIr::new(
                contribution_id.clone(),
                group,
                None,
                source(8),
            ))
            .unwrap();
        let declaration = DeclarationIr::new(
            address::<VoiceKind>("lead"),
            DeclarationOwner::Contribution(contribution_id.clone()),
            source(9),
            LifecycleMetadata::register(Composition::Contribution),
            DeclarationPayload::Empty,
        );

        assert!(matches!(
            draft.declare::<PatternKind>(declaration),
            Err(CandidateError::WrongRefKind {
                expected: EntityKind::Pattern,
                actual: EntityKind::Voice,
            })
        ));
        assert_eq!(draft.declaration_count(), 0);
        assert!(draft
            .contributions
            .get(&contribution_id)
            .unwrap()
            .owned_declarations
            .is_empty());
    }

    #[test]
    fn typed_namespaces_allow_cross_kind_key_reuse() {
        let mut draft = CandidateDraft::new(identity(1), CandidateOrigin::RhaiHost);
        draft
            .declare::<VoiceKind>(declaration::<VoiceKind>("lead"))
            .unwrap();
        draft
            .declare::<PatternKind>(declaration::<PatternKind>("lead"))
            .unwrap();
        assert_eq!(
            draft
                .finish(&ReferenceCatalog::default())
                .unwrap()
                .declarations()
                .len(),
            2
        );
    }

    #[test]
    fn unresolved_reference_rejects_the_whole_candidate() {
        let identity = identity(1);
        let reference = TypedRef::new(identity.clone(), address::<VoiceKind>("missing"));
        let mut draft = CandidateDraft::new(identity, CandidateOrigin::RhaiHost);
        draft.add_reference(&reference, source(2)).unwrap();

        assert!(matches!(
            draft.finish(&ReferenceCatalog::default()),
            Err(CandidateError::UnresolvedReference(_))
        ));
    }

    #[test]
    fn reference_catalog_resolves_existing_typed_addresses() {
        let identity = identity(1);
        let reference = TypedRef::new(identity.clone(), address::<VoiceKind>("existing"));
        let mut catalog = ReferenceCatalog::default();
        catalog.insert(&reference, &identity).unwrap();
        let mut draft = CandidateDraft::new(identity, CandidateOrigin::RhaiHost);
        draft.add_reference(&reference, source(2)).unwrap();

        assert_eq!(draft.finish(&catalog).unwrap().references().len(), 1);
    }

    #[test]
    fn contract_engine_and_epoch_misuse_reject_before_candidate_residue() {
        let expected = identity(1);
        let base_address = address::<VoiceKind>("lead");
        let wrong_contract = TypedRef::new(identity(2), base_address.clone());
        let wrong_engine = TypedRef::new(
            EvaluationIdentity::new(
                expected.language.clone(),
                EngineInstanceId::new(),
                expected.runtime_epoch,
            ),
            base_address.clone(),
        );
        let wrong_epoch = TypedRef::new(
            EvaluationIdentity::new(
                expected.language.clone(),
                expected.engine_instance,
                RuntimeEpoch::new(),
            ),
            base_address,
        );
        let mut draft = CandidateDraft::new(expected, CandidateOrigin::RhaiHost);

        for reference in [&wrong_contract, &wrong_engine, &wrong_epoch] {
            assert!(matches!(
                draft.add_reference(reference, source(3)),
                Err(CandidateError::Compatibility(_))
            ));
            assert_eq!(draft.declaration_count(), 0);
            assert!(draft.references.is_empty());
        }
    }

    #[test]
    fn persisted_ref_address_requires_exact_contract_before_reresolution() {
        let original = identity(1);
        let reference = TypedRef::new(original.clone(), address::<VoiceKind>("lead"));
        let persisted = reference.persisted_address();
        let target =
            EvaluationIdentity::new(contract(2), EngineInstanceId::new(), RuntimeEpoch::new());

        assert!(matches!(
            persisted.resolve(&target),
            Err(CompatibilityError::Contract { .. })
        ));
        let replacement_identity = EvaluationIdentity::new(
            original.language.clone(),
            EngineInstanceId::new(),
            RuntimeEpoch::new(),
        );
        let resolved = persisted.resolve(&replacement_identity).unwrap();
        assert_eq!(resolved.identity(), &replacement_identity);
        assert_eq!(resolved.address(), reference.address());
    }

    #[test]
    fn deterministic_syntax_keys_ignore_source_line_numbers() {
        let left = SyntaxKey::deterministic(&module(), &[2, 4, 1], "body").unwrap();
        let right = SyntaxKey::deterministic(&module(), &[2, 4, 1], "body").unwrap();
        let moved = SyntaxKey::deterministic(&module(), &[2, 5, 1], "body").unwrap();
        assert_eq!(left, right);
        assert_ne!(left, moved);
    }

    #[test]
    fn external_effect_metadata_is_rejected_without_declaration_residue() {
        let mut lifecycle = LifecycleMetadata::register(Composition::Standalone);
        lifecycle.effect_domain = EffectDomain::External;
        let mut declaration = declaration::<VoiceKind>("lead");
        declaration.lifecycle = lifecycle;
        let mut draft = CandidateDraft::new(identity(1), CandidateOrigin::RhaiHost);

        assert_eq!(
            draft.declare::<VoiceKind>(declaration),
            Err(CandidateError::ExternalEffect)
        );
        assert_eq!(draft.declaration_count(), 0);
    }
}
