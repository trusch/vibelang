//! Walk every `ugen_manifests/*.json`, sample one UGen per category, invoke its
//! codegenerated function (e.g. `sin_osc_ar`, `lpf_ar`, `disk_in_ar`) via the
//! Rhai engine, encode the resulting graph through `encoder.rs`, and verify the
//! bytes round-trip through a minimal scsyndef header parser without panicking.
//!
//! The codegenerated UGen functions are not re-exported as Rust items, so the
//! test drives them through `register_dsp_api`, which is the same path real
//! VibeLang scripts take. A failure here means a manifest entry produced
//! invalid scsyndef bytes — caught at `cargo test` instead of at scsynth load.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rhai::{Dynamic, Engine};
use serde::Deserialize;
use vibelang_dsp::{
    clear_active_builder, encode_synthdef, register_dsp_api, set_active_builder, GraphBuilderInner,
    GraphIR, Input,
};

const MANIFESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ugen_manifests");

#[derive(Debug, Deserialize)]
struct UGenManifest {
    name: String,
    rates: Vec<String>,
    category: String,
    #[serde(default)]
    ugen_class: Option<String>,
    #[serde(default)]
    pseudo: bool,
    #[serde(default)]
    requires_plugin: Option<String>,
    #[serde(default)]
    unavailable_reason: Option<String>,
}

const CONFIRMED_PSEUDO_UGENS: &[&str] = &[
    "AMClip",
    "AbsDif",
    "Atan2",
    "BLowPass4",
    "BHiPass4",
    "Changed",
    "Clip2",
    "DifSqr",
    "Excess",
    "ExpExp",
    "ExpLin",
    "FFTCentroid",
    "FirstArg",
    "Fold2",
    "Greyhole",
    "Hypot",
    "HypotApx",
    "JPverb",
    "LinLin",
    "Mix",
    "OnsetsDS",
    "PMOsc",
    "PV_DiffMags",
    "PanX2D",
    "PulseDPW",
    "Ring1",
    "Ring2",
    "Ring3",
    "Ring4",
    "Rotate",
    "ScaleNeg",
    "SelectX",
    "Silence",
    "SoundIn",
    "Splay",
    "SplayAz",
    "SqrDif",
    "SqrSum",
    "SumSqr",
    "TWChoose",
    "Thresh",
    "Tilt",
    "Tumble",
    "Wrap2",
];

const STALE_NON_BINARY_UGENS: &[&str] = &[
    "AtsFile",
    "BigArity24",
    "CQ_Diff",
    "FFTSubbandFlux",
    "FaustGreyholeRaw",
    "HOAEncLebedev061",
    "HOALibEnc3D1",
    "HOALibEnc3D2",
    "HOALibEnc3D3",
    "HOALibEnc3D4",
    "HOALibEnc3D5",
    "HOAmbiPanner1",
    "HOAmbiPanner2",
    "HOAmbiPanner3",
    "HOAmbiPanner4",
    "HOAmbiPanner5",
    "ITU5001",
    "ITU5002",
    "LinkJump",
    "LinkPhase",
    "LinkTempo",
    "MIDelay",
    "MiBraids",
    "MiClouds",
    "MiElements",
    "MiGrids",
    "MiMu",
    "MiOmi",
    "MiPlaits",
    "MiRings",
    "MiRipples",
    "MiTides",
    "MiVerb",
    "MiWarps",
    "RMAFoodChainL",
    "RosslerResL",
    "SimpleLoopBuf",
    "VBAPSpeaker",
    "VBAPSpeakerArray",
    "envelope",
];

const MANUAL_PSEUDO_LOWERINGS: &[&str] = &[
    "Changed", "Greyhole", "JPverb", "LinLin", "Mix", "Splay", "SplayAz",
];

fn load_manifests() -> Vec<UGenManifest> {
    let mut paths: Vec<PathBuf> = fs::read_dir(Path::new(MANIFESTS_DIR))
        .expect("read ugen_manifests dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    paths.sort();

    let mut all = Vec::new();
    for path in paths {
        let body =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
        let parsed: Vec<UGenManifest> = serde_json::from_str(&body)
            .unwrap_or_else(|e| panic!("parse {}: {}", path.display(), e));
        all.extend(parsed);
    }
    all
}

/// Mirror of `build.rs::to_snake_case`. Kept in sync by the round-trip test
/// itself: if either drifts, function-name lookup via Rhai will fail.
fn to_snake_case(s: &str) -> String {
    if s == "DC" {
        return "dc".to_string();
    }
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                let prev = chars[i - 1];
                let prev_lower = prev.is_lowercase();
                let next_lower = chars.get(i + 1).map(|c| c.is_lowercase()).unwrap_or(false);
                if (prev_lower || next_lower) && prev != '_' {
                    out.push('_');
                }
            }
            out.push(c.to_lowercase().next().unwrap());
        } else {
            out.push(c);
        }
    }
    out
}

fn pick_one_per_category(manifests: &[UGenManifest]) -> Vec<&UGenManifest> {
    let mut by_cat: BTreeMap<&str, &UGenManifest> = BTreeMap::new();
    for m in manifests {
        // Documentation-only pseudo-entries (rate == "builder") have no
        // codegenerated function and are skipped by build.rs too.
        if m.rates.iter().all(|r| r == "builder") {
            continue;
        }
        by_cat.entry(m.category.as_str()).or_insert(m);
    }
    by_cat.into_values().collect()
}

fn first_concrete_rate(ugen: &UGenManifest) -> Option<&str> {
    ugen.rates
        .iter()
        .map(String::as_str)
        .find(|r| *r != "builder")
}

fn expects_literal_ugen_name(ugen: &UGenManifest) -> bool {
    !ugen.pseudo && ugen.ugen_class.is_none()
}

fn pseudo_name_can_appear_inside_raw_name(name: &str) -> bool {
    matches!(name, "Greyhole" | "JPverb")
}

/// Validate header magic and version. The full scsyndef format is large
/// enough that re-implementing a parser here would be its own project; the
/// goal is to exercise the encoder path and confirm the bytes are at least
/// well-formed at the envelope.
fn parse_scsyndef_header(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 8 {
        return Err(format!("buffer too short: {} bytes", bytes.len()));
    }
    if &bytes[0..4] != b"SCgf" {
        return Err(format!("bad magic: {:?}", &bytes[0..4]));
    }
    let version = i32::from_be_bytes(bytes[4..8].try_into().unwrap());
    if version != 2 {
        return Err(format!("unexpected version: {}", version));
    }
    Ok(())
}

#[test]
fn channel_count_shape_args_are_metadata_not_server_inputs() {
    let mut engine = Engine::new();
    register_dsp_api(&mut engine);

    let cases = [
        ("play_buf_ar(2, 0, 1, 1, 0, 0, 0);", "PlayBuf", 6usize),
        ("buf_rd_ar(2, 0, 0, 1, 4);", "BufRd", 4usize),
        (
            "grain_buf_ar(2, 1, 0.1, 0, 1, 0, 4, 0, -1, 64);",
            "GrainBuf",
            9usize,
        ),
        (
            "t_grains_ar(2, 1, 0, 1, 0, 0.1, 0, 0.5, 4);",
            "TGrains",
            8usize,
        ),
    ];

    for (script, ugen_name, expected_inputs) in cases {
        set_active_builder(GraphBuilderInner::new());
        let _ = engine
            .eval::<Dynamic>(script)
            .unwrap_or_else(|e| panic!("{script} failed: {e}"));
        let builder = clear_active_builder().expect("active builder vanished");
        let node = builder
            .nodes
            .iter()
            .find(|node| node.name == ugen_name)
            .unwrap_or_else(|| panic!("{ugen_name} node missing after {script}"));

        assert_eq!(node.inputs.len(), expected_inputs, "{ugen_name} inputs");
        assert_eq!(node.num_outputs, 2, "{ugen_name} output count");
        assert_eq!(node.special_index, 2, "{ugen_name} special index");
        assert!(
            !matches!(node.inputs.first(), Some(Input::Constant(2.0))),
            "{ugen_name} encoded numChannels as the first runtime input"
        );
    }
}

#[test]
fn local_out_ar_accepts_stereo_signal_array() {
    let mut engine = Engine::new();
    register_dsp_api(&mut engine);

    set_active_builder(GraphBuilderInner::new());
    let script = "local_out_ar([sin_osc_ar(440), sin_osc_ar(442)]);";
    let _ = engine
        .eval::<Dynamic>(script)
        .unwrap_or_else(|e| panic!("{script} failed: {e}"));
    let builder = clear_active_builder().expect("active builder vanished");
    let node = builder
        .nodes
        .iter()
        .find(|node| node.name == "LocalOut")
        .expect("LocalOut node missing");

    assert_eq!(
        node.inputs.len(),
        2,
        "LocalOut should receive both channels"
    );
}

#[test]
fn ugen_manifest_round_trip_one_per_category() {
    let manifests = load_manifests();
    assert!(!manifests.is_empty(), "no UGen manifests found");

    let picks = pick_one_per_category(&manifests);
    assert!(
        picks.len() >= 10,
        "expected many categories, got only {}",
        picks.len()
    );

    let mut engine = Engine::new();
    register_dsp_api(&mut engine);

    let mut tested = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for ugen in &picks {
        let Some(rate) = first_concrete_rate(ugen) else {
            continue;
        };
        let func_name = format!("{}_{}", to_snake_case(&ugen.name), rate);

        // The codegenerated function reads the thread-local active builder
        // and pushes its node into it; arity-0 invocation uses manifest
        // defaults for every parameter.
        set_active_builder(GraphBuilderInner::new());
        let script = format!("{}();", func_name);
        let eval_result: Result<Dynamic, _> = engine.eval(&script);
        let builder = clear_active_builder().expect("active builder vanished");

        if let Err(e) = eval_result {
            failures.push(format!(
                "{} (category={}): rhai eval failed: {}",
                func_name, ugen.category, e
            ));
            continue;
        }

        let ir = GraphIR::from_builder(format!("rt_{}", func_name), builder);
        let bytes = match encode_synthdef(&ir) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!(
                    "{} (category={}): encode failed: {}",
                    func_name, ugen.category, e
                ));
                continue;
            }
        };

        if let Err(e) = parse_scsyndef_header(&bytes) {
            failures.push(format!(
                "{} (category={}): header parse failed: {}",
                func_name, ugen.category, e
            ));
            continue;
        }

        // Real server UGens appear verbatim in the encoded bytes. Pseudo-UGens
        // are intentionally lowered to supported graph fragments instead.
        let needle = ugen.name.as_bytes();
        let contains_name = bytes.windows(needle.len()).any(|w| w == needle);
        if expects_literal_ugen_name(ugen) && !contains_name {
            failures.push(format!(
                "{} (category={}): UGen name '{}' not found in encoded bytes",
                func_name, ugen.category, ugen.name
            ));
            continue;
        }
        if !expects_literal_ugen_name(ugen)
            && !pseudo_name_can_appear_inside_raw_name(&ugen.name)
            && contains_name
        {
            failures.push(format!(
                "{} (category={}): pseudo-UGen name '{}' should not appear in encoded bytes",
                func_name, ugen.category, ugen.name
            ));
            continue;
        }

        tested += 1;
    }

    assert!(tested > 0, "no UGens were exercised");
    if !failures.is_empty() {
        panic!(
            "{} of {} sampled UGens failed round-trip:\n  {}",
            failures.len(),
            tested + failures.len(),
            failures.join("\n  ")
        );
    }
}

#[test]
fn audited_non_binary_manifest_entries_are_not_literal_callables() {
    let manifests = load_manifests();
    let by_name: BTreeMap<&str, &UGenManifest> = manifests
        .iter()
        .map(|manifest| (manifest.name.as_str(), manifest))
        .collect();

    for name in CONFIRMED_PSEUDO_UGENS {
        let manifest = by_name
            .get(name)
            .unwrap_or_else(|| panic!("confirmed pseudo-UGen {name} missing from manifest"));
        assert!(manifest.pseudo, "{name} must be tagged pseudo");
        let is_builder_only = manifest.rates.iter().all(|rate| rate == "builder");
        let has_lowering = manifest.ugen_class.is_some() || MANUAL_PSEUDO_LOWERINGS.contains(name);
        assert!(
            is_builder_only || has_lowering,
            "{name} must be unavailable or have an explicit lowering"
        );
    }

    for name in STALE_NON_BINARY_UGENS {
        let Some(manifest) = by_name.get(name) else {
            continue;
        };
        let is_builder_only = manifest.rates.iter().all(|rate| rate == "builder");
        let has_rationale = manifest
            .unavailable_reason
            .as_deref()
            .map(str::trim)
            .is_some_and(|reason| !reason.is_empty());
        assert!(
            manifest.requires_plugin.is_some() || (is_builder_only && has_rationale),
            "{name} must require a plugin or be builder-only with an unavailable rationale"
        );
    }
}

/// Cross-check: every concrete (non-`builder`-only) UGen in every manifest
/// produces at least one parseable scsyndef binary across its rates. This is
/// a stricter guard than the per-category sample — it catches a manifest
/// entry that codegens to a function that crashes the encoder.
#[test]
fn ugen_manifest_round_trip_all_entries() {
    let manifests = load_manifests();
    assert!(!manifests.is_empty(), "no UGen manifests found");

    let mut engine = Engine::new();
    register_dsp_api(&mut engine);

    let mut tested = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for ugen in &manifests {
        let Some(rate) = first_concrete_rate(ugen) else {
            continue;
        };
        let func_name = format!("{}_{}", to_snake_case(&ugen.name), rate);

        set_active_builder(GraphBuilderInner::new());
        let script = format!("{}();", func_name);
        let eval_result: Result<Dynamic, _> = engine.eval(&script);
        let builder = clear_active_builder().expect("active builder vanished");

        if let Err(e) = eval_result {
            failures.push(format!(
                "{} ({}): rhai eval failed: {}",
                func_name, ugen.name, e
            ));
            continue;
        }

        let ir = GraphIR::from_builder(format!("rt_{}", func_name), builder);
        match encode_synthdef(&ir) {
            Ok(bytes) => {
                if let Err(e) = parse_scsyndef_header(&bytes) {
                    failures.push(format!(
                        "{} ({}): header parse failed: {}",
                        func_name, ugen.name, e
                    ));
                    continue;
                }
            }
            Err(e) => {
                failures.push(format!(
                    "{} ({}): encode failed: {}",
                    func_name, ugen.name, e
                ));
                continue;
            }
        }
        tested += 1;
    }

    assert!(tested > 0, "no UGens were exercised");
    if !failures.is_empty() {
        panic!(
            "{} of {} UGens failed round-trip:\n  {}",
            failures.len(),
            tested + failures.len(),
            failures.join("\n  ")
        );
    }
}
