#!/usr/bin/env bash
# Execute the Terlan-owned plan inside the staged-source validation clone.
set -euo pipefail
changes=$1
base=$2
hook_mode=${TERLAN_HOOK_MODE:-fast}
case "$hook_mode" in fast|full) ;; *) echo "unsupported hook mode: $hook_mode" >&2; exit 2 ;; esac
hosted_source_preflight=${TERLAN_HOSTED_SOURCE_PREFLIGHT:-0}
case "$hosted_source_preflight" in 0|1) ;; *) echo "unsupported hosted source-preflight mode: $hosted_source_preflight" >&2; exit 2 ;; esac
hosted_dependency_audit=${TERLAN_HOSTED_DEPENDENCY_AUDIT:-0}
case "$hosted_dependency_audit" in 0|1) ;; *) echo "unsupported hosted dependency-audit mode: $hosted_dependency_audit" >&2; exit 2 ;; esac
target_terlc="$CARGO_TARGET_DIR/debug/terlc"
terlc="$target_terlc"
export TERLC="$terlc"
raw_plan="${changes%.txt}.raw-plan"
preflight_plan="${changes%.txt}.preflight-plan"
plan="${changes%.txt}.plan"
specialized_tests="${changes%.txt}.specialized-tests.txt"
prechecked_terlan="${changes%.txt}.prechecked-terlan.txt"
compiler_package_roots() {
  if [[ ! -f Cargo.toml ]]; then
    if [[ -d scripts ]]; then printf 'scripts\n'; else printf '.\n'; fi
    return
  fi
  cargo metadata --locked --format-version 1 --features terlan/validation-union \
    | jq -r '
      . as $metadata
      | ($metadata.packages[] | select(.name == "terlan") | .id) as $root
      | def expand:
          . as $ids
          | reduce ($metadata.resolve.nodes[]
              | select(.id as $id | $ids | index($id))
              | .deps[].pkg) as $dependency
              ($ids; if index($dependency) then . else . + [$dependency] end);
        [$root]
      | until((expand) == .; expand)
      | . as $ids
      | $metadata.packages[]
      | select(.source == null and (.id as $id | $ids | index($id)))
      | .manifest_path
    ' \
    | while IFS= read -r manifest; do
        realpath --relative-to="$PWD" "$(dirname "$manifest")"
      done \
    | LC_ALL=C sort -u
}
compiler_source_is_test_only() {
  local path=$1
  case "/$path/" in
    */tests/*|*/*_test/*|*/*_tests/*) return 0 ;;
  esac
  case "$path" in
    *_test.rs|*_test_support.rs|*_test_fixture.rs) return 0 ;;
  esac
  return 1
}
compiler_input_key="$({
  printf '%s\n' terlan-staged-compiler-input-v4
  git ls-files -s -z -- \
    Cargo.toml Cargo.lock rust-toolchain.toml .cargo
  while IFS= read -r package_root; do
    while IFS= read -r -d '' entry; do
      path=${entry#*$'\t'}
      if ! compiler_source_is_test_only "$path"; then
        printf '%s\0' "$entry"
      fi
    done < <(git ls-files -s -z -- "$package_root" \
      ':(exclude)crates/terlan/src/compiler/syntax/terlan_lalrpop.lalrpop')
  done < <(compiler_package_roots)
  embedded_interfaces=crates/terlan/src/formal_pipeline/embedded_interfaces.rs
  if [[ -f "$embedded_interfaces" ]]; then
    sed -nE 's/^[[:space:]]*(embedded|namespace)!\("([^"]+)"\),?$/\2/p' \
      "$embedded_interfaces" \
      | while IFS= read -r module; do
          git ls-files -s -z -- "std/summaries/$module.typi"
        done
  fi
  if [[ -f crates/terlan/src/compiler/syntax/terlan_lalrpop.lalrpop ]]; then
    printf 'normalized-lalrpop:'
    sed '/^\/\/ canonical-ebnf-sha256: /d' \
      crates/terlan/src/compiler/syntax/terlan_lalrpop.lalrpop | git hash-object --stdin
  fi
} | git hash-object --stdin)"
compiler_input_receipt="$CARGO_TARGET_DIR/.terlan-compiler-input"
exact_compiler_inputs=false
if [[ -x "$target_terlc" \
  && -f "$compiler_input_receipt" \
  && ! -L "$compiler_input_receipt" \
  && $(wc -l < "$compiler_input_receipt") == 1 \
  && $(sed -n '1p' "$compiler_input_receipt") == "$compiler_input_key" ]]; then
  exact_compiler_inputs=true
fi
compiler_build=true
candidate_clippy_checked=false
if [[ "$exact_compiler_inputs" == true ]]; then compiler_build=false; fi
run_plan() {
  local compiler=$1
  local planner_arguments=("$changes")
  if [[ "$hosted_source_preflight" == 1 ]]; then
    planner_arguments+=(--hosted-source-preflight)
  fi
  : > "$raw_plan"
  if ! "$compiler" run scripts/git_hooks/scripts/Plan.terls -- \
    "${planner_arguments[@]}" > "$raw_plan"; then
    echo 'pre-commit: candidate compiler could not produce a validation plan' >&2
    return 1
  fi
  if [[ ! -s "$raw_plan" ]]; then
    echo 'pre-commit: candidate compiler produced an empty validation plan' >&2
    return 1
  fi
  if [[ -f tests/language/RepeatedLetSyntaxTest.terl ]] \
    && ! "$compiler" check tests/language/RepeatedLetSyntaxTest.terl >/dev/null; then
    echo 'pre-commit: candidate compiler rejected the syntax sentinel' >&2
    return 1
  fi
}
if [[ "$hook_mode" == fast ]]; then
  terlc=${TERLAN_HOOK_LATEST_TERLC:-$target_terlc}
  if [[ ! -x "$terlc" || -L "$terlc" ]]; then
    echo 'pre-commit: fast admission requires a locally built terlc' >&2
    exit 1
  fi
  export TERLC="$terlc"
  run_plan "$terlc" || { echo 'pre-commit: latest terlc cannot plan the staged source' >&2; exit 1; }
  compiler_build=false
  echo "pre-commit: fast admission uses $terlc"
elif [[ "$exact_compiler_inputs" == false \
  && -n ${TERLAN_HOOK_LATEST_TERLC:-} \
  && -x ${TERLAN_HOOK_LATEST_TERLC:-} \
  && ! -L ${TERLAN_HOOK_LATEST_TERLC:-} ]] \
  && run_plan "$TERLAN_HOOK_LATEST_TERLC" \
  && ! grep -Fxq 'preflight:compiler-build' "$raw_plan"; then
  # The cached staged compiler may predate an earlier compiler commit.  That
  # does not justify rebuilding it for a later documentation, report, or hook
  # policy commit.  Use the latest checkout compiler to plan those changes;
  # the planner still requires a candidate rebuild for real compiler inputs.
  compiler_build=false
  terlc=$TERLAN_HOOK_LATEST_TERLC
  export TERLC="$terlc"
  echo 'pre-commit: reuse latest compiler; staged changes are not compiler inputs'
elif [[ "$compiler_build" == false ]]; then
  if [[ -x "$terlc" ]] && run_plan "$terlc"; then
    :
  else
    repository_root=$(git remote get-url origin)
    reusable_terlc="$repository_root/target/debug/terlc"
    if [[ "$repository_root" == /* && -f "$reusable_terlc" && -x "$reusable_terlc" && ! -L "$reusable_terlc" ]] \
      && run_plan "$reusable_terlc"; then
      terlc="$reusable_terlc"
      export TERLC="$terlc"
      echo "pre-commit: reuse compatible compiler $terlc"
    else
      compiler_build=true
    fi
  fi
fi
if [[ "$hook_mode" == full && "$compiler_build" == false ]]; then
  if grep -Fxq 'preflight:compiler-build' "$raw_plan"; then compiler_build=true; fi
fi
if [[ "$compiler_build" == true \
  && "$exact_compiler_inputs" == true ]] \
  && run_plan "$target_terlc"; then
  compiler_build=false
  terlc="$target_terlc"
  export TERLC="$terlc"
  echo 'pre-commit: reuse exact staged compiler inputs'
fi
if [[ "$compiler_build" == true ]]; then
  # This target survives staged checkout revisions. Cargo dep-info can still
  # point at the prior snapshot even when no caller supplied a shared target.
  # Invalidate workspace-owned fingerprints before every required rebuild.
  cache_metadata=$(mktemp)
  cargo metadata --locked --no-deps --format-version 1 > "$cache_metadata"
  cleanup_cache_metadata() {
    local result=$?
    trap - EXIT
    rm -f "$cache_metadata"
    exit "$result"
  }
  trap cleanup_cache_metadata EXIT
  node scripts/git_hooks/invalidate_workspace_cache.cjs \
    "$cache_metadata" "$CARGO_TARGET_DIR" "$PWD"
  echo 'pre-commit: lint and check the changed staged workspace once'
  # One default-feature Clippy build owns candidate checking and Rust linting.
  # The affected Clippy lane below reuses this result instead of compiling the
  # workspace again. Release validation owns the all-features profile.
  make --no-print-directory rust-clippy-precommit-check
  candidate_clippy_checked=true
  terlc=${TERLAN_HOOK_LATEST_TERLC:-$target_terlc}
  [[ -x "$terlc" && ! -L "$terlc" ]] || {
    echo 'pre-commit: candidate checking requires a latest working compiler' >&2
    exit 1
  }
  export TERLC="$terlc"
  run_plan "$terlc"
else
  echo 'pre-commit: reuse compiler; no compiler inputs changed'
fi
compiler_validation_identity=$compiler_input_key
if [[ "$terlc" != "$target_terlc" ]]; then
  # A compatible external compiler may bootstrap planning, but its validation
  # evidence is not interchangeable with the staged candidate compiler.
  compiler_validation_identity=$(git hash-object "$terlc")
fi
sed -n 's/^preflight://p' "$raw_plan" > "$preflight_plan"
sed -n 's/^specialized-test://p' "$raw_plan" > "$specialized_tests"
grep -Ev '^(preflight:|specialized-test:)' "$raw_plan" > "$plan"
: > "$prechecked_terlan"
preflight_has() { grep -Fxq "$1" "$preflight_plan"; }
lane_has() { grep -Fxq "$1" "$plan"; }

if [[ "$hook_mode" == fast ]]; then
  admission_started=$SECONDS
  admission_status=0
  fast_jobs=${TERLAN_HOOK_FAST_JOBS:-8}
  case "$fast_jobs" in ''|*[!0-9]*|0) echo 'Invalid fast admission job count' >&2; exit 2 ;; esac
  fast_output_dir=$(mktemp -d "$CARGO_TARGET_DIR/fast-admission.XXXXXX")
  fast_receipt_dir=${TERLAN_HOOK_VALIDATION_RECEIPT_DIR:-$CARGO_TARGET_DIR/validation-results/fast-admission}
  fast_cache_dir="$CARGO_TARGET_DIR/fast-admission-cache/$(git hash-object "$terlc")"
  test ! -L "$fast_receipt_dir"
  test ! -L "$fast_cache_dir"
  mkdir -p "$fast_receipt_dir" "$fast_cache_dir"
  fast_receipt_schema=terlan-fast-admission-v1
  fast_compiler_hash=$(git hash-object "$terlc")
  fast_labels=()
  fast_pids=()
  fast_logs=()
  fast_durations=()
  fast_keys=()
  fast_receipts=()
  declare -A fast_admitted_keys=()
  fast_terlan_paths=()
  while IFS= read -r path; do
    case "$path" in
      compiler/self_host/corpus/*|scripts/git_hooks/scripts/Plan.terls) ;;
      *.terl|*.terli|*.terls) [[ -f "$path" ]] && fast_terlan_paths+=("$path") ;;
    esac
  done < "$changes"
  fast_rust_format() {
    "$terlc" run scripts/ci/scripts/CheckRustFormatting.terls \
      --out-dir "$fast_cache_dir/rust-format-build" \
      --cache-dir "$fast_cache_dir/rust-format-cache" \
      -- --paths-file "$changes"
  }
  fast_grammar() {
    local status=0
    TERLAN_EBNF_ROOT="$PWD" TERLAN_EBNF_COMPILER="$terlc" \
      "$terlc" run scripts/self_validation/EbnfValidator.terls \
        --out-dir "$fast_cache_dir/grammar-build" \
        --cache-dir "$fast_cache_dir/grammar-cache" || status=1
    TERLAN_SYNTAX_ARTIFACT_ROOT="$PWD" TERLAN_SYNTAX_ARTIFACT_COMPILER="$terlc" \
      "$terlc" run scripts/ci/scripts/SyncSyntaxContractArtifacts.terls \
        --out-dir "$fast_cache_dir/syntax-artifacts-build" \
        --cache-dir "$fast_cache_dir/syntax-artifacts-cache" \
        -- --check || status=1
    return "$status"
  }
  fast_terlan_format() {
    local status=0 lint_pid format_pid
    if (( ${#fast_terlan_paths[@]} == 0 )); then return 0; fi
    (printf '%s\0' "${fast_terlan_paths[@]}" \
      | xargs -0 -r -n 48 -P "$fast_jobs" "$terlc" lint --only TL0009 --only TL0010) &
    lint_pid=$!
    (printf '%s\0' "${fast_terlan_paths[@]}" \
      | xargs -0 -r -n 48 -P "$fast_jobs" "$terlc" fmt --check) &
    format_pid=$!
    wait "$lint_pid" || status=1
    wait "$format_pid" || status=1
    return "$status"
  }
  fast_shell() {
    local status=0 path
    while IFS= read -r path; do
      if [[ -f "$path" && ( "$path" == *.sh || "$path" == .githooks/* ) ]]; then
        bash -n "$path" || status=1
      fi
    done < "$changes"
    return "$status"
  }
  fast_gate_key() {
    local label=$1 path
    {
      printf '%s\n%s\n%s\n' "$fast_receipt_schema" "$label" "$fast_compiler_hash"
      git ls-files -s -- scripts/git_hooks/check.sh \
        scripts/git_hooks/scripts/Plan.terls \
        scripts/git_hooks/src/git_hooks/Plan.terl
      for tool in git bash xargs; do
        tool_path=$(command -v "$tool")
        printf 'tool:%s:%s\n' "$tool" "$(git hash-object "$tool_path")"
      done
      case "$label" in
        rust-format)
          git ls-files -s -z -- rustfmt.toml scripts/ci/scripts/CheckRustFormatting.terls \
            scripts/ci/src/ci/RustFormatting.terl
          while IFS= read -r path; do
            case "$path" in *.rs) printf 'path:%s\n' "$path"; git ls-files -s -- "$path" ;; esac
          done < "$changes"
          ;;
        grammar)
          git ls-files -s -z -- docs/grammar crates/terlan/src/compiler/syntax \
            proofs/lean/artifacts proofs/lean/ci/lean-proof-artifacts.tsv \
            proofs/lean/parser_shape/ParserShape.lean tests/fixtures/phase_contract \
            scripts/ci/scripts/SyncSyntaxContractArtifacts.terls \
            scripts/ci/src/ci/SyntaxContractArtifacts.terl \
            scripts/self_validation/EbnfValidator.terls
          ;;
        terlan-format)
          while IFS= read -r path; do
            case "$path" in
              *.terl|*.terli|*.terls)
                printf 'path:%s\n' "$path"
                git ls-files -s -- "$path"
                ;;
            esac
          done < "$changes"
          ;;
        shell)
          while IFS= read -r path; do
            case "$path" in *.sh|.githooks/*) printf 'path:%s\n' "$path"; git ls-files -s -- "$path" ;; esac
          done < "$changes"
          ;;
      esac
    } | git hash-object --stdin
  }
  start_fast_gate() {
    local label=$1 function=$2 log="$fast_output_dir/$1.log" \
      duration="$fast_output_dir/$1.duration" key receipt
    key=$(fast_gate_key "$label")
    receipt="$fast_receipt_dir/$label-$key.passed"
    if [[ ${TERLAN_TEST_FORCE_RERUN:-0} != 1 \
      && -f "$receipt" \
      && $(wc -l < "$receipt") == 2 \
      && $(sed -n '1p' "$receipt") == "$fast_receipt_schema" \
      && $(sed -n '2p' "$receipt") == "$key" ]]; then
      echo "pre-commit: reuse $label ($key)"
      fast_admitted_keys["$label"]=$key
      return 0
    fi
    echo "pre-commit: start $label"
    (
      gate_started=$SECONDS
      set +e
      "$function"
      gate_status=$?
      set -e
      printf '%s\n' "$((SECONDS - gate_started))" > "$duration"
      exit "$gate_status"
    ) >"$log" 2>&1 &
    fast_labels+=("$label")
    fast_pids+=("$!")
    fast_logs+=("$log")
    fast_durations+=("$duration")
    fast_keys+=("$key")
    fast_receipts+=("$receipt")
  }
  if preflight_has rust-format; then
    start_fast_gate rust-format fast_rust_format
  fi
  if preflight_has grammar; then
    start_fast_gate grammar fast_grammar
  fi
  if preflight_has terlan-format && (( ${#fast_terlan_paths[@]} > 0 )); then
    echo "pre-commit: changed Terlan syntax (${#fast_terlan_paths[@]} paths, $fast_jobs workers)"
    start_fast_gate terlan-format fast_terlan_format
  fi
  start_fast_gate shell fast_shell
  for index in "${!fast_pids[@]}"; do
    fast_elapsed=unknown
    if wait "${fast_pids[$index]}"; then
      if [[ -f ${fast_durations[$index]} ]]; then
        fast_elapsed=$(sed -n '1p' "${fast_durations[$index]}")s
      fi
      echo "pre-commit: pass ${fast_labels[$index]} (${fast_elapsed})"
      temporary_fast_receipt=$(mktemp "$fast_receipt_dir/.fast-admission.XXXXXX")
      printf '%s\n%s\n' "$fast_receipt_schema" "${fast_keys[$index]}" > "$temporary_fast_receipt"
      mv "$temporary_fast_receipt" "${fast_receipts[$index]}"
      fast_admitted_keys["${fast_labels[$index]}"]=${fast_keys[$index]}
    else
      admission_status=1
      if [[ -f ${fast_durations[$index]} ]]; then
        fast_elapsed=$(sed -n '1p' "${fast_durations[$index]}")s
      fi
      echo "pre-commit: fail ${fast_labels[$index]} (${fast_elapsed})" >&2
    fi
    cat "${fast_logs[$index]}"
  done
  rm -rf -- "$fast_output_dir"
  admission_elapsed=$((SECONDS - admission_started))
  echo "pre-commit: fast admission finished in ${admission_elapsed}s"
  if (( admission_elapsed > ${TERLAN_HOOK_FAST_TIMEOUT_SECONDS:-60} )); then
    echo 'pre-commit: fast admission exceeded its performance budget' >&2
    admission_status=1
  fi
  if (( admission_status != 0 )); then exit "$admission_status"; fi
  candidate_plan="${changes%.txt}.candidate-plan"
  {
    while IFS= read -r label; do
      [[ -n "$label" ]] || continue
      if [[ -n ${fast_admitted_keys[$label]+present} ]]; then
        printf 'admitted:%s:%s\n' "$label" "${fast_admitted_keys[$label]}"
      else
        printf 'preflight:%s\n' "$label"
      fi
    done < "$preflight_plan"
    while IFS= read -r label; do
      [[ -n "$label" ]] || continue
      if [[ -n ${fast_admitted_keys[$label]+present} ]]; then
        printf 'admitted:%s:%s\n' "$label" "${fast_admitted_keys[$label]}"
      else
        printf '%s\n' "$label"
      fi
    done < "$plan"
  } > "$candidate_plan"
  echo "pre-commit: candidate validation deferred to content-addressed graph $candidate_plan"
  exit 0
fi

if preflight_has rust-format; then
  echo 'pre-commit: changed Rust formatting'
  "$terlc" run scripts/ci/scripts/CheckRustFormatting.terls -- --paths-file "$changes"
fi

if preflight_has grammar; then
  echo 'pre-commit: early canonical grammar and summary'
  TERLAN_EBNF_ROOT="$PWD" TERLAN_EBNF_COMPILER="$terlc" \
    "$terlc" run scripts/self_validation/EbnfValidator.terls
  TERLAN_SYNTAX_ARTIFACT_ROOT="$PWD" TERLAN_SYNTAX_ARTIFACT_COMPILER="$terlc" \
    "$terlc" run scripts/ci/scripts/SyncSyntaxContractArtifacts.terls -- --check
fi

if preflight_has terlan-format; then
  terlan_paths=()
  while IFS= read -r path; do
    case "$path" in
      compiler/self_host/corpus/*)
        # Parser acceptance fixtures intentionally contain syntax that the
        # production formatter must reject before macro resolution.
        ;;
      scripts/git_hooks/scripts/Plan.terls)
        # This bootstrap planner is executed before this point and deliberately
        # stays parseable by both the previous and candidate compiler syntax.
        ;;
      *.terl|*.terli|*.terls)
        if [[ -f "$path" ]]; then terlan_paths+=("$path"); fi
        ;;
    esac
  done < "$changes"
  if (( ${#terlan_paths[@]} > 0 )); then
    echo 'pre-commit: early Terlan readability canonicalization'
    "$terlc" lint --only TL0009 --only TL0010 "${terlan_paths[@]}"
    echo 'pre-commit: early Terlan formatting'
    "$terlc" fmt --check "${terlan_paths[@]}"
  fi
fi

if preflight_has rust-lint-allowances; then
  lint_allowance_schema=terlan-rust-lint-allowance-result-v1
  lint_allowance_key="$({
    printf '%s\n%s\n' "$lint_allowance_schema" "$compiler_validation_identity"
    git ls-files -s -- \
      ':(glob)**/*.rs' \
      docs/quality/RUST_LINT_ALLOWANCES.tsv \
      scripts/self_validation/rust_quality
  } | git hash-object --stdin)"
  lint_allowance_receipt_dir="${TERLAN_HOOK_VALIDATION_RECEIPT_DIR:-$CARGO_TARGET_DIR/validation-results/terlan}/rust-lint-allowances"
  lint_allowance_receipt="$lint_allowance_receipt_dir/$lint_allowance_key.passed"
  if [[ ${TERLAN_TEST_FORCE_RERUN:-0} != 1 \
    && -f "$lint_allowance_receipt" \
    && $(wc -l < "$lint_allowance_receipt") == 2 \
    && $(sed -n '1p' "$lint_allowance_receipt") == "$lint_allowance_schema" \
    && $(sed -n '2p' "$lint_allowance_receipt") == "$lint_allowance_key" ]]; then
    echo "pre-commit: reuse Rust lint allowance policy ($lint_allowance_key)"
  else
    echo 'pre-commit: early Rust lint allowance policy'
    rust_quality_dir="$CARGO_TARGET_DIR/terlan-validators/rust-quality"
    rust_quality_image="$rust_quality_dir/vm/scripts_RustQuality.tvm"
    "$terlc" build --incremental \
      scripts/self_validation/rust_quality/scripts/RustQuality.terls \
      --target terlan-vm --out-dir "$rust_quality_dir"
    test -s "$rust_quality_image"
    rust_allowance_paths=()
    lint_allowance_registry_changed=false
    while IFS= read -r path; do
      case "$path" in
        docs/quality/RUST_LINT_ALLOWANCES.tsv)
          lint_allowance_registry_changed=true
          ;;
        *.rs)
          rust_allowance_paths+=("$path")
          ;;
      esac
    done < "$changes"
    if [[ "$lint_allowance_registry_changed" == true ]]; then
      "$CARGO_TARGET_DIR/debug/terlan-vm" run "$rust_quality_image" \
        --script-eval -- lint-allowance-check
    else
      "$CARGO_TARGET_DIR/debug/terlan-vm" run "$rust_quality_image" \
        --script-eval -- lint-allowance-changed-check "${rust_allowance_paths[@]}"
    fi
    test ! -L "$lint_allowance_receipt_dir"
    mkdir -p "$lint_allowance_receipt_dir"
    temporary_lint_allowance_receipt=$(mktemp "$lint_allowance_receipt_dir/.result.XXXXXX")
    printf '%s\n%s\n' "$lint_allowance_schema" "$lint_allowance_key" \
      > "$temporary_lint_allowance_receipt"
    mv "$temporary_lint_allowance_receipt" "$lint_allowance_receipt"
  fi
fi

release_tools_ready=false
quality_tools_ready=false
orchestrator_ready=false
orchestrator_path=
quality_path=
native_test_companions_ready=false
test_program_snapshots_ready=false
ensure_release_tools() {
  if [[ "$release_tools_ready" == false ]]; then
    echo 'pre-commit: build required release tools once'
    cargo build --release --locked -p terlan \
      --bin terlc --bin terlan-vm --bin terlan-native-worker
    release_tools_ready=true
  fi
  terlc="$CARGO_TARGET_DIR/release/terlc"
  export TERLC="$terlc"
}
ensure_quality_tools() {
  if [[ "$quality_tools_ready" == false ]]; then
    local latest_quality
    latest_quality="$(dirname "$terlc")/terlan-quality"
    if [[ "$terlc" != "$target_terlc" \
      && -x "$latest_quality" \
      && ! -L "$latest_quality" ]]; then
      echo "pre-commit: reuse latest validation quality tool $latest_quality"
      quality_path="$latest_quality"
    else
      cargo build --locked -p terlan --bin terlan-quality --features validation-union
      quality_path="$CARGO_TARGET_DIR/debug/terlan-quality"
    fi
    quality_tools_ready=true
  fi
}
ensure_orchestrator() {
  if [[ "$orchestrator_ready" == false ]]; then
    local source digest directory temporary
    echo 'pre-commit: link the exact staged test orchestrator'
    cargo build --locked -p terlan-test-orchestrator
    source="$CARGO_TARGET_DIR/debug/terlan-test-orchestrator"
    digest=$(sha256sum "$source" | awk '{print $1}')
    directory="$CARGO_TARGET_DIR/validation-tools"
    orchestrator_path="$directory/terlan-test-orchestrator-$digest"
    mkdir -p "$directory"
    if [[ ! -x "$orchestrator_path" \
      || "$(sha256sum "$orchestrator_path" | awk '{print $1}')" != "$digest" ]]; then
      temporary="$orchestrator_path.tmp.$$"
      install -m 755 "$source" "$temporary"
      mv "$temporary" "$orchestrator_path"
    fi
    orchestrator_ready=true
  fi
}
ensure_native_test_companions() {
  if [[ "$native_test_companions_ready" == true ]]; then
    return
  fi
  if [[ "$terlc" != "$target_terlc" ]]; then
    local latest_worker
    latest_worker="$(dirname "$terlc")/terlan-native-worker"
    [[ -x "$latest_worker" && ! -L "$latest_worker" ]] || {
      echo "pre-commit: latest compiler has no matching native worker: $latest_worker" >&2
      return 1
    }
    echo "pre-commit: reuse latest native test companion $latest_worker"
    export TERLAN_NATIVE_WORKER="$latest_worker"
    native_test_companions_ready=true
    return
  fi
  local worker="$CARGO_TARGET_DIR/debug/terlan-native-worker"
  local receipt="$CARGO_TARGET_DIR/.terlan-native-worker-input"
  if [[ -x "$worker" \
    && ! -L "$worker" \
    && -f "$receipt" \
    && ! -L "$receipt" \
    && $(wc -l < "$receipt") == 1 \
    && $(sed -n '1p' "$receipt") == "$compiler_input_key" ]]; then
    echo 'pre-commit: reuse exact native test companion'
  else
    echo 'pre-commit: build required native test companion once'
    cargo build --locked -p terlan --bin terlan-native-worker --features validation-union
    local temporary_receipt
    temporary_receipt=$(mktemp "$CARGO_TARGET_DIR/.terlan-native-worker-input.XXXXXX")
    printf '%s\n' "$compiler_input_key" > "$temporary_receipt"
    mv "$temporary_receipt" "$receipt"
  fi
  native_test_companions_ready=true
  export TERLAN_NATIVE_WORKER="$CARGO_TARGET_DIR/debug/terlan-native-worker"
}
snapshot_test_programs() {
  "$orchestrator_path" --snapshot-test-programs "$CARGO_TARGET_DIR"
}
ensure_test_program_snapshots() {
  if [[ "$test_program_snapshots_ready" == false ]]; then
    local directory compiler_directory
    ensure_native_test_companions
    ensure_orchestrator
    compiler_directory=$(dirname "$terlc")
    TERLAN_RUST_SUITE_TERLC="$terlc"
    TERLAN_RUST_SUITE_VM="$compiler_directory/terlan-vm"
    TERLAN_RUST_SUITE_NATIVE_WORKER="$TERLAN_NATIVE_WORKER"
    export TERLAN_RUST_SUITE_TERLC
    export TERLAN_RUST_SUITE_VM
    export TERLAN_RUST_SUITE_NATIVE_WORKER
    directory=$(snapshot_test_programs)
    TERLAN_RUST_SUITE_TERLC="$directory/terlc"
    TERLAN_RUST_SUITE_VM="$directory/terlan-vm"
    TERLAN_RUST_SUITE_NATIVE_WORKER="$directory/terlan-native-worker"
    test_program_snapshots_ready=true
  fi
}
compiler_heavy_test_shape() {
  local host_threads build_threads test_threads
  host_threads=$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf '1\n')
  if [[ ! "$host_threads" =~ ^[1-9][0-9]*$ ]]; then host_threads=1; fi
  build_threads=$(((host_threads + 1) / 2))
  if (( build_threads > 4 )); then build_threads=4; fi
  test_threads=$((host_threads / build_threads))
  if (( test_threads < 1 )); then test_threads=1; fi
  printf '%s %s\n' "$build_threads" "$test_threads"
}
run_compiler_heavy_rust_tests() {
  local default_build_threads default_test_threads build_threads test_threads
  read -r default_build_threads default_test_threads < <(compiler_heavy_test_shape)
  build_threads=${TERLAN_BUILD_JOBS:-$default_build_threads}
  test_threads=${RUST_TEST_THREADS:-$default_test_threads}
  echo "pre-commit: compiler-heavy Rust shape: tests=$test_threads builds=$build_threads"
  RUST_TEST_THREADS=$test_threads TERLAN_BUILD_JOBS=$build_threads \
    bash scripts/run_exact_cargo_test.sh "$@"
}
run_commands_test_lane() {
  run_compiler_heavy_rust_tests --locked -p terlan --lib commands::test:: \
    && run_compiler_heavy_rust_tests --locked -p terlan --lib \
    tests::help_test::top_level_usage_hides_internal_scratch_commands -- --exact
}
run_summaries_lane() {
  TERLAN_SUMMARY_HOOK_TERLC="$CARGO_TARGET_DIR/debug/terlc" \
    bash scripts/git_hooks/stdlib_summaries.sh
}

vm_contract_modules=(
  BitString Bytes Cluster DistributedState DistributedStorage Fault ModelSync
  PersistentActor Process Scheduler Task Timeout
)
run_vm_contracts_lane() {
  local module elapsed started=$SECONDS
  local command=(test) coverage=()
  ensure_native_test_companions
  for module in "${vm_contract_modules[@]}"; do
    command+=("std/vm/${module}Test.terl")
    coverage+=(--coverage-source "std/vm/${module}.terl")
  done
  echo 'pre-commit: early VM source contracts'
  "$terlc" --cache-dir "$CARGO_TARGET_DIR/terlan-cache/vm-contracts" \
    "${command[@]}" --coverage "${coverage[@]}" --coverage-threshold 100
  elapsed=$((SECONDS - started))
  echo "pre-commit: VM source contracts finished in ${elapsed}s"
  if (( elapsed > 60 )); then
    echo 'pre-commit: red flag: VM source contracts exceeded 60s' >&2
  fi
}

core_contract_modules=(
  Atom Bool Effect Float Functional GuardResult Int Option Ordering Result Secret Unit
)
run_core_contracts_lane() {
  local module elapsed started=$SECONDS
  local command=(test) coverage=()
  for module in "${core_contract_modules[@]}"; do
    command+=("std/core/${module}Test.terl")
    coverage+=(--coverage-source "std/core/${module}.terl")
  done
  echo 'pre-commit: early core source contracts'
  "$terlc" --cache-dir "$CARGO_TARGET_DIR/terlan-cache/core-contracts" \
    "${command[@]}" --coverage "${coverage[@]}" --coverage-threshold 100
  # Keep this application separate from core declaration coverage: its
  # effectful cases legitimately specialize helpers owned by the test module.
  # Native lowering changes must still execute it before broad Cargo suites.
  "$terlc" --cache-dir "$CARGO_TARGET_DIR/terlan-cache/core-contracts" \
    test tests/language/ComprehensionGuardsTest.terl
  elapsed=$((SECONDS - started))
  echo "pre-commit: core source contracts finished in ${elapsed}s"
  if (( elapsed > 60 )); then
    echo 'pre-commit: red flag: core source contracts exceeded 60s' >&2
  fi
}

run_affected_rust_lane() {
  local lane=$1 filter
  affected_rust_lane_supported=true
  case "$lane" in
    commands-test) run_commands_test_lane ;;
    commands)
      ensure_native_test_companions
      run_compiler_heavy_rust_tests --locked -p terlan --lib commands::
      ;;
    commands-emit-js)
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib commands::emit_js::
      ;;
    commands-clean)
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib commands::clean::clean_test
      ;;
    commands-doctor)
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib commands::doctor::doctor_test
      ;;
    commands-init)
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib commands::init::init_test
      ;;
    phase)
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib \
        --features validation-union \
        tests::run_phase_contract_fixtures_match_golden -- --exact
      ;;
    native-ir)
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib compiler::native_ir::
      ;;
    runtime)
      ensure_native_test_companions
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib runtime::
      ;;
    syntax|compiler)
      filter=compiler::
      if [[ "$lane" == syntax ]]; then filter=compiler::syntax::; fi
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib "$filter"
      ;;
    workspace)
      run_workspace_suite
      ;;
    dependencies)
      echo 'pre-commit: Terlan library and binaries reuse candidate Clippy'
      cargo check --locked --workspace --tests --exclude terlan
      ;;
    native-planning) cargo test --profile dev --locked -p terlan-native-planning --lib ;;
    type-fuzz) cargo test --profile dev --locked -p terlan-type-fuzz --lib ;;
    clippy)
      if [[ "$candidate_clippy_checked" == true ]]; then
        echo 'pre-commit: reuse candidate workspace Clippy result'
      else
        make --no-print-directory rust-clippy-precommit-check
      fi
      ;;
    test-orchestrator) cargo test --profile dev --locked -p terlan-test-orchestrator ;;
    rust-boundary-audit) cargo test --profile dev --locked -p terlan-rust-boundary-audit ;;
    *) affected_rust_lane_supported=false; return 2 ;;
  esac
}

run_workspace_suite() {
  local status
  local report="$PWD/target/quality/rust-test-suite-report.json"
  local metadata="$PWD/target/quality/rust-cargo-metadata.json"
  local receipts="${TERLAN_RUST_PHASE_RECEIPT_DIR:-${CARGO_TARGET_DIR:-$PWD/target}/validation-results/rust-phases}"
  ensure_test_program_snapshots
  TERLC="$TERLAN_RUST_SUITE_TERLC" \
  TERLAN_VALIDATION_BOOTSTRAPPED=1 \
  TERLAN_RUST_ORCHESTRATOR="$orchestrator_path" \
  TERLAN_CARGO_METADATA_OWNER="$orchestrator_path" \
    "$orchestrator_path" --cargo-metadata "$metadata" -- cargo
  if TERLC="$TERLAN_RUST_SUITE_TERLC" \
    TERLAN_VALIDATION_BOOTSTRAPPED=1 \
    TERLAN_RUST_ORCHESTRATOR="$orchestrator_path" \
    TERLAN_CARGO_METADATA_OWNER="$orchestrator_path" \
    TERLAN_RUST_SUITE_REPORT="$report" \
    TERLAN_RUST_PHASE_RECEIPT_DIR="$receipts" \
      "$orchestrator_path"; then
    test -s "$report"
    return 0
  else
    status=$?
    echo "pre-commit: canonical Rust suite failed with status $status" >&2
    return "$status"
  fi
}

run_proof_checks() {
  ensure_quality_tools
  TERLC="$TERLAN_RUST_SUITE_TERLC" bash scripts/git_hooks/proofs.sh
}

run_proof_evidence() {
  ensure_quality_tools
  TERLC="$terlc" bash scripts/git_hooks/proof_evidence.sh
}

run_release_contract_lane() {
  ensure_quality_tools
  make --no-print-directory vm-release-install-validation-check \
    TERLAN_VALIDATION_BOOTSTRAPPED=1 \
    "TERLAN_QUALITY=$quality_path"
  "$terlc" test scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/ReleaseCleanupTest.terl
}

rust_lane_receipt_schema=terlan-affected-rust-lane-v1
rust_lane_receipt_dir="$CARGO_TARGET_DIR/validation-results/rust-lanes"
workspace_lane_selected=false
rust_lane_key() {
  local lane=$1
  {
    printf '%s\n%s\n' "$rust_lane_receipt_schema" "$lane"
    git ls-files -s -z -- . \
      ':(exclude)docs/quality/RUST_DEPENDENCY_IMPACT.json'
    printf '%s\n%s\n' \
      "CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-<default>}" \
      "CARGO_PROFILE_DEV_DEBUG=${CARGO_PROFILE_DEV_DEBUG:-<default>}"
    cargo -Vv
    rustc -Vv
  } | git hash-object --stdin
}
run_cached_affected_rust_lane() {
  local lane=$1 key receipt temporary status
  if [[ "$lane" == workspace ]]; then workspace_lane_selected=true; fi
  key=$(rust_lane_key "$lane")
  receipt="$rust_lane_receipt_dir/$lane-$key.passed"
  if [[ ${TERLAN_TEST_FORCE_RERUN:-0} != 1 \
    && -f "$receipt" \
    && $(wc -l < "$receipt") == 2 \
    && $(sed -n '1p' "$receipt") == "$rust_lane_receipt_schema" \
    && $(sed -n '2p' "$receipt") == "$key" ]]; then
    echo "pre-commit: reuse affected Rust lane $lane ($key)"
    return 0
  fi
  if run_affected_rust_lane "$lane"; then status=0; else status=$?; fi
  if (( status != 0 )); then return "$status"; fi
  test ! -L "$rust_lane_receipt_dir"
  mkdir -p "$rust_lane_receipt_dir"
  temporary=$(mktemp "$rust_lane_receipt_dir/.rust-lane.XXXXXX")
  printf '%s\n%s\n' "$rust_lane_receipt_schema" "$key" > "$temporary"
  mv "$temporary" "$receipt"
}

# Full-repository Rust inventories are release evidence, not commit admission.
# They scan the entire compiler even when one source changes and previously
# delayed actual affected tests by minutes. Publication owns their canonical
# regeneration; pre-commit reaches changed-file checks and executable tests
# without repeating that repository-wide work.
if lane_has rust-reports; then
  echo 'pre-commit: defer full Rust inventories to release validation'
fi

# Native representation changes are cross-cutting: validate all bounded VM
# source contracts before broad Cargo lanes, then mark them as consumed so
# directly changed stdlib modules are not executed again by terlan.sh.
if lane_has vm-contracts; then
  run_vm_contracts_lane
  for module in "${vm_contract_modules[@]}"; do
    printf 'std/vm/%s.terl\nstd/vm/%sTest.terl\n' "$module" "$module" \
      >> "$prechecked_terlan"
  done
  remaining_plan="${plan%.plan}.without-vm-contracts.plan"
  grep -Fvx 'vm-contracts' "$plan" > "$remaining_plan" || true
  mv "$remaining_plan" "$plan"
fi
if lane_has core-contracts; then
  run_core_contracts_lane
  for module in "${core_contract_modules[@]}"; do
    printf 'std/core/%s.terl\nstd/core/%sTest.terl\n' "$module" "$module" \
      >> "$prechecked_terlan"
  done
  printf '%s\n' tests/language/ComprehensionGuardsTest.terl >> "$prechecked_terlan"
  remaining_plan="${plan%.plan}.without-core-contracts.plan"
  grep -Fvx 'core-contracts' "$plan" > "$remaining_plan" || true
  mv "$remaining_plan" "$plan"
fi

typed_css_checked=false
if lane_has typed-css; then
  echo 'pre-commit: typed CSS generation, types, native values, and browser build'
  npm ci --ignore-scripts --prefix tools/css
  npm ci --ignore-scripts --prefix tools/web-toolchain
  TERLC="$terlc" bash scripts/check_typed_css.sh
  typed_css_checked=true
  remaining_plan="${plan%.plan}.without-typed-css.plan"
  grep -Fvx 'typed-css' "$plan" > "$remaining_plan" || true
  mv "$remaining_plan" "$plan"
fi
# Generated Rust reports and source-structure policy fail before any broad
# Terlan compilation. Directly affected compiler and source ownership follows.
if lane_has terlan; then
  echo 'pre-commit: early terlan ownership and coverage'
  # Terlan tests may cross a capability boundary even when no Rust test lane
  # is selected.  Provision the worker at the test boundary so a clean CI
  # checkout behaves exactly like a developer checkout with cached binaries.
  ensure_native_test_companions
  TERLAN_HOOK_TERLAN_CACHE_DIR="$CARGO_TARGET_DIR/terlan-cache" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="${TERLAN_HOOK_VALIDATION_RECEIPT_DIR:-$CARGO_TARGET_DIR/validation-results/terlan}" \
    TERLAN_HOOK_COMPILER_INPUT_KEY="$compiler_validation_identity" \
    bash scripts/git_hooks/terlan.sh \
    "$changes" "$base" "$terlc" "$typed_css_checked" "$prechecked_terlan" \
    "$specialized_tests"
fi
if lane_has summaries; then
  echo 'pre-commit: early summary drift'
  run_summaries_lane
fi
# Static shell and Clippy diagnostics are cheap candidate contracts. Run them
# before broad Rust suites so deterministic source failures cannot invalidate
# an expensive test result. Proof validation still follows its runtime owner.
if lane_has shell; then
  echo 'pre-commit: early shell syntax'
  while IFS= read -r path; do
    if [[ -f "$path" && ( "$path" == *.sh || "$path" == .githooks/* ) ]]; then bash -n "$path"; fi
  done < "$changes"
  remaining_plan="${plan%.plan}.without-shell.plan"
  grep -Fvx 'shell' "$plan" > "$remaining_plan" || true
  mv "$remaining_plan" "$plan"
fi
if lane_has release-contract; then
  echo 'pre-commit: early release and install contract'
  run_release_contract_lane
  remaining_plan="${plan%.plan}.without-release-contract.plan"
  grep -Fvx 'release-contract' "$plan" > "$remaining_plan" || true
  mv "$remaining_plan" "$plan"
fi
if preflight_has artifact-bootstrap; then
  echo 'pre-commit: early artifact bootstrap ownership'
  bash scripts/build_artifact_bootstrap_test.sh
fi

# Focused Terlan and generated-contract checks above are cheaper and produce
# more actionable diagnostics than broad Cargo suites. Only after they pass,
# execute each selected Rust lane and remove it from the later broad plan.
remaining_plan="${plan%.plan}.remaining-plan"
: > "$remaining_plan"
while IFS= read -r lane; do
  affected_rust_lane_supported=true
  if run_cached_affected_rust_lane "$lane"; then
    echo "pre-commit: early affected Rust lane passed: $lane"
  else
    lane_status=$?
    if [[ "$affected_rust_lane_supported" == false ]]; then
      printf '%s\n' "$lane" >> "$remaining_plan"
    else
      echo "pre-commit: early affected Rust lane failed: $lane" >&2
      exit "$lane_status"
    fi
  fi
done < "$plan"
mv "$remaining_plan" "$plan"
if [[ "$workspace_lane_selected" == true ]] && lane_has native-bindings; then
  echo 'pre-commit: canonical Rust suite owns native binding contracts'
  remaining_plan="${plan%.plan}.without-native-bindings.plan"
  grep -Fvx 'native-bindings' "$plan" > "$remaining_plan" || true
  mv "$remaining_plan" "$plan"
fi
if [[ "$workspace_lane_selected" == true ]] && lane_has proofs; then
  echo 'pre-commit: proof evidence and checks follow canonical Rust suite'
  run_proof_evidence
  run_proof_checks
  remaining_plan="${plan%.plan}.without-proofs.plan"
  grep -Fvx 'proofs' "$plan" > "$remaining_plan" || true
  mv "$remaining_plan" "$plan"
fi

if preflight_has version-metadata; then
  echo 'pre-commit: release version metadata'
  version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
  test -n "$version"
  cargo metadata --locked --no-deps --format-version 1 >/dev/null
  TERLAN_RELEASE_VERSION="$version" TERLAN_RELEASE_CHANNEL=dev \
    "$terlc" test scripts/self_validation/ReleaseVersionChannelTest.terl
fi
if preflight_has workflow-contract; then
  echo 'pre-commit: workflow contract'
  workflow_contract_cache="$CARGO_TARGET_DIR/terlan-validators/tvm-aot-platform-matrix"
  "$terlc" test \
    scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/WorkflowContractTest.terl \
    --out-dir "$workflow_contract_cache/test-build" \
    --cache-dir "$workflow_contract_cache/test-cache" \
    --coverage --coverage-threshold 100
  printf '%s\n' \
    scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/Core.terl \
    scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/MulticoreRelease.terl \
    scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/WorkflowContractTest.terl \
    >> "$prechecked_terlan"
fi
if preflight_has rust-security; then
  if [[ "$hosted_dependency_audit" == 1 ]]; then
    echo 'pre-commit: Rust dependency security audit owned by dedicated hosted job'
  else
    echo 'pre-commit: Rust dependency security audit'
    make --no-print-directory rust-security-audit-check
  fi
fi
if preflight_has node-security; then
  if [[ "$hosted_dependency_audit" == 1 ]]; then
    echo 'pre-commit: locked Node dependency security audit owned by dedicated hosted job'
  else
    echo 'pre-commit: locked Node dependency security audit'
    make --no-print-directory node-security-audit-check
  fi
fi
if preflight_has editor-release; then
  echo 'pre-commit: editor release contracts'
  make --no-print-directory editor-release-contract-check
fi
if preflight_has js-bindings; then
  echo 'pre-commit: generated JavaScript bindings'
  TERLAN_JS_BINDINGS_ROOT="$PWD" TERLAN_JS_BINDINGS_TERLC="$terlc" \
    "$terlc" test scripts/self_validation/JsBindingsDriftTest.terl
fi
if preflight_has negative-api; then
  echo 'pre-commit: standard-library negative API contracts'
  for contract in negative-api io-negative-api; do
    TERLAN_REPOSITORY_ROOT="$PWD" TERLAN_STDLIB_VALIDATION_TERLC="$terlc" \
      "$terlc" run scripts/self_validation/StdlibValidation.terls -- "$contract"
  done
fi
if lane_has native-bindings; then
  echo 'pre-commit: native binding generation, drift, and compiled protocols'
  make --no-print-directory native-binding-consistency-check
fi
if preflight_has artifact-budget; then
  echo 'pre-commit: canonical executable size budget'
  ensure_release_tools
  cargo build --locked -p terlan --bin terlan-native-worker
  cargo build --release --locked -p terlan-test-orchestrator --bin terlan-test-orchestrator
  ensure_orchestrator
  TERLAN_REPOSITORY_ROOT="$PWD" "$terlc" run scripts/self_validation/BuildArtifactBudgetTest.terl \
    -- --check-executables "$CARGO_TARGET_DIR"
fi
if preflight_has release-manifest; then
  echo 'pre-commit: standard-library release manifest'
  # This gate validates source contracts and JavaScript emission. The
  # already-selected compatible compiler is sufficient; rebuilding optimized
  # release tools here made validator-only commits pay the full LTO cost.
  "$terlc" build tests/js/fixtures/JsConstructorTypesTest.terl --target js.browser \
    --out-dir target/git-hooks-js-constructor-types
  TERLAN_RELEASE_MANIFEST_ROOT="$PWD" TERLAN_RELEASE_MANIFEST_TERLC="$terlc" \
    "$terlc" test scripts/self_validation/release_manifest
fi
if preflight_has rust-docs; then
  echo 'pre-commit: Rust documentation'
  ensure_quality_tools
  "$quality_path" rust-docs
fi
if preflight_has preparation; then
  echo 'pre-commit: preparation owner Cargo resource controls'
  "$terlc" test scripts/self_validation/release_promotion/src/release_promotion/PreparationEnvironmentTest.terl
fi
if preflight_has hook-policy; then
  echo 'pre-commit: hook policy and snapshot regression tests'
  "$terlc" test scripts/git_hooks/src/git_hooks/PlanTest.terl scripts/ci \
    --coverage --coverage-threshold 100
  hook_policy_status=0
  "$terlc" run scripts/ci/scripts/CheckRustFormatting.terls -- \
    --revisions HEAD HEAD false &
  formatting_pid=$!
  bash scripts/git_hooks_test.sh &
  hook_regression_pid=$!
  bash scripts/git_commit_message_test.sh &
  commit_message_pid=$!
  bash scripts/publication_process_preflight_test.sh &
  publication_preflight_pid=$!
  node scripts/git_hooks/cache_test.cjs &
  cache_test_pid=$!
  node --test scripts/git_hooks/filter_version_changes_test.cjs &
  version_filter_pid=$!
  TERLC="$terlc" bash scripts/git_hooks_formatting_test.sh &
  formatting_test_pid=$!
  wait "$formatting_pid" || hook_policy_status=1
  wait "$hook_regression_pid" || hook_policy_status=1
  wait "$commit_message_pid" || hook_policy_status=1
  wait "$publication_preflight_pid" || hook_policy_status=1
  wait "$cache_test_pid" || hook_policy_status=1
  wait "$version_filter_pid" || hook_policy_status=1
  wait "$formatting_test_pid" || hook_policy_status=1
  [[ "$hook_policy_status" == 0 ]]
  printf '%s\n' \
    scripts/git_hooks/src/git_hooks/Plan.terl \
    scripts/git_hooks/src/git_hooks/PlanTest.terl \
    scripts/ci/src/ci/ChangeClassifier.terl \
    scripts/ci/src/ci/ChangeClassifierTest.terl \
    scripts/ci/src/ci/HostedFormattingContractTest.terl \
    scripts/ci/src/ci/RustFormatting.terl \
    scripts/ci/src/ci/RustFormattingTest.terl \
    scripts/ci/scripts/CheckRustFormatting.terls \
    >> "$prechecked_terlan"
fi
while IFS= read -r lane; do
  echo "pre-commit: $lane"
  case "$lane" in
    native-bindings)
      # Already checked once before release-profile consumers.
      ;;
    proofs)
      if [[ "$workspace_lane_selected" == false ]]; then
        echo 'pre-commit: canonical Rust suite required by proof runtime oracles'
        if run_workspace_suite; then
          workspace_lane_selected=true
        else
          lane_status=$?
          echo 'pre-commit: stopping after canonical Rust suite failure' >&2
          exit "$lane_status"
        fi
      fi
      run_proof_evidence
      run_proof_checks
      ;;
    bootstrap)
      if [[ $(uname -s) == Linux ]]; then
        cargo test --profile dev --locked -p terlan-build-cache --test support_bootstrap_make
        cargo test --profile dev --locked -p terlan-test-orchestrator --test hosted_release_make
        cargo test --profile dev --locked -p terlan-test-orchestrator --test publish_preflight
      else
        echo 'pre-commit: Linux bootstrap integration tests require a Linux host'
      fi
      ;;
    rust-reports)
      # Executed before expensive proof and workspace gates.
      ;;
    summaries)
      # Executed before expensive proof and workspace gates.
      ;;
    shell)
      while IFS= read -r path; do
        if [[ -f "$path" && ( "$path" == *.sh || "$path" == .githooks/* ) ]]; then bash -n "$path"; fi
      done < "$changes"
      ;;
    clippy|core-contracts|phase|commands|commands-emit-js|commands-test|commands-clean|commands-doctor|commands-init|rust-boundary-audit|native-ir|runtime|syntax|compiler|workspace|dependencies|native-planning|type-fuzz|test-orchestrator)
      echo "pre-commit: duplicate affected Rust lane remained scheduled: $lane" >&2
      exit 1
      ;;
    editor)
      (cd tree-sitter-terlan && npm ci && npm run check && npm run check:cli)
      ;;
    terlan)
      # Executed before expensive proof and workspace gates.
      ;;
    workflow)
      scripts/actionlint.sh -color=false .github/workflows/*.yml
      ;;
    release-contract)
      echo 'pre-commit: release contract escaped early candidate validation' >&2
      exit 1
      ;;
    '') ;;
    *) echo "Unknown pre-commit lane: $lane" >&2; exit 1 ;;
  esac
done < "$plan"

if [[ "$workspace_lane_selected" == true ]]; then
  echo 'pre-commit: terminal Cargo artifact retention'
  make --no-print-directory rust-artifact-retention-check
fi
