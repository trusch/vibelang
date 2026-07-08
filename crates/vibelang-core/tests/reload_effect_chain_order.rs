//! Effect-chain ORDER preservation across reloads.
//!
//! Product invariant: a hot reload must produce the same audible result as
//! cold-booting the new script. For a group's effect chain that means the
//! serial signal flow (mixers → effect1 → effect2 → … → link synth) must
//! end up in *script order* on the live server, not just in config state.
//!
//! These tests pin down two corners:
//!
//! 1. **Reorder** — swapping `[a, b]` to `[b, a]` in the script must be
//!    expressed as `/n_before` moves of the existing nodes (cheap,
//!    glitch-free, preserves effect state), NOT a free/respawn, and the
//!    resulting server tree order must match a cold boot of the same script.
//! 2. **Mid-chain removal** — removing `b` from `[a, b, c]` must fade the
//!    node (mix → 0) and defer the `/n_free` past the grace period, while
//!    the surviving `[a, c]` keep cold-boot order.
//!
//! The mock backend simulates the scsynth node tree (ordered children,
//! honouring AddActions and `/n_before`) so "tree order" is asserted for
//! real, not inferred from message counts.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vibelang_core::reload::{EffectConfig, GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, EffectId, GroupId, NodeId, ParamMap, ReloadMessage,
    Runtime, SynthDefMessage,
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

/// Ordered log of every tree-affecting backend call.
#[derive(Clone, Debug, PartialEq)]
enum Op {
    Create {
        def: String,
        node: NodeId,
    },
    Free(NodeId),
    MoveBefore {
        node: NodeId,
        before: NodeId,
    },
    Set {
        node: NodeId,
        param: String,
        value: f32,
    },
}

/// Minimal scsynth tree: ordered children per group, honouring AddActions
/// and `/n_before` moves. Root group is NodeId(0).
#[derive(Default)]
struct Tree {
    children: HashMap<NodeId, Vec<NodeId>>,
    parent: HashMap<NodeId, NodeId>,
    defs: HashMap<NodeId, String>,
}

impl Tree {
    fn insert(&mut self, node: NodeId, target: NodeId, action: AddAction) -> Result<(), MockError> {
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

    fn free(&mut self, node: NodeId) {
        if let Some(parent) = self.parent.remove(&node) {
            if let Some(siblings) = self.children.get_mut(&parent) {
                siblings.retain(|n| *n != node);
            }
        }
        self.defs.remove(&node);
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

    /// Synthdef names in depth-first tree order under `root`.
    fn def_order(&self, root: NodeId) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self
            .children
            .get(&root)
            .map(|kids| kids.iter().rev().copied().collect())
            .unwrap_or_default();
        while let Some(node) = stack.pop() {
            if let Some(def) = self.defs.get(&node) {
                out.push(def.clone());
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
    ops: Mutex<Vec<Op>>,
}

#[derive(Clone, Default)]
struct MockBackend {
    state: Arc<BackendState>,
}

impl MockBackend {
    fn new() -> Self {
        Self::default()
    }

    fn ops(&self) -> Vec<Op> {
        self.state.ops.lock().unwrap().clone()
    }

    fn moves(&self) -> Vec<(NodeId, NodeId)> {
        self.ops()
            .into_iter()
            .filter_map(|op| match op {
                Op::MoveBefore { node, before } => Some((node, before)),
                _ => None,
            })
            .collect()
    }

    fn frees(&self) -> Vec<NodeId> {
        self.ops()
            .into_iter()
            .filter_map(|op| match op {
                Op::Free(node) => Some(node),
                _ => None,
            })
            .collect()
    }

    fn creates_of(&self, def: &str) -> usize {
        self.ops()
            .into_iter()
            .filter(|op| matches!(op, Op::Create { def: d, .. } if d == def))
            .count()
    }

    fn sets_of(&self, param: &str) -> Vec<(NodeId, f32)> {
        self.ops()
            .into_iter()
            .filter_map(|op| match op {
                Op::Set {
                    node,
                    param: p,
                    value,
                } if p == param => Some((node, value)),
                _ => None,
            })
            .collect()
    }

    /// Depth-first synthdef order under the root, filtered to `defs`.
    fn tree_order_of(&self, defs: &[&str]) -> Vec<String> {
        self.state
            .tree
            .lock()
            .unwrap()
            .def_order(NodeId::new(0))
            .into_iter()
            .filter(|d| defs.contains(&d.as_str()))
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
        let mut tree = self.state.tree.lock().unwrap();
        tree.insert(node, target, action)?;
        tree.defs.insert(node, def.to_string());
        drop(tree);
        self.state.ops.lock().unwrap().push(Op::Create {
            def: def.to_string(),
            node,
        });
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
        self.state.ops.lock().unwrap().push(Op::Free(node));
        Ok(())
    }

    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn move_node_before(&self, node: NodeId, before: NodeId) -> Result<(), Self::Error> {
        self.state.tree.lock().unwrap().move_before(node, before)?;
        self.state
            .ops
            .lock()
            .unwrap()
            .push(Op::MoveBefore { node, before });
        Ok(())
    }

    async fn set_param(&self, node: NodeId, param: &str, value: f32) -> Result<(), Self::Error> {
        self.state.ops.lock().unwrap().push(Op::Set {
            node,
            param: param.to_string(),
            value,
        });
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
// Fixtures
// =========================================================================

const GROUP: GroupId = GroupId(1);
const FX_A: EffectId = EffectId(20);
const FX_B: EffectId = EffectId(21);
const FX_C: EffectId = EffectId(22);

const DEF_A: &str = "chain_order_fx_a";
const DEF_B: &str = "chain_order_fx_b";
const DEF_C: &str = "chain_order_fx_c";
const LINK: &str = "system_link_audio";

fn def_of(id: EffectId) -> &'static str {
    match id {
        FX_A => DEF_A,
        FX_B => DEF_B,
        FX_C => DEF_C,
        _ => unreachable!(),
    }
}

/// Script with one group whose effect chain is `chain`, in order.
fn chain_script(chain: &[EffectId]) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            effects: chain.to_vec(),
            ..Default::default()
        },
    );
    for id in chain {
        script.add_effect(
            *id,
            EffectConfig {
                group: GROUP,
                synthdef: def_of(*id).to_string(),
                params: ParamMap::new(),
            },
        );
    }
    script
}

async fn load_synthdef(runtime: &mut Runtime<MockBackend>, name: &str) {
    runtime
        .send(
            SynthDefMessage::Load {
                name: name.to_string(),
                data: Vec::new(),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;
}

async fn apply(runtime: &mut Runtime<MockBackend>, script: ScriptState) {
    runtime
        .send(ReloadMessage::Apply { state: script }.into())
        .await
        .unwrap();
    runtime.tick().await;
}

async fn boot(chain: &[EffectId]) -> (Runtime<MockBackend>, MockBackend) {
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    for def in [DEF_A, DEF_B, DEF_C] {
        load_synthdef(&mut runtime, def).await;
    }
    apply(&mut runtime, chain_script(chain)).await;
    (runtime, backend)
}

async fn fx_node(runtime: &Runtime<MockBackend>, id: EffectId) -> NodeId {
    runtime
        .state()
        .read()
        .await
        .effects
        .get(&id)
        .expect("effect exists")
        .node_id
}

/// Full group tree order including the link synth, proving the invariant
/// "effects in script order, link synth last".
const ALL_DEFS: &[&str] = &[DEF_A, DEF_B, DEF_C, LINK];

// =========================================================================
// (a) Reorder [a, b] → [b, a]: /n_before moves, no respawn, cold-boot order.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn reorder_uses_n_before_and_matches_cold_boot() {
    let (mut runtime, backend) = boot(&[FX_A, FX_B]).await;

    // Cold boot must not need any moves: creation order IS script order.
    assert!(
        backend.moves().is_empty(),
        "cold boot must not emit /n_before moves: {:?}",
        backend.moves()
    );
    assert_eq!(
        backend.tree_order_of(ALL_DEFS),
        vec![DEF_A, DEF_B, LINK],
        "cold boot tree order"
    );

    let node_a = fx_node(&runtime, FX_A).await;
    let node_b = fx_node(&runtime, FX_B).await;
    let creates_a = backend.creates_of(DEF_A);
    let creates_b = backend.creates_of(DEF_B);

    // Hot reload with the chain reversed.
    apply(&mut runtime, chain_script(&[FX_B, FX_A])).await;

    // The reorder must be expressed as node moves of the EXISTING nodes...
    assert_eq!(
        backend.moves(),
        vec![(node_b, node_a)],
        "reorder must move b immediately before a via /n_before"
    );
    // ...not as a free/respawn cycle (which would cut reverb tails etc.).
    assert!(
        backend.frees().is_empty(),
        "no effect node may be freed for a pure reorder: {:?}",
        backend.frees()
    );
    assert_eq!(backend.creates_of(DEF_A), creates_a, "fx_a not respawned");
    assert_eq!(backend.creates_of(DEF_B), creates_b, "fx_b not respawned");

    // Live tree order now matches a cold boot of the reversed script,
    // with the link synth still last.
    assert_eq!(
        backend.tree_order_of(ALL_DEFS),
        vec![DEF_B, DEF_A, LINK],
        "hot-reloaded tree order must match cold boot"
    );
    let (_, cold_backend) = boot(&[FX_B, FX_A]).await;
    assert_eq!(
        backend.tree_order_of(ALL_DEFS),
        cold_backend.tree_order_of(ALL_DEFS),
        "hot reload and cold boot must agree on tree order"
    );

    // Runtime state tracks the new chain order and converges: re-applying
    // the same script is a clean no-op (no further moves).
    let state = runtime.state().read().await;
    assert_eq!(state.effect_ids_in_group(GROUP), vec![FX_B, FX_A]);
    drop(state);
    let moves_before = backend.moves().len();
    apply(&mut runtime, chain_script(&[FX_B, FX_A])).await;
    assert_eq!(
        backend.moves().len(),
        moves_before,
        "identical script must not emit further moves"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn three_effect_rotation_lands_in_script_order() {
    let (mut runtime, backend) = boot(&[FX_A, FX_B, FX_C]).await;
    assert_eq!(
        backend.tree_order_of(ALL_DEFS),
        vec![DEF_A, DEF_B, DEF_C, LINK]
    );

    apply(&mut runtime, chain_script(&[FX_C, FX_A, FX_B])).await;

    assert!(backend.frees().is_empty(), "rotation must not free nodes");
    assert_eq!(
        backend.tree_order_of(ALL_DEFS),
        vec![DEF_C, DEF_A, DEF_B, LINK],
        "rotated chain must land in script order with the link synth last"
    );
    assert_eq!(
        runtime.state().read().await.effect_ids_in_group(GROUP),
        vec![FX_C, FX_A, FX_B]
    );
}

// =========================================================================
// (b) Mid-chain removal: fade + deferred free, survivors keep order.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn mid_chain_removal_fades_defers_free_and_keeps_order() {
    let (mut runtime, backend) = boot(&[FX_A, FX_B, FX_C]).await;
    let node_b = fx_node(&runtime, FX_B).await;

    // Remove the middle effect.
    apply(&mut runtime, chain_script(&[FX_A, FX_C])).await;

    // Click-free teardown: mix ramped to 0 on the doomed node, free
    // deferred past the grace period (not in the apply itself).
    assert!(
        backend.sets_of("mix").contains(&(node_b, 0.0)),
        "removed effect must have its mix set to 0 before the free: {:?}",
        backend.sets_of("mix")
    );
    assert!(
        !backend.frees().contains(&node_b),
        "free must be deferred past the grace period, not immediate"
    );

    // After the grace period elapses, a runtime tick performs the free.
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    runtime.tick().await;
    assert!(
        backend.frees().contains(&node_b),
        "removed effect node must be freed after the grace period"
    );

    // Survivors keep cold-boot order — no moves were needed.
    assert_eq!(
        backend.tree_order_of(ALL_DEFS),
        vec![DEF_A, DEF_C, LINK],
        "surviving chain must match a cold boot of [a, c]"
    );
    let (_, cold_backend) = boot(&[FX_A, FX_C]).await;
    assert_eq!(
        backend.tree_order_of(ALL_DEFS),
        cold_backend.tree_order_of(ALL_DEFS)
    );
    assert_eq!(
        runtime.state().read().await.effect_ids_in_group(GROUP),
        vec![FX_A, FX_C]
    );
    assert!(
        backend.moves().is_empty(),
        "a removal that preserves relative order needs no moves: {:?}",
        backend.moves()
    );
}
