#!/usr/bin/env bash
# Validate checked-in reports against the same staged sources as CI.
set -euo pipefail
mode=${1:-check}
case "$mode" in
  check) dependency_impact_command=dependency-impact-check ;;
  --record-dependency-impact) dependency_impact_command=dependency-impact-record ;;
  *) echo "unsupported Rust report mode: $mode" >&2; exit 2 ;;
esac
if [[ ${TERLAN_HOOK_ARTIFACTS_PREBUILT:-0} != 1 ]]; then
  cargo build --locked -p terlan-rust-boundary-audit -p terlan-test-orchestrator
  cargo build --locked -p terlan --bin terlan-quality --features quality-tools
else
  cargo build --locked -p terlan-rust-boundary-audit
fi
# The commit hook supplies its already-built staged debug tools. Standalone and
# release callers retain the canonical release-profile defaults.
terlc=${TERLAN_RUST_REPORT_TERLC:-"$CARGO_TARGET_DIR/release/terlc"}
vm=${TERLAN_RUST_REPORT_VM:-"$CARGO_TARGET_DIR/release/terlan-vm"}
quality="$CARGO_TARGET_DIR/debug/terlan-quality"
boundary="$CARGO_TARGET_DIR/debug/terlan-rust-boundary-audit"
orchestrator="$CARGO_TARGET_DIR/debug/terlan-test-orchestrator"

hash_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 | awk '{ print $1 }'
  else
    echo 'Rust report caching requires sha256sum or shasum' >&2
    return 1
  fi
}

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

cache_receipt=
cache_key=
cache_schema=terlan-rust-report-result-v1
if [[ "$mode" == check && -n ${TERLAN_RUST_REPORT_CACHE_DIR:-} ]]; then
  cache_dir=$TERLAN_RUST_REPORT_CACHE_DIR
  test ! -L "$cache_dir"
  mkdir -p "$cache_dir"
  cargo_executable=$(command -v cargo)
  rustc_executable=$(command -v rustc)
  cache_key="$({
    printf '%s\n' "$cache_schema" "$(git write-tree)"
    # The staged tree owns every workspace executable source and build command.
    # Hash the external toolchain executables, not checkout-path-sensitive Rust
    # outputs whose bytes can differ despite identical admitted source.
    for executable in "$cargo_executable" "$rustc_executable"; do
      test -f "$executable"
      printf '%s\n' "$executable" "$(hash_file "$executable")"
    done
    cargo -Vv
    rustc -Vv
  } | hash_stream)"
  cache_receipt="$cache_dir/$cache_key.passed"
  if [[ ${TERLAN_TEST_FORCE_RERUN:-0} != 1 \
    && -f "$cache_receipt" \
    && $(wc -l < "$cache_receipt") == 2 \
    && $(sed -n '1p' "$cache_receipt") == "$cache_schema" \
    && $(sed -n '2p' "$cache_receipt") == "$cache_key" ]]; then
    echo "Rust quality reports unchanged; reusing $cache_key"
    exit 0
  fi
fi

"$quality" rust-quality
mkdir -p target/quality
"$boundary" . \
  --api-boundary-input target/quality/rust-api-boundary-input.tsv \
  --shared-helper-input target/quality/rust-shared-helper-input.tsv \
  --structural-input target/quality/rust-structural-input.json \
  > target/quality/rust-boundary-ast.json
"$orchestrator" --cargo-metadata \
  "$PWD/target/quality/rust-cargo-metadata.json" -- cargo --locked
output=target/git-hooks-rust-quality
"$terlc" build scripts/self_validation/rust_quality/scripts/RustQuality.terls \
  --target terlan-vm --out-dir "$output"
for check in api-boundary-check "$dependency_impact_command" file-headroom-check workspace-policy-check build-graph-boundary-check; do
  TERLAN_RUST_QUALITY_ROOT="$PWD" "$vm" run "$output/vm/scripts_RustQuality.tvm" \
    --script-eval -- "$check"
done
if [[ -n "$cache_receipt" ]]; then
  temporary_receipt=$(mktemp "$cache_dir/.rust-report-result.XXXXXX")
  trap 'rm -f "$temporary_receipt"' EXIT
  printf '%s\n%s\n' "$cache_schema" "$cache_key" > "$temporary_receipt"
  mv "$temporary_receipt" "$cache_receipt"
  trap - EXIT
fi
