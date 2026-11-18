#!/usr/bin/env bash
# Verify proofs under the canonical Rust suite's live coverage owner.
set -euo pipefail
"$CARGO_TARGET_DIR/debug/terlc" test scripts/self_validation/LalrpopGrammarContractTest.terl
orchestrator=${TERLAN_RUST_ORCHESTRATOR:?missing TERLAN_RUST_ORCHESTRATOR}
report="$PWD/target/quality/rust-test-suite-report.json"
test -s "$report"
"$orchestrator" --install-snapshot "$PWD/target/validation-tools/terlan-test-orchestrator"
TERLAN_RUST_ORCHESTRATOR="$orchestrator" \
  "$orchestrator" --with-cargo-coverage "$report" \
    --graph-timeout-seconds 900 -- \
    make --no-print-directory \
      TERLAN_VALIDATION_BOOTSTRAPPED=1 \
      TERLAN_PROOF_RUNTIME_RUN=: \
      TERLAN_PROOF_TRACK_RUN=: \
      lean-proof-lanes-check
