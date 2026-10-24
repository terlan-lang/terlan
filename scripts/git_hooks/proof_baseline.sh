#!/usr/bin/env bash
# Produce fresh proof reports before comparing the staged release baseline.
set -euo pipefail
terlc="$CARGO_TARGET_DIR/release/terlc"
# Semantic runtime oracles use the repository's conventional binary path.
# The hook's fresh snapshot shares only binaries with its external build cache.
mkdir -p target
if [[ ! -e target/debug ]]; then
  ln -s "$CARGO_TARGET_DIR/debug" target/debug
fi
test "$(realpath target/debug/terlc)" = "$(realpath "$CARGO_TARGET_DIR/debug/terlc")"
"$terlc" build scripts/self_validation/proof_release_evidence/scripts/SemanticKernels.terls \
  --target terlan-vm --out-dir target/self-validation/semantic-kernels
"$terlc" build scripts/self_validation/proof_release_evidence/scripts/ProofReleaseEvidence.terls \
  --target terlan-vm --out-dir target/self-validation/proof-release-evidence
# These are the same ordered producers and baseline consumer used by CI.
# Only their compiler, quality binary, and typed images are already built above.
make --no-print-directory --jobs=1 lean-proof-lanes-check \
  TERLAN_VALIDATION_BOOTSTRAPPED=1
make --no-print-directory --jobs=1 release-proof-baseline-check \
  TERLAN_VALIDATION_BOOTSTRAPPED=1
