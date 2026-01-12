//! Group API for Rhai scripts.
//!
//! Groups organize voices and provide hierarchical mixing.

use rhai::{CustomType, Engine, FnPtr, NativeCallContext, TypeBuilder};
use std::collections::HashMap;
use vibelang_core2::reload::GroupConfig;

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
    pub fn parent(&mut self) -> String {
        if let Some(pos) = self.path.rfind('/') {
            self.path[..pos].to_string()
        } else {
            "main".to_string()
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
            state.groups.get(&group_id).map(|c| c.muted).unwrap_or(false)
        })
    }

    /// Check if the group is soloed.
    pub fn is_soloed(&mut self) -> bool {
        let group_id = context::get_or_create_group_id(&self.path);
        context::with_state(|state| {
            state.groups.get(&group_id).map(|c| c.soloed).unwrap_or(false)
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
            state.groups.get(&group_id).map(|c| c.effects.len() as i64).unwrap_or(0)
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
            state.effects.remove(&effect_id);
        });
        self
    }

    /// Clear all effects from this group.
    pub fn clear_effects(self) -> Self {
        let group_id = context::get_or_create_group_id(&self.path);

        context::with_state(|state| {
            if let Some(config) = state.groups.get_mut(&group_id) {
                // Remove all effect configs
                for effect_id in config.effects.drain(..) {
                    state.effects.remove(&effect_id);
                }
            }
        });
        self
    }
}

/// Define a group with a closure.
pub fn define_group(ctx: NativeCallContext, name: String, closure: FnPtr) -> GroupHandle {
    // Build the full path
    let parent_path = context::current_group_path();
    let full_path = if parent_path == "main" {
        format!("main/{}", name)
    } else {
        format!("{}/{}", parent_path, name)
    };

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
        name: name.clone(),
        parent: parent_id,
        params: HashMap::new(),
        effects: Vec::new(),
        muted: false,
        soloed: false,
    };

    // Add to script state
    context::with_state(|state| {
        state.groups.insert(group_id, config);
    });

    // Push group context for nested definitions
    context::push_group(&name);

    // Execute closure
    if let Err(e) = closure.call_within_context::<rhai::Dynamic>(&ctx, ()) {
        log::error!("Error in define_group '{}': {}", name, e);
    }

    // Pop group context
    context::pop_group();

    GroupHandle::new(full_path)
}

/// Get a group handle by path.
pub fn group(path: String) -> GroupHandle {
    let full_path = if path.starts_with("main/") || path == "main" {
        path
    } else {
        let current = context::current_group_path();
        format!("{}/{}", current, path)
    };
    GroupHandle::new(full_path)
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
        assert_eq!(handle.parent(), "main");
    }

    #[test]
    fn test_group_handle_parent_root() {
        let mut handle = GroupHandle::new("drums".to_string());
        assert_eq!(handle.parent(), "main");
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
}
