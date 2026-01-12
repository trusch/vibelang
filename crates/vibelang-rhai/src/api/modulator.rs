//! Modulator API for Rhai scripts.
//!
//! Modulators are control-rate synthdefs that output to control buses.
//! They can be connected to voice parameters for dynamic modulation.

use rhai::{CustomType, Engine, NativeCallContext, TypeBuilder};
use std::collections::HashMap;
use vibelang_core2::traits::ModulatorConfig;
use vibelang_core2::types::ModulatorId;

use crate::context;

/// A Modulator builder for creating and configuring modulators.
///
/// Modulators are control-rate signal generators (LFOs, envelopes, followers)
/// that output to control buses. These can be connected to voice parameters
/// for dynamic modulation.
///
/// Nested modulation is supported: modulator parameters can be modulated by
/// other modulators using the `.modulate()` method.
#[derive(Debug, Clone, CustomType)]
pub struct Modulator {
    /// Modulator name (unique identifier).
    pub name: String,
    /// SynthDef name (the modulator synthdef to use).
    pub(crate) synthdef: String,
    /// Parameter values for the modulator synth.
    pub(crate) params: HashMap<String, f32>,
    /// Modulation mappings (param_name -> source modulator_id).
    pub(crate) modulations: HashMap<String, ModulatorId>,
}

impl Modulator {
    /// Create a new modulator with the given name.
    ///
    /// Use `.synth()` to set the synthdef, e.g.:
    /// ```rhai
    /// let lfo = modulator("my_lfo")
    ///     .synth("lfo_sine")
    ///     .param("rate", 4.0)
    ///     .apply();
    /// ```
    pub fn new(_ctx: NativeCallContext, name: String) -> Self {
        Self {
            name,
            synthdef: String::new(),
            params: HashMap::new(),
            modulations: HashMap::new(),
        }
    }

    // === Getters ===

    /// Get the modulator ID (name).
    pub fn id(&mut self) -> String {
        self.name.clone()
    }

    /// Get the modulator name.
    pub fn get_name(&mut self) -> String {
        self.name.clone()
    }

    /// Get the synthdef name.
    pub fn get_synthdef(&mut self) -> String {
        self.synthdef.clone()
    }

    // === Builder methods ===

    /// Set the synthdef for this modulator.
    ///
    /// The synthdef should be a control-rate synthdef (e.g., lfo_sine, lfo_saw).
    pub fn synth(mut self, synthdef: String) -> Self {
        self.synthdef = synthdef;
        self
    }

    /// Alias for synth() - set the synthdef.
    pub fn on(mut self, synthdef: String) -> Self {
        self.synthdef = synthdef;
        self
    }

    /// Set a parameter on the modulator (chainable).
    ///
    /// Use for setting LFO rate, range, etc.
    pub fn param(mut self, param: String, value: f64) -> Self {
        self.params.insert(param, value as f32);
        self
    }

    /// Alias for param() - for backwards compatibility.
    pub fn set_param(mut self, param: String, value: f64) -> Self {
        self.params.insert(param, value as f32);
        self
    }

    /// Set the modulator name explicitly (chainable).
    pub fn name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    /// Connect another modulator to a parameter of this modulator (nested modulation).
    ///
    /// This enables complex modulation routing, e.g., an LFO's rate controlled
    /// by another LFO.
    ///
    /// # Example
    ///
    /// ```rhai
    /// // Rate modulator - controls how fast the main LFO runs
    /// let rate_mod = modulator("rate_lfo")
    ///     .synth("lfo_sine")
    ///     .param("rate", 0.1)     // Very slow
    ///     .param("lo", 0.5)       // LFO rate min
    ///     .param("hi", 8.0)       // LFO rate max
    ///     .apply();
    ///
    /// // Main filter LFO with modulated rate
    /// let filter_lfo = modulator("filter_lfo")
    ///     .synth("lfo_sine")
    ///     .param("lo", 200.0)
    ///     .param("hi", 2000.0)
    ///     .modulate("rate", rate_mod)  // Rate controlled by rate_mod!
    ///     .apply();
    /// ```
    pub fn modulate(mut self, param: String, source: Modulator) -> Self {
        let source_id = context::get_or_create_modulator_id(&source.name);
        self.modulations.insert(param, source_id);
        self
    }

    /// Register this modulator with the script state (chainable).
    fn sync_to_state(&self) -> ModulatorId {
        let modulator_id = context::get_or_create_modulator_id(&self.name);

        let config = ModulatorConfig {
            name: self.name.clone(),
            synthdef: self.synthdef.clone(),
            params: self.params.clone(),
            modulations: self.modulations.clone(),
        };

        context::with_state(|state| {
            state.modulators.insert(modulator_id, config);
        });

        modulator_id
    }

    /// Apply the modulator configuration and return self for chaining.
    ///
    /// This registers the modulator with the runtime and allocates
    /// a control bus for its output.
    pub fn apply(self) -> Self {
        self.sync_to_state();
        self
    }
}

/// A handle representing a modulator that can be used for modulation routing.
///
/// This is returned by `modulator().apply()` and can be passed to
/// `voice.modulate(param, handle)` to connect the modulator's output
/// to a voice parameter.
#[derive(Debug, Clone, CustomType)]
pub struct ModulatorHandle {
    /// The modulator ID.
    pub id: ModulatorId,
    /// The modulator name.
    pub name: String,
}

impl ModulatorHandle {
    /// Get the modulator name.
    pub fn get_name(&mut self) -> String {
        self.name.clone()
    }
}

/// Create a new modulator builder with the given name.
///
/// Use `.synth()` to set the synthdef, e.g.:
/// ```rhai
/// let lfo = modulator("my_lfo")
///     .synth("lfo_sine")
///     .param("rate", 4.0)
///     .apply();
/// ```
pub fn modulator(ctx: NativeCallContext, name: String) -> Modulator {
    Modulator::new(ctx, name)
}

/// Create a new modulator builder with explicit name and synthdef.
pub fn modulator_named(ctx: NativeCallContext, name: String, synthdef: String) -> Modulator {
    let mut m = Modulator::new(ctx, name);
    m.synthdef = synthdef;
    m
}

/// Register modulator API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register Modulator type
    engine.build_type::<Modulator>();
    engine.build_type::<ModulatorHandle>();

    // Constructors
    engine.register_fn("modulator", modulator);
    engine.register_fn("modulator", modulator_named);

    // Getters
    engine.register_fn("id", Modulator::id);
    engine.register_fn("name", Modulator::get_name);
    engine.register_get("name", Modulator::get_name);
    engine.register_fn("synthdef", Modulator::get_synthdef);
    engine.register_get("synthdef", Modulator::get_synthdef);

    // Builder methods
    engine.register_fn("synth", Modulator::synth);
    engine.register_fn("on", Modulator::on);
    engine.register_fn("param", Modulator::param);
    engine.register_fn("set_param", Modulator::set_param);
    engine.register_fn("name", Modulator::name);
    engine.register_fn("modulate", Modulator::modulate);
    engine.register_fn("apply", Modulator::apply);

    // ModulatorHandle getters
    engine.register_fn("name", ModulatorHandle::get_name);
    engine.register_get("name", ModulatorHandle::get_name);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a Modulator for testing without NativeCallContext.
    /// Uses new API: modulator("name").synth("synthdef")
    fn test_modulator(name: &str, synthdef: &str) -> Modulator {
        Modulator {
            name: name.to_string(),
            synthdef: synthdef.to_string(),
            params: HashMap::new(),
            modulations: HashMap::new(),
        }
    }

    /// Legacy helper for tests that just need any modulator with a synthdef
    fn test_modulator_simple(synthdef: &str) -> Modulator {
        Modulator {
            name: format!("{}_test", synthdef),
            synthdef: synthdef.to_string(),
            params: HashMap::new(),
            modulations: HashMap::new(),
        }
    }

    #[test]
    fn test_modulator_new_api_style() {
        // Test the user's preferred API style:
        // modulator("my-wobble").synth("lfo_sine").param("lo", 100.).param("hi", 500.)
        let m = Modulator {
            name: "my-wobble".to_string(),
            synthdef: String::new(),
            params: HashMap::new(),
            modulations: HashMap::new(),
        }
        .synth("lfo_sine".to_string())
        .param("lo".to_string(), 100.0)
        .param("hi".to_string(), 500.0);

        assert_eq!(m.name, "my-wobble");
        assert_eq!(m.synthdef, "lfo_sine");
        assert_eq!(m.params.get("lo"), Some(&100.0_f32));
        assert_eq!(m.params.get("hi"), Some(&500.0_f32));
        assert!(m.modulations.is_empty());
    }

    #[test]
    fn test_modulator_synth_method() {
        let m = Modulator {
            name: "my_lfo".to_string(),
            synthdef: String::new(),
            params: HashMap::new(),
            modulations: HashMap::new(),
        }
        .synth("lfo_saw".to_string());

        assert_eq!(m.synthdef, "lfo_saw");
    }

    #[test]
    fn test_modulator_on_alias() {
        // .on() is an alias for .synth()
        let m = Modulator {
            name: "my_lfo".to_string(),
            synthdef: String::new(),
            params: HashMap::new(),
            modulations: HashMap::new(),
        }
        .on("lfo_tri".to_string());

        assert_eq!(m.synthdef, "lfo_tri");
    }

    #[test]
    fn test_modulator_param_method() {
        // Test the new .param() method
        let m = test_modulator("lfo", "lfo_sine")
            .param("rate".to_string(), 4.0)
            .param("lo".to_string(), 200.0);

        assert_eq!(m.params.get("rate"), Some(&4.0_f32));
        assert_eq!(m.params.get("lo"), Some(&200.0_f32));
    }

    #[test]
    fn test_modulator_set_param() {
        let m = test_modulator_simple("lfo_sine")
            .set_param("rate".to_string(), 4.0)
            .set_param("lo".to_string(), 200.0)
            .set_param("hi".to_string(), 2000.0);

        assert_eq!(m.params.get("rate"), Some(&4.0_f32));
        assert_eq!(m.params.get("lo"), Some(&200.0_f32));
        assert_eq!(m.params.get("hi"), Some(&2000.0_f32));
    }

    #[test]
    fn test_modulator_name() {
        let m = test_modulator_simple("lfo_sine").name("my_lfo".to_string());
        assert_eq!(m.name, "my_lfo");
    }

    #[test]
    fn test_modulator_getters() {
        let mut m = test_modulator_simple("envelope_follower");
        m.name = "my_follower".to_string();

        assert_eq!(m.id(), "my_follower");
        assert_eq!(m.get_name(), "my_follower");
        assert_eq!(m.get_synthdef(), "envelope_follower");
    }

    #[test]
    fn test_modulator_chained_builders() {
        let m = test_modulator_simple("lfo_sine")
            .name("cutoff_lfo".to_string())
            .set_param("rate".to_string(), 2.0)
            .set_param("lo".to_string(), 0.0)
            .set_param("hi".to_string(), 1.0);

        assert_eq!(m.name, "cutoff_lfo");
        assert_eq!(m.synthdef, "lfo_sine");
        assert_eq!(m.params.len(), 3);
    }

    #[test]
    fn test_modulator_param_override() {
        // Test that setting the same param twice uses the last value
        let m = test_modulator_simple("lfo_sine")
            .set_param("rate".to_string(), 1.0)
            .set_param("rate".to_string(), 4.0);

        assert_eq!(m.params.get("rate"), Some(&4.0_f32));
    }

    #[test]
    fn test_modulator_empty_params() {
        let m = test_modulator_simple("lfo_sine");
        assert!(m.params.is_empty());
        assert_eq!(m.synthdef, "lfo_sine");
    }

    #[test]
    fn test_modulator_float_precision() {
        // Test that f64 to f32 conversion works correctly
        let m = test_modulator_simple("lfo_sine")
            .set_param("rate".to_string(), 0.125) // Common rate for 8-bar cycle
            .set_param("lo".to_string(), 20.0)
            .set_param("hi".to_string(), 20000.0);

        assert!((m.params.get("rate").unwrap() - 0.125_f32).abs() < f32::EPSILON);
        assert_eq!(m.params.get("lo"), Some(&20.0_f32));
        assert_eq!(m.params.get("hi"), Some(&20000.0_f32));
    }

    #[test]
    fn test_modulator_negative_values() {
        // LFOs can output negative values (e.g., for bipolar modulation)
        let m = test_modulator_simple("lfo_sine")
            .set_param("lo".to_string(), -1.0)
            .set_param("hi".to_string(), 1.0);

        assert_eq!(m.params.get("lo"), Some(&-1.0_f32));
        assert_eq!(m.params.get("hi"), Some(&1.0_f32));
    }

    #[test]
    fn test_modulator_different_lfo_types() {
        // Test different LFO synthdef names
        let sine = test_modulator_simple("lfo_sine");
        let saw = test_modulator_simple("lfo_saw");
        let tri = test_modulator_simple("lfo_tri");
        let square = test_modulator_simple("lfo_square");
        let random = test_modulator_simple("lfo_random");

        assert_eq!(sine.synthdef, "lfo_sine");
        assert_eq!(saw.synthdef, "lfo_saw");
        assert_eq!(tri.synthdef, "lfo_tri");
        assert_eq!(square.synthdef, "lfo_square");
        assert_eq!(random.synthdef, "lfo_random");
    }

    #[test]
    fn test_modulator_envelope_follower_params() {
        let m = test_modulator_simple("envelope_follower")
            .set_param("attack".to_string(), 0.01)
            .set_param("release".to_string(), 0.1)
            .set_param("input_bus".to_string(), 16.0);

        assert_eq!(m.params.len(), 3);
        assert_eq!(m.params.get("attack"), Some(&0.01_f32));
        assert_eq!(m.params.get("release"), Some(&0.1_f32));
        assert_eq!(m.params.get("input_bus"), Some(&16.0_f32));
    }

    #[test]
    fn test_modulator_handle_getters() {
        let mut handle = ModulatorHandle {
            id: vibelang_core2::types::ModulatorId::new(42),
            name: "test_mod".to_string(),
        };

        assert_eq!(handle.get_name(), "test_mod");
    }

    #[test]
    fn test_modulator_name_chain_preserves_params() {
        // Ensure name() doesn't clear params
        let m = test_modulator_simple("lfo_sine")
            .set_param("rate".to_string(), 2.0)
            .name("renamed".to_string())
            .set_param("lo".to_string(), 0.0);

        assert_eq!(m.name, "renamed");
        assert_eq!(m.params.get("rate"), Some(&2.0_f32));
        assert_eq!(m.params.get("lo"), Some(&0.0_f32));
    }
}
