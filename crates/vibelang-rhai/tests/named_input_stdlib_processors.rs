//! End-to-end coverage for stdlib processor named-input routing.
//!
//! These tests import the stdlib processor scripts through the public Rhai
//! module path, apply user-facing `target.input("name").from(source)` routes
//! through `Runtime::apply_reload`, and assert against the committed runtime
//! state and backend calls.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use async_trait::async_trait;
use tracing::{Event, Id, Metadata, Subscriber};
use vibelang_core::compat::Instant;
use vibelang_core::handlers::InputRouteSrc;
use vibelang_core::message::{ReloadMessage, SynthDefMessage, VoiceMessage};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, NodeId, ParamMap, Runtime, VoiceId,
};
use vibelang_rhai::ScriptEngine;

#[derive(Debug)]
struct MockError;

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock backend error")
    }
}

impl std::error::Error for MockError {}

#[derive(Clone, Debug)]
struct SynthCreate {
    def: String,
    params: ParamMap,
}

#[derive(Debug, Default)]
struct RecordingBackend {
    creates: Mutex<Vec<SynthCreate>>,
    frees: Mutex<Vec<NodeId>>,
}

impl RecordingBackend {
    fn synth_creates(&self) -> Vec<SynthCreate> {
        self.creates.lock().unwrap().clone()
    }

    fn freed_nodes(&self) -> Vec<NodeId> {
        self.frees.lock().unwrap().clone()
    }
}

#[async_trait]
impl Backend for RecordingBackend {
    type Error = MockError;

    async fn load_synthdef(&self, _name: &str, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn create_synth(
        &self,
        def: &str,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.lock().unwrap().push(SynthCreate {
            def: def.to_string(),
            params: params.clone(),
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

    async fn free_node(&self, node: NodeId) -> Result<(), Self::Error> {
        self.frees.lock().unwrap().push(node);
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
            channels: 1,
            sample_rate: 44100.0,
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
            sample_rate: 44100.0,
        })
    }

    async fn write_buffer(&self, _id: BufferId, _path: &Path) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn free_buffer(&self, _id: BufferId) -> Result<(), Self::Error> {
        Ok(())
    }

    fn current_time(&self) -> Instant {
        Instant::now()
    }
}

#[derive(Clone, Default)]
struct CapturedEvents {
    lines: Arc<Mutex<Vec<String>>>,
}

impl CapturedEvents {
    fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

struct CaptureSubscriber {
    sink: CapturedEvents,
}

impl Subscriber for CaptureSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        struct Visitor<'a>(&'a mut String);

        impl tracing::field::Visit for Visitor<'_> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    let _ = std::fmt::Write::write_fmt(self.0, format_args!("{:?}", value));
                }
            }

            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "message" {
                    self.0.push_str(value);
                }
            }
        }

        let mut line = format!("{} ", event.metadata().level());
        event.record(&mut Visitor(&mut line));
        self.sink.lines.lock().unwrap().push(line);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

fn install_tracing_capture() -> (CapturedEvents, tracing::dispatcher::DefaultGuard) {
    let captured = CapturedEvents::default();
    let subscriber = CaptureSubscriber {
        sink: captured.clone(),
    };
    let guard = tracing::subscriber::set_default(subscriber);
    (captured, guard)
}

fn registry_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fnv1a_id(name: &str) -> u32 {
    const FNV_OFFSET_BASIS: u32 = 2166136261;
    const FNV_PRIME: u32 = 16777619;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    if hash == 0 {
        1
    } else {
        hash
    }
}

fn voice_id(name: &str) -> VoiceId {
    VoiceId::new(fnv1a_id(name))
}

fn group_id(name: &str) -> GroupId {
    GroupId::new(fnv1a_id(name))
}

const TEST_SOURCE_SYNTHDEFS: &str = r#"
define_synthdef("named_input_std_src_mono")
    .output("out", 1)
    .body(|| [sin_osc_ar(220.0, 0.0)]);

define_synthdef("named_input_std_src_mono_alt")
    .output("out", 1)
    .body(|| [sin_osc_ar(330.0, 0.0)]);

define_synthdef("named_input_std_src_stereo")
    .output("out", 2)
    .body(|| [sin_osc_ar(220.0, 0.0)]);
"#;

async fn apply_script(
    runtime: &mut Runtime<RecordingBackend>,
    imports: &[&str],
    body: &str,
    synthdefs_to_load: &[&str],
) {
    vibelang_dsp::set_deploy_callback(|_| Ok(()));

    let script_path = write_script(imports, body);
    let mut engine = ScriptEngine::new();
    engine.add_import_path(stdlib_crate_dir());
    let state = engine
        .execute_file(&script_path)
        .expect("script should execute");
    fs::remove_file(&script_path).ok();

    for name in synthdefs_to_load {
        runtime
            .send(
                SynthDefMessage::Load {
                    name: (*name).to_string(),
                    data: Vec::new(),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;
    }

    runtime
        .send(ReloadMessage::Apply { state }.into())
        .await
        .unwrap();
    runtime.tick().await;
}

async fn reload_synthdef(runtime: &mut Runtime<RecordingBackend>, name: &str) {
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

fn write_script(imports: &[&str], body: &str) -> PathBuf {
    let mut script = String::new();
    for import in imports {
        script.push_str(&format!("import \"{}\";\n", import));
    }
    script.push_str(body);

    let path = temp_script_path();
    fs::write(&path, script).expect("write temp script");
    path
}

fn temp_script_path() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "vibelang-rhai-named-input-stdlib-{}-{}.vibe",
        std::process::id(),
        nonce
    ))
}

fn stdlib_crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vibelang-std")
}

fn input_link_creates(runtime: &Runtime<RecordingBackend>, def: &str) -> Vec<SynthCreate> {
    runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == def)
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn stdlib_stereo_named_input_route_materializes_without_warnings() {
    let _registry = registry_lock();
    let (captured, _trace) = install_tracing_capture();
    let mut runtime = Runtime::new(RecordingBackend::default());

    let script = format!(
        r#"
        {}

        let src = voice("stdlib_happy_stereo_src")
            .synth("named_input_std_src_stereo")
            .group("stdlib_happy_rt");
        let target = voice("stdlib_happy_lowpass")
            .synth("lowpass_stereo")
            .group("stdlib_happy_rt");
        target.input("in").from(src);
        "#,
        TEST_SOURCE_SYNTHDEFS
    );
    apply_script(
        &mut runtime,
        &["stdlib/processors/filters/lowpass_stereo.vibe"],
        &script,
        &["named_input_std_src_stereo", "lowpass_stereo"],
    )
    .await;

    let source = voice_id("stdlib_happy_stereo_src");
    let target = voice_id("stdlib_happy_lowpass");
    let route = InputRouteSrc::Voice(source, "out".to_string());

    {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&(target, "in".to_string())),
            Some(&vec![route.clone()])
        );
        assert!(state
            .input_route_synths
            .contains_key(&(target, "in".to_string(), route)));
    }

    assert_eq!(
        input_link_creates(&runtime, "input_link_2").len(),
        1,
        "lowpass_stereo.in should be fed by one stereo input link"
    );
    assert!(
        !captured
            .lines()
            .iter()
            .any(|line| line.contains("failed to plan input link")
                || line.contains("dropped input route")),
        "happy path should not warn about route planning or dropped input routes: {:?}",
        captured.lines()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn stdlib_unpatched_inputs_default_to_silence_and_in_autofeeds() {
    let _registry = registry_lock();
    let mut runtime = Runtime::new(RecordingBackend::default());

    let script = r#"
        let _ring = voice("stdlib_defaults_ring")
            .synth("ring_mod_mono")
            .group("stdlib_defaults_rt");
        let _lowpass = voice("stdlib_defaults_lowpass")
            .synth("lowpass_stereo")
            .group("stdlib_defaults_rt");
    "#;
    apply_script(
        &mut runtime,
        &[
            "stdlib/processors/modulation/ring_mod_mono.vibe",
            "stdlib/processors/filters/lowpass_stereo.vibe",
        ],
        script,
        &["ring_mod_mono", "lowpass_stereo"],
    )
    .await;

    let ring = voice_id("stdlib_defaults_ring");
    let lowpass = voice_id("stdlib_defaults_lowpass");
    let group = group_id("stdlib_defaults_rt");
    let (silent_bus, carrier_bus, modulator_bus) = {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&(ring, "carrier".to_string())),
            Some(&vec![InputRouteSrc::Silent]),
            "ring_mod_mono.carrier should not hear any unpatched source"
        );
        assert_eq!(
            state.input_routes.get(&(ring, "modulator".to_string())),
            Some(&vec![InputRouteSrc::Silent]),
            "ring_mod_mono.modulator should not hear any unpatched source"
        );
        assert_eq!(
            state.input_routes.get(&(lowpass, "in".to_string())),
            Some(&vec![InputRouteSrc::Group(group)]),
            "stereo input named 'in' should autofeed from the parent group"
        );

        let ring_state = state.voices.get(&ring).unwrap();
        let carrier_bus = ring_state
            .input_buses
            .iter()
            .find(|(name, _)| name == "carrier")
            .unwrap()
            .1;
        let modulator_bus = ring_state
            .input_buses
            .iter()
            .find(|(name, _)| name == "modulator")
            .unwrap()
            .1;
        (
            state.silent_ar_bus.expect("silent ar bus allocated"),
            carrier_bus,
            modulator_bus,
        )
    };

    let mono_links = input_link_creates(&runtime, "input_link_1");
    assert_eq!(
        mono_links.len(),
        2,
        "carrier and modulator should each get a silent mono input link"
    );
    assert!(
        mono_links
            .iter()
            .all(|link| link.params.get("in_bus") == Some(&(silent_bus.raw() as f32))),
        "unpatched ring_mod_mono inputs should be sourced from the silent bus: {:?}",
        mono_links
    );

    runtime
        .send(
            VoiceMessage::Trigger {
                id: ring,
                params: ParamMap::new(),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    let ring_creates: Vec<_> = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == "ring_mod_mono")
        .collect();
    assert_eq!(ring_creates.len(), 1);
    assert_eq!(
        ring_creates[0].params.get("__in0"),
        Some(&(carrier_bus.raw() as f32))
    );
    assert_eq!(
        ring_creates[0].params.get("__in1"),
        Some(&(modulator_bus.raw() as f32))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn stdlib_named_input_width_mismatches_are_rejected() {
    let _registry = registry_lock();
    let (captured, _trace) = install_tracing_capture();
    let mut runtime = Runtime::new(RecordingBackend::default());

    let script = format!(
        r#"
        {}

        let stereo = voice("stdlib_width_stereo_src")
            .synth("named_input_std_src_stereo")
            .group("stdlib_width_rt");
        let mono = voice("stdlib_width_mono_src")
            .synth("named_input_std_src_mono")
            .group("stdlib_width_rt");

        let ring = voice("stdlib_width_ring")
            .synth("ring_mod_mono")
            .group("stdlib_width_rt");
        ring.input("carrier").from(stereo);

        let mixer = voice("stdlib_width_mixer")
            .synth("mixer4_stereo")
            .group("stdlib_width_rt");
        mixer.input("ch1").from(mono);
        "#,
        TEST_SOURCE_SYNTHDEFS
    );
    apply_script(
        &mut runtime,
        &[
            "stdlib/processors/modulation/ring_mod_mono.vibe",
            "stdlib/processors/mixers/mixer4_stereo.vibe",
        ],
        &script,
        &[
            "named_input_std_src_stereo",
            "named_input_std_src_mono",
            "ring_mod_mono",
            "mixer4_stereo",
        ],
    )
    .await;

    let stereo = voice_id("stdlib_width_stereo_src");
    let mono = voice_id("stdlib_width_mono_src");
    let ring = voice_id("stdlib_width_ring");
    let mixer = voice_id("stdlib_width_mixer");

    {
        let state = runtime.state().read().await;
        assert!(!state.input_route_synths.contains_key(&(
            ring,
            "carrier".to_string(),
            InputRouteSrc::Voice(stereo, "out".to_string())
        )));
        assert!(!state.input_route_synths.contains_key(&(
            mixer,
            "ch1".to_string(),
            InputRouteSrc::Voice(mono, "out".to_string())
        )));
        assert!(
            state
                .input_routes
                .get(&(ring, "carrier".to_string()))
                .is_none(),
            "rejected stereo-to-mono route must not materialize"
        );
        assert!(
            state
                .input_routes
                .get(&(mixer, "ch1".to_string()))
                .is_none(),
            "rejected mono-to-stereo route must not materialize"
        );
    }

    assert!(
        input_link_creates(&runtime, "input_link_1")
            .iter()
            .all(|link| link.def != "named_input_std_src_stereo"),
        "width rejection should not silently downmix through a mono link"
    );
    let logs = captured.lines();
    assert!(
        logs.iter().any(|line| {
            line.contains("Input port 'carrier' expects 1 channel(s), source provides 2")
        }),
        "expected ring_mod_mono carrier width warning, got {:?}",
        logs
    );
    assert!(
        logs.iter()
            .any(|line| line.contains("Input port 'ch1' expects 2 channel(s), source provides 1")),
        "expected mixer4_stereo ch1 width warning, got {:?}",
        logs
    );
}

#[tokio::test(flavor = "current_thread")]
async fn stdlib_named_input_disconnect_and_reconnect_replaces_links() {
    let _registry = registry_lock();
    let mut runtime = Runtime::new(RecordingBackend::default());

    let first = format!(
        r#"
        {}

        let a = voice("stdlib_reconnect_src_a")
            .synth("named_input_std_src_mono")
            .group("stdlib_reconnect_rt");
        let _b = voice("stdlib_reconnect_src_b")
            .synth("named_input_std_src_mono_alt")
            .group("stdlib_reconnect_rt");
        let ring = voice("stdlib_reconnect_ring")
            .synth("ring_mod_mono")
            .group("stdlib_reconnect_rt");
        ring.input("carrier").from(a);
        "#,
        TEST_SOURCE_SYNTHDEFS
    );
    apply_script(
        &mut runtime,
        &["stdlib/processors/modulation/ring_mod_mono.vibe"],
        &first,
        &[
            "named_input_std_src_mono",
            "named_input_std_src_mono_alt",
            "ring_mod_mono",
        ],
    )
    .await;

    let src_a = voice_id("stdlib_reconnect_src_a");
    let src_b = voice_id("stdlib_reconnect_src_b");
    let ring = voice_id("stdlib_reconnect_ring");
    let route_a = InputRouteSrc::Voice(src_a, "out".to_string());
    let route_b = InputRouteSrc::Voice(src_b, "out".to_string());

    {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&(ring, "carrier".to_string())),
            Some(&vec![route_a.clone()])
        );
        assert!(state.input_route_synths.contains_key(&(
            ring,
            "carrier".to_string(),
            route_a.clone()
        )));
    }

    let disconnect = format!(
        r#"
        {}

        let _a = voice("stdlib_reconnect_src_a")
            .synth("named_input_std_src_mono")
            .group("stdlib_reconnect_rt");
        let _b = voice("stdlib_reconnect_src_b")
            .synth("named_input_std_src_mono_alt")
            .group("stdlib_reconnect_rt");
        let ring = voice("stdlib_reconnect_ring")
            .synth("ring_mod_mono")
            .group("stdlib_reconnect_rt");
        ring.input("carrier").disconnect();
        "#,
        TEST_SOURCE_SYNTHDEFS
    );
    apply_script(
        &mut runtime,
        &["stdlib/processors/modulation/ring_mod_mono.vibe"],
        &disconnect,
        &[
            "named_input_std_src_mono",
            "named_input_std_src_mono_alt",
            "ring_mod_mono",
        ],
    )
    .await;

    {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&(ring, "carrier".to_string())),
            Some(&vec![InputRouteSrc::Silent])
        );
        assert!(!state
            .input_route_synths
            .contains_key(&(ring, "carrier".to_string(), route_a)));
        assert!(state.input_route_synths.contains_key(&(
            ring,
            "carrier".to_string(),
            InputRouteSrc::Silent
        )));
    }

    let reconnect = format!(
        r#"
        {}

        let _a = voice("stdlib_reconnect_src_a")
            .synth("named_input_std_src_mono")
            .group("stdlib_reconnect_rt");
        let b = voice("stdlib_reconnect_src_b")
            .synth("named_input_std_src_mono_alt")
            .group("stdlib_reconnect_rt");
        let ring = voice("stdlib_reconnect_ring")
            .synth("ring_mod_mono")
            .group("stdlib_reconnect_rt");
        ring.input("carrier").from(b);
        "#,
        TEST_SOURCE_SYNTHDEFS
    );
    apply_script(
        &mut runtime,
        &["stdlib/processors/modulation/ring_mod_mono.vibe"],
        &reconnect,
        &[
            "named_input_std_src_mono",
            "named_input_std_src_mono_alt",
            "ring_mod_mono",
        ],
    )
    .await;

    {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&(ring, "carrier".to_string())),
            Some(&vec![route_b.clone()])
        );
        assert!(!state.input_route_synths.contains_key(&(
            ring,
            "carrier".to_string(),
            InputRouteSrc::Silent
        )));
        assert!(state
            .input_route_synths
            .contains_key(&(ring, "carrier".to_string(), route_b)));
    }

    assert_eq!(
        runtime.backend().freed_nodes().len(),
        2,
        "disconnect and reconnect should each free the previous carrier link"
    );
    assert_eq!(
        input_link_creates(&runtime, "input_link_1").len(),
        4,
        "first carrier, default modulator, disconnect, and reconnect links should be spawned"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn stdlib_input_rename_reload_drops_dependent_route_with_warning() {
    let _registry = registry_lock();
    let (captured, _trace) = install_tracing_capture();
    let mut runtime = Runtime::new(RecordingBackend::default());

    let first = format!(
        r#"
        {}

        let src = voice("stdlib_reload_src")
            .synth("named_input_std_src_mono")
            .group("stdlib_reload_rt");
        let ring = voice("stdlib_reload_ring")
            .synth("ring_mod_mono")
            .group("stdlib_reload_rt");
        ring.input("carrier").from(src);
        "#,
        TEST_SOURCE_SYNTHDEFS
    );
    apply_script(
        &mut runtime,
        &["stdlib/processors/modulation/ring_mod_mono.vibe"],
        &first,
        &["named_input_std_src_mono", "ring_mod_mono"],
    )
    .await;

    let src = voice_id("stdlib_reload_src");
    let ring = voice_id("stdlib_reload_ring");
    let old_route = InputRouteSrc::Voice(src, "out".to_string());
    {
        let state = runtime.state().read().await;
        assert!(state.input_route_synths.contains_key(&(
            ring,
            "carrier".to_string(),
            old_route.clone()
        )));
    }

    vibelang_dsp::register_synthdef_inputs(
        "ring_mod_mono".to_string(),
        vec![
            vibelang_dsp::InputPort::ar("mod_in", 1),
            vibelang_dsp::InputPort::ar("modulator", 1),
        ],
    );
    reload_synthdef(&mut runtime, "ring_mod_mono").await;

    {
        let state = runtime.state().read().await;
        assert!(!state
            .input_routes
            .contains_key(&(ring, "carrier".to_string())));
        assert!(!state
            .input_route_synths
            .contains_key(&(ring, "carrier".to_string(), old_route)));
    }
    assert_eq!(
        runtime.backend().freed_nodes().len(),
        1,
        "renaming carrier should free the dependent input link"
    );

    let second = r#"
        let src = voice("stdlib_reload_src")
            .synth("named_input_std_src_mono")
            .group("stdlib_reload_rt");
        let ring = voice("stdlib_reload_ring")
            .synth("ring_mod_mono")
            .group("stdlib_reload_rt");
        ring.input("mod_in").from(src);
    "#;
    apply_script(
        &mut runtime,
        &[],
        second,
        &["named_input_std_src_mono", "ring_mod_mono"],
    )
    .await;

    {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&(ring, "mod_in".to_string())),
            Some(&vec![InputRouteSrc::Voice(src, "out".to_string())])
        );
    }

    let logs = captured.lines();
    assert!(
        logs.iter().any(|line| {
            line.contains("dropped input route")
                && line.contains("ring_mod_mono")
                && line.contains("carrier")
                && line.contains("input removed")
        }),
        "expected input rename warning, got {:?}",
        logs
    );
}
