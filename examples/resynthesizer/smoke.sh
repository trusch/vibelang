#!/usr/bin/env bash
set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VIBE_BIN="${VIBE_BIN:-$PROJECT_ROOT/target/release/vibe}"
SCRIPT_FILE="$PROJECT_ROOT/examples/resynthesizer/main.vibe"
SMOKE_SECONDS="${VIBE_RESYNTH_SMOKE_SECONDS:-20}"
LOG_FILE="${VIBE_RESYNTH_SMOKE_LOG:-$(mktemp -t vibelang-resynth-smoke.XXXXXX.log)}"
TEMP_DATA_HOME=""

if ! command -v timeout >/dev/null 2>&1; then
    echo "error: timeout(1) is required for the bounded smoke run" >&2
    exit 2
fi

if [[ ! -x "$VIBE_BIN" ]]; then
    echo "error: release binary not found or not executable: $VIBE_BIN" >&2
    echo "build it with: cargo build --release -p vibelang-cli" >&2
    exit 2
fi

case "$SMOKE_SECONDS" in
    ''|*[!0-9]*)
        echo "error: VIBE_RESYNTH_SMOKE_SECONDS must be a positive integer" >&2
        exit 2
        ;;
esac

if [[ "$SMOKE_SECONDS" -eq 0 ]]; then
    echo "error: VIBE_RESYNTH_SMOKE_SECONDS must be greater than zero" >&2
    exit 2
fi

export RUST_LOG="${VIBE_RESYNTH_SMOKE_RUST_LOG:-info}"

if [[ -n "${VIBE_RESYNTH_SMOKE_XDG_DATA_HOME:-}" ]]; then
    SMOKE_XDG_DATA_HOME="$VIBE_RESYNTH_SMOKE_XDG_DATA_HOME"
else
    TEMP_DATA_HOME="$(mktemp -d -t vibelang-resynth-data.XXXXXX)"
    SMOKE_XDG_DATA_HOME="$TEMP_DATA_HOME"
fi

cleanup() {
    if [[ -n "$TEMP_DATA_HOME" ]]; then
        rm -rf "$TEMP_DATA_HOME"
    fi
}
trap cleanup EXIT

echo "running: $VIBE_BIN $SCRIPT_FILE"
echo "mode: run --no-watch --no-api --no-jack-connect"
echo "duration: ${SMOKE_SECONDS}s"
echo "log: $LOG_FILE"
echo "XDG_DATA_HOME: $SMOKE_XDG_DATA_HOME"

XDG_DATA_HOME="$SMOKE_XDG_DATA_HOME" \
    timeout -s INT -k 5s "${SMOKE_SECONDS}s" \
        "$VIBE_BIN" run --no-watch --no-api --no-jack-connect "$SCRIPT_FILE" \
        >"$LOG_FILE" 2>&1
status=$?

bad_matches="$(
    grep -Ein \
        "UGen ('.*' )?not installed|failed to load synthdef|SynthDef .*not found|synthdef not found|Message too long|LocalBuf tried to allocate too many local buffers|alloc failed|Buffer UGen: no buffer data|Too many grains" \
        "$LOG_FILE" || true
)"

if [[ -n "$bad_matches" ]]; then
    echo "FAIL: resynthesizer smoke found known runtime regression output" >&2
    echo "$bad_matches" | head -n 40 >&2
    exit 1
fi

if ! grep -q "Transport started" "$LOG_FILE"; then
    echo "FAIL: resynthesizer smoke did not reach the transport startup marker" >&2
    tail -n 80 "$LOG_FILE" >&2
    exit 1
fi

case "$status" in
    0|124|130)
        echo "PASS: resynthesizer smoke reached transport startup with no known regression output"
        ;;
    *)
        echo "FAIL: smoke command exited with status $status" >&2
        tail -n 80 "$LOG_FILE" >&2
        exit "$status"
        ;;
esac
