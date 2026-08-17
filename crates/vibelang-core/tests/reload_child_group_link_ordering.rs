//! A reload must never leave a child group's link synth AFTER the node that
//! reads its parent's bus.
//!
//! Board defect (super-gamma, 2026-08-17): after a run of preset recalls a
//! reload left `system_link_audio` nodes writing the parent's audio bus while
//! the parent's own link synth — the only reader of that bus — had already
//! run earlier in the same cycle. scsynth zeroes audio buses every block, so
//! the whole performance side went silent with every SynthDef loaded and
//! every synth alive. A recall could not repair it; only restarting the
//! engine could.
//!
//! The mock backend simulates the scsynth node tree (ordered children,
//! honouring AddActions and `/n_before`), and records the `inbus`/`outbus`
//! controls of every synth, so the assertion is the ENGINE'S OWN verdict:
//! `audio_path_breaks` over the live tree in evaluation order.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vibelang_core::backends::scsynth::{audio_path_breaks, AudioPathBreak, TreeSynth};
use vibelang_core::reload::{EffectConfig, GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, EffectId, GroupId, NodeId, ParamMap, ReloadMessage,
    Runtime, SynthDefMessage, VoiceConfig, VoiceId, VoiceMessage,
};

// =========================================================================
// Mock backend with a node-tree simulation.
// =========================================================================

#[derive(Debug)]
struct MockError(String);

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error: {}", self.0)
    }
}

impl std::error::Error for MockError {}

/// Minimal scsynth tree: ordered children per group, honouring AddActions
/// and `/n_before` moves. Root group is NodeId(0).
#[derive(Default)]
struct Tree {
    children: HashMap<NodeId, Vec<NodeId>>,
    parent: HashMap<NodeId, NodeId>,
    defs: HashMap<NodeId, String>,
    controls: HashMap<NodeId, ParamMap>,
    /// Things a real scsynth would have REFUSED. The mock is permissive by
    /// construction (a `HashMap` happily takes a node id twice), so anything
    /// the server would reject has to be recorded explicitly or the harness
    /// silently simulates a tree the board can never have.
    rejects: Vec<String>,
}

impl Tree {
    fn insert(&mut self, node: NodeId, target: NodeId, action: AddAction) -> Result<(), MockError> {
        // scsynth refuses `/s_new` and `/g_new` for a node id that is already
        // alive ("node id already in use") and creates NOTHING — the runtime
        // then holds a node id in state that does not exist in the tree.
        // That is state/tree divergence by the front door, so it must never
        // be simulated as a success.
        if self.parent.contains_key(&node) {
            self.rejects.push(format!(
                "create with node id {} which is ALREADY ALIVE — scsynth would reject it",
                node.0
            ));
        }
        self.children.entry(target).or_default();
        match action {
            AddAction::Head => {
                self.children.get_mut(&target).unwrap().insert(0, node);
                self.parent.insert(node, target);
            }
            AddAction::Tail => {
                self.children.get_mut(&target).unwrap().push(node);
                self.parent.insert(node, target);
            }
            AddAction::Before | AddAction::After => {
                let parent = *self
                    .parent
                    .get(&target)
                    .ok_or_else(|| MockError(format!("{:?} has no parent", target)))?;
                let siblings = self.children.get_mut(&parent).unwrap();
                let idx = siblings
                    .iter()
                    .position(|n| *n == target)
                    .ok_or_else(|| MockError(format!("{:?} not among siblings", target)))?;
                let idx = if action == AddAction::After {
                    idx + 1
                } else {
                    idx
                };
                siblings.insert(idx, node);
                self.parent.insert(node, parent);
            }
            AddAction::Replace => return Err(MockError("Replace unsupported".into())),
        }
        self.children.entry(node).or_default();
        Ok(())
    }

    /// Every node id currently alive in the tree carries a parent entry, so
    /// this is the tree's own answer to "does this node exist".
    fn alive(&self, node: NodeId) -> bool {
        self.parent.contains_key(&node)
    }

    fn free(&mut self, node: NodeId) {
        if let Some(parent) = self.parent.remove(&node) {
            if let Some(siblings) = self.children.get_mut(&parent) {
                siblings.retain(|n| *n != node);
            }
        }
        self.defs.remove(&node);
        self.controls.remove(&node);
        if let Some(kids) = self.children.remove(&node) {
            for kid in kids {
                self.free(kid);
            }
        }
    }

    fn move_before(&mut self, node: NodeId, before: NodeId) -> Result<(), MockError> {
        let old_parent = self
            .parent
            .remove(&node)
            .ok_or_else(|| MockError(format!("moved node {:?} not in tree", node)))?;
        self.children
            .get_mut(&old_parent)
            .unwrap()
            .retain(|n| *n != node);
        let new_parent = *self
            .parent
            .get(&before)
            .ok_or_else(|| MockError(format!("anchor {:?} not in tree", before)))?;
        let siblings = self.children.get_mut(&new_parent).unwrap();
        let idx = siblings
            .iter()
            .position(|n| *n == before)
            .ok_or_else(|| MockError(format!("anchor {:?} not among siblings", before)))?;
        siblings.insert(idx, node);
        self.parent.insert(node, new_parent);
        Ok(())
    }

    fn move_to_tail(&mut self, node: NodeId, group: NodeId) -> Result<(), MockError> {
        let old_parent = self
            .parent
            .remove(&node)
            .ok_or_else(|| MockError(format!("moved node {:?} not in tree", node)))?;
        self.children
            .get_mut(&old_parent)
            .unwrap()
            .retain(|n| *n != node);
        self.children.entry(group).or_default().push(node);
        self.parent.insert(node, group);
        Ok(())
    }

    /// Every synth under `root` in evaluation order — exactly what
    /// `/g_queryTree` reports and what `audio_path_breaks` expects.
    fn synths_in_evaluation_order(&self, root: NodeId) -> Vec<TreeSynth> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self
            .children
            .get(&root)
            .map(|kids| kids.iter().rev().copied().collect())
            .unwrap_or_default();
        while let Some(node) = stack.pop() {
            if let Some(def) = self.defs.get(&node) {
                out.push(TreeSynth {
                    node: node.0 as i32,
                    def: def.clone(),
                    controls: self
                        .controls
                        .get(&node)
                        .map(|p| p.iter().map(|(k, v)| (k.clone(), *v)).collect())
                        .unwrap_or_default(),
                });
            }
            if let Some(kids) = self.children.get(&node) {
                stack.extend(kids.iter().rev().copied());
            }
        }
        out
    }
}

#[derive(Default)]
struct BackendState {
    tree: Mutex<Tree>,
}

#[derive(Clone, Default)]
struct MockBackend {
    state: Arc<BackendState>,
}

impl MockBackend {
    fn new() -> Self {
        Self::default()
    }

    /// The live tree as the engine's own graph-integrity check sees it.
    fn tree(&self) -> Vec<TreeSynth> {
        self.state
            .tree
            .lock()
            .unwrap()
            .synths_in_evaluation_order(NodeId::new(0))
    }

    fn breaks(&self) -> Vec<AudioPathBreak> {
        audio_path_breaks(&self.tree())
    }

    fn rejects(&self) -> Vec<String> {
        self.state.tree.lock().unwrap().rejects.clone()
    }

    fn alive(&self, node: NodeId) -> bool {
        self.state.tree.lock().unwrap().alive(node)
    }

    fn control(&self, node: NodeId, name: &str) -> Option<f32> {
        self.state
            .tree
            .lock()
            .unwrap()
            .controls
            .get(&node)
            .and_then(|c| c.get(name))
            .copied()
    }

    /// The node's parent in the TREE — which is not necessarily the parent
    /// the runtime's state believes it has.
    fn tree_parent(&self, node: NodeId) -> Option<NodeId> {
        self.state.tree.lock().unwrap().parent.get(&node).copied()
    }

    /// Evaluation position of every node — groups included, unlike
    /// `synths_in_evaluation_order`, because a group's position is what
    /// decides whether its link can be reached in time.
    fn eval_index(&self) -> HashMap<NodeId, usize> {
        let tree = self.state.tree.lock().unwrap();
        let mut out = HashMap::new();
        let mut stack: Vec<NodeId> = tree
            .children
            .get(&NodeId::new(0))
            .map(|kids| kids.iter().rev().copied().collect())
            .unwrap_or_default();
        let mut idx = 0usize;
        while let Some(node) = stack.pop() {
            out.insert(node, idx);
            idx += 1;
            if let Some(kids) = tree.children.get(&node) {
                stack.extend(kids.iter().rev().copied());
            }
        }
        out
    }

    /// The whole tree — groups included — in evaluation order, so a failing
    /// assertion says WHERE a node landed and not just that it is stranded.
    fn render(&self) -> String {
        let tree = self.state.tree.lock().unwrap();
        let mut out = String::new();
        let mut stack: Vec<(NodeId, usize)> = tree
            .children
            .get(&NodeId::new(0))
            .map(|kids| kids.iter().rev().map(|n| (*n, 1usize)).collect())
            .unwrap_or_default();
        while let Some((node, depth)) = stack.pop() {
            let pad = "  ".repeat(depth);
            match tree.defs.get(&node) {
                Some(def) => {
                    let ctl = tree.controls.get(&node);
                    let get = |k: &str| ctl.and_then(|c| c.get(k)).copied();
                    let ports = match (get("inbus"), get("outbus")) {
                        (Some(i), Some(o)) => format!(" in={i} out={o}"),
                        (None, Some(o)) => format!(" out={o}"),
                        (Some(i), None) => format!(" in={i}"),
                        (None, None) => String::new(),
                    };
                    out.push_str(&format!("{pad}{} {}{}\n", node.0, def, ports));
                }
                None => out.push_str(&format!("{pad}{} [group]\n", node.0)),
            }
            if let Some(kids) = tree.children.get(&node) {
                stack.extend(kids.iter().rev().map(|n| (*n, depth + 1)));
            }
        }
        out
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
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        let mut tree = self.state.tree.lock().unwrap();
        tree.insert(node, target, action)?;
        tree.defs.insert(node, def.to_string());
        tree.controls.insert(node, params.clone());
        Ok(())
    }

    async fn create_group(
        &self,
        node: NodeId,
        target: NodeId,
        action: AddAction,
    ) -> Result<(), Self::Error> {
        self.state.tree.lock().unwrap().insert(node, target, action)
    }

    async fn free_node(&self, node: NodeId) -> Result<(), Self::Error> {
        self.state.tree.lock().unwrap().free(node);
        Ok(())
    }

    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn move_node_before(&self, node: NodeId, before: NodeId) -> Result<(), Self::Error> {
        self.state.tree.lock().unwrap().move_before(node, before)
    }

    async fn move_node_to_tail(&self, node: NodeId, group: NodeId) -> Result<(), Self::Error> {
        self.state.tree.lock().unwrap().move_to_tail(node, group)
    }

    async fn set_param(&self, node: NodeId, param: &str, value: f32) -> Result<(), Self::Error> {
        let mut tree = self.state.tree.lock().unwrap();
        if let Some(controls) = tree.controls.get_mut(&node) {
            controls.insert(param.to_string(), value);
        }
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

// =========================================================================
// Fixtures — the shape of a super-gamma patch.
// =========================================================================

/// Group and effect ids on the board are FNV hashes of the entity's name,
/// so the SAME id comes back patch after patch — sometimes under a
/// different master, sometimes not at all. These fixtures keep that: a
/// small pool of ids, recombined per patch.
const MASTER_A: GroupId = GroupId(1);
const MASTER_B: GroupId = GroupId(2);
const SIDE_A: GroupId = GroupId(3);
const SIDE_B: GroupId = GroupId(4);
const SIDE_C: GroupId = GroupId(5);

/// The board's factory patches carry the same effect *names* — hence the
/// same ids — on the master chain of one patch and on a performance side of
/// the next. That migration is the "synthdef or group changed" recreate.
const FX_1: EffectId = EffectId(10);
const FX_2: EffectId = EffectId(11);
const FX_3: EffectId = EffectId(12);

const DEF_FX: &str = "chain_fx";

/// A GATED voice synthdef. `voice_is_gated` asks the DSP registry whether the
/// synthdef declares a `gate` param, and `voice_release_grace` reads its
/// `release` default — so a voice only earns a teardown grace if its synthdef
/// is registered there. Without one, `reload_group_teardown_grace` returns
/// ZERO for every reload and `delete_with_grace` always takes the IMMEDIATE
/// branch, which is what left the deferred group-free window untested.
const DEF_VOICE: &str = "fuzz_gated_pad";

/// Release time of `DEF_VOICE`. The grace is `release + 100ms` margin, so a
/// group delete that catches this voice sounding defers its `/n_free` by
/// 350ms — deliberately far longer than the 50ms
/// `EFFECT_GRACE_PERIOD_MS` a deleted effect already buys, so a test can tell
/// the two windows apart by timing alone. `settle()` outlasts it.
const VOICE_RELEASE_S: f32 = 0.25;

/// Long enough to be past the effect grace (50ms) and still well inside the
/// voice grace (350ms).
const BETWEEN_THE_TWO_GRACES: std::time::Duration = std::time::Duration::from_millis(150);

/// Register `DEF_VOICE` in the process-global DSP registry. The registry is
/// shared by every test in the binary, so this runs once.
fn register_gated_voice_synthdef() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let params = [("gate", 1.0f32), ("release", VOICE_RELEASE_S), ("amp", 0.5)];
        let ir = vibelang_dsp::GraphIR {
            name: DEF_VOICE.to_string(),
            constants: Vec::new(),
            params: params
                .iter()
                .enumerate()
                .map(|(index, (name, value))| vibelang_dsp::ParamSpec {
                    name: (*name).to_string(),
                    default: vec![*value],
                    index,
                    lag_ms: None,
                })
                .collect(),
            nodes: Vec::new(),
            out_bus: 0,
        };
        vibelang_dsp::register_synthdef_ir(DEF_VOICE.to_string(), ir);
    });
}

/// One performance side: the group plus the effects on it.
struct Side {
    group: GroupId,
    fx: &'static [EffectId],
}

/// A patch: one master group carrying a serial fx chain, plus one
/// performance-side sub-group per entry in `sides` — the
/// two-sides-into-a-master-chain shape of every factory patch.
fn patch(master: GroupId, master_fx: &[EffectId], sides: &[Side]) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        master,
        GroupConfig {
            name: format!("master{}", master.0),
            effects: master_fx.to_vec(),
            ..Default::default()
        },
    );
    for id in master_fx {
        script.add_effect(
            *id,
            EffectConfig {
                group: master,
                synthdef: DEF_FX.to_string(),
                params: ParamMap::new(),
            },
        );
    }
    for side in sides {
        script.add_group(
            side.group,
            GroupConfig {
                name: format!("master{}/side{}", master.0, side.group.0),
                parent: Some(master),
                effects: side.fx.to_vec(),
                ..Default::default()
            },
        );
        for id in side.fx {
            script.add_effect(
                *id,
                EffectConfig {
                    group: side.group,
                    synthdef: DEF_FX.to_string(),
                    params: ParamMap::new(),
                },
            );
        }
    }
    script
}

/// The board shape, exactly: one master that SURVIVES the reload under a
/// constant id, carrying a serial fx chain whose synthdef CHANGES, plus one
/// child group whose id CHANGES because ids hash the full path.
///
/// `master_fx` is given in audio-path order — the order the chain runs in.
/// It is deliberately NOT id order in the tests below, so that the
/// `min_by_key(id.raw())` fallback in `State::first_effect_node_in_group`
/// picks a different node than the true chain head.
fn patch_with_def(
    master: GroupId,
    master_fx: &[EffectId],
    master_def: &str,
    sides: &[(GroupId, &[EffectId], &str)],
) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        master,
        GroupConfig {
            name: format!("master{}", master.0),
            effects: master_fx.to_vec(),
            ..Default::default()
        },
    );
    for id in master_fx {
        script.add_effect(
            *id,
            EffectConfig {
                group: master,
                synthdef: master_def.to_string(),
                params: ParamMap::new(),
            },
        );
    }
    for (group, fx, def) in sides {
        script.add_group(
            *group,
            GroupConfig {
                name: format!("master{}/side{}", master.0, group.0),
                parent: Some(master),
                effects: fx.to_vec(),
                ..Default::default()
            },
        );
        for id in *fx {
            script.add_effect(
                *id,
                EffectConfig {
                    group: *group,
                    synthdef: def.to_string(),
                    params: ParamMap::new(),
                },
            );
        }
    }
    script
}

fn side(group: GroupId, fx: &'static [EffectId]) -> Side {
    Side { group, fx }
}

async fn apply(runtime: &mut Runtime<MockBackend>, script: ScriptState) {
    runtime
        .send(ReloadMessage::Apply { state: script }.into())
        .await
        .unwrap();
    runtime.tick().await;
}

/// Let the reload's deferred work land: removed effects and deleted groups
/// are faded and freed past a grace period, so the tree straight after an
/// `Apply` is mid-transition and not what the board ever plays. Every
/// assertion about audio-path order must be made on the SETTLED tree.
///
/// This has to outlast the LONGEST grace any recall can ask for, which with
/// sounding gated voices is `VOICE_RELEASE_S + 100ms` = 350ms, not the 50ms
/// effect grace. Under-settling would hide a divergence rather than report
/// it, since a group whose free is still pending is already out of state and
/// so out of the invariant's reach.
async fn settle(runtime: &mut Runtime<MockBackend>) {
    for _ in 0..6 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        runtime.tick().await;
    }
}

/// Trigger every voice in `voices` so it is SOUNDING when the next reload
/// looks at it: `reload_group_teardown_grace` only counts a deleted or
/// structurally recreated voice whose `active_nodes` are non-empty, so an
/// untriggered voice buys no grace at all.
async fn sound_voices(runtime: &mut Runtime<MockBackend>, voices: &[VoiceId]) {
    for id in voices {
        runtime
            .send(
                VoiceMessage::Trigger {
                    id: *id,
                    params: ParamMap::new(),
                }
                .into(),
            )
            .await
            .unwrap();
    }
    runtime.tick().await;

    // A `Trigger` for a voice the reload never created fails INSIDE the tick
    // and is logged, not returned — so without this the fuzz would happily
    // report a green run in which nothing ever sounded and no group delete
    // ever earned a grace. Check the premise instead of assuming it.
    let state = runtime.state().read().await;
    for id in voices {
        let voice = state
            .voices
            .get(id)
            .unwrap_or_else(|| panic!("voice {} was never created by the reload", id.0));
        assert!(
            !voice.active_nodes.is_empty(),
            "voice {} is not sounding, so it buys no teardown grace",
            id.0
        );
    }
}

/// Groups that the reload has already dropped from state but whose nodes are
/// STILL ALIVE in the tree — the deferred group-free window, counted at the
/// instant the caller asks.
///
/// `before` is the (group, node, link) triple of every group that existed
/// before the reload.
async fn open_free_windows(
    runtime: &Runtime<MockBackend>,
    backend: &MockBackend,
    before: &[(GroupId, NodeId, Option<NodeId>)],
) -> usize {
    let state = runtime.state().read().await;
    before
        .iter()
        .filter(|(id, node, link)| {
            !state.groups.contains_key(id)
                && (backend.alive(*node) || link.is_some_and(|l| backend.alive(l)))
        })
        .count()
}

/// Every live group as (id, node, link), for `open_free_windows`.
async fn group_nodes(runtime: &Runtime<MockBackend>) -> Vec<(GroupId, NodeId, Option<NodeId>)> {
    runtime
        .state()
        .read()
        .await
        .groups
        .values()
        .map(|g| (g.id, g.node_id, g.link_synth_node_id))
        .collect()
}

async fn boot_with_defs(script: ScriptState, defs: &[&str]) -> (Runtime<MockBackend>, MockBackend) {
    register_gated_voice_synthdef();
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    for def in defs {
        runtime
            .send(
                SynthDefMessage::Load {
                    name: (*def).to_string(),
                    data: Vec::new(),
                }
                .into(),
            )
            .await
            .unwrap();
    }
    runtime.tick().await;
    apply(&mut runtime, script).await;
    (runtime, backend)
}

async fn boot(script: ScriptState) -> (Runtime<MockBackend>, MockBackend) {
    boot_with_defs(script, &[DEF_FX]).await
}

fn report(breaks: &[AudioPathBreak]) -> String {
    breaks
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

// =========================================================================
// Tests
// =========================================================================

/// The factory bank, in miniature: masters that repeat, sides that move
/// between masters, and effects that migrate between a master chain and a
/// side chain.
fn bank() -> Vec<ScriptState> {
    vec![
        patch(
            MASTER_A,
            &[FX_1, FX_2],
            &[side(SIDE_A, &[FX_3]), side(SIDE_B, &[])],
        ),
        patch(MASTER_B, &[FX_2], &[side(SIDE_C, &[FX_1])]),
        patch(
            MASTER_A,
            &[FX_2, FX_1],
            &[side(SIDE_C, &[]), side(SIDE_A, &[FX_3])],
        ),
        patch(MASTER_A, &[FX_3], &[side(SIDE_B, &[FX_1, FX_2])]),
        patch(
            MASTER_B,
            &[FX_1, FX_3],
            &[side(SIDE_A, &[]), side(SIDE_B, &[FX_2])],
        ),
    ]
}

/// Baseline: a cold boot of the two-sides patch is connected.
#[tokio::test(flavor = "current_thread")]
async fn cold_boot_of_a_two_sided_patch_is_connected() {
    let (_runtime, backend) = boot(bank().remove(0)).await;
    let breaks = backend.breaks();
    assert!(
        breaks.is_empty(),
        "cold boot must leave no node writing a bus nobody reads:\n{}",
        report(&breaks)
    );
}

/// The board defect: a recall that replaces the performance sides while the
/// master group survives. The new side groups must land before every node
/// that reads the master bus — its fx chain and its link synth.
#[tokio::test(flavor = "current_thread")]
async fn recall_that_replaces_the_sides_keeps_them_ahead_of_the_master_link() {
    let (mut runtime, backend) = boot(patch(
        MASTER_A,
        &[FX_1, FX_2],
        &[side(SIDE_A, &[FX_3]), side(SIDE_B, &[])],
    ))
    .await;
    assert!(
        backend.breaks().is_empty(),
        "precondition: cold boot is whole"
    );

    // The recall: same master, both sides replaced by new groups.
    apply(
        &mut runtime,
        patch(
            MASTER_A,
            &[FX_1, FX_2],
            &[side(SIDE_C, &[FX_3]), side(SIDE_A, &[])],
        ),
    )
    .await;

    let breaks = backend.breaks();
    assert!(
        breaks.is_empty(),
        "a recall left a side writing a bus whose only reader already ran — \
         that side is SILENT with every synth alive:\n{}",
        report(&breaks)
    );
}

/// Many recalls in a row: the preset gate drives ~35 of them, and the board
/// only broke after a run of them.
#[tokio::test(flavor = "current_thread")]
async fn a_run_of_recalls_never_strands_a_side() {
    let bank = bank();
    let (mut runtime, backend) = boot(bank[0].clone()).await;

    for (round, script) in bank.iter().cycle().take(35).enumerate() {
        apply(&mut runtime, script.clone()).await;
        let breaks = backend.breaks();
        assert!(
            breaks.is_empty(),
            "recall #{round} stranded a side on a dead bus:\n{}",
            report(&breaks)
        );
    }
}

// =========================================================================
// The board shape: surviving master, changing child, recreated master fx.
// =========================================================================

/// `vibes-and-air` -> `horn-section`: master `sg_chain_n21` is declared by
/// BOTH patches, so its id is constant and it survives the reload. The child
/// is `sg_chain_n21/n17` in one and `sg_chain_n21/n18` in the other — ids
/// hash the full path, so the child is a NEW group. In the same reload the
/// surviving master's own effects change synthdef (`reverb_jpverb` ->
/// `hall_reverb`), so they are structurally recreated too.
const DEF_JPVERB: &str = "reverb_jpverb";
const DEF_HALL: &str = "hall_reverb";

/// Master fx in AUDIO-PATH order `[FX_3, FX_2, FX_1]` — the reverse of id
/// order, so the `min_by_key(id.raw())` fallback in
/// `first_effect_node_in_group` resolves to the chain TAIL (`FX_1`), not
/// the head. That is the anchor lead: a child group anchored `Before` the
/// tail fx lands after the fx that should have seen it.
const MASTER_CHAIN: [EffectId; 3] = [FX_3, FX_2, FX_1];

const CHILD_N17: GroupId = SIDE_A;
const CHILD_N18: GroupId = SIDE_B;

#[tokio::test(flavor = "current_thread")]
async fn board_shape_surviving_master_changing_child_recreated_fx() {
    let (mut runtime, backend) = boot_with_defs(
        patch_with_def(
            MASTER_A,
            &MASTER_CHAIN,
            DEF_JPVERB,
            &[(CHILD_N17, &[], DEF_JPVERB)],
        ),
        &[DEF_JPVERB, DEF_HALL],
    )
    .await;
    assert!(
        backend.breaks().is_empty(),
        "precondition: cold boot is whole:\n{}\n{}",
        backend.render(),
        report(&backend.breaks())
    );
    eprintln!(
        "--- after cold boot (vibes-and-air) ---\n{}",
        backend.render()
    );

    // The recall: same master id, NEW child id, master fx change synthdef.
    apply(
        &mut runtime,
        patch_with_def(
            MASTER_A,
            &MASTER_CHAIN,
            DEF_HALL,
            &[(CHILD_N18, &[], DEF_HALL)],
        ),
    )
    .await;
    eprintln!("--- after recall, unsettled ---\n{}", backend.render());
    settle(&mut runtime).await;
    eprintln!(
        "--- after recall, settled (horn-section) ---\n{}",
        backend.render()
    );

    let breaks = backend.breaks();
    assert!(
        breaks.is_empty(),
        "a recall left a child group writing a bus whose only reader already \
         ran — that child is SILENT with every synth alive:\n{}\n{}",
        backend.render(),
        report(&breaks)
    );
}

/// The factory bank in the real shape: masters that repeat under a constant
/// id, children whose ids change every recall, effects that MIGRATE between
/// a master chain and a child chain, and synthdefs that flip on the
/// surviving master so its chain is structurally recreated in place.
///
/// The migration is the part the old fixtures never had: once an effect id
/// listed in `State::group_effect_chain[master]` belongs to a CHILD instead,
/// `first_effect_node_in_group` finds nothing in the chain and falls through
/// to `min_by_key(id.raw())` over whatever effects are left on the master —
/// the smallest id hash, which is not the head of the audio path.
fn real_bank() -> Vec<ScriptState> {
    vec![
        // vibes-and-air: master chain [FX_3, FX_2, FX_1], child n17.
        patch_with_def(
            MASTER_A,
            &[FX_3, FX_2, FX_1],
            DEF_JPVERB,
            &[(CHILD_N17, &[], DEF_JPVERB)],
        ),
        // horn-section: same master id, NEW child id, defs flip, and FX_1
        // MIGRATES off the master onto the child.
        patch_with_def(
            MASTER_A,
            &[FX_3, FX_2],
            DEF_HALL,
            &[(CHILD_N18, &[FX_1], DEF_HALL)],
        ),
        // FX_1 comes back to the master, but at the chain TAIL, while FX_3
        // migrates out — so the min-id effect is no longer the chain head.
        patch_with_def(
            MASTER_A,
            &[FX_2, FX_1],
            DEF_JPVERB,
            &[(CHILD_N17, &[FX_3], DEF_JPVERB)],
        ),
        patch_with_def(
            MASTER_A,
            &[FX_1, FX_3],
            DEF_HALL,
            &[(CHILD_N18, &[FX_2], DEF_HALL)],
        ),
    ]
}

#[tokio::test(flavor = "current_thread")]
async fn a_run_of_real_shape_recalls_never_strands_a_child() {
    let bank = real_bank();
    let (mut runtime, backend) = boot_with_defs(bank[0].clone(), &[DEF_JPVERB, DEF_HALL]).await;
    settle(&mut runtime).await;

    for (round, script) in bank.iter().cycle().take(35).enumerate() {
        apply(&mut runtime, script.clone()).await;
        settle(&mut runtime).await;
        let breaks = backend.breaks();
        assert!(
            breaks.is_empty(),
            "recall #{round} stranded a child on a dead bus:\n{}\n{}",
            backend.render(),
            report(&breaks)
        );
    }
}

// =========================================================================
// Detector sensitivity — what the green runs above are worth.
// =========================================================================

/// A green run only means something if the detector would go red on the
/// tree the board actually produced. These drive the mock tree directly,
/// bypassing the engine, to pin down which orderings `audio_path_breaks`
/// calls a break — and therefore what the fixtures above can and cannot
/// ever catch.
fn synth(node: i32, def: &str, inbus: u32, outbus: u32) -> TreeSynth {
    TreeSynth {
        node,
        def: def.to_string(),
        controls: [
            ("inbus".to_string(), inbus as f32),
            ("outbus".to_string(), outbus as f32),
        ]
        .into_iter()
        .collect(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn detector_flags_a_child_link_after_the_parent_link() {
    // The board tree: child link writes bus 16, parent link (its only
    // reader) already ran.
    let tree = vec![
        synth(1002, "reverb_jpverb", 16, 16),
        synth(1005, "system_link_audio", 16, 0), // parent link — last reader
        synth(1011, "system_link_audio", 20, 16), // child link — STRANDED
    ];
    let breaks = audio_path_breaks(&tree);
    assert_eq!(
        breaks.len(),
        1,
        "detector must flag a child link that runs after the parent link:\n{}",
        report(&breaks)
    );
    assert_eq!(breaks[0].node, 1011);
}

/// The consequence of the `min_by_key(id.raw())` anchor in
/// `State::first_effect_node_in_group`: a child anchored `Before` the WRONG
/// fx lands mid-chain, so the fx ahead of it never hear that child. That is
/// a real wetness defect — but it is NOT an audio-path break, because the
/// parent's link synth still reads the bus afterwards. No fixture driven
/// through `audio_path_breaks` can ever turn this lead red.
// =========================================================================
// STATE invariants — the assertion `audio_path_breaks` cannot make.
// =========================================================================

/// What the board defect violates, stated over runtime STATE and the live
/// tree together instead of over the audio path:
///
/// 1. every group in state has a group node that is ALIVE in the tree;
/// 2. every group in state has `link_synth_node_id == Some(n)`, `n` alive —
///    a `None` link with a live link node is the state/tree divergence that
///    sends the next child group down `groups.rs` case 3 (`Tail`), and a
///    `Some` link pointing at a dead node is the same divergence mirrored;
/// 3. a child group's node sits inside its parent's subtree in the TREE, not
///    only in state;
/// 4. a child group's link runs BEFORE its parent's link.
///
/// `audio_path_breaks` can only ever see (4), and only when nothing else
/// reads the bus afterwards. (1)-(3) are pure divergence and are invisible
/// to any assertion made over the audio path alone.
async fn state_violations(runtime: &Runtime<MockBackend>, backend: &MockBackend) -> Vec<String> {
    let state = runtime.state().read().await;
    let order = backend.eval_index();
    let mut groups: Vec<&vibelang_core::GroupState> = state.groups.values().collect();
    groups.sort_by_key(|g| g.id.0);

    let mut out = Vec::new();
    for g in groups {
        if !backend.alive(g.node_id) {
            out.push(format!(
                "group {} is in state with node {} but that node is DEAD in the tree",
                g.id.0, g.node_id.0
            ));
        }
        match g.link_synth_node_id {
            None => out.push(format!(
                "group {} (node {}) has link_synth_node_id == None after a settled reload",
                g.id.0, g.node_id.0
            )),
            Some(link) => {
                if !backend.alive(link) {
                    out.push(format!(
                        "group {} points at link node {} which is DEAD in the tree",
                        g.id.0, link.0
                    ));
                }
            }
        }

        let Some(parent_id) = g.parent else { continue };
        let Some(parent) = state.groups.get(&parent_id) else {
            out.push(format!(
                "group {} claims parent {} which is not in state",
                g.id.0, parent_id.0
            ));
            continue;
        };
        let tree_parent = backend.tree_parent(g.node_id);
        if tree_parent != Some(parent.node_id) {
            out.push(format!(
                "group {} (node {}) claims parent {} (node {}) but the TREE has it under {:?}",
                g.id.0,
                g.node_id.0,
                parent_id.0,
                parent.node_id.0,
                tree_parent.map(|n| n.0)
            ));
        }
        if let (Some(child_link), Some(parent_link)) =
            (g.link_synth_node_id, parent.link_synth_node_id)
        {
            match (order.get(&child_link), order.get(&parent_link)) {
                (Some(c), Some(p)) if c > p => out.push(format!(
                    "group {}'s link (node {}, eval #{c}) runs AFTER its parent {}'s link \
                     (node {}, eval #{p}) — the child is silent",
                    g.id.0, child_link.0, parent_id.0, parent_link.0
                )),
                _ => {}
            }
        }
    }
    out
}

/// Everything that is wrong with the machine right now: state/tree
/// divergence, orderings scsynth would have refused, and audio-path breaks.
async fn all_violations(runtime: &Runtime<MockBackend>, backend: &MockBackend) -> Vec<String> {
    let mut out = state_violations(runtime, backend).await;
    out.extend(backend.rejects());
    out.extend(backend.breaks().iter().map(|b| b.to_string()));
    out
}

/// The invariant holds on a cold boot — otherwise every fuzz failure below
/// would just be the harness being wrong about what "correct" looks like.
#[tokio::test(flavor = "current_thread")]
async fn state_invariant_holds_on_a_cold_boot() {
    let (mut runtime, backend) = boot_with_defs(
        patch_with_def(
            MASTER_A,
            &MASTER_CHAIN,
            DEF_JPVERB,
            &[(CHILD_N17, &[FX_1], DEF_JPVERB)],
        ),
        &[DEF_JPVERB, DEF_HALL, DEF_FX],
    )
    .await;
    settle(&mut runtime).await;
    let violations = all_violations(&runtime, &backend).await;
    assert!(
        violations.is_empty(),
        "cold boot must satisfy the state invariant:\n{}\n{}",
        backend.render(),
        violations.join("\n")
    );
}

// =========================================================================
// Fuzzing reload SEQUENCES.
// =========================================================================

/// xorshift64*, so a failing run is a seed and not a mystery.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// One patch, in the form the failure message can print verbatim so a fuzz
/// hit converts straight into a deterministic test.
#[derive(Clone, Debug, PartialEq)]
struct Spec {
    /// (master id, fx chain in audio-path order, synthdef)
    masters: Vec<(u32, Vec<u32>, &'static str)>,
    /// (child id, parent master id, fx chain, synthdef)
    children: Vec<(u32, u32, Vec<u32>, &'static str)>,
    /// (voice id, group id) — a gated voice living in that group. Triggered
    /// after every settled recall, so the NEXT recall finds it sounding and
    /// a delete of its group takes the DEFERRED free path.
    voices: Vec<(u32, u32)>,
}

impl Spec {
    /// Every voice this patch declares, for `sound_voices`.
    fn voice_ids(&self) -> Vec<VoiceId> {
        self.voices.iter().map(|(v, _)| VoiceId(*v)).collect()
    }

    fn build(&self) -> ScriptState {
        let mut script = ScriptState::new();
        for (master, fx, def) in &self.masters {
            script.add_group(
                GroupId(*master),
                GroupConfig {
                    // Names are id-stable: on the board an id IS the hash of
                    // the name, so a group cannot change its name and keep
                    // its id.
                    name: format!("g{master}"),
                    effects: fx.iter().map(|f| EffectId(*f)).collect(),
                    ..Default::default()
                },
            );
            for f in fx {
                script.add_effect(
                    EffectId(*f),
                    EffectConfig {
                        group: GroupId(*master),
                        synthdef: (*def).to_string(),
                        params: ParamMap::new(),
                    },
                );
            }
        }
        for (child, parent, fx, def) in &self.children {
            script.add_group(
                GroupId(*child),
                GroupConfig {
                    name: format!("g{child}"),
                    parent: Some(GroupId(*parent)),
                    effects: fx.iter().map(|f| EffectId(*f)).collect(),
                    ..Default::default()
                },
            );
            for f in fx {
                script.add_effect(
                    EffectId(*f),
                    EffectConfig {
                        group: GroupId(*child),
                        synthdef: (*def).to_string(),
                        params: ParamMap::new(),
                    },
                );
            }
        }
        for (voice, group) in &self.voices {
            script.add_voice(
                VoiceId(*voice),
                VoiceConfig::new(format!("v{voice}"), DEF_VOICE, GroupId(*group)),
            );
        }
        script
    }
}

const MASTER_IDS: [u32; 2] = [1, 2];
const CHILD_IDS: [u32; 3] = [3, 4, 5];
const FX_IDS: [u32; 3] = [10, 11, 12];
const DEFS: [&str; 2] = [DEF_JPVERB, DEF_HALL];
/// A group's own voice is `VOICE_ID_BASE + group id`, so it lives and dies
/// with that group.
const VOICE_ID_BASE: u32 = 100;
/// A voice id that survives a change of group — the `voice("n16")` shape.
const ROAMING_VOICE: u32 = 200;

/// A random patch of the board's shape: one or two masters, each with a
/// serial fx chain, each carrying zero or more child groups that mix into
/// it, and effects that migrate freely between chains.
///
/// `stable_parents` distinguishes the two id regimes. The board hashes a
/// group's FULL PATH, so a child that moves to another master necessarily
/// changes id (`stable_parents = true`: a given child id always hangs off
/// the same master). A script that names groups without their path gets the
/// same id under a new parent (`stable_parents = false`), which is a
/// legitimate vibelang program but NOT the board's shape — keeping them
/// apart is what makes a hit interpretable.
fn random_spec(rng: &mut Rng, stable_parents: bool) -> Spec {
    let masters: Vec<u32> = match rng.below(3) {
        0 => vec![MASTER_IDS[0]],
        1 => vec![MASTER_IDS[1]],
        _ => vec![MASTER_IDS[0], MASTER_IDS[1]],
    };

    let mut children: Vec<(u32, u32, Vec<u32>, &'static str)> = Vec::new();
    for (i, child) in CHILD_IDS.iter().enumerate() {
        // Each child is present in roughly two patches out of three.
        let pick = rng.below(masters.len() + 1);
        if pick == masters.len() {
            continue;
        }
        let parent = if stable_parents {
            // A child id belongs to exactly one master forever; it is
            // present only when that master is.
            let owner = MASTER_IDS[i % MASTER_IDS.len()];
            if !masters.contains(&owner) {
                continue;
            }
            owner
        } else {
            masters[pick]
        };
        children.push((*child, parent, Vec::new(), DEFS[rng.below(2)]));
    }

    // Every effect lands on some chain — a master's or a child's — or is
    // absent from this patch entirely. Migration between the two is the
    // structural-recreate that the board does on every recall.
    let mut master_fx: Vec<Vec<u32>> = masters.iter().map(|_| Vec::new()).collect();
    for fx in FX_IDS {
        let slots = masters.len() + children.len() + 1;
        let slot = rng.below(slots);
        if slot < masters.len() {
            master_fx[slot].push(fx);
        } else if slot < masters.len() + children.len() {
            children[slot - masters.len()].2.push(fx);
        }
    }

    // Gated voices, the thing that gives a group delete a real grace. On the
    // board every sounding node is one of these: `voice("n16").synth("dx7_epiano")`
    // is polyphonic and gated, and a recall lands while notes are still ringing.
    //
    // Voice names on the board are NOT path-qualified (`voice("n16")`, not
    // `voice("sg_chain_n21/n16")`) even though group names are — so unlike a
    // group id, a VOICE id is stable across a group change. `ROAMING_VOICE`
    // keeps that shape: the same voice id turns up in a different group next
    // patch, which the reload treats as a structural recreate and which
    // therefore also feeds `reload_group_teardown_grace`.
    let group_ids: Vec<u32> = masters
        .iter()
        .copied()
        .chain(children.iter().map(|(c, _, _, _)| *c))
        .collect();
    let mut voices: Vec<(u32, u32)> = Vec::new();
    for g in &group_ids {
        // Roughly two groups in three carry their own voice.
        if rng.below(3) != 0 {
            voices.push((VOICE_ID_BASE + *g, *g));
        }
    }
    if !group_ids.is_empty() && rng.below(4) != 0 {
        let host = group_ids[rng.below(group_ids.len())];
        voices.push((ROAMING_VOICE, host));
    }

    let mut spec = Spec {
        masters: masters
            .iter()
            .enumerate()
            .map(|(i, m)| (*m, master_fx[i].clone(), DEFS[rng.below(2)]))
            .collect(),
        children,
        voices,
    };
    // Declaration order is audio-path order, and it is deliberately NOT id
    // order: reversing a chain is what makes the `min_by_key(id.raw())`
    // anchor disagree with the real chain head.
    for (_, fx, _) in spec.masters.iter_mut() {
        if rng.below(2) == 0 {
            fx.reverse();
        }
    }
    spec
}

/// Drive a random sequence of recalls through ONE runtime — an aged process,
/// which is the condition the predecessor's 35-cycle bank could not
/// reproduce from a fresh start — and check the STATE invariant after every
/// settled recall.
async fn fuzz_sequences(seed: u64, runs: usize, steps: usize, stable_parents: bool) {
    // Coverage, not results: how many group deletes were still holding their
    // nodes 150ms after the Apply. The 50ms effect grace cannot reach that
    // far, so every one of these is a window a SOUNDING VOICE held open, and
    // a run that scores zero has not tested the thing this fuzz exists for.
    let mut wide_windows = 0usize;
    // Recalls that landed while a previous recall's groups were still
    // pending their deferred free.
    let mut interleaved = 0usize;

    for run in 0..runs {
        let mut rng = Rng(seed
            .wrapping_add(run as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            | 1);
        let mut history: Vec<Spec> = Vec::new();

        let first = random_spec(&mut rng, stable_parents);
        history.push(first.clone());
        let (mut runtime, backend) =
            boot_with_defs(first.build(), &[DEF_JPVERB, DEF_HALL, DEF_FX, DEF_VOICE]).await;
        settle(&mut runtime).await;
        // Notes are ringing when the next recall arrives — that is what makes
        // the group deletes below take the DEFERRED free path.
        sound_voices(&mut runtime, &first.voice_ids()).await;

        for step in 0..steps {
            let spec = random_spec(&mut rng, stable_parents);
            history.push(spec.clone());
            let live_before = group_nodes(&runtime).await;

            apply(&mut runtime, spec.build()).await;
            // Mid-transition, deliberately: past the effect grace, inside the
            // voice grace. Deleted groups are out of state here while their
            // nodes and links are still in the tree, which is the shape of
            // the board's symptom and the state the invariant below is blind
            // to — so the next recall runs its diff against a tree that is
            // still carrying them.
            tokio::time::sleep(BETWEEN_THE_TWO_GRACES).await;
            runtime.tick().await;
            let open = open_free_windows(&runtime, &backend, &live_before).await;
            wide_windows += open;

            // One recall in three lands WHILE the window is open, instead of
            // waiting for it to close. That is what the board does: the
            // player walks the preset gate with notes still ringing, so the
            // next reload diffs against a tree that still carries the last
            // one's pending groups. Nothing is asserted mid-flight — the
            // invariant is a settled-tree statement — but any divergence this
            // interleaving creates is permanent, so the next settled recall
            // reports it.
            if open > 0 && step + 1 < steps && rng.below(3) == 0 {
                interleaved += 1;
                continue;
            }

            settle(&mut runtime).await;
            sound_voices(&mut runtime, &spec.voice_ids()).await;

            let violations = all_violations(&runtime, &backend).await;
            assert!(
                violations.is_empty(),
                "run {run} (seed {seed}) broke at recall #{step}\n\
                 --- violations ---\n{}\n--- settled tree ---\n{}\n--- sequence ---\n{}",
                violations.join("\n"),
                backend.render(),
                history
                    .iter()
                    .map(|s| format!("{s:?}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
    }

    // A green run is only worth something if the window was actually open.
    assert!(
        wide_windows >= runs,
        "the deferred group-free window opened only {wide_windows} times across \
         {runs} runs x {steps} recalls — the voices are not buying a grace and \
         this fuzz is measuring the immediate path again"
    );
    assert!(
        interleaved >= runs / 2,
        "only {interleaved} recalls landed while the window was open — the \
         mid-flight reload this fuzz is meant to cover barely happened"
    );
    eprintln!(
        "coverage: {wide_windows} voice-held group-free windows across {runs} runs x \
         {steps} recalls, {interleaved} recalls landing mid-window"
    );
}

/// The fuzz hit above, minimized to two recalls: a child group moves to
/// another master in the same reload that deletes its old master.
///
/// Unfixed, the reload never moves the group node. It stays inside master 1's
/// subtree, so `/n_free` on master 1's group node takes it — and its link —
/// with it, while the runtime keeps the group in state pointing at both dead
/// nodes and at a parent that no longer exists. The branch is silent, and no
/// further recall can repair it: the group is still in state, so it is never
/// re-created.
#[tokio::test(flavor = "current_thread")]
async fn a_child_that_moves_master_survives_the_old_master_being_deleted() {
    let before = Spec {
        masters: vec![(1, vec![11], DEF_HALL), (2, vec![], DEF_HALL)],
        children: vec![(3, 1, vec![10], DEF_HALL)],
        voices: Vec::new(),
    };
    let after = Spec {
        masters: vec![(2, vec![], DEF_HALL)],
        children: vec![(3, 2, vec![10], DEF_HALL)],
        voices: Vec::new(),
    };

    let (mut runtime, backend) =
        boot_with_defs(before.build(), &[DEF_JPVERB, DEF_HALL, DEF_FX]).await;
    settle(&mut runtime).await;
    assert!(
        all_violations(&runtime, &backend).await.is_empty(),
        "precondition: cold boot is whole"
    );

    apply(&mut runtime, after.build()).await;
    settle(&mut runtime).await;

    let violations = all_violations(&runtime, &backend).await;
    assert!(
        violations.is_empty(),
        "a child that moved to another master was destroyed with its old \
         master, and the runtime does not know:\n{}\n{}",
        backend.render(),
        violations.join("\n")
    );

    let state = runtime.state().read().await;
    let child = state.groups.get(&GroupId(3)).expect("child still in state");
    let master = state.groups.get(&GroupId(2)).expect("new master in state");
    assert_eq!(
        child.parent,
        Some(GroupId(2)),
        "state must record the new parent, or the group diffs as `updated` forever"
    );
    assert_eq!(
        backend.control(child.link_synth_node_id.unwrap(), "outbus"),
        Some(master.audio_bus.0 as f32),
        "the moved group's link must write its NEW parent's bus"
    );
}

/// The same move with both masters surviving: no node is destroyed, but the
/// group still has to leave the old subtree and re-point its link, or it
/// keeps mixing into the master the script no longer sends it to.
#[tokio::test(flavor = "current_thread")]
async fn a_child_that_moves_master_mixes_into_the_new_master() {
    let before = Spec {
        masters: vec![(1, vec![11], DEF_HALL), (2, vec![12], DEF_HALL)],
        children: vec![(3, 1, vec![10], DEF_HALL)],
        voices: Vec::new(),
    };
    let after = Spec {
        masters: vec![(1, vec![11], DEF_HALL), (2, vec![12], DEF_HALL)],
        children: vec![(3, 2, vec![10], DEF_HALL)],
        voices: Vec::new(),
    };

    let (mut runtime, backend) =
        boot_with_defs(before.build(), &[DEF_JPVERB, DEF_HALL, DEF_FX]).await;
    settle(&mut runtime).await;
    apply(&mut runtime, after.build()).await;
    settle(&mut runtime).await;

    let violations = all_violations(&runtime, &backend).await;
    assert!(
        violations.is_empty(),
        "{}\n{}",
        backend.render(),
        violations.join("\n")
    );

    let state = runtime.state().read().await;
    let child = state.groups.get(&GroupId(3)).expect("child in state");
    let master_2 = state.groups.get(&GroupId(2)).expect("new master in state");
    assert_eq!(child.parent, Some(GroupId(2)));
    assert_eq!(
        backend.tree_parent(child.node_id),
        Some(master_2.node_id),
        "the group node must physically live under its new master:\n{}",
        backend.render()
    );
    assert_eq!(
        backend.control(child.link_synth_node_id.unwrap(), "outbus"),
        Some(master_2.audio_bus.0 as f32),
        "the moved group's link must write its NEW parent's bus:\n{}",
        backend.render()
    );
}

/// What the fuzz above is worth depends entirely on how wide the deferred
/// window actually is, so pin it by TIMING.
///
/// A deleted effect alone already buys `EFFECT_GRACE_PERIOD_MS` (50ms), so
/// the voiceless fuzz was NOT running with a zero grace — it was running with
/// a 50ms one. A sounding gated voice widens that to `release + 100ms` =
/// 350ms. This test proves the wider window: 150ms after the Apply the group
/// is already out of state but its node and link are still ALIVE in the tree,
/// which the effect grace alone could not produce, and the free lands only
/// after the voice's own grace expires.
#[tokio::test(flavor = "current_thread")]
async fn a_sounding_gated_voice_makes_a_group_delete_defer_its_free() {
    let before = Spec {
        masters: vec![(1, vec![11], DEF_HALL), (2, vec![12], DEF_HALL)],
        children: vec![(3, 1, vec![10], DEF_HALL)],
        voices: vec![(VOICE_ID_BASE + 3, 3)],
    };
    // Master 1 and its child are gone; master 2 survives.
    let after = Spec {
        masters: vec![(2, vec![12], DEF_HALL)],
        children: Vec::new(),
        voices: Vec::new(),
    };

    let (mut runtime, backend) =
        boot_with_defs(before.build(), &[DEF_JPVERB, DEF_HALL, DEF_FX, DEF_VOICE]).await;
    settle(&mut runtime).await;
    sound_voices(&mut runtime, &before.voice_ids()).await;

    let (child_node, child_link) = {
        let state = runtime.state().read().await;
        let voice = state
            .voices
            .get(&VoiceId(VOICE_ID_BASE + 3))
            .expect("voice in state");
        assert!(
            !voice.active_nodes.is_empty(),
            "the voice must be SOUNDING or it buys no grace at all"
        );
        let child = state.groups.get(&GroupId(3)).expect("child in state");
        (child.node_id, child.link_synth_node_id.expect("link"))
    };

    apply(&mut runtime, after.build()).await;
    // Past the 50ms effect grace, still inside the 350ms voice grace.
    tokio::time::sleep(BETWEEN_THE_TWO_GRACES).await;
    runtime.tick().await;

    // Gone from state, still alive in the tree: the window the board's
    // symptom lives in, and now wide enough that only the voice can hold it.
    {
        let state = runtime.state().read().await;
        assert!(
            !state.groups.contains_key(&GroupId(3)),
            "the deleted group must already be out of state"
        );
    }
    assert!(
        backend.alive(child_node) && backend.alive(child_link),
        "DEFERRED branch not taken: the group's nodes were freed immediately, \
         so the grace window this fuzz exists to exercise never opened:\n{}",
        backend.render()
    );

    settle(&mut runtime).await;
    assert!(
        !backend.alive(child_node) && !backend.alive(child_link),
        "the deferred free never landed — the tree keeps a group nothing owns:\n{}",
        backend.render()
    );
    let violations = all_violations(&runtime, &backend).await;
    assert!(
        violations.is_empty(),
        "{}\n{}",
        backend.render(),
        violations.join("\n")
    );
}

/// The board's id regime: a child group belongs to one master forever,
/// because its id hashes the full path — `define_group("sg_chain_n21/n17")`
/// names the parent inside the child's own name, so a child cannot change
/// master without changing id. Two children share each master, so the
/// `n17` -> `n18` alternation under a surviving master is in the space.
#[tokio::test(flavor = "current_thread")]
async fn fuzz_reload_sequences_board_id_regime() {
    fuzz_sequences(0x5EED_0001, 10, 15, true).await;
}

/// The looser regime: the same group id reappears under a different parent.
#[tokio::test(flavor = "current_thread")]
async fn fuzz_reload_sequences_reparenting() {
    fuzz_sequences(0x5EED_0002, 10, 15, false).await;
}

#[tokio::test(flavor = "current_thread")]
async fn detector_is_blind_to_a_child_landing_mid_fx_chain() {
    let tree = vec![
        synth(1002, "reverb_jpverb", 16, 16), // never hears the child
        synth(1011, "system_link_audio", 20, 16), // child link, mid-chain
        synth(1003, "reverb_jpverb", 16, 16),
        synth(1005, "system_link_audio", 16, 0), // parent link still reads 16
    ];
    assert!(
        audio_path_breaks(&tree).is_empty(),
        "a mid-chain child is silently wrong, not stranded — the detector \
         cannot see it, so the anchor lead needs a DIFFERENT assertion"
    );
}
