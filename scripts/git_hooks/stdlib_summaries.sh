#!/usr/bin/env bash
# Regenerate only temporary outputs; stale committed summaries reject the commit.
set -euo pipefail
terlc="$CARGO_TARGET_DIR/debug/terlc"
TERLAN_SUMMARY_DRIFT_ROOT="$PWD" \
TERLAN_SUMMARY_DRIFT_TERLC="$terlc" \
TERLAN_SUMMARY_DRIFT_GENERATOR="$PWD/scripts/self_validation/BuildInterfacesTest.terl" \
  "$terlc" test scripts/self_validation/SummaryDriftTest.terl
TERLAN_JS_BINDINGS_ROOT="$PWD" TERLAN_JS_BINDINGS_TERLC="$terlc" \
  "$terlc" test scripts/self_validation/JsBindingsDriftTest.terl
