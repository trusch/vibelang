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
use vibelang_dsp::{effect_exists, get_all_effect_names};

use super::group::GroupHandle;
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
    /// Per-port FX chain (Story 6b) — empty until `.fx([...])` populates it.
    /// Terminal verbs (`to`, `to_main`, `to_current_group`) propagate the
    /// chain to [`ScriptState::set_route_fx_chain`] alongside the dest.
    fx_chain: Vec<String>,
}

impl RouteHandle {
    pub(crate) fn new(voice_id: VoiceId, port_name: String) -> Self {
        Self {
            voice_id,
            port_name,
            fx_chain: Vec::new(),
        }
    }

    /// Attach a per-port FX chain (Story 6b).
    ///
    /// `names` is a Rhai array of FX synthdef names. Each entry must be a
    /// string, and each name must already be registered in the FX registry
    /// (via `define_fx(...).body(...)`). On a typo or unknown name the
    /// chain is rejected with a Rhai error citing the offender, the closest
    /// known FX (Levenshtein), and the full available set.
    ///
    /// Chainable: returns the same handle so a terminal verb can finalize the
    /// route. The chain is stored on the handle and only committed to
    /// [`ScriptState`](vibelang_core::reload::ScriptState) once `.to(...)`,
    /// `.to_main()`, or `.to_current_group()` runs.
    pub fn fx(mut self, names: rhai::Array) -> Result<Self, Box<EvalAltResult>> {
        let mut chain = Vec::with_capacity(names.len());
        for entry in names {
            let name = if entry.is_string() {
                entry.into_string().unwrap_or_default()
            } else {
                return Err(invalid_fx_entry_error(&self.port_name, &entry));
            };
            if !effect_exists(&name) {
                return Err(unknown_fx_error(&self.port_name, &name));
            }
            chain.push(name);
        }
        self.fx_chain = chain;
        Ok(self)
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
    ///
    /// `.mute()` does not propagate any prior `.fx([...])` chain — muted
    /// routes drop their signal before any FX would run, so attaching FX is
    /// meaningless. Any previously stored chain on the same `(voice, port)`
    /// is cleared so reroutes don't leak FX from earlier installs.
    pub fn mute(self) -> Self {
        context::with_state(|state| {
            state.set_route(self.voice_id, self.port_name.clone(), RouteDest::Muted);
            state.set_route_fx_chain(self.voice_id, self.port_name.clone(), Vec::new());
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

    /// Commit the resolved destination + the accumulated FX chain to script state.
    ///
    /// Both writes are issued under the same script-state borrow so a route
    /// install never observes a half-applied state where dest and fx_chain
    /// disagree.
    fn commit(&self, dest: RouteDest) {
        context::with_state(|state| {
            state.set_route(self.voice_id, self.port_name.clone(), dest);
            state.set_route_fx_chain(
                self.voice_id,
                self.port_name.clone(),
                self.fx_chain.clone(),
            );
        });
    }
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

/// Error raised when a non-string entry shows up in the `.fx([...])` array.
fn invalid_fx_entry_error(port: &str, entry: &rhai::Dynamic) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "fx() on port '{}': every entry must be a string FX synthdef name, \
             got {}",
            port,
            entry.type_name()
        )
        .into(),
        Position::NONE,
    ))
}

/// Error raised when an FX name in `.fx([...])` isn't a registered FX synthdef.
///
/// The message names the offending entry, points the user at the closest
/// registered FX (best Levenshtein match) when one is within edit-distance 3,
/// and lists the full set of registered FX so a typo is fixable without
/// scrolling back to the synthdef definitions.
fn unknown_fx_error(port: &str, name: &str) -> Box<EvalAltResult> {
    let available = get_all_effect_names();
    let mut available_sorted = available.clone();
    available_sorted.sort();
    let closest = closest_match(name, &available_sorted);
    let avail_list = if available_sorted.is_empty() {
        "no FX synthdefs registered".to_string()
    } else {
        available_sorted
            .iter()
            .map(|n| format!("'{}'", n))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let suggestion = match closest {
        Some(s) => format!(" — did you mean '{}'?", s),
        None => String::new(),
    };
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "fx() on port '{}': '{}' is not a registered FX synthdef{} \
             (available: {})",
            port, name, suggestion, avail_list
        )
        .into(),
        Position::NONE,
    ))
}

/// Pick the registered FX whose name has the lowest Levenshtein distance to
/// the offender, returning `None` when the best match is too distant
/// (> 3 edits) or the candidate list is empty.
fn closest_match(target: &str, candidates: &[String]) -> Option<String> {
    let target_lower = target.to_lowercase();
    candidates
        .iter()
        .map(|c| (c.clone(), levenshtein(&target_lower, &c.to_lowercase())))
        .filter(|(_, d)| *d <= 3)
        .min_by_key(|(_, d)| *d)
        .map(|(name, _)| name)
}

/// Iterative two-row Levenshtein edit distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_bytes: Vec<char> = a.chars().collect();
    let b_bytes: Vec<char> = b.chars().collect();
    let (m, n) = (a_bytes.len(), b_bytes.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
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

/// Register the `RouteHandle` type and its terminal verbs with a Rhai engine.
pub fn register(engine: &mut Engine) {
    engine.build_type::<RouteHandle>();
    engine.register_fn("fx", RouteHandle::fx);
    engine.register_fn("to", RouteHandle::to);
    engine.register_fn("to_main", RouteHandle::to_main);
    engine.register_fn("mute", RouteHandle::mute);
    engine.register_fn("to_current_group", RouteHandle::to_current_group);

    engine.build_type::<MultiRouteHandle>();
    engine.register_fn("to", MultiRouteHandle::to);
    engine.register_fn("to_main", MultiRouteHandle::to_main);
    engine.register_fn("mute", MultiRouteHandle::mute);
    engine.register_fn("to_current_group", MultiRouteHandle::to_current_group);
}
