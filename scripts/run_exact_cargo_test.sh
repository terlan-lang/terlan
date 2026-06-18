#!/usr/bin/env bash
# Run one exact Cargo test gate and fail when the filter matches no tests.
#
# Inputs:
# - All command-line arguments after `cargo test`.
#
# Outputs:
# - The original Cargo test output on stdout/stderr.
# - Exit code 0 only when Cargo succeeds and at least one test group runs a
#   non-zero number of tests.
#
# Transformation:
# - Captures Cargo output, preserves it for the caller, and scans the stable
#   `running N tests` lines emitted by Rust's test harness.
set -uo pipefail

set +e
output="$({ cargo test "$@"; } 2>&1)"
status=$?
set -e
printf '%s\n' "$output"

if [ "$status" -ne 0 ]; then
  exit "$status"
fi

if ! printf '%s\n' "$output" | awk '/^running [0-9]+ tests?$/ { if ($2 + 0 > 0) found = 1 } END { exit found ? 0 : 1 }'; then
  echo "exact cargo test matched zero tests: cargo test $*" >&2
  exit 1
fi
