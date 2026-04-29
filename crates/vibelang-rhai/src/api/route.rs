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
        context::with_state(|state| {
            state.set_route(self.voice_id, self.port_name.clone(), RouteDest::Group(group_id));
        });
        self
    }

    /// Install a route straight to the main hardware output (bus 0), bypassing
    /// any group routing.
    pub fn to_main(self) -> Self {
        context::with_state(|state| {
            state.set_route(self.voice_id, self.port_name.clone(), RouteDest::Main);
        });
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
                context::with_state(|state| {
                    state.set_route(
                        self.voice_id,
                        self.port_name.clone(),
                        RouteDest::Group(gid),
                    );
                });
                Ok(self)
            }
            _ => Err(no_current_group_error(&self.port_name)),
        }
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
