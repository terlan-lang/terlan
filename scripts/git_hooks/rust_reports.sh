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
# Compile the shared dispatch image once; each check supplies runtime arguments.
runner=target/quality/rust-quality-runner
"$CARGO_TARGET_DIR/debug/terlc" build \
  scripts/self_validation/rust_quality/scripts/RustQuality.terls \
  --target terlan-vm --out-dir "$runner"
for check in api-boundary-check dependency-impact-check file-headroom-check workspace-policy-check build-graph-boundary-check; do
  TERLAN_RUST_QUALITY_ROOT="$PWD" "$CARGO_TARGET_DIR/debug/terlan-vm" run \
    "$runner/vm/scripts_RustQuality.tvm" --script-eval -- "$check"
done
