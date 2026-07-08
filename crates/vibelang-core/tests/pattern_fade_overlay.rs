//! Pattern-fade overlay semantics (perf fix #7).
//!
//! A `FadeTarget::Pattern` fade must NOT rewrite the pattern's content on
//! every 500 Hz tick. Instead the live value rides in `PatternState::fade_overlay`
//! (a cheap float insert) and the pattern trigger path stamps that overlay on
//! top of each step at trigger time. These tests pin the observable contract:
//!
//! 1. Mid-fade, a triggered note uses the interpolated overlay value (the
//!    overlay overrides the step's recorded velocity), while `content` stays
//!    pristine — the whole point of the fix.
//! 2. On completion, the final value is flushed into `content` once (so a
//!    later reload diff sees the end-state) and the overlay is cleared.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::compat::Instant;
use vibelang_core::handlers::{FadesHandler, PatternsHandler, VoicesHandler};
use vibelang_core::{
    AddAction, Backend, Beat, BufferId, BufferInfo, Duration, FadeConfig, FadeTarget, Fades,
    GroupId, GroupState, NodeId, ParamMap, PatternConfig, PatternId, Patterns, State, Step,
    VoiceConfig, VoiceId, Voices,
};

// =========================================================================
// Mock backend — records the params of every scheduled synth creation
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
struct MockBackend {
    triggers: Arc<Mutex<Vec<ParamMap>>>,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            triggers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// `amp` param of every recorded trigger, in dispatch order.
    fn amp_seq(&self) -> Vec<f32> {
        self.triggers
            .lock()
            .unwrap()
            .iter()
            .map(|p| p.get("amp").copied().unwrap_or(f32::NAN))
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
        self.triggers.lock().unwrap().push(params.clone());
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
        _at: Option<Instant>,
    ) -> Result<(), MockError> {
        self.triggers.lock().unwrap().push(params.clone());
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

const SYNTH: &str = "test_synth";
const GROUP: GroupId = GroupId(1);
const VOICE: VoiceId = VoiceId(1);
const PATTERN: PatternId = PatternId(1);

/// One-step pattern (step at beat 0 carrying a recorded velocity `amp`).
async fn setup() -> (
    MockBackend,
    Arc<RwLock<State>>,
    PatternsHandler<MockBackend>,
    FadesHandler<MockBackend>,
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
    let fades = FadesHandler::new(Arc::new(backend.clone()), state.clone());

    let mut config = PatternConfig::with_length("p", VOICE, 4.0);
    let mut params = ParamMap::new();
    params.insert("amp".to_string(), 0.8); // recorded step velocity
    config.steps.push(Step {
        beat: Beat::from_f64(0.0),
        params,
    });
    patterns.create(PATTERN, config).await.expect("create");
    patterns.start(PATTERN).await.expect("start");

    (backend, state, patterns, fades)
}

/// Mid-fade: the overlay carries the live interpolated value, `content` stays
/// pristine, and a triggered note picks up the overlay (not the recorded
/// velocity, not the pristine content value).
#[tokio::test(flavor = "current_thread")]
async fn mid_fade_trigger_uses_interpolated_overlay_and_content_stays_pristine() {
    let (backend, state, patterns, fades) = setup().await;

    // Long fade so a single tick lands strictly between start and end.
    fades
        .fade(FadeConfig::new(
            FadeTarget::Pattern(PATTERN),
            "amp",
            0.0,
            Duration::from_beats(100.0),
        ))
        .await
        .expect("fade start");

    // Let a little wall-clock time pass, then advance the fade one tick.
    std::thread::sleep(std::time::Duration::from_millis(10));
    fades.tick().await;

    // The overlay now holds the interpolated value; content is untouched.
    let overlay_amp = {
        let s = state.read().await;
        let p = s.patterns.get(&PATTERN).unwrap();
        assert_eq!(
            p.content.steps[0].params.get("amp").copied(),
            Some(0.8),
            "content must stay pristine mid-fade (no per-tick rewrite)"
        );
        p.fade_overlay.get("amp").copied().expect("overlay set")
    };
    assert!(
        overlay_amp > 0.0 && overlay_amp < 1.0,
        "overlay must be interpolated (0 < v < 1), got {overlay_amp}"
    );

    // Trigger the step at beat 0: its amp must be the overlay value (voice amp
    // defaults to 1.0, and the overlay overrides the step's 0.8 velocity).
    patterns.tick(Beat::ZERO).await;
    let amps = backend.amp_seq();
    assert_eq!(amps.len(), 1, "exactly one step must fire");
    assert!(
        (amps[0] - overlay_amp).abs() < 1e-6,
        "triggered amp {} must equal the interpolated overlay {}",
        amps[0],
        overlay_amp
    );
}

/// On completion the final target value is flushed into `content` exactly once
/// and the overlay entry is cleared; a subsequent trigger reads the flushed
/// content value.
#[tokio::test(flavor = "current_thread")]
async fn completed_fade_flushes_target_into_content_and_clears_overlay() {
    let (backend, state, patterns, fades) = setup().await;

    // Zero-duration fade: completes on its first tick with value == target.
    fades
        .fade(FadeConfig::new(
            FadeTarget::Pattern(PATTERN),
            "amp",
            0.3,
            Duration::from_beats(0.0),
        ))
        .await
        .expect("fade start");
    fades.tick().await;

    {
        let s = state.read().await;
        let p = s.patterns.get(&PATTERN).unwrap();
        assert_eq!(
            p.content.steps[0].params.get("amp").copied(),
            Some(0.3),
            "completed fade must flush the target into content"
        );
        assert!(
            !p.fade_overlay.contains_key("amp"),
            "overlay entry must be cleared on completion"
        );
        assert!(
            s.active_fades.is_empty(),
            "completed fade must be removed from active_fades"
        );
    }

    // With the overlay gone, a trigger reads the flushed content value: the
    // step amp (0.3) times the default voice amp (1.0).
    patterns.tick(Beat::ZERO).await;
    let amps = backend.amp_seq();
    assert_eq!(amps.len(), 1, "exactly one step must fire");
    assert!(
        (amps[0] - 0.3).abs() < 1e-6,
        "post-fade trigger must use the flushed content value, got {}",
        amps[0]
    );
}
