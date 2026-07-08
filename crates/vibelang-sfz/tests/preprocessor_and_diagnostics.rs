//! Integration tests for the SFZ preprocessor (#include / #define),
//! unknown-opcode collection, dropped-region diagnostics, and the
//! region-matching data needed by the NOTE_OFF path.

use std::fs;
use std::path::{Path, PathBuf};

use vibelang_sfz::parser::{parse_sfz_file, parse_sfz_str};
use vibelang_sfz::{
    calculate_playback_rate, find_matching_regions, load_sfz_instrument, DropReason,
    RoundRobinState, TriggerMode,
};

/// Create a unique temp directory for one test.
fn temp_dir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("vibelang_sfz_test_{}_{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write a minimal valid 16-bit mono PCM WAV file with `frames` frames.
fn write_wav(path: &Path, frames: u32) {
    let channels: u16 = 1;
    let sample_rate: u32 = 44100;
    let bits: u16 = 16;
    let data_size = frames * (bits as u32 / 8) * channels as u32;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * channels as u32 * bits as u32 / 8;
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&(channels * bits / 8).to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    bytes.resize(bytes.len() + data_size as usize, 0);
    fs::write(path, bytes).unwrap();
}

// ---------------------------------------------------------------------------
// #include
// ---------------------------------------------------------------------------

#[test]
fn include_nested_resolves_relative_to_including_file() {
    let dir = temp_dir("include_nested");
    let sub = dir.join("inc");
    fs::create_dir_all(&sub).unwrap();

    // main.sfz -> inc/level1.sfz -> level2.sfz (relative to inc/)
    fs::write(
        dir.join("main.sfz"),
        "<region>\nsample=root.wav\nkey=60\n#include \"inc/level1.sfz\"\n",
    )
    .unwrap();
    fs::write(
        sub.join("level1.sfz"),
        "<region>\nsample=one.wav\nkey=61\n#include \"level2.sfz\"\n",
    )
    .unwrap();
    fs::write(sub.join("level2.sfz"), "<region>\nsample=two.wav\nkey=62\n").unwrap();

    let sfz = parse_sfz_file(dir.join("main.sfz")).expect("nested include should parse");
    assert_eq!(sfz.regions.len(), 3);
    assert_eq!(sfz.regions[0].get_opcode_str("key"), Some("60"));
    assert_eq!(sfz.regions[1].get_opcode_str("sample"), Some("one.wav"));
    assert_eq!(sfz.regions[2].get_opcode_str("sample"), Some("two.wav"));
}

#[test]
fn include_cycle_is_detected() {
    let dir = temp_dir("include_cycle");
    fs::write(dir.join("a.sfz"), "#include \"b.sfz\"\n").unwrap();
    fs::write(dir.join("b.sfz"), "#include \"a.sfz\"\n").unwrap();

    let err = parse_sfz_file(dir.join("a.sfz")).unwrap_err();
    assert!(
        err.to_string().contains("cycle"),
        "expected cycle error, got: {}",
        err
    );
}

#[test]
fn include_self_is_detected() {
    let dir = temp_dir("include_self");
    fs::write(dir.join("a.sfz"), "#include \"a.sfz\"\n").unwrap();

    let err = parse_sfz_file(dir.join("a.sfz")).unwrap_err();
    assert!(err.to_string().contains("cycle"), "got: {}", err);
}

#[test]
fn include_missing_file_errors() {
    let dir = temp_dir("include_missing");
    fs::write(
        dir.join("main.sfz"),
        "<region>\nsample=x.wav\n#include \"nope.sfz\"\n",
    )
    .unwrap();

    let err = parse_sfz_file(dir.join("main.sfz")).unwrap_err();
    assert!(
        err.to_string().contains("File not found"),
        "expected file-not-found, got: {}",
        err
    );
}

#[test]
fn include_in_string_context_errors() {
    let err = parse_sfz_str("#include \"other.sfz\"\n<region>\n").unwrap_err();
    assert!(
        err.to_string().contains("no base directory"),
        "got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// #define
// ---------------------------------------------------------------------------

#[test]
fn define_substitutes_in_opcodes_and_values() {
    let sfz = parse_sfz_str(
        "#define $NOTE 64\n#define $DIR samples\n<region>\nsample=$DIR/a.wav\nkey=$NOTE\n",
    )
    .unwrap();
    assert_eq!(sfz.regions[0].get_opcode_str("key"), Some("64"));
    assert_eq!(
        sfz.regions[0].get_opcode_str("sample"),
        Some("samples/a.wav")
    );
}

#[test]
fn later_define_overrides_earlier() {
    let sfz = parse_sfz_str(
        "#define $VEL 10\n<region>\nlovel=$VEL\n#define $VEL 90\n<region>\nlovel=$VEL\n",
    )
    .unwrap();
    assert_eq!(sfz.regions[0].get_opcode_str("lovel"), Some("10"));
    assert_eq!(sfz.regions[1].get_opcode_str("lovel"), Some("90"));
}

#[test]
fn define_from_include_is_visible_in_parent() {
    let dir = temp_dir("define_include");
    fs::write(dir.join("defs.sfz"), "#define $CENTER 57\n").unwrap();
    fs::write(
        dir.join("main.sfz"),
        "#include \"defs.sfz\"\n<region>\nsample=a.wav\npitch_keycenter=$CENTER\n",
    )
    .unwrap();

    let sfz = parse_sfz_file(dir.join("main.sfz")).unwrap();
    assert_eq!(sfz.regions[0].get_opcode_str("pitch_keycenter"), Some("57"));
}

#[test]
fn define_can_parameterize_include_path() {
    let dir = temp_dir("define_include_path");
    let sub = dir.join("kits");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("kit_a.sfz"), "<region>\nsample=a.wav\n").unwrap();
    fs::write(
        dir.join("main.sfz"),
        "#define $KIT kit_a\n#include \"kits/$KIT.sfz\"\n",
    )
    .unwrap();

    let sfz = parse_sfz_file(dir.join("main.sfz")).unwrap();
    assert_eq!(sfz.regions.len(), 1);
}

// ---------------------------------------------------------------------------
// Unknown-opcode collection
// ---------------------------------------------------------------------------

#[test]
fn unknown_opcodes_are_collected_with_counts() {
    let sfz = parse_sfz_str(
        "<global>\nvolume=0\nmy_fancy_opcode=1\n\
         <region>\nsample=a.wav\nkey=60\nmy_fancy_opcode=2\ntypo_opcod=3\n",
    )
    .unwrap();
    assert_eq!(sfz.unknown_opcodes.get("my_fancy_opcode"), Some(&2));
    assert_eq!(sfz.unknown_opcodes.get("typo_opcod"), Some(&1));
    // Known opcodes are not flagged.
    assert!(!sfz.unknown_opcodes.contains_key("volume"));
    assert!(!sfz.unknown_opcodes.contains_key("sample"));
    assert!(!sfz.unknown_opcodes.contains_key("key"));
}

#[test]
fn numbered_cc_opcodes_are_recognized() {
    let sfz = parse_sfz_str(
        "<region>\nsample=a.wav\ncutoff_oncc23=1200\nlocc64=0\namp_velcurve_96=0.5\n",
    )
    .unwrap();
    assert!(
        sfz.unknown_opcodes.is_empty(),
        "numbered opcodes flagged as unknown: {:?}",
        sfz.unknown_opcodes
    );
}

#[test]
fn curve_and_effect_sections_are_exempt_from_unknown_tracking() {
    let sfz = parse_sfz_str("<curve>\ncurve_index=1\nv000=0\nv127=1\n<effect>\nvendor_param=3\n")
        .unwrap();
    assert!(
        sfz.unknown_opcodes.is_empty(),
        "curve/effect opcodes flagged: {:?}",
        sfz.unknown_opcodes
    );
}

// ---------------------------------------------------------------------------
// Dropped-region diagnostics
// ---------------------------------------------------------------------------

#[test]
fn loader_counts_parsed_loaded_and_dropped_regions() {
    let dir = temp_dir("dropped_regions");
    write_wav(&dir.join("good.wav"), 1000);
    fs::write(
        dir.join("inst.sfz"),
        "<region>\nsample=good.wav\nkey=60\n\
         <region>\nsample=missing.wav\nkey=61\n\
         <region>\nkey=62\n", // no sample opcode
    )
    .unwrap();

    let mut next_buffer_id = 0;
    let instrument = load_sfz_instrument(
        dir.join("inst.sfz"),
        "test".to_string(),
        &mut |_path, _id| Ok(()),
        &mut next_buffer_id,
    )
    .unwrap();

    let diag = &instrument.diagnostics;
    assert_eq!(diag.regions_parsed, 3);
    assert_eq!(diag.regions_loaded, 1);
    assert_eq!(diag.regions_dropped(), 2);
    assert_eq!(instrument.regions.len(), 1);

    let reasons: Vec<&DropReason> = diag.dropped_regions.iter().map(|d| &d.reason).collect();
    assert!(reasons.contains(&&DropReason::MissingSampleFile));
    assert!(reasons.contains(&&DropReason::NoSampleOpcode));

    let summary = diag.dropped_summary();
    assert!(summary.contains("missing sample file: 1"), "{}", summary);
    assert!(summary.contains("no sample opcode: 1"), "{}", summary);
}

#[test]
fn loader_reports_buffer_load_failures() {
    let dir = temp_dir("buffer_fail");
    write_wav(&dir.join("good.wav"), 100);
    fs::write(dir.join("inst.sfz"), "<region>\nsample=good.wav\nkey=60\n").unwrap();

    let mut next_buffer_id = 0;
    let instrument = load_sfz_instrument(
        dir.join("inst.sfz"),
        "test".to_string(),
        &mut |_path, _id| anyhow::bail!("backend exploded"),
        &mut next_buffer_id,
    )
    .unwrap();

    assert_eq!(instrument.diagnostics.regions_loaded, 0);
    assert_eq!(instrument.diagnostics.regions_dropped(), 1);
    match &instrument.diagnostics.dropped_regions[0].reason {
        DropReason::BufferLoadFailed(msg) => assert!(msg.contains("backend exploded")),
        other => panic!("wrong reason: {:?}", other),
    }
}

#[test]
fn loader_surfaces_unknown_opcodes_in_diagnostics() {
    let dir = temp_dir("unknown_in_loader");
    write_wav(&dir.join("a.wav"), 100);
    fs::write(
        dir.join("inst.sfz"),
        "<region>\nsample=a.wav\nkey=60\nweird_op=1\nweird_op=2\n",
    )
    .unwrap();

    let mut next_buffer_id = 0;
    let instrument = load_sfz_instrument(
        dir.join("inst.sfz"),
        "test".to_string(),
        &mut |_path, _id| Ok(()),
        &mut next_buffer_id,
    )
    .unwrap();

    // Same-named opcode twice in one section overwrites, but both source
    // lines are counted.
    assert_eq!(
        instrument.diagnostics.unknown_opcodes,
        vec![("weird_op".to_string(), 2)]
    );
    assert!(instrument
        .diagnostics
        .unknown_opcodes_summary()
        .contains("weird_op (x2)"));
}

// ---------------------------------------------------------------------------
// NOTE_OFF root-cause: the crate delivers everything the note-on path must
// apply to the synth. These tests pin the region-matching contract that
// vibelang-core's note-on currently fails to consume (see report).
// ---------------------------------------------------------------------------

#[test]
fn region_matching_provides_rate_and_release_for_note_on() {
    let dir = temp_dir("noteoff_contract");
    write_wav(&dir.join("a3.wav"), 44100);
    write_wav(&dir.join("a4.wav"), 44100);
    // Mirror of examples/tutorials/assets/tutorial_pluck.sfz
    fs::write(
        dir.join("pluck.sfz"),
        "<group>\nampeg_attack=0.001\nampeg_release=0.15\n\
         <region>\nsample=a3.wav\npitch_keycenter=57\nlokey=0\nhikey=59\n\
         <region>\nsample=a4.wav\npitch_keycenter=69\nlokey=60\nhikey=127\n",
    )
    .unwrap();

    let mut next_buffer_id = 0;
    let instrument = load_sfz_instrument(
        dir.join("pluck.sfz"),
        "pluck".to_string(),
        &mut |_path, _id| Ok(()),
        &mut next_buffer_id,
    )
    .unwrap();

    let mut rr = RoundRobinState::new();

    // Note 45 (A2) must match the A3 region and repitch one octave down.
    let regions = find_matching_regions(&instrument, 45, 100, TriggerMode::Attack, &mut rr);
    assert_eq!(regions.len(), 1);
    let region = regions[0];
    assert_eq!(region.opcodes.pitch_keycenter, Some(57));
    let rate = calculate_playback_rate(45, region.opcodes.pitch_keycenter, None, None);
    assert!((rate - 0.5).abs() < 1e-6, "expected rate 0.5, got {}", rate);

    // The region carries the release the gate-close (NOTE_OFF) must use.
    // vibelang-core's note-on path must forward this as the `release`
    // synth param — the sfz_voice synthdef default (0.01s) clicks.
    assert_eq!(region.opcodes.ampeg_release, Some(0.15));

    // Note 69 (A4) matches the second region at unity rate.
    let regions = find_matching_regions(&instrument, 69, 100, TriggerMode::Attack, &mut rr);
    assert_eq!(regions.len(), 1);
    let rate = calculate_playback_rate(69, regions[0].opcodes.pitch_keycenter, None, None);
    assert!((rate - 1.0).abs() < 1e-6);

    // The two regions map to different buffers — note-on must set `bufnum`
    // per note; the synthdef default (bufnum=0) always plays the first.
    let low = find_matching_regions(&instrument, 45, 100, TriggerMode::Attack, &mut rr);
    let high = find_matching_regions(&instrument, 69, 100, TriggerMode::Attack, &mut rr);
    assert_ne!(low[0].buffer_id, high[0].buffer_id);
}

#[test]
fn release_trigger_regions_do_not_match_attack() {
    let dir = temp_dir("release_trigger");
    write_wav(&dir.join("a.wav"), 100);
    write_wav(&dir.join("rel.wav"), 100);
    fs::write(
        dir.join("inst.sfz"),
        "<region>\nsample=a.wav\nkey=60\n\
         <region>\nsample=rel.wav\nkey=60\ntrigger=release\n",
    )
    .unwrap();

    let mut next_buffer_id = 0;
    let instrument = load_sfz_instrument(
        dir.join("inst.sfz"),
        "t".to_string(),
        &mut |_p, _i| Ok(()),
        &mut next_buffer_id,
    )
    .unwrap();

    let mut rr = RoundRobinState::new();
    let attack = find_matching_regions(&instrument, 60, 100, TriggerMode::Attack, &mut rr);
    assert_eq!(attack.len(), 1);
    assert_eq!(attack[0].trigger, TriggerMode::Attack);

    let release = find_matching_regions(&instrument, 60, 100, TriggerMode::Release, &mut rr);
    assert_eq!(release.len(), 1);
    assert_eq!(release[0].trigger, TriggerMode::Release);
}

#[test]
fn loop_sustain_region_reports_loop_enabled() {
    // loop_sustain patches are the ones that must be gate-released by
    // NOTE_OFF (they never end on their own). The loader must expose
    // loop_mode so note-on can set `loop=1`.
    let dir = temp_dir("loop_sustain");
    write_wav(&dir.join("pad.wav"), 100);
    fs::write(
        dir.join("inst.sfz"),
        "<region>\nsample=pad.wav\nkey=60\nloop_mode=loop_sustain\nloop_start=10\nloop_end=90\n",
    )
    .unwrap();

    let mut next_buffer_id = 0;
    let instrument = load_sfz_instrument(
        dir.join("inst.sfz"),
        "t".to_string(),
        &mut |_p, _i| Ok(()),
        &mut next_buffer_id,
    )
    .unwrap();

    let region = &instrument.regions[0];
    assert!(matches!(
        region.loop_mode,
        vibelang_sfz::LoopMode::Loop | vibelang_sfz::LoopMode::LoopContinuous
    ));
    assert_eq!(region.loop_start, Some(10));
    assert_eq!(region.loop_end, Some(90));
}
