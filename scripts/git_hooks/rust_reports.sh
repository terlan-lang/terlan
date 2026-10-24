#!/usr/bin/env bash
# Validate checked-in reports against the same staged sources as CI.
set -euo pipefail
cargo build --locked -p terlan-rust-boundary-audit -p terlan-test-orchestrator
cargo build --locked -p terlan --bin terlan-quality --features quality-tools
"$CARGO_TARGET_DIR/debug/terlan-quality" rust-quality
mkdir -p target/quality
"$CARGO_TARGET_DIR/debug/terlan-rust-boundary-audit" . \
  --api-boundary-input target/quality/rust-api-boundary-input.tsv \
  --shared-helper-input target/quality/rust-shared-helper-input.tsv \
  --structural-input target/quality/rust-structural-input.json \
  > target/quality/rust-boundary-ast.json
"$CARGO_TARGET_DIR/debug/terlan-test-orchestrator" --cargo-metadata \
  "$PWD/target/quality/rust-cargo-metadata.json" -- cargo --locked
# check.sh has already built and validated these canonical release tools.
# Compile this exact snapshot once, then execute every report check from that image.
terlc="$CARGO_TARGET_DIR/release/terlc"
vm="$CARGO_TARGET_DIR/release/terlan-vm"
output=target/git-hooks-rust-quality
"$terlc" build scripts/self_validation/rust_quality/scripts/RustQuality.terls \
  --target terlan-vm --out-dir "$output"
for check in api-boundary-check dependency-impact-check file-headroom-check workspace-policy-check build-graph-boundary-check; do
  TERLAN_RUST_QUALITY_ROOT="$PWD" "$vm" run "$output/vm/scripts_RustQuality.tvm" \
    --script-eval -- "$check"
done
