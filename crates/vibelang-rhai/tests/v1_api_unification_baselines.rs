use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use vibelang_core::handlers::RouteDest;
use vibelang_core::reload::ScriptState;
use vibelang_core::types::{GroupId, VoiceId};
use vibelang_rhai::ScriptEngine;

const GOLDENS: &[&str] = &[
    "01_groups_voices",
    "02_patterns_melodies",
    "03_sequences_fades_effects",
    "04_routes",
    "05_resources_recording",
    "06_definitions_dsp",
    "07_midi",
    "08_transport",
];

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/api-unification/v1/golden")
}

fn sorted_names<'a>(values: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut values: Vec<_> = values.collect();
    values.sort_unstable();
    values
}

fn voice_name(state: &ScriptState, id: VoiceId) -> String {
    state
        .voices
        .get(&id)
        .map(|config| config.name.clone())
        .unwrap_or_else(|| format!("voice#{}", id.raw()))
}

fn group_name(state: &ScriptState, id: GroupId) -> String {
    state
        .groups
        .get(&id)
        .map(|config| config.name.clone())
        .unwrap_or_else(|| format!("group#{}", id.raw()))
}

fn join_names(mut names: Vec<String>) -> String {
    names.sort();
    names.join(",")
}

fn summarize(state: &ScriptState) -> String {
    let mut out = String::new();
    writeln!(out, "tempo={}", state.tempo).unwrap();
    writeln!(out, "time_signature={}", state.time_sig).unwrap();
    match state.quantization {
        Some(value) => writeln!(out, "quantization={value}").unwrap(),
        None => writeln!(out, "quantization=unset").unwrap(),
    }

    writeln!(
        out,
        "groups={}",
        sorted_names(state.groups.values().map(|config| config.name.as_str())).join(",")
    )
    .unwrap();
    let group_params = state
        .groups
        .values()
        .flat_map(|config| {
            config
                .params
                .iter()
                .map(move |(name, value)| format!("{}.{name}={value}", config.name))
        })
        .collect();
    writeln!(out, "group_params={}", join_names(group_params)).unwrap();
    writeln!(
        out,
        "body_contributions={}",
        state
            .body_contributions
            .iter()
            .map(|item| format!("{}:{}", item.ordinal, item.target_path))
            .collect::<Vec<_>>()
            .join(",")
    )
    .unwrap();

    writeln!(
        out,
        "voice_order={}",
        state
            .voice_order
            .iter()
            .map(|id| voice_name(state, *id))
            .collect::<Vec<_>>()
            .join(",")
    )
    .unwrap();
    writeln!(
        out,
        "voices={}",
        join_names(
            state
                .voices
                .values()
                .map(|config| format!("{}:{}", config.name, config.synthdef))
                .collect()
        )
    )
    .unwrap();
    writeln!(
        out,
        "running_voices={}",
        join_names(
            state
                .running_voices
                .iter()
                .map(|id| voice_name(state, *id))
                .collect()
        )
    )
    .unwrap();

    writeln!(
        out,
        "patterns={}",
        join_names(
            state
                .patterns
                .values()
                .map(|config| config.name.clone())
                .collect()
        )
    )
    .unwrap();
    writeln!(
        out,
        "playing_patterns={}",
        join_names(
            state
                .playing_patterns
                .iter()
                .filter_map(|id| state.patterns.get(id).map(|config| config.name.clone()))
                .collect()
        )
    )
    .unwrap();
    writeln!(
        out,
        "melodies={}",
        join_names(
            state
                .melodies
                .values()
                .map(|config| config.name.clone())
                .collect()
        )
    )
    .unwrap();
    writeln!(
        out,
        "playing_melodies={}",
        join_names(
            state
                .playing_melodies
                .iter()
                .filter_map(|id| state.melodies.get(id).map(|config| config.name.clone()))
                .collect()
        )
    )
    .unwrap();
    writeln!(
        out,
        "sequences={}",
        join_names(
            state
                .sequences
                .values()
                .map(|config| format!("{}:{}clips", config.name, config.clips.len()))
                .collect()
        )
    )
    .unwrap();
    writeln!(
        out,
        "playing_sequences={}",
        join_names(
            state
                .playing_sequences
                .iter()
                .filter_map(|id| state.sequences.get(id).map(|config| config.name.clone()))
                .collect()
        )
    )
    .unwrap();

    writeln!(out, "fades={}", state.fades.len()).unwrap();
    writeln!(out, "playing_fades={}", state.playing_fades.len()).unwrap();
    writeln!(
        out,
        "force_restart_fades={}",
        state.force_restart_fades.len()
    )
    .unwrap();
    writeln!(out, "pending_fades={}", state.pending_fades.len()).unwrap();
    writeln!(
        out,
        "pending_fades_quantized={}",
        state.pending_fades_quantized.len()
    )
    .unwrap();

    let effects_by_id: HashMap<_, _> = state
        .effects
        .iter()
        .map(|(id, config)| (*id, config.synthdef.as_str()))
        .collect();
    writeln!(
        out,
        "effect_order={}",
        state
            .effect_order
            .iter()
            .map(|id| effects_by_id.get(id).copied().unwrap_or("<missing>"))
            .collect::<Vec<_>>()
            .join(",")
    )
    .unwrap();

    let mut routes = Vec::new();
    for (voice_id, port) in &state.route_order {
        let destinations =
            state
                .routes
                .get(&(*voice_id, port.clone()))
                .map_or_else(Vec::new, |values| {
                    values
                        .iter()
                        .map(|dest| match dest {
                            RouteDest::Group(group_id) => {
                                format!("group:{}", group_name(state, *group_id))
                            }
                            RouteDest::Main => "main".to_string(),
                            RouteDest::Muted => "muted".to_string(),
                            RouteDest::Param {
                                voice_id,
                                param_name,
                            } => {
                                format!("param:{}:{param_name}", voice_name(state, *voice_id))
                            }
                        })
                        .collect()
                });
        routes.push(format!(
            "{}.{port}->{}",
            voice_name(state, *voice_id),
            destinations.join("+")
        ));
    }
    writeln!(out, "routes={}", routes.join(",")).unwrap();
    writeln!(out, "input_routes={}", state.input_routes.len()).unwrap();
    writeln!(out, "param_routes_set={}", state.param_routes_set.len()).unwrap();
    writeln!(out, "param_routes_bend={}", state.param_routes_bend.len()).unwrap();
    writeln!(
        out,
        "param_routes_trigger={}",
        state.param_routes_trigger.len()
    )
    .unwrap();

    let samples = state
        .samples
        .iter()
        .map(|(id, config)| format!("{}:{}", id.raw(), config.path.display()))
        .collect();
    writeln!(out, "samples={}", join_names(samples)).unwrap();
    let buffers = state
        .buffers
        .iter()
        .map(|(id, config)| {
            format!(
                "{}:{}:{}x{}",
                config.name,
                id.raw(),
                config.frames,
                config.channels
            )
        })
        .collect();
    writeln!(out, "buffers={}", join_names(buffers)).unwrap();
    let sfz = state
        .sfz_instruments
        .iter()
        .map(|(id, config)| format!("{}:{}", id.raw(), config.path.display()))
        .collect();
    writeln!(out, "sfz={}", join_names(sfz)).unwrap();
    let recordings = state
        .recordings
        .iter()
        .map(|(id, config)| {
            format!(
                "{}:start={}:beats={}",
                id.raw(),
                config
                    .start_beat
                    .map(|beat| beat.to_f64().to_string())
                    .unwrap_or_else(|| "unset".to_string()),
                config
                    .length_beats
                    .map(|beats| beats.to_string())
                    .unwrap_or_else(|| "unset".to_string())
            )
        })
        .collect();
    writeln!(out, "recordings={}", join_names(recordings)).unwrap();

    let mut hashes: Vec<_> = state
        .synthdef_hashes
        .iter()
        .map(|(name, hash)| format!("{name}:{hash:016x}"))
        .collect();
    hashes.sort();
    writeln!(out, "synthdef_hashes={}", hashes.join(",")).unwrap();

    writeln!(
        out,
        "midi_keyboard_routes={}",
        state.midi_keyboard_routes.len()
    )
    .unwrap();
    writeln!(
        out,
        "advanced_keyboard_routes={}",
        state.advanced_keyboard_routes.len()
    )
    .unwrap();
    writeln!(out, "midi_inputs={}", state.midi_inputs.len()).unwrap();
    writeln!(out, "midi_outputs={}", state.midi_outputs.len()).unwrap();
    writeln!(
        out,
        "midi_output_messages={}",
        state.midi_output_messages.len()
    )
    .unwrap();
    writeln!(out, "midi_callbacks={}", state.midi_callbacks.len()).unwrap();
    writeln!(
        out,
        "midi_recording_requests={}",
        state.midi_recording_requests.len()
    )
    .unwrap();
    writeln!(out, "loopers={}", state.loopers.len()).unwrap();
    writeln!(out, "midi_clock_outputs={}", state.midi_clock_outputs.len()).unwrap();
    writeln!(
        out,
        "midi2_keyboard_routes={}",
        state.midi2_keyboard_routes.len()
    )
    .unwrap();
    writeln!(
        out,
        "midi2_per_note_routes={}",
        state.midi2_per_note_routes.len()
    )
    .unwrap();
    writeln!(out, "midi2_cc_routes={}", state.midi2_cc_routes.len()).unwrap();
    out
}

#[test]
fn v1_authoring_family_state_matches_golden_snapshots() {
    vibelang_dsp::set_deploy_callback(|_| Ok(()));
    let update = std::env::var_os("VIBELANG_UPDATE_V1_GOLDENS").is_some();
    let dir = fixture_dir();
    let mut engine = ScriptEngine::new();

    for name in GOLDENS {
        let script_path = dir.join(format!("{name}.vibe"));
        let snapshot_path = dir.join(format!("{name}.snapshot"));
        let script = fs::read_to_string(&script_path).unwrap();
        let state = engine
            .execute(&script)
            .unwrap_or_else(|error| panic!("{} failed: {error}", script_path.display()));
        let actual = summarize(&state);
        if update {
            fs::write(&snapshot_path, &actual).unwrap();
            println!("updated {}", snapshot_path.display());
        } else {
            let expected = fs::read_to_string(&snapshot_path).unwrap();
            assert_eq!(actual, expected, "{} drifted", script_path.display());
        }
    }
}
