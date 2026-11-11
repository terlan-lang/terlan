#!/usr/bin/env bash
# Exercise the Git boundary with tiny repositories and a staged fixture checker.
set -euo pipefail
unset TERLAN_HOOK_SHARED_TARGET TERLAN_HOOK_TERLAN_CACHE_DIR
unset TERLAN_HOOK_VALIDATION_RECEIPT_DIR TERLAN_TEST_FORCE_RERUN
export TERLAN_HOOK_MODE=full
driver="$(cd "$(dirname "$0")" && pwd)/git_hooks.sh"
version_filter="$(cd "$(dirname "$0")" && pwd)/git_hooks/filter_version_changes.cjs"
mkdir -p target
temporary=$(mktemp -d "$(pwd)/target/git-hooks-test.XXXXXX")
trap 'result=$?; if (( result != 0 )); then cat "$temporary"/*.log >&2; fi; rm -rf "$temporary"' EXIT
while IFS= read -r variable; do unset "$variable"; done < <(git rev-parse --local-env-vars)
git init --quiet --initial-branch=main "$temporary/unborn"
(
  cd "$temporary/unborn"
  mkdir -p scripts/git_hooks
  cp "$version_filter" scripts/git_hooks/
  printf 'test -f new.txt\n' > scripts/git_hooks/check.sh
  touch new.txt
  git add .
  bash "$driver" check > "$temporary/unborn.log" 2>&1
)
git init --quiet --initial-branch=main "$temporary/repo"
cd "$temporary/repo"
git config user.name 'Hook regression test'
git config user.email 'hooks@example.invalid'
git config core.hooksPath /dev/null
mkdir -p scripts/git_hooks
cp "$version_filter" scripts/git_hooks/
cat > scripts/git_hooks/check.sh <<'CHECK'
set -eu
test "$(cat tracked.txt)" = good
test -z "${GIT_INDEX_FILE:-}"
test "$2" = "$(git -C "$(git remote get-url origin)" rev-parse HEAD)"
test -L target/debug
test "$(readlink target/debug)" = "$CARGO_TARGET_DIR/debug"
if test -n "${EXPECTED_HOOK_CARGO_HOME:-}"; then
  test "$CARGO_HOME" = "$EXPECTED_HOOK_CARGO_HOME"
fi
test "$CARGO_INCREMENTAL" = "${EXPECTED_HOOK_CARGO_INCREMENTAL:-0}"
CHECK
printf 'initial\n' > tracked.txt
printf 'original\n' > marker.txt
git add .
git commit --quiet -m Initial

# Partially staged files: validate the good index, preserve the bad working copy.
printf 'good\n' > tracked.txt
git add tracked.txt
printf 'unstaged\n' > tracked.txt
printf 'untracked\n' > keep.txt
bash "$driver" check > "$temporary/pass.log" 2>&1
test "$(cat tracked.txt)" = unstaged
test "$(cat keep.txt)" = untracked
test "$(git show :tracked.txt)" = good

# A publication wait owns one exact main revision across every worktree. The
# hook must reject a concurrent commit before starting staged validation.
exec 7>>.git/terlan-publication.lock
flock --exclusive 7
if bash "$driver" check > "$temporary/publication-lock.log" 2>&1; then
  echo 'Hook ignored the active shared publication lease' >&2; exit 1
fi
grep -q 'publication owns main' "$temporary/publication-lock.log"
flock --unlock 7
exec 7>&-

# Default Cargo inputs are isolated; explicit relative overrides remain supported.
env -u CARGO_HOME EXPECTED_HOOK_CARGO_HOME="$PWD/target/git-hooks/cargo-home" \
  bash "$driver" check > "$temporary/cargo-default.log" 2>&1
CARGO_HOME=custom-cache EXPECTED_HOOK_CARGO_HOME="$PWD/custom-cache" \
  bash "$driver" check > "$temporary/cargo-explicit.log" 2>&1
env -u CARGO_INCREMENTAL EXPECTED_HOOK_CARGO_INCREMENTAL=0 \
  bash "$driver" check > "$temporary/incremental-default.log" 2>&1
CARGO_INCREMENTAL=1 EXPECTED_HOOK_CARGO_INCREMENTAL=1 \
  bash "$driver" check > "$temporary/incremental-explicit.log" 2>&1

# The default cache cannot redirect validation into an unrelated global cache.
mkdir "$temporary/unrelated-cargo"
ln -s "$temporary/unrelated-cargo" target/git-hooks/cargo-home
if env -u CARGO_HOME bash "$driver" check > "$temporary/cargo-link.log" 2>&1; then
  echo 'Hook accepted a redirected default Cargo cache' >&2; exit 1
fi
rm target/git-hooks/cargo-home

# A good working copy cannot hide a broken staged candidate.
printf 'bad\n' > tracked.txt
git add tracked.txt
printf 'good\n' > tracked.txt
if bash "$driver" check > "$temporary/fail.log" 2>&1; then
  echo 'Hook accepted broken staged content' >&2; exit 1
fi
test "$(git show :tracked.txt)" = bad
test "$(cat tracked.txt)" = good

# `git commit --only` uses an alternate index; it must not leak into the clone.
cp .git/index "$temporary/alternate-index"
GIT_INDEX_FILE="$temporary/alternate-index" git add tracked.txt
GIT_INDEX_FILE="$temporary/alternate-index" bash "$driver" check > "$temporary/alternate.log" 2>&1
test "$(git show :tracked.txt)" = bad
test "$(GIT_INDEX_FILE="$temporary/alternate-index" git show :tracked.txt)" = good

# Concurrent checks cannot reset a snapshot that is currently being validated.
mkdir -p target/git-hooks
lock_ready="$temporary/validation-lock.ready"
lock_wait="$temporary/validation-lock.wait"
mkfifo "$lock_wait"
(
  exec 9>target/git-hooks/validation.lock
  flock --exclusive 9
  touch "$lock_ready"
  read -r _ < "$lock_wait"
) &
lock_owner=$!
while [[ ! -f "$lock_ready" ]]; do sleep 0.01; done
if bash "$driver" check > "$temporary/locked.log" 2>&1; then
  echo 'Hook ignored the active snapshot lock' >&2; exit 1
fi
kill "$lock_owner"
wait "$lock_owner" 2>/dev/null || true
rm -f "$lock_ready" "$lock_wait"

# Kernel locks are released automatically when a killed owner exits.
GIT_INDEX_FILE="$temporary/alternate-index" \
  bash "$driver" check > "$temporary/recovered-lock.log" 2>&1

# Stage a checker that changes the real index while it runs: approval must fail.
cat > scripts/git_hooks/check.sh <<'CHECK'
set -eu
original=$(git remote get-url origin)
printf 'changed-during-validation\n' > "$original/tracked.txt"
git -C "$original" add tracked.txt
CHECK
git add tracked.txt scripts/git_hooks/check.sh
if bash "$driver" check > "$temporary/race.log" 2>&1; then
  echo 'Hook accepted an index changed during validation' >&2; exit 1
fi
grep -q 'index or HEAD changed' "$temporary/race.log"

# Deleted paths stay in the plan, and snapshot-side repairs cannot pass.
cat > scripts/git_hooks/check.sh <<'CHECK'
set -eu
grep -Fx tracked.txt "$1"
test ! -e tracked.txt
printf 'repair\n' >> marker.txt
CHECK
git rm --quiet --force tracked.txt
git add scripts/git_hooks/check.sh
if bash "$driver" check > "$temporary/repair.log" 2>&1; then
  echo 'Hook accepted a checker that repaired staged source' >&2; exit 1
fi
grep -q '^+repair' "$temporary/repair.log"

# Installation preserves custom hook configurations.
if bash "$driver" install > "$temporary/install.log" 2>&1; then
  echo 'Hook installer replaced custom hooks' >&2; exit 1
fi
test "$(git config --get core.hooksPath)" = /dev/null

# Grammar and generated binding drift must stop documentation-only commits
# before the optimized build, using the real checker with bounded stand-ins.
mkdir -p "$temporary/early-bindings/bin" "$temporary/early-bindings/cache/debug" \
  "$temporary/early-bindings/scripts"

# Default commit admission executes only bounded checks and emits the complete
# candidate plan without launching its expensive lanes.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
case "$*" in
  'run scripts/git_hooks/scripts/Plan.terls -- changes.txt')
    printf 'preflight:inventory\nshell\n\nworkspace\n'
    ;;
  test\ scripts/self_validation/InventoryTest.terl\ --out-dir\ *\ --cache-dir\ *) ;;
  *) exit 96 ;;
esac
TERLC
chmod +x "$temporary/early-bindings/cache/debug/terlc"
printf 'README.md\n' > "$temporary/early-bindings/changes.txt"
(
  cd "$temporary/early-bindings"
  TERLAN_HOOK_MODE=fast \
    TERLAN_HOOK_LATEST_TERLC="$PWD/cache/debug/terlc" \
    CARGO_TARGET_DIR="$PWD/cache" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/fast-admission.log" 2>&1
  grep -Eq '^admitted:inventory:[0-9a-f]+$' changes.candidate-plan
  grep -Eq '^admitted:shell:[0-9a-f]+$' changes.candidate-plan
  test "$(grep -Ec '^(preflight:)?shell$' changes.candidate-plan)" = 0
  grep -qx 'workspace' changes.candidate-plan
  test "$(grep -Ec '^$' changes.candidate-plan)" = 0
  grep -Eq 'pass inventory \([0-9]+s\)' "$temporary/fast-admission.log"
  grep -q 'candidate validation deferred' "$temporary/fast-admission.log"
)

cat > "$temporary/early-bindings/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
case "$*" in
  'build --locked -p terlan --bin terlc --bin terlan-vm --features quality-tools') exit 0 ;;
  *'--release'*) exit 99 ;;
  *) exit 97 ;;
esac
CARGO
printf '#!/usr/bin/env bash\nexit 0\n' > "$temporary/early-bindings/bin/make"
printf 'exit 0\n' > "$temporary/early-bindings/scripts/build_artifact_bootstrap_test.sh"
# Readability must check only changed Terlan files and fail before
# release-profile work. The release build sentinel makes ordering observable.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
if [[ "$*" == 'run scripts/git_hooks/scripts/Plan.terls -- changes.txt' ]]; then
  printf 'preflight:inventory\npreflight:release-manifest\npreflight:terlan-format\nshell\n'
  exit 0
fi
if [[ "$*" == 'run scripts/ci/scripts/CheckRustFormatting.terls -- --paths-file changes.txt' ]]; then
  exit 0
fi
if [[ "$*" == 'test scripts/self_validation/InventoryTest.terl' ]]; then exit 0; fi
if [[ "$*" == 'lint --only TL0009 --only TL0010 std/core/Int.terl' ]]; then exit 35; fi
exit 96
TERLC
chmod +x "$temporary/early-bindings/bin/"* "$temporary/early-bindings/cache/debug/terlc"
mkdir -p "$temporary/early-bindings/std/core"
printf 'module std.core.Int.\n' > "$temporary/early-bindings/std/core/Int.terl"
printf 'std/core/Int.terl\nREADME.md\n' > "$temporary/early-bindings/changes.txt"
(
  cd "$temporary/early-bindings"
  {
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
  } | git hash-object --stdin > cache/.terlan-compiler-input
)
(
  cd "$temporary/early-bindings"
  status=0
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/early-readability.log" 2>&1 || status=$?
  test "$status" = 35
)
printf '#!/usr/bin/env bash\nexit 0\n' > "$temporary/early-bindings/bin/make"
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
if [[ "$*" == 'run scripts/git_hooks/scripts/Plan.terls -- changes.txt' ]]; then
  printf 'preflight:inventory\npreflight:grammar\nshell\n'
  exit 0
fi
if [[ "$*" == 'test scripts/self_validation/InventoryTest.terl' ]]; then exit 0; fi
test "$TERLAN_EBNF_ROOT" = "$PWD"
test "$TERLAN_EBNF_COMPILER" = "$0"
test "$*" = 'run scripts/self_validation/EbnfValidator.terls'
exit 39
TERLC
chmod +x "$temporary/early-bindings/bin/"* "$temporary/early-bindings/cache/debug/terlc"
printf 'README.md\n' > "$temporary/early-bindings/changes.txt"
(
  cd "$temporary/early-bindings"
  status=0
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/early-grammar.log" 2>&1 || status=$?
  test "$status" = 39
)
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
if [[ "$*" == 'run scripts/git_hooks/scripts/Plan.terls -- changes.txt' ]]; then
  printf 'preflight:inventory\npreflight:grammar\npreflight:js-bindings\nshell\n'
  exit 0
fi
if [[ "$*" == 'test scripts/self_validation/InventoryTest.terl' ]]; then exit 0; fi
if [[ "$*" == 'run scripts/self_validation/EbnfValidator.terls' ]]; then exit 0; fi
if [[ "$*" == 'run scripts/ci/scripts/SyncSyntaxContractArtifacts.terls -- --check' ]]; then
  test "$TERLAN_SYNTAX_ARTIFACT_ROOT" = "$PWD"
  test "$TERLAN_SYNTAX_ARTIFACT_COMPILER" = "$0"
  exit 0
fi
test "$TERLAN_JS_BINDINGS_ROOT" = "$PWD"
test "$TERLAN_JS_BINDINGS_TERLC" = "$0"
test "$*" = 'test scripts/self_validation/JsBindingsDriftTest.terl'
exit 37
TERLC
chmod +x "$temporary/early-bindings/bin/"* "$temporary/early-bindings/cache/debug/terlc"
printf 'README.md\n' > "$temporary/early-bindings/changes.txt"
(
  cd "$temporary/early-bindings"
  status=0
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/early-bindings.log" 2>&1 || status=$?
  test "$status" = 37
)
# Both negative API contracts must run before the optimized compiler build.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
if [[ "$*" == 'run scripts/git_hooks/scripts/Plan.terls -- changes.txt' ]]; then
  printf 'preflight:inventory\npreflight:grammar\npreflight:js-bindings\npreflight:negative-api\nshell\n'
  exit 0
fi
if [[ "$*" == 'test scripts/self_validation/InventoryTest.terl' ]]; then exit 0; fi
if [[ "$*" == 'run scripts/self_validation/EbnfValidator.terls' ]]; then exit 0; fi
if [[ "$*" == 'run scripts/ci/scripts/SyncSyntaxContractArtifacts.terls -- --check' ]]; then
  test "$TERLAN_SYNTAX_ARTIFACT_ROOT" = "$PWD"
  test "$TERLAN_SYNTAX_ARTIFACT_COMPILER" = "$0"
  exit 0
fi
if [[ "$*" == 'test scripts/self_validation/JsBindingsDriftTest.terl' ]]; then
  exit 0
fi
test "$TERLAN_REPOSITORY_ROOT" = "$PWD"
test "$TERLAN_STDLIB_VALIDATION_TERLC" = "$0"
test "$1" = run
test "$2" = scripts/self_validation/StdlibValidation.terls
test "$3" = --
case "$4" in negative-api|io-negative-api) ;; *) exit 98 ;; esac
if [[ "$4" == "$HOOK_NEGATIVE_FAIL" ]]; then exit 38; fi
TERLC
for contract in negative-api io-negative-api; do
  (
    cd "$temporary/early-bindings"
    status=0
    PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" HOOK_NEGATIVE_FAIL="$contract" \
      bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
      > "$temporary/early-$contract.log" 2>&1 || status=$?
    test "$status" = 38
  )
done
# Retained-source inventory drift must stop before planning or optimized builds.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
case "$*" in
  'run scripts/git_hooks/scripts/Plan.terls -- changes.txt') printf 'preflight:inventory\nshell\n' ;;
  'test scripts/self_validation/InventoryTest.terl') exit 36 ;;
  *) exit 98 ;;
esac
TERLC
(
  cd "$temporary/early-bindings"
  status=0
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/early-python-inventory.log" 2>&1 || status=$?
  test "$status" = 36
)
# Native generation drift must stop before optimized builds. The real planner
# separately tests the changed-path classification; this checks execution order
# and failure propagation through the actual hook checker.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
case "$*" in
  'run scripts/git_hooks/scripts/Plan.terls -- changes.txt')
    printf 'preflight:inventory\nshell\n'
    if [[ "$HOOK_NATIVE_SELECTED" == yes ]]; then printf 'native-bindings\n'; fi
    ;;
  'test scripts/self_validation/InventoryTest.terl') exit 0 ;;
  *) exit 98 ;;
esac
TERLC
cat > "$temporary/early-bindings/bin/make" <<'MAKE'
#!/usr/bin/env bash
if [[ "$*" == '--no-print-directory native-binding-consistency-check' ]]; then
  printf '%s\n' "$*" >> "$HOOK_NATIVE_CALLS"
  exit "${HOOK_NATIVE_FAILURE:-0}"
fi
exit 0
MAKE
chmod +x "$temporary/early-bindings/cache/debug/terlc" "$temporary/early-bindings/bin/make"
for scenario in failure success unrelated; do
  (
    cd "$temporary/early-bindings"
    selected=yes
    failure=0
    expected=0 # Unselected optimized artifacts must not be built.
    if [[ "$scenario" == failure ]]; then failure=41; expected=41; fi
    if [[ "$scenario" == unrelated ]]; then selected=no; fi
    calls="$temporary/native-$scenario.calls"
    : > "$calls"
    status=0
    PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" \
      HOOK_NATIVE_SELECTED="$selected" HOOK_NATIVE_FAILURE="$failure" HOOK_NATIVE_CALLS="$calls" \
      bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
      > "$temporary/native-$scenario.log" 2>&1 || status=$?
    test "$status" = "$expected"
    if [[ "$selected" == yes ]]; then
      test "$(wc -l < "$calls")" = 1
    else
      test ! -s "$calls"
    fi
  )
done

# Release-facing source changes must execute the concrete installer validator
# before the broader repository contract. This would have caught stale getter
# spellings during the struct-field migration rather than at publication time.
release_install_line=$(rg -n -F 'make --no-print-directory vm-release-install-validation-check' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
repository_contract_line=$(rg -n -F 'make --no-print-directory repository-build-release-contract-check' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
test -n "$release_install_line"
test -n "$repository_contract_line"
test "$release_install_line" -lt "$repository_contract_line"
release_contract_body=$(sed -n '/^    release-contract)/,/^      ;;/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
bootstrap_body=$(sed -n '/^    bootstrap)/,/^      ;;/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
publication_test_pattern='cargo '
publication_test_pattern+='test --locked -p terlan-test-orchestrator --test publish_preflight'
grep -Fq "$publication_test_pattern" <<< "$bootstrap_body"
grep -Fq 'ensure_quality_tools' <<< "$release_contract_body"
if grep -Fq 'ensure_optimized_compiler' <<< "$release_contract_body" \
  || grep -Fq 'ensure_release_tools' <<< "$release_contract_body"; then
  echo 'Release contract rebuilt optimized compiler tools unnecessarily' >&2; exit 1
fi
grep -Fq 'TERLAN_VALIDATION_BOOTSTRAPPED=1' <<< "$release_contract_body"
grep -Fq 'TERLAN_QUALITY=$CARGO_TARGET_DIR/debug/terlan-quality' <<< "$release_contract_body"
grep -Fq 'prechecked_terlan' <<< "$release_contract_body"
grep -Fq 'build_typed_validator.sh' <<< "$release_contract_body"
grep -Fq 'repository_validator_image' <<< "$release_contract_body"
grep -Fq 'TERLAN_REPOSITORY_VALIDATION=$CARGO_TARGET_DIR/debug/terlan-vm run' \
  <<< "$release_contract_body"
if grep -Fq 'TERLAN_REPOSITORY_VALIDATION=\"$terlc\" run' <<< "$release_contract_body"; then
  echo 'Release contract recompiles the repository validator on every run' >&2; exit 1
fi
repository_contract_recipe=$(awk '
  /^repository-build-release-contract-check:/ { active=1 }
  active && /^release-boundary-check:/ { exit }
  active { print }
' "$(dirname "$driver")/../Makefile")
test "$(grep -Fc '$(TERLAN_REPOSITORY_VALIDATION)' <<< "$repository_contract_recipe")" = 1
if grep -Fq '$(TERLAN_REPOSITORY_VALIDATION) build-plan-report' <<< "$repository_contract_recipe"; then
  echo 'Repository build-plan report was validated in a second compiler process' >&2; exit 1
fi

# Formatting follows changed source inputs. A commit must not silently restore
# the former whole-workspace Rust style pass.
grep -Fq 'CheckRustFormatting.terls -- --paths-file "$changes"' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq '"$terlc" test scripts/ci --coverage --coverage-threshold 100' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'reuse compiler; no compiler inputs changed' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'repository_root=$(git remote get-url origin)' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'reuse compatible compiler $terlc' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq "rg -qx 'preflight:inventory'" \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'check tests/language/RepeatedLetSyntaxTest.terl' \
  "$(dirname "$driver")/git_hooks/check.sh"
test "$(grep -Fc 'run_plan "$terlc"' "$(dirname "$driver")/git_hooks/check.sh")" = 3
grep -Fq 'run_plan "$reusable_terlc"' "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'compiler/self_host/corpus/*' \
  "$(dirname "$driver")/git_hooks/terlan.sh"
grep -Fq 'terlc="$target_terlc"' \
  "$(dirname "$driver")/git_hooks/check.sh"
if grep -Fq 'cargo fmt --all' "$(dirname "$driver")/git_hooks/check.sh"; then
  echo 'Hook redundantly formats unchanged Rust sources' >&2; exit 1
fi
early_terlan_line=$(rg -n -F 'early terlan ownership and coverage' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
early_rust_reports_line=$(rg -n -F 'early Rust report drift' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
early_terlan_format_line=$(rg -n -F 'early Terlan formatting' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
early_grammar_line=$(rg -n -F 'early canonical grammar and summary' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
inventory_line=$(rg -n -F 'retained source inventory' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
test -n "$early_rust_reports_line"
test -n "$early_terlan_format_line"
test -n "$early_grammar_line"
test "$early_grammar_line" -lt "$early_terlan_format_line"
test "$early_terlan_format_line" -lt "$early_rust_reports_line"
test "$early_rust_reports_line" -lt "$early_terlan_line"
test "$early_terlan_line" -lt "$inventory_line"

# The executable budget owns four binaries in both profiles. Planner bootstrap
# already provides debug terlc and terlan-vm; the budget lane must explicitly
# provide the remaining debug native worker before invoking its validator.
native_worker_build_pattern='cargo '
native_worker_build_pattern+='build --locked -p terlan --bin terlan-native-worker'
native_worker_build_line=$(rg -n -F "$native_worker_build_pattern" \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
executable_budget_line=$(rg -n -F -- '-- --check-executables "$CARGO_TARGET_DIR"' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
test -n "$native_worker_build_line"
test -n "$executable_budget_line"
test "$native_worker_build_line" -lt "$executable_budget_line"

# Adjacent legacy `*Test.terl` modules without `@test` declarations are still
# compiled with their package, but must not be sent to an empty test runner.
# Real declaration-based suites continue to execute.
mkdir -p "$temporary/terlan-lane/scripts/git_hooks" "$temporary/terlan-lane/src"
cat > "$temporary/terlan-lane/scripts/git_hooks/formatting_only.sh" <<'FORMAT'
#!/usr/bin/env bash
exit 1
FORMAT
cat > "$temporary/terlan-lane/terlc" <<'TERLC'
#!/usr/bin/env bash
if [[ "$*" == 'test --help' ]]; then echo '--coverage-source <file>'; exit 0; fi
printf '%s\n' "$*" >> "$TERLAN_LANE_CALLS"
if [[ ${TERLAN_LANE_FAIL:-} == test && " $* " == *' test '* ]]; then exit 9; fi
TERLC
cat > "$temporary/terlan-lane/src/Legacy.terl" <<'SOURCE'
module fixture.Legacy.
SOURCE
cat > "$temporary/terlan-lane/src/LegacyTest.terl" <<'SOURCE'
module fixture.LegacyTest.
pub self_test(): Bool -> true.
SOURCE
cat > "$temporary/terlan-lane/src/Declared.terl" <<'SOURCE'
module fixture.Declared.
SOURCE
cat > "$temporary/terlan-lane/src/DeclaredTest.terl" <<'SOURCE'
module fixture.DeclaredTest.
@test
declared_test(): Bool -> true.
SOURCE
cat > "$temporary/terlan-lane/src/Second.terl" <<'SOURCE'
module fixture.Second.
SOURCE
cat > "$temporary/terlan-lane/src/SecondTest.terl" <<'SOURCE'
module fixture.SecondTest.
@test
second_test(): Bool -> true.
SOURCE
cat > "$temporary/terlan-lane/src/Prechecked.terls" <<'SOURCE'
assert(true).
SOURCE
cat > "$temporary/terlan-lane/src/SpecializedTest.terl" <<'SOURCE'
module fixture.SpecializedTest.
@test
specialized_test(): Bool -> true.
SOURCE
cat > "$temporary/terlan-lane/terlan.toml" <<'MANIFEST'
[package]
name = "hook-fixture"
version = "0.0.1"

[build]
source_roots = ["src"]
MANIFEST
printf '%s\n' src/Legacy.terl src/Declared.terl src/Second.terl src/Prechecked.terls src/SpecializedTest.terl > "$temporary/terlan-lane/changes.txt"
printf '%s\n' src/Prechecked.terls > "$temporary/terlan-lane/prechecked.txt"
printf '%s\n' src/SpecializedTest.terl > "$temporary/terlan-lane/specialized.txt"
chmod +x "$temporary/terlan-lane/terlc" \
  "$temporary/terlan-lane/scripts/git_hooks/formatting_only.sh"
git init --quiet --initial-branch=main "$temporary/terlan-lane"
git -C "$temporary/terlan-lane" add .
(
  cd "$temporary/terlan-lane"
  TERLAN_LANE_CALLS="$temporary/terlan-lane.calls" \
    TERLAN_HOOK_TERLAN_CACHE_DIR="$temporary/terlan-lane-cache" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$temporary/terlan-lane-receipts" \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      changes.txt unused "$PWD/terlc" false prechecked.txt specialized.txt \
    > "$temporary/terlan-lane.log" 2>&1
)
rg -q -- "^--cache-dir $temporary/terlan-lane-cache/[0-9a-f]{64} check src$" "$temporary/terlan-lane.calls"
rg -q -- "^--cache-dir $temporary/terlan-lane-cache/[0-9a-f]{64} test src/DeclaredTest.terl src/SecondTest.terl --coverage --coverage-source src/Declared.terl --coverage-source src/Second.terl --coverage-threshold 100$" "$temporary/terlan-lane.calls"
if rg -q -- ' build ' "$temporary/terlan-lane.calls"; then
  echo 'Hook linked a native image during source validation' >&2; exit 1
fi
if grep -Fxq 'test src/LegacyTest.terl' "$temporary/terlan-lane.calls"; then
  echo 'Hook launched an empty legacy test suite' >&2; exit 1
fi
grep -Fq 'compiled legacy self-test module: src/LegacyTest.terl' "$temporary/terlan-lane.log"
grep -Fq 'reused earlier Terlan validation: src/Prechecked.terls' "$temporary/terlan-lane.log"
grep -Fq 'specialized owner will execute: src/SpecializedTest.terl' "$temporary/terlan-lane.log"
if grep -Fq 'test src/SpecializedTest.terl' "$temporary/terlan-lane.calls"; then
  echo 'Hook executed a specialized integration contract without its owner' >&2; exit 1
fi
if grep -Fq 'src/Prechecked.terls' "$temporary/terlan-lane.calls"; then
  echo 'Hook repeated a prechecked Terlan entry build' >&2; exit 1
fi
first_call_count=$(wc -l < "$temporary/terlan-lane.calls")
(
  cd "$temporary/terlan-lane"
  TERLAN_LANE_CALLS="$temporary/terlan-lane.calls" \
    TERLAN_HOOK_TERLAN_CACHE_DIR="$temporary/terlan-lane-cache" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$temporary/terlan-lane-receipts" \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      changes.txt unused "$PWD/terlc" false prechecked.txt specialized.txt \
    >> "$temporary/terlan-lane.log" 2>&1
)
test "$(wc -l < "$temporary/terlan-lane.calls")" = "$first_call_count"
test "$(find "$temporary/terlan-lane-receipts" -type f -name '*.passed' | wc -l)" = 2
grep -Fq 'reuse test validation for . ' "$temporary/terlan-lane.log"
grep -Fq 'reuse check validation for . ' "$temporary/terlan-lane.log"

# Exact input changes invalidate only their project units. Corrupt receipts are
# rejected, and a failed unit never records success.
printf '\n// changed\n' >> "$temporary/terlan-lane/src/Declared.terl"
git -C "$temporary/terlan-lane" add src/Declared.terl
(
  cd "$temporary/terlan-lane"
  TERLAN_LANE_CALLS="$temporary/terlan-lane.calls" \
    TERLAN_HOOK_TERLAN_CACHE_DIR="$temporary/terlan-lane-cache" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$temporary/terlan-lane-receipts" \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      changes.txt unused "$PWD/terlc" false prechecked.txt specialized.txt \
    >> "$temporary/terlan-lane.log" 2>&1
)
test "$(wc -l < "$temporary/terlan-lane.calls")" = "$((first_call_count + 2))"
test "$(find "$temporary/terlan-lane-receipts" -type f -name '*.passed' | wc -l)" = 4
test "$(awk '{ print $2 }' "$temporary/terlan-lane.calls" | sort -u | wc -l)" = 1
while IFS= read -r corrupt_receipt; do
  printf 'corrupt\n' > "$corrupt_receipt"
done < <(find "$temporary/terlan-lane-receipts" -type f -name '*.passed')
before_corrupt_retry=$(wc -l < "$temporary/terlan-lane.calls")
(
  cd "$temporary/terlan-lane"
  TERLAN_LANE_CALLS="$temporary/terlan-lane.calls" \
    TERLAN_HOOK_TERLAN_CACHE_DIR="$temporary/terlan-lane-cache" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$temporary/terlan-lane-receipts" \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      changes.txt unused "$PWD/terlc" false prechecked.txt specialized.txt \
    >> "$temporary/terlan-lane.log" 2>&1
)
test "$(wc -l < "$temporary/terlan-lane.calls")" = "$((before_corrupt_retry + 2))"
if (
  cd "$temporary/terlan-lane"
  TERLAN_LANE_FAIL=test \
    TERLAN_LANE_CALLS="$temporary/terlan-lane-failure.calls" \
    TERLAN_HOOK_TERLAN_CACHE_DIR="$temporary/terlan-lane-cache" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$temporary/terlan-lane-failure-receipts" \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      changes.txt unused "$PWD/terlc" false prechecked.txt specialized.txt \
    > "$temporary/terlan-lane-failure.log" 2>&1
); then
  echo 'Failed Terlan validation unexpectedly passed' >&2; exit 1
fi
test "$(find "$temporary/terlan-lane-failure-receipts" -type f -name '*.passed' | wc -l)" = 0

# A test-only edit in one standard-library project must not invalidate the
# receipt for an unrelated project. Shared production changes still invalidate
# both receipts.
mkdir -p "$temporary/terlan-receipt-scope/std/alpha" \
  "$temporary/terlan-receipt-scope/std/beta" \
  "$temporary/terlan-receipt-scope/std/test" \
  "$temporary/terlan-receipt-scope/scripts/git_hooks"
cp "$(dirname "$driver")/git_hooks/formatting_only.sh" \
  "$temporary/terlan-receipt-scope/scripts/git_hooks/"
cp "$temporary/terlan-lane/terlc" "$temporary/terlan-receipt-scope/terlc"
cat > "$temporary/terlan-receipt-scope/std/alpha/Alpha.terl" <<'SOURCE'
module std.alpha.Alpha.
SOURCE
cat > "$temporary/terlan-receipt-scope/std/alpha/AlphaTest.terl" <<'SOURCE'
module std.alpha.AlphaTest.
@test
alpha_works(): Bool -> true.
SOURCE
cat > "$temporary/terlan-receipt-scope/std/beta/Beta.terl" <<'SOURCE'
module std.beta.Beta.
SOURCE
cat > "$temporary/terlan-receipt-scope/std/beta/BetaTest.terl" <<'SOURCE'
module std.beta.BetaTest.
@test
beta_works(): Bool -> true.
SOURCE
cat > "$temporary/terlan-receipt-scope/std/test/Test.terl" <<'SOURCE'
module std.test.Test.
SOURCE
printf '%s\n' std/alpha/AlphaTest.terl std/beta/BetaTest.terl \
  > "$temporary/terlan-receipt-scope/changes.txt"
chmod +x "$temporary/terlan-receipt-scope/terlc" \
  "$temporary/terlan-receipt-scope/scripts/git_hooks/formatting_only.sh"
git init --quiet --initial-branch=main "$temporary/terlan-receipt-scope"
git -C "$temporary/terlan-receipt-scope" add .
run_receipt_scope() {
  (
    cd "$temporary/terlan-receipt-scope"
    TERLAN_LANE_CALLS="$temporary/terlan-receipt-scope.calls" \
      TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$temporary/terlan-receipt-scope-receipts" \
      bash "$(dirname "$driver")/git_hooks/terlan.sh" \
        changes.txt unused "$PWD/terlc" false /dev/null /dev/null \
      >> "$temporary/terlan-receipt-scope.log" 2>&1
  )
}
: > "$temporary/terlan-receipt-scope.calls"
: > "$temporary/terlan-receipt-scope.log"
run_receipt_scope
test "$(wc -l < "$temporary/terlan-receipt-scope.calls")" = 2
printf '\n// alpha test changed\n' \
  >> "$temporary/terlan-receipt-scope/std/alpha/AlphaTest.terl"
git -C "$temporary/terlan-receipt-scope" add std/alpha/AlphaTest.terl
run_receipt_scope
test "$(wc -l < "$temporary/terlan-receipt-scope.calls")" = 3
grep -Fq 'reuse test validation for std/beta ' "$temporary/terlan-receipt-scope.log"
printf '\n// shared production changed\n' \
  >> "$temporary/terlan-receipt-scope/std/test/Test.terl"
git -C "$temporary/terlan-receipt-scope" add std/test/Test.terl
run_receipt_scope
test "$(wc -l < "$temporary/terlan-receipt-scope.calls")" = 5

# Fast package tests run before compiler self-validation even when the changed
# path inventory lists the compiler first.
mkdir -p "$temporary/terlan-order/compiler/self_host" \
  "$temporary/terlan-order/std/native" \
  "$temporary/terlan-order/scripts/git_hooks"
cp "$(dirname "$driver")/git_hooks/formatting_only.sh" \
  "$temporary/terlan-order/scripts/git_hooks/"
cp "$temporary/terlan-lane/terlc" "$temporary/terlan-order/terlc"
cat > "$temporary/terlan-order/compiler/self_host/Slow.terl" <<'SOURCE'
module fixture.Slow.
SOURCE
cat > "$temporary/terlan-order/compiler/self_host/SlowTest.terl" <<'SOURCE'
module fixture.SlowTest.
@test
slow_test(): Bool -> true.
SOURCE
cat > "$temporary/terlan-order/std/native/Fast.terl" <<'SOURCE'
module fixture.Fast.
SOURCE
cat > "$temporary/terlan-order/std/native/FastTest.terl" <<'SOURCE'
module fixture.FastTest.
@test
fast_test(): Bool -> true.
SOURCE
printf '%s\n' compiler/self_host/Slow.terl std/native/Fast.terl \
  > "$temporary/terlan-order/changes.txt"
chmod +x "$temporary/terlan-order/terlc" \
  "$temporary/terlan-order/scripts/git_hooks/formatting_only.sh"
git init --quiet --initial-branch=main "$temporary/terlan-order"
git -C "$temporary/terlan-order" add .
(
  cd "$temporary/terlan-order"
  TERLAN_LANE_CALLS="$temporary/terlan-order.calls" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$temporary/terlan-order-receipts" \
    TERLAN_TEST_FORCE_RERUN=1 \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      changes.txt unused "$PWD/terlc" false /dev/null /dev/null
)
test "$(sed -n '1p' "$temporary/terlan-order.calls")" = \
  'test std/native/FastTest.terl --coverage --coverage-source std/native/Fast.terl --coverage-threshold 100'
test "$(sed -n '2p' "$temporary/terlan-order.calls")" = \
  'test compiler/self_host/SlowTest.terl --coverage --coverage-source compiler/self_host/Slow.terl --coverage-threshold 100'

# Prebuilt workspace binaries must honor the hook's isolated Cargo target.
# A relative target/debug path points at the staged checkout instead and made
# release-contract validation fail only after all earlier commit gates passed.
mkdir -p "$temporary/prebuilt/scripts" "$temporary/prebuilt/bin"
cp "$(dirname "$driver")/run_prebuilt_terlan_binary.sh" "$temporary/prebuilt/scripts/"
cat > "$temporary/prebuilt/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
set -eu
printf '%s\n' "$*" >> "$PREBUILT_CARGO_CALLS"
mkdir -p "$CARGO_TARGET_DIR/debug"
cat > "$CARGO_TARGET_DIR/debug/terlan-quality" <<'BINARY'
#!/usr/bin/env bash
printf 'prebuilt:%s\n' "$*"
BINARY
chmod +x "$CARGO_TARGET_DIR/debug/terlan-quality"
CARGO
chmod +x "$temporary/prebuilt/bin/cargo"
(
  cd "$temporary/prebuilt"
  PATH="$PWD/bin:$PATH" \
    CARGO_TARGET_DIR="$PWD/owned-target" \
    PREBUILT_CARGO_CALLS="$temporary/prebuilt-cargo.calls" \
    bash scripts/run_prebuilt_terlan_binary.sh terlan-quality quality-tools -- release-check
) > "$temporary/prebuilt.log"
grep -Fx 'prebuilt:release-check' "$temporary/prebuilt.log"
grep -Fx 'quality-tools' "$temporary/prebuilt/owned-target/debug/.terlan-prebuilt-terlan-quality.profile"
test "$(wc -l < "$temporary/prebuilt-cargo.calls")" = 1
test ! -e "$temporary/prebuilt/target/debug/terlan-quality"

# Rust report reuse is bound to the exact staged tree and Rust toolchain.
# Corruption and failed validation never grant reuse.
mkdir -p "$temporary/report-cache/scripts/git_hooks" "$temporary/report-cache/bin" \
  "$temporary/report-cache/owned-target/debug"
cp "$(dirname "$driver")/git_hooks/rust_reports.sh" \
  "$temporary/report-cache/scripts/git_hooks/"
printf 'first\n' > "$temporary/report-cache/input.txt"
cat > "$temporary/report-cache/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
if [[ "${1:-}" == -Vv ]]; then printf 'cargo fixture 1\n'; exit 0; fi
printf 'cargo:%s\n' "$*" >> "$REPORT_CACHE_CALLS"
CARGO
cat > "$temporary/report-cache/bin/rustc" <<'RUSTC'
#!/usr/bin/env bash
test "${1:-}" = -Vv
printf 'rustc fixture 1\n'
RUSTC
cat > "$temporary/report-cache/owned-target/debug/terlan-quality" <<'QUALITY'
#!/usr/bin/env bash
printf 'quality:%s\n' "$*" >> "$REPORT_CACHE_CALLS"
mkdir -p target/quality
touch target/quality/rust-api-boundary-input.tsv \
  target/quality/rust-shared-helper-input.tsv \
  target/quality/rust-structural-input.json
QUALITY
cat > "$temporary/report-cache/owned-target/debug/terlan-rust-boundary-audit" <<'BOUNDARY'
#!/usr/bin/env bash
printf 'boundary:%s\n' "$*" >> "$REPORT_CACHE_CALLS"
printf '{}\n'
BOUNDARY
cat > "$temporary/report-cache/owned-target/debug/terlan-test-orchestrator" <<'ORCHESTRATOR'
#!/usr/bin/env bash
printf 'orchestrator:%s\n' "$*" >> "$REPORT_CACHE_CALLS"
test "$1" = --cargo-metadata
mkdir -p "$(dirname "$2")"
printf '{}\n' > "$2"
ORCHESTRATOR
cat > "$temporary/report-cache/owned-target/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
printf 'terlc:%s\n' "$*" >> "$REPORT_CACHE_CALLS"
mkdir -p target/git-hooks-rust-quality/vm
printf 'image\n' > target/git-hooks-rust-quality/vm/scripts_RustQuality.tvm
TERLC
cat > "$temporary/report-cache/owned-target/debug/terlan-vm" <<'VM'
#!/usr/bin/env bash
printf 'vm:%s\n' "$*" >> "$REPORT_CACHE_CALLS"
exit "${REPORT_VM_FAILURE:-0}"
VM
chmod +x "$temporary/report-cache/bin/"* \
  "$temporary/report-cache/owned-target/debug/"*
git init --quiet --initial-branch=main "$temporary/report-cache"
(
  cd "$temporary/report-cache"
  git config user.name 'Report cache test'
  git config user.email 'reports@example.invalid'
  git add input.txt scripts/git_hooks/rust_reports.sh
  git commit --quiet -m Initial
)
report_cache_calls="$temporary/report-cache.calls"
: > "$report_cache_calls"
run_report_cache() {
  (
    cd "$temporary/report-cache"
    PATH="$PWD/bin:$PATH" \
    CARGO_TARGET_DIR="$PWD/owned-target" \
    REPORT_CACHE_CALLS="$report_cache_calls" \
    TERLAN_HOOK_ARTIFACTS_PREBUILT=1 \
    TERLAN_RUST_REPORT_TERLC="$PWD/owned-target/debug/terlc" \
    TERLAN_RUST_REPORT_VM="$PWD/owned-target/debug/terlan-vm" \
    TERLAN_RUST_REPORT_CACHE_DIR="$PWD/owned-target/validation-results/rust-reports" \
      bash scripts/git_hooks/rust_reports.sh
  )
}
run_report_cache > "$temporary/report-cache-first.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 5
run_report_cache > "$temporary/report-cache-reused.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 5
grep -Fq 'Rust quality reports unchanged; reusing' "$temporary/report-cache-reused.log"
receipt=$(find "$temporary/report-cache/owned-target/validation-results/rust-reports" \
  -type f -name '*.passed' -print -quit)
printf 'corrupt\n' > "$receipt"
run_report_cache > "$temporary/report-cache-corrupt.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 10
printf 'second\n' > "$temporary/report-cache/input.txt"
git -C "$temporary/report-cache" add input.txt
run_report_cache > "$temporary/report-cache-tree.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 15
printf '\n# changed toolchain executable\n' >> "$temporary/report-cache/bin/rustc"
run_report_cache > "$temporary/report-cache-tool.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 20
printf 'third\n' > "$temporary/report-cache/input.txt"
git -C "$temporary/report-cache" add input.txt
receipt_count=$(find "$temporary/report-cache/owned-target/validation-results/rust-reports" \
  -type f -name '*.passed' | wc -l)
if REPORT_VM_FAILURE=49 run_report_cache > "$temporary/report-cache-failure.log" 2>&1; then
  echo 'Failed Rust report validation sealed a cache receipt' >&2; exit 1
fi
test "$(find "$temporary/report-cache/owned-target/validation-results/rust-reports" \
  -type f -name '*.passed' | wc -l)" = "$receipt_count"
run_report_cache > "$temporary/report-cache-recovery.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 26
TERLAN_TEST_FORCE_RERUN=1 run_report_cache > "$temporary/report-cache-forced.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 31

echo 'Git hook regression tests passed (24 scenarios).'
