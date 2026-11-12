#!/usr/bin/env bash
# Regenerate only temporary outputs; stale committed summaries reject the commit.
set -euo pipefail
terlc=${TERLAN_SUMMARY_HOOK_TERLC:-"$CARGO_TARGET_DIR/release/terlc"}
TERLAN_SUMMARY_DRIFT_ROOT="$PWD" \
TERLAN_SUMMARY_DRIFT_TERLC="$terlc" \
  "$terlc" test scripts/self_validation/stdlib_interfaces/tests/stdlib_interfaces/SummaryDriftTest.terl
