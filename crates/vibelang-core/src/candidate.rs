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
use vibelang_dsp::{DspDefinitionIr, DspDefinitionKind, PortRate, StagedDspRegistry};

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
    pub fn new(identity: EvaluationIdentity, address: LogicalAddress) -> Self {
        Self { identity, address }
    }

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

impl fmt::Display for OverrideId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
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

    #[must_use]
    pub fn start(composition: Composition) -> Self {
        Self {
            role: AuthoringRole::Builder,
            phase: LifecyclePhase::Register,
            effects: BTreeSet::from([LifecycleEffect::Register, LifecycleEffect::Start]),
            terminal_effect: TerminalEffect::Start,
            synchronization: Synchronization::CandidateLocal,
            cancellation: Cancellation::BeforePlanning,
            composition,
            effect_domain: EffectDomain::Managed,
        }
    }

    #[must_use]
    pub fn reference(effect: TerminalEffect, cancellation: Cancellation) -> Self {
        Self {
            role: AuthoringRole::Ref,
            phase: LifecyclePhase::Register,
            effects: BTreeSet::from([effect.lifecycle_effect()]),
            terminal_effect: effect,
            synchronization: Synchronization::CandidateLocal,
            cancellation,
            composition: Composition::Reference,
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CanonicalF64(u64);

impl CanonicalF64 {
    pub fn new(value: f64) -> Result<Self, CandidateError> {
        if !value.is_finite() {
            return Err(CandidateError::InvalidAuthoring(
                "authoring numbers must be finite".into(),
            ));
        }
        let value = if value == 0.0 { 0.0 } else { value };
        Ok(Self(value.to_bits()))
    }

    #[must_use]
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }

    #[must_use]
    const fn bits(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StartMode {
    Normal,
    Immediate,
    Continuous,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DesiredLifecycle {
    Dormant,
    Start(StartMode),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupAuthoring {
    pub parent: Option<TypedRef<GroupKind>>,
    pub gain: CanonicalF64,
    pub muted: bool,
    pub soloed: bool,
    pub params: BTreeMap<String, CanonicalF64>,
    pub output_channels: Option<(u32, u8)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VoiceSourceAuthoring {
    SynthDef(String),
    Sfz(TypedRef<SfzKind>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceAuthoring {
    pub group: Option<TypedRef<GroupKind>>,
    pub source: VoiceSourceAuthoring,
    pub polyphony: u32,
    pub gain: CanonicalF64,
    pub params: BTreeMap<String, CanonicalF64>,
    pub muted: bool,
    pub soloed: bool,
    pub lifecycle: DesiredLifecycle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternStepAuthoring {
    pub beat_ticks: i64,
    pub velocity: CanonicalF64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternAuthoring {
    pub voice: TypedRef<VoiceKind>,
    pub steps: Vec<PatternStepAuthoring>,
    pub length_ticks: i64,
    pub swing: CanonicalF64,
    pub params: BTreeMap<String, CanonicalF64>,
    pub lifecycle: DesiredLifecycle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MelodyEventAuthoring {
    Note {
        beat_ticks: i64,
        duration_ticks: i64,
        midi_note: u8,
        velocity: CanonicalF64,
    },
    Rest {
        beat_ticks: i64,
        duration_ticks: i64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MelodyAuthoring {
    pub voice: TypedRef<VoiceKind>,
    pub events: Vec<MelodyEventAuthoring>,
    pub length_ticks: i64,
    pub lifecycle: DesiredLifecycle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SequenceContentAuthoring {
    Pattern(TypedRef<PatternKind>),
    Melody(TypedRef<MelodyKind>),
    Fade(TypedRef<FadeKind>),
    Sequence(TypedRef<SequenceKind>),
}

impl SequenceContentAuthoring {
    #[must_use]
    pub fn reference(&self) -> ErasedRef {
        match self {
            Self::Pattern(reference) => reference.erase(),
            Self::Melody(reference) => reference.erase(),
            Self::Fade(reference) => reference.erase(),
            Self::Sequence(reference) => reference.erase(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequenceClipAuthoring {
    pub start_ticks: i64,
    pub end_ticks: i64,
    pub content: SequenceContentAuthoring,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequenceAuthoring {
    pub clips: Vec<SequenceClipAuthoring>,
    pub length_ticks: i64,
    pub looping: bool,
    pub lifecycle: DesiredLifecycle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FadeTargetAuthoring {
    Group(TypedRef<GroupKind>),
    Voice(TypedRef<VoiceKind>),
    Effect(TypedRef<EffectKind>),
    Pattern(TypedRef<PatternKind>),
    Melody(TypedRef<MelodyKind>),
}

impl FadeTargetAuthoring {
    #[must_use]
    pub fn reference(&self) -> ErasedRef {
        match self {
            Self::Group(reference) => reference.erase(),
            Self::Voice(reference) => reference.erase(),
            Self::Effect(reference) => reference.erase(),
            Self::Pattern(reference) => reference.erase(),
            Self::Melody(reference) => reference.erase(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FadePointAuthoring {
    pub time: CanonicalF64,
    pub value: CanonicalF64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FadeCurveAuthoring {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    SineIn,
    SineOut,
    SineInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    Exponential(CanonicalF64),
    Logarithmic,
    Step,
    CubicSpline(Vec<FadePointAuthoring>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FadeAuthoring {
    pub target: FadeTargetAuthoring,
    pub parameter: String,
    pub from: CanonicalF64,
    pub to: CanonicalF64,
    pub duration_ticks: i64,
    pub curve: FadeCurveAuthoring,
    pub lifecycle: DesiredLifecycle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectAuthoring {
    pub definition: TypedRef<EffectDefKind>,
    pub params: BTreeMap<String, CanonicalF64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DspDefinitionAuthoring {
    pub definition: DspDefinitionIr,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SampleTriggerAuthoring {
    Gate,
    OneShot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SampleWarpAuthoring {
    pub speed: CanonicalF64,
    pub pitch: CanonicalF64,
    pub target_bpm: Option<CanonicalF64>,
    pub window_size: CanonicalF64,
    pub overlaps: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SampleAuthoring {
    pub source: String,
    pub attack: CanonicalF64,
    pub sustain: CanonicalF64,
    pub release: CanonicalF64,
    pub amp: CanonicalF64,
    pub rate: CanonicalF64,
    pub loop_mode: bool,
    pub offset: CanonicalF64,
    pub length: Option<CanonicalF64>,
    pub trigger: SampleTriggerAuthoring,
    pub warp: Option<SampleWarpAuthoring>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BufferReplacementAuthoring {
    Clear,
    CopyOverlap,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BufferAuthoring {
    pub frames: u32,
    pub channels: u16,
    pub replacement: BufferReplacementAuthoring,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SfzAuthoring {
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordingLengthAuthoring {
    Beats(i64),
    Seconds(CanonicalF64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingAuthoring {
    pub source: TypedRef<GroupKind>,
    pub length: Option<RecordingLengthAuthoring>,
    pub count_in_ticks: i64,
    pub metronome: bool,
    pub destination: Option<String>,
    pub channels: u8,
    pub lifecycle: DesiredLifecycle,
}

fn validate_resource_source(value: &str, role: &str) -> Result<(), CandidateError> {
    if value.is_empty() || value.trim() != value || value.bytes().any(|byte| byte < 0x20) {
        return Err(CandidateError::InvalidAuthoring(format!(
            "{role} must be a non-empty path without surrounding whitespace or control bytes"
        )));
    }
    Ok(())
}

/// Rate a v2 route terminal declares for its source port.
///
/// The verb fixes the declared rate — SET and BEND are control-rate, A2K
/// coerces an audio-rate source into the SET registry, TRIGGER is
/// trigger-rate, and the audio destinations require audio rate. The runtime
/// cross-checks the declared rate against the synthdef's real port rate at
/// plan time; a mismatch is a structured plan rejection, never a coercion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RoutePortRate {
    Audio,
    Control,
    Trigger,
}

/// A named output port on a source voice, rate-qualified by the verb.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePortAuthoring {
    pub voice: TypedRef<VoiceKind>,
    pub port: String,
    pub rate: RoutePortRate,
}

/// Target entity of a param route: a Voice's or an Effect's parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteTargetAuthoring {
    Voice(TypedRef<VoiceKind>),
    Effect(TypedRef<EffectKind>),
}

impl RouteTargetAuthoring {
    #[must_use]
    pub fn reference(&self) -> ErasedRef {
        match self {
            Self::Voice(reference) => reference.erase(),
            Self::Effect(reference) => reference.erase(),
        }
    }

    #[must_use]
    pub fn address(&self) -> LogicalAddress {
        match self {
            Self::Voice(reference) => reference.address().erase(),
            Self::Effect(reference) => reference.address().erase(),
        }
    }

    const fn key_prefix(&self) -> &'static str {
        match self {
            Self::Voice(_) => "voice",
            Self::Effect(_) => "effect",
        }
    }
}

/// One ordered fan-out destination of an audio route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioRouteDestinationAuthoring {
    Group(TypedRef<GroupKind>),
    Main,
    Muted,
}

/// Per-edge SET/BEND shaping. TRIGGER routes carry none — triggers don't
/// bend, matching the v1 registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteShapingAuthoring {
    pub scale: CanonicalF64,
    pub offset: CanonicalF64,
}

impl RouteShapingAuthoring {
    #[must_use]
    pub fn identity() -> Self {
        Self {
            scale: CanonicalF64::new(1.0).expect("1.0 is finite"),
            offset: CanonicalF64::new(0.0).expect("0.0 is finite"),
        }
    }
}

impl Default for RouteShapingAuthoring {
    fn default() -> Self {
        Self::identity()
    }
}

/// Source of a single-source named-input wiring.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputRouteSourceAuthoring {
    /// An audio-rate output port on a source voice.
    VoicePort {
        voice: TypedRef<VoiceKind>,
        port: String,
    },
    /// A group's mix bus.
    Group(TypedRef<GroupKind>),
    /// The shared silent bus — an explicit disconnect declaration.
    Silent,
}

/// Verb namespace of a param route. The three verbs feed one shared
/// modulation registry and stay mutually exclusive per `(target, param)`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RouteVerb {
    Set,
    Bend,
    Trigger,
}

impl fmt::Display for RouteVerb {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", format!("{self:?}").to_ascii_lowercase())
    }
}

/// V2 route declarations under [`EntityKind::Route`].
///
/// Route identity is target-side for the single-writer verbs (SET, A2K, and
/// TRIGGER own the registry slot for their `(target, param)`), edge-wise for
/// additive BEND fan-in, and port-side for audio and input routes. A repeated
/// single-writer terminal therefore lands on the same logical address and is
/// a duplicate-declaration error, preserving the v1 registry's
/// one-writer-per-slot rule as a candidate validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteAuthoring {
    /// Ordered audio fan-out from one source port. Group destinations are
    /// additive edges; Main and Muted replace — the variants stay mutually
    /// exclusive on a single port exactly like the v1 route map.
    Audio {
        source: RoutePortAuthoring,
        destinations: Vec<AudioRouteDestinationAuthoring>,
    },
    /// Single-source named-input wiring, replace-by-declaration.
    Input {
        target: TypedRef<VoiceKind>,
        input_port: String,
        source: InputRouteSourceAuthoring,
    },
    /// SET: one source writes the target param. `coerce_audio` is the A2K
    /// variant — an audio-rate source downsampled into the same SET slot.
    Set {
        source: RoutePortAuthoring,
        coerce_audio: bool,
        target: RouteTargetAuthoring,
        target_param: String,
        shaping: RouteShapingAuthoring,
    },
    /// BEND: additive fan-in modulation edge onto the target param.
    Bend {
        source: RoutePortAuthoring,
        target: RouteTargetAuthoring,
        target_param: String,
        shaping: RouteShapingAuthoring,
    },
    /// TRIGGER: 1:1 trigger-rate edge, no shaping, no fan-in.
    Trigger {
        source: RoutePortAuthoring,
        target: RouteTargetAuthoring,
        target_param: String,
    },
}

fn scoped_key_component(address: &LogicalAddress) -> String {
    let mut component = String::new();
    for scope in address.group_scope().components() {
        component.push_str(scope.as_str());
        component.push('.');
    }
    component.push_str(address.key().as_str());
    component
}

impl RouteAuthoring {
    /// Derive the stable declaration key encoding this route's identity.
    pub fn canonical_key(&self) -> Result<DeclarationKey, CandidateError> {
        let key = match self {
            Self::Audio { source, .. } => format!(
                "audio.{}.{}",
                scoped_key_component(&source.voice.address().erase()),
                source.port
            ),
            Self::Input {
                target, input_port, ..
            } => format!(
                "input.{}.{}",
                scoped_key_component(&target.address().erase()),
                input_port
            ),
            Self::Set {
                target,
                target_param,
                ..
            } => format!(
                "set.{}.{}.{}",
                target.key_prefix(),
                scoped_key_component(&target.address()),
                target_param
            ),
            Self::Bend {
                source,
                target,
                target_param,
                ..
            } => format!(
                "bend.{}.{}.{}.{}.{}",
                target.key_prefix(),
                scoped_key_component(&target.address()),
                target_param,
                scoped_key_component(&source.voice.address().erase()),
                source.port
            ),
            Self::Trigger {
                target,
                target_param,
                ..
            } => format!(
                "trigger.{}.{}.{}",
                target.key_prefix(),
                scoped_key_component(&target.address()),
                target_param
            ),
        };
        DeclarationKey::new(key)
    }

    /// The shared-registry slot this route occupies, if it is a param route.
    #[must_use]
    pub fn registry_slot(&self) -> Option<(RouteVerb, LogicalAddress, &str)> {
        match self {
            Self::Set {
                target,
                target_param,
                ..
            } => Some((RouteVerb::Set, target.address(), target_param.as_str())),
            Self::Bend {
                target,
                target_param,
                ..
            } => Some((RouteVerb::Bend, target.address(), target_param.as_str())),
            Self::Trigger {
                target,
                target_param,
                ..
            } => Some((RouteVerb::Trigger, target.address(), target_param.as_str())),
            Self::Audio { .. } | Self::Input { .. } => None,
        }
    }
}

/// A MIDI channel. The public spelling is uniformly 1-16; the zero-based
/// index exists only behind the explicit `from_index` API per the M09
/// roadmap. Out-of-range values reject strictly — the v1 clamp has no v2
/// respelling.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MidiChannel(u8);

impl MidiChannel {
    /// Public one-based channel number, 1..=16.
    pub fn from_number(number: i64) -> Result<Self, CandidateError> {
        if !(1..=16).contains(&number) {
            return Err(CandidateError::InvalidAuthoring(format!(
                "MIDI channel numbers are 1..=16, got {number}"
            )));
        }
        Ok(Self((number - 1) as u8))
    }

    /// Explicit zero-based channel index, 0..=15.
    pub fn from_index(index: i64) -> Result<Self, CandidateError> {
        if !(0..=15).contains(&index) {
            return Err(CandidateError::InvalidAuthoring(format!(
                "MIDI channel indexes are 0..=15, got {index}"
            )));
        }
        Ok(Self(index as u8))
    }

    #[must_use]
    pub const fn number(self) -> u8 {
        self.0 + 1
    }

    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// A MIDI 2.0 group with the same public 1-16 / explicit-index contract
/// as [`MidiChannel`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MidiGroup(u8);

impl MidiGroup {
    /// Public one-based group number, 1..=16.
    pub fn from_number(number: i64) -> Result<Self, CandidateError> {
        if !(1..=16).contains(&number) {
            return Err(CandidateError::InvalidAuthoring(format!(
                "MIDI group numbers are 1..=16, got {number}"
            )));
        }
        Ok(Self((number - 1) as u8))
    }

    /// Explicit zero-based group index, 0..=15.
    pub fn from_index(index: i64) -> Result<Self, CandidateError> {
        if !(0..=15).contains(&index) {
            return Err(CandidateError::InvalidAuthoring(format!(
                "MIDI group indexes are 0..=15, got {index}"
            )));
        }
        Ok(Self(index as u8))
    }

    #[must_use]
    pub const fn number(self) -> u8 {
        self.0 + 1
    }

    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// A width-qualified MIDI data value. Every constructor names its width
/// and rejects out-of-range input strictly — v1 clamping has no v2
/// respelling.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MidiValue {
    V7(u8),
    V14(u16),
    V16(u16),
    V32(u32),
}

impl MidiValue {
    fn ranged(value: i64, max: i64, width: &'static str) -> Result<i64, CandidateError> {
        if !(0..=max).contains(&value) {
            return Err(CandidateError::InvalidAuthoring(format!(
                "{width} MIDI values are 0..={max}, got {value}"
            )));
        }
        Ok(value)
    }

    pub fn v7(value: i64) -> Result<Self, CandidateError> {
        Ok(Self::V7(Self::ranged(value, 127, "7-bit")? as u8))
    }

    pub fn v14(value: i64) -> Result<Self, CandidateError> {
        Ok(Self::V14(Self::ranged(value, 16_383, "14-bit")? as u16))
    }

    pub fn v16(value: i64) -> Result<Self, CandidateError> {
        Ok(Self::V16(Self::ranged(value, 65_535, "16-bit")? as u16))
    }

    pub fn v32(value: i64) -> Result<Self, CandidateError> {
        Ok(Self::V32(
            Self::ranged(value, i64::from(u32::MAX), "32-bit")? as u32,
        ))
    }

    #[must_use]
    pub const fn width_bits(self) -> u8 {
        match self {
            Self::V7(_) => 7,
            Self::V14(_) => 14,
            Self::V16(_) => 16,
            Self::V32(_) => 32,
        }
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        match self {
            Self::V7(value) => value as u32,
            Self::V14(value) | Self::V16(value) => value as u32,
            Self::V32(value) => value,
        }
    }
}

/// Which direction(s) a declared MIDI device binding covers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MidiDeviceDirectionAuthoring {
    Input,
    Output,
    Bidirectional,
}

/// V2 MIDI device declarations bind one exact port name. The v1 partial
/// case-insensitive match and the not-found sentinel device have no v2
/// respelling — resolution failures are typed results, never placeholder
/// handles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiDeviceAuthoring {
    pub port: String,
    pub direction: MidiDeviceDirectionAuthoring,
}

/// V2 MIDI route declarations under [`EntityKind::MidiRoute`].
///
/// Keyboard routes are edge-wise (one declaration per device/channel/
/// voice edge, matching the additive v1 route list); CC routes are
/// target-side single-writer (a second CC writer on the same target
/// parameter is a duplicate declaration).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MidiRouteAuthoring {
    Keyboard {
        device: TypedRef<MidiDeviceKind>,
        /// `None` listens on all channels.
        channel: Option<MidiChannel>,
        voice: TypedRef<VoiceKind>,
    },
    Cc {
        device: TypedRef<MidiDeviceKind>,
        /// `None` listens on all channels.
        channel: Option<MidiChannel>,
        controller: u8,
        target: RouteTargetAuthoring,
        target_param: String,
        /// Mapped value at controller 0. Reverse mappings (min > max)
        /// stay legal exactly as in v1.
        min: CanonicalF64,
        /// Mapped value at the controller maximum.
        max: CanonicalF64,
    },
}

fn midi_channel_key_component(channel: Option<MidiChannel>) -> String {
    channel.map_or_else(|| "all".into(), |channel| channel.number().to_string())
}

impl MidiRouteAuthoring {
    /// Derive the stable declaration key encoding this route's identity.
    pub fn canonical_key(&self) -> Result<DeclarationKey, CandidateError> {
        let key = match self {
            Self::Keyboard {
                device,
                channel,
                voice,
            } => format!(
                "keyboard.{}.{}.{}",
                scoped_key_component(&device.address().erase()),
                midi_channel_key_component(*channel),
                scoped_key_component(&voice.address().erase())
            ),
            Self::Cc {
                target,
                target_param,
                ..
            } => format!(
                "cc.{}.{}.{}",
                target.key_prefix(),
                scoped_key_component(&target.address()),
                target_param
            ),
        };
        DeclarationKey::new(key)
    }
}

/// What fires a V2 MIDI callback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallbackTriggerAuthoring {
    NoteOn {
        channel: Option<MidiChannel>,
        /// `None` fires for every note.
        note: Option<u8>,
    },
    ControlChange {
        channel: Option<MidiChannel>,
        /// `None` fires for every controller.
        controller: Option<u8>,
    },
    ClockSync,
    AnyMessage,
}

/// V2 MIDI callback declarations under [`EntityKind::Callback`]. The
/// handler is a named script function — candidate IR is pure data, so
/// the v1 captured-closure spelling has no v2 respelling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallbackAuthoring {
    pub device: TypedRef<MidiDeviceKind>,
    pub trigger: CallbackTriggerAuthoring,
    pub handler: String,
}

impl CallbackAuthoring {
    /// Derive the stable declaration key encoding this callback's
    /// identity: one declaration per device/trigger/handler edge.
    pub fn canonical_key(&self) -> Result<DeclarationKey, CandidateError> {
        let trigger = match &self.trigger {
            CallbackTriggerAuthoring::NoteOn { channel, note } => format!(
                "note-on.{}.{}",
                midi_channel_key_component(*channel),
                note.map_or_else(|| "any".into(), |note| note.to_string())
            ),
            CallbackTriggerAuthoring::ControlChange {
                channel,
                controller,
            } => format!(
                "control-change.{}.{}",
                midi_channel_key_component(*channel),
                controller.map_or_else(|| "any".into(), |controller| controller.to_string())
            ),
            CallbackTriggerAuthoring::ClockSync => "clock-sync".into(),
            CallbackTriggerAuthoring::AnyMessage => "any-message".into(),
        };
        DeclarationKey::new(format!(
            "callback.{}.{}.{}",
            scoped_key_component(&self.device.address().erase()),
            trigger,
            self.handler
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthoringDeclaration {
    Group(GroupAuthoring),
    Voice(VoiceAuthoring),
    Pattern(PatternAuthoring),
    Melody(MelodyAuthoring),
    Sequence(SequenceAuthoring),
    Fade(FadeAuthoring),
    Effect(EffectAuthoring),
    SynthDef(DspDefinitionAuthoring),
    EffectDef(DspDefinitionAuthoring),
    Sample(SampleAuthoring),
    Buffer(BufferAuthoring),
    Sfz(SfzAuthoring),
    Recording(RecordingAuthoring),
    Route(RouteAuthoring),
    MidiDevice(MidiDeviceAuthoring),
    MidiRoute(MidiRouteAuthoring),
    Callback(CallbackAuthoring),
}

impl AuthoringDeclaration {
    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        match self {
            Self::Group(_) => EntityKind::Group,
            Self::Voice(_) => EntityKind::Voice,
            Self::Pattern(_) => EntityKind::Pattern,
            Self::Melody(_) => EntityKind::Melody,
            Self::Sequence(_) => EntityKind::Sequence,
            Self::Fade(_) => EntityKind::Fade,
            Self::Effect(_) => EntityKind::Effect,
            Self::SynthDef(_) => EntityKind::SynthDef,
            Self::EffectDef(_) => EntityKind::EffectDef,
            Self::Sample(_) => EntityKind::Sample,
            Self::Buffer(_) => EntityKind::Buffer,
            Self::Sfz(_) => EntityKind::Sfz,
            Self::Recording(_) => EntityKind::Recording,
            Self::Route(_) => EntityKind::Route,
            Self::MidiDevice(_) => EntityKind::MidiDevice,
            Self::MidiRoute(_) => EntityKind::MidiRoute,
            Self::Callback(_) => EntityKind::Callback,
        }
    }

    fn desired_lifecycle(&self) -> Option<DesiredLifecycle> {
        match self {
            Self::Group(_) => None,
            Self::Voice(voice) => Some(voice.lifecycle),
            Self::Pattern(pattern) => Some(pattern.lifecycle),
            Self::Melody(melody) => Some(melody.lifecycle),
            Self::Sequence(sequence) => Some(sequence.lifecycle),
            Self::Fade(fade) => Some(fade.lifecycle),
            Self::Effect(_) | Self::SynthDef(_) | Self::EffectDef(_) => None,
            Self::Sample(_) | Self::Buffer(_) | Self::Sfz(_) => None,
            Self::Recording(recording) => Some(recording.lifecycle),
            Self::Route(_) => None,
            Self::MidiDevice(_) | Self::MidiRoute(_) | Self::Callback(_) => None,
        }
    }

    fn validate(&self) -> Result<(), CandidateError> {
        fn validate_params(params: &BTreeMap<String, CanonicalF64>) -> Result<(), CandidateError> {
            for name in params.keys() {
                validate_component(name, "authoring parameter")?;
            }
            Ok(())
        }

        fn validate_timeline_lifecycle(lifecycle: DesiredLifecycle) -> Result<(), CandidateError> {
            if lifecycle == DesiredLifecycle::Start(StartMode::Continuous) {
                return Err(CandidateError::InvalidAuthoring(
                    "continuous lifecycle is Voice-only".into(),
                ));
            }
            Ok(())
        }

        match self {
            Self::Group(group) => {
                validate_params(&group.params)?;
                if let Some((bus, channels)) = group.output_channels {
                    if !matches!(channels, 1 | 2) || bus >= 16 || bus + u32::from(channels) > 16 {
                        return Err(CandidateError::InvalidAuthoring(
                            "Group output must be one or two channels within buses 0..16".into(),
                        ));
                    }
                }
            }
            Self::Voice(voice) => {
                if voice.polyphony == 0 || voice.polyphony > 128 {
                    return Err(CandidateError::InvalidAuthoring(
                        "Voice polyphony must be in 1..=128".into(),
                    ));
                }
                if let VoiceSourceAuthoring::SynthDef(name) = &voice.source {
                    validate_component(name, "Voice synthdef")?;
                }
                validate_params(&voice.params)?;
                if matches!(
                    voice.lifecycle,
                    DesiredLifecycle::Start(StartMode::Normal | StartMode::Immediate)
                ) {
                    return Err(CandidateError::InvalidAuthoring(
                        "Voice uses run/continuous lifecycle rather than start timing".into(),
                    ));
                }
            }
            Self::Pattern(pattern) => {
                validate_timeline_lifecycle(pattern.lifecycle)?;
                validate_params(&pattern.params)?;
                if pattern.length_ticks <= 0 || !(0.0..=1.0).contains(&pattern.swing.get()) {
                    return Err(CandidateError::InvalidAuthoring(
                        "Pattern length must be positive and swing must be in 0.0..=1.0".into(),
                    ));
                }
                if pattern.steps.iter().any(|step| {
                    step.beat_ticks < 0
                        || step.beat_ticks >= pattern.length_ticks
                        || !(0.0..=1.0).contains(&step.velocity.get())
                }) {
                    return Err(CandidateError::InvalidAuthoring(
                        "Pattern steps must be in-range with normalized velocity".into(),
                    ));
                }
            }
            Self::Melody(melody) => {
                validate_timeline_lifecycle(melody.lifecycle)?;
                if melody.length_ticks <= 0 {
                    return Err(CandidateError::InvalidAuthoring(
                        "Melody length must be positive".into(),
                    ));
                }
                for event in &melody.events {
                    let (beat, duration, velocity) = match event {
                        MelodyEventAuthoring::Note {
                            beat_ticks,
                            duration_ticks,
                            velocity,
                            ..
                        } => (*beat_ticks, *duration_ticks, Some(velocity.get())),
                        MelodyEventAuthoring::Rest {
                            beat_ticks,
                            duration_ticks,
                        } => (*beat_ticks, *duration_ticks, None),
                    };
                    if beat < 0
                        || duration <= 0
                        || beat.saturating_add(duration) > melody.length_ticks
                        || velocity.is_some_and(|value| !(0.0..=1.0).contains(&value))
                    {
                        return Err(CandidateError::InvalidAuthoring(
                            "Melody events must be positive, in-range, and normalized".into(),
                        ));
                    }
                }
            }
            Self::Sequence(sequence) => {
                validate_timeline_lifecycle(sequence.lifecycle)?;
                if sequence.length_ticks <= 0 {
                    return Err(CandidateError::InvalidAuthoring(
                        "Sequence length must be positive".into(),
                    ));
                }
                if sequence.clips.iter().any(|clip| {
                    clip.start_ticks < 0
                        || clip.end_ticks <= clip.start_ticks
                        || clip.end_ticks > sequence.length_ticks
                }) {
                    return Err(CandidateError::InvalidAuthoring(
                        "Sequence clips must be non-empty and within the declared length".into(),
                    ));
                }
            }
            Self::Fade(fade) => {
                validate_timeline_lifecycle(fade.lifecycle)?;
                validate_component(&fade.parameter, "Fade parameter")?;
                if fade.duration_ticks <= 0 {
                    return Err(CandidateError::InvalidAuthoring(
                        "Fade duration must be positive".into(),
                    ));
                }
                match &fade.curve {
                    FadeCurveAuthoring::Exponential(exponent)
                        if !exponent.get().is_finite() || exponent.get() <= 0.0 =>
                    {
                        return Err(CandidateError::InvalidAuthoring(
                            "Fade exponential curve needs a finite positive exponent".into(),
                        ));
                    }
                    FadeCurveAuthoring::CubicSpline(points) => {
                        let mut previous = None;
                        for point in points {
                            let time = point.time.get();
                            if !(0.0..1.0).contains(&time)
                                || previous.is_some_and(|previous| time <= previous)
                            {
                                return Err(CandidateError::InvalidAuthoring(
                                    "Fade spline points must have strictly increasing times within 0.0..1.0"
                                        .into(),
                                ));
                            }
                            previous = Some(time);
                        }
                    }
                    _ => {}
                }
            }
            Self::Effect(effect) => validate_params(&effect.params)?,
            Self::SynthDef(definition) => {
                if definition.definition.kind() != DspDefinitionKind::SynthDef {
                    return Err(CandidateError::InvalidDspDefinition(
                        "SynthDef declaration contains effect definition IR".into(),
                    ));
                }
            }
            Self::EffectDef(definition) => {
                if definition.definition.kind() != DspDefinitionKind::Effect {
                    return Err(CandidateError::InvalidDspDefinition(
                        "EffectDef declaration contains synth definition IR".into(),
                    ));
                }
            }
            Self::Sample(sample) => {
                validate_resource_source(&sample.source, "Sample source")?;
                if sample.attack.get() < 0.0
                    || sample.release.get() < 0.0
                    || !(0.0..=1.0).contains(&sample.sustain.get())
                {
                    return Err(CandidateError::InvalidAuthoring(
                        "Sample envelope needs non-negative attack/release and sustain in 0.0..=1.0"
                            .into(),
                    ));
                }
                if sample.amp.get() < 0.0 || sample.rate.get() <= 0.0 {
                    return Err(CandidateError::InvalidAuthoring(
                        "Sample amp must be non-negative and rate must be positive".into(),
                    ));
                }
                if sample.offset.get() < 0.0
                    || sample.length.is_some_and(|length| length.get() <= 0.0)
                {
                    return Err(CandidateError::InvalidAuthoring(
                        "Sample offset must be non-negative and length must be positive".into(),
                    ));
                }
                if let Some(warp) = &sample.warp {
                    if warp.speed.get() <= 0.0
                        || warp.pitch.get() <= 0.0
                        || warp.target_bpm.is_some_and(|bpm| bpm.get() <= 0.0)
                        || !(0.01..=1.0).contains(&warp.window_size.get())
                        || !(1..=32).contains(&warp.overlaps)
                    {
                        return Err(CandidateError::InvalidAuthoring(
                            "Sample warp needs positive speed/pitch/BPM, window in 0.01..=1.0, and overlaps in 1..=32"
                                .into(),
                        ));
                    }
                }
            }
            Self::Buffer(buffer) => {
                if buffer.frames == 0 {
                    return Err(CandidateError::InvalidAuthoring(
                        "Buffer frames must be at least 1".into(),
                    ));
                }
                if !(1..=16).contains(&buffer.channels) {
                    return Err(CandidateError::InvalidAuthoring(
                        "Buffer channels must be in 1..=16".into(),
                    ));
                }
            }
            Self::Sfz(sfz) => {
                validate_resource_source(&sfz.source, "SFZ source")?;
                if !sfz.source.to_ascii_lowercase().ends_with(".sfz") {
                    return Err(CandidateError::InvalidAuthoring(
                        "SFZ source must name an .sfz file".into(),
                    ));
                }
            }
            Self::Recording(recording) => {
                validate_timeline_lifecycle(recording.lifecycle)?;
                match &recording.length {
                    Some(RecordingLengthAuthoring::Beats(ticks)) if *ticks <= 0 => {
                        return Err(CandidateError::InvalidAuthoring(
                            "Recording length in beats must be positive".into(),
                        ));
                    }
                    Some(RecordingLengthAuthoring::Seconds(seconds)) if seconds.get() <= 0.0 => {
                        return Err(CandidateError::InvalidAuthoring(
                            "Recording length in seconds must be positive".into(),
                        ));
                    }
                    _ => {}
                }
                if recording.count_in_ticks < 0 {
                    return Err(CandidateError::InvalidAuthoring(
                        "Recording count-in must be non-negative".into(),
                    ));
                }
                if !(1..=2).contains(&recording.channels) {
                    return Err(CandidateError::InvalidAuthoring(
                        "Recording channels must be 1 or 2".into(),
                    ));
                }
                if let Some(destination) = &recording.destination {
                    validate_resource_source(destination, "Recording destination")?;
                }
            }
            Self::Route(route) => {
                fn validate_port(port: &str, role: &'static str) -> Result<(), CandidateError> {
                    validate_component(port, role).map_err(|_| {
                        CandidateError::InvalidAuthoring(format!(
                            "{role} must be a canonical port name, got {port:?}"
                        ))
                    })
                }
                fn validate_rate(
                    declared: RoutePortRate,
                    required: RoutePortRate,
                    verb: &str,
                ) -> Result<(), CandidateError> {
                    if declared != required {
                        return Err(CandidateError::InvalidAuthoring(format!(
                            "{verb} requires a {required:?}-rate source port, got {declared:?}"
                        )));
                    }
                    Ok(())
                }
                match route {
                    RouteAuthoring::Audio {
                        source,
                        destinations,
                    } => {
                        validate_port(&source.port, "route source port")?;
                        validate_rate(source.rate, RoutePortRate::Audio, "audio routing")?;
                        if destinations.is_empty() {
                            return Err(CandidateError::InvalidAuthoring(
                                "an audio route needs at least one destination".into(),
                            ));
                        }
                        let group_count = destinations
                            .iter()
                            .filter(|destination| {
                                matches!(destination, AudioRouteDestinationAuthoring::Group(_))
                            })
                            .count();
                        if group_count != destinations.len() && destinations.len() != 1 {
                            return Err(CandidateError::InvalidAuthoring(
                                "Main and Muted audio destinations replace — they cannot join a fan-out list"
                                    .into(),
                            ));
                        }
                        let mut seen = BTreeSet::new();
                        for destination in destinations {
                            if let AudioRouteDestinationAuthoring::Group(group) = destination {
                                if !seen.insert(group.address().erase()) {
                                    return Err(CandidateError::InvalidAuthoring(format!(
                                        "duplicate audio fan-out destination {}",
                                        group.address()
                                    )));
                                }
                            }
                        }
                    }
                    RouteAuthoring::Input {
                        input_port, source, ..
                    } => {
                        validate_port(input_port, "route input port")?;
                        if let InputRouteSourceAuthoring::VoicePort { port, .. } = source {
                            validate_port(port, "route source port")?;
                        }
                    }
                    RouteAuthoring::Set {
                        source,
                        coerce_audio,
                        target_param,
                        ..
                    } => {
                        validate_port(&source.port, "route source port")?;
                        validate_port(target_param, "route target parameter")?;
                        if *coerce_audio {
                            validate_rate(source.rate, RoutePortRate::Audio, "A2K routing")?;
                        } else {
                            validate_rate(source.rate, RoutePortRate::Control, "SET routing")?;
                        }
                    }
                    RouteAuthoring::Bend {
                        source,
                        target_param,
                        ..
                    } => {
                        validate_port(&source.port, "route source port")?;
                        validate_port(target_param, "route target parameter")?;
                        validate_rate(source.rate, RoutePortRate::Control, "BEND routing")?;
                    }
                    RouteAuthoring::Trigger {
                        source,
                        target_param,
                        ..
                    } => {
                        validate_port(&source.port, "route source port")?;
                        validate_port(target_param, "route target parameter")?;
                        validate_rate(source.rate, RoutePortRate::Trigger, "TRIGGER routing")?;
                    }
                }
            }
            Self::MidiDevice(device) => {
                let port = &device.port;
                if port.is_empty()
                    || port.len() > 255
                    || port.trim() != port
                    || port.bytes().any(|byte| byte < 0x20)
                {
                    return Err(CandidateError::InvalidAuthoring(
                        "MIDI device port must be a non-empty exact port name of at most 255 \
                         bytes without surrounding whitespace or control bytes"
                            .into(),
                    ));
                }
            }
            Self::MidiRoute(route) => match route {
                MidiRouteAuthoring::Keyboard { .. } => {}
                MidiRouteAuthoring::Cc {
                    controller,
                    target_param,
                    ..
                } => {
                    if *controller > 127 {
                        return Err(CandidateError::InvalidAuthoring(format!(
                            "MIDI controller numbers are 0..=127, got {controller}"
                        )));
                    }
                    validate_component(target_param, "MIDI CC target parameter")?;
                }
            },
            Self::Callback(callback) => {
                validate_component(&callback.handler, "MIDI callback handler")?;
                match &callback.trigger {
                    CallbackTriggerAuthoring::NoteOn {
                        note: Some(note), ..
                    } if *note > 127 => {
                        return Err(CandidateError::InvalidAuthoring(format!(
                            "MIDI note numbers are 0..=127, got {note}"
                        )));
                    }
                    CallbackTriggerAuthoring::ControlChange {
                        controller: Some(controller),
                        ..
                    } if *controller > 127 => {
                        return Err(CandidateError::InvalidAuthoring(format!(
                            "MIDI controller numbers are 0..=127, got {controller}"
                        )));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn validate_binding(
        &self,
        address: &LogicalAddress,
        owner: &DeclarationOwner,
        lifecycle: &LifecycleMetadata,
    ) -> Result<(), CandidateError> {
        if self.kind() != address.kind() {
            return Err(CandidateError::InvalidAuthoring(format!(
                "{} authoring payload cannot bind to {} address",
                self.kind(),
                address.kind()
            )));
        }
        let expected_effects = match self.desired_lifecycle() {
            None | Some(DesiredLifecycle::Dormant) => {
                if lifecycle.terminal_effect != TerminalEffect::Register {
                    return Err(CandidateError::InvalidLifecycle(
                        "a dormant authoring declaration requires a register terminal".into(),
                    ));
                }
                BTreeSet::from([LifecycleEffect::Register])
            }
            Some(DesiredLifecycle::Start(_)) => {
                if lifecycle.terminal_effect != TerminalEffect::Start {
                    return Err(CandidateError::InvalidLifecycle(
                        "a started authoring declaration requires a start terminal".into(),
                    ));
                }
                BTreeSet::from([LifecycleEffect::Register, LifecycleEffect::Start])
            }
        };
        if lifecycle.role != AuthoringRole::Builder || lifecycle.effects != expected_effects {
            return Err(CandidateError::InvalidLifecycle(
                "authoring declaration effects do not match its desired lifecycle".into(),
            ));
        }
        match owner {
            DeclarationOwner::Parent(parent)
                if parent.kind() != EntityKind::Sequence
                    || !matches!(
                        self,
                        Self::Pattern(_) | Self::Melody(_) | Self::Sequence(_) | Self::Fade(_)
                    ) =>
            {
                Err(CandidateError::InvalidAuthoring(
                    "parent-owned authoring is limited to Sequence-owned inline Pattern, Melody, Fade, and Sequence fragments"
                        .into(),
                ))
            }
            DeclarationOwner::Structural(_) if matches!(self, Self::Effect(_)) => {
                Err(CandidateError::InvalidAuthoring(
                    "Effect declarations require explicit Group contribution ownership".into(),
                ))
            }
            DeclarationOwner::Contribution(_)
                if matches!(self, Self::SynthDef(_) | Self::EffectDef(_)) =>
            {
                Err(CandidateError::InvalidDspDefinition(
                    "DSP definitions require structural module ownership".into(),
                ))
            }
            DeclarationOwner::Structural(_)
                if matches!(self, Self::SynthDef(_) | Self::EffectDef(_)) =>
            {
                let definition = self
                    .dsp_definition()
                    .expect("DSP definition variants expose definition IR");
                if definition.name() == address.to_string() {
                    Ok(())
                } else {
                    Err(CandidateError::InvalidDspDefinition(format!(
                        "definition IR name '{}' does not match module-qualified address {address}",
                        definition.name()
                    )))
                }
            }
            DeclarationOwner::Override(_) => Err(CandidateError::InvalidAuthoring(
                "authoring declarations cannot impersonate override layers".into(),
            )),
            _ => Ok(()),
        }
    }

    fn canonical_bytes(&self) -> Arc<[u8]> {
        struct Encoder(Vec<u8>);

        impl Encoder {
            fn tag(&mut self, value: u8) {
                self.0.push(value);
            }

            fn bool(&mut self, value: bool) {
                self.tag(u8::from(value));
            }

            fn u32(&mut self, value: u32) {
                self.0.extend_from_slice(&value.to_be_bytes());
            }

            fn u64(&mut self, value: u64) {
                self.0.extend_from_slice(&value.to_be_bytes());
            }

            fn i64(&mut self, value: i64) {
                self.0.extend_from_slice(&value.to_be_bytes());
            }

            fn number(&mut self, value: CanonicalF64) {
                self.u64(value.bits());
            }

            fn text(&mut self, value: &str) {
                self.u64(u64::try_from(value.len()).expect("authoring text length fits u64"));
                self.0.extend_from_slice(value.as_bytes());
            }

            fn bytes(&mut self, value: &[u8]) {
                self.u64(u64::try_from(value.len()).expect("authoring byte length fits u64"));
                self.0.extend_from_slice(value);
            }

            fn reference<K: RefKind>(&mut self, reference: &TypedRef<K>) {
                self.text(&reference.address().to_string());
            }

            fn lifecycle(&mut self, lifecycle: DesiredLifecycle) {
                self.tag(match lifecycle {
                    DesiredLifecycle::Dormant => 0,
                    DesiredLifecycle::Start(StartMode::Normal) => 1,
                    DesiredLifecycle::Start(StartMode::Immediate) => 2,
                    DesiredLifecycle::Start(StartMode::Continuous) => 3,
                });
            }

            fn params(&mut self, params: &BTreeMap<String, CanonicalF64>) {
                self.u64(u64::try_from(params.len()).expect("authoring parameter count fits u64"));
                for (name, value) in params {
                    self.text(name);
                    self.number(*value);
                }
            }
        }

        let mut encoder = Encoder(b"vibelang.authoring-declaration.v1\0".to_vec());
        match self {
            Self::Group(group) => {
                encoder.tag(0);
                if let Some(parent) = &group.parent {
                    encoder.tag(1);
                    encoder.reference(parent);
                } else {
                    encoder.tag(0);
                }
                encoder.number(group.gain);
                encoder.bool(group.muted);
                encoder.bool(group.soloed);
                encoder.params(&group.params);
                if let Some((bus, channels)) = group.output_channels {
                    encoder.tag(1);
                    encoder.u32(bus);
                    encoder.tag(channels);
                } else {
                    encoder.tag(0);
                }
            }
            Self::Voice(voice) => {
                encoder.tag(1);
                if let Some(group) = &voice.group {
                    encoder.tag(1);
                    encoder.reference(group);
                } else {
                    encoder.tag(0);
                }
                match &voice.source {
                    VoiceSourceAuthoring::SynthDef(name) => {
                        encoder.tag(0);
                        encoder.text(name);
                    }
                    VoiceSourceAuthoring::Sfz(reference) => {
                        encoder.tag(1);
                        encoder.reference(reference);
                    }
                }
                encoder.u32(voice.polyphony);
                encoder.number(voice.gain);
                encoder.bool(voice.muted);
                encoder.bool(voice.soloed);
                encoder.lifecycle(voice.lifecycle);
                encoder.params(&voice.params);
            }
            Self::Pattern(pattern) => {
                encoder.tag(2);
                encoder.reference(&pattern.voice);
                encoder.i64(pattern.length_ticks);
                encoder.number(pattern.swing);
                encoder.lifecycle(pattern.lifecycle);
                encoder.u64(
                    u64::try_from(pattern.steps.len()).expect("authoring step count fits u64"),
                );
                for step in &pattern.steps {
                    encoder.i64(step.beat_ticks);
                    encoder.number(step.velocity);
                }
                encoder.params(&pattern.params);
            }
            Self::Melody(melody) => {
                encoder.tag(3);
                encoder.reference(&melody.voice);
                encoder.i64(melody.length_ticks);
                encoder.lifecycle(melody.lifecycle);
                encoder.u64(
                    u64::try_from(melody.events.len()).expect("authoring event count fits u64"),
                );
                for event in &melody.events {
                    match event {
                        MelodyEventAuthoring::Note {
                            beat_ticks,
                            duration_ticks,
                            midi_note,
                            velocity,
                        } => {
                            encoder.tag(0);
                            encoder.i64(*beat_ticks);
                            encoder.i64(*duration_ticks);
                            encoder.tag(*midi_note);
                            encoder.number(*velocity);
                        }
                        MelodyEventAuthoring::Rest {
                            beat_ticks,
                            duration_ticks,
                        } => {
                            encoder.tag(1);
                            encoder.i64(*beat_ticks);
                            encoder.i64(*duration_ticks);
                        }
                    }
                }
            }
            Self::Sequence(sequence) => {
                encoder.tag(4);
                encoder.i64(sequence.length_ticks);
                encoder.bool(sequence.looping);
                encoder.lifecycle(sequence.lifecycle);
                encoder.u64(
                    u64::try_from(sequence.clips.len()).expect("authoring clip count fits u64"),
                );
                for clip in &sequence.clips {
                    encoder.tag(match &clip.content {
                        SequenceContentAuthoring::Pattern(_) => 0,
                        SequenceContentAuthoring::Melody(_) => 1,
                        SequenceContentAuthoring::Fade(_) => 2,
                        SequenceContentAuthoring::Sequence(_) => 3,
                    });
                    encoder.i64(clip.start_ticks);
                    encoder.i64(clip.end_ticks);
                    encoder.text(&clip.content.reference().address().to_string());
                }
            }
            Self::Fade(fade) => {
                encoder.tag(5);
                encoder.tag(match &fade.target {
                    FadeTargetAuthoring::Group(_) => 0,
                    FadeTargetAuthoring::Voice(_) => 1,
                    FadeTargetAuthoring::Effect(_) => 2,
                    FadeTargetAuthoring::Pattern(_) => 3,
                    FadeTargetAuthoring::Melody(_) => 4,
                });
                encoder.text(&fade.target.reference().address().to_string());
                encoder.text(&fade.parameter);
                encoder.number(fade.from);
                encoder.number(fade.to);
                encoder.i64(fade.duration_ticks);
                encoder.lifecycle(fade.lifecycle);
                match &fade.curve {
                    FadeCurveAuthoring::Linear => encoder.tag(0),
                    FadeCurveAuthoring::EaseIn => encoder.tag(1),
                    FadeCurveAuthoring::EaseOut => encoder.tag(2),
                    FadeCurveAuthoring::EaseInOut => encoder.tag(3),
                    FadeCurveAuthoring::SineIn => encoder.tag(4),
                    FadeCurveAuthoring::SineOut => encoder.tag(5),
                    FadeCurveAuthoring::SineInOut => encoder.tag(6),
                    FadeCurveAuthoring::CubicIn => encoder.tag(7),
                    FadeCurveAuthoring::CubicOut => encoder.tag(8),
                    FadeCurveAuthoring::CubicInOut => encoder.tag(9),
                    FadeCurveAuthoring::Exponential(exponent) => {
                        encoder.tag(10);
                        encoder.number(*exponent);
                    }
                    FadeCurveAuthoring::Logarithmic => encoder.tag(11),
                    FadeCurveAuthoring::Step => encoder.tag(12),
                    FadeCurveAuthoring::CubicSpline(points) => {
                        encoder.tag(13);
                        encoder
                            .u64(u64::try_from(points.len()).expect("Fade point count fits u64"));
                        for point in points {
                            encoder.number(point.time);
                            encoder.number(point.value);
                        }
                    }
                }
            }
            Self::Effect(effect) => {
                encoder.tag(6);
                encoder.reference(&effect.definition);
                encoder.params(&effect.params);
            }
            Self::SynthDef(definition) | Self::EffectDef(definition) => {
                encoder.tag(if matches!(self, Self::SynthDef(_)) {
                    7
                } else {
                    8
                });
                encoder.text(definition.definition.name());
                encoder.u64(definition.definition.content_hash());
                encoder.bytes(definition.definition.canonical_bytes());
                encoder.u64(
                    u64::try_from(definition.definition.outputs().len())
                        .expect("DSP output count fits u64"),
                );
                for output in definition.definition.outputs() {
                    encoder.text(&output.name);
                    encoder.tag(output.channels);
                    encoder.tag(match output.rate {
                        PortRate::Ar => 0,
                        PortRate::Kr => 1,
                        PortRate::Tr => 2,
                    });
                }
                encoder.u64(
                    u64::try_from(definition.definition.inputs().len())
                        .expect("DSP input count fits u64"),
                );
                for input in definition.definition.inputs() {
                    encoder.text(&input.name);
                    encoder.tag(input.channels);
                    encoder.tag(match input.rate {
                        PortRate::Ar => 0,
                        PortRate::Kr => 1,
                        PortRate::Tr => 2,
                    });
                }
            }
            Self::Sample(sample) => {
                encoder.tag(9);
                encoder.text(&sample.source);
                encoder.number(sample.attack);
                encoder.number(sample.sustain);
                encoder.number(sample.release);
                encoder.number(sample.amp);
                encoder.number(sample.rate);
                encoder.bool(sample.loop_mode);
                encoder.number(sample.offset);
                if let Some(length) = sample.length {
                    encoder.tag(1);
                    encoder.number(length);
                } else {
                    encoder.tag(0);
                }
                encoder.tag(match sample.trigger {
                    SampleTriggerAuthoring::Gate => 0,
                    SampleTriggerAuthoring::OneShot => 1,
                });
                if let Some(warp) = &sample.warp {
                    encoder.tag(1);
                    encoder.number(warp.speed);
                    encoder.number(warp.pitch);
                    if let Some(bpm) = warp.target_bpm {
                        encoder.tag(1);
                        encoder.number(bpm);
                    } else {
                        encoder.tag(0);
                    }
                    encoder.number(warp.window_size);
                    encoder.tag(warp.overlaps);
                } else {
                    encoder.tag(0);
                }
            }
            Self::Buffer(buffer) => {
                encoder.tag(10);
                encoder.u32(buffer.frames);
                encoder.u32(u32::from(buffer.channels));
                encoder.tag(match buffer.replacement {
                    BufferReplacementAuthoring::Clear => 0,
                    BufferReplacementAuthoring::CopyOverlap => 1,
                });
            }
            Self::Sfz(sfz) => {
                encoder.tag(11);
                encoder.text(&sfz.source);
            }
            Self::Recording(recording) => {
                encoder.tag(12);
                encoder.reference(&recording.source);
                match &recording.length {
                    None => encoder.tag(0),
                    Some(RecordingLengthAuthoring::Beats(ticks)) => {
                        encoder.tag(1);
                        encoder.i64(*ticks);
                    }
                    Some(RecordingLengthAuthoring::Seconds(seconds)) => {
                        encoder.tag(2);
                        encoder.number(*seconds);
                    }
                }
                encoder.i64(recording.count_in_ticks);
                encoder.bool(recording.metronome);
                if let Some(destination) = &recording.destination {
                    encoder.tag(1);
                    encoder.text(destination);
                } else {
                    encoder.tag(0);
                }
                encoder.tag(recording.channels);
                encoder.lifecycle(recording.lifecycle);
            }
            Self::Route(route) => {
                fn port(encoder: &mut Encoder, port: &RoutePortAuthoring) {
                    encoder.reference(&port.voice);
                    encoder.text(&port.port);
                    encoder.tag(match port.rate {
                        RoutePortRate::Audio => 0,
                        RoutePortRate::Control => 1,
                        RoutePortRate::Trigger => 2,
                    });
                }
                fn target(encoder: &mut Encoder, target: &RouteTargetAuthoring) {
                    match target {
                        RouteTargetAuthoring::Voice(reference) => {
                            encoder.tag(0);
                            encoder.reference(reference);
                        }
                        RouteTargetAuthoring::Effect(reference) => {
                            encoder.tag(1);
                            encoder.reference(reference);
                        }
                    }
                }
                fn shaping(encoder: &mut Encoder, shaping: &RouteShapingAuthoring) {
                    encoder.number(shaping.scale);
                    encoder.number(shaping.offset);
                }
                encoder.tag(13);
                match route {
                    RouteAuthoring::Audio {
                        source,
                        destinations,
                    } => {
                        encoder.tag(0);
                        port(&mut encoder, source);
                        encoder.u64(
                            u64::try_from(destinations.len())
                                .expect("route destination count fits u64"),
                        );
                        for destination in destinations {
                            match destination {
                                AudioRouteDestinationAuthoring::Group(group) => {
                                    encoder.tag(0);
                                    encoder.reference(group);
                                }
                                AudioRouteDestinationAuthoring::Main => encoder.tag(1),
                                AudioRouteDestinationAuthoring::Muted => encoder.tag(2),
                            }
                        }
                    }
                    RouteAuthoring::Input {
                        target: input_target,
                        input_port,
                        source,
                    } => {
                        encoder.tag(1);
                        encoder.reference(input_target);
                        encoder.text(input_port);
                        match source {
                            InputRouteSourceAuthoring::VoicePort { voice, port } => {
                                encoder.tag(0);
                                encoder.reference(voice);
                                encoder.text(port);
                            }
                            InputRouteSourceAuthoring::Group(group) => {
                                encoder.tag(1);
                                encoder.reference(group);
                            }
                            InputRouteSourceAuthoring::Silent => encoder.tag(2),
                        }
                    }
                    RouteAuthoring::Set {
                        source,
                        coerce_audio,
                        target: route_target,
                        target_param,
                        shaping: route_shaping,
                    } => {
                        encoder.tag(2);
                        port(&mut encoder, source);
                        encoder.bool(*coerce_audio);
                        target(&mut encoder, route_target);
                        encoder.text(target_param);
                        shaping(&mut encoder, route_shaping);
                    }
                    RouteAuthoring::Bend {
                        source,
                        target: route_target,
                        target_param,
                        shaping: route_shaping,
                    } => {
                        encoder.tag(3);
                        port(&mut encoder, source);
                        target(&mut encoder, route_target);
                        encoder.text(target_param);
                        shaping(&mut encoder, route_shaping);
                    }
                    RouteAuthoring::Trigger {
                        source,
                        target: route_target,
                        target_param,
                    } => {
                        encoder.tag(4);
                        port(&mut encoder, source);
                        target(&mut encoder, route_target);
                        encoder.text(target_param);
                    }
                }
            }
            Self::MidiDevice(device) => {
                encoder.tag(14);
                encoder.text(&device.port);
                encoder.tag(match device.direction {
                    MidiDeviceDirectionAuthoring::Input => 0,
                    MidiDeviceDirectionAuthoring::Output => 1,
                    MidiDeviceDirectionAuthoring::Bidirectional => 2,
                });
            }
            Self::MidiRoute(route) => {
                fn channel(encoder: &mut Encoder, channel: Option<MidiChannel>) {
                    match channel {
                        None => encoder.tag(0),
                        Some(channel) => {
                            encoder.tag(1);
                            encoder.tag(channel.index());
                        }
                    }
                }
                fn target(encoder: &mut Encoder, target: &RouteTargetAuthoring) {
                    match target {
                        RouteTargetAuthoring::Voice(reference) => {
                            encoder.tag(0);
                            encoder.reference(reference);
                        }
                        RouteTargetAuthoring::Effect(reference) => {
                            encoder.tag(1);
                            encoder.reference(reference);
                        }
                    }
                }
                encoder.tag(15);
                match route {
                    MidiRouteAuthoring::Keyboard {
                        device,
                        channel: route_channel,
                        voice,
                    } => {
                        encoder.tag(0);
                        encoder.reference(device);
                        channel(&mut encoder, *route_channel);
                        encoder.reference(voice);
                    }
                    MidiRouteAuthoring::Cc {
                        device,
                        channel: route_channel,
                        controller,
                        target: route_target,
                        target_param,
                        min,
                        max,
                    } => {
                        encoder.tag(1);
                        encoder.reference(device);
                        channel(&mut encoder, *route_channel);
                        encoder.tag(*controller);
                        target(&mut encoder, route_target);
                        encoder.text(target_param);
                        encoder.number(*min);
                        encoder.number(*max);
                    }
                }
            }
            Self::Callback(callback) => {
                fn channel(encoder: &mut Encoder, channel: Option<MidiChannel>) {
                    match channel {
                        None => encoder.tag(0),
                        Some(channel) => {
                            encoder.tag(1);
                            encoder.tag(channel.index());
                        }
                    }
                }
                fn optional_data(encoder: &mut Encoder, data: Option<u8>) {
                    match data {
                        None => encoder.tag(0),
                        Some(data) => {
                            encoder.tag(1);
                            encoder.tag(data);
                        }
                    }
                }
                encoder.tag(16);
                encoder.reference(&callback.device);
                match &callback.trigger {
                    CallbackTriggerAuthoring::NoteOn {
                        channel: trigger_channel,
                        note,
                    } => {
                        encoder.tag(0);
                        channel(&mut encoder, *trigger_channel);
                        optional_data(&mut encoder, *note);
                    }
                    CallbackTriggerAuthoring::ControlChange {
                        channel: trigger_channel,
                        controller,
                    } => {
                        encoder.tag(1);
                        channel(&mut encoder, *trigger_channel);
                        optional_data(&mut encoder, *controller);
                    }
                    CallbackTriggerAuthoring::ClockSync => encoder.tag(2),
                    CallbackTriggerAuthoring::AnyMessage => encoder.tag(3),
                }
                encoder.text(&callback.handler);
            }
        }
        Arc::from(encoder.0)
    }

    fn references(&self) -> Vec<ErasedRef> {
        match self {
            Self::Group(group) => group
                .parent
                .as_ref()
                .map(TypedRef::erase)
                .into_iter()
                .collect(),
            Self::Voice(voice) => {
                let mut references = Vec::new();
                if let Some(group) = &voice.group {
                    references.push(group.erase());
                }
                if let VoiceSourceAuthoring::Sfz(sfz) = &voice.source {
                    references.push(sfz.erase());
                }
                references
            }
            Self::Pattern(pattern) => vec![pattern.voice.erase()],
            Self::Melody(melody) => vec![melody.voice.erase()],
            Self::Sequence(sequence) => sequence
                .clips
                .iter()
                .map(|clip| clip.content.reference())
                .collect(),
            Self::Fade(fade) => vec![fade.target.reference()],
            Self::Effect(effect) => vec![effect.definition.erase()],
            Self::SynthDef(_) | Self::EffectDef(_) => Vec::new(),
            Self::Sample(_) | Self::Buffer(_) | Self::Sfz(_) => Vec::new(),
            Self::Recording(recording) => vec![recording.source.erase()],
            Self::Route(route) => match route {
                RouteAuthoring::Audio {
                    source,
                    destinations,
                } => {
                    let mut references = vec![source.voice.erase()];
                    for destination in destinations {
                        if let AudioRouteDestinationAuthoring::Group(group) = destination {
                            references.push(group.erase());
                        }
                    }
                    references
                }
                RouteAuthoring::Input { target, source, .. } => {
                    let mut references = vec![target.erase()];
                    match source {
                        InputRouteSourceAuthoring::VoicePort { voice, .. } => {
                            references.push(voice.erase());
                        }
                        InputRouteSourceAuthoring::Group(group) => references.push(group.erase()),
                        InputRouteSourceAuthoring::Silent => {}
                    }
                    references
                }
                RouteAuthoring::Set { source, target, .. }
                | RouteAuthoring::Bend { source, target, .. }
                | RouteAuthoring::Trigger { source, target, .. } => {
                    vec![source.voice.erase(), target.reference()]
                }
            },
            Self::MidiDevice(_) => Vec::new(),
            Self::MidiRoute(route) => match route {
                MidiRouteAuthoring::Keyboard { device, voice, .. } => {
                    vec![device.erase(), voice.erase()]
                }
                MidiRouteAuthoring::Cc { device, target, .. } => {
                    vec![device.erase(), target.reference()]
                }
            },
            Self::Callback(callback) => vec![callback.device.erase()],
        }
    }

    fn dsp_definition(&self) -> Option<&DspDefinitionIr> {
        match self {
            Self::SynthDef(definition) | Self::EffectDef(definition) => {
                Some(&definition.definition)
            }
            _ => None,
        }
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
    #[must_use]
    pub fn composition(&self) -> Composition {
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
    Authoring {
        declaration: AuthoringDeclaration,
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

    pub fn authoring(declaration: AuthoringDeclaration) -> Result<Self, CandidateError> {
        declaration.validate()?;
        let canonical_bytes = declaration.canonical_bytes();
        Ok(Self::Authoring {
            declaration,
            canonical_bytes,
        })
    }

    pub fn authoring_checked(
        declaration: AuthoringDeclaration,
        canonical_bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self, CandidateError> {
        declaration.validate()?;
        let canonical_bytes = canonical_bytes.into();
        if canonical_bytes.as_ref() != declaration.canonical_bytes().as_ref() {
            return Err(CandidateError::InvalidAuthoring(
                "authoring payload bytes do not match the canonical tagged declaration".into(),
            ));
        }
        Ok(Self::Authoring {
            declaration,
            canonical_bytes,
        })
    }

    fn references(&self) -> Vec<ErasedRef> {
        match self {
            Self::Authoring { declaration, .. } => declaration.references(),
            Self::Empty | Self::Opaque { .. } => Vec::new(),
        }
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
    pub fn new(reference: ErasedRef, source: SourceAnchor) -> Self {
        Self { reference, source }
    }

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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LifecycleOperationId {
    target: LogicalAddress,
    syntax_key: SyntaxKey,
}

impl LifecycleOperationId {
    #[must_use]
    pub fn new(target: LogicalAddress, syntax_key: SyntaxKey) -> Self {
        Self { target, syntax_key }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleAction {
    Start(StartMode),
    Restart,
    Stop,
    Remove,
    Cancel,
    SetMuted(bool),
    SetSoloed(bool),
    RemoveContribution(ContributionId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleOperationIr {
    id: LifecycleOperationId,
    target: ErasedRef,
    lifecycle: LifecycleMetadata,
    action: LifecycleAction,
    source: SourceAnchor,
}

impl LifecycleOperationIr {
    pub fn new(
        target: ErasedRef,
        lifecycle: LifecycleMetadata,
        action: LifecycleAction,
        source: SourceAnchor,
    ) -> Result<Self, CandidateError> {
        lifecycle.validate()?;
        let expected = match &action {
            LifecycleAction::Start(_) | LifecycleAction::Restart => TerminalEffect::Start,
            LifecycleAction::Stop => TerminalEffect::Stop,
            LifecycleAction::Remove | LifecycleAction::Cancel => TerminalEffect::Cancel,
            LifecycleAction::SetMuted(_)
            | LifecycleAction::SetSoloed(_)
            | LifecycleAction::RemoveContribution(_) => TerminalEffect::Register,
        };
        if lifecycle.terminal_effect != expected {
            return Err(CandidateError::InvalidLifecycle(format!(
                "action {action:?} requires terminal effect {expected:?}"
            )));
        }
        let kind = target.address().kind();
        let supported = match &action {
            LifecycleAction::Start(StartMode::Continuous) => kind == EntityKind::Voice,
            LifecycleAction::Start(StartMode::Normal | StartMode::Immediate)
            | LifecycleAction::Cancel => matches!(
                kind,
                EntityKind::Pattern
                    | EntityKind::Melody
                    | EntityKind::Sequence
                    | EntityKind::Fade
                    | EntityKind::Recording
            ),
            LifecycleAction::Restart => kind == EntityKind::Fade,
            LifecycleAction::Stop => matches!(
                kind,
                EntityKind::Voice
                    | EntityKind::Pattern
                    | EntityKind::Melody
                    | EntityKind::Sequence
                    | EntityKind::Fade
                    | EntityKind::Recording
            ),
            LifecycleAction::Remove => true,
            LifecycleAction::SetMuted(_) | LifecycleAction::SetSoloed(_) => {
                matches!(kind, EntityKind::Group | EntityKind::Voice)
            }
            LifecycleAction::RemoveContribution(_) => kind == EntityKind::Group,
        };
        if !supported {
            return Err(CandidateError::InvalidLifecycle(format!(
                "action {action:?} is unsupported for {kind}"
            )));
        }
        let id = LifecycleOperationId::new(target.address().clone(), source.syntax_key().clone());
        Ok(Self {
            id,
            target,
            lifecycle,
            action,
            source,
        })
    }

    #[must_use]
    pub fn id(&self) -> &LifecycleOperationId {
        &self.id
    }

    #[must_use]
    pub fn target(&self) -> &ErasedRef {
        &self.target
    }

    #[must_use]
    pub fn lifecycle(&self) -> &LifecycleMetadata {
        &self.lifecycle
    }

    #[must_use]
    pub fn action(&self) -> &LifecycleAction {
        &self.action
    }

    #[must_use]
    pub fn source(&self) -> &SourceAnchor {
        &self.source
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CandidateFragment {
    declarations: Vec<DeclarationIr>,
    references: Vec<ReferenceUse>,
    contributions: Vec<ContributionIr>,
    overrides: Vec<OverrideIr>,
    operations: Vec<LifecycleOperationIr>,
}

impl CandidateFragment {
    #[must_use]
    pub fn declaration(mut self, declaration: DeclarationIr) -> Self {
        self.declarations.push(declaration);
        self
    }

    #[must_use]
    pub fn reference(mut self, reference: ReferenceUse) -> Self {
        self.references.push(reference);
        self
    }

    #[must_use]
    pub fn contribution(mut self, contribution: ContributionIr) -> Self {
        self.contributions.push(contribution);
        self
    }

    #[must_use]
    pub fn override_ir(mut self, override_ir: OverrideIr) -> Self {
        self.overrides.push(override_ir);
        self
    }

    #[must_use]
    pub fn operation(mut self, operation: LifecycleOperationIr) -> Self {
        self.operations.push(operation);
        self
    }

    pub fn extend(&mut self, mut other: Self) {
        self.declarations.append(&mut other.declarations);
        self.references.append(&mut other.references);
        self.contributions.append(&mut other.contributions);
        self.overrides.append(&mut other.overrides);
        self.operations.append(&mut other.operations);
    }

    #[must_use]
    pub fn declarations(&self) -> &[DeclarationIr] {
        &self.declarations
    }

    #[must_use]
    pub fn operations(&self) -> &[LifecycleOperationIr] {
        &self.operations
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
    operations: Vec<LifecycleOperationIr>,
    dsp_definitions: StagedDspRegistry,
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

    #[must_use]
    pub fn operations(&self) -> &[LifecycleOperationIr] {
        &self.0.operations
    }

    #[must_use]
    pub fn dsp_definitions(&self) -> &StagedDspRegistry {
        &self.0.dsp_definitions
    }

    /// Complete fan-in/fan-out/conflict metadata over this candidate's
    /// route declarations.
    #[must_use]
    pub fn route_topology(&self) -> RouteTopology {
        RouteTopology::from_declarations(&self.0.declarations)
    }
}

/// A `(source voice, port)` endpoint in route topology metadata.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RouteEndpoint {
    pub voice: LogicalAddress,
    pub port: String,
}

/// Summary of one audio route declaration: the ordered fan-out list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioRouteSummary {
    pub route: LogicalAddress,
    pub source: RouteEndpoint,
    /// Group destinations in declaration order; `None` entries are the
    /// replacing Main/Muted destinations.
    pub group_destinations: Vec<LogicalAddress>,
    pub to_main: bool,
    pub muted: bool,
}

impl AudioRouteSummary {
    #[must_use]
    pub fn fan_out(&self) -> usize {
        self.group_destinations.len() + usize::from(self.to_main)
    }
}

/// One modulation edge feeding a `(target, param)` registry slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParamRouteEdge {
    pub route: LogicalAddress,
    pub source: RouteEndpoint,
    pub coerced_from_audio: bool,
    pub scale: Option<CanonicalF64>,
    pub offset: Option<CanonicalF64>,
}

/// The shared-registry slot for one `(target, param)` pair: exactly one verb
/// with its complete fan-in edge list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParamRouteSlot {
    pub verb: RouteVerb,
    pub edges: Vec<ParamRouteEdge>,
}

impl ParamRouteSlot {
    #[must_use]
    pub fn fan_in(&self) -> usize {
        self.edges.len()
    }
}

/// Summary of one named-input wiring; `source: None` is an explicit
/// disconnect onto the shared silent bus.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputRouteSummary {
    pub route: LogicalAddress,
    pub source: Option<InputRouteSourceAuthoring>,
}

/// Complete route metadata of a candidate: audio fan-out per source port,
/// param fan-in per registry slot, and input wiring per target port.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouteTopology {
    pub audio: BTreeMap<RouteEndpoint, AudioRouteSummary>,
    pub params: BTreeMap<(LogicalAddress, String), ParamRouteSlot>,
    pub inputs: BTreeMap<(LogicalAddress, String), InputRouteSummary>,
}

impl RouteTopology {
    fn from_declarations(declarations: &[DeclarationIr]) -> Self {
        let mut topology = Self::default();
        for declaration in declarations {
            let DeclarationPayload::Authoring {
                declaration: AuthoringDeclaration::Route(route),
                ..
            } = declaration.payload()
            else {
                continue;
            };
            let address = declaration.address().clone();
            match route {
                RouteAuthoring::Audio {
                    source,
                    destinations,
                } => {
                    let endpoint = RouteEndpoint {
                        voice: source.voice.address().erase(),
                        port: source.port.clone(),
                    };
                    let mut summary = AudioRouteSummary {
                        route: address,
                        source: endpoint.clone(),
                        group_destinations: Vec::new(),
                        to_main: false,
                        muted: false,
                    };
                    for destination in destinations {
                        match destination {
                            AudioRouteDestinationAuthoring::Group(group) => {
                                summary.group_destinations.push(group.address().erase());
                            }
                            AudioRouteDestinationAuthoring::Main => summary.to_main = true,
                            AudioRouteDestinationAuthoring::Muted => summary.muted = true,
                        }
                    }
                    topology.audio.insert(endpoint, summary);
                }
                RouteAuthoring::Input {
                    target,
                    input_port,
                    source,
                } => {
                    let key = (target.address().erase(), input_port.clone());
                    topology.inputs.insert(
                        key,
                        InputRouteSummary {
                            route: address,
                            source: match source {
                                InputRouteSourceAuthoring::Silent => None,
                                other => Some(other.clone()),
                            },
                        },
                    );
                }
                RouteAuthoring::Set {
                    source,
                    coerce_audio,
                    target,
                    target_param,
                    shaping,
                } => topology.record_param_edge(
                    RouteVerb::Set,
                    target,
                    target_param,
                    ParamRouteEdge {
                        route: address,
                        source: RouteEndpoint {
                            voice: source.voice.address().erase(),
                            port: source.port.clone(),
                        },
                        coerced_from_audio: *coerce_audio,
                        scale: Some(shaping.scale),
                        offset: Some(shaping.offset),
                    },
                ),
                RouteAuthoring::Bend {
                    source,
                    target,
                    target_param,
                    shaping,
                } => topology.record_param_edge(
                    RouteVerb::Bend,
                    target,
                    target_param,
                    ParamRouteEdge {
                        route: address,
                        source: RouteEndpoint {
                            voice: source.voice.address().erase(),
                            port: source.port.clone(),
                        },
                        coerced_from_audio: false,
                        scale: Some(shaping.scale),
                        offset: Some(shaping.offset),
                    },
                ),
                RouteAuthoring::Trigger {
                    source,
                    target,
                    target_param,
                } => topology.record_param_edge(
                    RouteVerb::Trigger,
                    target,
                    target_param,
                    ParamRouteEdge {
                        route: address,
                        source: RouteEndpoint {
                            voice: source.voice.address().erase(),
                            port: source.port.clone(),
                        },
                        coerced_from_audio: false,
                        scale: None,
                        offset: None,
                    },
                ),
            }
        }
        topology
    }

    fn record_param_edge(
        &mut self,
        verb: RouteVerb,
        target: &RouteTargetAuthoring,
        target_param: &str,
        edge: ParamRouteEdge,
    ) {
        self.params
            .entry((target.address(), target_param.to_string()))
            .or_insert_with(|| ParamRouteSlot {
                verb,
                edges: Vec::new(),
            })
            .edges
            .push(edge);
    }

    /// Number of distinct `(target, param)` slots fed by the given source
    /// port — the SET/BEND fan-out metric.
    #[must_use]
    pub fn param_fan_out(&self, source: &RouteEndpoint) -> usize {
        self.params
            .values()
            .filter(|slot| slot.edges.iter().any(|edge| &edge.source == source))
            .count()
    }
}

#[derive(Clone, Debug)]
pub struct CandidateDraft {
    identity: EvaluationIdentity,
    origin: CandidateOrigin,
    declarations: BTreeMap<LogicalAddress, DeclarationIr>,
    references: BTreeMap<LogicalAddress, ReferenceUse>,
    contributions: BTreeMap<ContributionId, ContributionIr>,
    overrides: BTreeMap<OverrideId, OverrideIr>,
    operations: BTreeMap<LifecycleOperationId, LifecycleOperationIr>,
    dsp_definitions: StagedDspRegistry,
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
            operations: BTreeMap::new(),
            dsp_definitions: StagedDspRegistry::default(),
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
        if let DeclarationPayload::Authoring {
            declaration: authoring,
            ..
        } = &declaration.payload
        {
            authoring.validate_binding(
                &declaration.address,
                &declaration.owner,
                &declaration.lifecycle,
            )?;
        }
        if let Some(first) = self.declarations.get(&declaration.address) {
            return Err(CandidateError::DuplicateDeclaration {
                address: Box::new(declaration.address.clone()),
                first: Box::new(first.source.clone()),
                duplicate: Box::new(declaration.source.clone()),
            });
        }
        if let DeclarationOwner::Contribution(id) = &declaration.owner {
            if !self.contributions.contains_key(id) {
                return Err(CandidateError::UnknownContribution(id.clone()));
            }
        }
        if let DeclarationPayload::Authoring {
            declaration: authoring,
            ..
        } = &declaration.payload
        {
            if let Some(definition) = authoring.dsp_definition() {
                self.dsp_definitions
                    .stage(definition.clone())
                    .map_err(|error| CandidateError::InvalidDspDefinition(error.to_string()))?;
            }
        }
        if let DeclarationOwner::Contribution(id) = &declaration.owner {
            let contribution = self
                .contributions
                .get_mut(id)
                .expect("contribution existence was validated before DSP staging");
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

    pub fn add_operation(&mut self, operation: LifecycleOperationIr) -> Result<(), CandidateError> {
        operation
            .target
            .identity
            .ensure_compatible(&self.identity)?;
        if self.operations.contains_key(operation.id()) {
            return Err(CandidateError::DuplicateLifecycleOperation(
                operation.id().clone(),
            ));
        }
        self.operations.insert(operation.id().clone(), operation);
        Ok(())
    }

    pub fn append_fragment(&mut self, fragment: CandidateFragment) -> Result<(), CandidateError> {
        let mut staged = self.clone();
        for contribution in fragment.contributions {
            staged.add_contribution(contribution)?;
        }
        for declaration in fragment.declarations {
            staged.declare_erased(declaration)?;
        }
        for reference in fragment.references {
            reference
                .reference
                .identity
                .ensure_compatible(&staged.identity)?;
            staged
                .references
                .entry(reference.reference.address().clone())
                .or_insert(reference);
        }
        for override_ir in fragment.overrides {
            staged.add_override(override_ir)?;
        }
        for operation in fragment.operations {
            staged.add_operation(operation)?;
        }
        *self = staged;
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
            for reference in declaration.payload.references() {
                if !target_exists(reference.address()) {
                    return Err(CandidateError::UnresolvedReference(
                        reference.address().clone(),
                    ));
                }
                reference.identity().ensure_compatible(&self.identity)?;
            }
        }
        for operation in self.operations.values() {
            if !target_exists(operation.target.address()) {
                return Err(CandidateError::UnresolvedReference(
                    operation.target.address().clone(),
                ));
            }
        }

        validate_parent_ownership(&self.declarations)?;
        validate_sequence_dependencies(&self.declarations)?;
        validate_recording_runs(&self.declarations, &self.operations)?;
        validate_route_registry(&self.declarations)?;

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
            operations: self.operations.into_values().collect(),
            dsp_definitions: self.dsp_definitions,
        })))
    }
}

fn validate_parent_ownership(
    declarations: &BTreeMap<LogicalAddress, DeclarationIr>,
) -> Result<(), CandidateError> {
    for (child_address, child) in declarations {
        let DeclarationOwner::Parent(parent_address) = child.owner() else {
            continue;
        };
        if child_address == parent_address {
            return Err(CandidateError::InvalidAuthoring(
                "a parent-owned declaration cannot own itself".into(),
            ));
        }
        let parent = declarations.get(parent_address).ok_or_else(|| {
            CandidateError::InvalidAuthoring(format!(
                "parent-owned declaration {child_address} requires its Sequence parent {parent_address} in the same Candidate"
            ))
        })?;
        let DeclarationPayload::Authoring {
            declaration: AuthoringDeclaration::Sequence(sequence),
            ..
        } = parent.payload()
        else {
            return Err(CandidateError::InvalidAuthoring(format!(
                "parent-owned declaration {child_address} requires an authoring Sequence parent"
            )));
        };
        if !sequence
            .clips
            .iter()
            .any(|clip| clip.content.reference().address() == child_address)
        {
            return Err(CandidateError::InvalidAuthoring(format!(
                "parent-owned declaration {child_address} is not a direct clip of {parent_address}"
            )));
        }
    }
    Ok(())
}

fn validate_recording_runs(
    declarations: &BTreeMap<LogicalAddress, DeclarationIr>,
    operations: &BTreeMap<LifecycleOperationId, LifecycleOperationIr>,
) -> Result<(), CandidateError> {
    let mut start_requests = BTreeMap::<LogicalAddress, usize>::new();
    for (address, declaration) in declarations {
        if let DeclarationPayload::Authoring {
            declaration: AuthoringDeclaration::Recording(recording),
            ..
        } = declaration.payload()
        {
            if matches!(recording.lifecycle, DesiredLifecycle::Start(_)) {
                *start_requests.entry(address.clone()).or_default() += 1;
            }
        }
    }
    for operation in operations.values() {
        if operation.target().address().kind() == EntityKind::Recording
            && matches!(
                operation.action(),
                LifecycleAction::Start(_) | LifecycleAction::Restart
            )
        {
            *start_requests
                .entry(operation.target().address().clone())
                .or_default() += 1;
        }
    }
    for (address, count) in start_requests {
        if count > 1 {
            return Err(CandidateError::InvalidAuthoring(format!(
                "Recording {address} admits one active run; this candidate requests {count} starts"
            )));
        }
    }
    Ok(())
}

fn validate_route_registry(
    declarations: &BTreeMap<LogicalAddress, DeclarationIr>,
) -> Result<(), CandidateError> {
    let mut slots =
        BTreeMap::<(LogicalAddress, String), (RouteVerb, LogicalAddress, SourceAnchor)>::new();
    for (address, declaration) in declarations {
        let DeclarationPayload::Authoring {
            declaration: AuthoringDeclaration::Route(route),
            ..
        } = declaration.payload()
        else {
            continue;
        };
        let Some((verb, target, target_param)) = route.registry_slot() else {
            continue;
        };
        let key = (target, target_param.to_string());
        if let Some((existing_verb, _, existing_anchor)) = slots.get(&key) {
            if *existing_verb != verb {
                return Err(CandidateError::RouteConflict {
                    target: Box::new(key.0),
                    target_param: key.1,
                    existing_verb: *existing_verb,
                    existing: Box::new(existing_anchor.clone()),
                    conflicting_verb: verb,
                    conflicting: Box::new(declaration.source().clone()),
                });
            }
        } else {
            slots.insert(key, (verb, address.clone(), declaration.source().clone()));
        }
    }
    Ok(())
}

fn validate_sequence_dependencies(
    declarations: &BTreeMap<LogicalAddress, DeclarationIr>,
) -> Result<(), CandidateError> {
    let mut dependencies = BTreeMap::<LogicalAddress, Vec<LogicalAddress>>::new();
    for (address, declaration) in declarations {
        let DeclarationPayload::Authoring {
            declaration: AuthoringDeclaration::Sequence(sequence),
            ..
        } = declaration.payload()
        else {
            continue;
        };
        dependencies.insert(
            address.clone(),
            sequence
                .clips
                .iter()
                .filter_map(|clip| match &clip.content {
                    SequenceContentAuthoring::Sequence(reference) => {
                        Some(reference.address().erase())
                    }
                    _ => None,
                })
                .collect(),
        );
    }

    fn visit(
        node: &LogicalAddress,
        dependencies: &BTreeMap<LogicalAddress, Vec<LogicalAddress>>,
        visiting: &mut Vec<LogicalAddress>,
        visited: &mut BTreeSet<LogicalAddress>,
    ) -> Result<(), CandidateError> {
        if let Some(index) = visiting.iter().position(|entry| entry == node) {
            let mut cycle = visiting[index..].to_vec();
            cycle.push(node.clone());
            return Err(CandidateError::DependencyCycle(cycle));
        }
        if !visited.insert(node.clone()) {
            return Ok(());
        }
        visiting.push(node.clone());
        if let Some(children) = dependencies.get(node) {
            for child in children {
                if dependencies.contains_key(child) {
                    visit(child, dependencies, visiting, visited)?;
                }
            }
        }
        visiting.pop();
        Ok(())
    }

    let mut visited = BTreeSet::new();
    for address in dependencies.keys() {
        visit(address, &dependencies, &mut Vec::new(), &mut visited)?;
    }
    Ok(())
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
    #[error("invalid authoring declaration: {0}")]
    InvalidAuthoring(String),
    #[error("invalid detached DSP definition: {0}")]
    InvalidDspDefinition(String),
    #[error("duplicate lifecycle operation: {0:?}")]
    DuplicateLifecycleOperation(LifecycleOperationId),
    #[error(
        "conflicting route verbs: {existing_verb} and {conflicting_verb} both claim parameter \
         {target_param} on {target}; SET, BEND, and TRIGGER stay mutually exclusive per target"
    )]
    RouteConflict {
        target: Box<LogicalAddress>,
        target_param: String,
        existing_verb: RouteVerb,
        existing: Box<SourceAnchor>,
        conflicting_verb: RouteVerb,
        conflicting: Box<SourceAnchor>,
    },
    #[error("authoring dependency cycle: {0:?}")]
    DependencyCycle(Vec<LogicalAddress>),
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

    #[test]
    fn canonical_authoring_numbers_normalize_signed_zero_and_reject_non_finite_values() {
        assert_eq!(
            CanonicalF64::new(-0.0).unwrap(),
            CanonicalF64::new(0.0).unwrap()
        );
        assert!(matches!(
            CanonicalF64::new(f64::NAN),
            Err(CandidateError::InvalidAuthoring(_))
        ));
        assert!(matches!(
            CanonicalF64::new(f64::INFINITY),
            Err(CandidateError::InvalidAuthoring(_))
        ));
    }

    #[test]
    fn canonical_authoring_payload_is_core_owned_and_rejects_forged_bytes() {
        let declaration = AuthoringDeclaration::Group(GroupAuthoring {
            parent: None,
            gain: CanonicalF64::new(-0.0).unwrap(),
            muted: false,
            soloed: false,
            params: BTreeMap::new(),
            output_channels: None,
        });
        let canonical = DeclarationPayload::authoring(declaration.clone()).unwrap();
        let DeclarationPayload::Authoring {
            canonical_bytes, ..
        } = canonical
        else {
            panic!("authoring payload")
        };

        assert!(canonical_bytes.starts_with(b"vibelang.authoring-declaration.v1\0"));
        assert!(matches!(
            DeclarationPayload::authoring_checked(declaration, Arc::<[u8]>::from([0_u8])),
            Err(CandidateError::InvalidAuthoring(_))
        ));
    }

    #[test]
    fn authoring_kind_lifecycle_and_parent_ownership_reject_before_draft_mutation() {
        let identity = identity(1);
        let voice = TypedRef::new(identity.clone(), address::<VoiceKind>("lead"));
        let pattern = AuthoringDeclaration::Pattern(PatternAuthoring {
            voice,
            steps: vec![PatternStepAuthoring {
                beat_ticks: 0,
                velocity: CanonicalF64::new(1.0).unwrap(),
            }],
            length_ticks: 65_536,
            swing: CanonicalF64::new(0.0).unwrap(),
            params: BTreeMap::new(),
            lifecycle: DesiredLifecycle::Dormant,
        });
        let payload = DeclarationPayload::authoring(pattern.clone()).unwrap();
        let mut draft = CandidateDraft::new(identity, CandidateOrigin::RhaiHost);
        let wrong_kind = DeclarationIr::new(
            address::<MelodyKind>("beat"),
            DeclarationOwner::Structural(source(40).syntax_key().clone()),
            source(40),
            LifecycleMetadata::register(Composition::Standalone),
            payload,
        );
        assert!(matches!(
            draft.declare::<MelodyKind>(wrong_kind),
            Err(CandidateError::InvalidAuthoring(_))
        ));

        let wrong_lifecycle = DeclarationIr::new(
            address::<PatternKind>("beat"),
            DeclarationOwner::Structural(source(41).syntax_key().clone()),
            source(41),
            LifecycleMetadata::start(Composition::Standalone),
            DeclarationPayload::authoring(pattern.clone()).unwrap(),
        );
        assert!(matches!(
            draft.declare::<PatternKind>(wrong_lifecycle),
            Err(CandidateError::InvalidLifecycle(_))
        ));

        let unsupported_parent = DeclarationIr::new(
            address::<PatternKind>("beat"),
            DeclarationOwner::Parent(address::<GroupKind>("band").erase()),
            source(42),
            LifecycleMetadata::register(Composition::ParentOwned),
            DeclarationPayload::authoring(pattern).unwrap(),
        );
        assert!(matches!(
            draft.declare::<PatternKind>(unsupported_parent),
            Err(CandidateError::InvalidAuthoring(_))
        ));
        assert_eq!(draft.declaration_count(), 0);
    }

    #[test]
    fn fragment_append_is_atomic_when_a_late_declaration_rejects() {
        let identity = identity(1);
        let mut draft = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        draft
            .declare::<VoiceKind>(declaration::<VoiceKind>("lead"))
            .unwrap();
        let contribution_id = ContributionId::new(
            module(),
            SyntaxKey::deterministic(&module(), &[14], "body").unwrap(),
        );
        let target = TypedRef::new(identity, address::<GroupKind>("band"));
        let fragment = CandidateFragment::default()
            .contribution(ContributionIr::new(
                contribution_id,
                target,
                Some(4),
                source(14),
            ))
            .declaration(declaration::<VoiceKind>("lead"));

        assert!(matches!(
            draft.append_fragment(fragment),
            Err(CandidateError::DuplicateDeclaration { .. })
        ));
        assert_eq!(draft.declaration_count(), 1);
        assert!(draft.contributions.is_empty());
    }

    #[test]
    fn sequence_reference_cycles_reject_the_whole_candidate() {
        let identity = identity(1);
        let a_ref = TypedRef::new(identity.clone(), address::<SequenceKind>("a"));
        let b_ref = TypedRef::new(identity.clone(), address::<SequenceKind>("b"));
        let make_sequence = |key: &str, dependency: TypedRef<SequenceKind>, index: u32| {
            DeclarationIr::new(
                address::<SequenceKind>(key),
                DeclarationOwner::Structural(
                    SyntaxKey::deterministic(&module(), &[index], "owner").unwrap(),
                ),
                source(index),
                LifecycleMetadata::register(Composition::Standalone),
                DeclarationPayload::authoring(AuthoringDeclaration::Sequence(SequenceAuthoring {
                    clips: vec![SequenceClipAuthoring {
                        start_ticks: 0,
                        end_ticks: 1,
                        content: SequenceContentAuthoring::Sequence(dependency),
                    }],
                    length_ticks: 4,
                    looping: true,
                    lifecycle: DesiredLifecycle::Dormant,
                }))
                .unwrap(),
            )
        };
        let mut draft = CandidateDraft::new(identity, CandidateOrigin::RhaiHost);
        draft
            .declare::<SequenceKind>(make_sequence("a", b_ref, 21))
            .unwrap();
        draft
            .declare::<SequenceKind>(make_sequence("b", a_ref, 22))
            .unwrap();

        assert!(matches!(
            draft.finish(&ReferenceCatalog::default()),
            Err(CandidateError::DependencyCycle(cycle)) if cycle.len() == 3
        ));
    }

    #[test]
    fn parent_owned_declarations_must_be_direct_clips_of_their_sequence_parent() {
        let identity = identity(1);
        let parent_address = address::<SequenceKind>("parent");
        let child_address = address::<SequenceKind>("child");
        let sequence = |clips| {
            DeclarationPayload::authoring(AuthoringDeclaration::Sequence(SequenceAuthoring {
                clips,
                length_ticks: 4,
                looping: true,
                lifecycle: DesiredLifecycle::Dormant,
            }))
            .unwrap()
        };
        let mut draft = CandidateDraft::new(identity, CandidateOrigin::RhaiHost);
        draft
            .declare::<SequenceKind>(DeclarationIr::new(
                parent_address.clone(),
                DeclarationOwner::Structural(source(23).syntax_key().clone()),
                source(23),
                LifecycleMetadata::register(Composition::Standalone),
                sequence(Vec::new()),
            ))
            .unwrap();
        draft
            .declare::<SequenceKind>(DeclarationIr::new(
                child_address,
                DeclarationOwner::Parent(parent_address.erase()),
                source(24),
                LifecycleMetadata::register(Composition::ParentOwned),
                sequence(Vec::new()),
            ))
            .unwrap();

        assert!(matches!(
            draft.finish(&ReferenceCatalog::default()),
            Err(CandidateError::InvalidAuthoring(message))
                if message.contains("not a direct clip")
        ));
    }

    #[test]
    fn lifecycle_actions_reject_a_mismatched_effect_category_before_insertion() {
        let evaluation = identity(1);
        let target = TypedRef::new(evaluation.clone(), address::<PatternKind>("beat")).erase();
        assert!(matches!(
            LifecycleOperationIr::new(
                target,
                LifecycleMetadata::reference(
                    TerminalEffect::Register,
                    Cancellation::NotCancellable,
                ),
                LifecycleAction::Start(StartMode::Normal),
                source(30),
            ),
            Err(CandidateError::InvalidLifecycle(_))
        ));
        let group = TypedRef::new(evaluation, address::<GroupKind>("band")).erase();
        assert!(matches!(
            LifecycleOperationIr::new(
                group,
                LifecycleMetadata::reference(TerminalEffect::Start, Cancellation::BeforePlanning,),
                LifecycleAction::Start(StartMode::Normal),
                source(31),
            ),
            Err(CandidateError::InvalidLifecycle(_))
        ));
    }

    fn sample_authoring(source_path: &str) -> SampleAuthoring {
        SampleAuthoring {
            source: source_path.into(),
            attack: CanonicalF64::new(0.001).unwrap(),
            sustain: CanonicalF64::new(1.0).unwrap(),
            release: CanonicalF64::new(0.01).unwrap(),
            amp: CanonicalF64::new(1.0).unwrap(),
            rate: CanonicalF64::new(1.0).unwrap(),
            loop_mode: false,
            offset: CanonicalF64::new(0.0).unwrap(),
            length: None,
            trigger: SampleTriggerAuthoring::Gate,
            warp: None,
        }
    }

    fn recording_authoring(
        identity: &EvaluationIdentity,
        lifecycle: DesiredLifecycle,
    ) -> RecordingAuthoring {
        RecordingAuthoring {
            source: TypedRef::new(identity.clone(), address::<GroupKind>("drums")),
            length: Some(RecordingLengthAuthoring::Beats(3840)),
            count_in_ticks: 0,
            metronome: false,
            destination: None,
            channels: 2,
            lifecycle,
        }
    }

    fn recording_declaration(
        key: &str,
        identity: &EvaluationIdentity,
        lifecycle: DesiredLifecycle,
        index: u32,
    ) -> DeclarationIr {
        let metadata = match lifecycle {
            DesiredLifecycle::Dormant => LifecycleMetadata::register(Composition::Standalone),
            DesiredLifecycle::Start(_) => LifecycleMetadata::start(Composition::Standalone),
        };
        DeclarationIr::new(
            address::<RecordingKind>(key),
            DeclarationOwner::Structural(
                SyntaxKey::deterministic(&module(), &[index], "owner").unwrap(),
            ),
            source(index),
            metadata,
            DeclarationPayload::authoring(AuthoringDeclaration::Recording(recording_authoring(
                identity, lifecycle,
            )))
            .unwrap(),
        )
    }

    fn recording_start_operation(
        identity: &EvaluationIdentity,
        key: &str,
        mode: StartMode,
        index: u32,
    ) -> LifecycleOperationIr {
        LifecycleOperationIr::new(
            TypedRef::new(identity.clone(), address::<RecordingKind>(key)).erase(),
            LifecycleMetadata::reference(TerminalEffect::Start, Cancellation::BeforePlanning),
            LifecycleAction::Start(mode),
            source(index),
        )
        .unwrap()
    }

    #[test]
    fn resource_authoring_validates_strict_ranges_and_sources() {
        assert!(
            DeclarationPayload::authoring(AuthoringDeclaration::Sample(sample_authoring(
                "samples/kick.wav"
            )))
            .is_ok()
        );
        for invalid in [
            SampleAuthoring {
                source: " samples/kick.wav".into(),
                ..sample_authoring("x")
            },
            SampleAuthoring {
                sustain: CanonicalF64::new(1.5).unwrap(),
                ..sample_authoring("samples/kick.wav")
            },
            SampleAuthoring {
                rate: CanonicalF64::new(0.0).unwrap(),
                ..sample_authoring("samples/kick.wav")
            },
            SampleAuthoring {
                length: Some(CanonicalF64::new(0.0).unwrap()),
                ..sample_authoring("samples/kick.wav")
            },
            SampleAuthoring {
                warp: Some(SampleWarpAuthoring {
                    speed: CanonicalF64::new(1.0).unwrap(),
                    pitch: CanonicalF64::new(1.0).unwrap(),
                    target_bpm: None,
                    window_size: CanonicalF64::new(0.1).unwrap(),
                    overlaps: 33,
                }),
                ..sample_authoring("samples/kick.wav")
            },
        ] {
            assert!(matches!(
                DeclarationPayload::authoring(AuthoringDeclaration::Sample(invalid)),
                Err(CandidateError::InvalidAuthoring(_))
            ));
        }

        assert!(
            DeclarationPayload::authoring(AuthoringDeclaration::Buffer(BufferAuthoring {
                frames: 65536,
                channels: 1,
                replacement: BufferReplacementAuthoring::Clear,
            }))
            .is_ok()
        );
        for (frames, channels) in [(0_u32, 1_u16), (1024, 0), (1024, 17)] {
            assert!(matches!(
                DeclarationPayload::authoring(AuthoringDeclaration::Buffer(BufferAuthoring {
                    frames,
                    channels,
                    replacement: BufferReplacementAuthoring::CopyOverlap,
                })),
                Err(CandidateError::InvalidAuthoring(_))
            ));
        }

        assert!(
            DeclarationPayload::authoring(AuthoringDeclaration::Sfz(SfzAuthoring {
                source: "instruments/piano.SFZ".into(),
            }))
            .is_ok()
        );
        for source in ["", "instruments/piano.wav", "piano.sfz "] {
            assert!(matches!(
                DeclarationPayload::authoring(AuthoringDeclaration::Sfz(SfzAuthoring {
                    source: source.into(),
                })),
                Err(CandidateError::InvalidAuthoring(_))
            ));
        }
    }

    #[test]
    fn resource_payload_bytes_are_canonical_and_carry_no_physical_ids() {
        let base = sample_authoring("samples/kick.wav");
        let one_shot = SampleAuthoring {
            trigger: SampleTriggerAuthoring::OneShot,
            ..sample_authoring("samples/kick.wav")
        };
        assert_eq!(
            AuthoringDeclaration::Sample(base.clone()).canonical_bytes(),
            AuthoringDeclaration::Sample(base.clone()).canonical_bytes()
        );
        assert_ne!(
            AuthoringDeclaration::Sample(base.clone()).canonical_bytes(),
            AuthoringDeclaration::Sample(one_shot).canonical_bytes()
        );
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::Sample(base)).unwrap();
        let DeclarationPayload::Authoring {
            canonical_bytes, ..
        } = &payload
        else {
            panic!("sample payload must be authoring-tagged");
        };
        DeclarationPayload::authoring_checked(
            AuthoringDeclaration::Sample(sample_authoring("samples/kick.wav")),
            canonical_bytes.clone(),
        )
        .unwrap();

        let clear = AuthoringDeclaration::Buffer(BufferAuthoring {
            frames: 1024,
            channels: 2,
            replacement: BufferReplacementAuthoring::Clear,
        });
        let copy = AuthoringDeclaration::Buffer(BufferAuthoring {
            frames: 1024,
            channels: 2,
            replacement: BufferReplacementAuthoring::CopyOverlap,
        });
        assert_ne!(clear.canonical_bytes(), copy.canonical_bytes());
    }

    #[test]
    fn recording_authoring_validates_length_channels_and_destination() {
        let identity = identity(1);
        assert!(
            DeclarationPayload::authoring(AuthoringDeclaration::Recording(recording_authoring(
                &identity,
                DesiredLifecycle::Dormant
            )))
            .is_ok()
        );
        let invalid = [
            RecordingAuthoring {
                length: Some(RecordingLengthAuthoring::Beats(0)),
                ..recording_authoring(&identity, DesiredLifecycle::Dormant)
            },
            RecordingAuthoring {
                length: Some(RecordingLengthAuthoring::Seconds(
                    CanonicalF64::new(0.0).unwrap(),
                )),
                ..recording_authoring(&identity, DesiredLifecycle::Dormant)
            },
            RecordingAuthoring {
                count_in_ticks: -1,
                ..recording_authoring(&identity, DesiredLifecycle::Dormant)
            },
            RecordingAuthoring {
                channels: 0,
                ..recording_authoring(&identity, DesiredLifecycle::Dormant)
            },
            RecordingAuthoring {
                channels: 3,
                ..recording_authoring(&identity, DesiredLifecycle::Dormant)
            },
            RecordingAuthoring {
                destination: Some("takes/one.wav ".into()),
                ..recording_authoring(&identity, DesiredLifecycle::Dormant)
            },
            recording_authoring(&identity, DesiredLifecycle::Start(StartMode::Continuous)),
        ];
        for authoring in invalid {
            assert!(matches!(
                DeclarationPayload::authoring(AuthoringDeclaration::Recording(authoring)),
                Err(CandidateError::InvalidAuthoring(_))
            ));
        }
    }

    #[test]
    fn recording_requires_its_source_group_to_resolve() {
        let identity = identity(1);
        let mut draft = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        draft
            .declare::<RecordingKind>(recording_declaration(
                "take1",
                &identity,
                DesiredLifecycle::Dormant,
                40,
            ))
            .unwrap();
        assert!(matches!(
            draft.clone().finish(&ReferenceCatalog::default()),
            Err(CandidateError::UnresolvedReference(address))
                if address.kind() == EntityKind::Group
        ));

        let mut catalog = ReferenceCatalog::default();
        catalog
            .insert(
                &TypedRef::new(identity.clone(), address::<GroupKind>("drums")),
                &identity,
            )
            .unwrap();
        draft.finish(&catalog).unwrap();
    }

    #[test]
    fn recording_admits_one_active_run_per_candidate() {
        let identity = identity(1);
        let catalog = {
            let mut catalog = ReferenceCatalog::default();
            catalog
                .insert(
                    &TypedRef::new(identity.clone(), address::<GroupKind>("drums")),
                    &identity,
                )
                .unwrap();
            catalog
        };

        let mut started_twice = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        started_twice
            .declare::<RecordingKind>(recording_declaration(
                "take1",
                &identity,
                DesiredLifecycle::Start(StartMode::Normal),
                41,
            ))
            .unwrap();
        started_twice
            .add_operation(recording_start_operation(
                &identity,
                "take1",
                StartMode::Immediate,
                42,
            ))
            .unwrap();
        assert!(matches!(
            started_twice.finish(&catalog),
            Err(CandidateError::InvalidAuthoring(message))
                if message.contains("one active run")
        ));

        let mut double_operation = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        double_operation
            .declare::<RecordingKind>(recording_declaration(
                "take1",
                &identity,
                DesiredLifecycle::Dormant,
                43,
            ))
            .unwrap();
        double_operation
            .add_operation(recording_start_operation(
                &identity,
                "take1",
                StartMode::Normal,
                44,
            ))
            .unwrap();
        double_operation
            .add_operation(recording_start_operation(
                &identity,
                "take1",
                StartMode::Immediate,
                45,
            ))
            .unwrap();
        assert!(matches!(
            double_operation.finish(&catalog),
            Err(CandidateError::InvalidAuthoring(message))
                if message.contains("one active run")
        ));

        let mut single_run = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        single_run
            .declare::<RecordingKind>(recording_declaration(
                "take1",
                &identity,
                DesiredLifecycle::Dormant,
                46,
            ))
            .unwrap();
        single_run
            .add_operation(recording_start_operation(
                &identity,
                "take1",
                StartMode::Normal,
                47,
            ))
            .unwrap();
        let candidate = single_run.finish(&catalog).unwrap();
        assert_eq!(candidate.operations().len(), 1);
    }

    fn entity_declaration<K: RefKind>(key: &str, index: u32) -> DeclarationIr {
        DeclarationIr::new(
            address::<K>(key),
            DeclarationOwner::Structural(
                SyntaxKey::deterministic(&module(), &[index], "owner").unwrap(),
            ),
            source(index),
            LifecycleMetadata::register(Composition::Standalone),
            DeclarationPayload::Empty,
        )
    }

    fn route_port(
        identity: &EvaluationIdentity,
        voice_key: &str,
        port: &str,
        rate: RoutePortRate,
    ) -> RoutePortAuthoring {
        RoutePortAuthoring {
            voice: TypedRef::new(identity.clone(), address::<VoiceKind>(voice_key)),
            port: port.into(),
            rate,
        }
    }

    fn route_declaration(route: RouteAuthoring, index: u32) -> DeclarationIr {
        DeclarationIr::new(
            TypedAddress::<RouteKind>::new(
                ProjectNamespace::new("test-project").unwrap(),
                module(),
                GroupScope::root(),
                route.canonical_key().unwrap(),
            ),
            DeclarationOwner::Structural(
                SyntaxKey::deterministic(&module(), &[index], "owner").unwrap(),
            ),
            source(index),
            LifecycleMetadata::register(Composition::Standalone),
            DeclarationPayload::authoring(AuthoringDeclaration::Route(route)).unwrap(),
        )
    }

    #[test]
    fn route_authoring_validates_rates_ports_and_destinations() {
        let identity = identity(21);
        let group = TypedRef::new(identity.clone(), address::<GroupKind>("leads"));
        let target = RouteTargetAuthoring::Voice(TypedRef::new(
            identity.clone(),
            address::<VoiceKind>("bass"),
        ));
        let rejected = [
            AuthoringDeclaration::Route(RouteAuthoring::Audio {
                source: route_port(&identity, "lfo", "out", RoutePortRate::Control),
                destinations: vec![AudioRouteDestinationAuthoring::Group(group.clone())],
            }),
            AuthoringDeclaration::Route(RouteAuthoring::Audio {
                source: route_port(&identity, "lead", "out", RoutePortRate::Audio),
                destinations: Vec::new(),
            }),
            AuthoringDeclaration::Route(RouteAuthoring::Audio {
                source: route_port(&identity, "lead", "out", RoutePortRate::Audio),
                destinations: vec![
                    AudioRouteDestinationAuthoring::Group(group.clone()),
                    AudioRouteDestinationAuthoring::Main,
                ],
            }),
            AuthoringDeclaration::Route(RouteAuthoring::Audio {
                source: route_port(&identity, "lead", "out", RoutePortRate::Audio),
                destinations: vec![
                    AudioRouteDestinationAuthoring::Group(group.clone()),
                    AudioRouteDestinationAuthoring::Group(group.clone()),
                ],
            }),
            AuthoringDeclaration::Route(RouteAuthoring::Set {
                source: route_port(&identity, "lfo", "out", RoutePortRate::Audio),
                coerce_audio: false,
                target: target.clone(),
                target_param: "cutoff".into(),
                shaping: RouteShapingAuthoring::identity(),
            }),
            AuthoringDeclaration::Route(RouteAuthoring::Set {
                source: route_port(&identity, "lead", "env", RoutePortRate::Control),
                coerce_audio: true,
                target: target.clone(),
                target_param: "cutoff".into(),
                shaping: RouteShapingAuthoring::identity(),
            }),
            AuthoringDeclaration::Route(RouteAuthoring::Bend {
                source: route_port(&identity, "lfo", "out", RoutePortRate::Trigger),
                target: target.clone(),
                target_param: "cutoff".into(),
                shaping: RouteShapingAuthoring::identity(),
            }),
            AuthoringDeclaration::Route(RouteAuthoring::Trigger {
                source: route_port(&identity, "clock", "tick", RoutePortRate::Control),
                target: target.clone(),
                target_param: "gate".into(),
            }),
            AuthoringDeclaration::Route(RouteAuthoring::Set {
                source: route_port(&identity, "lfo", "bad port", RoutePortRate::Control),
                coerce_audio: false,
                target: target.clone(),
                target_param: "cutoff".into(),
                shaping: RouteShapingAuthoring::identity(),
            }),
        ];
        for declaration in rejected {
            assert!(
                matches!(
                    DeclarationPayload::authoring(declaration.clone()),
                    Err(CandidateError::InvalidAuthoring(_))
                ),
                "expected strict rejection for {declaration:?}"
            );
        }

        assert!(
            DeclarationPayload::authoring(AuthoringDeclaration::Route(RouteAuthoring::Set {
                source: route_port(&identity, "lead", "env", RoutePortRate::Audio),
                coerce_audio: true,
                target: target.clone(),
                target_param: "cutoff".into(),
                shaping: RouteShapingAuthoring::identity(),
            }))
            .is_ok()
        );
        assert!(DeclarationPayload::authoring(AuthoringDeclaration::Route(
            RouteAuthoring::Audio {
                source: route_port(&identity, "lead", "out", RoutePortRate::Audio),
                destinations: vec![AudioRouteDestinationAuthoring::Muted],
            }
        ))
        .is_ok());
    }

    #[test]
    fn route_keys_encode_single_writer_and_fan_in_identity() {
        let identity = identity(22);
        let target = RouteTargetAuthoring::Voice(TypedRef::new(
            identity.clone(),
            address::<VoiceKind>("bass"),
        ));
        let set_from_lfo = RouteAuthoring::Set {
            source: route_port(&identity, "lfo", "out", RoutePortRate::Control),
            coerce_audio: false,
            target: target.clone(),
            target_param: "cutoff".into(),
            shaping: RouteShapingAuthoring::identity(),
        };
        let set_from_env = RouteAuthoring::Set {
            source: route_port(&identity, "env", "out", RoutePortRate::Control),
            coerce_audio: false,
            target: target.clone(),
            target_param: "cutoff".into(),
            shaping: RouteShapingAuthoring::identity(),
        };
        assert_eq!(
            set_from_lfo.canonical_key().unwrap(),
            set_from_env.canonical_key().unwrap(),
            "SET identity is target-side: a second source lands on the same slot"
        );

        let bend_from_lfo = RouteAuthoring::Bend {
            source: route_port(&identity, "lfo", "out", RoutePortRate::Control),
            target: target.clone(),
            target_param: "cutoff".into(),
            shaping: RouteShapingAuthoring::identity(),
        };
        let bend_from_env = RouteAuthoring::Bend {
            source: route_port(&identity, "env", "out", RoutePortRate::Control),
            target: target.clone(),
            target_param: "cutoff".into(),
            shaping: RouteShapingAuthoring::identity(),
        };
        assert_ne!(
            bend_from_lfo.canonical_key().unwrap(),
            bend_from_env.canonical_key().unwrap(),
            "BEND identity is edge-wise: fan-in edges are distinct declarations"
        );

        let mut draft = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        draft
            .declare::<VoiceKind>(entity_declaration::<VoiceKind>("bass", 1))
            .unwrap();
        draft
            .declare::<VoiceKind>(entity_declaration::<VoiceKind>("lfo", 2))
            .unwrap();
        draft
            .declare::<VoiceKind>(entity_declaration::<VoiceKind>("env", 3))
            .unwrap();
        draft
            .declare::<RouteKind>(route_declaration(set_from_lfo, 4))
            .unwrap();
        let error = draft
            .declare::<RouteKind>(route_declaration(set_from_env, 5))
            .unwrap_err();
        assert!(
            matches!(error, CandidateError::DuplicateDeclaration { .. }),
            "a second SET writer is a duplicate of the registry slot, got {error:?}"
        );
    }

    #[test]
    fn route_cross_verb_conflicts_reject_with_both_anchors() {
        let identity = identity(23);
        let target = RouteTargetAuthoring::Voice(TypedRef::new(
            identity.clone(),
            address::<VoiceKind>("bass"),
        ));
        let mut draft = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        draft
            .declare::<VoiceKind>(entity_declaration::<VoiceKind>("bass", 1))
            .unwrap();
        draft
            .declare::<VoiceKind>(entity_declaration::<VoiceKind>("lfo", 2))
            .unwrap();
        draft
            .declare::<VoiceKind>(entity_declaration::<VoiceKind>("env", 3))
            .unwrap();
        draft
            .declare::<RouteKind>(route_declaration(
                RouteAuthoring::Set {
                    source: route_port(&identity, "lfo", "out", RoutePortRate::Control),
                    coerce_audio: false,
                    target: target.clone(),
                    target_param: "cutoff".into(),
                    shaping: RouteShapingAuthoring::identity(),
                },
                4,
            ))
            .unwrap();
        draft
            .declare::<RouteKind>(route_declaration(
                RouteAuthoring::Bend {
                    source: route_port(&identity, "env", "out", RoutePortRate::Control),
                    target: target.clone(),
                    target_param: "cutoff".into(),
                    shaping: RouteShapingAuthoring::identity(),
                },
                5,
            ))
            .unwrap();

        let error = draft.finish(&ReferenceCatalog::default()).unwrap_err();
        match error {
            CandidateError::RouteConflict {
                target: conflict_target,
                target_param,
                existing_verb,
                existing,
                conflicting_verb,
                conflicting,
            } => {
                assert_eq!(conflict_target.kind(), EntityKind::Voice);
                assert_eq!(target_param, "cutoff");
                assert_ne!(existing_verb, conflicting_verb);
                assert_ne!(existing, conflicting, "both source anchors are reported");
                assert!(matches!(
                    (existing_verb, conflicting_verb),
                    (RouteVerb::Bend, RouteVerb::Set) | (RouteVerb::Set, RouteVerb::Bend)
                ));
            }
            other => panic!("expected RouteConflict, got {other:?}"),
        }
    }

    #[test]
    fn route_payload_bytes_are_canonical_and_shaping_sensitive() {
        let identity = identity(24);
        let target = RouteTargetAuthoring::Voice(TypedRef::new(
            identity.clone(),
            address::<VoiceKind>("bass"),
        ));
        let make = |scale: f64| {
            AuthoringDeclaration::Route(RouteAuthoring::Set {
                source: route_port(&identity, "lfo", "out", RoutePortRate::Control),
                coerce_audio: false,
                target: target.clone(),
                target_param: "cutoff".into(),
                shaping: RouteShapingAuthoring {
                    scale: CanonicalF64::new(scale).unwrap(),
                    offset: CanonicalF64::new(0.0).unwrap(),
                },
            })
        };
        assert_eq!(make(2.0).canonical_bytes(), make(2.0).canonical_bytes());
        assert_ne!(make(2.0).canonical_bytes(), make(3.0).canonical_bytes());
    }

    #[test]
    fn route_endpoints_must_resolve_in_the_candidate() {
        let identity = identity(25);
        let mut draft = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        draft
            .declare::<RouteKind>(route_declaration(
                RouteAuthoring::Audio {
                    source: route_port(&identity, "ghost", "out", RoutePortRate::Audio),
                    destinations: vec![AudioRouteDestinationAuthoring::Main],
                },
                1,
            ))
            .unwrap();
        assert!(matches!(
            draft.finish(&ReferenceCatalog::default()),
            Err(CandidateError::UnresolvedReference(_))
        ));
    }

    #[test]
    fn route_topology_reports_complete_fan_metadata() {
        let identity = identity(26);
        let bass = RouteTargetAuthoring::Voice(TypedRef::new(
            identity.clone(),
            address::<VoiceKind>("bass"),
        ));
        let mut draft = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        for (key, index) in [("bass", 1), ("lead", 2), ("lfo", 3), ("env", 4)] {
            draft
                .declare::<VoiceKind>(entity_declaration::<VoiceKind>(key, index))
                .unwrap();
        }
        draft
            .declare::<GroupKind>(entity_declaration::<GroupKind>("leads", 5))
            .unwrap();
        draft
            .declare::<GroupKind>(entity_declaration::<GroupKind>("sends", 6))
            .unwrap();
        let leads = TypedRef::new(identity.clone(), address::<GroupKind>("leads"));
        let sends = TypedRef::new(identity.clone(), address::<GroupKind>("sends"));
        draft
            .declare::<RouteKind>(route_declaration(
                RouteAuthoring::Audio {
                    source: route_port(&identity, "lead", "out", RoutePortRate::Audio),
                    destinations: vec![
                        AudioRouteDestinationAuthoring::Group(leads),
                        AudioRouteDestinationAuthoring::Group(sends),
                    ],
                },
                7,
            ))
            .unwrap();
        draft
            .declare::<RouteKind>(route_declaration(
                RouteAuthoring::Set {
                    source: route_port(&identity, "lead", "body", RoutePortRate::Audio),
                    coerce_audio: true,
                    target: bass.clone(),
                    target_param: "cutoff".into(),
                    shaping: RouteShapingAuthoring {
                        scale: CanonicalF64::new(0.5).unwrap(),
                        offset: CanonicalF64::new(0.25).unwrap(),
                    },
                },
                8,
            ))
            .unwrap();
        for (voice, index) in [("lfo", 9_u32), ("env", 10)] {
            draft
                .declare::<RouteKind>(route_declaration(
                    RouteAuthoring::Bend {
                        source: route_port(&identity, voice, "out", RoutePortRate::Control),
                        target: bass.clone(),
                        target_param: "resonance".into(),
                        shaping: RouteShapingAuthoring::identity(),
                    },
                    index,
                ))
                .unwrap();
        }
        draft
            .declare::<RouteKind>(route_declaration(
                RouteAuthoring::Input {
                    target: TypedRef::new(identity.clone(), address::<VoiceKind>("bass")),
                    input_port: "sidechain".into(),
                    source: InputRouteSourceAuthoring::Silent,
                },
                11,
            ))
            .unwrap();

        let candidate = draft.finish(&ReferenceCatalog::default()).unwrap();
        let topology = candidate.route_topology();

        let lead_out = RouteEndpoint {
            voice: address::<VoiceKind>("lead").erase(),
            port: "out".into(),
        };
        let audio = topology.audio.get(&lead_out).unwrap();
        assert_eq!(audio.fan_out(), 2);
        assert_eq!(audio.group_destinations.len(), 2);
        assert!(!audio.to_main);
        assert!(!audio.muted);

        let cutoff = topology
            .params
            .get(&(address::<VoiceKind>("bass").erase(), "cutoff".into()))
            .unwrap();
        assert_eq!(cutoff.verb, RouteVerb::Set);
        assert_eq!(cutoff.fan_in(), 1);
        assert!(cutoff.edges[0].coerced_from_audio);
        assert_eq!(cutoff.edges[0].scale.unwrap().get(), 0.5);
        assert_eq!(cutoff.edges[0].offset.unwrap().get(), 0.25);

        let resonance = topology
            .params
            .get(&(address::<VoiceKind>("bass").erase(), "resonance".into()))
            .unwrap();
        assert_eq!(resonance.verb, RouteVerb::Bend);
        assert_eq!(resonance.fan_in(), 2, "BEND fan-in is additive");
        assert!(resonance
            .edges
            .iter()
            .all(|edge| !edge.coerced_from_audio && edge.scale.is_some()));

        let lfo_out = RouteEndpoint {
            voice: address::<VoiceKind>("lfo").erase(),
            port: "out".into(),
        };
        assert_eq!(topology.param_fan_out(&lfo_out), 1);
        let lead_body = RouteEndpoint {
            voice: address::<VoiceKind>("lead").erase(),
            port: "body".into(),
        };
        assert_eq!(topology.param_fan_out(&lead_body), 1);

        let sidechain = topology
            .inputs
            .get(&(address::<VoiceKind>("bass").erase(), "sidechain".into()))
            .unwrap();
        assert!(
            sidechain.source.is_none(),
            "explicit disconnect reports no source"
        );
    }

    fn midi_device_ref(identity: &EvaluationIdentity, key: &str) -> TypedRef<MidiDeviceKind> {
        TypedRef::new(identity.clone(), address::<MidiDeviceKind>(key))
    }

    fn midi_route_declaration(route: MidiRouteAuthoring, index: u32) -> DeclarationIr {
        DeclarationIr::new(
            TypedAddress::<MidiRouteKind>::new(
                ProjectNamespace::new("test-project").unwrap(),
                module(),
                GroupScope::root(),
                route.canonical_key().unwrap(),
            ),
            DeclarationOwner::Structural(
                SyntaxKey::deterministic(&module(), &[index], "owner").unwrap(),
            ),
            source(index),
            LifecycleMetadata::register(Composition::Standalone),
            DeclarationPayload::authoring(AuthoringDeclaration::MidiRoute(route)).unwrap(),
        )
    }

    #[test]
    fn midi_channels_groups_and_values_are_strict_width_qualified_boundaries() {
        for number in [1, 8, 16] {
            let channel = MidiChannel::from_number(number).unwrap();
            assert_eq!(i64::from(channel.number()), number);
            assert_eq!(i64::from(channel.index()), number - 1);
            let group = MidiGroup::from_number(number).unwrap();
            assert_eq!(i64::from(group.number()), number);
        }
        for number in [0, 17, -1] {
            assert!(MidiChannel::from_number(number).is_err());
            assert!(MidiGroup::from_number(number).is_err());
        }
        assert_eq!(MidiChannel::from_index(0).unwrap().number(), 1);
        assert_eq!(MidiChannel::from_index(15).unwrap().number(), 16);
        assert!(MidiChannel::from_index(16).is_err());
        assert!(MidiGroup::from_index(-1).is_err());

        let table: [(fn(i64) -> Result<MidiValue, CandidateError>, i64, u8); 4] = [
            (MidiValue::v7, 127, 7),
            (MidiValue::v14, 16_383, 14),
            (MidiValue::v16, 65_535, 16),
            (MidiValue::v32, i64::from(u32::MAX), 32),
        ];
        for (constructor, max, width) in table {
            let value = constructor(max).unwrap();
            assert_eq!(value.width_bits(), width);
            assert_eq!(i64::try_from(value.raw()).unwrap(), max);
            assert_eq!(constructor(0).unwrap().raw(), 0);
            assert!(constructor(max + 1).is_err(), "{width}-bit max+1 rejects");
            assert!(constructor(-1).is_err(), "{width}-bit negatives reject");
        }
    }

    #[test]
    fn midi_authoring_validates_devices_routes_and_callbacks_strictly() {
        let identity = identity(31);
        let device = midi_device_ref(&identity, "mpk");
        let voice = TypedRef::new(identity.clone(), address::<VoiceKind>("lead"));
        let target = RouteTargetAuthoring::Voice(voice.clone());

        for port in ["", " padded ", "ctrl\u{1}byte", &"p".repeat(256)] {
            let declaration = AuthoringDeclaration::MidiDevice(MidiDeviceAuthoring {
                port: port.into(),
                direction: MidiDeviceDirectionAuthoring::Output,
            });
            assert!(
                matches!(
                    DeclarationPayload::authoring(declaration),
                    Err(CandidateError::InvalidAuthoring(_))
                ),
                "port {port:?} rejects"
            );
        }

        assert!(matches!(
            DeclarationPayload::authoring(AuthoringDeclaration::MidiRoute(
                MidiRouteAuthoring::Cc {
                    device: device.clone(),
                    channel: None,
                    controller: 128,
                    target: target.clone(),
                    target_param: "cutoff".into(),
                    min: CanonicalF64::new(0.0).unwrap(),
                    max: CanonicalF64::new(1.0).unwrap(),
                }
            )),
            Err(CandidateError::InvalidAuthoring(_))
        ));

        assert!(matches!(
            DeclarationPayload::authoring(AuthoringDeclaration::Callback(CallbackAuthoring {
                device: device.clone(),
                trigger: CallbackTriggerAuthoring::NoteOn {
                    channel: None,
                    note: Some(128),
                },
                handler: "on_pad".into(),
            })),
            Err(CandidateError::InvalidAuthoring(_))
        ));
        assert!(matches!(
            DeclarationPayload::authoring(AuthoringDeclaration::Callback(CallbackAuthoring {
                device,
                trigger: CallbackTriggerAuthoring::ClockSync,
                handler: "bad handler".into(),
            })),
            Err(CandidateError::InvalidAddress(_))
        ));
    }

    #[test]
    fn midi_route_keys_are_edgewise_for_keyboard_and_single_writer_for_cc() {
        let identity = identity(32);
        let device = midi_device_ref(&identity, "mpk");
        let voice = TypedRef::new(identity.clone(), address::<VoiceKind>("lead"));
        let channel = MidiChannel::from_number(2).unwrap();

        let keyboard = MidiRouteAuthoring::Keyboard {
            device: device.clone(),
            channel: Some(channel),
            voice: voice.clone(),
        };
        assert_eq!(
            keyboard.canonical_key().unwrap().as_str(),
            "keyboard.mpk.2.lead"
        );
        let other_voice = MidiRouteAuthoring::Keyboard {
            device: device.clone(),
            channel: Some(channel),
            voice: TypedRef::new(identity.clone(), address::<VoiceKind>("pad")),
        };
        assert_ne!(
            keyboard.canonical_key().unwrap(),
            other_voice.canonical_key().unwrap(),
            "keyboard routes are edge-wise: another voice is another declaration"
        );

        let cc = |controller: u8| MidiRouteAuthoring::Cc {
            device: device.clone(),
            channel: None,
            controller,
            target: RouteTargetAuthoring::Voice(voice.clone()),
            target_param: "cutoff".into(),
            min: CanonicalF64::new(0.0).unwrap(),
            max: CanonicalF64::new(1.0).unwrap(),
        };
        assert_eq!(
            cc(20).canonical_key().unwrap(),
            cc(21).canonical_key().unwrap(),
            "CC routes are target-side single-writer: a second controller on the same \
             target parameter lands on the same address"
        );

        let mut draft = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        draft
            .declare::<MidiRouteKind>(midi_route_declaration(cc(20), 1))
            .unwrap();
        assert!(matches!(
            draft.declare::<MidiRouteKind>(midi_route_declaration(cc(21), 2)),
            Err(CandidateError::DuplicateDeclaration { .. })
        ));
    }

    #[test]
    fn midi_payload_bytes_are_canonical_and_channel_sensitive() {
        let identity = identity(33);
        let device = midi_device_ref(&identity, "mpk");
        let voice = TypedRef::new(identity.clone(), address::<VoiceKind>("lead"));
        let keyboard = |channel: Option<MidiChannel>| {
            AuthoringDeclaration::MidiRoute(MidiRouteAuthoring::Keyboard {
                device: device.clone(),
                channel,
                voice: voice.clone(),
            })
        };

        let all_channels = keyboard(None).canonical_bytes();
        assert_eq!(
            all_channels,
            keyboard(None).canonical_bytes(),
            "identical MIDI authoring encodes identical canonical bytes"
        );
        assert_ne!(
            all_channels,
            keyboard(Some(MidiChannel::from_number(3).unwrap())).canonical_bytes(),
            "the channel qualifier is part of the canonical payload"
        );

        let callback = AuthoringDeclaration::Callback(CallbackAuthoring {
            device: device.clone(),
            trigger: CallbackTriggerAuthoring::ControlChange {
                channel: Some(MidiChannel::from_number(1).unwrap()),
                controller: Some(64),
            },
            handler: "on_sustain".into(),
        });
        let renamed = AuthoringDeclaration::Callback(CallbackAuthoring {
            device: device.clone(),
            trigger: CallbackTriggerAuthoring::ControlChange {
                channel: Some(MidiChannel::from_number(1).unwrap()),
                controller: Some(64),
            },
            handler: "on_sustain_b".into(),
        });
        assert_ne!(callback.canonical_bytes(), renamed.canonical_bytes());
    }

    #[test]
    fn midi_routes_and_callbacks_must_resolve_their_device_in_the_candidate() {
        let identity = identity(34);
        let device = midi_device_ref(&identity, "ghost");
        let voice = TypedRef::new(identity.clone(), address::<VoiceKind>("lead"));

        let mut draft = CandidateDraft::new(identity.clone(), CandidateOrigin::RhaiHost);
        draft
            .declare::<VoiceKind>(entity_declaration::<VoiceKind>("lead", 1))
            .unwrap();
        draft
            .declare::<MidiRouteKind>(midi_route_declaration(
                MidiRouteAuthoring::Keyboard {
                    device: device.clone(),
                    channel: None,
                    voice,
                },
                2,
            ))
            .unwrap();
        assert!(matches!(
            draft.finish(&ReferenceCatalog::default()),
            Err(CandidateError::UnresolvedReference(address))
                if address.kind() == EntityKind::MidiDevice
        ));
    }
}
