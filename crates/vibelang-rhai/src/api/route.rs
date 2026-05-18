//! Route API for Rhai scripts.
//!
//! `RouteHandle` is the chainable builder produced by
//! [`Voice::output(name|idx)`](super::voice::Voice). It carries the resolved
//! `(voice_id, port_name)` and exposes the three terminal verbs
//! (`to`, `to_main`, `mute`) that install a [`RouteDest`] into the
//! [`ScriptState`](vibelang_core::reload::ScriptState) routes map.
//!
//! Variant-dependent semantics on a repeated `(voice, port)`:
//! - `.to(group)` is **additive** — multiple `.to(g_a)` + `.to(g_b)` calls
//!   on the same port install both as distinct fan-out edges (splitter /
//!   mult patterns: instrument out → main + reverb send).
//! - `.to_main()` and `.mute()` keep replace semantics — Main is the
//!   hardware bus and Muted is silence, neither is meaningfully fanned out.
//! - Routing across the variants (`.to_main()` after `.to(g_a)`, etc.) clears
//!   prior entries on that port: the variants are mutually exclusive.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, Position, TypeBuilder};
use vibelang_core::handlers::{InputRouteSrc, ParamRouteTarget, RouteDest};
use vibelang_core::reload::{ParamRouteConflict, ParamRouteKind};
use vibelang_core::types::VoiceId;
use vibelang_dsp::{get_synthdef_outputs, get_synthdef_param_defaults, OutputPort, PortRate};

use super::group::GroupHandle;
use super::sequence::Fx;
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
    /// `(target, target_param_name)` of the most-recently-installed
    /// `.to_param(...)` (or `.to_param_audio(...)`) call on this handle.
    /// Read by the chained `.scale(...)` / `.offset(...)` modifiers to know
    /// which `(source, target)` shaping slot to update. `None` until the
    /// first successful install. The target side is a [`ParamRouteTarget`]
    /// enum so fx-target routes share the same shaping path.
    last_param_target: Option<(ParamRouteTarget, String)>,
}

impl RouteHandle {
    pub(crate) fn new(voice_id: VoiceId, port_name: String) -> Self {
        Self {
            voice_id,
            port_name,
            last_param_target: None,
        }
    }

    /// Install a route to the given group's mix bus.
    ///
    /// Multi-target fan-out: chaining `.to(g_a).to(g_b)` on a single
    /// `(voice, port)` installs both groups as distinct fan-out edges.
    /// A repeated `.to(g_a)` is deduplicated. If the port previously routed
    /// to `Main` or was `Muted`, the prior dest is cleared first — the
    /// variants are mutually exclusive (see [`crate::route`] module docs).
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
    /// Errors when the voice has no explicit group set (lives at the implicit
    /// root); the message points users at `.group("name")` on the voice or
    /// `.to(group("name"))` on the route. Re-routing replaces the prior dest
    /// (same `HashMap::insert` semantics as the other terminal verbs).
    pub fn to_current_group(self) -> Result<Self, Box<EvalAltResult>> {
        let root_id = context::get_or_create_group_id("");
        let voice_group =
            context::with_state(|state| state.voices.get(&self.voice_id).map(|v| v.group));
        match voice_group {
            Some(gid) if gid != root_id => {
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
    pub fn to_param(self, target: Voice, param_name: String) -> Result<Self, Box<EvalAltResult>> {
        let mut target = target;
        target.resolve_name();
        let target_name = target.name.clone();
        let target_synth = target.get_synth_name();
        let target_id = context::get_or_create_voice_id(&target_name);
        self.to_param_target(
            ParamRouteKind::Set,
            ParamSourceValidation::ToParam,
            "to_param",
            ParamRouteTarget::Voice(target_id),
            target_name,
            target_synth,
            param_name,
        )
    }

    /// Install a CV-to-param route from this audio-rate output port to the
    /// named parameter on `target`, downsampling the source audio to kr
    /// at runtime via a shared `a2k_adapter_1` synth.
    ///
    /// Mirrors [`Self::to_param`] but flips the rate constraint: the source
    /// port must be audio-rate (`output(...)`); kr-rate ports are rejected
    /// with a clean error pointing at `.to_param()`. The route still lands
    /// in the SET map and shares the multi-source SET / cross-verb conflict
    /// rules with `.to_param`. The summer infrastructure is shared too —
    /// chained `.scale(...)` / `.offset(...)` still apply because the
    /// adapter's kr bus feeds the same `param_kr_modulate_<n>` summer that
    /// pure-kr routes use.
    ///
    /// One adapter is spawned per `(source_voice, source_port)` pair the
    /// runtime sees in either param-route map; multiple `.to_param_audio()`
    /// routes from the same source share that adapter rather than each
    /// allocating a new kr bus.
    pub fn to_param_audio(
        self,
        target: Voice,
        param_name: String,
    ) -> Result<Self, Box<EvalAltResult>> {
        let mut target = target;
        target.resolve_name();
        let target_name = target.name.clone();
        let target_synth = target.get_synth_name();
        let target_id = context::get_or_create_voice_id(&target_name);
        self.to_param_target(
            ParamRouteKind::Set,
            ParamSourceValidation::ToParamAudio,
            "to_param_audio",
            ParamRouteTarget::Voice(target_id),
            target_name,
            target_synth,
            param_name,
        )
    }

    /// Install a CV-to-param route from this **trigger-rate** (`Tr`) output
    /// port to the named parameter on `target`.
    ///
    /// Multi-output v3 Story B2.c. The source port must be trigger-rate
    /// (`output_tr`); audio- and control-rate ports are rejected with a
    /// clean error citing the actual rate plus a hint pointing at `.to(...)`
    /// / `.to_param(...)` / `.to_param_audio(...)` as appropriate. The
    /// target param must exist on the target voice's synthdef.
    ///
    /// Wiring: trigger-rate sources land in
    /// [`ScriptState::param_routes_trigger`](vibelang_core::reload::ScriptState::param_routes_trigger).
    /// The runtime spawns a `port_tr_to_param_link_1` synth that 1:1
    /// forwards the source's Tr bus to an intermediate kr bus, and the
    /// target param is `/n_map`-bound to that bus. Sample-accurate edges
    /// from `Out.tr` are preserved end-to-end — there is no scale/offset
    /// shaping (triggers don't bend), and multi-source fan-in is rejected
    /// (trigger routing is 1:1).
    pub fn to_trigger(self, target: Voice, param_name: String) -> Result<Self, Box<EvalAltResult>> {
        let mut target = target;
        target.resolve_name();
        let target_name = target.name.clone();
        let target_synth = target.get_synth_name();
        let target_id = context::get_or_create_voice_id(&target_name);
        self.to_param_target(
            ParamRouteKind::Trigger,
            ParamSourceValidation::ToTrigger,
            "to_trigger",
            ParamRouteTarget::Voice(target_id),
            target_name,
            target_synth,
            param_name,
        )
    }

    /// Set the per-source `scale` factor on the most-recently-installed
    /// `.to_param(...)` route on this handle. Default is `1.0`. Multi-call:
    /// last `.scale()` wins (offset is left untouched).
    ///
    /// Errors if called before any `.to_param(...)` install — chained order
    /// must be `.to_param(...).scale(...)`, not the other way around.
    pub fn scale(self, scale: f64) -> Result<Self, Box<EvalAltResult>> {
        let (target, target_param) = self
            .last_param_target
            .clone()
            .ok_or_else(|| no_prior_param_route_error("scale", "RouteHandle"))?;
        let scale = scale as f32;
        context::with_state(|state| {
            state.set_param_route_set_scale(
                self.voice_id,
                self.port_name.clone(),
                target,
                target_param,
                scale,
            );
        });
        Ok(self)
    }

    /// Set the per-source `offset` factor on the most-recently-installed
    /// `.to_param(...)` route. Default is `0.0`. Multi-call: last `.offset()`
    /// wins (scale is left untouched).
    ///
    /// Errors if called before any `.to_param(...)` install.
    pub fn offset(self, offset: f64) -> Result<Self, Box<EvalAltResult>> {
        let (target, target_param) = self
            .last_param_target
            .clone()
            .ok_or_else(|| no_prior_param_route_error("offset", "RouteHandle"))?;
        let offset = offset as f32;
        context::with_state(|state| {
            state.set_param_route_set_offset(
                self.voice_id,
                self.port_name.clone(),
                target,
                target_param,
                offset,
            );
        });
        Ok(self)
    }

    /// Fx-target variant of [`Self::to_param`]. Same semantics, but the
    /// route's target is an [`Fx`]'s param instead of a Voice's. Rhai
    /// dispatches by argument type so the surface verb is just `.to_param`.
    pub fn to_param_fx(self, target: Fx, param_name: String) -> Result<Self, Box<EvalAltResult>> {
        let target_name = target.id.clone();
        let target_synth = target.synth_name();
        let effect_id = context::get_or_create_effect_id(&target_name);
        self.to_param_target(
            ParamRouteKind::Set,
            ParamSourceValidation::ToParam,
            "to_param",
            ParamRouteTarget::Effect(effect_id),
            target_name,
            target_synth,
            param_name,
        )
    }

    /// Fx-target variant of [`Self::to_param_audio`]. ar source coerced to
    /// kr via the shared `a2k_adapter_1`, then routed into the target fx's
    /// param via the same summer infrastructure as voice targets.
    pub fn to_param_audio_fx(
        self,
        target: Fx,
        param_name: String,
    ) -> Result<Self, Box<EvalAltResult>> {
        let target_name = target.id.clone();
        let target_synth = target.synth_name();
        let effect_id = context::get_or_create_effect_id(&target_name);
        self.to_param_target(
            ParamRouteKind::Set,
            ParamSourceValidation::ToParamAudio,
            "to_param_audio",
            ParamRouteTarget::Effect(effect_id),
            target_name,
            target_synth,
            param_name,
        )
    }

    /// Fx-target variant of [`Self::to_trigger`]. Tr-rate sources forward
    /// edges to the target fx's param via the same `port_tr_to_param_link_1`
    /// infrastructure used for voice targets.
    pub fn to_trigger_fx(self, target: Fx, param_name: String) -> Result<Self, Box<EvalAltResult>> {
        let target_name = target.id.clone();
        let target_synth = target.synth_name();
        let effect_id = context::get_or_create_effect_id(&target_name);
        self.to_param_target(
            ParamRouteKind::Trigger,
            ParamSourceValidation::ToTrigger,
            "to_trigger",
            ParamRouteTarget::Effect(effect_id),
            target_name,
            target_synth,
            param_name,
        )
    }

    fn to_param_target(
        mut self,
        kind: ParamRouteKind,
        validation: ParamSourceValidation,
        verb: &'static str,
        target: ParamRouteTarget,
        target_name: String,
        target_synth: String,
        param_name: String,
    ) -> Result<Self, Box<EvalAltResult>> {
        install_param_route(
            kind,
            self.voice_id,
            self.port_name.clone(),
            target,
            &target_name,
            &target_synth,
            param_name.clone(),
            validation,
            verb,
        )?;
        if kind != ParamRouteKind::Trigger {
            self.last_param_target = Some((target, param_name));
        }
        Ok(self)
    }

    /// Commit the resolved destination to script state.
    fn commit(&self, dest: RouteDest) {
        context::with_state(|state| {
            state.set_route(self.voice_id, self.port_name.clone(), dest);
        });
    }
}

#[derive(Clone, Copy, Debug)]
enum ParamSourceValidation {
    ToParam,
    ToParamAudio,
    ToTrigger,
    ModulateBy,
}

impl ParamSourceValidation {
    fn required_rate(self) -> PortRate {
        match self {
            Self::ToParam | Self::ModulateBy => PortRate::Kr,
            Self::ToParamAudio => PortRate::Ar,
            Self::ToTrigger => PortRate::Tr,
        }
    }

    fn source_rate_error(self, port: &str, synth: &str, rate: PortRate) -> Box<EvalAltResult> {
        match self {
            Self::ToParam => ar_rate_to_param_error(port, synth, rate),
            Self::ToParamAudio => kr_or_tr_rate_to_param_audio_error(port, synth, rate),
            Self::ToTrigger => non_tr_rate_to_trigger_error(port, synth, rate),
            Self::ModulateBy => modulate_by_ar_rate_error(port, synth, rate),
        }
    }

    fn missing_source_port_error(
        self,
        port: &str,
        synth: &str,
        outputs: &[OutputPort],
    ) -> Box<EvalAltResult> {
        match self {
            Self::ModulateBy => modulate_by_missing_source_port_error(port, synth, outputs),
            Self::ToParam | Self::ToParamAudio | Self::ToTrigger => {
                missing_source_port_error(port, synth, outputs)
            }
        }
    }

    fn unknown_target_param_error(
        self,
        target_name: &str,
        target_synth: &str,
        param: &str,
        available: &std::collections::HashMap<String, f32>,
    ) -> Box<EvalAltResult> {
        match self {
            Self::ModulateBy => {
                modulate_by_unknown_target_param_error(target_name, target_synth, param, available)
            }
            Self::ToParam | Self::ToParamAudio | Self::ToTrigger => {
                unknown_target_param_error(target_name, target_synth, param, available)
            }
        }
    }
}

fn install_param_route(
    kind: ParamRouteKind,
    source_voice: VoiceId,
    source_port: String,
    target: ParamRouteTarget,
    target_name: &str,
    target_synth: &str,
    target_param: String,
    validation: ParamSourceValidation,
    conflict_verb: &str,
) -> Result<(), Box<EvalAltResult>> {
    let src_synth = source_synthdef_name(source_voice);
    let src_outputs = get_synthdef_outputs(&src_synth).unwrap_or_default();
    match src_outputs.iter().find(|p| p.name == source_port) {
        Some(p) if p.rate == validation.required_rate() => {}
        Some(p) => return Err(validation.source_rate_error(&source_port, &src_synth, p.rate)),
        None => {
            return Err(validation.missing_source_port_error(
                &source_port,
                &src_synth,
                &src_outputs,
            ))
        }
    }

    let target_params = get_synthdef_param_defaults(target_synth);
    if !target_params.contains_key(&target_param) {
        return Err(validation.unknown_target_param_error(
            target_name,
            target_synth,
            &target_param,
            &target_params,
        ));
    }

    let conflict = context::with_state(|state| {
        state
            .add_param_route(
                kind,
                source_voice,
                source_port,
                target,
                target_param.clone(),
            )
            .err()
    });
    if let Some(c) = conflict {
        return Err(param_route_conflict_error(conflict_verb, &c));
    }
    Ok(())
}

/// Render a `ParamRouteConflict` as a Rhai EvalAltResult error, scoped by the
/// surface verb so the message lands cleanly in the user's stack trace.
fn param_route_conflict_error(verb: &str, conflict: &ParamRouteConflict) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        format!("{}(): {}", verb, conflict).into(),
        Position::NONE,
    ))
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

fn ar_rate_to_param_error(port: &str, synth: &str, rate: PortRate) -> Box<EvalAltResult> {
    let rate_str = match rate {
        PortRate::Ar => "ar",
        PortRate::Kr => "kr",
        PortRate::Tr => "tr",
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

fn non_tr_rate_to_trigger_error(port: &str, synth: &str, rate: PortRate) -> Box<EvalAltResult> {
    let (rate_str, hint) = match rate {
        PortRate::Ar => (
            "ar",
            "use .to(group(...)) / .to_main() for audio routing, or .to_param_audio() \
             for ar→param coercion",
        ),
        PortRate::Kr => ("kr", "use .to_param() for kr→param routing"),
        PortRate::Tr => ("tr", "(unreachable — Tr is the valid rate for .to_trigger)"),
    };
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "to_trigger() on port '{}' (synthdef '{}'): port is {}-rate, but \
             .to_trigger() requires a tr-rate (trigger) port — declare the port \
             with .output_tr(...) on the synthdef. {}",
            port, synth, rate_str, hint
        )
        .into(),
        Position::NONE,
    ))
}

fn kr_or_tr_rate_to_param_audio_error(
    port: &str,
    synth: &str,
    rate: PortRate,
) -> Box<EvalAltResult> {
    let rate_str = match rate {
        PortRate::Ar => "ar",
        PortRate::Kr => "kr",
        PortRate::Tr => "tr",
    };
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "to_param_audio() on port '{}' (synthdef '{}'): port is {}-rate, \
             but .to_param_audio() requires an ar-rate (audio) port — for \
             kr-rate sources use .to_param() instead, which routes the \
             control bus directly without rate coercion",
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

/// Target-first dual of [`RouteHandle::to_param`].
///
/// Constructed by [`Voice::param_handle`](super::voice::Voice). Carries the
/// resolved target `(voice_id, voice_name, synthdef_name, param_name)` —
/// every modulator wired into this param from a kr source port reads better
/// `target.param("freq").modulate_by(source, "out")` than
/// `source.output("out").to_param(target, "freq")` when ONE target receives
/// MANY modulators (target-first scan).
///
/// `.modulate_by(...)` installs the same [`ScriptState::add_param_route`]
/// entry that `.to_param(...)` does — the registry is direction-agnostic, so
/// a route built either way looks identical post-install (additive across
/// distinct source ports; duplicate `(source, port)` pairs deduplicated).
#[derive(Debug, Clone, CustomType)]
pub struct ParamHandle {
    /// Resolved target — either a Voice or an Effect. Recorded as a
    /// [`ParamRouteTarget`] so the route registry can carry both.
    target: ParamRouteTarget,
    /// User-visible name of the target (voice name or fx id). Used only
    /// for error messages.
    target_name: String,
    /// Synthdef declaring the target's params — needed to validate
    /// `param_name` against the target's declared param set.
    target_synth: String,
    /// The param on `target` that this handle binds.
    param_name: String,
    /// `(source_voice_id, source_port_name)` of the most-recently-installed
    /// `.modulate_by(...)` call on this handle. Read by chained `.scale(...)`
    /// / `.offset(...)` modifiers to know which `(source, target)` shaping
    /// slot to update. `None` until the first successful `.modulate_by`.
    last_modulate_source: Option<(VoiceId, String)>,
}

impl ParamHandle {
    pub(crate) fn new(
        target: ParamRouteTarget,
        target_name: String,
        target_synth: String,
        param_name: String,
    ) -> Self {
        Self {
            target,
            target_name,
            target_synth,
            param_name,
            last_modulate_source: None,
        }
    }

    /// Install a CV-to-param route from `source`'s kr output `port` to this
    /// handle's target `(voice, param)`.
    ///
    /// Validation:
    /// - Source port must be declared on the source voice's synthdef and be
    ///   control-rate (`output_kr`); audio-rate or unknown ports error with
    ///   the synthdef's declared output set.
    /// - Target param must exist on the target voice's synthdef; unknown
    ///   params error citing the synthdef's declared param set.
    ///
    /// Returns `self` so callers chain multiple modulators into the same
    /// param: `t.param("freq").modulate_by(s1, "out").modulate_by(s2, "out2")`.
    /// Each call appends a separate `(source_voice, source_port)` entry in
    /// [`ScriptState::param_routes`] — additive fan-in is recorded; runtime
    /// last-write-wins resolution is the v2 contract.
    pub fn modulate_by(mut self, source: Voice, port: String) -> Result<Self, Box<EvalAltResult>> {
        let mut source = source;
        source.resolve_name();
        let source_id = context::get_or_create_voice_id(&source.name);

        install_param_route(
            ParamRouteKind::Bend,
            source_id,
            port.clone(),
            self.target,
            &self.target_name,
            &self.target_synth,
            self.param_name.clone(),
            ParamSourceValidation::ModulateBy,
            "modulate_by",
        )?;
        self.last_modulate_source = Some((source_id, port));
        Ok(self)
    }

    /// Set the per-source `scale` factor on the most-recently-installed
    /// `.modulate_by(...)` route on this handle. Default is `1.0`. Multi-call:
    /// last `.scale()` wins (offset is left untouched).
    ///
    /// Errors if called before any `.modulate_by(...)` install — chained
    /// order must be `.modulate_by(...).scale(...)`.
    pub fn scale(self, scale: f64) -> Result<Self, Box<EvalAltResult>> {
        let (source_voice, source_port) = self
            .last_modulate_source
            .clone()
            .ok_or_else(|| no_prior_param_route_error("scale", "ParamHandle"))?;
        let scale = scale as f32;
        context::with_state(|state| {
            state.set_param_route_bend_scale(
                source_voice,
                source_port,
                self.target,
                self.param_name.clone(),
                scale,
            );
        });
        Ok(self)
    }

    /// Set the per-source `offset` factor on the most-recently-installed
    /// `.modulate_by(...)` route. Default is `0.0`. Multi-call: last
    /// `.offset()` wins (scale is left untouched).
    ///
    /// Errors if called before any `.modulate_by(...)` install.
    pub fn offset(self, offset: f64) -> Result<Self, Box<EvalAltResult>> {
        let (source_voice, source_port) = self
            .last_modulate_source
            .clone()
            .ok_or_else(|| no_prior_param_route_error("offset", "ParamHandle"))?;
        let offset = offset as f32;
        context::with_state(|state| {
            state.set_param_route_bend_offset(
                source_voice,
                source_port,
                self.target,
                self.param_name.clone(),
                offset,
            );
        });
        Ok(self)
    }
}

fn no_prior_param_route_error(verb: &str, handle_kind: &str) -> Box<EvalAltResult> {
    let install_verb = match handle_kind {
        "ParamHandle" => ".modulate_by(source, \"port\")",
        _ => ".to_param(target, \"param\")",
    };
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "{}() called on a {} with no prior {} install — chain it after \
             the route is committed: e.g. `{}.{}(value)`",
            verb, handle_kind, install_verb, install_verb, verb,
        )
        .into(),
        Position::NONE,
    ))
}

fn modulate_by_ar_rate_error(port: &str, synth: &str, rate: PortRate) -> Box<EvalAltResult> {
    let rate_str = match rate {
        PortRate::Ar => "ar",
        PortRate::Kr => "kr",
        PortRate::Tr => "tr",
    };
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "modulate_by() on source port '{}' (synthdef '{}'): port is {}-rate, \
             but .modulate_by() requires a kr-rate (control) port — declare the \
             port with .output_kr(...) on the synthdef, or route audio with \
             .to(group(...)) / .to_main()",
            port, synth, rate_str
        )
        .into(),
        Position::NONE,
    ))
}

fn modulate_by_missing_source_port_error(
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
            "modulate_by(): source port '{}' not found on synthdef '{}' \
             (available: {})",
            port, synth, available
        )
        .into(),
        Position::NONE,
    ))
}

fn modulate_by_unknown_target_param_error(
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
            "modulate_by(): target voice '{}' synthdef '{}' has no param '{}' \
             (available: {})",
            target_voice, target_synth, param, avail_list
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
}

/// Chainable handle for installing an input wiring on a voice's named input
/// port (target-first, single-source).
///
/// Constructed by [`Voice::input`](super::voice::Voice::input). Terminal verbs
/// — `.from(...)`, `.from_current_group()`, `.disconnect()` — each commit one
/// entry to the script state's input-route map, replacing any prior source
/// on the same `(target_voice, input_port)` key per the named-inputs design
/// (kb/named-inputs-design-notes.md decision #1).
///
/// Multi-source fan-in (`.add_from`, `.from_all`) lands in P2.3 — out of scope
/// here. Source-first sugar (`source.output(port).to_input(target, name)`)
/// lands in P2.2.
#[derive(Debug, Clone, CustomType)]
pub struct InputHandle {
    /// Target voice ID — the voice receiving the input.
    voice_id: VoiceId,
    /// Target input port name on the voice's synthdef. Not validated against
    /// the synthdef's declared input ports (no input registry yet); the
    /// dispatcher (P3.3) is the source of truth on bus resolution.
    port_name: String,
}

impl InputHandle {
    pub(crate) fn new(voice_id: VoiceId, port_name: String) -> Self {
        Self {
            voice_id,
            port_name,
        }
    }

    /// Pin this input to a source voice's default output port (`"out"`).
    ///
    /// Replace semantics: any prior source on `(this_voice, port)` is
    /// overwritten with a fresh single-element source list. Explicit port
    /// selection on the source (e.g. `source.output("ring")`) is the P2.2
    /// source-first sugar; this `.from(voice)` overload always pairs with
    /// the legacy `"out"` port name on the source.
    pub fn from_voice(self, source: Voice) -> Self {
        let mut source = source;
        source.resolve_name();
        let source_id = context::get_or_create_voice_id(&source.name);
        self.commit(InputRouteSrc::Voice(source_id, "out".to_string()));
        self
    }

    /// Pin this input to a group's mix bus.
    ///
    /// The same audio bus that `.to(group)` writes into on the output side —
    /// reading from it is symmetric with writing to it. Replace semantics
    /// (see [`Self::from_voice`]).
    pub fn from_group(self, source: GroupHandle) -> Self {
        let group_id = context::get_or_create_group_id(&source.path);
        self.commit(InputRouteSrc::Group(group_id));
        self
    }

    /// Pin this input to the parent group's pre-fader mix bus — i.e. the
    /// group this voice is currently configured into.
    ///
    /// Errors when the voice lives at the implicit root (no explicit
    /// `.group(...)` call): the error message points users at
    /// `.group("name")` on the voice or `.from(group("name"))` on the input
    /// handle, matching the error path on the output-side
    /// `RouteHandle::to_current_group`.
    pub fn from_current_group(self) -> Result<Self, Box<EvalAltResult>> {
        let root_id = context::get_or_create_group_id("");
        let voice_group =
            context::with_state(|state| state.voices.get(&self.voice_id).map(|v| v.group));
        match voice_group {
            Some(gid) if gid != root_id => {
                self.commit(InputRouteSrc::Group(gid));
                Ok(self)
            }
            _ => Err(no_current_group_input_error(&self.port_name)),
        }
    }

    /// Pin this input to the shared silent bus.
    ///
    /// Explicit "no source" — the P3.3 dispatcher resolves `Silent` to the
    /// shared silent ar/kr bus allocated at startup (kb/named-inputs-design-
    /// notes.md decision #2). Replace semantics like the other verbs.
    pub fn disconnect(self) -> Self {
        self.commit(InputRouteSrc::Silent);
        self
    }

    /// Commit the resolved source to script state.
    fn commit(&self, src: InputRouteSrc) {
        context::with_state(|state| {
            state.set_input_route(self.voice_id, self.port_name.clone(), src);
        });
    }
}

fn no_current_group_input_error(port: &str) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "from_current_group() on input '{}': voice has no explicit group set — \
             call `.group(\"name\")` on the voice first, or write \
             `.from(group(\"name\"))` to target a group explicitly",
            port
        )
        .into(),
        Position::NONE,
    ))
}

/// Register the `RouteHandle` type and its terminal verbs with a Rhai engine.
pub fn register(engine: &mut Engine) {
    engine.build_type::<RouteHandle>();
    engine.register_fn("to", RouteHandle::to);
    engine.register_fn("to_main", RouteHandle::to_main);
    engine.register_fn("mute", RouteHandle::mute);
    engine.register_fn("to_current_group", RouteHandle::to_current_group);
    // Voice-target verbs (existing surface).
    engine.register_fn("to_param", RouteHandle::to_param);
    engine.register_fn("to_param_audio", RouteHandle::to_param_audio);
    engine.register_fn("to_trigger", RouteHandle::to_trigger);
    // Fx-target overloads — Rhai dispatches by argument type.
    engine.register_fn("to_param", RouteHandle::to_param_fx);
    engine.register_fn("to_param_audio", RouteHandle::to_param_audio_fx);
    engine.register_fn("to_trigger", RouteHandle::to_trigger_fx);
    engine.register_fn("scale", RouteHandle::scale);
    engine.register_fn("offset", RouteHandle::offset);

    engine.build_type::<MultiRouteHandle>();
    engine.register_fn("to", MultiRouteHandle::to);
    engine.register_fn("to_main", MultiRouteHandle::to_main);
    engine.register_fn("mute", MultiRouteHandle::mute);
    engine.register_fn("to_current_group", MultiRouteHandle::to_current_group);

    engine.build_type::<ParamHandle>();
    engine.register_fn("modulate_by", ParamHandle::modulate_by);
    engine.register_fn("scale", ParamHandle::scale);
    engine.register_fn("offset", ParamHandle::offset);

    engine.build_type::<InputHandle>();
    // `.from(...)` dispatches by argument type: Voice or GroupHandle.
    engine.register_fn("from", InputHandle::from_voice);
    engine.register_fn("from", InputHandle::from_group);
    engine.register_fn("from_current_group", InputHandle::from_current_group);
    engine.register_fn("disconnect", InputHandle::disconnect);
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
    use super::ParamRouteTarget;
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
                    .param_routes_set
                    .get(&(src_id, "env".to_string()))
                    .expect("param route installed");
                assert_eq!(entries.len(), 1);
                assert_eq!(
                    entries[0],
                    (ParamRouteTarget::Voice(tgt_id), "cutoff".to_string())
                );

                // No legacy audio-route entry installed.
                assert!(state.routes.get(&(src_id, "env".to_string())).is_none());
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
                    .param_routes_set
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
                    .param_routes_set
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
                    .param_routes_set
                    .get(&(src_id, "env".to_string()))
                    .expect("param routes installed");
                assert_eq!(entries.len(), 2, "both targets must be present");
                assert!(
                    entries.contains(&(ParamRouteTarget::Voice(tgt_a_id), "cutoff".to_string()))
                );
                assert!(entries.contains(&(ParamRouteTarget::Voice(tgt_b_id), "pitch".to_string())));
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
                    .param_routes_set
                    .get(&(src_id, "env".to_string()))
                    .expect("installed");
                assert_eq!(entries.len(), 1, "duplicate (target, param) deduped");
            });
        });
    }

    // (Two MultiRouteHandle.to_param tests removed: post-SET/BEND-split, plural
    //  fan-out via .to_param violates SET semantics — `/n_map` binds one bus,
    //  so multi-source SET is a conflict. MultiRouteHandle.to_param was removed.
    //  Plural sugar still applies to .to / .to_main / .to_current_group / .mute /
    //  .modulate_by, where multi-source semantics are well-defined.)

    // ==================== ParamHandle / .modulate_by tests ====================

    #[test]
    fn modulate_by_installs_param_route() {
        with_test_context(|| {
            let src_synth = "story2_modulate_by_round_trip_src";
            let tgt_synth = "story2_modulate_by_round_trip_tgt";
            declare_kr_synthdef(src_synth, &["out"]);
            declare_synthdef_with_params(tgt_synth, &["freq", "amp"]);

            let src = make_voice("vox_src_modby_rt").synth(src_synth.to_string());
            let mut tgt = make_voice("vox_tgt_modby_rt").synth(tgt_synth.to_string());

            tgt.param_handle("freq")
                .modulate_by(src, "out".to_string())
                .expect("kr source + valid target param");

            let src_id = context::get_or_create_voice_id("vox_src_modby_rt");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_modby_rt");
            context::with_state(|state| {
                // .modulate_by lands in the BEND map.
                let entries = state
                    .param_routes_bend
                    .get(&(src_id, "out".to_string()))
                    .expect("bend route installed");
                assert_eq!(entries.len(), 1);
                assert_eq!(
                    entries[0],
                    (ParamRouteTarget::Voice(tgt_id), "freq".to_string())
                );

                // SET map and audio routes untouched.
                assert!(state
                    .param_routes_set
                    .get(&(src_id, "out".to_string()))
                    .is_none());
                assert!(state.routes.get(&(src_id, "out".to_string())).is_none());
            });
        });
    }

    #[test]
    fn modulate_by_ar_rate_source_port_returns_clean_error_citing_rate() {
        with_test_context(|| {
            let src_synth = "story2_modulate_by_ar_rate_src";
            let tgt_synth = "story2_modulate_by_ar_rate_tgt";
            declare_ar_synthdef(src_synth, &["sine"]);
            declare_synthdef_with_params(tgt_synth, &["freq"]);

            let src = make_voice("vox_src_modby_ar").synth(src_synth.to_string());
            let mut tgt = make_voice("vox_tgt_modby_ar").synth(tgt_synth.to_string());

            let err = tgt
                .param_handle("freq")
                .modulate_by(src, "sine".to_string())
                .expect_err("ar-rate source port must error");
            let msg = err.to_string();
            assert!(msg.contains("ar-rate"), "msg = {}", msg);
            assert!(msg.contains("'sine'"), "msg = {}", msg);
            assert!(msg.contains("output_kr"), "msg = {}", msg);

            let src_id = context::get_or_create_voice_id("vox_src_modby_ar");
            context::with_state(|state| {
                assert!(state
                    .param_routes_bend
                    .get(&(src_id, "sine".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn modulate_by_unknown_target_param_errors_with_available_params() {
        with_test_context(|| {
            let src_synth = "story2_modulate_by_unk_param_src";
            let tgt_synth = "story2_modulate_by_unk_param_tgt";
            declare_kr_synthdef(src_synth, &["out"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff", "resonance", "amp"]);

            let src = make_voice("vox_src_modby_unk").synth(src_synth.to_string());
            let mut tgt = make_voice("vox_tgt_modby_unk").synth(tgt_synth.to_string());

            let err = tgt
                .param_handle("freq")
                .modulate_by(src, "out".to_string())
                .expect_err("unknown target param must error");
            let msg = err.to_string();
            assert!(msg.contains("'freq'"), "msg = {}", msg);
            assert!(msg.contains("'cutoff'"), "msg = {}", msg);
            assert!(msg.contains("'resonance'"), "msg = {}", msg);
            assert!(msg.contains("'amp'"), "msg = {}", msg);

            let src_id = context::get_or_create_voice_id("vox_src_modby_unk");
            context::with_state(|state| {
                assert!(state
                    .param_routes_bend
                    .get(&(src_id, "out".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn modulate_by_chained_two_sources_both_install() {
        with_test_context(|| {
            // target.param("a").modulate_by(s, "out").modulate_by(s2, "other")
            // — both source→target wires must persist in param_routes.
            // Last-write-wins runtime resolution is a v2 contract, but the
            // registry records both edges.
            let src_synth_a = "story2_modulate_by_chain_src_a";
            let src_synth_b = "story2_modulate_by_chain_src_b";
            let tgt_synth = "story2_modulate_by_chain_tgt";
            declare_kr_synthdef(src_synth_a, &["out"]);
            declare_kr_synthdef(src_synth_b, &["other"]);
            declare_synthdef_with_params(tgt_synth, &["a", "amp"]);

            let s = make_voice("vox_src_modby_chain_a").synth(src_synth_a.to_string());
            let s2 = make_voice("vox_src_modby_chain_b").synth(src_synth_b.to_string());
            let mut tgt = make_voice("vox_tgt_modby_chain").synth(tgt_synth.to_string());

            tgt.param_handle("a")
                .modulate_by(s, "out".to_string())
                .expect("first wire")
                .modulate_by(s2, "other".to_string())
                .expect("second wire");

            let s_id = context::get_or_create_voice_id("vox_src_modby_chain_a");
            let s2_id = context::get_or_create_voice_id("vox_src_modby_chain_b");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_modby_chain");
            context::with_state(|state| {
                let from_s = state
                    .param_routes_bend
                    .get(&(s_id, "out".to_string()))
                    .expect("first source wire installed");
                assert_eq!(
                    from_s,
                    &vec![(ParamRouteTarget::Voice(tgt_id), "a".to_string())]
                );

                let from_s2 = state
                    .param_routes_bend
                    .get(&(s2_id, "other".to_string()))
                    .expect("second source wire installed");
                assert_eq!(
                    from_s2,
                    &vec![(ParamRouteTarget::Voice(tgt_id), "a".to_string())]
                );
            });
        });
    }

    #[test]
    fn cross_direction_to_param_lands_in_set_modulate_by_lands_in_bend() {
        // Multi-output v2 split: `.to_param` is SET (param_routes_set) and
        // `.modulate_by` is BEND (param_routes_bend). The two surfaces are
        // *not* interchangeable any more — they carry distinct semantics.
        with_test_context(|| {
            let src_synth = "story_split_xdir_src";
            let tgt_synth = "story_split_xdir_tgt";
            declare_kr_synthdef(src_synth, &["out"]);
            declare_synthdef_with_params(tgt_synth, &["freq"]);

            // Path A: source-first .to_param(target, "freq") → SET.
            let mut src_a = make_voice("vox_src_xdir_a").synth(src_synth.to_string());
            let tgt_a = make_voice("vox_tgt_xdir_a").synth(tgt_synth.to_string());
            src_a
                .output_by_name("out")
                .expect("port resolves")
                .to_param(tgt_a, "freq".to_string())
                .expect("install A");
            let src_a_id = context::get_or_create_voice_id("vox_src_xdir_a");
            let tgt_a_id = context::get_or_create_voice_id("vox_tgt_xdir_a");

            // Path B: target-first .param("freq").modulate_by(source, "out") → BEND.
            let src_b = make_voice("vox_src_xdir_b").synth(src_synth.to_string());
            let mut tgt_b = make_voice("vox_tgt_xdir_b").synth(tgt_synth.to_string());
            tgt_b
                .param_handle("freq")
                .modulate_by(src_b, "out".to_string())
                .expect("install B");
            let src_b_id = context::get_or_create_voice_id("vox_src_xdir_b");
            let tgt_b_id = context::get_or_create_voice_id("vox_tgt_xdir_b");

            context::with_state(|state| {
                let set_entries = state
                    .param_routes_set
                    .get(&(src_a_id, "out".to_string()))
                    .expect("A installed in SET map");
                assert_eq!(set_entries.len(), 1);
                assert_eq!(
                    set_entries[0],
                    (ParamRouteTarget::Voice(tgt_a_id), "freq".to_string())
                );

                let bend_entries = state
                    .param_routes_bend
                    .get(&(src_b_id, "out".to_string()))
                    .expect("B installed in BEND map");
                assert_eq!(bend_entries.len(), 1);
                assert_eq!(
                    bend_entries[0],
                    (ParamRouteTarget::Voice(tgt_b_id), "freq".to_string())
                );

                // Cross-pollination: A's source key is *not* in BEND, B's source
                // key is *not* in SET. The maps are disjoint.
                assert!(state
                    .param_routes_bend
                    .get(&(src_a_id, "out".to_string()))
                    .is_none());
                assert!(state
                    .param_routes_set
                    .get(&(src_b_id, "out".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn to_param_then_modulate_by_on_same_target_errors() {
        // Cross-verb conflict: `.to_param` followed by `.modulate_by` on the
        // same `(target_voice, target_param)` must error at script time.
        with_test_context(|| {
            let src_synth = "story_split_xverb_src";
            let tgt_synth = "story_split_xverb_tgt";
            declare_kr_synthdef(src_synth, &["out", "lfo"]);
            declare_synthdef_with_params(tgt_synth, &["freq"]);

            let src = make_voice("vox_src_xverb").synth(src_synth.to_string());
            let mut tgt = make_voice("vox_tgt_xverb").synth(tgt_synth.to_string());

            // First wire: SET.
            src.clone()
                .output_by_name("out")
                .expect("port resolves")
                .to_param(tgt.clone(), "freq".to_string())
                .expect("install set");

            // Now try BEND on the same `(target, param)` from a different source
            // port — must error.
            let err = tgt
                .param_handle("freq")
                .modulate_by(src.clone(), "lfo".to_string())
                .expect_err("cross-verb conflict must error");
            let msg = err.to_string();
            assert!(
                msg.contains("modulate_by"),
                "msg should be scoped to modulate_by, got: {}",
                msg,
            );
            assert!(
                msg.contains("to_param") && msg.contains("modulate_by"),
                "msg should mention both verbs, got: {}",
                msg,
            );

            // BEND map must not have a leak.
            let src_id = context::get_or_create_voice_id("vox_src_xverb");
            context::with_state(|state| {
                assert!(state
                    .param_routes_bend
                    .get(&(src_id, "lfo".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn modulate_by_then_to_param_on_same_target_errors() {
        // Symmetric: BEND first, SET second — also errors.
        with_test_context(|| {
            let src_synth = "story_split_xverb_rev_src";
            let tgt_synth = "story_split_xverb_rev_tgt";
            declare_kr_synthdef(src_synth, &["out", "lfo"]);
            declare_synthdef_with_params(tgt_synth, &["freq"]);

            let mut src = make_voice("vox_src_xverb_rev").synth(src_synth.to_string());
            let mut tgt = make_voice("vox_tgt_xverb_rev").synth(tgt_synth.to_string());

            tgt.param_handle("freq")
                .modulate_by(src.clone(), "lfo".to_string())
                .expect("install bend");

            let err = src
                .output_by_name("out")
                .expect("port resolves")
                .to_param(tgt, "freq".to_string())
                .expect_err("cross-verb conflict must error");
            let msg = err.to_string();
            assert!(
                msg.contains("to_param"),
                "msg should be scoped to to_param, got: {}",
                msg,
            );

            // SET map must not have a leak.
            let src_id = context::get_or_create_voice_id("vox_src_xverb_rev");
            context::with_state(|state| {
                assert!(state
                    .param_routes_set
                    .get(&(src_id, "out".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn to_param_multi_source_on_same_target_errors() {
        // Multi-source on `.to_param` must error: two different source ports
        // pointing at the same `(target, param)` is meaningless under SET
        // (scsynth's /n_map only honours one source) and the script-time
        // check rejects it cleanly.
        with_test_context(|| {
            let src_synth = "story_split_mset_src";
            let tgt_synth = "story_split_mset_tgt";
            declare_kr_synthdef(src_synth, &["out", "alt"]);
            declare_synthdef_with_params(tgt_synth, &["freq"]);

            let mut src = make_voice("vox_src_mset").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_mset").synth(tgt_synth.to_string());

            src.clone()
                .output_by_name("out")
                .expect("port resolves")
                .to_param(tgt.clone(), "freq".to_string())
                .expect("first set");

            let err = src
                .output_by_name("alt")
                .expect("port resolves")
                .to_param(tgt, "freq".to_string())
                .expect_err("multi-source SET must error");
            let msg = err.to_string();
            assert!(
                msg.contains("to_param") && msg.contains("modulate_by"),
                "msg should suggest using modulate_by, got: {}",
                msg,
            );
        });
    }

    // ==================== .scale() / .offset() shaping tests ====================

    #[test]
    fn to_param_scale_round_trips_via_set_shaping_map() {
        with_test_context(|| {
            let src_synth = "v3_a1b_to_param_scale_src";
            let tgt_synth = "v3_a1b_to_param_scale_tgt";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_scale_rt").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_scale_rt").synth(tgt_synth.to_string());

            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect("install")
                .scale(0.5)
                .expect("scale install");

            let src_id = context::get_or_create_voice_id("vox_src_scale_rt");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_scale_rt");
            context::with_state(|state| {
                let key = (
                    src_id,
                    "env".to_string(),
                    ParamRouteTarget::Voice(tgt_id),
                    "cutoff".to_string(),
                );
                let (scale, offset) = state
                    .param_route_set_shaping
                    .get(&key)
                    .copied()
                    .expect("shaping entry installed");
                assert_eq!(scale, 0.5);
                assert_eq!(offset, 0.0, "offset must remain at default");
                // BEND map must not be touched.
                assert!(state.param_route_bend_shaping.get(&key).is_none());
            });
        });
    }

    #[test]
    fn modulate_by_offset_round_trips_via_bend_shaping_map() {
        with_test_context(|| {
            let src_synth = "v3_a1b_modulate_by_offset_src";
            let tgt_synth = "v3_a1b_modulate_by_offset_tgt";
            declare_kr_synthdef(src_synth, &["out"]);
            declare_synthdef_with_params(tgt_synth, &["freq"]);

            let src = make_voice("vox_src_offset_rt").synth(src_synth.to_string());
            let mut tgt = make_voice("vox_tgt_offset_rt").synth(tgt_synth.to_string());

            tgt.param_handle("freq")
                .modulate_by(src, "out".to_string())
                .expect("install")
                .offset(-0.25)
                .expect("offset install");

            let src_id = context::get_or_create_voice_id("vox_src_offset_rt");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_offset_rt");
            context::with_state(|state| {
                let key = (
                    src_id,
                    "out".to_string(),
                    ParamRouteTarget::Voice(tgt_id),
                    "freq".to_string(),
                );
                let (scale, offset) = state
                    .param_route_bend_shaping
                    .get(&key)
                    .copied()
                    .expect("shaping entry installed");
                assert_eq!(scale, 1.0, "scale must remain at default");
                assert_eq!(offset, -0.25);
                // SET map must not be touched.
                assert!(state.param_route_set_shaping.get(&key).is_none());
            });
        });
    }

    #[test]
    fn chained_scale_then_offset_round_trips_both() {
        with_test_context(|| {
            let src_synth = "v3_a1b_chain_scale_offset_src";
            let tgt_synth = "v3_a1b_chain_scale_offset_tgt";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_chain_so").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_chain_so").synth(tgt_synth.to_string());

            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect("install")
                .scale(2.0)
                .expect("scale install")
                .offset(0.125)
                .expect("offset install");

            let src_id = context::get_or_create_voice_id("vox_src_chain_so");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_chain_so");
            context::with_state(|state| {
                let key = (
                    src_id,
                    "env".to_string(),
                    ParamRouteTarget::Voice(tgt_id),
                    "cutoff".to_string(),
                );
                let (scale, offset) = state
                    .param_route_set_shaping
                    .get(&key)
                    .copied()
                    .expect("shaping entry installed");
                assert_eq!(scale, 2.0);
                assert_eq!(offset, 0.125);
            });
        });
    }

    #[test]
    fn to_param_without_scale_or_offset_uses_defaults() {
        with_test_context(|| {
            let src_synth = "v3_a1b_default_src";
            let tgt_synth = "v3_a1b_default_tgt";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_default").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_default").synth(tgt_synth.to_string());

            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect("install");

            let src_id = context::get_or_create_voice_id("vox_src_default");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_default");
            context::with_state(|state| {
                let key = (
                    src_id,
                    "env".to_string(),
                    ParamRouteTarget::Voice(tgt_id),
                    "cutoff".to_string(),
                );
                let (scale, offset) = state
                    .param_route_set_shaping
                    .get(&key)
                    .copied()
                    .expect("shaping entry seeded by add_param_route");
                assert_eq!(scale, 1.0);
                assert_eq!(offset, 0.0);
            });
        });
    }

    #[test]
    fn modulate_by_without_scale_or_offset_uses_defaults() {
        with_test_context(|| {
            let src_synth = "v3_a1b_modby_default_src";
            let tgt_synth = "v3_a1b_modby_default_tgt";
            declare_kr_synthdef(src_synth, &["out"]);
            declare_synthdef_with_params(tgt_synth, &["freq"]);

            let src = make_voice("vox_src_modby_default").synth(src_synth.to_string());
            let mut tgt = make_voice("vox_tgt_modby_default").synth(tgt_synth.to_string());

            tgt.param_handle("freq")
                .modulate_by(src, "out".to_string())
                .expect("install");

            let src_id = context::get_or_create_voice_id("vox_src_modby_default");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_modby_default");
            context::with_state(|state| {
                let key = (
                    src_id,
                    "out".to_string(),
                    ParamRouteTarget::Voice(tgt_id),
                    "freq".to_string(),
                );
                let (scale, offset) = state
                    .param_route_bend_shaping
                    .get(&key)
                    .copied()
                    .expect("shaping entry seeded by add_param_route");
                assert_eq!(scale, 1.0);
                assert_eq!(offset, 0.0);
            });
        });
    }

    #[test]
    fn last_scale_call_wins_on_repeated_invocations() {
        with_test_context(|| {
            let src_synth = "v3_a1b_last_wins_src";
            let tgt_synth = "v3_a1b_last_wins_tgt";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_lw").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_lw").synth(tgt_synth.to_string());

            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect("install")
                .scale(0.25)
                .expect("first scale")
                .scale(0.75)
                .expect("second scale — must overwrite the first");

            let src_id = context::get_or_create_voice_id("vox_src_lw");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_lw");
            context::with_state(|state| {
                let key = (
                    src_id,
                    "env".to_string(),
                    ParamRouteTarget::Voice(tgt_id),
                    "cutoff".to_string(),
                );
                let (scale, offset) = state
                    .param_route_set_shaping
                    .get(&key)
                    .copied()
                    .expect("shaping entry");
                assert_eq!(scale, 0.75, "second .scale() must win");
                assert_eq!(offset, 0.0);
            });
        });
    }

    #[test]
    fn scale_per_target_when_chained_to_param_calls_share_a_handle() {
        // Subtle: a single RouteHandle returned by .output(...) keeps
        // `last_param_target` tracking the most-recent .to_param(...). Chained
        // .to_param().scale().to_param().scale() applies each scale to its
        // respective target without bleed-through.
        with_test_context(|| {
            let src_synth = "v3_a1b_per_target_src";
            let tgt_synth_a = "v3_a1b_per_target_tgt_a";
            let tgt_synth_b = "v3_a1b_per_target_tgt_b";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth_a, &["cutoff"]);
            declare_synthdef_with_params(tgt_synth_b, &["pitch"]);

            let mut src = make_voice("vox_src_per_tgt").synth(src_synth.to_string());
            let tgt_a = make_voice("vox_tgt_per_tgt_a").synth(tgt_synth_a.to_string());
            let tgt_b = make_voice("vox_tgt_per_tgt_b").synth(tgt_synth_b.to_string());

            src.output_by_name("env")
                .expect("port resolves")
                .to_param(tgt_a, "cutoff".to_string())
                .expect("install A")
                .scale(0.5)
                .expect("scale A")
                .to_param(tgt_b, "pitch".to_string())
                .expect("install B")
                .scale(2.0)
                .expect("scale B");

            let src_id = context::get_or_create_voice_id("vox_src_per_tgt");
            let tgt_a_id = context::get_or_create_voice_id("vox_tgt_per_tgt_a");
            let tgt_b_id = context::get_or_create_voice_id("vox_tgt_per_tgt_b");
            context::with_state(|state| {
                let key_a = (
                    src_id,
                    "env".to_string(),
                    ParamRouteTarget::Voice(tgt_a_id),
                    "cutoff".to_string(),
                );
                let key_b = (
                    src_id,
                    "env".to_string(),
                    ParamRouteTarget::Voice(tgt_b_id),
                    "pitch".to_string(),
                );
                assert_eq!(
                    state.param_route_set_shaping.get(&key_a).copied(),
                    Some((0.5, 0.0)),
                );
                assert_eq!(
                    state.param_route_set_shaping.get(&key_b).copied(),
                    Some((2.0, 0.0)),
                );
            });
        });
    }

    // ==================== .to_param_audio() (ar→param coercion) tests ===================

    #[test]
    fn to_param_audio_round_trip_lands_in_set_map_from_ar_source() {
        with_test_context(|| {
            let src_synth = "v3_a2_to_param_audio_round_trip_src";
            let tgt_synth = "v3_a2_to_param_audio_round_trip_tgt";
            declare_ar_synthdef(src_synth, &["sine"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff", "amp"]);

            let mut src = make_voice("vox_src_tpa_rt").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_tpa_rt").synth(tgt_synth.to_string());

            src.output_by_name("sine")
                .expect("port resolves")
                .to_param_audio(tgt, "cutoff".to_string())
                .expect("ar port + valid target param");

            let src_id = context::get_or_create_voice_id("vox_src_tpa_rt");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_tpa_rt");
            context::with_state(|state| {
                let entries = state
                    .param_routes_set
                    .get(&(src_id, "sine".to_string()))
                    .expect("param route installed in SET map");
                assert_eq!(entries.len(), 1);
                assert_eq!(
                    entries[0],
                    (ParamRouteTarget::Voice(tgt_id), "cutoff".to_string())
                );

                // Default shaping seeded so chained .scale/.offset work.
                let key = (
                    src_id,
                    "sine".to_string(),
                    ParamRouteTarget::Voice(tgt_id),
                    "cutoff".to_string(),
                );
                let shaping = state.param_route_set_shaping.get(&key).copied();
                assert_eq!(shaping, Some((1.0, 0.0)));

                // Not in BEND map, no audio mixer route installed.
                assert!(state
                    .param_routes_bend
                    .get(&(src_id, "sine".to_string()))
                    .is_none());
                assert!(state.routes.get(&(src_id, "sine".to_string())).is_none());
            });
        });
    }

    #[test]
    fn to_param_audio_kr_source_returns_clean_error_pointing_at_to_param() {
        with_test_context(|| {
            let src_synth = "v3_a2_to_param_audio_kr_err_src";
            let tgt_synth = "v3_a2_to_param_audio_kr_err_tgt";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_tpa_kr").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_tpa_kr").synth(tgt_synth.to_string());

            let err = src
                .output_by_name("env")
                .expect("port resolves")
                .to_param_audio(tgt, "cutoff".to_string())
                .expect_err("kr-rate source must error");
            let msg = err.to_string();
            assert!(msg.contains("kr-rate"), "msg = {}", msg);
            assert!(msg.contains("'env'"), "msg = {}", msg);
            // The error must point users at the correct verb for kr sources.
            assert!(
                msg.contains(".to_param()"),
                "msg should suggest .to_param(), got: {}",
                msg,
            );

            // No partial route installed in either map.
            let src_id = context::get_or_create_voice_id("vox_src_tpa_kr");
            context::with_state(|state| {
                assert!(state
                    .param_routes_set
                    .get(&(src_id, "env".to_string()))
                    .is_none());
                assert!(state
                    .param_routes_bend
                    .get(&(src_id, "env".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn to_param_keeps_ar_rate_mismatch_error() {
        // Symmetric guard: the existing .to_param() rejection of ar sources
        // must not regress now that .to_param_audio() exists.
        with_test_context(|| {
            let src_synth = "v3_a2_to_param_ar_err_src";
            let tgt_synth = "v3_a2_to_param_ar_err_tgt";
            declare_ar_synthdef(src_synth, &["sine"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_tp_ar").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_tp_ar").synth(tgt_synth.to_string());

            let err = src
                .output_by_name("sine")
                .expect("port resolves")
                .to_param(tgt, "cutoff".to_string())
                .expect_err("ar source on .to_param must error");
            let msg = err.to_string();
            assert!(msg.contains("ar-rate"), "msg = {}", msg);
            assert!(msg.contains("'sine'"), "msg = {}", msg);
            assert!(msg.contains("output_kr"), "msg = {}", msg);
        });
    }

    #[test]
    fn to_param_audio_with_chained_scale_and_offset_round_trips() {
        // Per-source shaping must apply to .to_param_audio routes too —
        // the adapter feeds the same summer, so .scale() / .offset() write
        // into the same SET shaping map as pure-kr routes.
        with_test_context(|| {
            let src_synth = "v3_a2_tpa_shape_src";
            let tgt_synth = "v3_a2_tpa_shape_tgt";
            declare_ar_synthdef(src_synth, &["sine"]);
            declare_synthdef_with_params(tgt_synth, &["cutoff"]);

            let mut src = make_voice("vox_src_tpa_shape").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_tpa_shape").synth(tgt_synth.to_string());

            src.output_by_name("sine")
                .expect("port resolves")
                .to_param_audio(tgt, "cutoff".to_string())
                .expect("install")
                .scale(0.25)
                .expect("scale install")
                .offset(0.5)
                .expect("offset install");

            let src_id = context::get_or_create_voice_id("vox_src_tpa_shape");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_tpa_shape");
            context::with_state(|state| {
                let key = (
                    src_id,
                    "sine".to_string(),
                    ParamRouteTarget::Voice(tgt_id),
                    "cutoff".to_string(),
                );
                let (scale, offset) = state
                    .param_route_set_shaping
                    .get(&key)
                    .copied()
                    .expect("shaping recorded for ar→param route");
                assert_eq!(scale, 0.25);
                assert_eq!(offset, 0.5);
            });
        });
    }

    #[test]
    fn to_param_audio_then_modulate_by_on_same_target_errors() {
        // .to_param_audio shares the SET map with .to_param, so the same
        // cross-verb conflict guard must reject .modulate_by on a target
        // already wired by ar coercion.
        with_test_context(|| {
            let src_synth = "v3_a2_tpa_xverb_src";
            let tgt_synth = "v3_a2_tpa_xverb_tgt";
            declare_ar_synthdef(src_synth, &["sine"]);
            declare_kr_synthdef("v3_a2_tpa_xverb_lfo", &["out"]);
            declare_synthdef_with_params(tgt_synth, &["freq"]);

            let src = make_voice("vox_src_tpa_xverb").synth(src_synth.to_string());
            let lfo = make_voice("vox_lfo_tpa_xverb").synth("v3_a2_tpa_xverb_lfo".to_string());
            let mut tgt = make_voice("vox_tgt_tpa_xverb").synth(tgt_synth.to_string());

            src.clone()
                .output_by_name("sine")
                .expect("port resolves")
                .to_param_audio(tgt.clone(), "freq".to_string())
                .expect("install set via to_param_audio");

            let err = tgt
                .param_handle("freq")
                .modulate_by(lfo, "out".to_string())
                .expect_err("cross-verb conflict must error");
            let msg = err.to_string();
            assert!(msg.contains("modulate_by"), "msg = {}", msg);
            assert!(msg.contains("to_param"), "msg = {}", msg);
        });
    }

    #[test]
    fn scale_before_to_param_errors_with_clean_message() {
        with_test_context(|| {
            let src_synth = "v3_a1b_no_prior_src";
            declare_kr_synthdef(src_synth, &["env"]);

            let mut src = make_voice("vox_src_no_prior").synth(src_synth.to_string());
            let err = src
                .output_by_name("env")
                .expect("port resolves")
                .scale(0.5)
                .expect_err("scale before .to_param must error");
            let msg = err.to_string();
            assert!(
                msg.contains("scale") && msg.contains("to_param"),
                "msg should mention both scale and to_param, got: {}",
                msg,
            );
        });
    }

    // ==================== .to_trigger tests (v3 B2.c) ====================

    fn declare_tr_synthdef(synth: &str, tr_ports: &[&str]) {
        let outputs: Vec<OutputPort> = tr_ports
            .iter()
            .map(|n| OutputPort {
                name: (*n).to_string(),
                channels: 1,
                rate: PortRate::Tr,
            })
            .collect();
        register_synthdef_outputs(synth.to_string(), outputs);
    }

    #[test]
    fn to_trigger_round_trips_into_param_routes_trigger() {
        with_test_context(|| {
            let src_synth = "v3_b2c_to_trigger_round_trip_src";
            let tgt_synth = "v3_b2c_to_trigger_round_trip_tgt";
            declare_tr_synthdef(src_synth, &["kick_trig"]);
            declare_synthdef_with_params(tgt_synth, &["gate", "amp"]);

            let mut src = make_voice("vox_src_b2c_rt").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_b2c_rt").synth(tgt_synth.to_string());

            src.output_by_name("kick_trig")
                .expect("port resolves")
                .to_trigger(tgt, "gate".to_string())
                .expect("tr port + valid target param");

            let src_id = context::get_or_create_voice_id("vox_src_b2c_rt");
            let tgt_id = context::get_or_create_voice_id("vox_tgt_b2c_rt");
            context::with_state(|state| {
                let entries = state
                    .param_routes_trigger
                    .get(&(src_id, "kick_trig".to_string()))
                    .expect("trigger route installed");
                assert_eq!(entries.len(), 1);
                assert_eq!(
                    entries[0],
                    (ParamRouteTarget::Voice(tgt_id), "gate".to_string())
                );

                // SET / BEND maps stay empty.
                assert!(state
                    .param_routes_set
                    .get(&(src_id, "kick_trig".to_string()))
                    .is_none());
                assert!(state
                    .param_routes_bend
                    .get(&(src_id, "kick_trig".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn to_trigger_kr_rate_source_errors_with_clean_message() {
        with_test_context(|| {
            let src_synth = "v3_b2c_to_trigger_kr_src";
            let tgt_synth = "v3_b2c_to_trigger_kr_tgt";
            declare_kr_synthdef(src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["gate"]);

            let mut src = make_voice("vox_src_b2c_kr").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_b2c_kr").synth(tgt_synth.to_string());

            let err = src
                .output_by_name("env")
                .expect("port resolves")
                .to_trigger(tgt, "gate".to_string())
                .expect_err("kr-rate source on .to_trigger must error");
            let msg = err.to_string();
            assert!(msg.contains("kr-rate"), "msg = {}", msg);
            assert!(msg.contains("'env'"), "msg = {}", msg);
            assert!(msg.contains("output_tr"), "msg = {}", msg);
            assert!(
                msg.contains("to_param()"),
                "msg should hint at .to_param, got: {}",
                msg
            );

            let src_id = context::get_or_create_voice_id("vox_src_b2c_kr");
            context::with_state(|state| {
                assert!(state
                    .param_routes_trigger
                    .get(&(src_id, "env".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn to_trigger_ar_rate_source_errors_with_clean_message() {
        with_test_context(|| {
            let src_synth = "v3_b2c_to_trigger_ar_src";
            let tgt_synth = "v3_b2c_to_trigger_ar_tgt";
            declare_ar_synthdef(src_synth, &["sine"]);
            declare_synthdef_with_params(tgt_synth, &["gate"]);

            let mut src = make_voice("vox_src_b2c_ar").synth(src_synth.to_string());
            let tgt = make_voice("vox_tgt_b2c_ar").synth(tgt_synth.to_string());

            let err = src
                .output_by_name("sine")
                .expect("port resolves")
                .to_trigger(tgt, "gate".to_string())
                .expect_err("ar-rate source on .to_trigger must error");
            let msg = err.to_string();
            assert!(msg.contains("ar-rate"), "msg = {}", msg);
            assert!(msg.contains("'sine'"), "msg = {}", msg);
            assert!(msg.contains("output_tr"), "msg = {}", msg);
            assert!(
                msg.contains("to(") || msg.contains("to_main") || msg.contains("to_param_audio"),
                "msg should hint at audio routing verbs, got: {}",
                msg,
            );
        });
    }

    #[test]
    fn to_trigger_cross_verb_conflict_with_set_errors() {
        with_test_context(|| {
            let kr_src_synth = "v3_b2c_to_trigger_cross_set_kr";
            let tr_src_synth = "v3_b2c_to_trigger_cross_set_tr";
            let tgt_synth = "v3_b2c_to_trigger_cross_set_tgt";
            declare_kr_synthdef(kr_src_synth, &["env"]);
            declare_tr_synthdef(tr_src_synth, &["trig"]);
            declare_synthdef_with_params(tgt_synth, &["gate"]);

            let mut kr_src = make_voice("vox_kr_src_cross_set").synth(kr_src_synth.to_string());
            let mut tr_src = make_voice("vox_tr_src_cross_set").synth(tr_src_synth.to_string());
            let tgt = make_voice("vox_tgt_cross_set").synth(tgt_synth.to_string());

            // Install SET first.
            kr_src
                .output_by_name("env")
                .expect("port resolves")
                .to_param(tgt.clone(), "gate".to_string())
                .expect("install kr→param SET");

            // Now .to_trigger on the same target (gate) — must error.
            let err = tr_src
                .output_by_name("trig")
                .expect("port resolves")
                .to_trigger(tgt, "gate".to_string())
                .expect_err("cross-verb conflict (SET ↔ TRIGGER) must error");
            let msg = err.to_string();
            assert!(msg.contains("to_trigger"), "msg = {}", msg);
            assert!(
                msg.contains("to_param") || msg.contains("modulate_by"),
                "msg should reference the conflicting verb set, got: {}",
                msg,
            );
        });
    }

    #[test]
    fn to_trigger_cross_verb_conflict_with_bend_errors() {
        with_test_context(|| {
            let kr_src_synth = "v3_b2c_to_trigger_cross_bend_kr";
            let tr_src_synth = "v3_b2c_to_trigger_cross_bend_tr";
            let tgt_synth = "v3_b2c_to_trigger_cross_bend_tgt";
            declare_kr_synthdef(kr_src_synth, &["lfo"]);
            declare_tr_synthdef(tr_src_synth, &["trig"]);
            declare_synthdef_with_params(tgt_synth, &["gate"]);

            let kr_src = make_voice("vox_kr_src_cross_bend").synth(kr_src_synth.to_string());
            let mut tr_src = make_voice("vox_tr_src_cross_bend").synth(tr_src_synth.to_string());
            let mut tgt = make_voice("vox_tgt_cross_bend").synth(tgt_synth.to_string());

            // Install BEND first via target-first surface.
            tgt.param_handle("gate")
                .modulate_by(kr_src, "lfo".to_string())
                .expect("install kr→param BEND");

            // Now .to_trigger on the same target — must error.
            let err = tr_src
                .output_by_name("trig")
                .expect("port resolves")
                .to_trigger(tgt, "gate".to_string())
                .expect_err("cross-verb conflict (BEND ↔ TRIGGER) must error");
            let msg = err.to_string();
            assert!(msg.contains("to_trigger"), "msg = {}", msg);
        });
    }

    #[test]
    fn set_after_trigger_on_same_target_errors_with_cross_verb_conflict() {
        with_test_context(|| {
            let tr_src_synth = "v3_b2c_set_after_trigger_tr";
            let kr_src_synth = "v3_b2c_set_after_trigger_kr";
            let tgt_synth = "v3_b2c_set_after_trigger_tgt";
            declare_tr_synthdef(tr_src_synth, &["trig"]);
            declare_kr_synthdef(kr_src_synth, &["env"]);
            declare_synthdef_with_params(tgt_synth, &["gate"]);

            let mut tr_src = make_voice("vox_tr_src_sat").synth(tr_src_synth.to_string());
            let mut kr_src = make_voice("vox_kr_src_sat").synth(kr_src_synth.to_string());
            let tgt = make_voice("vox_tgt_sat").synth(tgt_synth.to_string());

            // Install TRIGGER first.
            tr_src
                .output_by_name("trig")
                .expect("port resolves")
                .to_trigger(tgt.clone(), "gate".to_string())
                .expect("install tr→param TRIGGER");

            // Now .to_param on the same target — must error.
            let err = kr_src
                .output_by_name("env")
                .expect("port resolves")
                .to_param(tgt, "gate".to_string())
                .expect_err("cross-verb conflict (TRIGGER ↔ SET) must error");
            let msg = err.to_string();
            assert!(msg.contains("to_param"), "msg = {}", msg);
        });
    }

    // ==================== InputHandle / .from / .disconnect tests (P2.1) ====================

    use crate::api::group::GroupHandle;
    use vibelang_core::handlers::InputRouteSrc;

    #[test]
    fn input_from_voice_writes_single_source_entry() {
        with_test_context(|| {
            let src_synth = "p2_1_input_from_voice_src";
            let tgt_synth = "p2_1_input_from_voice_tgt";
            // No declared port set needed for inputs (no synthdef-input
            // registry yet); the source-side default port name is hardcoded
            // to "out" by `.from(voice)`.
            declare_ar_synthdef(src_synth, &["out"]);
            declare_synthdef_with_params(tgt_synth, &["amp"]);

            let pad = make_voice("vox_pad_input_rt").synth(src_synth.to_string());
            let mut voc = make_voice("vox_voc_input_rt").synth(tgt_synth.to_string());

            voc.input("carrier").from_voice(pad);

            let pad_id = context::get_or_create_voice_id("vox_pad_input_rt");
            let voc_id = context::get_or_create_voice_id("vox_voc_input_rt");
            context::with_state(|state| {
                let entries = state
                    .input_routes
                    .get(&(voc_id, "carrier".to_string()))
                    .expect("input route installed");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0], InputRouteSrc::Voice(pad_id, "out".to_string()));
            });
        });
    }

    #[test]
    fn input_from_voice_repeated_replaces_prior_source() {
        with_test_context(|| {
            let src_synth = "p2_1_input_replace_src";
            let tgt_synth = "p2_1_input_replace_tgt";
            declare_ar_synthdef(src_synth, &["out"]);
            declare_synthdef_with_params(tgt_synth, &["amp"]);

            let pad = make_voice("vox_pad_replace").synth(src_synth.to_string());
            let other = make_voice("vox_other_replace").synth(src_synth.to_string());
            let mut voc = make_voice("vox_voc_replace").synth(tgt_synth.to_string());

            voc.input("carrier").from_voice(pad);
            voc.input("carrier").from_voice(other);

            let other_id = context::get_or_create_voice_id("vox_other_replace");
            let voc_id = context::get_or_create_voice_id("vox_voc_replace");
            context::with_state(|state| {
                let entries = state
                    .input_routes
                    .get(&(voc_id, "carrier".to_string()))
                    .expect("input route installed");
                // Vec stays length 1 — single-source replace, not fan-in.
                assert_eq!(entries.len(), 1, "replace, not fan-in");
                assert_eq!(
                    entries[0],
                    InputRouteSrc::Voice(other_id, "out".to_string()),
                    "second .from(...) must win over the first",
                );
            });
        });
    }

    #[test]
    fn input_disconnect_writes_silent_source() {
        with_test_context(|| {
            let tgt_synth = "p2_1_input_disconnect_tgt";
            declare_synthdef_with_params(tgt_synth, &["amp"]);

            let mut voc = make_voice("vox_voc_disconnect").synth(tgt_synth.to_string());

            voc.input("carrier").disconnect();

            let voc_id = context::get_or_create_voice_id("vox_voc_disconnect");
            context::with_state(|state| {
                let entries = state
                    .input_routes
                    .get(&(voc_id, "carrier".to_string()))
                    .expect("disconnect installs an entry, not a removal");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0], InputRouteSrc::Silent);
            });
        });
    }

    #[test]
    fn input_from_voice_then_disconnect_replaces_with_silent() {
        with_test_context(|| {
            let src_synth = "p2_1_input_from_then_disc_src";
            let tgt_synth = "p2_1_input_from_then_disc_tgt";
            declare_ar_synthdef(src_synth, &["out"]);
            declare_synthdef_with_params(tgt_synth, &["amp"]);

            let pad = make_voice("vox_pad_disc_after").synth(src_synth.to_string());
            let mut voc = make_voice("vox_voc_disc_after").synth(tgt_synth.to_string());

            voc.input("carrier").from_voice(pad);
            voc.input("carrier").disconnect();

            let voc_id = context::get_or_create_voice_id("vox_voc_disc_after");
            context::with_state(|state| {
                let entries = state
                    .input_routes
                    .get(&(voc_id, "carrier".to_string()))
                    .expect("entry present");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0], InputRouteSrc::Silent);
            });
        });
    }

    #[test]
    fn input_from_group_writes_group_source() {
        with_test_context(|| {
            let tgt_synth = "p2_1_input_from_group_tgt";
            declare_synthdef_with_params(tgt_synth, &["amp"]);

            let mut voc = make_voice("vox_voc_from_group").synth(tgt_synth.to_string());

            let bus_group = GroupHandle::new("kit".to_string());
            let expected_gid = context::get_or_create_group_id("kit");
            voc.input("carrier").from_group(bus_group);

            let voc_id = context::get_or_create_voice_id("vox_voc_from_group");
            context::with_state(|state| {
                let entries = state
                    .input_routes
                    .get(&(voc_id, "carrier".to_string()))
                    .expect("input route installed");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0], InputRouteSrc::Group(expected_gid));
            });
        });
    }

    #[test]
    fn input_from_current_group_pins_to_voices_group() {
        with_test_context(|| {
            let tgt_synth = "p2_1_input_fcg_tgt";
            declare_synthdef_with_params(tgt_synth, &["amp"]);

            // `.group("kit")` syncs the voice with `group_path = "kit"`, so
            // state.voices[voc].group resolves to the kit GroupId.
            let mut voc = make_voice("vox_voc_fcg")
                .synth(tgt_synth.to_string())
                .group("kit".to_string());

            let expected_gid = context::get_or_create_group_id("kit");

            voc.input("carrier")
                .from_current_group()
                .expect("voice has explicit group");

            let voc_id = context::get_or_create_voice_id("vox_voc_fcg");
            context::with_state(|state| {
                let entries = state
                    .input_routes
                    .get(&(voc_id, "carrier".to_string()))
                    .expect("input route installed");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0], InputRouteSrc::Group(expected_gid));
            });
        });
    }

    #[test]
    fn input_from_current_group_errors_when_voice_at_root() {
        with_test_context(|| {
            let tgt_synth = "p2_1_input_fcg_root_tgt";
            declare_synthdef_with_params(tgt_synth, &["amp"]);

            // No `.group(...)` → voice lives at the implicit root.
            let mut voc = make_voice("vox_voc_fcg_root").synth(tgt_synth.to_string());

            let err = voc
                .input("carrier")
                .from_current_group()
                .expect_err("no explicit group must error");
            let msg = err.to_string();
            assert!(msg.contains("from_current_group"), "msg = {}", msg);
            assert!(msg.contains("'carrier'"), "msg = {}", msg);
            assert!(msg.contains("group("), "msg = {}", msg);

            // No partial entry installed.
            let voc_id = context::get_or_create_voice_id("vox_voc_fcg_root");
            context::with_state(|state| {
                assert!(state
                    .input_routes
                    .get(&(voc_id, "carrier".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn input_handle_does_not_touch_output_routes() {
        with_test_context(|| {
            let src_synth = "p2_1_input_no_output_leak_src";
            let tgt_synth = "p2_1_input_no_output_leak_tgt";
            declare_ar_synthdef(src_synth, &["out"]);
            declare_synthdef_with_params(tgt_synth, &["amp"]);

            let pad = make_voice("vox_pad_no_leak").synth(src_synth.to_string());
            let mut voc = make_voice("vox_voc_no_leak").synth(tgt_synth.to_string());

            voc.input("carrier").from_voice(pad);

            let pad_id = context::get_or_create_voice_id("vox_pad_no_leak");
            let voc_id = context::get_or_create_voice_id("vox_voc_no_leak");
            context::with_state(|state| {
                // Output-side routes/param-route maps stay empty.
                assert!(state.routes.get(&(pad_id, "out".to_string())).is_none());
                assert!(state
                    .param_routes_set
                    .get(&(pad_id, "out".to_string()))
                    .is_none());
                assert!(state
                    .param_routes_bend
                    .get(&(pad_id, "out".to_string()))
                    .is_none());
                // Input-route map carries the wiring.
                assert!(state
                    .input_routes
                    .contains_key(&(voc_id, "carrier".to_string())));
            });
        });
    }
}
