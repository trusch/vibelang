use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vibelang_sfz::parser::parse_sfz_file;

const MAX_SFZ_FILES: usize = 1024;
const MAX_SFZ_BYTES: u64 = 32 * 1024 * 1024;

fn collect_sfz(dir: &Path, files: &mut Vec<PathBuf>, bytes: &mut u64) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("reading {}: {error}", dir.display()))
        .map(|entry| entry.expect("reading corpus directory entry"))
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type().expect("reading corpus entry type");
        assert!(
            !file_type.is_symlink(),
            "corpus contains symlink: {}",
            entry.path().display()
        );
        if file_type.is_dir() {
            if entry.file_name() != ".git" {
                collect_sfz(&entry.path(), files, bytes);
            }
        } else if entry
            .path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("sfz"))
        {
            *bytes += entry
                .metadata()
                .expect("reading corpus entry metadata")
                .len();
            assert!(
                *bytes <= MAX_SFZ_BYTES,
                "SFZ corpus exceeds {MAX_SFZ_BYTES} bytes"
            );
            files.push(entry.path());
            assert!(
                files.len() <= MAX_SFZ_FILES,
                "SFZ corpus exceeds {MAX_SFZ_FILES} files"
            );
        }
    }
}

#[test]
fn versilian_corpus_parses() {
    let Some(roots) = std::env::var_os("VIBELANG_SFZ_CORPUS") else {
        eprintln!("VIBELANG_SFZ_CORPUS is unset; skipping external corpus audit");
        return;
    };
    let mut files = Vec::new();
    let mut bytes = 0;
    for root in std::env::split_paths(&roots) {
        collect_sfz(&root, &mut files, &mut bytes);
    }
    files.sort();
    assert!(
        !files.is_empty(),
        "VIBELANG_SFZ_CORPUS contains no .sfz files"
    );

    let mut failures = Vec::new();
    let mut unknown = BTreeMap::<String, usize>::new();
    for path in &files {
        match parse_sfz_file(path) {
            Ok(parsed) => {
                for (opcode, count) in parsed.unknown_opcodes {
                    *unknown.entry(opcode).or_default() += count;
                }
            }
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }

    eprintln!(
        "audited {} SFZ files ({} bytes); ignored opcodes: {:?}",
        files.len(),
        bytes,
        unknown
    );
    assert!(
        failures.is_empty(),
        "corpus parse failures:\n{}",
        failures.join("\n")
    );
}
