#!/usr/bin/env bash
# Regenerate only temporary outputs; stale committed summaries reject the commit.
set -euo pipefail
terlc="$CARGO_TARGET_DIR/release/terlc"
TERLAN_SUMMARY_DRIFT_ROOT="$PWD" \
TERLAN_SUMMARY_DRIFT_TERLC="$terlc" \
TERLAN_SUMMARY_DRIFT_GENERATOR="$PWD/scripts/self_validation/BuildInterfacesTest.terl" \
  "$terlc" test scripts/self_validation/SummaryDriftTest.terl
