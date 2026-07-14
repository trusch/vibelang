mod public_api;

use std::env;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() != Some("public-api") {
        return Err("usage: cargo run -p xtask -- public-api <generate|check>".into());
    }
    let command = args
        .next()
        .ok_or_else(|| "missing public-api command: generate or check".to_string())?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    match command.as_str() {
        "generate" => public_api::generate(&root, false),
        "check" | "--check" => public_api::generate(&root, true),
        _ => Err(format!("unknown public-api command: {command}")),
    }
}
