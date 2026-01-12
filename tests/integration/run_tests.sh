#!/bin/bash
# Integration Test Runner for VibeLang
#
# This script runs all .vibe integration tests and reports results.
# Tests are evaluated based on:
# 1. Script execution (no parse/runtime errors)
# 2. Assertion results ([PASS]/[FAIL] markers in output)
# 3. HTTP API state verification (optional, with --api flag)
#
# Requirements:
# - vibe2 binary built (cargo build --release -p vibelang-cli2)
# - SuperCollider installed (scsynth is started automatically by vibe2)
#
# Usage:
#   ./run_tests.sh              # Run all tests (5 seconds each)
#   ./run_tests.sh --quick      # Run all tests (2 seconds each)
#   ./run_tests.sh --verbose    # Show test output
#   ./run_tests.sh --api        # Enable HTTP API verification
#   ./run_tests.sh test_01      # Run specific test (partial match)

# Don't use set -e as it can cause silent exits on expected failures
# set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VIBE_BIN="$PROJECT_ROOT/target/release/vibe2"

# Default settings
TIMEOUT=5
VERBOSE=false
FILTER=""
USE_API=false
API_PORT=16060

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --quick|-q)
            TIMEOUT=2
            shift
            ;;
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        --timeout|-t)
            TIMEOUT="$2"
            shift 2
            ;;
        --api|-a)
            USE_API=true
            shift
            ;;
        --api-port)
            API_PORT="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [options] [filter]"
            echo ""
            echo "Options:"
            echo "  --quick, -q       Run tests with 2s timeout (default: 5s)"
            echo "  --verbose, -v     Show test output"
            echo "  --api, -a         Enable HTTP API state verification"
            echo "  --api-port N      API port (default: 16060)"
            echo "  --timeout, -t N   Set timeout to N seconds"
            echo "  --help, -h        Show this help"
            echo ""
            echo "Filter:"
            echo "  Provide a partial test name to run specific tests"
            echo "  e.g., '$0 transport' runs test_01_transport.vibe"
            echo ""
            echo "Assertion markers parsed from output:"
            echo "  [TEST:name]  - Test suite started"
            echo "  [PASS] msg   - Assertion passed"
            echo "  [FAIL] msg   - Assertion failed"
            echo "  [DONE] n/m   - Test suite completed (n passed of m total)"
            exit 0
            ;;
        *)
            FILTER="$1"
            shift
            ;;
    esac
done

# Check for vibe2 binary
if [[ ! -f "$VIBE_BIN" ]]; then
    echo -e "${YELLOW}Building vibe2...${NC}"
    (cd "$PROJECT_ROOT" && cargo build --release -p vibelang-cli2)
fi

# Collect test files
TEST_FILES=()
for f in "$SCRIPT_DIR"/test_*.vibe; do
    if [[ -f "$f" ]]; then
        if [[ -z "$FILTER" ]] || [[ "$(basename "$f")" == *"$FILTER"* ]]; then
            TEST_FILES+=("$f")
        fi
    fi
done

if [[ ${#TEST_FILES[@]} -eq 0 ]]; then
    echo -e "${RED}No test files found matching filter: $FILTER${NC}"
    exit 1
fi

echo -e "${BLUE}Running ${#TEST_FILES[@]} integration tests (timeout: ${TIMEOUT}s)${NC}"
if [[ "$USE_API" == "true" ]]; then
    echo -e "${CYAN}HTTP API verification enabled on port $API_PORT${NC}"
fi
echo ""

# Parse assertion output from test
# Returns: "passed:failed:total" or "none" if no assertions
parse_assertions() {
    local output_file="$1"
    local passed
    local failed

    # Count [PASS] and [FAIL] markers (strip newlines and handle no matches)
    passed=$(grep -c '^\[PASS\]' "$output_file" 2>/dev/null | tr -d '[:space:]')
    failed=$(grep -c '^\[FAIL\]' "$output_file" 2>/dev/null | tr -d '[:space:]')

    # Default to 0 if empty
    passed=${passed:-0}
    failed=${failed:-0}

    local total=$((passed + failed))

    if [[ $total -eq 0 ]]; then
        echo "none"
    else
        echo "$passed:$failed:$total"
    fi
}

# Verify API state (if enabled)
# Returns: "ok" or error message
verify_api_state() {
    local test_name="$1"

    if [[ "$USE_API" != "true" ]]; then
        echo "skipped"
        return
    fi

    # Wait a bit for the API to be ready
    sleep 0.5

    # Check if API is responding
    local api_url="http://localhost:$API_PORT"

    if ! curl -s --max-time 2 "$api_url/transport" > /dev/null 2>&1; then
        echo "api_unavailable"
        return
    fi

    # Get state snapshots
    local transport=$(curl -s --max-time 2 "$api_url/transport" 2>/dev/null)
    local voices=$(curl -s --max-time 2 "$api_url/voices" 2>/dev/null)
    local patterns=$(curl -s --max-time 2 "$api_url/patterns" 2>/dev/null)
    local melodies=$(curl -s --max-time 2 "$api_url/melodies" 2>/dev/null)

    # Basic sanity checks
    local errors=""

    # Check transport has tempo
    if [[ -n "$transport" ]]; then
        if ! echo "$transport" | grep -q '"tempo"'; then
            errors="${errors}transport_missing_tempo;"
        fi
    fi

    # Check voices array is valid JSON
    if [[ -n "$voices" ]]; then
        if ! echo "$voices" | python3 -m json.tool > /dev/null 2>&1; then
            errors="${errors}voices_invalid_json;"
        fi
    fi

    # Check patterns array
    if [[ -n "$patterns" ]]; then
        if ! echo "$patterns" | python3 -m json.tool > /dev/null 2>&1; then
            errors="${errors}patterns_invalid_json;"
        fi
    fi

    if [[ -z "$errors" ]]; then
        echo "ok"
    else
        echo "$errors"
    fi
}

# Run tests
TOTAL_PASSED=0
TOTAL_FAILED=0
TOTAL_SKIPPED=0
ASSERTION_PASSED=0
ASSERTION_FAILED=0
API_CHECKS_PASSED=0
API_CHECKS_FAILED=0

for test_file in "${TEST_FILES[@]}"; do
    test_name=$(basename "$test_file" .vibe)

    # Check for skip conditions (tests that need external resources)
    skip_reason=""
    case "$test_name" in
        test_07_samples)
            if [[ ! -d "$SCRIPT_DIR/../../samples" ]]; then
                skip_reason="samples/ directory not found"
            fi
            ;;
        test_09_sfz)
            if [[ ! -d "$SCRIPT_DIR/../../instruments" ]]; then
                skip_reason="instruments/ directory not found"
            fi
            ;;
    esac

    if [[ -n "$skip_reason" ]]; then
        echo -e "  ${YELLOW}SKIP${NC} $test_name ($skip_reason)"
        ((TOTAL_SKIPPED++))
        continue
    fi

    # Run test with timeout
    printf "  %-40s " "$test_name"

    output_file=$(mktemp)
    start_time=$(date +%s.%N)

    # Build vibe2 command
    vibe_args=("run" "-I" "$PROJECT_ROOT")
    if [[ "$USE_API" == "true" ]]; then
        vibe_args+=("--api" "--api-port" "$API_PORT")
    fi
    vibe_args+=("$test_file")

    # Run vibe2 with the test file
    if timeout "${TIMEOUT}s" "$VIBE_BIN" "${vibe_args[@]}" > "$output_file" 2>&1; then
        exit_code=0
    else
        exit_code=$?
    fi

    end_time=$(date +%s.%N)
    duration=$(echo "$end_time - $start_time" | bc 2>/dev/null || echo "?")

    # Parse assertions from output
    assertion_result=$(parse_assertions "$output_file")

    # Verify API state
    api_result="skipped"
    if [[ "$USE_API" == "true" ]] && [[ $exit_code -eq 0 || $exit_code -eq 124 ]]; then
        api_result=$(verify_api_state "$test_name")
    fi

    # Determine overall result
    test_passed=true
    failure_reasons=()

    # Check exit code (124 = timeout, which is expected)
    if [[ $exit_code -ne 0 ]] && [[ $exit_code -ne 124 ]]; then
        test_passed=false
        failure_reasons+=("exit:$exit_code")
    fi

    # Check assertions
    if [[ "$assertion_result" != "none" ]]; then
        IFS=':' read -r a_passed a_failed a_total <<< "$assertion_result"
        ASSERTION_PASSED=$((ASSERTION_PASSED + a_passed))
        ASSERTION_FAILED=$((ASSERTION_FAILED + a_failed))

        if [[ $a_failed -gt 0 ]]; then
            test_passed=false
            failure_reasons+=("assertions:$a_failed/$a_total failed")
        fi
    fi

    # Check API result
    if [[ "$api_result" == "ok" ]]; then
        ((API_CHECKS_PASSED++))
    elif [[ "$api_result" != "skipped" ]] && [[ "$api_result" != "api_unavailable" ]]; then
        ((API_CHECKS_FAILED++))
        test_passed=false
        failure_reasons+=("api:$api_result")
    fi

    # Report result
    if [[ "$test_passed" == "true" ]]; then
        status="${GREEN}PASS${NC}"
        ((TOTAL_PASSED++))

        # Show assertion counts if any
        if [[ "$assertion_result" != "none" ]]; then
            IFS=':' read -r a_passed a_failed a_total <<< "$assertion_result"
            status="$status ${CYAN}[$a_passed/$a_total]${NC}"
        fi

        echo -e "$status (${duration}s)"
    else
        echo -e "${RED}FAIL${NC} (${duration}s)"
        ((TOTAL_FAILED++))

        # Show failure reasons
        for reason in "${failure_reasons[@]}"; do
            echo -e "         ${RED}→${NC} $reason"
        done
    fi

    # Show verbose output
    if [[ "$VERBOSE" == "true" ]] || [[ "$test_passed" != "true" ]]; then
        echo -e "${YELLOW}--- Output ---${NC}"

        # Show assertions summary
        if grep -E '^\[(PASS|FAIL|TEST:|DONE)\]' "$output_file" 2>/dev/null; then
            echo ""
        fi

        # Show other output (limited)
        grep -v -E '^\[(PASS|FAIL|TEST:|DONE)\]' "$output_file" 2>/dev/null | head -30

        echo -e "${YELLOW}--- End Output ---${NC}"
    fi

    rm -f "$output_file"
done

echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════${NC}"
echo -e "  ${BLUE}Test Files:${NC}"
echo -e "    ${GREEN}Passed:${NC}  $TOTAL_PASSED"
echo -e "    ${RED}Failed:${NC}  $TOTAL_FAILED"
echo -e "    ${YELLOW}Skipped:${NC} $TOTAL_SKIPPED"

if [[ $((ASSERTION_PASSED + ASSERTION_FAILED)) -gt 0 ]]; then
    echo ""
    echo -e "  ${BLUE}Assertions:${NC}"
    echo -e "    ${GREEN}Passed:${NC}  $ASSERTION_PASSED"
    echo -e "    ${RED}Failed:${NC}  $ASSERTION_FAILED"
fi

if [[ "$USE_API" == "true" ]]; then
    echo ""
    echo -e "  ${BLUE}API Checks:${NC}"
    echo -e "    ${GREEN}Passed:${NC}  $API_CHECKS_PASSED"
    echo -e "    ${RED}Failed:${NC}  $API_CHECKS_FAILED"
fi

echo -e "${BLUE}═══════════════════════════════════════════════════════${NC}"

# Exit with failure if any tests failed
if [[ $TOTAL_FAILED -gt 0 ]] || [[ $ASSERTION_FAILED -gt 0 ]]; then
    exit 1
fi
