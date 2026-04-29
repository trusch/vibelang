//! Route API for Rhai scripts.
//!
//! `RouteHandle` is the chainable builder produced by
//! [`Voice::output(name|idx)`](super::voice::Voice). It carries the resolved
//! `(voice_id, port_name)` and exposes the three terminal verbs
//! (`to`, `to_main`, `mute`) that install a [`RouteDest`] into the
//! [`ScriptState`](vibelang_core::reload::ScriptState) routes map.
//!
//! Re-routing the same `(voice_id, port_name)` overwrites the prior dest
//! (`HashMap::insert` semantics on
//! [`ScriptState::set_route`](vibelang_core::reload::ScriptState::set_route)).
//! Additive fan-out is deferred to a later story.

use rhai::{CustomType, Engine, EvalAltResult, Position, TypeBuilder};
use vibelang_core::handlers::RouteDest;
use vibelang_core::types::VoiceId;
use vibelang_dsp::{
    get_synthdef_outputs, get_synthdef_param_defaults, OutputPort, PortRate,
};

use super::group::GroupHandle;
use super::voice::Voice;
use crate::context;

/// Chainable handle for installing a route on a voice's named output port.
///
/// Constructed by [`Voice::output`](super::voice::Voice). Terminal verbs
/// consume the handle to commit the route to the script state.
#[derive(Debug, Clone, CustomType)]
pub struct RouteHandle {
    /// Resolved voice ID — the source voice for this route.
    voice_id: VoiceId,
    /// Resolved port name — already validated against the synthdef's
    /// declared `OutputPort` set when the handle was created.
    port_name: String,
}

impl RouteHandle {
    pub(crate) fn new(voice_id: VoiceId, port_name: String) -> Self {
        Self {
            voice_id,
            port_name,
        }
    }

    /// Install a route to the given group's mix bus.
    pub fn to(self, group: GroupHandle) -> Self {
        let group_id = context::get_or_create_group_id(&group.path);
        self.commit(RouteDest::Group(group_id));
        self
    }

    /// Install a route straight to the main hardware output (bus 0), bypassing
    /// any group routing.
    pub fn to_main(self) -> Self {
        self.commit(RouteDest::Main);
        self
    }

    /// Install a muted route — the port's signal is discarded.
    pub fn mute(self) -> Self {
        context::with_state(|state| {
            state.set_route(self.voice_id, self.port_name.clone(), RouteDest::Muted);
        });
        self
    }

    /// Install a route to the voice's currently configured group — the value
    /// last set via `voice.group(...)` (or inferred from the surrounding
    /// `define_group` scope at voice-creation time).
    ///
    /// Errors when the voice is in the implicit `main` group with no explicit
    /// group set; the message points users at `.group("name")` on the voice or
    /// `.to(group("name"))` on the route. Re-routing replaces the prior dest
    /// (same `HashMap::insert` semantics as the other terminal verbs).
    pub fn to_current_group(self) -> Result<Self, Box<EvalAltResult>> {
        let main_id = context::get_or_create_group_id("main");
        let voice_group = context::with_state(|state| {
            state.voices.get(&self.voice_id).map(|v| v.group)
        });
        match voice_group {
            Some(gid) if gid != main_id => {
                self.commit(RouteDest::Group(gid));
                Ok(self)
            }
            _ => Err(no_current_group_error(&self.port_name)),
        }
    }

    /// Install a CV-to-param route from this kr-rate output port to the named
    /// parameter on `target`.
    ///
    /// Multi-output v2 Story 4. The source port must be control-rate
    /// (`output_kr`); audio-rate ports are rejected with a clean error citing
    /// the port's actual rate. The target parameter must exist on the target
    /// voice's synthdef; unknown params produce a clean error citing the
    /// synthdef's declared param set so a typo is fixable without scrolling
    /// back to the synthdef definition.
    ///
    /// Additive across calls — multiple `.to_param(...)` calls from the same
    /// `(voice, port)` to *different* `(target, param)` pairs all install
    /// (mirroring [`ScriptState::add_param_route`](vibelang_core::reload::ScriptState::add_param_route)).
    /// A duplicate `(target, param)` is deduplicated.
    ///
    /// Distinct from the audio terminal verbs (`to`, `to_main`, `mute`,
    /// `to_current_group`): those install [`RouteDest`] entries into
    /// [`ScriptState::routes`](vibelang_core::reload::ScriptState::routes).
    /// `.to_param(...)` instead populates
    /// [`ScriptState::param_routes`](vibelang_core::reload::ScriptState::param_routes),
    /// which feeds [`RoutesHandler::finalize_params`](vibelang_core::handlers::RoutesHandler::finalize_params)'s
    /// `/n_map` pipeline at reload time.
    pub fn to_param(
        self,
        target: Voice,
        param_name: String,
    ) -> Result<Self, Box<EvalAltResult>> {
        let mut target = target;
        target.resolve_name();
        let target_name = target.name.clone();
        let target_synth = target.get_synth_name();

        let src_synth = source_synthdef_name(self.voice_id);
        let src_outputs = get_synthdef_outputs(&src_synth).unwrap_or_default();
        match src_outputs.iter().find(|p| p.name == self.port_name) {
            Some(p) if p.rate == PortRate::Kr => {}
            Some(p) => {
                return Err(ar_rate_to_param_error(
                    &self.port_name,
                    &src_synth,
                    p.rate,
                ));
            }
            None => {
                return Err(missing_source_port_error(
                    &self.port_name,
                    &src_synth,
                    &src_outputs,
                ));
            }
        }

        let target_params = get_synthdef_param_defaults(&target_synth);
        if !target_params.contains_key(&param_name) {
            return Err(unknown_target_param_error(
                &target_name,
                &target_synth,
                &param_name,
                &target_params,
            ));
        }

        let target_id = context::get_or_create_voice_id(&target_name);
        context::with_state(|state| {
            state.add_param_route(
                self.voice_id,
                self.port_name.clone(),
                target_id,
                param_name.clone(),
            );
        });
        Ok(self)
    }

    /// Commit the resolved destination to script state.
    fn commit(&self, dest: RouteDest) {
        context::with_state(|state| {
            state.set_route(self.voice_id, self.port_name.clone(), dest);
        });
    }
}

/// Look up the source voice's synthdef name from the script state.
///
/// The voice was sync_to_state'd by the `voice("...").synth(...)` builder
/// chain before any [`RouteHandle`] was constructed, so a missing entry here
/// means a programming error in the caller (e.g. calling `.to_param` on a
/// voice that was never registered). Returns the empty string in that case so
/// downstream lookups (`get_synthdef_outputs`, `get_synthdef_param_defaults`)
/// surface a clean "no port / no param" error rather than a panic.
fn source_synthdef_name(voice_id: VoiceId) -> String {
    context::with_state(|state| {
        state
            .voices
            .get(&voice_id)
            .map(|v| v.synthdef.clone())
            .unwrap_or_default()
    })
}

fn ar_rate_to_param_error(
    port: &str,
    synth: &str,
    rate: PortRate,
) -> Box<EvalAltResult> {
    let rate_str = match rate {
        PortRate::Ar => "ar",
        PortRate::Kr => "kr",
    };
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "to_param() on port '{}' (synthdef '{}'): port is {}-rate, but \
             .to_param() requires a kr-rate (control) port — declare the \
             port with .output_kr(...) on the synthdef, or use .to(group(...)) \
             / .to_main() for audio-rate routing",
            port, synth, rate_str
        )
        .into(),
        Position::NONE,
    ))
}

fn missing_source_port_error(
    port: &str,
    synth: &str,
    outputs: &[OutputPort],
) -> Box<EvalAltResult> {
    let available = if outputs.is_empty() {
        "<none>".to_string()
    } else {
        outputs
            .iter()
            .map(|p| format!("'{}'", p.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "to_param(): source port '{}' not found on synthdef '{}' \
             (available: {})",
            port, synth, available
        )
        .into(),
        Position::NONE,
    ))
}

fn unknown_target_param_error(
    target_voice: &str,
    target_synth: &str,
    param: &str,
    available: &std::collections::HashMap<String, f32>,
) -> Box<EvalAltResult> {
    let avail_list = if available.is_empty() {
        "<none>".to_string()
    } else {
        let mut names: Vec<&str> = available.keys().map(String::as_str).collect();
        names.sort();
        names
            .iter()
            .map(|n| format!("'{}'", n))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "to_param(): target voice '{}' synthdef '{}' has no param '{}' \
             (available: {})",
            target_voice, target_synth, param, avail_list
        )
        .into(),
        Position::NONE,
    ))
}

fn no_current_group_error(port: &str) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "to_current_group() on port '{}': voice has no explicit group set — \
             call `.group(\"name\")` on the voice first, or write \
             `.to(group(\"name\"))` to target a group explicitly",
            port
        )
        .into(),
        Position::NONE,
    ))
}

/// Plural sugar for [`RouteHandle`] — fan-out a terminal verb across a list
/// of `(voice_id, port_name)` pairs produced by
/// [`Voice::outputs`](super::voice::Voice).
///
/// Each terminal verb iterates the inner port list and installs the same
/// destination per port. Replace semantics are inherited from the singular
/// form (last call to `set_route` for a given `(voice, port)` wins).
#[derive(Debug, Clone, CustomType)]
pub struct MultiRouteHandle {
    routes: Vec<RouteHandle>,
}

impl MultiRouteHandle {
    pub(crate) fn new(routes: Vec<RouteHandle>) -> Self {
        Self { routes }
    }

    /// Install a route to the given group's mix bus for every listed port.
    pub fn to(self, group: GroupHandle) -> Self {
        for handle in self.routes.iter() {
            handle.clone().to(group.clone());
        }
        self
    }

    /// Route every listed port straight to the main hardware output.
    pub fn to_main(self) -> Self {
        for handle in self.routes.iter() {
            handle.clone().to_main();
        }
        self
    }

    /// Mute every listed port.
    pub fn mute(self) -> Self {
        for handle in self.routes.iter() {
            handle.clone().mute();
        }
        self
    }

    /// Route every listed port to the voice's currently configured group.
    ///
    /// Errors via the same path as the singular form when the voice has no
    /// explicit group; the first failing port short-circuits the iteration
    /// (so a partial fan-out won't be silently committed before the error).
    pub fn to_current_group(self) -> Result<Self, Box<EvalAltResult>> {
        for handle in self.routes.iter() {
            handle.clone().to_current_group()?;
        }
        Ok(self)
    }

    /// Install a CV-to-param route from every listed kr-rate port to the same
    /// `(target, param)` pair.
    ///
    /// Per-port validation mirrors [`RouteHandle::to_param`]: the first ar-rate
    /// port or unknown target param short-circuits the fan-out so a partial
    /// install isn't silently committed before the error.
    pub fn to_param(
        self,
        target: Voice,
        param_name: String,
    ) -> Result<Self, Box<EvalAltResult>> {
        for handle in self.routes.iter() {
            handle.clone().to_param(target.clone(), param_name.clone())?;
        }
        Ok(self)
    }
}

/// Register the `RouteHandle` type and its terminal verbs with a Rhai engine.
pub fn register(engine: &mut Engine) {
    engine.build_type::<RouteHandle>();
    engine.register_fn("to", RouteHandle::to);
    engine.register_fn("to_main", RouteHandle::to_main);
    engine.register_fn("mute", RouteHandle::mute);
    engine.register_fn("to_current_group", RouteHandle::to_current_group);
    engine.register_fn("to_param", RouteHandle::to_param);

    engine.build_type::<MultiRouteHandle>();
    engine.register_fn("to", MultiRouteHandle::to);
    engine.register_fn("to_main", MultiRouteHandle::to_main);
    engine.register_fn("mute", MultiRouteHandle::mute);
    engine.register_fn("to_current_group", MultiRouteHandle::to_current_group);
    engine.register_fn("to_param", MultiRouteHandle::to_param);
}

#[cfg(test)]
mod tests {
    //! `.to_param(...)` Rhai surface tests.
    //!
    //! The vibelang-dsp synthdef-output and synthdef-IR registries are
    //! process-wide singletons, so each test runs in a uniquely-named
    //! synth/voice scope to stay isolated under cargo's parallel runner.
    //! Assertions touch `state.param_routes` directly — the canonical
    //! installation surface for CV-to-param routes (mirrors
    //! `RouteDest::Param` semantics: `(target_voice_id, target_param_name)`
    //! pairs keyed by source `(voice_id, port_name)`).
    use crate::api::voice::Voice;
    use crate::context;
    use vibelang_dsp::{
        register_synthdef_ir, register_synthdef_outputs, GraphIR, OutputPort, ParamSpec, PortRate,
    };

    fn with_test_context<F: FnOnce()>(f: F) {
        context::init_context();
        f();
        context::clear_context();
    }

    fn make_voice(name: &str) -> Voice {
        Voice::for_test(name)
    }

    fn declare_kr_synthdef(synth: &str, kr_ports: &[&str]) {
        let outputs: Vec<OutputPort> = kr_ports
            .iter()
            .map(|n| OutputPort {
                name: (*n).to_string(),
                channels: 1,
                rate: PortRate::Kr,
            })
            .collect();
        register_synthdef_outputs(synth.to_string(), outputs);
    }

    fn declare_ar_synthdef(synth: &str, ar_ports: &[&str]) {
        let outputs: Vec<OutputPort> = ar_ports
            .iter()
            .map(|n| OutputPort {
                name: (*n).to_string(),
                channels: 1,
                rate: PortRate::Ar,
            })
            .collect();
        register_synthdef_outputs(synth.to_string(), outputs);
    }

    /// Register a synthdef IR carrying the given param names so
    /// `get_synthdef_param_defaults` returns them as the available param set
    /// for the target-side validation in `.to_param(...)`.
    fn declare_synthdef_with_params(synth: &str, params: &[&str]) {
        let param_specs: Vec<ParamSpec> = params
            .iter()
            .enumerate()
            .map(|(i, n)| ParamSpec {
                name: (*n).to_string(),
                default: vec![0.0],
                index: i,
                lag_ms: None,
            })
            .collect();
        register_synthdef_ir(
            synth.to_string(),
            GraphIR {
                name: synth.to_string(),
                constants: vec![],
                params: param_specs,
                nodes: vec![],
                out_bus: 0,
            },
        );
    }

    #[test]
    fn to_param_round_trip_into_param_routes() {
        with_test_context(|| {
            let src_synth = "story4_to_param_round_trip_src";
            let tgt_synth = "story4_to_param_round_trip_tgt";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff", "amp"]);

            let mut src = make_voice("vox_src_round_trip").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_round_trip").synth(tgt_synth.to_string());

            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect("kr port + valid target param");

            let src_id = context::get_or_create_voice_id("vox_src_round_trip");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_round_trip");
            context::with_state(|state| {
                let entries = state
                    .param_routes
                    .get(&(src_id, "env".to_string()))
                    .expect("param route installed");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0], (tgt_id, "cutoff".to_string()));

                // No legacy audio-route entry installed.
                assert!(state
                    .routes
                    .get(&(src_id, "env".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn to_param_ar_rate_port_returns_clean_error_citing_rate() {
        with_test_context(|| {
            let src_synth = "story4_to_param_ar_rate_src";
            let tgt_synth = "story4_to_param_ar_rate_tgt";
            declare_ar_synthdef(src_synth, &["sine"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_ar_rate").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_ar_rate").synth(tgt_synth.to_string());

            let err = src
                .output_by_name("sine")
                .expect("port resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect_err("ar-rate source port must error");
            let msg = err.to_string();
            assert!(msg.contains("ar-rate"), "msg = {}", msg);
            assert!(msg.contains("'sine'"), "msg = {}", msg);
            assert!(msg.contains("output_kr"), "msg = {}", msg);

            // No partial route installed.
            let src_id = context::get_or_create_voice_id("vox_src_ar_rate");
            context::with_state(|state| {
                assert!(state
                    .param_routes
                    .get(&(src_id, "sine".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn to_param_unknown_target_param_errors_with_available_params() {
        with_test_context(|| {
            let src_synth = "story4_to_param_unknown_param_src";
            let tgt_synth = "story4_to_param_unknown_param_tgt";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff", "resonance", "amp"]);

            let mut src = make_voice("vox_src_unk_param").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_unk_param").synth(tgt_synth.to_string());

            let err = src
                .output_by_name("env")
                .expect("port resolves")
                .to_param(tgt, "freq".to_string())
                .expect_err("unknown target param must error");
            let msg = err.to_string();
            assert!(msg.contains("'freq'"), "msg = {}", msg);
            // Cites the full target-side param set so a typo is fixable.
            assert!(msg.contains("'cutoff'"), "msg = {}", msg);
            assert!(msg.contains("'resonance'"), "msg = {}", msg);
            assert!(msg.contains("'amp'"), "msg = {}", msg);

            // No partial route installed.
            let src_id = context::get_or_create_voice_id("vox_src_unk_param");
            context::with_state(|state| {
                assert!(state
                    .param_routes
                    .get(&(src_id, "env".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn multiple_to_param_calls_from_same_source_are_additive() {
        with_test_context(|| {
            let src_synth = "story4_to_param_additive_src";
            let tgt_synth_a = "story4_to_param_additive_tgt_a";
            let tgt_synth_b = "story4_to_param_additive_tgt_b";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth_a, &["cutoff"]);
            declare_synthdef_with_params(tgt_synth_b, &["pitch"]);

            let mut src = make_voice("vox_src_additive").synth(src_synth.to_string());
            let tgt_a = make_voice("vox_tgt_additive_a").synth(tgt_synth_a.to_string());
            let tgt_b = make_voice("vox_tgt_additive_b").synth(tgt_synth_b.to_string());

            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt_a, "cutoff".to_string())
                .expect("first install");
            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt_b, "pitch".to_string())
                .expect("second install — must NOT replace the first");

            let src_id = context::get_or_create_voice_id("vox_src_additive");
            let tgt_a_id = context::get_or_create_voice_id("vox_tgt_additive_a");
            let tgt_b_id = context::get_or_create_voice_id("vox_tgt_additive_b");
            context::with_state(|state| {
                let entries = state
                    .param_routes
                    .get(&(src_id, "env".to_string()))
                    .expect("param routes installed");
                assert_eq!(entries.len(), 2, "both targets must be present");
                assert!(entries.contains(&(tgt_a_id, "cutoff".to_string())));
                assert!(entries.contains(&(tgt_b_id, "pitch".to_string())));
            });
        });
    }

    #[test]
    fn to_param_repeat_same_target_pair_is_deduplicated() {
        with_test_context(|| {
            let src_synth = "story4_to_param_dedup_src";
            let tgt_synth = "story4_to_param_dedup_tgt";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_dedup").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_dedup").synth(tgt_synth.to_string());

            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt.clone(), "cutoff".to_string())
                .expect("first");
            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect("repeat");

            let src_id = context::get_or_create_voice_id("vox_src_dedup");
            context::with_state(|state| {
                let entries = state
                    .param_routes
                    .get(&(src_id, "env".to_string()))
                    .expect("installed");
                assert_eq!(entries.len(), 1, "duplicate (target, param) deduped");
            });
        });
    }

    #[test]
    fn multi_route_handle_to_param_fans_out_across_listed_ports() {
        with_test_context(|| {
            let src_synth = "story4_multi_to_param_src";
            let tgt_synth = "story4_multi_to_param_tgt";
            declare_kr_synthdef(src_synth, &["env_a", "env_b", "env_c"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_multi").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_multi").synth(tgt_synth.to_string());

            let arr: rhai::Array = vec![
                rhai::Dynamic::from("env_a".to_string()),
                rhai::Dynamic::from("env_c".to_string()),
            ];
            src.outputs(arr)
                .expect("port list resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect("kr ports + valid target param");

            let src_id = context::get_or_create_voice_id("vox_src_multi");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_multi");
            context::with_state(|state| {
                let a = state
                    .param_routes
                    .get(&(src_id, "env_a".to_string()))
                    .expect("env_a installed");
                let c = state
                    .param_routes
                    .get(&(src_id, "env_c".to_string()))
                    .expect("env_c installed");
                assert_eq!(a, &vec![(tgt_id, "cutoff".to_string())]);
                assert_eq!(c, &vec![(tgt_id, "cutoff".to_string())]);
                // Unlisted port not installed.
                assert!(state
                    .param_routes
                    .get(&(src_id, "env_b".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn multi_route_handle_to_param_short_circuits_on_first_ar_port() {
        with_test_context(|| {
            // Mixed-rate synthdef: env is kr, sine is ar. Fanning to_param
            // across [env, sine] must error on `sine` and leave neither
            // entry installed (the first failure short-circuits before the
            // ar-rate port can leak a partial install).
            let src_synth = "story4_multi_to_param_mixed_rate";
            let tgt_synth = "story4_multi_to_param_mixed_tgt";
            register_synthdef_outputs(
                src_synth.to_string(),
                vec![
                    OutputPort {
                        name: "sine".to_string(),
                        channels: 1,
                        rate: PortRate::Ar,
                    },
                    OutputPort {
                        name: "env".to_string(),
                        channels: 1,
                        rate: PortRate::Kr,
                    },
                ],
            );
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_mixed").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_mixed").synth(tgt_synth.to_string());

            // List "sine" first so the ar-rate validation fires before "env"
            // installs anything.
            let arr: rhai::Array = vec![
                rhai::Dynamic::from("sine".to_string()),
                rhai::Dynamic::from("env".to_string()),
            ];
            let err = src
                .outputs(arr)
                .expect("port list resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect_err("ar-rate port must short-circuit fan-out");
            assert!(err.to_string().contains("ar-rate"));

            let src_id = context::get_or_create_voice_id("vox_src_mixed");
            context::with_state(|state| {
                assert!(state
                    .param_routes
                    .get(&(src_id, "sine".to_string()))
                    .is_none());
                assert!(state
                    .param_routes
                    .get(&(src_id, "env".to_string()))
                    .is_none());
            });
        });
    }
}
