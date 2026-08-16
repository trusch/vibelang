# Generated HTTP route and schema index

> Generated from `api/http-api-snapshot-v1.json`; edit Axum routes or Serde DTOs and regenerate instead of editing this file.

The source snapshot contains **103 method/path registrations** and **108 public serialized/deserialized Rust types**. It records declarations and feature gates, not handler effectiveness or runtime status semantics.

## Routes

| Method | Path | Handler | Availability |
|---|---|---|---|
| `GET` | `/capabilities` | `routes::v2::capabilities` | always |
| `GET` | `/capabilities/details` | `routes::v2::capability_details` | always |
| `GET` | `/effects` | `routes::effects::list_effects` | always |
| `DELETE` | `/effects/{id}` | `routes::effects::delete_effect` | always |
| `GET` | `/effects/{id}` | `routes::effects::get_effect` | always |
| `PATCH` | `/effects/{id}` | `routes::effects::update_effect` | always |
| `PUT` | `/effects/{id}/params/{param}` | `routes::effects::set_effect_param` | always |
| `POST` | `/eval` | `routes::eval::eval_code` | always |
| `DELETE` | `/fades` | `routes::fades::cancel_fade` | always |
| `GET` | `/fades` | `routes::live::get_active_fades` | always |
| `POST` | `/fades` | `routes::fades::start_fade` | always |
| `POST` | `/fades/effect/{id}` | `routes::fades::fade_effect` | always |
| `POST` | `/fades/group/{path}` | `routes::fades::fade_group` | always |
| `POST` | `/fades/melody/{name}` | `routes::fades::fade_melody` | always |
| `POST` | `/fades/pattern/{name}` | `routes::fades::fade_pattern` | always |
| `POST` | `/fades/voice/{name}` | `routes::fades::fade_voice` | always |
| `GET` | `/groups` | `routes::groups::list_groups` | always |
| `GET` | `/groups/{id}` | `routes::groups::get_group` | always |
| `PATCH` | `/groups/{id}` | `routes::groups::update_group` | always |
| `POST` | `/groups/{id}/mute` | `routes::groups::mute_group` | always |
| `PUT` | `/groups/{id}/params/{param}` | `routes::groups::set_group_param` | always |
| `POST` | `/groups/{id}/solo` | `routes::groups::solo_group` | always |
| `POST` | `/groups/{id}/unmute` | `routes::groups::unmute_group` | always |
| `POST` | `/groups/{id}/unsolo` | `routes::groups::unsolo_group` | always |
| `GET` | `/live` | `routes::live::get_live_state` | always |
| `GET` | `/live/fades` | `routes::live::get_active_fades` | always |
| `GET` | `/live/meters` | `routes::live::get_meters` | always |
| `GET` | `/live/transport` | `routes::live::get_transport_state` | always |
| `GET` | `/melodies` | `routes::melodies::list_melodies` | always |
| `POST` | `/melodies` | `routes::melodies::create_melody` | always |
| `DELETE` | `/melodies/{id}` | `routes::melodies::delete_melody` | always |
| `GET` | `/melodies/{id}` | `routes::melodies::get_melody` | always |
| `PATCH` | `/melodies/{id}` | `routes::melodies::update_melody` | always |
| `POST` | `/melodies/{id}/start` | `routes::melodies::start_melody` | always |
| `POST` | `/melodies/{id}/stop` | `routes::melodies::stop_melody` | always |
| `POST` | `/midi/cc` | `routes::midi::send_cc` | `feature = "midi"` |
| `POST` | `/midi/clock/disable` | `routes::midi::disable_clock_output` | `feature = "midi"` |
| `POST` | `/midi/clock/enable` | `routes::midi::enable_clock_output` | `feature = "midi"` |
| `POST` | `/midi/close` | `routes::midi::close_device` | `feature = "midi"` |
| `GET` | `/midi/devices` | `routes::midi::list_devices` | `feature = "midi"` |
| `POST` | `/midi/input/open` | `routes::midi::open_input` | `feature = "midi"` |
| `POST` | `/midi/note/off` | `routes::midi::send_note_off` | `feature = "midi"` |
| `POST` | `/midi/note/on` | `routes::midi::send_note_on` | `feature = "midi"` |
| `POST` | `/midi/output/open` | `routes::midi::open_output` | `feature = "midi"` |
| `GET` | `/midi/readiness` | `routes::midi::get_readiness` | `feature = "midi"` |
| `POST` | `/midi/record/start` | `routes::midi::start_recording` | `feature = "midi"` |
| `POST` | `/midi/record/stop` | `routes::midi::stop_recording` | `feature = "midi"` |
| `POST` | `/midi/route/keyboard` | `routes::midi::add_keyboard_route` | `feature = "midi"` |
| `DELETE` | `/midi/route/{index}` | `routes::midi::remove_keyboard_route` | `feature = "midi"` |
| `DELETE` | `/midi/routes` | `routes::midi::clear_routes` | `feature = "midi"` |
| `GET` | `/midi/routes` | `routes::midi::list_routes` | `feature = "midi"` |
| `POST` | `/midi/transport/continue` | `routes::midi::send_midi_continue` | `feature = "midi"` |
| `POST` | `/midi/transport/start` | `routes::midi::send_midi_start` | `feature = "midi"` |
| `POST` | `/midi/transport/stop` | `routes::midi::send_midi_stop` | `feature = "midi"` |
| `GET` | `/mutation-status` | `routes::v2::mutation_status` | always |
| `GET` | `/patterns` | `routes::patterns::list_patterns` | always |
| `POST` | `/patterns` | `routes::patterns::create_pattern` | always |
| `DELETE` | `/patterns/{id}` | `routes::patterns::delete_pattern` | always |
| `GET` | `/patterns/{id}` | `routes::patterns::get_pattern` | always |
| `PATCH` | `/patterns/{id}` | `routes::patterns::update_pattern` | always |
| `PUT` | `/patterns/{id}/params/{param}` | `routes::patterns::set_pattern_param` | always |
| `POST` | `/patterns/{id}/start` | `routes::patterns::start_pattern` | always |
| `POST` | `/patterns/{id}/stop` | `routes::patterns::stop_pattern` | always |
| `GET` | `/receipt-events` | `routes::v2::receipt_events` | always |
| `DELETE` | `/receipts/{attempt_id}` | `routes::v2::cancel_receipt` | always |
| `GET` | `/receipts/{attempt_id}` | `routes::eval::get_receipt` | always |
| `GET` | `/recordings` | `routes::recordings::list_recordings` | `not(target_arch = "wasm32")` |
| `GET` | `/recordings/{id}` | `routes::recordings::get_recording` | `not(target_arch = "wasm32")` |
| `POST` | `/recordings/{id}/cancel` | `routes::recordings::cancel_recording` | `not(target_arch = "wasm32")` |
| `POST` | `/recordings/{id}/stop` | `routes::recordings::stop_recording` | `not(target_arch = "wasm32")` |
| `GET` | `/samples` | `routes::samples::list_samples` | always |
| `POST` | `/samples` | `routes::samples::load_sample` | always |
| `DELETE` | `/samples/{id}` | `routes::samples::delete_sample` | always |
| `GET` | `/samples/{id}` | `routes::samples::get_sample` | always |
| `GET` | `/sequences` | `routes::sequences::list_sequences` | always |
| `POST` | `/sequences` | `routes::sequences::create_sequence` | always |
| `DELETE` | `/sequences/{id}` | `routes::sequences::delete_sequence` | always |
| `GET` | `/sequences/{id}` | `routes::sequences::get_sequence` | always |
| `PATCH` | `/sequences/{id}` | `routes::sequences::update_sequence` | always |
| `POST` | `/sequences/{id}/pause` | `routes::sequences::pause_sequence` | always |
| `POST` | `/sequences/{id}/resume` | `routes::sequences::resume_sequence` | always |
| `POST` | `/sequences/{id}/start` | `routes::sequences::start_sequence` | always |
| `POST` | `/sequences/{id}/stop` | `routes::sequences::stop_sequence` | always |
| `GET` | `/synthdefs` | `routes::synthdefs::list_synthdefs` | always |
| `GET` | `/synthdefs/{name}` | `routes::synthdefs::get_synthdef` | always |
| `GET` | `/transport` | `routes::transport::get_transport` | always |
| `PATCH` | `/transport` | `routes::transport::update_transport` | always |
| `POST` | `/transport/seek` | `routes::transport::seek_transport` | always |
| `POST` | `/transport/start` | `routes::transport::start_transport` | always |
| `POST` | `/transport/stop` | `routes::transport::stop_transport` | always |
| `GET` | `/voices` | `routes::voices::list_voices` | always |
| `POST` | `/voices` | `routes::voices::create_voice` | always |
| `DELETE` | `/voices/{id}` | `routes::voices::delete_voice` | always |
| `GET` | `/voices/{id}` | `routes::voices::get_voice` | always |
| `PATCH` | `/voices/{id}` | `routes::voices::update_voice` | always |
| `POST` | `/voices/{id}/mute` | `routes::voices::mute_voice` | always |
| `POST` | `/voices/{id}/note-off` | `routes::voices::note_off` | always |
| `POST` | `/voices/{id}/note-on` | `routes::voices::note_on` | always |
| `PUT` | `/voices/{id}/params/{param}` | `routes::voices::set_voice_param` | always |
| `POST` | `/voices/{id}/stop` | `routes::voices::stop_voice` | always |
| `POST` | `/voices/{id}/trigger` | `routes::voices::trigger_voice` | always |
| `POST` | `/voices/{id}/unmute` | `routes::voices::unmute_voice` | always |
| `GET` | `/ws` | `websocket::ws_handler` | always |

## Serde types

| Type | Kind | Source | Direction |
|---|---|---|---|
| `MutationHttpResponse` | `struct` | [`crates/vibelang-http/src/lib.rs`](../../../crates/vibelang-http/src/lib.rs) | `Serialize` |
| `ActiveFade` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `ActiveSequence` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `ActiveSynth` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `Effect` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `EffectCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `EffectUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `ErrorResponse` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `FadeCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `FadeTargetType` | `enum` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `Group` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `GroupCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `GroupUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `HttpCapabilities` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `HttpCapabilityDetails` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `HttpErrorDetail` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `HttpErrorEnvelope` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `HttpSecurityCapabilities` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `HttpSecurityPolicyDetails` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `LiveState` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `LoopState` | `enum` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `LoopStatus` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `Melody` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `MelodyCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `MelodyEvent` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `MelodyUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `MeterLevel` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `NoteOffRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `NoteOnRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `ParamSet` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `Pattern` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `PatternCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `PatternEvent` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `PatternUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `Recording` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `RecordingStatus` | `enum` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `Revisioned` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `Sample` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `SampleLoad` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `SampleSlice` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `SeekRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `Sequence` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `SequenceClip` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `SequenceCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `SequenceStartRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `SequenceUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `SourceLocation` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `StartRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `StopRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `SynthDefInfo` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `SynthDefParam` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `SynthDefSource` | `enum` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `TimeSignature` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `TransportState` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `TransportUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `TriggerRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2EffectUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2FadeCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2GroupUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2LoopControlRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2MelodyCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2MelodyEvent` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `V2MelodyUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2NoteOffRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2NoteOnRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2ParamSet` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2PatternCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2PatternEvent` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `V2PatternUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2SampleLoad` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2SeekRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2SequenceClip` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize + Serialize` |
| `V2SequenceCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2SequenceStartRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2SequenceUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2TimeSignature` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2TransportUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2TriggerRequest` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2VoiceCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `V2VoiceUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `Voice` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Serialize` |
| `VoiceCreate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `VoiceUpdate` | `struct` | [`crates/vibelang-http/src/models.rs`](../../../crates/vibelang-http/src/models.rs) | `Deserialize` |
| `EvalRequest` | `struct` | [`crates/vibelang-http/src/routes/eval.rs`](../../../crates/vibelang-http/src/routes/eval.rs) | `Deserialize` |
| `EvalResponse` | `struct` | [`crates/vibelang-http/src/routes/eval.rs`](../../../crates/vibelang-http/src/routes/eval.rs) | `Serialize` |
| `CancelFadeRequest` | `struct` | [`crates/vibelang-http/src/routes/fades.rs`](../../../crates/vibelang-http/src/routes/fades.rs) | `Deserialize` |
| `CurveSpec` | `enum` | [`crates/vibelang-http/src/routes/fades.rs`](../../../crates/vibelang-http/src/routes/fades.rs) | `Deserialize` |
| `EffectFadeRequest` | `struct` | [`crates/vibelang-http/src/routes/fades.rs`](../../../crates/vibelang-http/src/routes/fades.rs) | `Deserialize` |
| `GroupFadeRequest` | `struct` | [`crates/vibelang-http/src/routes/fades.rs`](../../../crates/vibelang-http/src/routes/fades.rs) | `Deserialize` |
| `MelodyFadeRequest` | `struct` | [`crates/vibelang-http/src/routes/fades.rs`](../../../crates/vibelang-http/src/routes/fades.rs) | `Deserialize` |
| `PatternFadeRequest` | `struct` | [`crates/vibelang-http/src/routes/fades.rs`](../../../crates/vibelang-http/src/routes/fades.rs) | `Deserialize` |
| `VoiceFadeRequest` | `struct` | [`crates/vibelang-http/src/routes/fades.rs`](../../../crates/vibelang-http/src/routes/fades.rs) | `Deserialize` |
| `AddKeyboardRouteRequest` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Deserialize` |
| `AddKeyboardRouteResponse` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Serialize` |
| `ClockOutputRequest` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Deserialize` |
| `ClockStatusDto` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Serialize` |
| `ErrorResponse` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Serialize` |
| `MidiDeviceDto` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Serialize` |
| `OpenDeviceRequest` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Deserialize` |
| `RecordedNoteDto` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Serialize` |
| `RecordingResultDto` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Serialize` |
| `RouteInfoDto` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Serialize` |
| `SendCcRequest` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Deserialize` |
| `SendNoteRequest` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Deserialize` |
| `StartRecordingRequest` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Deserialize` |
| `StopRecordingRequest` | `struct` | [`crates/vibelang-http/src/routes/midi.rs`](../../../crates/vibelang-http/src/routes/midi.rs) | `Deserialize` |
| `ReceiptEventsQuery` | `struct` | [`crates/vibelang-http/src/routes/v2.rs`](../../../crates/vibelang-http/src/routes/v2.rs) | `Deserialize` |
| `WebSocketEvent` | `struct` | [`crates/vibelang-http/src/websocket.rs`](../../../crates/vibelang-http/src/websocket.rs) | `Serialize` |
