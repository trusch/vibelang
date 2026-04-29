//! Modulator API for Rhai scripts.
//!
//! Multi-output v2 Story 5: modulators are now sugar over a single-kr-port
//! voice. `modulator("env").synth("lfo_sine").apply()` registers the
//! modulator's synthdef as a kr-output voice synthdef, inserts a
//! [`VoiceConfig`] for the modulator into the script state, and marks the
//! voice for auto-triggering (`running_voices`). `voice.modulate("amp", m)`
//! becomes sugar for `m.output("out").to_param(voice, "amp")` — installed
//! via [`ScriptState::add_param_route`].
//!
//! The user-facing API is unchanged: `modulator()`, `.synth()`, `.param()`,
//! `.modulate()`, `.apply()` all return the same handle types and behave
//! identically from the script's point of view. Internally the runtime sees
//! a kr-port voice and a CV-to-param route instead of the legacy
//! `state.modulators` + per-trigger `/n_map` pipeline.

use rhai::{CustomType, Engine, NativeCallContext, TypeBuilder};
use std::collections::HashMap;
use vibelang_core::traits::VoiceConfig;
use vibelang_core::types::{ModulatorId, ParamMap};
use vibelang_dsp::{register_synthdef_outputs, OutputPort, PortRate};

use crate::context;

/// Output port name written by the kr-port voice that backs every modulator.
///
/// The voice handler allocates a control bus for this port via the
/// [`vibelang_core::state::State::alloc_control_bus`] free list; CV-to-param
/// routes from this port (installed by `voice.modulate(...)` sugar) feed the
/// `/n_map` pipeline that drives target-voice parameters.
const MODULATOR_OUTPUT_PORT: &str = "out";

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
    ///
    /// Carries the legacy `ModulatorId` typing so direct field-level tests
    /// (e.g. `m.modulations.get("rate") == Some(&ModulatorId::new(...))`)
    /// keep passing. The Story 5 sync path consults
    /// [`Self::modulation_source_names`] for the param-route install since
    /// it needs the source's name, not the id.
    pub(crate) modulations: HashMap<String, ModulatorId>,
    /// Source modulator name per modulated param — populated by
    /// [`Modulator::modulate`] alongside [`Self::modulations`].
    ///
    /// Stored separately so [`Self::sync_to_state`] can install param-routes
    /// without a reverse lookup against the context's modulator-id map.
    pub(crate) modulation_source_names: HashMap<String, String>,
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
            modulation_source_names: HashMap::new(),
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
    /// by another LFO. Story 5 installs the route via [`ScriptState::add_param_route`]
    /// from the source's `out` port to this modulator's voice param at sync time.
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
        self.modulations.insert(param.clone(), source_id);
        self.modulation_source_names.insert(param, source.name);
        self
    }

    /// Register this modulator with the script state.
    ///
    /// Story 5 implementation: the modulator becomes a `VoiceConfig` whose
    /// synthdef is registered with a single kr `out` output port. Param-routes
    /// from any source modulators feed this voice's params. The voice is
    /// marked for auto-triggering so the modulator runs continuously, matching
    /// the legacy behaviour of `ModulatorsHandler::create`.
    ///
    /// Returns the modulator id (name-keyed; same value as a voice id derived
    /// from the same name) so callers that capture it for [`ModulatorHandle`]
    /// keep working unchanged.
    fn sync_to_state(&self) -> ModulatorId {
        let modulator_id = context::get_or_create_modulator_id(&self.name);

        // Without a synthdef the builder is incomplete — preserve the
        // chained-builder semantics by allocating the id but not committing
        // any voice / route state. A later `.synth(...).apply()` will run
        // sync_to_state again with the synthdef populated.
        if self.synthdef.is_empty() {
            return modulator_id;
        }

        // Declare the kr `out` port for this modulator's synthdef. Idempotent
        // overwrites are fine — the same shape every time.
        register_synthdef_outputs(
            self.synthdef.clone(),
            vec![OutputPort {
                name: MODULATOR_OUTPUT_PORT.to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        );

        let voice_id = context::get_or_create_voice_id(&self.name);
        let group_id = context::get_or_create_group_id("main");

        // Snapshot the source-name → param map under the same key shape as
        // self.modulations so `add_param_route` calls below stay aligned.
        let modulation_sources = self.modulation_source_names.clone();

        let mut params: ParamMap = self.params.clone();
        // The `amp` param has special meaning on regular voices (gain).
        // Modulators don't use it — leave it absent so the voice config
        // doesn't accidentally clamp or scale the kr output.
        params.remove("amp");

        let config = VoiceConfig {
            name: self.name.clone(),
            synthdef: self.synthdef.clone(),
            group: group_id,
            polyphony: 1,
            params,
            muted: false,
            soloed: false,
            sfz_instrument: None,
            sample_id: None,
            round_robin_count: 0,
            trigger_mode: "gate".to_string(),
            choke_group: None,
            modulations: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_output: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
            #[cfg(feature = "midi")]
            param_cc_map: HashMap::new(),
        };

        // Resolve source-voice ids outside `with_state` — `with_state` already
        // holds the context's `RefCell` borrow, and `get_or_create_voice_id`
        // re-borrows it, so doing this inline panics with "already borrowed".
        let resolved_routes: Vec<(_, String, String)> = modulation_sources
            .into_iter()
            .map(|(param, source_name)| {
                let source_voice_id = context::get_or_create_voice_id(&source_name);
                (source_voice_id, param, source_name)
            })
            .collect();

        context::with_state(|state| {
            state.voices.insert(voice_id, config);
            state.running_voices.insert(voice_id);

            for (source_voice_id, param, _) in &resolved_routes {
                state.add_param_route(
                    *source_voice_id,
                    MODULATOR_OUTPUT_PORT,
                    voice_id,
                    param.clone(),
                );
            }
        });

        modulator_id
    }

    /// Apply the modulator configuration and return self for chaining.
    ///
    /// This registers the modulator with the script state as a kr-port voice
    /// and installs any pending nested-modulation param-routes.
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
            modulation_source_names: HashMap::new(),
        }
    }

    /// Legacy helper for tests that just need any modulator with a synthdef
    fn test_modulator_simple(synthdef: &str) -> Modulator {
        Modulator {
            name: format!("{}_test", synthdef),
            synthdef: synthdef.to_string(),
            params: HashMap::new(),
            modulations: HashMap::new(),
            modulation_source_names: HashMap::new(),
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
            modulation_source_names: HashMap::new(),
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
            modulation_source_names: HashMap::new(),
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
            modulation_source_names: HashMap::new(),
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
            id: vibelang_core::types::ModulatorId::new(42),
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

    // === Story 5 sync-to-state tests ===

    /// Initialize a script context for testing, run the closure, then clean up.
    fn with_test_context<F: FnOnce()>(f: F) {
        context::init_context();
        f();
        context::clear_context();
    }

    #[test]
    fn test_apply_installs_running_voice_with_kr_port() {
        with_test_context(|| {
            let m = test_modulator("env", "story5_lfo_sine_voice").apply();
            assert_eq!(m.name, "env");

            let voice_id = context::get_or_create_voice_id("env");
            context::with_state(|state| {
                let voice = state
                    .voices
                    .get(&voice_id)
                    .expect("modulator installs a voice config");
                assert_eq!(voice.synthdef, "story5_lfo_sine_voice");
                assert_eq!(voice.polyphony, 1);
                assert!(state.running_voices.contains(&voice_id));
            });

            // The kr `out` port is registered against the synthdef so
            // downstream `voice.output("out").to_param(...)` resolves.
            let ports = vibelang_dsp::get_synthdef_outputs("story5_lfo_sine_voice")
                .expect("kr port descriptor registered");
            assert_eq!(ports.len(), 1);
            assert_eq!(ports[0].name, MODULATOR_OUTPUT_PORT);
            assert_eq!(ports[0].rate, PortRate::Kr);
        });
    }

    #[test]
    fn test_apply_with_params_copies_to_voice_config() {
        with_test_context(|| {
            test_modulator("rate_lfo", "story5_lfo_sine_params")
                .param("rate".to_string(), 4.0)
                .param("lo".to_string(), 200.0)
                .param("hi".to_string(), 2000.0)
                .apply();

            let voice_id = context::get_or_create_voice_id("rate_lfo");
            context::with_state(|state| {
                let voice = state.voices.get(&voice_id).expect("voice installed");
                assert_eq!(voice.params.get("rate"), Some(&4.0_f32));
                assert_eq!(voice.params.get("lo"), Some(&200.0_f32));
                assert_eq!(voice.params.get("hi"), Some(&2000.0_f32));
            });
        });
    }

    #[test]
    fn test_apply_without_synthdef_skips_voice_install() {
        with_test_context(|| {
            // Modulator with no `.synth(...)` call: builder is incomplete
            // and must not commit any voice / route state.
            let m = Modulator {
                name: "incomplete".to_string(),
                synthdef: String::new(),
                params: HashMap::new(),
                modulations: HashMap::new(),
                modulation_source_names: HashMap::new(),
            }
            .apply();
            assert_eq!(m.name, "incomplete");

            let voice_id = context::get_or_create_voice_id("incomplete");
            context::with_state(|state| {
                assert!(state.voices.get(&voice_id).is_none());
                assert!(!state.running_voices.contains(&voice_id));
            });
        });
    }

    #[test]
    fn test_nested_modulate_installs_param_route() {
        with_test_context(|| {
            // Source modulator first.
            let rate_mod = test_modulator("rate_mod", "story5_lfo_sine_nested")
                .param("rate".to_string(), 0.1)
                .apply();

            // Main modulator that depends on rate_mod for its `rate` param.
            test_modulator("filter_lfo", "story5_lfo_sine_nested")
                .param("lo".to_string(), 200.0)
                .param("hi".to_string(), 2000.0)
                .modulate("rate".to_string(), rate_mod)
                .apply();

            let source_voice_id = context::get_or_create_voice_id("rate_mod");
            let target_voice_id = context::get_or_create_voice_id("filter_lfo");
            context::with_state(|state| {
                let entries = state
                    .param_routes
                    .get(&(source_voice_id, MODULATOR_OUTPUT_PORT.to_string()))
                    .expect("nested modulation installs param route");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0], (target_voice_id, "rate".to_string()));
            });
        });
    }
}
