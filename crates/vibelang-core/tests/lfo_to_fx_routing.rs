//! Fx-target CV-to-param routing — integration tests.
//!
//! Multi-output v3 follow-up: `.to_param`, `.to_param_audio`, and
//! `.modulate_by` accept Effect targets in addition to Voice targets.
//! These tests drive the public seam between
//! [`vibelang_core::handlers::RoutesHandler::finalize_params`] and
//! [`vibelang_core::state::State::param_routes_set`] /
//! [`vibelang_core::state::State::param_routes_bend`] with the new
//! [`vibelang_core::handlers::ParamRouteTarget::Effect`] variant, asserting
//! that the same `/n_map`-via-summer pipeline carries through to the fx's
//! single node.
//!
//! The unknown-fx-param error path is covered by the Rhai-surface tests
//! that mirror the existing voice-target validation logic — see
//! `crates/vibelang-rhai/src/api/route.rs` (the Fx-overload tests).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{ParamRoute, ParamRouteDiff, ParamRouteTarget, RoutesHandler};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, BusId, EffectId, EffectState, GroupId, GroupState,
    NodeId, ParamMap, State, VoiceConfig, VoiceId, VoiceRole, VoiceState,
};
use vibelang_dsp::{OutputPort, PortRate};

const SRC_KR_SYNTH: &str = "lfo_to_fx_kr_src";
const SRC_AR_SYNTH: &str = "lfo_to_fx_ar_src";
const FX_SYNTH: &str = "lfo_to_fx_pan_synth";

#[derive(Debug)]
struct MockError;
impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}
impl std::error::Error for MockError {}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CreateCall {
    def: String,
    node: NodeId,
}

#[derive(Debug, Clone, PartialEq)]
struct MapCall {
    node: NodeId,
    param: String,
    bus: u32,
}

struct MockBackend {
    creates: Mutex<Vec<CreateCall>>,
    maps: Mutex<Vec<MapCall>>,
    free_count: AtomicU32,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            creates: Mutex::new(Vec::new()),
            maps: Mutex::new(Vec::new()),
            free_count: AtomicU32::new(0),
        })
    }
    fn maps(&self) -> Vec<MapCall> {
        self.maps.lock().unwrap().clone()
    }
    fn creates(&self) -> Vec<CreateCall> {
        self.creates.lock().unwrap().clone()
    }
}

#[async_trait]
impl Backend for MockBackend {
    type Error = MockError;
    async fn load_synthdef(&self, _name: &str, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn create_synth(
        &self,
        def: &str,
        node: NodeId,
        _target: NodeId,
        _action: AddAction,
        _params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.lock().unwrap().push(CreateCall {
            def: def.to_string(),
            node,
        });
        Ok(())
    }
    async fn create_group(
        &self,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn free_node(&self, _node: NodeId) -> Result<(), Self::Error> {
        self.free_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn set_param(&self, _node: NodeId, _param: &str, _value: f32) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn map_param_to_bus(
        &self,
        node: NodeId,
        param: &str,
        bus: u32,
    ) -> Result<(), Self::Error> {
        self.maps.lock().unwrap().push(MapCall {
            node,
            param: param.to_string(),
            bus,
        });
        Ok(())
    }
    async fn load_buffer(&self, _id: BufferId, _path: &Path) -> Result<BufferInfo, Self::Error> {
        Ok(BufferInfo {
            frames: 0,
            channels: 0,
            sample_rate: 0.0,
        })
    }
    async fn alloc_buffer(
        &self,
        _id: BufferId,
        frames: u32,
        channels: u16,
    ) -> Result<BufferInfo, Self::Error> {
        Ok(BufferInfo {
            frames,
            channels,
            sample_rate: 0.0,
        })
    }
    async fn write_buffer(&self, _id: BufferId, _path: &Path) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn free_buffer(&self, _id: BufferId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn current_time(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
}

fn kr_port(name: &str) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels: 1,
        rate: PortRate::Kr,
    }
}
fn ar_port(name: &str, channels: u8) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels,
        rate: PortRate::Ar,
    }
}

async fn insert_group(state: &Arc<RwLock<State>>, group_id: GroupId, name: &str) {
    let mut s = state.write().await;
    let node = s.alloc_node_id().unwrap();
    let bus = s.alloc_audio_bus(2).unwrap();
    s.groups.insert(
        group_id,
        GroupState {
            id: group_id,
            name: name.to_string(),
            parent: None,
            node_id: node,
            audio_bus: bus,
            link_synth_node_id: None,
            muted: false,
            soloed: false,
            params: ParamMap::new(),
            output_bus: None,
            output_channels: None,
        },
    );
}

async fn insert_voice(
    state: &Arc<RwLock<State>>,
    voice_id: VoiceId,
    voice_group: GroupId,
    synthdef: &str,
    ports: &[OutputPort],
) {
    let mut s = state.write().await;
    s.synthdefs.insert(synthdef.to_string());
    s.synthdef_outputs
        .insert(synthdef.to_string(), ports.to_vec());

    let mut output_buses = Vec::with_capacity(ports.len());
    for p in ports {
        let bus = match p.rate {
            PortRate::Ar => s.alloc_audio_bus(p.channels).unwrap(),
            PortRate::Kr | PortRate::Tr => BusId::new(s.alloc_control_bus().unwrap().raw()),
        };
        output_buses.push((p.name.clone(), bus));
    }

    s.voices.insert(
        voice_id,
        VoiceState {
            id: voice_id,
            config: VoiceConfig::new("v", synthdef, voice_group),
            role: VoiceRole::Audible,
            active_nodes: Vec::new(),
            note_nodes: HashMap::new(),
            round_robin_position: 0,
            pending_params: HashMap::new(),
            output_buses,
            input_buses: Vec::new(),
        },
    );
}

/// Insert an effect into `state.effects`, returning the effect's node id
/// (the single node `/n_map` will target).
async fn insert_effect(
    state: &Arc<RwLock<State>>,
    effect_id: EffectId,
    group: GroupId,
    synthdef: &str,
    initial_params: &[(&str, f32)],
) -> NodeId {
    let mut s = state.write().await;
    s.synthdefs.insert(synthdef.to_string());
    let node_id = s.alloc_node_id().unwrap();
    let audio_bus = s
        .groups
        .get(&group)
        .map(|g| g.audio_bus)
        .expect("group exists");
    let mut params = ParamMap::new();
    for (k, v) in initial_params {
        params.insert((*k).to_string(), *v);
    }
    s.effects.insert(
        effect_id,
        EffectState {
            name: String::new(),
            id: effect_id,
            group,
            synthdef: synthdef.to_string(),
            node_id,
            audio_bus,
            params,
        },
    );
    node_id
}

// =========================================================================
// Test 1: kr LFO → fx.pan via .to_param_audio (using ar source for coverage
// of the a2k_adapter spawn path).
// =========================================================================
//
// Mirrors the voice-target `cv_to_param_kr_drives_target_param_via_n_map`
// from `multi_output_v2_kr_routing.rs`, but the target is an Effect, so the
// summer's intermediate bus is `/n_map`-bound to the fx's single node.

#[tokio::test]
async fn lfo_to_fx_pan_via_to_param_audio() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    insert_group(&state, group, "g").await;

    // Source: ar-rate LFO output port (so we exercise the a2k_adapter path).
    let lfo = VoiceId::new(10);
    insert_voice(&state, lfo, group, SRC_AR_SYNTH, &[ar_port("out", 1)]).await;

    // Target: a fx node with a `pan` param (initial value 0.0, the
    // canonical fx synthdef shape — `fx_pan` in stdlib).
    let fx = EffectId::new(1);
    let fx_node = insert_effect(&state, fx, group, FX_SYNTH, &[("pan", 0.0)]).await;

    // Diff carries one SET addition pointing at the fx target.
    let mut diff = ParamRouteDiff::default();
    diff.additions.push(ParamRoute {
        source_voice: lfo,
        source_port: "out".to_string(),
        target: ParamRouteTarget::Effect(fx),
        target_param: "pan".to_string(),
    });

    handler
        .finalize_params(
            &diff,
            &ParamRouteDiff::default(),
            &ParamRouteDiff::default(),
        )
        .await
        .unwrap();

    // The summer is keyed by the fx-target tuple, just like voice targets.
    let summer_bus = state
        .read()
        .await
        .param_summers
        .get(&(ParamRouteTarget::Effect(fx), "pan".to_string()))
        .expect("summer registered for (Effect(fx), pan)")
        .bus
        .raw();

    // /n_map fires on the fx's single node — same backend call shape as
    // voice targets, just one node instead of N.
    let maps = backend.maps();
    let want = MapCall {
        node: fx_node,
        param: "pan".to_string(),
        bus: summer_bus,
    };
    assert!(
        maps.contains(&want),
        "missing /n_map for fx pan — got {:?}",
        maps,
    );

    // Route registry holds the fx-target entry (typed as Effect, not Voice).
    let s = state.read().await;
    let entries = s
        .param_routes_set
        .get(&(lfo, "out".to_string()))
        .expect("source key recorded");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0],
        (ParamRouteTarget::Effect(fx), "pan".to_string()),
    );

    // ar source → exactly one a2k_adapter_1 spawned for (lfo, "out").
    let creates = backend.creates();
    let adapters: Vec<&CreateCall> = creates
        .iter()
        .filter(|c| c.def == "a2k_adapter_1")
        .collect();
    assert_eq!(
        adapters.len(),
        1,
        "exactly one a2k adapter spawned per (source, port) pair",
    );
    assert!(
        s.ar_to_kr_adapters.contains_key(&(lfo, "out".to_string())),
        "adapter recorded in state",
    );
}

// =========================================================================
// Test 2: target-first BEND via fx.param("pan").modulate_by(voice, "out")
// using a kr source. Asserts the symmetric registry entry plus /n_map.
// =========================================================================

#[tokio::test]
async fn lfo_to_fx_via_modulate_by() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    insert_group(&state, group, "g").await;

    // kr-rate source, target-first BEND lives in param_routes_bend.
    let lfo = VoiceId::new(20);
    insert_voice(&state, lfo, group, SRC_KR_SYNTH, &[kr_port("out")]).await;

    let fx = EffectId::new(2);
    let fx_node = insert_effect(&state, fx, group, FX_SYNTH, &[("pan", 0.5)]).await;

    let mut bend_diff = ParamRouteDiff::default();
    bend_diff.additions.push(ParamRoute {
        source_voice: lfo,
        source_port: "out".to_string(),
        target: ParamRouteTarget::Effect(fx),
        target_param: "pan".to_string(),
    });

    handler
        .finalize_params(
            &ParamRouteDiff::default(),
            &bend_diff,
            &ParamRouteDiff::default(),
        )
        .await
        .unwrap();

    // Same summer infrastructure, keyed on the fx target.
    let s = state.read().await;
    let summer = s
        .param_summers
        .get(&(ParamRouteTarget::Effect(fx), "pan".to_string()))
        .expect("summer registered");
    assert_eq!(summer.sources.len(), 1, "single source in BEND summer");

    // /n_map points at the summer's intermediate bus on the fx node.
    let maps = backend.maps();
    assert!(maps
        .iter()
        .any(|m| m.node == fx_node && m.param == "pan" && m.bus == summer.bus.raw()));

    // BEND route entry recorded; SET map untouched.
    let bend_entries = s
        .param_routes_bend
        .get(&(lfo, "out".to_string()))
        .expect("bend source key recorded");
    assert_eq!(bend_entries.len(), 1);
    assert_eq!(
        bend_entries[0],
        (ParamRouteTarget::Effect(fx), "pan".to_string()),
    );
    assert!(s.param_routes_set.get(&(lfo, "out".to_string())).is_none());
}

// =========================================================================
// Test 3: take_effect_param_routes drains target-side fx entries on remove.
// =========================================================================
//
// Mirrors `take_voice_param_routes` for fx — when a fx is removed (group's
// effect chain edited), routes targeting it must drop from the registry.

#[tokio::test]
async fn take_effect_param_routes_drains_target_entries() {
    let state = Arc::new(RwLock::new(State::default()));

    let group = GroupId::new(1);
    insert_group(&state, group, "g").await;

    let src = VoiceId::new(30);
    insert_voice(&state, src, group, SRC_KR_SYNTH, &[kr_port("out")]).await;

    let fx_a = EffectId::new(10);
    let fx_b = EffectId::new(11);
    let _ = insert_effect(&state, fx_a, group, FX_SYNTH, &[("pan", 0.0)]).await;
    let _ = insert_effect(&state, fx_b, group, FX_SYNTH, &[("pan", 0.0)]).await;

    // Plant routes pointing at both fx targets, plus a voice-target entry
    // that must survive the fx_a drain.
    {
        let mut s = state.write().await;
        let voice_target = VoiceId::new(99);
        s.param_routes_set.insert(
            (src, "out".to_string()),
            vec![
                (ParamRouteTarget::Effect(fx_a), "pan".to_string()),
                (ParamRouteTarget::Effect(fx_b), "pan".to_string()),
                (ParamRouteTarget::Voice(voice_target), "freq".to_string()),
            ],
        );
        s.param_routes_bend.insert(
            (src, "out".to_string()),
            vec![(ParamRouteTarget::Effect(fx_a), "depth".to_string())],
        );
    }

    // Drain fx_a only.
    state.write().await.take_effect_param_routes(fx_a);

    let s = state.read().await;
    let set_entries = s
        .param_routes_set
        .get(&(src, "out".to_string()))
        .expect("source key still alive");
    assert_eq!(
        set_entries.len(),
        2,
        "fx_a entry pruned; fx_b + voice survive",
    );
    assert!(set_entries
        .iter()
        .all(|(t, _)| *t != ParamRouteTarget::Effect(fx_a)));
    assert!(set_entries.contains(&(ParamRouteTarget::Effect(fx_b), "pan".to_string())));

    // BEND map: fx_a was the only target, so the source key drops entirely.
    assert!(
        s.param_routes_bend.get(&(src, "out".to_string())).is_none(),
        "bend source key pruned when its only target was fx_a",
    );
}
