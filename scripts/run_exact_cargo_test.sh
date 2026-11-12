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

output=$(mktemp)
trap 'rm -f "$output"' EXIT HUP INT TERM
set +e
cargo test "$@" 2>&1 | tee "$output"
status=${PIPESTATUS[0]}
set -e

if [ "$status" -ne 0 ]; then
  exit "$status"
fi

if ! awk '/^running [0-9]+ tests?$/ { if ($2 + 0 > 0) found = 1 } END { exit found ? 0 : 1 }' "$output"; then
  echo "exact cargo test matched zero tests: cargo test $*" >&2
  exit 1
fi
