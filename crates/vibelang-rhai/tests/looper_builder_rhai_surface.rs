use vibelang_core::reload::LooperConfig;
use vibelang_core::types::{MidiDeviceId, VoiceId};
use vibelang_rhai::ScriptEngine;

fn execute(script: &str) -> vibelang_core::reload::ScriptState {
    ScriptEngine::new()
        .execute(script)
        .unwrap_or_else(|err| panic!("script failed: {err}"))
}

fn voice_id(state: &vibelang_core::reload::ScriptState, name: &str) -> VoiceId {
    state
        .voices
        .iter()
        .find_map(|(id, config)| (config.name == name).then_some(*id))
        .unwrap_or_else(|| panic!("voice {name} should exist"))
}

fn looper_for_channel(loopers: &[LooperConfig], channel: Option<u8>) -> &LooperConfig {
    loopers
        .iter()
        .find(|config| config.channel == channel)
        .unwrap_or_else(|| panic!("looper for channel {channel:?} should exist"))
}

#[test]
fn same_device_distinct_channel_loopers_are_preserved_in_script_state() {
    let state = execute(
        r#"
        let dev = midi_device("vibelang-test-loopers");
        let left = voice("left").synth("test_synth").apply();
        let right = voice("right").synth("test_synth").apply();

        dev.looper().channel(1).to(left);
        dev.looper().channel(2).to(right);
        "#,
    );

    assert_eq!(state.loopers.len(), 2);
    let device_id = MidiDeviceId::new(u32::MAX);
    assert_eq!(
        looper_for_channel(&state.loopers, Some(0)).device_id,
        device_id
    );
    assert_eq!(
        looper_for_channel(&state.loopers, Some(1)).device_id,
        device_id
    );
    assert_eq!(
        looper_for_channel(&state.loopers, Some(0)).voice_id,
        voice_id(&state, "left")
    );
    assert_eq!(
        looper_for_channel(&state.loopers, Some(1)).voice_id,
        voice_id(&state, "right")
    );
}

#[test]
fn same_device_same_channel_looper_replaces_that_identity_only() {
    let state = execute(
        r#"
        let dev = midi_device("vibelang-test-loopers");
        let first = voice("first").synth("test_synth").apply();
        let second = voice("second").synth("test_synth").apply();
        let replacement = voice("replacement").synth("test_synth").apply();

        dev.looper().channel(1).to(first);
        dev.looper().channel(2).to(second);
        dev.looper().channel(1).to(replacement);
        "#,
    );

    assert_eq!(state.loopers.len(), 2);
    assert_eq!(
        looper_for_channel(&state.loopers, Some(0)).voice_id,
        voice_id(&state, "replacement")
    );
    assert_eq!(
        looper_for_channel(&state.loopers, Some(1)).voice_id,
        voice_id(&state, "second")
    );
}
