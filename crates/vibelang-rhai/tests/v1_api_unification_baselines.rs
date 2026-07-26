use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, Once, OnceLock};
use vibelang_core::handlers::RouteDest;
use vibelang_core::reload::ScriptState;
use vibelang_core::traits::{Clip, FadeConfig, FadeCurve, FadeTarget};
use vibelang_core::types::{Duration, GroupId, MidiDeviceId, ParamMap, VoiceId};
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
    "09_parser_compat",
];

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/api-unification/v1/golden")
}

fn updating_goldens() -> bool {
    std::env::var_os("VIBELANG_UPDATE_V1_GOLDENS").is_some()
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

fn format_params(params: &ParamMap) -> String {
    let mut values: Vec<_> = params
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    values.sort();
    values.join("+")
}

fn pattern_details(state: &ScriptState) -> String {
    join_names(
        state
            .patterns
            .values()
            .map(|config| {
                let voice = config
                    .voice
                    .map(|id| voice_name(state, id))
                    .unwrap_or_else(|| "unset".to_string());
                let steps = config
                    .steps
                    .iter()
                    .map(|step| format!("{}{{{}}}", step.beat, format_params(&step.params)))
                    .collect::<Vec<_>>()
                    .join(">");
                format!(
                    "{}:voice={voice}:length={}:swing={}:steps=[{steps}]",
                    config.name, config.length, config.swing
                )
            })
            .collect(),
    )
}

fn sequence_clip(state: &ScriptState, clip: &Clip) -> String {
    match clip {
        Clip::Pattern { id, start, end } => {
            let name = state
                .patterns
                .get(id)
                .map(|config| config.name.clone())
                .unwrap_or_else(|| format!("pattern#{}", id.raw()));
            format!("pattern:{name}@{start}..{end}")
        }
        Clip::Melody { id, start, end } => {
            let name = state
                .melodies
                .get(id)
                .map(|config| config.name.clone())
                .unwrap_or_else(|| format!("melody#{}", id.raw()));
            format!("melody:{name}@{start}..{end}")
        }
        Clip::Fade { config, start } => {
            format!("fade:{}@{start}", format_fade_config(state, config))
        }
        Clip::Sequence { id, start } => {
            let name = state
                .sequences
                .get(id)
                .map(|config| config.name.clone())
                .unwrap_or_else(|| format!("sequence#{}", id.raw()));
            format!("sequence:{name}@{start}")
        }
    }
}

fn sequence_details(state: &ScriptState) -> String {
    join_names(
        state
            .sequences
            .values()
            .map(|config| {
                let clips = config
                    .clips
                    .iter()
                    .map(|clip| sequence_clip(state, clip))
                    .collect::<Vec<_>>()
                    .join(">");
                format!("{}:length={}:clips=[{clips}]", config.name, config.length)
            })
            .collect(),
    )
}

fn fade_target(state: &ScriptState, target: &FadeTarget) -> String {
    match target {
        FadeTarget::Group(id) => format!("group:{}", group_name(state, *id)),
        FadeTarget::Voice(id) => format!("voice:{}", voice_name(state, *id)),
        FadeTarget::Pattern(id) => state.patterns.get(id).map_or_else(
            || format!("pattern#{}", id.raw()),
            |config| format!("pattern:{}", config.name),
        ),
        FadeTarget::Melody(id) => state.melodies.get(id).map_or_else(
            || format!("melody#{}", id.raw()),
            |config| format!("melody:{}", config.name),
        ),
        FadeTarget::Effect(id) => state.effects.get(id).map_or_else(
            || format!("effect#{}", id.raw()),
            |config| format!("effect:{}", config.synthdef),
        ),
    }
}

fn format_fade_config(state: &ScriptState, config: &FadeConfig) -> String {
    format!(
        "target={}:param={}:from={}:to={}:duration={}:curve={:?}",
        fade_target(state, &config.target),
        config.param,
        config
            .from
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unset".to_string()),
        config.to,
        config.duration,
        config.curve
    )
}

fn summarize(state: &ScriptState, diagnostics: &[String]) -> String {
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
    writeln!(out, "pattern_details={}", pattern_details(state)).unwrap();
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
    writeln!(out, "sequence_details={}", sequence_details(state)).unwrap();

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
    let mut fade_configs: Vec<_> = state
        .fades
        .iter()
        .map(|(id, config)| {
            format!(
                "{}:{}:playing={}:force_restart={}:timing=stateful",
                id.raw(),
                format_fade_config(state, config),
                state.playing_fades.contains(id),
                state.force_restart_fades.contains(id)
            )
        })
        .collect();
    fade_configs.sort();
    writeln!(out, "fade_configs={}", fade_configs.join(",")).unwrap();
    writeln!(
        out,
        "pending_fade_configs={}",
        state
            .pending_fades
            .iter()
            .map(|config| format!("{}:timing=immediate", format_fade_config(state, config)))
            .collect::<Vec<_>>()
            .join(",")
    )
    .unwrap();
    writeln!(
        out,
        "pending_quantized_fade_configs={}",
        state
            .pending_fades_quantized
            .iter()
            .map(|config| format!("{}:timing=quantized", format_fade_config(state, config)))
            .collect::<Vec<_>>()
            .join(",")
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
    let group_effect_order = state
        .groups
        .values()
        .filter(|group| !group.effects.is_empty())
        .map(|group| {
            let effects = group
                .effects
                .iter()
                .map(|id| effects_by_id.get(id).copied().unwrap_or("<missing>"))
                .collect::<Vec<_>>()
                .join(">");
            format!("{}:{effects}", group.name)
        })
        .collect();
    writeln!(out, "group_effect_order={}", join_names(group_effect_order)).unwrap();

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
    let sample_configs = state
        .samples
        .iter()
        .map(|(id, config)| {
            format!(
                "{}:amp={}:rate={}:trigger={}",
                id.raw(),
                config.amp,
                config.rate,
                config.trigger_mode
            )
        })
        .collect();
    writeln!(out, "sample_configs={}", join_names(sample_configs)).unwrap();
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
    writeln!(
        out,
        "advanced_keyboard_route_configs={}",
        state
            .advanced_keyboard_routes
            .iter()
            .map(|route| format!(
                "device={}:channel={}:notes={}..{}:transpose={}:velocity={}:voice={}",
                route.device_id.raw(),
                route
                    .channel
                    .map(|channel| channel.to_string())
                    .unwrap_or_else(|| "all".to_string()),
                route.note_min,
                route.note_max,
                route.transpose,
                route.velocity_curve,
                voice_name(state, route.voice)
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
    .unwrap();
    writeln!(out, "midi_inputs={}", state.midi_inputs.len()).unwrap();
    let mut midi_input_ids: Vec<_> = state.midi_inputs.iter().map(|id| id.raw()).collect();
    midi_input_ids.sort();
    writeln!(
        out,
        "midi_input_ids={}",
        midi_input_ids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
    .unwrap();
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
    writeln!(
        out,
        "looper_configs={}",
        state
            .loopers
            .iter()
            .map(|looper| format!(
                "device={}:voice={}:channel={}:silence={}:quantize={}",
                looper.device_id.raw(),
                voice_name(state, looper.voice_id),
                looper
                    .channel
                    .map(|channel| channel.to_string())
                    .unwrap_or_else(|| "all".to_string()),
                looper.silence_bars,
                looper.quantize_beats
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
    .unwrap();
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
    writeln!(
        out,
        "diagnostics={}",
        if diagnostics.is_empty() {
            "none".to_string()
        } else {
            diagnostics.join(" | ")
        }
    )
    .unwrap();
    out
}

fn diagnostic_lines() -> &'static Mutex<Vec<String>> {
    static LINES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    LINES.get_or_init(|| Mutex::new(Vec::new()))
}

fn execution_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn push_diagnostic(level: &str, target: &str, message: String) {
    if target.starts_with("vibelang_rhai") {
        diagnostic_lines()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(format!("{level} {target}: {message}"));
    }
}

struct DiagnosticLogger;

impl log::Log for DiagnosticLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn && metadata.target().starts_with("vibelang_rhai")
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            push_diagnostic(
                record.level().as_str(),
                record.target(),
                record.args().to_string(),
            );
        }
    }

    fn flush(&self) {}
}

struct DiagnosticSubscriber;

impl tracing::Subscriber for DiagnosticSubscriber {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() <= tracing::Level::WARN && metadata.target().starts_with("vibelang_rhai")
    }

    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        struct MessageVisitor(String);

        impl tracing::field::Visit for MessageVisitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    let _ = write!(self.0, "{value:?}");
                }
            }

            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "message" {
                    self.0.push_str(value);
                }
            }
        }

        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        push_diagnostic(
            event.metadata().level().as_str(),
            event.metadata().target(),
            visitor.0,
        );
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

fn execute_with_diagnostics(
    engine: &mut ScriptEngine,
    script: &str,
) -> (vibelang_rhai::Result<ScriptState>, Vec<String>) {
    static LOGGER: DiagnosticLogger = DiagnosticLogger;
    static INSTALL_LOGGER: Once = Once::new();
    let _execution = execution_lock();
    INSTALL_LOGGER.call_once(|| {
        log::set_logger(&LOGGER).expect("diagnostic logger should install once per test binary");
        log::set_max_level(log::LevelFilter::Warn);
    });
    diagnostic_lines()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    let result = tracing::subscriber::with_default(DiagnosticSubscriber, || engine.execute(script));
    let diagnostics = diagnostic_lines()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .drain(..)
        .collect();
    (result, diagnostics)
}

fn execute_golden(name: &str) -> (ScriptState, Vec<String>, String) {
    vibelang_dsp::set_deploy_callback(|_| Ok(()));
    let script_path = fixture_dir().join(format!("{name}.vibe"));
    let script = fs::read_to_string(&script_path).unwrap();
    let mut engine = ScriptEngine::new();
    let (state, diagnostics) = execute_with_diagnostics(&mut engine, &script);
    let state = state.unwrap_or_else(|error| panic!("{} failed: {error}", script_path.display()));
    let expected = fs::read_to_string(fixture_dir().join(format!("{name}.snapshot"))).unwrap();
    (state, diagnostics, expected)
}

fn assert_mutant_rejected(label: &str, expected: &str, actual: String) {
    assert_ne!(actual, expected, "{label} mutant escaped the golden gate");
}

#[cfg(all(feature = "ext-exec", feature = "ext-fs", feature = "ext-net"))]
#[test]
fn v1_extension_recoveries_emit_one_stable_diagnostic_each() {
    let mut engine = ScriptEngine::new();
    engine.register_all_extensions();
    let script = r#"
        let missing = env_var("VIBELANG_TEST_ENVIRONMENT_VALUE_MUST_NOT_EXIST");
        let empty_exec = exec("");
        let missing_extension = path_extension("README");
        let preserved_escape = url_decode("%G0");
        if missing != "" || empty_exec != "" || missing_extension != "" || preserved_escape != "%G0" {
            throw "v1 extension recovery drift";
        }
    "#;
    let (state, diagnostics) = execute_with_diagnostics(&mut engine, script);
    state.expect("v1 extension recoveries remain callable");
    assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
    for diagnostic in [
        "function=env_var",
        "function=exec",
        "function=path_extension",
        "parser=url_decode",
    ] {
        assert_eq!(
            diagnostics
                .iter()
                .filter(|line| line.contains(diagnostic))
                .count(),
            1,
            "{diagnostic}: {diagnostics:?}"
        );
    }
    assert!(diagnostics
        .iter()
        .all(|line| line.contains("diagnostic.compat.")));
}

#[test]
fn v1_authoring_family_state_matches_golden_snapshots() {
    vibelang_dsp::set_deploy_callback(|_| Ok(()));
    let update = updating_goldens();
    let dir = fixture_dir();
    let mut engine = ScriptEngine::new();

    for name in GOLDENS {
        let script_path = dir.join(format!("{name}.vibe"));
        let snapshot_path = dir.join(format!("{name}.snapshot"));
        let script = fs::read_to_string(&script_path).unwrap();
        let (state, diagnostics) = execute_with_diagnostics(&mut engine, &script);
        let state =
            state.unwrap_or_else(|error| panic!("{} failed: {error}", script_path.display()));
        let actual = summarize(&state, &diagnostics);
        if update {
            fs::write(&snapshot_path, &actual).unwrap();
            println!("updated {}", snapshot_path.display());
        } else {
            let expected = fs::read_to_string(&snapshot_path).unwrap();
            assert_eq!(actual, expected, "{} drifted", script_path.display());
        }
    }
}

#[test]
fn v1_warning_omission_and_drift_mutants_are_rejected() {
    if updating_goldens() {
        return;
    }
    let (state, diagnostics, expected) = execute_golden("02_patterns_melodies");
    assert!(
        diagnostics.iter().any(|line| line
            .contains("Pattern 'pattern_dormant': voice 'missing_pattern_voice' not found")),
        "expected pattern warning, got {diagnostics:?}"
    );
    assert_eq!(summarize(&state, &diagnostics), expected);
    assert_mutant_rejected("warning omission", &expected, summarize(&state, &[]));
    let mut altered = diagnostics.clone();
    altered[0].push_str(" [altered]");
    assert_mutant_rejected("warning text drift", &expected, summarize(&state, &altered));
}

#[test]
fn v1_error_omission_and_drift_mutants_are_rejected() {
    if updating_goldens() {
        return;
    }
    let (state, diagnostics, expected) = execute_golden("01_groups_voices");
    let error = "ERROR vibelang_rhai::api::group: group('Drums').output(-1): bus must be in 0..16. Supported forms: group.output(N) for mono, group.output([N]) for mono, group.output([L, R]) for stereo";
    assert_eq!(diagnostics, [error]);
    assert_eq!(summarize(&state, &diagnostics), expected);
    assert_mutant_rejected("ERROR omission", &expected, summarize(&state, &[]));
    let altered = [format!("{error} [altered]")];
    assert_mutant_rejected("ERROR text drift", &expected, summarize(&state, &altered));
}

#[test]
fn v1_melody_launch_only_drift_mutant_is_rejected() {
    if updating_goldens() {
        return;
    }
    let (state, diagnostics, expected) = execute_golden("02_patterns_melodies");
    assert_eq!(summarize(&state, &diagnostics), expected);
    let launched_id = *state
        .melodies
        .iter()
        .find(|(_, config)| config.name == "melody_launched")
        .map(|(id, _)| id)
        .unwrap();
    let started_id = *state
        .melodies
        .iter()
        .find(|(_, config)| config.name == "melody_playing")
        .map(|(id, _)| id)
        .unwrap();
    assert!(state.playing_melodies.contains(&launched_id));
    assert!(state.playing_melodies.contains(&started_id));

    let mut launch_only_drift = state.clone();
    launch_only_drift.playing_melodies.remove(&launched_id);
    assert!(launch_only_drift.playing_melodies.contains(&started_id));
    assert_mutant_rejected(
        "Melody.launch-only playing-state drift",
        &expected,
        summarize(&launch_only_drift, &diagnostics),
    );
}

#[test]
fn v1_pattern_param_and_sequence_order_mutants_are_rejected() {
    if updating_goldens() {
        return;
    }
    let (state, diagnostics, expected) = execute_golden("02_patterns_melodies");
    assert_eq!(summarize(&state, &diagnostics), expected);
    let mut pattern_mutant = state.clone();
    let pattern = pattern_mutant
        .patterns
        .values_mut()
        .find(|config| config.name == "pattern_dormant")
        .unwrap();
    pattern.steps[0]
        .params
        .insert("accepted_but_dropped".to_string(), 9.0);
    assert_mutant_rejected(
        "Pattern.set_param becoming effectful",
        &expected,
        summarize(&pattern_mutant, &diagnostics),
    );

    let (state, diagnostics, expected) = execute_golden("03_sequences_fades_effects");
    assert_eq!(summarize(&state, &diagnostics), expected);
    let mut sequence_mutant = state.clone();
    sequence_mutant
        .sequences
        .values_mut()
        .find(|config| config.name == "arrangement")
        .unwrap()
        .clips
        .swap(0, 1);
    assert_mutant_rejected(
        "sequence clip order",
        &expected,
        summarize(&sequence_mutant, &diagnostics),
    );
}

#[test]
fn v1_fade_timing_and_configuration_mutants_are_rejected() {
    if updating_goldens() {
        return;
    }
    let (state, diagnostics, expected) = execute_golden("03_sequences_fades_effects");
    assert_eq!(summarize(&state, &diagnostics), expected);

    let started_id = *state
        .fades
        .iter()
        .find(|(_, config)| (config.to - 0.75).abs() < f32::EPSILON)
        .map(|(id, _)| id)
        .unwrap();
    let now_id = *state
        .fades
        .iter()
        .find(|(_, config)| (config.to - 1.0).abs() < f32::EPSILON)
        .map(|(id, _)| id)
        .unwrap();

    let mut quantized_timing = state.clone();
    let config = quantized_timing.fades.remove(&started_id).unwrap();
    quantized_timing.playing_fades.remove(&started_id);
    quantized_timing.pending_fades_quantized.push(config);
    assert_mutant_rejected(
        "Fade.start quantized timing",
        &expected,
        summarize(&quantized_timing, &diagnostics),
    );

    let mut immediate_timing = state.clone();
    let config = immediate_timing.fades.remove(&now_id).unwrap();
    immediate_timing.playing_fades.remove(&now_id);
    immediate_timing.pending_fades.push(config);
    assert_mutant_rejected(
        "Fade.now immediate timing",
        &expected,
        summarize(&immediate_timing, &diagnostics),
    );

    let mut config_mutants: Vec<(&str, ScriptState)> = Vec::new();
    let mut target = state.clone();
    target.fades.get_mut(&started_id).unwrap().target = FadeTarget::Group(GroupId::new(7));
    config_mutants.push(("Fade target", target));
    let mut param = state.clone();
    param.fades.get_mut(&started_id).unwrap().param = "cutoff".to_string();
    config_mutants.push(("Fade param", param));
    let mut from = state.clone();
    from.fades.get_mut(&started_id).unwrap().from = Some(0.333);
    config_mutants.push(("Fade from", from));
    let mut to = state.clone();
    to.fades.get_mut(&started_id).unwrap().to = 0.875;
    config_mutants.push(("Fade to", to));
    let mut duration = state.clone();
    duration.fades.get_mut(&started_id).unwrap().duration = Duration::from_beats(9.0);
    config_mutants.push(("Fade duration", duration));
    let mut curve = state.clone();
    curve.fades.get_mut(&started_id).unwrap().curve = FadeCurve::EaseOut;
    config_mutants.push(("Fade curve", curve));
    for (label, mutant) in config_mutants {
        assert_mutant_rejected(label, &expected, summarize(&mutant, &diagnostics));
    }
}

#[test]
fn v1_sample_configuration_mutants_are_rejected() {
    if updating_goldens() {
        return;
    }
    let (state, diagnostics, expected) = execute_golden("05_resources_recording");
    assert_eq!(summarize(&state, &diagnostics), expected);
    let sample_id = *state.samples.keys().next().unwrap();
    let mut amp = state.clone();
    amp.samples.get_mut(&sample_id).unwrap().amp = 1.0;
    assert_mutant_rejected("Sample amp", &expected, summarize(&amp, &diagnostics));
    let mut rate = state.clone();
    rate.samples.get_mut(&sample_id).unwrap().rate = 1.0;
    assert_mutant_rejected("Sample rate", &expected, summarize(&rate, &diagnostics));
    let mut trigger = state.clone();
    trigger.samples.get_mut(&sample_id).unwrap().trigger_mode = "gate".to_string();
    assert_mutant_rejected(
        "Sample one_shot",
        &expected,
        summarize(&trigger, &diagnostics),
    );
}

#[test]
fn v1_effect_order_and_midi_sentinel_mutants_are_rejected() {
    if updating_goldens() {
        return;
    }
    let (state, diagnostics, expected) = execute_golden("03_sequences_fades_effects");
    assert_eq!(summarize(&state, &diagnostics), expected);
    let mut effect_order = state.clone();
    effect_order
        .groups
        .values_mut()
        .find(|group| group.name == "Arrangement")
        .unwrap()
        .effects
        .swap(0, 1);
    assert_mutant_rejected(
        "group effect order",
        &expected,
        summarize(&effect_order, &diagnostics),
    );

    let (state, diagnostics, expected) = execute_golden("07_midi");
    assert!(
        diagnostics
            .iter()
            .any(|line| line.contains("device not found, MIDI operations will be no-ops")),
        "expected MIDI diagnostic, got {diagnostics:?}"
    );
    assert_eq!(summarize(&state, &diagnostics), expected);
    let mut sentinel = state.clone();
    sentinel.advanced_keyboard_routes[0].device_id = MidiDeviceId::new(0);
    sentinel.loopers[0].device_id = MidiDeviceId::new(0);
    sentinel.midi_inputs.remove(&MidiDeviceId::new(u32::MAX));
    sentinel.midi_inputs.insert(MidiDeviceId::new(0));
    assert_mutant_rejected(
        "MIDI unknown-device sentinel",
        &expected,
        summarize(&sentinel, &diagnostics),
    );
}
