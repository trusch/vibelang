use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vibelang_core::compat::{channel, Instant, RwLock};
use vibelang_core::handlers::VoicesHandler;
use vibelang_core::midi::{
    Channel, ControlValue, GroupChannel, MidiMessage as NewMidiMessage, TimestampedMidiEvent,
    Velocity,
};
use vibelang_core::traits::{FadeTarget, Voices};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, BusId, GroupId, GroupState, MidiHandler, NodeId,
    ParamMap, State, VoiceConfig,
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
struct SetParamCall {
    node: NodeId,
    param: String,
    value: f32,
}

#[derive(Default)]
struct RecordingBackend {
    set_param_calls: Mutex<Vec<SetParamCall>>,
}

impl RecordingBackend {
    fn set_param_calls(&self) -> Vec<SetParamCall> {
        self.set_param_calls.lock().unwrap().clone()
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
        _def: &str,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
        _params: &ParamMap,
    ) -> Result<(), Self::Error> {
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
        Ok(())
    }

    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn set_param(&self, node: NodeId, param: &str, value: f32) -> Result<(), Self::Error> {
        self.set_param_calls.lock().unwrap().push(SetParamCall {
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

fn insert_group(state: &mut State, id: GroupId) {
    state.groups.insert(
        id,
        GroupState {
            id,
            name: "cc_target".to_string(),
            parent: None,
            node_id: NodeId(100),
            audio_bus: BusId(16),
            link_synth_node_id: None,
            muted: false,
            soloed: false,
            params: ParamMap::new(),
            output_bus: None,
            output_channels: None,
        },
    );
}

#[tokio::test]
async fn cc32_authored_route_reaches_midi1_without_pipewire() {
    let script_state = ScriptEngine::new()
        .execute(
            r#"
            let target = voice("cc32_target");
            let dev = midi_device("legacy-controller");
            dev.cc32(74).channel(1).curve("log").to(target, "cutoff", 200.0, 8000.0);
        "#,
        )
        .expect("cc32 compatibility script must execute");
    assert_eq!(script_state.advanced_cc_routes.len(), 1);
    let route = script_state.advanced_cc_routes[0].clone();
    let FadeTarget::Voice(voice_id) = route.target else {
        panic!("cc32 alias must retain its voice target");
    };

    let group_id = GroupId::new(1);
    let runtime_state = Arc::new(RwLock::new(State::default()));
    {
        let mut state = runtime_state.write().await;
        state.synthdefs.insert("test_synth".to_string());
        insert_group(&mut state, group_id);
    }
    let backend = Arc::new(RecordingBackend::default());
    let voices = VoicesHandler::new(Arc::clone(&backend), Arc::clone(&runtime_state));
    voices
        .create(
            voice_id,
            VoiceConfig::new("cc32_target", "test_synth", group_id),
        )
        .await
        .unwrap();
    let (runtime_tx, _runtime_rx) = channel(32);
    let midi = MidiHandler::new(backend, Arc::clone(&runtime_state), runtime_tx);
    midi.apply_advanced_cc_routes(&script_state.advanced_cc_routes)
        .await;

    assert!(midi.event_sender().try_send(TimestampedMidiEvent::new(
        1,
        Instant::now(),
        route.device_id,
        NewMidiMessage::ControlChange {
            channel: Channel::new(0),
            controller: 74,
            value: 127,
        },
    )));
    midi.tick().await;
    assert_eq!(
        runtime_state.read().await.voices[&voice_id].config.params["cutoff"],
        8000.0
    );
}

#[tokio::test]
async fn map_cc_authored_group_route_applies_one_native_ump_write() {
    let script_state = ScriptEngine::new()
        .execute(
            r#"
            let target = define_group("cc_target", || {});
            let dev = midi_device("ump-controller");
            dev.map_cc(74).curve("s-curve").to(target, "amp", 0.0, 1.0);
        "#,
        )
        .expect("map_cc script must execute");
    let route = script_state.advanced_cc_routes[0].clone();
    let FadeTarget::Group(group_id) = route.target else {
        panic!("map_cc route must retain its group target");
    };

    let runtime_state = Arc::new(RwLock::new(State::default()));
    {
        let mut state = runtime_state.write().await;
        insert_group(&mut state, group_id);
    }
    let backend = Arc::new(RecordingBackend::default());
    let (runtime_tx, _runtime_rx) = channel(32);
    let midi = MidiHandler::new(Arc::clone(&backend), runtime_state, runtime_tx);
    midi.apply_advanced_cc_routes(&script_state.advanced_cc_routes)
        .await;

    assert!(midi.event_sender().try_send(TimestampedMidiEvent::new(
        1,
        Instant::now(),
        route.device_id,
        NewMidiMessage::Midi2ControlChange {
            group_channel: GroupChannel::new(7, 0),
            controller: 74,
            value: ControlValue::from_32bit(0x4000_0000),
        },
    )));
    midi.tick().await;

    let calls = backend.set_param_calls();
    assert_eq!(calls.len(), 1, "one UMP CC must cause one backend write");
    assert_eq!(calls[0].node, NodeId(100));
    assert_eq!(calls[0].param, "amp");
    assert!((calls[0].value - 0.15625).abs() < 0.00001);
}

#[test]
fn control_value_widening_has_exact_midi1_endpoints() {
    assert_eq!(ControlValue::from_7bit(0), ControlValue::ZERO);
    assert_eq!(ControlValue::from_7bit(127), ControlValue::MAX);
    assert_eq!(
        ControlValue::from_7bit(0).as_f32().to_bits(),
        0.0f32.to_bits()
    );
    assert_eq!(
        ControlValue::from_7bit(127).as_f32().to_bits(),
        1.0f32.to_bits()
    );
    assert_eq!(
        Velocity::from_midi1(127).as_f32().to_bits(),
        1.0f32.to_bits()
    );
}
