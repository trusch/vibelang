//! Modulators trait for control-rate signal generators.
//!
//! Modulators are control-rate synths that output to control buses.
//! They can be connected to voice parameters for modulation (LFOs, envelopes, etc.).

use crate::types::{ModulatorId, ParamMap};
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Configuration for creating a modulator.
#[derive(Clone, Debug, PartialEq)]
pub struct ModulatorConfig {
    /// Modulator name (for display in TUI/API).
    pub name: String,

    /// Name of the synthdef to use (must be a control-rate output synthdef).
    pub synthdef: String,

    /// Parameter values for the modulator synth.
    pub params: ParamMap,

    /// Modulation mappings (param_name -> source modulator_id).
    ///
    /// When set, the specified parameters will be modulated by control buses
    /// written to by the corresponding source modulators. This enables nested
    /// modulation, e.g., an LFO's rate controlled by another LFO.
    pub modulations: HashMap<String, ModulatorId>,
}

impl ModulatorConfig {
    /// Create a new modulator configuration.
    pub fn new(name: impl Into<String>, synthdef: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            synthdef: synthdef.into(),
            params: ParamMap::new(),
            modulations: HashMap::new(),
        }
    }

    /// Add a parameter value.
    pub fn with_param(mut self, name: impl Into<String>, value: f32) -> Self {
        self.params.insert(name.into(), value);
        self
    }

    /// Add a modulation mapping (param modulated by source modulator).
    pub fn with_modulation(mut self, param: impl Into<String>, source: ModulatorId) -> Self {
        self.modulations.insert(param.into(), source);
        self
    }
}

/// Modulator management for control-rate signal generation.
///
/// Modulators output control-rate signals that can be routed to voice parameters.
/// This enables dynamic modulation of any voice parameter via LFOs, envelopes,
/// envelope followers, and custom control-rate synthdefs.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait Modulators: Send + Sync {
    /// Create a new modulator.
    ///
    /// This allocates a control bus and creates the modulator synth.
    async fn create(&self, id: ModulatorId, config: ModulatorConfig) -> Result<()>;

    /// Delete a modulator.
    ///
    /// Frees the modulator synth and releases the control bus.
    async fn delete(&self, id: ModulatorId) -> Result<()>;

    /// Set a parameter on a modulator.
    ///
    /// Updates the parameter on the running modulator synth.
    async fn set_param(&self, id: ModulatorId, param: &str, value: f32) -> Result<()>;
}

/// Modulator management for control-rate signal generation (WASM version).
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait Modulators {
    /// Create a new modulator.
    async fn create(&self, id: ModulatorId, config: ModulatorConfig) -> Result<()>;

    /// Delete a modulator.
    async fn delete(&self, id: ModulatorId) -> Result<()>;

    /// Set a parameter on a modulator.
    async fn set_param(&self, id: ModulatorId, param: &str, value: f32) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modulator_config_new() {
        let config = ModulatorConfig::new("my_lfo", "lfo_sine");

        assert_eq!(config.name, "my_lfo");
        assert_eq!(config.synthdef, "lfo_sine");
        assert!(config.params.is_empty());
        assert!(config.modulations.is_empty());
    }

    #[test]
    fn test_modulator_config_with_param() {
        let config = ModulatorConfig::new("my_lfo", "lfo_sine")
            .with_param("rate", 4.0)
            .with_param("lo", 200.0)
            .with_param("hi", 2000.0);

        assert_eq!(config.params.len(), 3);
        assert_eq!(config.params.get("rate"), Some(&4.0_f32));
        assert_eq!(config.params.get("lo"), Some(&200.0_f32));
        assert_eq!(config.params.get("hi"), Some(&2000.0_f32));
    }

    #[test]
    fn test_modulator_config_equality() {
        let config1 = ModulatorConfig::new("lfo", "lfo_sine").with_param("rate", 2.0);
        let config2 = ModulatorConfig::new("lfo", "lfo_sine").with_param("rate", 2.0);
        let config3 = ModulatorConfig::new("lfo", "lfo_sine").with_param("rate", 4.0);

        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }

    #[test]
    fn test_modulator_config_clone() {
        let config = ModulatorConfig::new("lfo", "lfo_sine").with_param("rate", 2.0);
        let cloned = config.clone();

        assert_eq!(config, cloned);
    }

    #[test]
    fn test_modulator_config_different_synthdefs() {
        let sine = ModulatorConfig::new("sine", "lfo_sine");
        let saw = ModulatorConfig::new("saw", "lfo_saw");
        let tri = ModulatorConfig::new("tri", "lfo_tri");
        let square = ModulatorConfig::new("square", "lfo_square");
        let random = ModulatorConfig::new("random", "lfo_random");
        let follower = ModulatorConfig::new("follower", "envelope_follower");

        assert_eq!(sine.synthdef, "lfo_sine");
        assert_eq!(saw.synthdef, "lfo_saw");
        assert_eq!(tri.synthdef, "lfo_tri");
        assert_eq!(square.synthdef, "lfo_square");
        assert_eq!(random.synthdef, "lfo_random");
        assert_eq!(follower.synthdef, "envelope_follower");
    }

    #[test]
    fn test_modulator_config_param_override() {
        let config = ModulatorConfig::new("lfo", "lfo_sine")
            .with_param("rate", 1.0)
            .with_param("rate", 4.0); // Override

        assert_eq!(config.params.get("rate"), Some(&4.0_f32));
    }

    #[test]
    fn test_modulator_config_negative_params() {
        // Bipolar modulation needs negative values
        let config = ModulatorConfig::new("lfo", "lfo_sine")
            .with_param("lo", -1.0)
            .with_param("hi", 1.0);

        assert_eq!(config.params.get("lo"), Some(&-1.0_f32));
        assert_eq!(config.params.get("hi"), Some(&1.0_f32));
    }

    #[test]
    fn test_modulator_config_with_modulation() {
        let source_id = ModulatorId::new(1);
        let config = ModulatorConfig::new("main_lfo", "lfo_sine")
            .with_param("lo", 200.0)
            .with_param("hi", 2000.0)
            .with_modulation("rate", source_id);

        assert_eq!(config.modulations.len(), 1);
        assert_eq!(config.modulations.get("rate"), Some(&source_id));
    }

    #[test]
    fn test_modulator_config_multiple_modulations() {
        let rate_mod = ModulatorId::new(1);
        let depth_mod = ModulatorId::new(2);
        let config = ModulatorConfig::new("main_lfo", "lfo_sine")
            .with_modulation("rate", rate_mod)
            .with_modulation("hi", depth_mod);

        assert_eq!(config.modulations.len(), 2);
        assert_eq!(config.modulations.get("rate"), Some(&rate_mod));
        assert_eq!(config.modulations.get("hi"), Some(&depth_mod));
    }

    #[test]
    fn test_modulator_config_equality_with_modulations() {
        let source_id = ModulatorId::new(1);
        let config1 = ModulatorConfig::new("lfo", "lfo_sine").with_modulation("rate", source_id);
        let config2 = ModulatorConfig::new("lfo", "lfo_sine").with_modulation("rate", source_id);
        let config3 = ModulatorConfig::new("lfo", "lfo_sine");

        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }
}
