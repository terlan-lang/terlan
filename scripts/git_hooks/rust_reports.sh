#!/usr/bin/env bash
# Validate checked-in reports against the same staged sources as CI.
set -euo pipefail
mode=${1:-check}
case "$mode" in
  check)
    api_boundary_command=api-boundary-check
    dependency_impact_command=dependency-impact-check
    file_headroom_command=file-headroom-check
    ;;
  --record-dependency-impact)
    api_boundary_command=api-boundary-record
    dependency_impact_command=dependency-impact-record
    file_headroom_command=file-headroom-record
    ;;
  *) echo "unsupported Rust report mode: $mode" >&2; exit 2 ;;
esac
# The commit hook supplies its already-built staged debug tools. Standalone and
# release callers retain the canonical release-profile defaults.
terlc=${TERLAN_RUST_REPORT_TERLC:-"$CARGO_TARGET_DIR/release/terlc"}
vm=${TERLAN_RUST_REPORT_VM:-"$CARGO_TARGET_DIR/release/terlan-vm"}
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
  "$api_boundary_command"
  "$dependency_impact_command"
  "$file_headroom_command"
  module-structure-check
  workspace-policy-check
  build-graph-boundary-check
)
checks_to_run=()
cache_enabled=false
cache_schema=terlan-rust-report-result-v3
declare -A cache_keys=()
declare -A cache_receipts=()

report_input_key() {
  local check=$1
  local generated_report=
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
      generated_report=docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
      paths+=(
        docs/quality/RUST_STRING_ERROR_BUDGETS.tsv
        docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
      )
      ;;
    dependency-impact-check)
      generated_report=docs/quality/RUST_DEPENDENCY_IMPACT.json
      paths+=(
        docs/quality/RUST_DEPENDENCY_IMPACT.json
        docs/quality/RUST_DUPLICATE_DEPENDENCY_CLASSIFICATION.tsv
        docs/quality/RUST_STRUCTURE_TIMINGS_CLOSEOUT.json
      )
      ;;
    file-headroom-check)
      generated_report=docs/quality/RUST_FILE_HEADROOM.tsv
      paths+=(docs/quality/RUST_FILE_HEADROOM.tsv)
      ;;
    module-structure-check|workspace-policy-check) ;;
    build-graph-boundary-check)
      paths+=(
        docs/quality/RUST_BUILD_GRAPH_BOUNDARIES.json
        docs/quality/RUST_CANONICAL_TYPE_CLASSIFICATIONS.tsv
      )
      ;;
    *) echo "unsupported cached Rust report: $check" >&2; return 2 ;;
  esac
  local -a working_paths=("${paths[@]}")
  if [[ -n "$generated_report" ]]; then
    working_paths+=(":(exclude)$generated_report")
  fi
  {
    printf '%s\n%s\n' "$cache_schema" "$check"
    if [[ -n "$generated_report" ]]; then
      git ls-files -s -- "${paths[@]}" ":(exclude)$generated_report"
      printf 'generated-report:%s:%s\n' "$generated_report" \
        "$(hash_file "$generated_report")"
    else
      git ls-files -s -- "${paths[@]}"
    fi
    local -a working_files=()
    while IFS= read -r -d '' path; do
      printf 'working-path:%s\0' "$path"
      if [[ -e "$path" || -L "$path" ]]; then
        working_files+=("$path")
      else
        printf 'working-object:missing\0'
      fi
    done < <(git ls-files -z --cached --others --exclude-standard -- "${working_paths[@]}")
    local offset
    for ((offset = 0; offset < ${#working_files[@]}; offset += 256)); do
      git hash-object --no-filters -- "${working_files[@]:offset:256}"
    done
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
    if [[ "$cache_check" == api-boundary-record ]]; then
      cache_check=api-boundary-check
    elif [[ "$cache_check" == dependency-impact-record ]]; then
      cache_check=dependency-impact-check
    elif [[ "$cache_check" == file-headroom-record ]]; then
      cache_check=file-headroom-check
    fi
    key=$(report_input_key "$cache_check")
    receipt="$cache_dir/$cache_check/$key.passed"
    cache_keys[$check]=$key
    cache_receipts[$check]=$receipt
    if [[ ${TERLAN_TEST_FORCE_RERUN:-0} != 1 ]] \
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
need_metadata=false
if needs_check "$api_boundary_command" \
  || needs_check build-graph-boundary-check \
  || needs_check "$dependency_impact_command"; then
  need_boundary=true
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
elif [[ "$need_boundary" == true ]]; then
  cargo build --locked -p terlan-rust-boundary-audit
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
report_timeout=${TERLAN_RUST_REPORT_TIMEOUT_SECONDS:-120}
case "$report_timeout" in ''|*[!0-9]*) echo 'invalid Rust report timeout' >&2; exit 2 ;; esac
report_jobs=$(mktemp -d "$output_parent/.rust-report-jobs.XXXXXX")
declare -a report_pids=() report_logs=() report_durations=()
declare -A report_job_statuses=()
report_max_jobs=${TERLAN_RUST_REPORT_MAX_JOBS:-2}
case "$report_max_jobs" in
  ''|*[!0-9]*|0) echo 'invalid Rust report worker limit' >&2; exit 2 ;;
esac
report_active_jobs=0
cleanup_report_jobs() {
  local status=$? pid
  trap - EXIT INT TERM HUP
  for pid in "${report_pids[@]}"; do
    kill -TERM "$pid" 2>/dev/null || true
  done
  for pid in "${report_pids[@]}"; do
    wait "$pid" 2>/dev/null || true
  done
  [[ ! -d "$report_jobs" ]] || find "$report_jobs" -depth -delete
  exit "$status"
}
trap cleanup_report_jobs EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
collect_report_job() {
  local completed_pid status=0
  wait -n -p completed_pid || status=$?
  report_job_statuses[$completed_pid]=$status
  report_active_jobs=$((report_active_jobs - 1))
}
for index in "${!checks_to_run[@]}"; do
  check=${checks_to_run[$index]}
  log="$report_jobs/$index.log"
  duration="$report_jobs/$index.duration"
  (
    started=$SECONDS child=
    status=0
    cleanup_report_child() {
      local child_status=$?
      trap - EXIT INT TERM HUP
      if [[ -n "$child" ]]; then
        kill -TERM "$child" 2>/dev/null || true
        wait "$child" 2>/dev/null || true
      fi
      exit "$child_status"
    }
    trap cleanup_report_child EXIT INT TERM HUP
    timeout --kill-after=5s "${report_timeout}s" \
      env TERLAN_RUST_QUALITY_ROOT="$PWD" \
      "$vm" run "$output/vm/scripts_RustQuality.tvm" \
      --script-eval -- "$check" > "$log" 2>&1 &
    child=$!
    wait "$child" || status=$?
    child=
    trap - EXIT INT TERM HUP
    printf '%s\n' "$((SECONDS - started))" > "$duration"
    exit "$status"
  ) &
  report_pids+=("$!")
  report_logs+=("$log")
  report_durations+=("$duration")
  report_active_jobs=$((report_active_jobs + 1))
  if (( report_active_jobs >= report_max_jobs )); then
    collect_report_job
  fi
done
while (( report_active_jobs > 0 )); do collect_report_job; done
report_status=0
for index in "${!report_pids[@]}"; do
  check=${checks_to_run[$index]}
  if (( report_job_statuses[${report_pids[$index]}] == 0 )); then
    echo "Rust quality report passed: $check ($(cat "${report_durations[$index]}")s)"
    if [[ "$cache_enabled" == true ]]; then
      cache_check=$check
      if [[ "$cache_check" == api-boundary-record ]]; then
        cache_check=api-boundary-check
      elif [[ "$cache_check" == dependency-impact-record ]]; then
        cache_check=dependency-impact-check
      elif [[ "$cache_check" == file-headroom-record ]]; then
        cache_check=file-headroom-check
      fi
      if [[ "$cache_check" != "$check" ]]; then
        key=$(report_input_key "$cache_check")
        cache_keys[$check]=$key
        cache_receipts[$check]="$cache_dir/$cache_check/$key.passed"
      fi
      write_receipt "$check"
    fi
  else
    report_status=1
    job_status=${report_job_statuses[${report_pids[$index]}]}
    if (( job_status == 124 )); then
      echo "Rust quality report exceeded ${report_timeout}s: $check" >&2
    else
      echo "Rust quality report failed: $check ($(cat "${report_durations[$index]}")s)" >&2
    fi
  fi
  cat "${report_logs[$index]}"
done
trap - EXIT INT TERM HUP
find "$report_jobs" -depth -delete
exit "$report_status"
