//! A documented resonance parameter may not reach an `rq` slot unconverted.
//!
//! `rlpf_ar`, `rhpf_ar`, `bpf_ar` and their siblings take **rq** — reciprocal
//! Q, where *lower* means *more* resonance (`ugen_manifests/filters.json`,
//! RLPF: *"Reciprocal of Q. Bandwidth / cutoff frequency. Lower values = more
//! resonance"*). A stdlib parameter documented as *"higher emphasizes the
//! peak"* that is handed straight to one of those slots is inverted against
//! its own docstring: turning it up takes resonance away.
//!
//! `minimoog.vibe:37,73` is the shape that is correct — it exposes a direct
//! 0..1 `resonance` and converts in-body (`1.0 - resonance * 0.9`), so the
//! ugen owns the inversion and the parameter means what it says.
//!
//! This test scans every shipped `.vibe` for the broken shape and compares the
//! result against a frozen inventory, so the docstring and the DSP cannot
//! drift apart again — in these two files or in a new one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Ugen calls whose argument at the given index is an `rq`/`rs`/`bwr` input:
/// lower = more resonance. Derived from `crates/vibelang-dsp/ugen_manifests`.
const RQ_SLOTS: &[(&str, usize)] = &[
    ("rlpf_ar", 2),
    ("rlpf_kr", 2),
    ("rhpf_ar", 2),
    ("rhpf_kr", 2),
    ("bpf_ar", 2),
    ("bpf_kr", 2),
    ("brf_ar", 2),
    ("brf_kr", 2),
    ("resonz_ar", 2),
    ("resonz_kr", 2),
    ("mid_eq_ar", 2),
    ("mid_eq_kr", 2),
    ("b_low_pass_ar", 2),
    ("b_low_pass_kr", 2),
    ("b_hi_pass_ar", 2),
    ("b_hi_pass_kr", 2),
    ("b_low_pass4_ar", 2),
    ("b_hi_pass4_ar", 2),
    ("b_peak_eq_ar", 2),
    ("b_peak_eq_kr", 2),
    ("b_all_pass_ar", 2),
    ("b_low_shelf_ar", 2),
    ("b_hi_shelf_ar", 2),
    ("allpass2_ar", 2),
    ("iir_filter_ar", 2),
    ("glitch_rhpf_ar", 2),
    ("glitch_bpf_ar", 2),
    ("glitch_brf_ar", 2),
    ("rmeq_ar", 2),
    ("peak_eq2_ar", 2),
    ("peak_eq4_ar", 2),
];

/// Every place the stdlib still hands a "higher = more resonance" parameter to
/// an `rq` slot without converting, as `stdlib_relative_path :: param`.
///
/// Frozen. Each entry is a real inversion of the same kind that was fixed in
/// `acid_303_classic.vibe` and `qpas.vibe`; they are listed rather than fixed
/// because each one moves a shipped default and needs its own downstream
/// re-tune. **Removing an entry is the fix — adding one is a regression.**
const KNOWN_INVERSIONS: &[&str] = &[
    "bass/synth/bass_moog.vibe::res",
    "effects/filters/eq_neve_1073.vibe::mid_q",
    "leads/modern/lead_psytrance.vibe::acid",
    "processors/filters/lowpass_mono.vibe::resonance",
    "processors/filters/lowpass_stereo.vibe::resonance",
    "processors/filters/smr8.vibe::q",
];

fn stdlib_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib")
}

fn vibe_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}"));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            vibe_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "vibe") {
            out.push(path);
        }
    }
}

/// Split a call's argument list, starting just after the opening paren.
fn split_args(rest: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in rest.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    args.push(cur);
                    return args;
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                args.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    args.push(cur);
    args
}

/// Identifiers in `expr`, each tagged with whether it was a `.field` access.
fn identifiers(expr: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut is_field = false;
    for ch in expr.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            cur.push(ch);
        } else {
            if !cur.is_empty() {
                out.push((std::mem::take(&mut cur), is_field));
            }
            is_field = ch == '.';
        }
    }
    if !cur.is_empty() {
        out.push((cur, is_field));
    }
    out
}

/// Docstrings that promise the parameter rises with resonance.
fn claims_higher_is_more_resonance(doc: &str) -> bool {
    let d = doc.to_lowercase();
    if d.contains("reciprocal") || d.contains("rq macro") {
        return false;
    }
    let promises_more = d.contains("resonance amount")
        || (d.contains("higher")
            && (d.contains("resona") || d.contains("peak") || d.contains("emphasi")));
    promises_more
}

/// Does the expression (with `let` aliases expanded) convert out of the direct
/// domain? `1.0 - x * k` (minimoog) or `k / (x * a + b)` (acid_bass) both count.
fn is_converted(expr: &str) -> bool {
    let flat: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
    flat.contains("1.0-") || flat.contains("1-") || flat.contains('/')
}

/// Every `<param>` in `src` that reaches an rq slot without being converted.
fn unconverted_rq_params(src: &str) -> Vec<String> {
    // `.param("name", default) // doc`
    let mut docs: HashMap<String, String> = HashMap::new();
    for line in src.lines() {
        let Some(at) = line.find(".param(\"") else {
            continue;
        };
        let after = &line[at + 8..];
        let Some(end) = after.find('"') else { continue };
        let name = after[..end].to_string();
        let doc = line
            .split_once("//")
            .map(|(_, d)| d)
            .unwrap_or("")
            .to_string();
        docs.insert(name, doc);
    }

    // `let name = expr;`
    let mut lets: HashMap<String, String> = HashMap::new();
    for line in src.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("let ") else {
            continue;
        };
        let Some((lhs, rhs)) = rest.split_once('=') else {
            continue;
        };
        let name = lhs.trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        lets.insert(
            name.to_string(),
            rhs.trim().trim_end_matches(';').to_string(),
        );
    }

    // Expand `let` aliases inside an expression, a few levels deep.
    fn expand(expr: &str, lets: &HashMap<String, String>, depth: usize) -> String {
        if depth == 0 {
            return expr.to_string();
        }
        let mut out = String::new();
        let mut cur = String::new();
        // `p.q` is a parameter access, NOT the local `let q`. Without this a
        // sibling conversion in the same body (`let q = 1.0 - q_res * 0.9`)
        // leaks into the expansion of every other `p.<name>` and masks a real
        // finding -- which is exactly what it did before this guard landed.
        let mut is_field = false;
        let flush = |cur: &mut String, out: &mut String, is_field: bool| {
            match lets.get(cur.as_str()) {
                Some(v) if !is_field => {
                    out.push('(');
                    out.push_str(&expand(v, lets, depth - 1));
                    out.push(')');
                }
                _ => out.push_str(cur),
            }
            cur.clear();
        };
        for ch in expr.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                cur.push(ch);
            } else {
                flush(&mut cur, &mut out, is_field);
                is_field = ch == '.';
                out.push(ch);
            }
        }
        flush(&mut cur, &mut out, is_field);
        out
    }

    fn roots(
        expr: &str,
        docs: &HashMap<String, String>,
        lets: &HashMap<String, String>,
        depth: usize,
        out: &mut Vec<String>,
    ) {
        for (id, is_field) in identifiers(expr) {
            // A local `let` shadows nothing when the name appears as `p.<id>`.
            if !is_field && depth > 0 {
                if let Some(rhs) = lets.get(&id) {
                    roots(rhs, docs, lets, depth - 1, out);
                    continue;
                }
            }
            if docs.contains_key(&id) && !out.contains(&id) {
                out.push(id);
            }
        }
    }

    let mut found: Vec<String> = Vec::new();
    for &(ugen, idx) in RQ_SLOTS {
        let needle = format!("{ugen}(");
        let mut from = 0usize;
        while let Some(rel) = src[from..].find(&needle) {
            let start = from + rel;
            // Must be a whole identifier, not the tail of a longer one.
            let is_call_start = src[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            from = start + needle.len();
            if !is_call_start {
                continue;
            }
            let args = split_args(&src[start + needle.len()..]);
            let Some(arg) = args.get(idx) else { continue };
            if is_converted(&expand(arg, &lets, 4)) {
                continue;
            }
            let mut rs = Vec::new();
            roots(arg, &docs, &lets, 4, &mut rs);
            for r in rs {
                let doc = &docs[&r];
                if claims_higher_is_more_resonance(doc) && !found.contains(&r) {
                    found.push(r);
                }
            }
        }
    }
    found.sort();
    found
}

/// The whole stdlib, scanned. Any site not on the frozen list is a regression.
#[test]
fn no_new_documented_resonance_reaches_an_rq_slot_unconverted() {
    let root = stdlib_root();
    let mut files = Vec::new();
    vibe_files(&root, &mut files);
    files.sort();
    assert!(
        files.len() > 500,
        "only {} .vibe files scanned from {root:?} -- the scan went vacuous",
        files.len()
    );

    let mut actual: Vec<String> = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path).expect("read .vibe");
        let rel = path.strip_prefix(&root).unwrap().display().to_string();
        for param in unconverted_rq_params(&src) {
            actual.push(format!("{rel}::{param}"));
        }
    }
    actual.sort();

    let expected: Vec<String> = KNOWN_INVERSIONS.iter().map(|s| s.to_string()).collect();
    let new: Vec<&String> = actual.iter().filter(|a| !expected.contains(a)).collect();
    let gone: Vec<&String> = expected.iter().filter(|e| !actual.contains(e)).collect();

    assert!(
        new.is_empty(),
        "a parameter documented as \"higher = more resonance\" now reaches an \
         rq slot unconverted: {new:?}. rq is reciprocal Q -- LOWER is more \
         resonance -- so turning that parameter up takes resonance AWAY, and \
         its docstring is a lie. Convert in the ugen body the way \
         stdlib/synths/minimoog.vibe:37 does (`rlpf_ar(mix, cutoff, \
         1.0 - resonance * 0.9)`), or re-document the parameter as rq."
    );
    assert!(
        gone.is_empty(),
        "{gone:?} no longer matches the broken shape. If you fixed it, drop it \
         from KNOWN_INVERSIONS -- and re-tune every downstream literal that was \
         voiced against the old rq meaning (super-gamma's factory bank is one)."
    );
}

/// The two files this ticket fixed, pinned individually so the conversion
/// cannot be dropped while the frozen list above still passes.
#[test]
fn the_fixed_instruments_convert_in_body_like_minimoog() {
    let root = stdlib_root();

    let minimoog = std::fs::read_to_string(root.join("synths/minimoog.vibe")).unwrap();
    assert!(
        minimoog.contains("rlpf_ar(mix, cutoff, 1.0 - resonance * 0.9)"),
        "minimoog.vibe is the reference shape for this convention; it moved"
    );

    let acid = std::fs::read_to_string(root.join("bass/acid/acid_303_classic.vibe")).unwrap();
    assert!(
        acid.contains("rlpf_ar(osc, mod_cutoff, 1.0 - res * 0.9)"),
        "acid_303_classic.vibe:8 documents `res` as \"higher values emphasize \
         the cutoff peak\". It must convert to rq in the body -- passing `res` \
         straight to rlpf_ar inverts it against its own docstring."
    );

    let qpas = std::fs::read_to_string(root.join("processors/filters/qpas.vibe")).unwrap();
    for expected in [
        "let q = clip(1.0 - q_res * 0.9, 0.05, 1.0);",
        "let q_low = clip(1.0 - q_low_res * 0.9, 0.05, 1.0);",
        "let q_high = clip(1.0 - q_high_res * 0.9, 0.05, 1.0);",
    ] {
        assert!(
            qpas.contains(expected),
            "qpas.vibe documents `q`/`q_plus`/`q_minus` as \"higher emphasizes \
             a narrower frequency peak\", so all three have to be summed in the \
             direct domain and converted to rq here. Missing: {expected}"
        );
    }
}

/// Negative control: the scanner detects the shape it claims to detect.
#[test]
fn the_scanner_can_fail() {
    let broken = r#"
        define_synthdef("x")
            .param("cutoff", 1000.0) // units: Hz, range: 20..20000, default: 1000.0. Filter cutoff.
            .param("res", 0.5) // units: resonance, range: 0..1, default: 0.5. Filter resonance amount; higher emphasizes a narrower frequency peak.
            .body(|cutoff, res| {
                let filtered = rlpf_ar(saw_ar(110.0), cutoff, res);
                filtered
            });
    "#;
    assert_eq!(unconverted_rq_params(broken), vec!["res".to_string()]);

    let fixed = broken.replace(", res);", ", 1.0 - res * 0.9);");
    assert!(
        unconverted_rq_params(&fixed).is_empty(),
        "converting in-body must clear the finding"
    );

    // A sibling conversion in the same body must not launder an unconverted
    // one. `p.q` is a parameter access; the local `let q` is a different thing.
    let two_bands = r#"
        define_synthdef("y")
            .param("cutoff", 1000.0) // units: Hz, range: 20..20000, default: 1000.0. Filter cutoff.
            .param("q", 0.7) // units: resonance, range: 0..1, default: 0.7. Filter resonance amount; higher emphasizes a narrower frequency peak.
            .body_map(|p| {
                let q_res = clip(p.q, 0.0, 1.0);
                let q = clip(1.0 - q_res * 0.9, 0.05, 1.0);
                let q_high = clip(q_res, 0.05, 1.0);
                let low = rlpf_ar(p.inputs["in"], p.cutoff, q);
                let high = rhpf_ar(p.inputs["in"], p.cutoff, q_high);
                [low + high]
            });
    "#;
    assert_eq!(
        unconverted_rq_params(two_bands),
        vec!["q".to_string()],
        "the converted `q` band must not hide the unconverted `q_high` one"
    );

    // A parameter honestly named and documented as rq is not a finding.
    let honest = broken.replace(
        "Filter resonance amount; higher emphasizes a narrower frequency peak.",
        "RQ macro; higher moves that model axis toward its brighter or more open setting.",
    );
    assert!(unconverted_rq_params(&honest).is_empty());
}
