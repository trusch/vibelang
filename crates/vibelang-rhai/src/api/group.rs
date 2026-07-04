//! Group API for Rhai scripts.
//!
//! Groups organize voices and provide hierarchical mixing.

use rhai::{
    Array, CustomType, Engine, EvalAltResult, FnPtr, NativeCallContext, Position, TypeBuilder,
};
use std::collections::HashMap;
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

/// Define the group body with a closure (builder method).
///
/// This is the builder equivalent of `define_group("name", || { ... })`.
/// The closure executes in the group's context, allowing nested definitions.
fn group_body(ctx: NativeCallContext, handle: GroupHandle, closure: FnPtr) -> GroupHandle {
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
fn group_alias(
    ctx: NativeCallContext,
    handle: GroupHandle,
    alias: String,
) -> Result<GroupHandle, Box<EvalAltResult>> {
    context::add_group_alias(alias, handle.path.clone())
        .map_err(|err| alias_error(err, ctx.call_position()))?;
    Ok(handle)
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
    engine.register_fn("body", group_body);
    engine.register_fn("alias", group_alias);
    engine.register_fn("output", GroupHandle::output);
    engine.register_fn("output", GroupHandle::output_mono);

    // Effect management
    engine.register_fn("remove_effect", GroupHandle::remove_effect);
    engine.register_fn("clear_effects", GroupHandle::clear_effects);
}

#[cfg(test)]
mod tests {
    use super::*;

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
