#!/usr/bin/env bash
# Verify staged proof inputs and execute the same isolated replicas used by CI.
set -euo pipefail
"$CARGO_TARGET_DIR/debug/terlc" test scripts/self_validation/LalrpopGrammarContractTest.terl
"$CARGO_TARGET_DIR/debug/terlan-quality" lean-proof-track
