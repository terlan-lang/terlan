#!/usr/bin/env bash
# Validate checked-in reports against the same staged sources as CI.
set -euo pipefail
mode=${1:-check}
case "$mode" in
  check) dependency_impact_command=dependency-impact-check ;;
  --record-dependency-impact) dependency_impact_command=dependency-impact-record ;;
  *) echo "unsupported Rust report mode: $mode" >&2; exit 2 ;;
esac
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

checks=(
  api-boundary-check
  "$dependency_impact_command"
  file-headroom-check
  workspace-policy-check
  build-graph-boundary-check
)
checks_to_run=()
cache_enabled=false
cache_schema=terlan-rust-report-result-v2
declare -A cache_keys=()
declare -A cache_receipts=()

report_input_key() {
  local check=$1
  local paths=(
    ':(glob)**/*.rs'
    ':(glob)**/Cargo.toml'
    ':(glob)std/**/*.terl'
    .cargo
    Cargo.lock
    clippy.toml
    rust-toolchain.toml
    rustfmt.toml
    scripts/git_hooks/rust_reports.sh
    scripts/self_validation/rust_quality
  )
  case "$check" in
    api-boundary-check)
      paths+=(
        docs/quality/RUST_STRING_ERROR_BUDGETS.tsv
        docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
      )
      ;;
    dependency-impact-check)
      paths+=(
        docs/quality/RUST_DEPENDENCY_IMPACT.json
        docs/quality/RUST_DUPLICATE_DEPENDENCY_CLASSIFICATION.tsv
        docs/quality/RUST_STRUCTURE_TIMINGS_CLOSEOUT.json
      )
      ;;
    file-headroom-check) paths+=(docs/quality/RUST_FILE_HEADROOM.tsv) ;;
    workspace-policy-check) ;;
    build-graph-boundary-check)
      paths+=(
        docs/quality/RUST_BUILD_GRAPH_BOUNDARIES.json
        docs/quality/RUST_CANONICAL_TYPE_CLASSIFICATIONS.tsv
      )
      ;;
    *) echo "unsupported cached Rust report: $check" >&2; return 2 ;;
  esac
  {
    printf '%s\n%s\n' "$cache_schema" "$check"
    git ls-files -s -- "${paths[@]}"
    # Hash external toolchain executables, not checkout-path-sensitive Rust
    # outputs whose bytes can differ despite identical admitted source.
    local executable
    for executable in "$(command -v cargo)" "$(command -v rustc)"; do
      test -f "$executable"
      printf '%s\n' "$executable" "$(hash_file "$executable")"
    done
    cargo -Vv
    rustc -Vv
  } | hash_stream
}

receipt_is_valid() {
  local receipt=$1 key=$2
  [[ -f "$receipt" \
    && $(wc -l < "$receipt") == 2 \
    && $(sed -n '1p' "$receipt") == "$cache_schema" \
    && $(sed -n '2p' "$receipt") == "$key" ]]
}

write_receipt() {
  local check=$1 key=${cache_keys[$1]} receipt=${cache_receipts[$1]}
  local receipt_dir temporary_receipt
  receipt_dir=$(dirname "$receipt")
  mkdir -p "$receipt_dir"
  temporary_receipt=$(mktemp "$receipt_dir/.result.XXXXXX")
  printf '%s\n%s\n' "$cache_schema" "$key" > "$temporary_receipt"
  mv "$temporary_receipt" "$receipt"
}

if [[ -n ${TERLAN_RUST_REPORT_CACHE_DIR:-} ]]; then
  cache_enabled=true
  cache_dir=$TERLAN_RUST_REPORT_CACHE_DIR
  test ! -L "$cache_dir"
  mkdir -p "$cache_dir"
  for check in "${checks[@]}"; do
    cache_check=$check
    if [[ "$cache_check" == dependency-impact-record ]]; then
      cache_check=dependency-impact-check
    fi
    key=$(report_input_key "$cache_check")
    receipt="$cache_dir/$cache_check/$key.passed"
    cache_keys[$check]=$key
    cache_receipts[$check]=$receipt
    if [[ "$check" != dependency-impact-record \
      && ${TERLAN_TEST_FORCE_RERUN:-0} != 1 ]] \
      && receipt_is_valid "$receipt" "$key"; then
      echo "Rust quality report unchanged: $check; reusing $key"
    else
      checks_to_run+=("$check")
    fi
  done
else
  checks_to_run=("${checks[@]}")
fi

if (( ${#checks_to_run[@]} == 0 )); then
  exit 0
fi

needs_check() {
  local expected=$1 check
  for check in "${checks_to_run[@]}"; do
    [[ "$check" == "$expected" ]] && return 0
  done
  return 1
}

need_boundary=false
need_quality=false
need_metadata=false
if needs_check api-boundary-check \
  || needs_check build-graph-boundary-check \
  || needs_check "$dependency_impact_command"; then
  need_boundary=true
fi
if [[ "$need_boundary" == true ]] || needs_check "$dependency_impact_command"; then
  need_quality=true
fi
if needs_check "$dependency_impact_command" || needs_check build-graph-boundary-check; then
  need_metadata=true
fi

if [[ ${TERLAN_HOOK_ARTIFACTS_PREBUILT:-0} != 1 ]]; then
  packages=()
  [[ "$need_boundary" == true ]] && packages+=(-p terlan-rust-boundary-audit)
  [[ "$need_metadata" == true ]] && packages+=(-p terlan-test-orchestrator)
  if (( ${#packages[@]} > 0 )); then
    cargo build --locked "${packages[@]}"
  fi
  if [[ "$need_quality" == true ]]; then
    cargo build --locked -p terlan --bin terlan-quality --features quality-tools
  fi
elif [[ "$need_boundary" == true ]]; then
  cargo build --locked -p terlan-rust-boundary-audit
fi
if [[ "$need_quality" == true ]]; then
  "$quality" rust-quality
fi
if [[ "$need_boundary" == true ]]; then
  mkdir -p target/quality
  "$boundary" . \
    --api-boundary-input target/quality/rust-api-boundary-input.tsv \
    --shared-helper-input target/quality/rust-shared-helper-input.tsv \
    --structural-input target/quality/rust-structural-input.json \
    > target/quality/rust-boundary-ast.json
fi
if [[ "$need_metadata" == true ]]; then
  mkdir -p target/quality
  "$orchestrator" --cargo-metadata \
    "$PWD/target/quality/rust-cargo-metadata.json" -- cargo --locked
fi
output=${TERLAN_RUST_REPORT_OUTPUT_DIR:-"$CARGO_TARGET_DIR/terlan-validators/rust-quality"}
output_parent=$(dirname "$output")
test ! -L "$output_parent"
mkdir -p "$output_parent"
output_lock="$output_parent/rust-quality.lock"
test ! -L "$output_lock"
exec 9>>"$output_lock"
flock --exclusive 9
# This validator is unchanged across most commits. Keep its compiler-owned
# summaries outside the disposable staged checkout so exact modules survive a
# retry; the compiler still validates every source and dependency identity.
"$terlc" build --incremental \
  scripts/self_validation/rust_quality/scripts/RustQuality.terls \
  --target terlan-vm --out-dir "$output"
for check in "${checks_to_run[@]}"; do
  TERLAN_RUST_QUALITY_ROOT="$PWD" "$vm" run "$output/vm/scripts_RustQuality.tvm" \
    --script-eval -- "$check"
  if [[ "$cache_enabled" == true && "$check" != dependency-impact-record ]]; then
    write_receipt "$check"
  fi
done
