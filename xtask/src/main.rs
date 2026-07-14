mod public_api;
mod public_artifacts;

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
    let task = args.next().ok_or_else(usage)?;
    let command = args
        .next()
        .ok_or_else(|| format!("missing {task} command: generate or check"))?;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let check = match command.as_str() {
        "generate" => false,
        "check" | "--check" => true,
        _ => return Err(format!("unknown {task} command: {command}")),
    };

    match task.as_str() {
        "public-api" => {
            if args.next().is_some() {
                return Err("unexpected extra arguments".into());
            }
            public_api::generate(&root, check)
        }
        "public-artifacts" => {
            let cli_help = args
                .next()
                .ok_or_else(|| "public-artifacts requires a CLI help snapshot path".to_string())?;
            if args.next().is_some() {
                return Err("unexpected extra arguments".into());
            }
            public_api::generate(&root, check)?;
            public_artifacts::generate(&root, &PathBuf::from(cli_help), check)
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: cargo run -p xtask -- <public-api|public-artifacts> <generate|check> [cli-help-snapshot]"
        .into()
}
