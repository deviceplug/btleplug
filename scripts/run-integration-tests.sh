#!/usr/bin/env bash
#
# Run each integration test individually to avoid multiple simultaneous
# BLE connections to the same test peripheral.
#
# Each test_*.rs file under tests/ is its own binary with a single test,
# ensuring process isolation for BLE stack stability.
#
# Usage:
#   ./scripts/run-integration-tests.sh              # run all tests
#   ./scripts/run-integration-tests.sh test_read_*   # run tests matching a glob
#
# Environment:
#   BTLEPLUG_TEST_PERIPHERAL  - peripheral name (default: btleplug-test)
#   RUST_LOG                  - log level (e.g. debug, btleplug=trace)
#   DELAY                     - seconds to wait between tests (default: 2)
#   TIMEOUT                   - seconds before a test is killed (default: 40)
#
# On Windows (Git Bash/MSYS), also runs the ignored winrtble::adapter radio
# tests from `src/winrtble/adapter.rs` (#476), which need a real Windows
# Bluetooth radio and are otherwise never exercised by this script.

set -euo pipefail

DELAY="${DELAY:-2}"
TIMEOUT="${TIMEOUT:-40}"
PASSED=0
FAILED=0
FAILURES=()

# Windows detection: Git Bash/MSYS sets $OS=Windows_NT; `uname -s` there
# reports MINGW64_NT-... or MSYS_NT-.... Neither matches on macOS/Linux.
IS_WINDOWS=false
if [[ "${OS:-}" == "Windows_NT" ]] || [[ "$(uname -s)" =~ ^(MINGW|MSYS) ]]; then
  IS_WINDOWS=true
fi

# Sentinel test name for the Windows-only winrtble radio lib tests, run
# alongside the per-file integration tests below.
WINRTBLE_RADIO_TESTS="winrtble_radio_tests"

# Discover all test binaries (one per file).
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TESTS_DIR="$(cd "$SCRIPT_DIR/../tests" && pwd)"

# Build list of test names from test_*.rs files (excluding the common/ module).
TEST_NAMES=()
for f in "$TESTS_DIR"/test_*.rs; do
  name="$(basename "$f" .rs)"
  # If a filter was provided, apply it as a glob.
  if [[ $# -gt 0 ]]; then
    matched=false
    for pattern in "$@"; do
      # shellcheck disable=SC2254
      case "$name" in $pattern) matched=true ;; esac
    done
    if ! $matched; then
      continue
    fi
  fi
  TEST_NAMES+=("$name")
done

if $IS_WINDOWS; then
  name="$WINRTBLE_RADIO_TESTS"
  if [[ $# -gt 0 ]]; then
    matched=false
    for pattern in "$@"; do
      # shellcheck disable=SC2254
      case "$name" in $pattern) matched=true ;; esac
    done
    $matched && TEST_NAMES+=("$name")
  else
    TEST_NAMES+=("$name")
  fi
fi

if [[ ${#TEST_NAMES[@]} -eq 0 ]]; then
  echo "No tests matched."
  [[ $# -gt 0 ]] && echo "Filter: $*"
  exit 1
fi

total=${#TEST_NAMES[@]}
echo "=== btleplug integration tests ==="
echo "Running $total tests sequentially (${DELAY}s delay, ${TIMEOUT}s timeout per test)"
echo ""

# Build all needed test binaries once, outside the timed loop below, so
# compile time doesn't eat into each test's TIMEOUT budget. This is
# especially important for the Windows lib-test target, which needs a
# whole-crate cfg(test) build that must otherwise fit entirely inside its
# 40s window alongside the actual test run.
echo "Building test binaries..."
prebuild_cmd=(cargo test --quiet --no-run)
run_lib_tests=false
for test_name in "${TEST_NAMES[@]}"; do
  if [[ "$test_name" == "$WINRTBLE_RADIO_TESTS" ]]; then
    run_lib_tests=true
  else
    prebuild_cmd+=(--test "$test_name")
  fi
done
$run_lib_tests && prebuild_cmd+=(--lib)
"${prebuild_cmd[@]}"

test_num=0
for test_name in "${TEST_NAMES[@]}"; do
  test_num=$((test_num + 1))
  printf "[%2d/%2d] %-55s " "$test_num" "$total" "$test_name"

  if [[ "$test_name" == "$WINRTBLE_RADIO_TESTS" ]]; then
    # Filter to the winrtble::adapter::cleanup_tests module path so this only
    # picks up the #476 radio tests, not any other #[ignore]d lib test that
    # might exist. Serialised (--test-threads=1) since these tests share a
    # single physical Bluetooth radio.
    cargo_cmd=(cargo test --lib "winrtble::adapter::cleanup_tests::" -- --ignored --test-threads=1)
  else
    cargo_cmd=(cargo test --test "$test_name" -- --ignored)
  fi

  # The zero-tests guard below needs the libtest harness's "running N tests"
  # line, which goes to stdout, not stderr -- so only the sentinel's
  # invocation captures both streams to the log. Every other test keeps the
  # existing stderr-only capture (stdout streams live to the terminal).
  if [[ "$test_name" == "$WINRTBLE_RADIO_TESTS" ]]; then
    timeout "${TIMEOUT}s" "${cargo_cmd[@]}" &>/tmp/btleplug-test-output.log && run_status=0 || run_status=$?
  else
    timeout "${TIMEOUT}s" "${cargo_cmd[@]}" 2>/tmp/btleplug-test-output.log && run_status=0 || run_status=$?
  fi

  test_ok=true
  if [[ $run_status -eq 0 ]]; then
    # Even on a zero exit code, cargo reports "running 0 tests" if the
    # module filter matched nothing -- don't let that pass silently.
    if [[ "$test_name" == "$WINRTBLE_RADIO_TESTS" ]] && grep -q "running 0 tests" /tmp/btleplug-test-output.log; then
      test_ok=false
      echo "FAIL (0 tests matched)"
    else
      echo "PASS"
    fi
  else
    test_ok=false
    if [[ $run_status -eq 124 ]]; then
      echo "TIMEOUT (${TIMEOUT}s)"
    else
      echo "FAIL"
    fi
  fi

  if $test_ok; then
    PASSED=$((PASSED + 1))
  else
    FAILED=$((FAILED + 1))
    FAILURES+=("$test_name")
    # Show output for failed tests.
    echo "  --- output ---"
    sed 's/^/  /' /tmp/btleplug-test-output.log | tail -20
    echo "  --- end ---"
  fi

  # Brief delay to let the BLE stack settle between tests.
  if [[ $test_num -lt $total ]]; then
    sleep "$DELAY"
  fi
done

rm -f /tmp/btleplug-test-output.log

echo ""
echo "=== Results ==="
echo "  Passed:  $PASSED"
echo "  Failed:  $FAILED"
echo "  Total:   $total"

if [[ ${#FAILURES[@]} -gt 0 ]]; then
  echo ""
  echo "Failed tests:"
  for f in "${FAILURES[@]}"; do
    echo "  - $f"
  done
  exit 1
fi

echo ""
echo "All tests passed."
