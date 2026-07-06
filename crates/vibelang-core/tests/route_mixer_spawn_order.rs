//! Route mixer spawn order — integration tests.
//!
//! scsynth evaluates nodes in tree order and `In.ar` reads zeros from buses
//! written later in the cycle. A group's link synth is Tail-added once at the
//! first FinalizeGroups and never respawned; effects insert `Before` the link.
//! Route mixer synths write the buses that both of those read, so a mixer
//! spawned *after* the link synth exists (hot-reload adding a voice/route to
//! an existing group, or a structural voice recreate respawning its mixers)
//! must not be Tail-added — it would land after its readers and the routed
//! audio would be silently lost (or bypass the fx chain).
//!
//! These tests drive [`vibelang_core::handlers::RoutesHandler::finalize`]
//! against a mock backend that records the `/s_new` add-action and target,
//! asserting the placement ladder in `spawn_route`:
//!
//! 1. first build (no link, no fx)  → `Tail` on the group node,
//! 2. link synth already exists     → `Before` the link synth,
//! 3. fx chain present              → `Before` the group's first effect.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{Route, RouteDest, RouteDiff, RoutesHandler};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, BusId, EffectId, EffectState, GroupId, GroupState,
    NodeId, ParamMap, State, VoiceConfig, VoiceId, VoiceRole, VoiceState,
};
use vibelang_dsp::{OutputPort, PortRate};

const SRC_SYNTH: &str = "route_order_src_synth";
const FX_SYNTH: &str = "route_order_fx_synth";

#[derive(Debug)]
struct MockError;
impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}
impl std::error::Error for MockError {}

#[derive(Debug, Clone)]
struct CreateCall {
    def: String,
    #[allow(dead_code)]
    node: NodeId,
    target: NodeId,
    action: AddAction,
}

struct MockBackend {
    creates: Mutex<Vec<CreateCall>>,
    free_count: AtomicU32,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            creates: Mutex::new(Vec::new()),
            free_count: AtomicU32::new(0),
        })
    }
    fn creates(&self) -> Vec<CreateCall> {
        self.creates.lock().unwrap().clone()
    }
    /// The recorded `/s_new` calls for `port_to_group_link_*` mixer synths.
    fn mixer_creates(&self) -> Vec<CreateCall> {
        self.creates()
            .into_iter()
            .filter(|c| c.def.starts_with("port_to_group_link_"))
            .collect()
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
        target: NodeId,
        action: AddAction,
        _params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.lock().unwrap().push(CreateCall {
            def: def.to_string(),
            node,
            target,
            action,
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
        _node: NodeId,
        _param: &str,
        _bus: u32,
    ) -> Result<(), Self::Error> {
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

fn ar_port(name: &str, channels: u8) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels,
        rate: PortRate::Ar,
    }
}

/// Insert a group, optionally with a live link synth node (as if
/// FinalizeGroups already ran). Returns `(group_node, link_node)`.
async fn insert_group(
    state: &Arc<RwLock<State>>,
    group_id: GroupId,
    name: &str,
    with_link: bool,
) -> (NodeId, Option<NodeId>) {
    let mut s = state.write().await;
    let node = s.alloc_node_id().unwrap();
    let bus = s.alloc_audio_bus(2).unwrap();
    let link = if with_link {
        Some(s.alloc_node_id().unwrap())
    } else {
        None
    };
    s.groups.insert(
        group_id,
        GroupState {
            id: group_id,
            name: name.to_string(),
            parent: None,
            node_id: node,
            audio_bus: bus,
            link_synth_node_id: link,
            muted: false,
            soloed: false,
            params: ParamMap::new(),
            output_bus: None,
            output_channels: None,
        },
    );
    (node, link)
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

/// Insert an effect on `group` (as if `EffectsHandler::add` already ran),
/// returning its node id.
async fn insert_effect(
    state: &Arc<RwLock<State>>,
    effect_id: EffectId,
    group: GroupId,
    synthdef: &str,
) -> NodeId {
    let mut s = state.write().await;
    s.synthdefs.insert(synthdef.to_string());
    let node_id = s.alloc_node_id().unwrap();
    let audio_bus = s
        .groups
        .get(&group)
        .map(|g| g.audio_bus)
        .expect("group exists");
    s.effects.insert(
        effect_id,
        EffectState {
            id: effect_id,
            group,
            synthdef: synthdef.to_string(),
            node_id,
            audio_bus,
            params: ParamMap::new(),
        },
    );
    node_id
}

fn group_route_addition(voice: VoiceId, group: GroupId) -> RouteDiff {
    RouteDiff {
        additions: vec![Route {
            voice_id: voice,
            port_name: "out".to_string(),
            dest: RouteDest::Group(group),
        }],
        removals: vec![],
    }
}

// =========================================================================
// Test 1: first build — no link synth, no effects. Tail on the group node
// reproduces the cold-boot order (voices → mixers → effects → link, with
// effects and link Tail-added after routes finalize).
// =========================================================================

#[tokio::test]
async fn first_build_mixer_is_tail_added_to_group_node() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    let (group_node, _) = insert_group(&state, group, "g", false).await;

    let voice = VoiceId::new(10);
    insert_voice(&state, voice, group, SRC_SYNTH, &[ar_port("out", 2)]).await;

    handler
        .finalize(&group_route_addition(voice, group))
        .await
        .unwrap();

    let mixers = backend.mixer_creates();
    assert_eq!(mixers.len(), 1, "exactly one mixer spawned");
    assert_eq!(mixers[0].def, "port_to_group_link_2");
    assert_eq!(
        mixers[0].action,
        AddAction::Tail,
        "first-build mixer is Tail-added",
    );
    assert_eq!(
        mixers[0].target, group_node,
        "first-build mixer targets the group node",
    );
}

// =========================================================================
// Test 2: link synth already exists (hot-reload after FinalizeGroups) —
// the mixer must insert Before the link, not Tail after it, or the link's
// In.ar never sees the mixer's output and the routed voice is inaudible.
// =========================================================================

#[tokio::test]
async fn late_mixer_inserts_before_existing_link_synth() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    let (group_node, link) = insert_group(&state, group, "g", true).await;
    let link = link.expect("link synth node present");

    let voice = VoiceId::new(10);
    insert_voice(&state, voice, group, SRC_SYNTH, &[ar_port("out", 2)]).await;

    handler
        .finalize(&group_route_addition(voice, group))
        .await
        .unwrap();

    let mixers = backend.mixer_creates();
    assert_eq!(mixers.len(), 1, "exactly one mixer spawned");
    assert_eq!(
        mixers[0].action,
        AddAction::Before,
        "late mixer must insert Before the link synth, got {:?} on {:?}",
        mixers[0].action,
        mixers[0].target,
    );
    assert_eq!(
        mixers[0].target, link,
        "late mixer targets the link synth node, not the group ({:?})",
        group_node,
    );
}

// =========================================================================
// Test 3: fx chain present — the mixer must precede the FIRST effect in
// the chain, otherwise the routed voice bypasses part or all of the fx.
// =========================================================================

#[tokio::test]
async fn late_mixer_inserts_before_first_effect_in_chain() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    let (_, link) = insert_group(&state, group, "g", true).await;
    let _link = link.expect("link synth node present");

    // Two effects, created in chain order (ids are handed out monotonically
    // by the script layer, so the smaller id is the head of the chain).
    let fx_head = insert_effect(&state, EffectId::new(1), group, FX_SYNTH).await;
    let fx_tail = insert_effect(&state, EffectId::new(2), group, FX_SYNTH).await;
    assert_ne!(fx_head, fx_tail);

    let voice = VoiceId::new(10);
    insert_voice(&state, voice, group, SRC_SYNTH, &[ar_port("out", 2)]).await;

    handler
        .finalize(&group_route_addition(voice, group))
        .await
        .unwrap();

    let mixers = backend.mixer_creates();
    assert_eq!(mixers.len(), 1, "exactly one mixer spawned");
    assert_eq!(
        mixers[0].action,
        AddAction::Before,
        "mixer must insert Before the fx chain",
    );
    assert_eq!(
        mixers[0].target, fx_head,
        "mixer targets the FIRST effect node (head of the chain), not the \
         second effect ({:?}) or the link",
        fx_tail,
    );
}

// =========================================================================
// Test 4: effects on an unrelated group don't hijack placement — only the
// voice's own group's fx chain / link matter.
// =========================================================================

#[tokio::test]
async fn effects_on_other_groups_do_not_affect_placement() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    let (_, link) = insert_group(&state, group, "g", true).await;
    let link = link.expect("link synth node present");

    let other = GroupId::new(2);
    insert_group(&state, other, "other", true).await;
    let other_fx = insert_effect(&state, EffectId::new(1), other, FX_SYNTH).await;

    let voice = VoiceId::new(10);
    insert_voice(&state, voice, group, SRC_SYNTH, &[ar_port("out", 2)]).await;

    handler
        .finalize(&group_route_addition(voice, group))
        .await
        .unwrap();

    let mixers = backend.mixer_creates();
    assert_eq!(mixers.len(), 1, "exactly one mixer spawned");
    assert_eq!(mixers[0].action, AddAction::Before);
    assert_eq!(
        mixers[0].target, link,
        "placement consults the voice's own group, not the other group's \
         effect ({:?})",
        other_fx,
    );
}
