#!/usr/bin/env bash
# Execute the Terlan-owned plan inside the staged-source validation clone.
set -euo pipefail
changes=$1
base=$2
hook_mode=${TERLAN_HOOK_MODE:-fast}
case "$hook_mode" in fast|full) ;; *) echo "unsupported hook mode: $hook_mode" >&2; exit 2 ;; esac
hosted_source_preflight=${TERLAN_HOSTED_SOURCE_PREFLIGHT:-0}
case "$hosted_source_preflight" in 0|1) ;; *) echo "unsupported hosted source-preflight mode: $hosted_source_preflight" >&2; exit 2 ;; esac
target_terlc="$CARGO_TARGET_DIR/debug/terlc"
terlc="$target_terlc"
export TERLC="$terlc"
raw_plan="${changes%.txt}.raw-plan"
preflight_plan="${changes%.txt}.preflight-plan"
plan="${changes%.txt}.plan"
specialized_tests="${changes%.txt}.specialized-tests.txt"
prechecked_terlan="${changes%.txt}.prechecked-terlan.txt"
compiler_input_key="$({
  printf '%s\n' terlan-staged-compiler-input-v2
  git ls-files -s -z -- \
    Cargo.toml Cargo.lock rust-toolchain.toml .cargo \
    ':(glob)**/Cargo.toml' ':(glob)**/*.rs' ':(glob)**/*.lalrpop' \
    ':(exclude)crates/terlan/src/compiler/syntax/terlan_lalrpop.lalrpop'
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
  if [[ "$hosted_source_preflight" == 0 ]] \
    && ! rg -qx 'preflight:inventory' "$raw_plan"; then
    echo 'pre-commit: local validation plan omitted source inventory' >&2
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
  if rg -qx 'preflight:compiler-build' "$raw_plan"; then compiler_build=true; fi
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
  invalidate_workspace_cache() {
    local result=$?
    trap - EXIT
    node scripts/git_hooks/invalidate_workspace_cache.cjs "$cache_metadata" "$CARGO_TARGET_DIR" || result=1
    rm -f "$cache_metadata"
    exit "$result"
  }
  trap invalidate_workspace_cache EXIT
  node scripts/git_hooks/invalidate_workspace_cache.cjs "$cache_metadata" "$CARGO_TARGET_DIR"
  echo 'pre-commit: build the changed staged compiler'
  # Use the quality superset once. Building without it and the report binary
  # with it would alternate two `terlan` feature graphs.
  cargo build --locked -p terlan --bin terlc --bin terlan-vm --features quality-tools
  terlc="$target_terlc"
  export TERLC="$terlc"
  run_plan "$terlc"
  temporary_compiler_receipt=$(mktemp "$CARGO_TARGET_DIR/.terlan-compiler-input.XXXXXX")
  printf '%s\n' "$compiler_input_key" > "$temporary_compiler_receipt"
  mv "$temporary_compiler_receipt" "$compiler_input_receipt"
else
  echo 'pre-commit: reuse compiler; no compiler inputs changed'
fi
sed -n 's/^preflight://p' "$raw_plan" > "$preflight_plan"
sed -n 's/^specialized-test://p' "$raw_plan" > "$specialized_tests"
rg -v '^(preflight:|specialized-test:)' "$raw_plan" > "$plan"
: > "$prechecked_terlan"
preflight_has() { rg -qx "$1" "$preflight_plan"; }
lane_has() { rg -qx "$1" "$plan"; }

if [[ "$hook_mode" == fast ]]; then
  admission_started=$SECONDS
  admission_status=0
  fast_jobs=${TERLAN_HOOK_FAST_JOBS:-8}
  case "$fast_jobs" in ''|*[!0-9]*|0) echo 'Invalid fast admission job count' >&2; exit 2 ;; esac
  fast_output_dir=$(mktemp -d "$CARGO_TARGET_DIR/fast-admission.XXXXXX")
  fast_receipt_dir=${TERLAN_HOOK_VALIDATION_RECEIPT_DIR:-$CARGO_TARGET_DIR/validation-results/fast-admission}
  test ! -L "$fast_receipt_dir"
  mkdir -p "$fast_receipt_dir"
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
      --out-dir "$fast_output_dir/rust-format-build" \
      --cache-dir "$fast_output_dir/rust-format-cache" \
      -- --paths-file "$changes"
  }
  fast_grammar() {
    local status=0
    TERLAN_EBNF_ROOT="$PWD" TERLAN_EBNF_COMPILER="$terlc" \
      "$terlc" run scripts/self_validation/EbnfValidator.terls \
        --out-dir "$fast_output_dir/grammar-build" \
        --cache-dir "$fast_output_dir/grammar-cache" || status=1
    TERLAN_SYNTAX_ARTIFACT_ROOT="$PWD" TERLAN_SYNTAX_ARTIFACT_COMPILER="$terlc" \
      "$terlc" run scripts/ci/scripts/SyncSyntaxContractArtifacts.terls \
        --out-dir "$fast_output_dir/syntax-artifacts-build" \
        --cache-dir "$fast_output_dir/syntax-artifacts-cache" \
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
  fast_inventory() {
    "$terlc" test scripts/self_validation/InventoryTest.terl \
      --out-dir "$fast_output_dir/inventory-build" \
      --cache-dir "$fast_output_dir/inventory-cache"
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
        inventory)
          git ls-files -z | git hash-object --stdin
          git ls-files -s -- scripts/self_validation/InventoryTest.terl
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
  if preflight_has inventory; then
    start_fast_gate inventory fast_inventory
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
  echo 'pre-commit: early Terlan readability canonicalization'
  "$terlc" lint --only TL0009 --only TL0010 "${terlan_paths[@]}"
  echo 'pre-commit: early Terlan formatting'
  "$terlc" fmt --check "${terlan_paths[@]}"
fi

release_tools_ready=false
quality_tools_ready=false
orchestrator_ready=false
native_test_companions_ready=false
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
    cargo build --locked -p terlan --bin terlan-quality --features quality-tools
    quality_tools_ready=true
  fi
}
ensure_orchestrator() {
  if [[ "$orchestrator_ready" == false ]]; then
    cargo build --locked -p terlan-test-orchestrator
    orchestrator_ready=true
  fi
}
ensure_native_test_companions() {
  if [[ "$native_test_companions_ready" == false ]]; then
    echo 'pre-commit: build required native test companions once'
    cargo build --locked -p terlan --bin terlan-native-worker
    native_test_companions_ready=true
  fi
  export TERLAN_NATIVE_WORKER="$CARGO_TARGET_DIR/debug/terlan-native-worker"
}
run_commands_test_lane() {
  bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib commands::test:: \
    && bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib \
    tests::help_test::top_level_usage_hides_internal_scratch_commands -- --exact
}
run_rust_reports_lane() {
  ensure_quality_tools
  ensure_orchestrator
  TERLAN_HOOK_ARTIFACTS_PREBUILT=1 \
  TERLAN_RUST_REPORT_TERLC="$CARGO_TARGET_DIR/debug/terlc" \
  TERLAN_RUST_REPORT_VM="$CARGO_TARGET_DIR/debug/terlan-vm" \
  TERLAN_RUST_REPORT_CACHE_DIR="$CARGO_TARGET_DIR/validation-results/rust-reports" \
    bash scripts/git_hooks/rust_reports.sh
}
run_summaries_lane() {
  TERLAN_SUMMARY_HOOK_TERLC="$CARGO_TARGET_DIR/debug/terlc" \
    bash scripts/git_hooks/stdlib_summaries.sh
}

run_affected_rust_lane() {
  local lane=$1 filter
  case "$lane" in
    commands-test) run_commands_test_lane ;;
    commands)
      ensure_native_test_companions
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib commands::
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
        tests::run_phase_contract_fixtures_match_golden -- --exact
      ;;
    native-ir)
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib compiler::native_ir::
      ;;
    syntax|compiler)
      filter=compiler::
      if [[ "$lane" == syntax ]]; then filter=compiler::syntax::; fi
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib "$filter"
      ;;
    workspace)
      ensure_native_test_companions
      bash scripts/run_exact_cargo_test.sh --locked --workspace --all-targets
      ;;
    dependencies) cargo check --locked --workspace --all-targets ;;
    test-orchestrator) cargo test --locked -p terlan-test-orchestrator ;;
    rust-boundary-audit) cargo test --locked -p terlan-rust-boundary-audit ;;
    *) return 2 ;;
  esac
}

# Derived Rust inventories are cheap structural inputs compared with affected
# test suites. Reject their drift before compiling or executing those suites so
# a stale generated report cannot invalidate a long compiler validation run.
if lane_has rust-reports; then
  echo 'pre-commit: early Rust report drift'
  run_rust_reports_lane
fi

typed_css_checked=false
if rg -q '^(std/css/|tools/css/|tools/web-toolchain/css-bundle|scripts/check_typed_css\.sh)' "$changes"; then
  echo 'pre-commit: typed CSS generation, types, native values, and browser build'
  npm ci --ignore-scripts --prefix tools/css
  npm ci --ignore-scripts --prefix tools/web-toolchain
  TERLC="$terlc" bash scripts/check_typed_css.sh
  typed_css_checked=true
fi
# Generated Rust reports and source-structure policy fail before any broad
# Terlan compilation. Directly affected compiler and source ownership follows.
if lane_has terlan; then
  echo 'pre-commit: early terlan ownership and coverage'
  TERLAN_HOOK_TERLAN_CACHE_DIR="$CARGO_TARGET_DIR/terlan-cache" \
    bash scripts/git_hooks/terlan.sh \
    "$changes" "$base" "$terlc" "$typed_css_checked" "$prechecked_terlan" \
    "$specialized_tests"
fi
if lane_has summaries; then
  echo 'pre-commit: early summary drift'
  run_summaries_lane
fi

# Focused Terlan and generated-contract checks above are cheaper and produce
# more actionable diagnostics than broad Cargo suites. Only after they pass,
# execute each selected Rust lane and remove it from the later broad plan.
remaining_plan="${plan%.plan}.remaining-plan"
: > "$remaining_plan"
while IFS= read -r lane; do
  if run_affected_rust_lane "$lane"; then
    echo "pre-commit: early affected Rust lane passed: $lane"
  else
    lane_status=$?
    if (( lane_status == 2 )); then
      printf '%s\n' "$lane" >> "$remaining_plan"
    else
      echo "pre-commit: early affected Rust lane failed: $lane" >&2
      exit "$lane_status"
    fi
  fi
done < "$plan"
mv "$remaining_plan" "$plan"

if preflight_has inventory; then
  echo 'pre-commit: retained source inventory'
  "$terlc" test scripts/self_validation/InventoryTest.terl
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
  echo 'pre-commit: workflow and compiled-cache contracts'
  "$terlc" run scripts/self_validation/tvm_aot_platform_matrix/scripts/TvmAotPlatformMatrix.terls \
    -- self-test
  "$terlc" test \
    scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/WorkflowContractTest.terl \
    --coverage --coverage-threshold 100
  printf '%s\n' \
    scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/Core.terl \
    scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/MulticoreRelease.terl \
    scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/WorkflowContractTest.terl \
    >> "$prechecked_terlan"
fi
if preflight_has rust-security; then
  echo 'pre-commit: Rust dependency security audit'
  make --no-print-directory rust-security-audit-check
fi
if preflight_has node-security; then
  echo 'pre-commit: locked Node dependency security audit'
  make --no-print-directory node-security-audit-check
fi
if preflight_has editor-release; then
  echo 'pre-commit: editor release contracts'
  make --no-print-directory editor-release-contract-check
fi
if preflight_has artifact-bootstrap; then
  echo 'pre-commit: artifact bootstrap ownership'
  bash scripts/build_artifact_bootstrap_test.sh
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
  ensure_release_tools
  "$terlc" build tests/js/fixtures/JsConstructorTypesTest.terl --target js.browser \
    --out-dir target/git-hooks-js-constructor-types
  TERLAN_RELEASE_MANIFEST_ROOT="$PWD" TERLAN_RELEASE_MANIFEST_TERLC="$terlc" \
    "$terlc" test scripts/self_validation/release_manifest
fi
if lane_has proofs; then
  echo 'pre-commit: proof release baseline'
  ensure_release_tools
  ensure_quality_tools
  bash scripts/git_hooks/proof_baseline.sh
fi
if preflight_has rust-docs; then
  echo 'pre-commit: Rust documentation'
  ensure_quality_tools
  "$CARGO_TARGET_DIR/debug/terlan-quality" rust-docs
fi
if preflight_has preparation; then
  echo 'pre-commit: preparation owner Cargo resource controls'
  "$terlc" test scripts/self_validation/release_promotion/src/release_promotion/PreparationEnvironmentTest.terl
fi
if preflight_has hook-policy; then
  echo 'pre-commit: hook policy and snapshot regression tests'
  "$terlc" test scripts/git_hooks/src/git_hooks/PlanTest.terl
  "$terlc" test scripts/ci --coverage --coverage-threshold 100
  "$terlc" run scripts/ci/scripts/CheckRustFormatting.terls -- \
    --revisions HEAD HEAD false
  bash scripts/git_hooks_test.sh
  bash scripts/git_commit_message_test.sh
  bash scripts/publication_process_preflight_test.sh
  node scripts/git_hooks/cache_test.cjs
  node --test scripts/git_hooks/filter_version_changes_test.cjs
  TERLC="$terlc" bash scripts/git_hooks_formatting_test.sh
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
      ensure_quality_tools
      bash scripts/git_hooks/proofs.sh
      ;;
    bootstrap)
      if [[ $(uname -s) == Linux ]]; then
        cargo test --locked -p terlan-build-cache --test support_bootstrap_make
        cargo test --locked -p terlan-test-orchestrator --test hosted_release_make
        cargo test --locked -p terlan-test-orchestrator --test publish_preflight
      else
        echo 'pre-commit: Linux bootstrap integration tests require a Linux host'
      fi
      ;;
    clippy)
      make --no-print-directory rust-clippy-check
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
    phase|commands|commands-emit-js|commands-test|commands-clean|commands-doctor|commands-init|rust-boundary-audit|native-ir|syntax|compiler|workspace|dependencies|test-orchestrator)
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
      go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 -color=false .github/workflows/*.yml
      ;;
    release-contract)
      ensure_quality_tools
      ensure_orchestrator
      "$CARGO_TARGET_DIR/debug/terlan-test-orchestrator" --install-snapshot "$PWD/target/validation-tools/terlan-test-orchestrator"
      repository_validator_dir="$CARGO_TARGET_DIR/terlan-validators/repository-validation"
      repository_validator_common="$repository_validator_dir/common.inputs.sha256"
      repository_validator_image="$repository_validator_dir/vm/scripts_self_validation_RepositoryValidation.tvm"
      repository_validator_identity=$("$terlc" build --print-toolchain-identity)
      TERLAN_TYPED_VALIDATOR_TOOLCHAIN_IDENTITY="$repository_validator_identity" \
        bash scripts/build_typed_validator.sh fingerprint \
          "$repository_validator_common" "$terlc" std
      bash scripts/build_typed_validator.sh \
        "$repository_validator_image" \
        "$repository_validator_common" scripts/self_validation/RepositoryValidation.terls -- \
        "$terlc" build --incremental \
          scripts/self_validation/RepositoryValidation.terls \
          --target terlan-vm --out-dir "$repository_validator_dir"
      make --no-print-directory vm-release-install-validation-check \
        TERLAN_VALIDATION_BOOTSTRAPPED=1 \
        "TERLAN_QUALITY=$CARGO_TARGET_DIR/debug/terlan-quality"
      make --no-print-directory repository-build-release-contract-check \
        TERLAN_VALIDATION_BOOTSTRAPPED=1 \
        "TERLAN_REPOSITORY_VALIDATION=$CARGO_TARGET_DIR/debug/terlan-vm run $repository_validator_image --script-eval --"
      printf '%s\n' scripts/self_validation/RepositoryValidation.terls >> "$prechecked_terlan"
      "$terlc" test scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/ReleaseCleanupTest.terl
      ;;
    '') ;;
    *) echo "Unknown pre-commit lane: $lane" >&2; exit 1 ;;
  esac
done < "$plan"
