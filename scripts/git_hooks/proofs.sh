#!/usr/bin/env bash
# Verify proofs after the canonical Rust suite has covered their Rust oracles.
set -euo pipefail
"$CARGO_TARGET_DIR/debug/terlc" test scripts/self_validation/LalrpopGrammarContractTest.terl
make --no-print-directory \
  TERLAN_VALIDATION_BOOTSTRAPPED=1 \
  TERLAN_BOOTSTRAP_COMPILER="$CARGO_TARGET_DIR/debug/terlc" \
  TERLAN_QUALITY="$CARGO_TARGET_DIR/debug/terlan-quality" \
  TERLAN_PROOF_RUNTIME_RUN=: \
  TERLAN_PROOF_TRACK_RUN=: \
  lean-proof-lanes-check
