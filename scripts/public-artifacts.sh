#!/usr/bin/env bash
set -euo pipefail

mode="${1:-check}"
case "$mode" in
  generate|check) ;;
  *)
    echo "usage: scripts/public-artifacts.sh <generate|check>" >&2
    exit 2
    ;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
mkdir -p "$target_dir"
snapshot="$(mktemp "$target_dir/public-artifacts.XXXXXX")"
trap 'rm -f "$snapshot"' EXIT

assert_cargo_idle() {
  local processes
  processes="$(ps -eo comm= | awk '$1 == "cargo" || $1 == "rustc" { print }')"
  if [[ -n "$processes" ]]; then
    echo "refusing to overlap Cargo/rustc processes:" >&2
    echo "$processes" >&2
    exit 1
  fi
}

run_cargo() {
  assert_cargo_idle
  echo "cargo/rustc idle before: cargo $*"
  CARGO_BUILD_JOBS=1 bash -c 'cargo "$@"' -- "$@"
}

cd "$root"
run_cargo build -p vibelang-cli

export COLUMNS=100
export LC_ALL=C
export NO_COLOR=1
{
  echo '$ vibe --help'
  "$target_dir/debug/vibe" --help
  for command in run render devices lsp; do
    echo
    echo "$ vibe $command --help"
    "$target_dir/debug/vibe" "$command" --help
  done
} > "$snapshot"

run_cargo run -p xtask -- public-artifacts "$mode" "$snapshot"

npm --prefix crates/vibelang-wasm ci --ignore-scripts --no-audit --no-fund
npm --prefix crates/vibelang-wasm run check:types
