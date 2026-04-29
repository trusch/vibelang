//! Per-voice output routing.
//!
//! Each voice declares one or more output ports (named audio buses, allocated
//! by Story 2's [`VoiceState::output_buses`](crate::state::VoiceState::output_buses)).
//! A `RouteDest` says where a single port's signal should go: into a group's
//! mix bus, straight to main, or discarded.
//!
//! Story 6a wires up the type registry and the diff machinery only —
//! [`RoutesHandler::finalize`] is a logging stub. Story 6b emits the mixer
//! synths that actually realize the route changes.

use crate::types::{GroupId, VoiceId};
use std::collections::HashMap;

/// Where a voice's output port should send its audio.
///
/// - `Group(id)` mixes into the named group's audio bus.
/// - `Main` sends directly to bus 0 (the hardware main output), bypassing groups.
/// - `Muted` discards the signal.
///
/// CV-to-param routing is deferred to a later story.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RouteDest {
    Group(GroupId),
    Main,
    Muted,
}

/// A single concrete route: one voice's named port → one destination.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Route {
    pub voice_id: VoiceId,
    pub port_name: String,
    pub dest: RouteDest,
}

/// Per-voice route registry, keyed by `(voice_id, port_name)`.
///
/// Stored on [`ScriptState`](crate::reload::ScriptState) so script-side mutations
/// (Rhai surface lands in Story 8) can carry the desired route map across reloads.
pub type RouteMap = HashMap<(VoiceId, String), RouteDest>;

/// Description of how an existing route's destination changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteChange {
    pub voice_id: VoiceId,
    pub port_name: String,
    pub old_dest: RouteDest,
    pub new_dest: RouteDest,
}

/// Difference between two route maps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteDiff {
    /// Keys present in `new` but not in `old`.
    pub additions: Vec<Route>,
    /// Keys present in `old` but not in `new`. Carries the prior `dest`.
    pub removals: Vec<Route>,
    /// Keys present in both, with a different `dest`.
    pub changes: Vec<RouteChange>,
}

impl RouteDiff {
    /// True when there is nothing to apply.
    pub fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.removals.is_empty() && self.changes.is_empty()
    }
}

/// Computes and applies per-voice routing changes.
///
/// Story 6a only ships the diff; [`RoutesHandler::finalize`] is a logging
/// stub until Story 6b wires up the mixer synths.
pub struct RoutesHandler;

impl RoutesHandler {
    /// Compute the additions, removals, and destination changes between two route maps.
    pub fn diff(old: &RouteMap, new: &RouteMap) -> RouteDiff {
        let mut diff = RouteDiff::default();

        for ((voice_id, port_name), new_dest) in new {
            match old.get(&(*voice_id, port_name.clone())) {
                None => diff.additions.push(Route {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    dest: new_dest.clone(),
                }),
                Some(old_dest) if old_dest != new_dest => diff.changes.push(RouteChange {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    old_dest: old_dest.clone(),
                    new_dest: new_dest.clone(),
                }),
                _ => {}
            }
        }

        for ((voice_id, port_name), old_dest) in old {
            if !new.contains_key(&(*voice_id, port_name.clone())) {
                diff.removals.push(Route {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    dest: old_dest.clone(),
                });
            }
        }

        diff
    }

    /// Apply a route diff.
    ///
    /// **Story 6a stub** — logs each addition / removal / change at debug level
    /// but performs no synth emission. Story 6b will replace this with mixer
    /// synth creation and teardown driven by the diff.
    pub fn finalize(diff: &RouteDiff) {
        for r in &diff.additions {
            tracing::debug!(
                "RoutesHandler::finalize (stub) add: voice={:?} port={:?} -> {:?}",
                r.voice_id,
                r.port_name,
                r.dest
            );
        }
        for r in &diff.removals {
            tracing::debug!(
                "RoutesHandler::finalize (stub) remove: voice={:?} port={:?} (was {:?})",
                r.voice_id,
                r.port_name,
                r.dest
            );
        }
        for c in &diff.changes {
            tracing::debug!(
                "RoutesHandler::finalize (stub) change: voice={:?} port={:?} {:?} -> {:?}",
                c.voice_id,
                c.port_name,
                c.old_dest,
                c.new_dest
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_map(entries: &[((u32, &str), RouteDest)]) -> RouteMap {
        entries
            .iter()
            .map(|((vid, port), dest)| ((VoiceId::new(*vid), (*port).to_string()), dest.clone()))
            .collect()
    }

    #[test]
    fn diff_empty_old_to_n_entries_returns_n_additions() {
        let old = RouteMap::new();
        let new = make_map(&[
            ((1, "out"), RouteDest::Group(GroupId::new(1))),
            ((2, "left"), RouteDest::Main),
            ((2, "right"), RouteDest::Muted),
        ]);

        let diff = RoutesHandler::diff(&old, &new);

        assert_eq!(diff.additions.len(), 3);
        assert!(diff.removals.is_empty());
        assert!(diff.changes.is_empty());
    }

    #[test]
    fn diff_identical_returns_empty() {
        let routes = make_map(&[
            ((1, "out"), RouteDest::Group(GroupId::new(1))),
            ((2, "left"), RouteDest::Main),
            ((2, "right"), RouteDest::Muted),
        ]);

        let diff = RoutesHandler::diff(&routes, &routes);
        assert!(diff.is_empty());
    }

    #[test]
    fn diff_addition_returns_one_addition_only() {
        let old = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(1)))]);
        let new = make_map(&[
            ((1, "out"), RouteDest::Group(GroupId::new(1))),
            ((2, "out"), RouteDest::Main),
        ]);

        let diff = RoutesHandler::diff(&old, &new);

        assert_eq!(diff.additions.len(), 1);
        let added = &diff.additions[0];
        assert_eq!(added.voice_id, VoiceId::new(2));
        assert_eq!(added.port_name, "out");
        assert_eq!(added.dest, RouteDest::Main);
        assert!(diff.removals.is_empty());
        assert!(diff.changes.is_empty());
    }

    #[test]
    fn diff_removal_returns_one_removal_only() {
        let old = make_map(&[
            ((1, "out"), RouteDest::Group(GroupId::new(1))),
            ((2, "out"), RouteDest::Main),
        ]);
        let new = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(1)))]);

        let diff = RoutesHandler::diff(&old, &new);

        assert!(diff.additions.is_empty());
        assert_eq!(diff.removals.len(), 1);
        let removed = &diff.removals[0];
        assert_eq!(removed.voice_id, VoiceId::new(2));
        assert_eq!(removed.port_name, "out");
        assert_eq!(removed.dest, RouteDest::Main);
        assert!(diff.changes.is_empty());
    }

    #[test]
    fn diff_changed_dest_returns_one_change_only() {
        let old = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(1)))]);
        let new = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(2)))]);

        let diff = RoutesHandler::diff(&old, &new);

        assert!(diff.additions.is_empty());
        assert!(diff.removals.is_empty());
        assert_eq!(diff.changes.len(), 1);
        let chg = &diff.changes[0];
        assert_eq!(chg.voice_id, VoiceId::new(1));
        assert_eq!(chg.port_name, "out");
        assert_eq!(chg.old_dest, RouteDest::Group(GroupId::new(1)));
        assert_eq!(chg.new_dest, RouteDest::Group(GroupId::new(2)));
    }

    #[test]
    fn diff_changed_dest_main_to_muted() {
        // Cross-variant change: not just GroupId-to-GroupId.
        let old = make_map(&[((3, "fx_send"), RouteDest::Main)]);
        let new = make_map(&[((3, "fx_send"), RouteDest::Muted)]);

        let diff = RoutesHandler::diff(&old, &new);

        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].old_dest, RouteDest::Main);
        assert_eq!(diff.changes[0].new_dest, RouteDest::Muted);
    }

    #[test]
    fn diff_finalize_stub_runs_without_panic() {
        let old = RouteMap::new();
        let new = make_map(&[
            ((1, "out"), RouteDest::Group(GroupId::new(1))),
            ((2, "out"), RouteDest::Main),
        ]);
        let diff = RoutesHandler::diff(&old, &new);
        // Stub must not panic on any combination of additions/removals/changes.
        RoutesHandler::finalize(&diff);
    }
}
