#!/usr/bin/env bash
# Validate checked-in reports against the same staged sources as CI.
set -euo pipefail
cargo build --locked -p terlan-rust-boundary-audit -p terlan-test-orchestrator
mkdir -p target/quality
"$CARGO_TARGET_DIR/debug/terlan-rust-boundary-audit" . \
  --api-boundary-input target/quality/rust-api-boundary-input.tsv \
  --shared-helper-input target/quality/rust-shared-helper-input.tsv \
  --structural-input target/quality/rust-structural-input.json \
  > target/quality/rust-boundary-ast.json
"$CARGO_TARGET_DIR/debug/terlan-test-orchestrator" --cargo-metadata \
  "$PWD/target/quality/rust-cargo-metadata.json" -- cargo --locked
for check in api-boundary-check dependency-impact-check; do
  TERLAN_RUST_QUALITY_ROOT="$PWD" "$CARGO_TARGET_DIR/debug/terlc" run \
    scripts/self_validation/rust_quality/scripts/RustQuality.terls -- "$check"
done
