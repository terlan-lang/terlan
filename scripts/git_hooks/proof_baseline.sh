#!/usr/bin/env bash
# Reject proof-source and policy drift before the canonical Rust suite.
set -euo pipefail
terlc=${TERLC:-"$CARGO_TARGET_DIR/debug/terlc"}
test -x "$terlc"
quality="$CARGO_TARGET_DIR/debug/terlan-quality"
test -x "$quality"
# Semantic runtime oracles use the repository's conventional binary path.
# The hook's fresh snapshot shares only binaries with its external build cache.
mkdir -p target
mkdir -p target/debug
for tool in terlc terlan-vm terlan-quality; do
  source_tool="$CARGO_TARGET_DIR/debug/$tool"
  target_tool="target/debug/$tool"
  test -x "$source_tool"
  if [[ ! -e "$target_tool" ]]; then
    ln -s "$source_tool" "$target_tool"
  fi
  test "$(realpath "$target_tool")" = "$(realpath "$source_tool")"
done
test "$(realpath target/debug/terlc)" = "$(realpath "$CARGO_TARGET_DIR/debug/terlc")"
"$terlc" build scripts/self_validation/proof_release_evidence/scripts/SemanticKernels.terls \
  --target terlan-vm --out-dir target/self-validation/semantic-kernels
"$quality" lean-proof-feature-cull
"$quality" lean-proof-pr
TERLAN_LEAN_PROOF_LANES_ROOT="$PWD" \
  "$terlc" test --incremental scripts/self_validation/LeanProofLanesTest.terl \
    --name repository_lean_proof_feature_map_is_current
TERLAN_LEAN_PROOF_ROOT="$PWD" \
  TERLAN_LEAN_PROOF_PATH=Terlan/FeatureCull/LegacyBoundaries.lean \
  "$terlc" test scripts/self_validation/LeanProofExecutionTest.terl
"$quality" lean-proof-runtime
"$quality" lean-proof-track
