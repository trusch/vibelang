//! Group API for Rhai scripts.
//!
//! Groups organize voices and provide hierarchical mixing.

use rhai::{
    Array, CustomType, Engine, EvalAltResult, FnPtr, NativeCallContext, Position, TypeBuilder,
};
use std::collections::{BTreeMap, HashMap};
use vibelang_core::reload::{GroupAliasError, GroupAliasTarget, GroupConfig};

use crate::context;

/// Handle to a defined group.
#[derive(Debug, Clone, CustomType)]
pub struct GroupHandle {
    /// Full path to the group.
    pub path: String,
    /// Group name (last segment of path).
    pub name: String,
}

impl GroupHandle {
    /// Create a new group handle.
    pub fn new(path: String) -> Self {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        Self { path, name }
    }

    /// Get the group name.
    pub fn get_name(&mut self) -> String {
        self.name.clone()
    }

    /// Get the parent group path.
    ///
    /// Returns an empty string for top-level groups: the implicit root has
    /// no name.
    pub fn parent(&mut self) -> String {
        if let Some(pos) = self.path.rfind('/') {
            self.path[..pos].to_string()
        } else {
            String::new()
        }
    }

    /// Set the group gain.
    pub fn gain(self, value: f64) -> Self {
        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            if let Some(config) = state.groups.get_mut(&group_id) {
                config.params.insert("amp".to_string(), value as f32);
            }
        });
        self
    }

    /// Mute the group.
    pub fn mute(self) -> Self {
        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            if let Some(config) = state.groups.get_mut(&group_id) {
                config.muted = true;
            }
        });
        self
    }

    /// Unmute the group.
    pub fn unmute(self) -> Self {
        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            if let Some(config) = state.groups.get_mut(&group_id) {
                config.muted = false;
            }
        });
        self
    }

    /// Solo the group.
    pub fn solo(self, enabled: bool) -> Self {
        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            if let Some(config) = state.groups.get_mut(&group_id) {
                config.soloed = enabled;
            }
        });
        self
    }

    /// Check if the group is muted.
    pub fn is_muted(&mut self) -> bool {
        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            state
                .groups
                .get(&group_id)
                .map(|c| c.muted)
                .unwrap_or(false)
        })
    }

    /// Check if the group is soloed.
    pub fn is_soloed(&mut self) -> bool {
        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            state
                .groups
                .get(&group_id)
                .map(|c| c.soloed)
                .unwrap_or(false)
        })
    }

    /// Set a parameter on the group.
    pub fn set_param(self, name: String, value: f64) -> Self {
        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            if let Some(config) = state.groups.get_mut(&group_id) {
                config.params.insert(name, value as f32);
            }
        });
        self
    }

    /// Get the number of effects on this group.
    pub fn effect_count(&mut self) -> i64 {
        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            state
                .groups
                .get(&group_id)
                .map(|c| c.effects.len() as i64)
                .unwrap_or(0)
        })
    }

    /// Remove an effect from this group by name/ID.
    ///
    /// Returns true if the effect was found and removed, false otherwise.
    pub fn remove_effect(self, effect_name: String) -> Self {
        let group_id = context::get_or_create_group_id(&self.path);
        let effect_id = context::get_or_create_effect_id(&effect_name);

        context::with_state(|state| {
            // Remove from group's effects list
            if let Some(config) = state.groups.get_mut(&group_id) {
                config.effects.retain(|&e| e != effect_id);
            }
            // Also remove from the effects map
            state.remove_effect(&effect_id);
        });
        self
    }

    /// Route this group's audio to a specific hardware output bus (mono).
    ///
    /// `bus` is a 0-indexed hardware output channel; the group mixdown is
    /// routed via `system_link_audio_mono` to that single channel.
    pub fn output_mono(self, bus: i64) -> Self {
        if bus < 0 || (bus as u32) >= 16 {
            log::error!(
                "group('{}').output({}): bus must be in 0..16. Supported forms: \
                 group.output(N) for mono, group.output([N]) for mono, group.output([L, R]) for stereo",
                self.name,
                bus,
            );
            return self;
        }

        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            if let Some(config) = state.groups.get_mut(&group_id) {
                config.output_bus = Some(bus as u32);
                config.output_channels = Some(1);
            }
        });

        tracing::info!(
            "Group '{}' output routed to hardware channel {} (mono)",
            self.name,
            bus
        );

        self
    }

    /// Route this group's audio to a specific hardware output bus or pair.
    ///
    /// Accepts:
    /// - `[N]` — mono, equivalent to `output(N)`.
    /// - `[L, R]` — stereo, consecutive channels (e.g. `[2, 3]`).
    ///
    /// Without this, the group mixes into its parent (default behavior).
    pub fn output(self, channels: Array) -> Self {
        match channels.len() {
            1 => {
                let bus = channels[0].as_int().unwrap_or(-1);
                self.output_mono(bus)
            }
            2 => {
                let left = channels[0].as_int().unwrap_or(-1);
                let right = channels[1].as_int().unwrap_or(-1);

                if left < 0 || right < 0 || right != left + 1 {
                    log::error!(
                        "group('{}').output([{}, {}]): channels must be consecutive (e.g. [0,1], [2,3]). \
                         Supported forms: group.output(N) for mono, group.output([L, R]) for stereo",
                        self.name,
                        left,
                        right
                    );
                    return self;
                }

                if (left as u32) >= 16 {
                    log::error!(
                        "group('{}').output([{}, {}]): channel index {} exceeds reasonable hardware output range (0-15). \
                         Make sure scsynth is started with enough output channels (--output-channels {})",
                        self.name,
                        left,
                        right,
                        right,
                        right + 1,
                    );
                    return self;
                }

                let group_id = context::get_or_create_group_id(&self.path);
                context::with_state(|state| {
                    if let Some(config) = state.groups.get_mut(&group_id) {
                        config.output_bus = Some(left as u32);
                        config.output_channels = Some(2);
                    }
                });

                tracing::info!(
                    "Group '{}' output routed to hardware channels [{}, {}] (stereo)",
                    self.name,
                    left,
                    right
                );

                self
            }
            n => {
                log::error!(
                    "group('{}').output() got {}-element array; supported forms: \
                     group.output(N) for mono, group.output([N]) for mono, group.output([L, R]) for stereo",
                    self.name,
                    n,
                );
                self
            }
        }
    }

    /// Clear all effects from this group.
    pub fn clear_effects(self) -> Self {
        let group_id = context::get_or_create_group_id(&self.path);

        context::with_state(|state| {
            let effect_ids = if let Some(config) = state.groups.get_mut(&group_id) {
                // Remove all effect configs
                config.effects.drain(..).collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            for effect_id in effect_ids {
                state.remove_effect(&effect_id);
            }
        });
        self
    }

    /// Define the group body with a closure (builder method).
    ///
    /// This is the builder equivalent of `define_group("name", || { ... })`.
    /// The closure executes in the group's context, allowing nested definitions.
    fn body(ctx: NativeCallContext, handle: Self, closure: FnPtr) -> Self {
        let group_id = context::get_or_create_group_id(&handle.path);

        context::with_state(|state| {
            if let Some(config) = state.groups.get_mut(&group_id) {
                config.name = handle.name.clone();
            }
        });

        let pos = ctx.call_position();
        let contribution_id = context::begin_body_contribution(
            group_id,
            handle.path.clone(),
            ctx.call_source().map(ToOwned::to_owned),
            pos.line().map(|line| line as u32),
            pos.position().map(|column| column as u32),
        );

        let result = context::with_group_path(&handle.path, || {
            closure.call_within_context::<rhai::Dynamic>(&ctx, ())
        });
        context::end_body_contribution(contribution_id);

        if let Err(e) = result {
            log::error!("Error in group('{}').body(): {}", handle.name, e);
        }

        handle
    }

    /// Register an authoring alias for a canonical group handle.
    fn alias(
        ctx: NativeCallContext,
        handle: Self,
        alias: String,
    ) -> Result<Self, Box<EvalAltResult>> {
        context::add_group_alias(alias, handle.path.clone())
            .map_err(|err| alias_error(err, ctx.call_position()))?;
        Ok(handle)
    }
}

fn format_alias_target(target: &GroupAliasTarget) -> String {
    format!("'{}' ({:?})", target.path, target.group_id)
}

pub(crate) fn format_group_alias_error(err: GroupAliasError) -> String {
    match err {
        GroupAliasError::InvalidAliasName { alias, reason } => {
            format!("group alias '{}' is invalid: {}", alias, reason)
        }
        GroupAliasError::ConflictingAliasTarget {
            alias,
            existing,
            attempted,
        } => format!(
            "group alias '{}' already points to {}, cannot point it to {}",
            alias,
            format_alias_target(&existing),
            format_alias_target(&attempted)
        ),
        GroupAliasError::ConflictingCanonicalGroupName {
            alias,
            existing,
            attempted,
        } => format!(
            "group alias '{}' collides with canonical group {}, cannot point it to {}",
            alias,
            format_alias_target(&existing),
            format_alias_target(&attempted)
        ),
        GroupAliasError::ConflictingContextualClaims {
            alias,
            existing,
            attempted,
        } => {
            let existing = existing
                .iter()
                .map(format_alias_target)
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "group alias '{}' conflicts with prior contextual group claim(s) {}; cannot point it to {}",
                alias,
                existing,
                format_alias_target(&attempted)
            )
        }
    }
}

pub(crate) fn alias_error(err: GroupAliasError, pos: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        format_group_alias_error(err).into(),
        pos,
    ))
}

/// Define a group with a closure.
pub fn define_group(
    ctx: NativeCallContext,
    name: String,
    closure: FnPtr,
) -> Result<GroupHandle, Box<EvalAltResult>> {
    let full_path = context::resolve_group_reference(&name)
        .map_err(|err| alias_error(err, ctx.call_position()))?;
    let group_name = full_path
        .rsplit('/')
        .next()
        .unwrap_or(&full_path)
        .to_string();
    let parent_path = full_path
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_else(|| "main".to_string());

    // Get or create group ID
    let group_id = context::get_or_create_group_id(&full_path);

    // Find parent group ID
    let parent_id = if parent_path == "main" {
        None
    } else {
        Some(context::get_or_create_group_id(&parent_path))
    };

    // Create group config
    let config = GroupConfig {
        name: group_name.clone(),
        parent: parent_id,
        params: HashMap::new(),
        effects: Vec::new(),
        muted: false,
        soloed: false,
        output_bus: None,
        output_channels: None,
    };

    // Add to script state
    context::with_state(|state| {
        state.groups.entry(group_id).or_insert(config);
    });

    context::with_group_path(&full_path, || {
        if let Err(e) = closure.call_within_context::<rhai::Dynamic>(&ctx, ()) {
            log::error!("Error in define_group '{}': {}", group_name, e);
        }
    });

    Ok(GroupHandle::new(full_path))
}

/// Get a group handle by path.
pub fn group(ctx: NativeCallContext, path: String) -> Result<GroupHandle, Box<EvalAltResult>> {
    let full_path = context::resolve_group_reference(&path)
        .map_err(|err| alias_error(err, ctx.call_position()))?;
    Ok(GroupHandle::new(full_path))
}

/// Register group API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register types
    engine.build_type::<GroupHandle>();

    // Constructors
    engine.register_fn("define_group", define_group);
    engine.register_fn("group", group);

    // GroupHandle methods
    engine.register_fn("name", GroupHandle::get_name);
    engine.register_get("name", GroupHandle::get_name);
    engine.register_fn("parent", GroupHandle::parent);
    engine.register_fn("gain", GroupHandle::gain);

    // Mute/solo
    engine.register_fn("mute", GroupHandle::mute);
    engine.register_fn("unmute", GroupHandle::unmute);
    engine.register_fn("solo", GroupHandle::solo);
    engine.register_fn("is_muted", GroupHandle::is_muted);
    engine.register_get("muted", GroupHandle::is_muted);
    engine.register_fn("is_soloed", GroupHandle::is_soloed);
    engine.register_get("soloed", GroupHandle::is_soloed);

    // Parameters
    engine.register_fn("set_param", GroupHandle::set_param);
    engine.register_fn("effect_count", GroupHandle::effect_count);

    // Group body and output routing
    engine.register_fn("body", GroupHandle::body);
    engine.register_fn("alias", GroupHandle::alias);
    engine.register_fn("output", GroupHandle::output);
    engine.register_fn("output", GroupHandle::output_mono);

    // Effect management
    engine.register_fn("remove_effect", GroupHandle::remove_effect);
    engine.register_fn("clear_effects", GroupHandle::clear_effects);
}

use vibelang_core::candidate::{
    Cancellation, CandidateError, CandidateFragment, CanonicalF64, Composition, ContributionId,
    ContributionIr, DeclarationOwner, DeclarationPayload, GroupAuthoring, GroupKind, GroupScope,
    LifecycleAction, LifecycleMetadata, TerminalEffect,
};

use crate::foundation::{self, BuilderBase, FoundationError, Observation, RefBase};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupRef {
    base: RefBase,
}

impl GroupRef {
    fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<GroupKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    pub(crate) fn scope(&self) -> Result<GroupScope, FoundationError> {
        let mut components = self
            .base
            .address()
            .group_scope()
            .components()
            .iter()
            .map(|component| component.as_str().to_string())
            .collect::<Vec<_>>();
        components.push(self.base.address().key().as_str().to_string());
        Ok(GroupScope::new(components)?)
    }

    fn action(self, action: LifecycleAction, role: &str) -> Result<Self, FoundationError> {
        let (effect, cancellation) = match &action {
            LifecycleAction::Remove => (TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::SetMuted(_)
            | LifecycleAction::SetSoloed(_)
            | LifecycleAction::RemoveContribution(_) => {
                (TerminalEffect::Register, Cancellation::NotCancellable)
            }
            _ => {
                return Err(CandidateError::InvalidLifecycle(
                    "unsupported GroupRef lifecycle action".into(),
                )
                .into())
            }
        };
        let source = foundation::operation_source(&self.base, role)?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(effect, cancellation),
            action,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn mute(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::SetMuted(true), "mute")
    }

    pub fn unmute(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::SetMuted(false), "unmute")
    }

    pub fn solo(self, enabled: bool) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::SetSoloed(enabled), "solo")
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Remove, "remove")
    }

    pub fn remove_contribution(self, id: String) -> Result<Self, FoundationError> {
        let key = vibelang_core::candidate::DeclarationKey::new(id)?;
        let contribution = ContributionId::new(
            self.base.address().module().clone(),
            vibelang_core::candidate::SyntaxKey::Explicit(key),
        );
        self.action(
            LifecycleAction::RemoveContribution(contribution),
            "remove-contribution",
        )
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GroupContribution {
    id: ContributionId,
    order: Option<i32>,
    source: vibelang_core::candidate::SourceAnchor,
    fragment: CandidateFragment,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupBuilder {
    base: BuilderBase,
    parent: Option<GroupRef>,
    gain: f64,
    muted: bool,
    soloed: bool,
    params: BTreeMap<String, f64>,
    output_channels: Option<(u32, u8)>,
    contributions: Vec<GroupContribution>,
}

impl GroupBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        Self {
            base,
            parent: None,
            gain: 1.0,
            muted: false,
            soloed: false,
            params: BTreeMap::new(),
            output_channels: None,
            contributions: Vec::new(),
        }
    }

    #[must_use]
    pub fn gain(mut self, value: f64) -> Self {
        self.gain = value;
        self
    }

    #[must_use]
    pub fn muted(mut self, value: bool) -> Self {
        self.muted = value;
        self
    }

    #[must_use]
    pub fn soloed(mut self, value: bool) -> Self {
        self.soloed = value;
        self
    }

    pub fn parent(mut self, parent: GroupRef) -> Result<Self, FoundationError> {
        self.base = self.base.in_group(parent.scope()?);
        self.parent = Some(parent);
        Ok(self)
    }

    pub fn set_param(mut self, name: String, value: f64) -> Result<Self, FoundationError> {
        self.base = self
            .base
            .configured(format!("param.{name}"), value.to_bits().to_be_bytes())?;
        self.params.insert(name, value);
        Ok(self)
    }

    pub fn output(mut self, bus: i64, channels: i64) -> Result<Self, FoundationError> {
        if !(0..16).contains(&bus) || !matches!(channels, 1 | 2) || bus + channels > 16 {
            return Err(CandidateError::InvalidAuthoring(
                "Group output needs bus 0..15 and one or two in-range channels".into(),
            )
            .into());
        }
        self.output_channels = Some((bus as u32, channels as u8));
        Ok(self)
    }

    fn push_contribution(
        self,
        id: String,
        order: Option<i32>,
        fragment: CandidateFragment,
    ) -> Result<Self, FoundationError> {
        let key = vibelang_core::candidate::DeclarationKey::new(id)?;
        self.push_contribution_with_key(
            vibelang_core::candidate::SyntaxKey::Explicit(key),
            order,
            fragment,
        )
    }

    fn push_contribution_with_key(
        mut self,
        key: vibelang_core::candidate::SyntaxKey,
        order: Option<i32>,
        fragment: CandidateFragment,
    ) -> Result<Self, FoundationError> {
        let source = vibelang_core::candidate::SourceAnchor::new(
            self.base.source().module().clone(),
            key.clone(),
            None,
        );
        let contribution_id = ContributionId::new(self.base.source().module().clone(), key);
        if self
            .contributions
            .iter()
            .any(|contribution| contribution.id == contribution_id)
        {
            return Err(CandidateError::DuplicateContribution(contribution_id).into());
        }
        self.contributions.push(GroupContribution {
            id: contribution_id,
            order,
            source,
            fragment,
        });
        Ok(self)
    }

    fn capture_body(
        ctx: NativeCallContext,
        builder: Self,
        id: String,
        order: Option<i32>,
        closure: FnPtr,
    ) -> Result<Self, Box<EvalAltResult>> {
        let key = vibelang_core::candidate::DeclarationKey::new(id)
            .map_err(|error| v2_error(error.into(), ctx.call_position()))?;
        Self::capture_body_with_key(
            ctx,
            builder,
            vibelang_core::candidate::SyntaxKey::Explicit(key),
            order,
            closure,
        )
    }

    fn capture_body_with_key(
        ctx: NativeCallContext,
        builder: Self,
        key: vibelang_core::candidate::SyntaxKey,
        order: Option<i32>,
        closure: FnPtr,
    ) -> Result<Self, Box<EvalAltResult>> {
        let contribution_id =
            ContributionId::new(builder.base.source().module().clone(), key.clone());
        foundation::begin_fragment(DeclarationOwner::Contribution(contribution_id))
            .map_err(|error| v2_error(error, ctx.call_position()))?;
        if let Err(error) = closure.call_within_context::<rhai::Dynamic>(&ctx, ()) {
            foundation::abort_fragment();
            return Err(error);
        }
        let fragment =
            foundation::finish_fragment().map_err(|error| v2_error(error, ctx.call_position()))?;
        builder
            .push_contribution_with_key(key, order, fragment)
            .map_err(|error| v2_error(error, ctx.call_position()))
    }

    fn body(
        ctx: NativeCallContext,
        builder: Self,
        id: String,
        closure: FnPtr,
    ) -> Result<Self, Box<EvalAltResult>> {
        Self::capture_body(ctx, builder, id, None, closure)
    }

    fn body_ordered(
        ctx: NativeCallContext,
        builder: Self,
        id: String,
        order: i64,
        closure: FnPtr,
    ) -> Result<Self, Box<EvalAltResult>> {
        let position = ctx.call_position();
        let order = i32::try_from(order).map_err(|_| {
            v2_error(
                CandidateError::InvalidAuthoring(
                    "Group contribution order must fit a signed 32-bit integer".into(),
                )
                .into(),
                position,
            )
        })?;
        Self::capture_body(ctx, builder, id, Some(order), closure)
    }

    pub fn apply(self) -> Result<GroupRef, FoundationError> {
        let parent_ref = self.parent.as_ref().map(|parent| parent.base.clone());
        let parent = self
            .parent
            .as_ref()
            .map(|parent| parent.base.typed::<GroupKind>())
            .transpose()?;
        let params = self
            .params
            .into_iter()
            .map(|(name, value)| Ok((name, CanonicalF64::new(value)?)))
            .collect::<Result<BTreeMap<_, _>, CandidateError>>()?;
        let declaration = GroupAuthoring {
            parent,
            gain: CanonicalF64::new(self.gain)?,
            muted: self.muted,
            soloed: self.soloed,
            params,
            output_channels: self.output_channels,
        };
        let payload = DeclarationPayload::authoring(
            vibelang_core::candidate::AuthoringDeclaration::Group(declaration),
        )?;
        let source = self.base.source().clone();
        let references = parent_ref
            .into_iter()
            .map(|reference| (reference, source.clone()));
        let (mut fragment, reference) = self.base.fragment(
            DeclarationOwner::Structural(source.syntax_key().clone()),
            LifecycleMetadata::register(Composition::Standalone),
            payload,
            references,
        )?;
        let typed = reference.typed::<GroupKind>()?;
        for contribution in self.contributions {
            fragment = fragment.contribution(ContributionIr::new(
                contribution.id,
                typed.clone(),
                contribution.order,
                contribution.source,
            ));
            fragment.extend(contribution.fragment);
        }
        foundation::commit_fragment(fragment)?;
        GroupRef::new(reference)
    }
}

pub(crate) fn split_group_path(path: &str) -> Result<(String, GroupScope), FoundationError> {
    if path.is_empty() || path.split('/').any(str::is_empty) {
        return Err(FoundationError::Candidate(CandidateError::InvalidAddress(
            "Group path must be project-relative with no empty components".into(),
        )));
    }
    let mut components = path.split('/').map(ToOwned::to_owned).collect::<Vec<_>>();
    let key = components.pop().ok_or_else(|| {
        FoundationError::Candidate(CandidateError::InvalidAddress(
            "Group path must contain a canonical component".into(),
        ))
    })?;
    Ok((key, GroupScope::new(components)?))
}

pub(crate) fn group_builder_v2(path: String) -> Result<GroupBuilder, Box<EvalAltResult>> {
    let (key, scope) = split_group_path(&path).map_err(|error| v2_error(error, Position::NONE))?;
    let mut builder = GroupBuilder::new(
        foundation::authoring_builder::<GroupKind>(&key, scope.clone())
            .map_err(|error| v2_error(error, Position::NONE))?,
    );
    if !scope.components().is_empty() {
        let mut parent_components = scope
            .components()
            .iter()
            .map(|component| component.as_str().to_string())
            .collect::<Vec<_>>();
        let parent_key = parent_components.pop().expect("non-empty Group scope");
        let parent_scope = GroupScope::new(parent_components)
            .map_err(|error| v2_error(error.into(), Position::NONE))?;
        let parent = GroupRef::new(
            foundation::authoring_ref::<GroupKind>(&parent_key, parent_scope)
                .map_err(|error| v2_error(error, Position::NONE))?,
        )
        .map_err(|error| v2_error(error, Position::NONE))?;
        builder = builder
            .parent(parent)
            .map_err(|error| v2_error(error, Position::NONE))?;
    }
    Ok(builder)
}

pub(crate) fn group_ref_v2(path: String) -> Result<GroupRef, Box<EvalAltResult>> {
    let (key, scope) = split_group_path(&path).map_err(|error| v2_error(error, Position::NONE))?;
    GroupRef::new(
        foundation::authoring_ref::<GroupKind>(&key, scope)
            .map_err(|error| v2_error(error, Position::NONE))?,
    )
    .map_err(|error| v2_error(error, Position::NONE))
}

fn define_group_v2(
    ctx: NativeCallContext,
    path: String,
    closure: FnPtr,
) -> Result<GroupRef, Box<EvalAltResult>> {
    let position = ctx.call_position();
    let syntax_path = path.bytes().map(u32::from).collect::<Vec<_>>();
    let builder = group_builder_v2(path)?;
    let contribution_key = vibelang_core::candidate::SyntaxKey::deterministic(
        builder.base.source().module(),
        &syntax_path,
        "define-group-body",
    )
    .map_err(|error| v2_error(error.into(), position))?;
    GroupBuilder::capture_body_with_key(ctx, builder, contribution_key, None, closure)?
        .apply()
        .map_err(|error| v2_error(error, position))
}

fn v2_error(error: FoundationError, position: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        error.to_string().into(),
        position,
    ))
}

#[cfg(test)]
fn install_v2_for_tests(engine: &mut Engine) {
    let error = |error| v2_error(error, Position::NONE);
    engine
        .register_type_with_name::<GroupBuilder>("GroupBuilder")
        .register_type_with_name::<GroupRef>("GroupRef")
        .register_fn("group_builder", group_builder_v2)
        .register_fn("group_ref", group_ref_v2)
        .register_fn("group", group_ref_v2)
        .register_fn("define_group", define_group_v2)
        .register_fn("gain", GroupBuilder::gain)
        .register_fn("muted", GroupBuilder::muted)
        .register_fn("soloed", GroupBuilder::soloed)
        .register_fn("parent", |builder: GroupBuilder, parent: GroupRef| {
            builder
                .parent(parent)
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn(
            "set_param",
            move |builder: GroupBuilder, name: String, value: f64| {
                builder.set_param(name, value).map_err(error)
            },
        )
        .register_fn(
            "output",
            |builder: GroupBuilder, bus: i64, channels: i64| {
                builder
                    .output(bus, channels)
                    .map_err(|error| v2_error(error, Position::NONE))
            },
        )
        .register_fn("body", GroupBuilder::body)
        .register_fn("body", GroupBuilder::body_ordered)
        .register_fn("apply", |builder: GroupBuilder| {
            builder
                .apply()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("mute", |reference: GroupRef| {
            reference
                .mute()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("unmute", |reference: GroupRef| {
            reference
                .unmute()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("solo", |reference: GroupRef, enabled: bool| {
            reference
                .solo(enabled)
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("remove", |reference: GroupRef| {
            reference
                .remove()
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("remove_contribution", |reference: GroupRef, id: String| {
            reference
                .remove_contribution(id)
                .map_err(|error| v2_error(error, Position::NONE))
        })
        .register_fn("status", |reference: GroupRef| {
            reference
                .status()
                .map_err(|error| v2_error(error, Position::NONE))
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v2_identity() -> vibelang_core::candidate::EvaluationIdentity {
        vibelang_core::candidate::EvaluationIdentity::new(
            vibelang_core::candidate::LanguageContract::v2(
                vibelang_core::candidate::ContractDigest::from_bytes(b"group-v2-test"),
            ),
            vibelang_core::candidate::EngineInstanceId::new(),
            vibelang_core::mutation::RuntimeEpoch::new(),
        )
    }

    fn v2_voice_fragment(name: &str, contribution: &ContributionId) -> CandidateFragment {
        let builder = crate::api::voice::VoiceBuilder::new(
            foundation::authoring_builder::<vibelang_core::candidate::VoiceKind>(
                name,
                GroupScope::root(),
            )
            .unwrap(),
        )
        .synth("sine".into())
        .unwrap();
        foundation::capture_fragment(DeclarationOwner::Contribution(contribution.clone()), || {
            builder.apply()
        })
        .unwrap()
        .1
    }

    #[test]
    fn v2_group_factories_configuration_and_lookup_alias_are_pure() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        install_v2_for_tests(&mut engine);
        let alias = engine.eval::<GroupRef>(r#"group("band")"#).unwrap();
        let canonical = engine.eval::<GroupRef>(r#"group_ref("band")"#).unwrap();
        let configured = group_builder_v2("band".into())
            .unwrap()
            .gain(0.5)
            .muted(true)
            .output(2, 2)
            .unwrap();
        let nested = group_builder_v2("band/drums".into()).unwrap();

        assert_eq!(alias, canonical);
        assert_eq!(configured.gain, 0.5);
        assert_eq!(
            nested
                .parent
                .as_ref()
                .unwrap()
                .base()
                .address()
                .key()
                .as_str(),
            "band"
        );
        assert_eq!(nested.base.address().group_scope().to_string(), "band");
        let candidate = foundation::finish_evaluation().unwrap();
        assert!(candidate.declarations().is_empty());
        assert!(candidate.operations().is_empty());
    }

    #[test]
    fn v2_group_paths_reject_noncanonical_empty_components() {
        for path in ["", "/band", "band/", "band//drums"] {
            assert!(matches!(
                split_group_path(path),
                Err(FoundationError::Candidate(CandidateError::InvalidAddress(
                    _
                )))
            ));
        }
    }

    #[test]
    fn v2_define_group_sugar_assigns_distinct_stable_body_contributions() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        install_v2_for_tests(&mut engine);
        engine
            .eval::<GroupRef>(
                r#"
                    define_group("band", || {});
                    define_group("drums", || {})
                "#,
            )
            .unwrap();

        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.declarations().len(), 2);
        assert_eq!(candidate.contributions().len(), 2);
        assert_ne!(
            candidate.contributions()[0].id(),
            candidate.contributions()[1].id()
        );
        assert_ne!(
            candidate.contributions()[0].target_group().address(),
            candidate.contributions()[1].target_group().address()
        );
    }

    #[test]
    fn v2_group_contributions_are_stable_owned_ordered_and_removable() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let module = vibelang_core::candidate::ModulePath::new("main").unwrap();
        let alpha = ContributionId::new(
            module.clone(),
            vibelang_core::candidate::SyntaxKey::Explicit(
                vibelang_core::candidate::DeclarationKey::new("alpha").unwrap(),
            ),
        );
        let beta = ContributionId::new(
            module,
            vibelang_core::candidate::SyntaxKey::Explicit(
                vibelang_core::candidate::DeclarationKey::new("beta").unwrap(),
            ),
        );
        let builder = GroupBuilder::new(
            foundation::authoring_builder::<GroupKind>("band", GroupScope::root()).unwrap(),
        )
        .push_contribution("alpha".into(), Some(10), v2_voice_fragment("lead", &alpha))
        .unwrap()
        .push_contribution("beta".into(), Some(5), v2_voice_fragment("bass", &beta))
        .unwrap();
        let group = builder.apply().unwrap();

        assert!(matches!(
            group.status(),
            Err(FoundationError::ObservationUnavailable)
        ));
        group.remove_contribution("beta".into()).unwrap();
        let candidate = foundation::finish_evaluation().unwrap();

        assert_eq!(candidate.contributions().len(), 2);
        assert_eq!(candidate.contributions()[0].id(), &beta);
        assert_eq!(candidate.contributions()[1].id(), &alpha);
        assert!(candidate
            .contributions()
            .iter()
            .all(|contribution| contribution.owned_declarations().len() == 1));
        assert!(candidate.declarations().iter().any(|declaration| {
            matches!(declaration.owner(), DeclarationOwner::Contribution(id) if id == &alpha)
        }));
        assert!(matches!(
            candidate.operations()[0].action(),
            LifecycleAction::RemoveContribution(id) if id == &beta
        ));
    }

    #[test]
    fn v2_duplicate_contribution_rejects_without_candidate_residue() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let module = vibelang_core::candidate::ModulePath::new("main").unwrap();
        let id = ContributionId::new(
            module,
            vibelang_core::candidate::SyntaxKey::Explicit(
                vibelang_core::candidate::DeclarationKey::new("body").unwrap(),
            ),
        );
        let fragment = v2_voice_fragment("lead", &id);
        let builder = GroupBuilder::new(
            foundation::authoring_builder::<GroupKind>("band", GroupScope::root()).unwrap(),
        )
        .push_contribution("body".into(), None, fragment.clone())
        .unwrap();

        assert!(matches!(
            builder.push_contribution("body".into(), None, fragment),
            Err(FoundationError::Candidate(
                CandidateError::DuplicateContribution(_)
            ))
        ));
        let candidate = foundation::finish_evaluation().unwrap();
        assert!(candidate.declarations().is_empty());
        assert!(candidate.contributions().is_empty());
    }

    // ==================== GroupHandle Constructor Tests ====================

    #[test]
    fn test_group_handle_new_simple() {
        let handle = GroupHandle::new("main/drums".to_string());
        assert_eq!(handle.path, "main/drums");
        assert_eq!(handle.name, "drums");
    }

    #[test]
    fn test_group_handle_new_nested() {
        let handle = GroupHandle::new("main/drums/kicks".to_string());
        assert_eq!(handle.path, "main/drums/kicks");
        assert_eq!(handle.name, "kicks");
    }

    #[test]
    fn test_group_handle_new_single() {
        let handle = GroupHandle::new("main".to_string());
        assert_eq!(handle.path, "main");
        assert_eq!(handle.name, "main");
    }

    #[test]
    fn test_group_handle_name_extraction() {
        // Name should be the last segment of the path
        let h1 = GroupHandle::new("a/b/c/d".to_string());
        assert_eq!(h1.name, "d");

        let h2 = GroupHandle::new("single".to_string());
        assert_eq!(h2.name, "single");
    }

    // ==================== Getter Tests ====================

    #[test]
    fn test_group_handle_get_name() {
        let mut handle = GroupHandle::new("main/synths".to_string());
        assert_eq!(handle.get_name(), "synths");
    }

    #[test]
    fn test_group_handle_parent() {
        let mut handle = GroupHandle::new("main/drums/kicks".to_string());
        assert_eq!(handle.parent(), "main/drums");
    }

    #[test]
    fn test_group_handle_parent_top_level() {
        let mut handle = GroupHandle::new("main/drums".to_string());
        assert_eq!(handle.parent(), "main");
    }

    #[test]
    fn test_group_handle_parent_no_parent() {
        let mut handle = GroupHandle::new("main".to_string());
        assert_eq!(handle.parent(), "");
    }

    #[test]
    fn test_group_handle_parent_root() {
        let mut handle = GroupHandle::new("drums".to_string());
        assert_eq!(handle.parent(), "");
    }

    // ==================== Group Path Tests ====================

    #[test]
    fn test_group_path_formats() {
        // Various path formats should be handled correctly
        let h1 = GroupHandle::new("main".to_string());
        assert_eq!(h1.path, "main");

        let h2 = GroupHandle::new("main/".to_string());
        assert_eq!(h2.name, ""); // Empty name from trailing slash

        let h3 = GroupHandle::new("main/a/b/c".to_string());
        assert_eq!(h3.name, "c");
    }

    // ==================== output() Tests ====================

    fn with_test_context<F: FnOnce()>(f: F) {
        context::init_context();
        f();
        context::clear_context();
    }

    fn read_output(path: &str) -> (Option<u32>, Option<u32>) {
        let group_id = context::get_or_create_group_id(path);
        context::with_state(|state| {
            state
                .groups
                .get(&group_id)
                .map(|c| (c.output_bus, c.output_channels))
                .unwrap_or((None, None))
        })
    }

    #[test]
    fn test_group_output_int_mono() {
        with_test_context(|| {
            let h = GroupHandle::new("main/cv1".to_string());
            let _ = h.clone().output_mono(2);
            assert_eq!(read_output("main/cv1"), (Some(2), Some(1)));
        });
    }

    #[test]
    fn test_group_output_array_one_element_mono() {
        with_test_context(|| {
            let h = GroupHandle::new("main/cv2".to_string());
            let arr: Array = vec![rhai::Dynamic::from(2_i64)];
            let _ = h.clone().output(arr);
            assert_eq!(read_output("main/cv2"), (Some(2), Some(1)));
        });
    }

    #[test]
    fn test_group_output_array_pair_stereo() {
        with_test_context(|| {
            let h = GroupHandle::new("main/leads".to_string());
            let arr: Array = vec![rhai::Dynamic::from(2_i64), rhai::Dynamic::from(3_i64)];
            let _ = h.clone().output(arr);
            assert_eq!(read_output("main/leads"), (Some(2), Some(2)));
        });
    }

    #[test]
    fn test_group_output_rejects_three_elements() {
        with_test_context(|| {
            let h = GroupHandle::new("main/bad3".to_string());
            // Touch the group so it exists with default routing.
            let _ = context::get_or_create_group_id("main/bad3");
            let arr: Array = vec![
                rhai::Dynamic::from(1_i64),
                rhai::Dynamic::from(2_i64),
                rhai::Dynamic::from(3_i64),
            ];
            let _ = h.clone().output(arr);
            // Unchanged: still no routing applied.
            assert_eq!(read_output("main/bad3"), (None, None));
        });
    }

    #[test]
    fn test_group_output_rejects_zero_elements() {
        with_test_context(|| {
            let h = GroupHandle::new("main/bad0".to_string());
            let _ = context::get_or_create_group_id("main/bad0");
            let arr: Array = vec![];
            let _ = h.clone().output(arr);
            assert_eq!(read_output("main/bad0"), (None, None));
        });
    }

    #[test]
    fn test_group_output_negative_bus_rejected() {
        with_test_context(|| {
            let h = GroupHandle::new("main/neg".to_string());
            let _ = context::get_or_create_group_id("main/neg");
            let _ = h.clone().output_mono(-1);
            assert_eq!(read_output("main/neg"), (None, None));
        });
    }

    #[test]
    fn test_group_output_out_of_range_int_rejected() {
        with_test_context(|| {
            let h = GroupHandle::new("main/big".to_string());
            let _ = context::get_or_create_group_id("main/big");
            let _ = h.clone().output_mono(16);
            assert_eq!(read_output("main/big"), (None, None));
        });
    }

    #[test]
    fn test_group_output_array_one_element_negative_rejected() {
        with_test_context(|| {
            let h = GroupHandle::new("main/neg_arr".to_string());
            let _ = context::get_or_create_group_id("main/neg_arr");
            let arr: Array = vec![rhai::Dynamic::from(-1_i64)];
            let _ = h.clone().output(arr);
            assert_eq!(read_output("main/neg_arr"), (None, None));
        });
    }
}
