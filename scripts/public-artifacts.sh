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
vscode_out="$(mktemp -d "$root/vscode-extension/.out-check.XXXXXX")"
trap 'rm -f "$snapshot"; rm -rf "$vscode_out"' EXIT

assert_cargo_idle() {
  local processes
  processes="$(
    ps -eo comm= |
      awk '$1 == "cargo" || $1 == "rustc" || $1 == "rustdoc" || $1 == "clippy-driver" { print }'
  )"
  if [[ -n "$processes" ]]; then
    echo "refusing to overlap Cargo compiler processes:" >&2
    echo "$processes" >&2
    exit 1
  fi
}

run_cargo() {
  local cargo_status=0
  assert_cargo_idle
  echo "cargo/rustc/rustdoc/clippy-driver idle before: cargo $*"
  CARGO_BUILD_JOBS=1 bash -c 'cargo "$@"' -- "$@" || cargo_status=$?
  assert_cargo_idle
  echo "cargo/rustc/rustdoc/clippy-driver idle after: cargo $*"
  return "$cargo_status"
}

cd "$root"
run_cargo build --locked -p vibelang-cli

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

run_cargo run --locked -p xtask -- public-projections "$mode" "$snapshot"

npm --prefix vscode-extension ci --ignore-scripts --no-audit --no-fund
if [[ "$mode" == "generate" ]]; then
  rm -rf vscode-extension/out
  npm --prefix vscode-extension run compile
  node --test vscode-extension/out/utils/sourceEmitters.test.js
else
  npm --prefix vscode-extension exec -- tsc \
    -p "$root/vscode-extension/tsconfig.json" \
    --outDir "$vscode_out"
  node --test "$vscode_out/utils/sourceEmitters.test.js"
  diff -ru vscode-extension/out "$vscode_out"
  echo "vscode-extension/out matches a clean deterministic TypeScript compile"
fi
vscode_main="$(node -p "require('./vscode-extension/package.json').main")"
case "$vscode_main" in
  ./out/*.js) ;;
  *)
    echo "vscode-extension package main is not a packaged out/*.js artifact: $vscode_main" >&2
    exit 1
    ;;
esac
test -f "vscode-extension/${vscode_main#./}"
echo "vscode-extension package main is compiled and present: $vscode_main"

npm --prefix crates/vibelang-wasm ci --ignore-scripts --no-audit --no-fund
npm --prefix crates/vibelang-wasm run check:types

python3 scripts/v1-baselines.py "$mode"
if [[ "$mode" == "check" ]]; then
  python3 scripts/v1-baselines.py test-drift
fi

run_cargo run --locked -p xtask -- effective-contract "$mode"
