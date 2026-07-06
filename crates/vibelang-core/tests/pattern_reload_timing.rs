//! Reload timing invariants for patterns and melodies.
//!
//! Target invariant: **reload == cold boot at the same script state, and the
//! music never audibly hiccups.** These tests pin the three failure modes
//! that used to break it:
//!
//! 1. A pattern started by a reload mid-song must fire NO burst on its first
//!    tick and must play the exact phase a cold boot would be playing at the
//!    same beat (song-grid anchoring, absolute scheduling watermark).
//! 2. A quantized content swap must play the NEW content's downbeat at the
//!    boundary — exactly once, at the right time — even though the lookahead
//!    scheduler runs ~50ms ahead of the transport.
//! 3. Length-shrink (and grow) swaps must not burst-schedule a whole loop
//!    and must land on the phase a cold boot of the new script would play.
//!
//! The tests drive `PatternsHandler`/`MelodiesHandler` directly with explicit
//! beats (deterministic), plus `Runtime` + `ReloadMessage::Apply` for the
//! reload phase itself (grid anchoring, quantize-spec wiring).

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::compat::Instant;
use vibelang_core::handlers::{MelodiesHandler, PatternsHandler, VoicesHandler};
use vibelang_core::reload::{ChangeQuant, GroupConfig, ScriptState};
use vibelang_core::traits::{MelodyContent, PatternContent};
use vibelang_core::{
    AddAction, Backend, Beat, BufferId, BufferInfo, GroupId, GroupState, Melodies, MelodyConfig,
    MelodyId, NodeId, NoteEvent, ParamMap, PatternConfig, PatternId, Patterns, ReloadMessage,
    Runtime, State, Step, SynthDefMessage, VoiceConfig, VoiceId, Voices,
};

// =========================================================================
// Mock backend — records every scheduled synth creation (params + deadline)
// =========================================================================

#[derive(Debug)]
struct MockError;
impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}
impl std::error::Error for MockError {}

#[derive(Clone)]
struct TriggerCall {
    params: ParamMap,
    at: Option<Instant>,
}

/// Cheaply cloneable (inner Arc) so one instance can be handed to the
/// runtime/handlers while the test keeps a handle for assertions.
#[derive(Clone)]
struct MockBackend {
    triggers: Arc<Mutex<Vec<TriggerCall>>>,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            triggers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn triggers(&self) -> Vec<TriggerCall> {
        self.triggers.lock().unwrap().clone()
    }

    fn trigger_count(&self) -> usize {
        self.triggers.lock().unwrap().len()
    }

    /// Value of param `key` for each recorded trigger, in dispatch order.
    fn param_seq(&self, key: &str) -> Vec<f32> {
        self.triggers()
            .iter()
            .map(|t| t.params.get(key).copied().unwrap_or(f32::NAN))
            .collect()
    }
}

#[async_trait]
impl Backend for MockBackend {
    type Error = MockError;

    async fn load_synthdef(&self, _n: &str, _d: &[u8]) -> Result<(), MockError> {
        Ok(())
    }
    async fn create_synth(
        &self,
        _def: &str,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
        params: &ParamMap,
    ) -> Result<(), MockError> {
        self.triggers.lock().unwrap().push(TriggerCall {
            params: params.clone(),
            at: None,
        });
        Ok(())
    }
    async fn create_synth_at(
        &self,
        _def: &str,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
        params: &ParamMap,
        _param_buses: &[(String, u32)],
        at: Option<Instant>,
    ) -> Result<(), MockError> {
        self.triggers.lock().unwrap().push(TriggerCall {
            params: params.clone(),
            at,
        });
        Ok(())
    }
    async fn create_group(
        &self,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
    ) -> Result<(), MockError> {
        Ok(())
    }
    async fn free_node(&self, _node: NodeId) -> Result<(), MockError> {
        Ok(())
    }
    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), MockError> {
        Ok(())
    }
    async fn set_param(&self, _node: NodeId, _param: &str, _value: f32) -> Result<(), MockError> {
        Ok(())
    }
    async fn map_param_to_bus(
        &self,
        _node: NodeId,
        _param: &str,
        _bus: u32,
    ) -> Result<(), MockError> {
        Ok(())
    }
    async fn load_buffer(&self, _id: BufferId, _path: &Path) -> Result<BufferInfo, MockError> {
        Ok(BufferInfo {
            frames: 44100,
            channels: 2,
            sample_rate: 44100.0,
        })
    }
    async fn alloc_buffer(
        &self,
        _id: BufferId,
        frames: u32,
        channels: u16,
    ) -> Result<BufferInfo, MockError> {
        Ok(BufferInfo {
            frames,
            channels,
            sample_rate: 44100.0,
        })
    }
    async fn write_buffer(&self, _id: BufferId, _path: &Path) -> Result<(), MockError> {
        Ok(())
    }
    async fn free_buffer(&self, _id: BufferId) -> Result<(), MockError> {
        Ok(())
    }
    fn current_time(&self) -> Instant {
        Instant::now()
    }
}

// =========================================================================
// Handler-level harness (deterministic beats)
// =========================================================================

const SYNTH: &str = "test_synth";
const GROUP: GroupId = GroupId(1);
const VOICE: VoiceId = VoiceId(1);
const PATTERN: PatternId = PatternId(1);
const MELODY: MelodyId = MelodyId(1);

async fn setup_pattern_harness() -> (
    MockBackend,
    Arc<RwLock<State>>,
    PatternsHandler<MockBackend>,
) {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    {
        let mut s = state.write().await;
        s.synthdefs.insert(SYNTH.to_string());
        s.groups.insert(
            GROUP,
            GroupState {
                id: GROUP,
                name: "g".to_string(),
                parent: None,
                node_id: NodeId(100),
                audio_bus: vibelang_core::BusId(16),
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );
    }
    let voices = Arc::new(VoicesHandler::new(Arc::new(backend.clone()), state.clone()));
    voices
        .create(VOICE, VoiceConfig::new("v", SYNTH, GROUP))
        .await
        .expect("voice creation");
    let patterns = PatternsHandler::new(state.clone(), voices);
    (backend, state, patterns)
}

async fn setup_melody_harness() -> (
    MockBackend,
    Arc<RwLock<State>>,
    MelodiesHandler<MockBackend>,
) {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    {
        let mut s = state.write().await;
        s.synthdefs.insert(SYNTH.to_string());
        s.groups.insert(
            GROUP,
            GroupState {
                id: GROUP,
                name: "g".to_string(),
                parent: None,
                node_id: NodeId(100),
                audio_bus: vibelang_core::BusId(16),
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );
    }
    let voices = Arc::new(VoicesHandler::new(Arc::new(backend.clone()), state.clone()));
    voices
        .create(VOICE, VoiceConfig::new("v", SYNTH, GROUP))
        .await
        .expect("voice creation");
    let melodies = MelodiesHandler::new(state.clone(), voices);
    (backend, state, melodies)
}

/// Pattern config with steps at the given loop-local beats. Every step
/// carries a `marker` (content identity) and a `sbeat` (loop-local position)
/// param so triggers reaching the backend are attributable.
fn pattern_config(marker: f32, length: f64, step_beats: &[f64]) -> PatternConfig {
    let mut config = PatternConfig::with_length("p", VOICE, length);
    for &b in step_beats {
        let mut params = ParamMap::new();
        params.insert("marker".to_string(), marker);
        params.insert("sbeat".to_string(), b as f32);
        config.steps.push(Step {
            beat: Beat::from_f64(b),
            params,
        });
    }
    config
}

/// Melody config with (per-note-params) markers, mirroring `pattern_config`.
fn melody_config(marker: f32, length: f64, note_beats: &[f64]) -> MelodyConfig {
    let mut config = MelodyConfig {
        name: "m".to_string(),
        voice: Some(VOICE),
        notes: Vec::new(),
        length: Beat::from_f64(length),
        swing: 0.0,
    };
    for &b in note_beats {
        let mut params = std::collections::HashMap::new();
        params.insert("marker".to_string(), marker);
        params.insert("sbeat".to_string(), b as f32);
        config.notes.push(NoteEvent {
            beat: Beat::from_f64(b),
            note: 60,
            velocity: 1.0,
            duration: Beat::from_f64(0.25),
            params,
        });
    }
    config
}

async fn set_beat(state: &Arc<RwLock<State>>, beat: f64) {
    state.write().await.current_beat = Beat::from_f64(beat);
}

/// Tick the pattern handler across `[from, to)` in 0.01-beat increments
/// (~5ms at 120 BPM, matching the 100Hz production tick), returning the
/// maximum number of triggers dispatched by any single tick.
async fn run_pattern_ticks(
    backend: &MockBackend,
    patterns: &PatternsHandler<MockBackend>,
    from: f64,
    to: f64,
) -> usize {
    let mut max_per_tick = 0;
    let mut beat = from;
    while beat < to {
        let before = backend.trigger_count();
        patterns.tick(Beat::from_f64(beat)).await;
        max_per_tick = max_per_tick.max(backend.trigger_count() - before);
        beat += 0.01;
    }
    max_per_tick
}

async fn run_melody_ticks(
    backend: &MockBackend,
    melodies: &MelodiesHandler<MockBackend>,
    from: f64,
    to: f64,
) -> usize {
    let mut max_per_tick = 0;
    let mut beat = from;
    while beat < to {
        let before = backend.trigger_count();
        melodies.tick(Beat::from_f64(beat)).await;
        max_per_tick = max_per_tick.max(backend.trigger_count() - before);
        beat += 0.01;
    }
    max_per_tick
}

// =========================================================================
// (a) + (b): reload start — no burst, phase equals cold boot
// =========================================================================

/// Reload-starting a pattern mid-song (beat 9.3, 4-beat loop with a step
/// every 0.5 beats) must not fire more triggers in any tick than a normal
/// steady-state tick does — the old code replayed the pattern tail + head
/// as one simultaneous burst on the first post-reload tick.
#[tokio::test(flavor = "current_thread")]
async fn reload_start_fires_no_burst() {
    let (backend, state, patterns) = setup_pattern_harness().await;
    let steps: Vec<f64> = (0..8).map(|i| i as f64 * 0.5).collect();
    patterns
        .create(PATTERN, pattern_config(1.0, 4.0, &steps))
        .await
        .unwrap();

    // Simulate the reload phase: start_on_grid at beat 9.3.
    set_beat(&state, 9.3).await;
    patterns.start_on_grid(PATTERN).await.unwrap();

    let max_per_tick = run_pattern_ticks(&backend, &patterns, 9.3, 10.31).await;

    // Steps are 0.5 beats apart, the lookahead window is 0.1 beats: no tick
    // can legitimately schedule more than one step.
    assert!(
        max_per_tick <= 1,
        "reload start must not burst: got {} triggers in one tick",
        max_per_tick
    );
    // Exactly the steps at absolute beats 9.5 and 10.0, 10.25? No — grid
    // steps in [9.3, 10.31 + lookahead) are 9.5 and 10.0 and 10.5 is out of
    // reach of the last window (10.30 + 0.1 < 10.5).
    assert_eq!(
        backend.param_seq("sbeat"),
        vec![1.5, 2.0],
        "reload-started pattern must play the song-grid phase"
    );
}

/// Post-reload phase must equal cold-boot phase: a cold boot playing since
/// beat 0 and a reload start at beat 9.3 must fire the identical step
/// sequence from beat 9.3 onward (equal trigger positions modulo length).
#[tokio::test(flavor = "current_thread")]
async fn reload_start_phase_equals_cold_boot() {
    let steps: Vec<f64> = (0..8).map(|i| i as f64 * 0.5).collect();

    // Cold boot: start at beat 0 and play through beat 12.
    let (cold_backend, cold_state, cold_patterns) = setup_pattern_harness().await;
    cold_patterns
        .create(PATTERN, pattern_config(1.0, 4.0, &steps))
        .await
        .unwrap();
    set_beat(&cold_state, 0.0).await;
    cold_patterns.start_on_grid(PATTERN).await.unwrap();
    run_pattern_ticks(&cold_backend, &cold_patterns, 0.0, 12.0).await;

    // Reload: identical script state, started at beat 9.3, played to 12.
    let (reload_backend, reload_state, reload_patterns) = setup_pattern_harness().await;
    reload_patterns
        .create(PATTERN, pattern_config(1.0, 4.0, &steps))
        .await
        .unwrap();
    set_beat(&reload_state, 9.3).await;
    reload_patterns.start_on_grid(PATTERN).await.unwrap();
    run_pattern_ticks(&reload_backend, &reload_patterns, 9.3, 12.0).await;

    let cold_seq = cold_backend.param_seq("sbeat");
    let reload_seq = reload_backend.param_seq("sbeat");

    // The reload sequence must be exactly the cold sequence's tail from
    // beat 9.3 on. Cold fired steps at absolute 0.0, 0.5, ..; the first
    // step at-or-after 9.3 is 9.5 (loop-local 1.5), i.e. index 19.
    assert!(!reload_seq.is_empty(), "reload run must fire steps");
    assert_eq!(
        reload_seq,
        cold_seq[19..],
        "post-reload phase must equal cold-boot phase at the same beats"
    );
}

/// The permanent half of the old bug: after a reload the pattern was
/// phase-locked to the reload instant instead of the song grid. Verify the
/// anchor directly: start_on_grid at 9.3 with a 4-beat loop must anchor at
/// beat 8 with the watermark at "now".
#[tokio::test(flavor = "current_thread")]
async fn reload_start_anchors_to_song_grid() {
    let (_backend, state, patterns) = setup_pattern_harness().await;
    patterns
        .create(PATTERN, pattern_config(1.0, 4.0, &[0.0]))
        .await
        .unwrap();
    set_beat(&state, 9.3).await;
    patterns.start_on_grid(PATTERN).await.unwrap();

    let s = state.read().await;
    let p = s.patterns.get(&PATTERN).unwrap();
    assert_eq!(
        p.start_beat,
        Beat::from_f64(8.0),
        "start_beat must be the grid multiple current_beat - current_beat % length"
    );
    assert_eq!(p.scheduled_until, Some(Beat::from_f64(9.3)));
    assert_eq!(p.loop_position, Beat::from_f64(9.3) % Beat::from_f64(4.0));
}

// =========================================================================
// (c) quantized content swap hits the boundary it targets
// =========================================================================

/// A NextBar swap queued mid-bar must play the NEW content's downbeat at
/// the bar line — exactly once, with the correct lookahead deadline. The
/// old code applied the swap when current_beat crossed the bar, after the
/// lookahead had already scheduled past it with old content.
#[tokio::test(flavor = "current_thread")]
async fn quantized_swap_plays_new_downbeat_at_boundary() {
    let (backend, _state, patterns) = setup_pattern_harness().await;
    patterns
        .create(PATTERN, pattern_config(1.0, 4.0, &[0.0, 1.0, 2.0, 3.0]))
        .await
        .unwrap();
    patterns.start(PATTERN).await.unwrap();

    // Play bars 0 and 1 with the old content.
    run_pattern_ticks(&backend, &patterns, 0.0, 7.5).await;

    // Live-coder saves mid-bar: queue the swap for the next bar (beat 8).
    {
        let mut s = _state.write().await;
        let p = s.patterns.get_mut(&PATTERN).unwrap();
        p.queue_content_swap(
            Arc::new(PatternContent::from_config(&pattern_config(
                2.0,
                4.0,
                &[0.0, 1.0, 2.0, 3.0],
            ))),
            ChangeQuant::NextBar,
        );
    }

    let before_boundary = Instant::now();
    run_pattern_ticks(&backend, &patterns, 7.5, 8.31).await;

    let markers = backend.param_seq("marker");
    let sbeats = backend.param_seq("sbeat");

    // Everything before the bar line came from the old content...
    let boundary_idx = markers.iter().position(|&m| m == 2.0).expect(
        "the new content must fire after the boundary",
    );
    assert!(
        markers[..boundary_idx].iter().all(|&m| m == 1.0),
        "no new-content step may fire before the bar line: {:?}",
        markers
    );
    // ...and the first new-content trigger is the downbeat, exactly once.
    assert_eq!(
        sbeats[boundary_idx], 0.0,
        "first post-boundary trigger must be the new content's downbeat"
    );
    let downbeat_triggers: Vec<_> = backend
        .triggers()
        .iter()
        .filter(|t| {
            t.params.get("marker") == Some(&2.0) && t.params.get("sbeat") == Some(&0.0)
        })
        .cloned()
        .collect();
    assert_eq!(
        downbeat_triggers.len(),
        1,
        "the new downbeat must fire exactly once"
    );
    // The old content's downbeat fired only for bars 0 and 1 — the bar at
    // beat 8 must NOT get an old-content downbeat:
    let old_downbeats = backend
        .triggers()
        .iter()
        .filter(|t| t.params.get("marker") == Some(&1.0) && t.params.get("sbeat") == Some(&0.0))
        .count();
    assert_eq!(
        old_downbeats, 2,
        "the old content's downbeat must not play into the new bar"
    );
    // Timing: the downbeat was scheduled before the transport reached beat 8
    // (lookahead) and its deadline is in the near future of the tick that
    // scheduled it (<= lookahead window + slack).
    let at = downbeat_triggers[0].at.expect("scheduled with a deadline");
    let lead = at.duration_since(before_boundary);
    assert!(
        lead <= Duration::from_millis(600),
        "downbeat deadline should be within the ticked range, got {:?}",
        lead
    );
}

/// If the save lands after the lookahead has already committed the bar line
/// (watermark past beat 8), the swap must defer to the NEXT bar — never
/// replay or double-fire the already-scheduled downbeat.
#[tokio::test(flavor = "current_thread")]
async fn quantized_swap_defers_when_boundary_already_scheduled() {
    let (backend, state, patterns) = setup_pattern_harness().await;
    patterns
        .create(PATTERN, pattern_config(1.0, 4.0, &[0.0, 1.0, 2.0, 3.0]))
        .await
        .unwrap();
    patterns.start(PATTERN).await.unwrap();

    // Tick just past bar 2's start: the watermark is now ~8.05.
    run_pattern_ticks(&backend, &patterns, 0.0, 7.96).await;

    {
        let mut s = state.write().await;
        let p = s.patterns.get_mut(&PATTERN).unwrap();
        p.queue_content_swap(
            Arc::new(PatternContent::from_config(&pattern_config(
                2.0,
                4.0,
                &[0.0, 1.0, 2.0, 3.0],
            ))),
            ChangeQuant::NextBar,
        );
    }

    run_pattern_ticks(&backend, &patterns, 7.96, 12.31).await;

    // The bar-8 downbeat fired exactly once (old content — it was already
    // committed to the backend when the swap arrived), and the new content
    // takes over at bar 12.
    let triggers = backend.triggers();
    let bar8_downbeats: Vec<f32> = triggers
        .iter()
        .zip(backend.param_seq("sbeat"))
        .filter(|(_, sb)| *sb == 0.0)
        .map(|(t, _)| t.params.get("marker").copied().unwrap())
        .collect();
    // Downbeats fired: abs 0 (start), abs 4, abs 8 (old, already committed),
    // abs 12 (new).
    assert_eq!(
        bar8_downbeats,
        vec![1.0, 1.0, 1.0, 2.0],
        "downbeats must fire once each; swap takes effect at the next clean bar"
    );
}

// =========================================================================
// (d) length shrink / grow swaps: no burst, correct phase
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn length_shrink_swap_does_not_burst() {
    let (backend, state, patterns) = setup_pattern_harness().await;
    patterns
        .create(PATTERN, pattern_config(1.0, 4.0, &[0.0, 1.0, 2.0, 3.0]))
        .await
        .unwrap();
    patterns.start(PATTERN).await.unwrap();
    run_pattern_ticks(&backend, &patterns, 0.0, 7.5).await;

    // Shrink 4 -> 2 beats at the next bar.
    {
        let mut s = state.write().await;
        let p = s.patterns.get_mut(&PATTERN).unwrap();
        p.queue_content_swap(
            Arc::new(PatternContent::from_config(&pattern_config(
                2.0,
                2.0,
                &[0.0, 1.0],
            ))),
            ChangeQuant::NextBar,
        );
    }

    let max_per_tick = run_pattern_ticks(&backend, &patterns, 7.5, 10.31).await;
    assert!(
        max_per_tick <= 1,
        "shrink swap must not burst: got {} triggers in one tick",
        max_per_tick
    );

    // Post-swap the 2-beat loop is grid-anchored at beat 8: steps at
    // 8.0 (0), 9.0 (1), 10.0 (0).
    let post: Vec<(f32, f32)> = backend
        .triggers()
        .iter()
        .filter(|t| t.params.get("marker") == Some(&2.0))
        .map(|t| {
            (
                t.params.get("sbeat").copied().unwrap(),
                t.params.get("marker").copied().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        post,
        vec![(0.0, 2.0), (1.0, 2.0), (0.0, 2.0)],
        "shrunk loop must land on grid phase without skipping or repeating"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn length_grow_swap_does_not_skip_or_burst() {
    let (backend, state, patterns) = setup_pattern_harness().await;
    patterns
        .create(PATTERN, pattern_config(1.0, 2.0, &[0.0, 1.0]))
        .await
        .unwrap();
    patterns.start(PATTERN).await.unwrap();
    run_pattern_ticks(&backend, &patterns, 0.0, 7.5).await;

    // Grow 2 -> 4 beats at the next bar.
    {
        let mut s = state.write().await;
        let p = s.patterns.get_mut(&PATTERN).unwrap();
        p.queue_content_swap(
            Arc::new(PatternContent::from_config(&pattern_config(
                2.0,
                4.0,
                &[0.0, 1.0, 2.0, 3.0],
            ))),
            ChangeQuant::NextBar,
        );
    }

    let max_per_tick = run_pattern_ticks(&backend, &patterns, 7.5, 12.31).await;
    assert!(max_per_tick <= 1, "grow swap must not burst");

    let post: Vec<f32> = backend
        .triggers()
        .iter()
        .filter(|t| t.params.get("marker") == Some(&2.0))
        .map(|t| t.params.get("sbeat").copied().unwrap())
        .collect();
    // Grid-anchored at beat 8: 8.0->0, 9.0->1, 10.0->2, 11.0->3, 12.0->0.
    assert_eq!(post, vec![0.0, 1.0, 2.0, 3.0, 0.0]);
}

/// Melody variant of the shrink bug: melodies schedule on absolute time, so
/// the old `loop_position = 0` reset made the next tick see a lookahead a
/// full loop ahead and schedule the entire new loop at once.
#[tokio::test(flavor = "current_thread")]
async fn melody_length_shrink_swap_does_not_burst() {
    let (backend, state, melodies) = setup_melody_harness().await;
    melodies
        .create(MELODY, melody_config(1.0, 4.0, &[0.0, 1.0, 2.0, 3.0]))
        .await
        .unwrap();
    set_beat(&state, 0.0).await;
    melodies.start(MELODY).await.unwrap();
    run_melody_ticks(&backend, &melodies, 0.0, 7.5).await;

    {
        let mut s = state.write().await;
        let m = s.melodies.get_mut(&MELODY).unwrap();
        m.queue_content_swap(
            Arc::new(MelodyContent::from_config(&melody_config(
                2.0,
                2.0,
                &[0.0, 1.0],
            ))),
            ChangeQuant::NextBar,
        );
    }

    let max_per_tick = run_melody_ticks(&backend, &melodies, 7.5, 10.31).await;
    assert!(
        max_per_tick <= 1,
        "melody shrink swap must not burst: got {} triggers in one tick",
        max_per_tick
    );

    let post: Vec<f32> = backend
        .triggers()
        .iter()
        .filter(|t| t.params.get("marker") == Some(&2.0))
        .map(|t| t.params.get("sbeat").copied().unwrap())
        .collect();
    // Absolute-time phase with the new 2-beat length: 8.0->0, 9.0->1, 10.0->0.
    assert_eq!(post, vec![0.0, 1.0, 0.0]);
}

// =========================================================================
// Runtime-level: reload phase uses grid anchoring + quantize-spec wiring
// =========================================================================

async fn apply(runtime: &mut Runtime<MockBackend>, script: ScriptState) {
    runtime
        .send(ReloadMessage::Apply { state: script }.into())
        .await
        .unwrap();
    runtime.tick().await;
}

fn base_script(group_amp: f32, pattern: PatternConfig, playing: bool) -> ScriptState {
    let mut script = ScriptState::new();
    let mut params = ParamMap::new();
    params.insert("amp".to_string(), group_amp);
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            params,
            ..Default::default()
        },
    );
    script.add_voice(VOICE, VoiceConfig::new("v", SYNTH, GROUP));
    script.add_pattern(PATTERN, pattern);
    if playing {
        script.playing_patterns.insert(PATTERN);
    }
    script
}

/// The reload phase must anchor a newly started pattern to the song grid
/// (via start_on_grid), not to the reload instant.
#[tokio::test(flavor = "current_thread")]
async fn runtime_reload_starts_pattern_on_song_grid() {
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    runtime
        .send(
            SynthDefMessage::Load {
                name: SYNTH.to_string(),
                data: Vec::new(),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    // Cold apply: pattern exists but is not playing.
    apply(
        &mut runtime,
        base_script(0.5, pattern_config(1.0, 4.0, &[0.0]), false),
    )
    .await;

    // Mid-song reload: transport sits at beat 9.3 (paused transport keeps
    // the beat where tests put it), the new script starts the pattern.
    runtime.state().write().await.current_beat = Beat::from_f64(9.3);
    apply(
        &mut runtime,
        base_script(0.6, pattern_config(1.0, 4.0, &[0.0]), true),
    )
    .await;

    let state = runtime.state().read().await;
    let p = state.patterns.get(&PATTERN).expect("pattern exists");
    assert!(p.playing);
    assert_eq!(
        p.start_beat,
        Beat::from_f64(8.0),
        "reload must anchor the pattern to the song grid, not the reload instant"
    );
}

/// The script's `set_quantization(beats)` spec must reach the queued content
/// swap instead of the hardcoded NextBar default.
#[tokio::test(flavor = "current_thread")]
async fn runtime_reload_wires_script_quantization_into_swap() {
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    runtime
        .send(
            SynthDefMessage::Load {
                name: SYNTH.to_string(),
                data: Vec::new(),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    apply(
        &mut runtime,
        base_script(0.5, pattern_config(1.0, 4.0, &[0.0]), true),
    )
    .await;

    // Reload with changed steps and a 1-beat quantization grid.
    let mut script = base_script(0.5, pattern_config(1.0, 4.0, &[0.0, 2.0]), true);
    script.quantization = 1.0;
    apply(&mut runtime, script).await;

    let state = runtime.state().read().await;
    let p = state.patterns.get(&PATTERN).expect("pattern exists");
    let (_, quant) = p
        .pending_content
        .as_ref()
        .expect("content swap queued on the playing pattern");
    assert_eq!(
        *quant,
        ChangeQuant::NextBeat,
        "set_quantization(1) must map to a NextBeat swap"
    );
}

/// A content update on a *stopped* pattern applies immediately (cold-boot
/// equivalence: a cold boot would obviously build the new content).
#[tokio::test(flavor = "current_thread")]
async fn runtime_reload_applies_swap_immediately_when_not_playing() {
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    runtime
        .send(
            SynthDefMessage::Load {
                name: SYNTH.to_string(),
                data: Vec::new(),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    apply(
        &mut runtime,
        base_script(0.5, pattern_config(1.0, 4.0, &[0.0]), false),
    )
    .await;
    apply(
        &mut runtime,
        base_script(0.5, pattern_config(2.0, 4.0, &[0.0, 2.0]), false),
    )
    .await;

    let state = runtime.state().read().await;
    let p = state.patterns.get(&PATTERN).expect("pattern exists");
    assert!(p.pending_content.is_none(), "no swap left pending");
    assert_eq!(p.content.steps.len(), 2, "new content applied immediately");
}
