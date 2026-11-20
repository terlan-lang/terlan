#!/usr/bin/env bash
# Exercise the Git boundary with tiny repositories and a staged fixture checker.
set -euo pipefail
trap 'status=$?; line=$LINENO; trap - ERR; echo "Git hook regression failed at line $line (status $status)" >&2; exit "$status"' ERR
unset TERLAN_HOOK_SHARED_TARGET TERLAN_HOOK_TERLAN_CACHE_DIR
unset TERLAN_HOOK_VALIDATION_RECEIPT_DIR TERLAN_TEST_FORCE_RERUN
export TERLAN_HOOK_MODE=full
driver="$(cd "$(dirname "$0")" && pwd)/git_hooks.sh"
version_filter="$(cd "$(dirname "$0")" && pwd)/git_hooks/filter_version_changes.cjs"
exact_cargo_test="$(cd "$(dirname "$0")" && pwd)/run_exact_cargo_test.sh"
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
for descriptor in /proc/$$/fd/*; do
  target=$(readlink "$descriptor" 2>/dev/null || true)
  case "$target" in
    */terlan-publication.lock|*/validation.lock)
      echo "Validation child inherited hook lease: $target" >&2
      exit 1
      ;;
  esac
done
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

# Proof evidence changes without their generated baseline fail before any
# inventory, build, or proof workload starts.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
case "$*" in
  'run scripts/git_hooks/scripts/Plan.terls -- changes.txt')
    printf 'preflight:inventory\npreflight:proof-baseline-missing\nshell\n'
    ;;
  *) exit 96 ;;
esac
TERLC
chmod +x "$temporary/early-bindings/cache/debug/terlc"
(
  cd "$temporary/early-bindings"
  status=0
  TERLAN_HOOK_MODE=fast \
    TERLAN_HOOK_LATEST_TERLC="$PWD/cache/debug/terlc" \
    CARGO_TARGET_DIR="$PWD/cache" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/proof-baseline-pairing.log" 2>&1 || status=$?
  test "$status" = 1
  grep -q 'proof evidence changed without refreshed release baseline' \
    "$temporary/proof-baseline-pairing.log"
)

cat > "$temporary/early-bindings/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
case "$*" in
  'build --locked -p terlan --bin terlc --bin terlan-vm --features validation-union') exit 0 ;;
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
    printf '%s\n' terlan-staged-compiler-input-v4
    git ls-files -s -z -- Cargo.toml Cargo.lock rust-toolchain.toml .cargo
    git ls-files -s -z -- scripts \
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
# A broad formatter lane may be selected for a non-Terlan hook change. The
# driver must not invoke lint or fmt with an empty path list.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
if [[ "$*" == 'run scripts/git_hooks/scripts/Plan.terls -- changes.txt' ]]; then
  printf 'preflight:inventory\npreflight:terlan-format\nshell\n'
  exit 0
fi
if [[ "$*" == 'test scripts/self_validation/InventoryTest.terl' ]]; then exit 0; fi
case "$1" in lint|fmt) exit 41 ;; esac
exit 0
TERLC
chmod +x "$temporary/early-bindings/cache/debug/terlc"
printf 'scripts/git_hooks/check.sh\n' > "$temporary/early-bindings/changes.txt"
(
  cd "$temporary/early-bindings"
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/empty-terlan-format.log" 2>&1
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
# A selected Rust lane must fail before broad preflight gates, and its failure
# must propagate without executing the later gate.
cp "$exact_cargo_test" "$temporary/early-bindings/scripts/run_exact_cargo_test.sh"
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
case "$*" in
  'run scripts/git_hooks/scripts/Plan.terls -- changes.txt')
    printf 'preflight:inventory\npreflight:negative-api\ncommands-doctor\nshell\n'
    ;;
  'run scripts/self_validation/StdlibValidation.terls -- negative-api')
    printf 'late-negative-api\n' >> "$HOOK_ORDER_CALLS"
    exit 43
    ;;
  *) exit 98 ;;
esac
TERLC
cat > "$temporary/early-bindings/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
if [[ "${1:-}" == -Vv ]]; then printf 'cargo fixture 1\n'; exit 0; fi
printf 'cargo:%s\n' "$*" >> "$HOOK_ORDER_CALLS"
exit "${HOOK_CARGO_STATUS:-42}"
CARGO
chmod +x "$temporary/early-bindings/cache/debug/terlc" \
  "$temporary/early-bindings/bin/cargo" \
  "$temporary/early-bindings/scripts/run_exact_cargo_test.sh"
(
  cd "$temporary/early-bindings"
  calls="$temporary/early-rust-order.calls"
  : > "$calls"
  status=0
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" HOOK_ORDER_CALLS="$calls" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/early-rust-order.log" 2>&1 || status=$?
  test "$status" = 42
  grep -Fq 'cargo:test --profile dev --locked -p terlan --lib commands::doctor::doctor_test' "$calls"
  test "$(wc -l < "$calls")" = 1
  grep -Fq 'early affected Rust lane failed: commands-doctor' \
    "$temporary/early-rust-order.log"
)
# Exit status 2 is a normal command failure, not an unsupported-lane marker.
# It must stop immediately instead of deferring the lane to the broad plan.
(
  cd "$temporary/early-bindings"
  calls="$temporary/early-rust-status-two.calls"
  : > "$calls"
  status=0
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" \
    HOOK_ORDER_CALLS="$calls" HOOK_CARGO_STATUS=2 \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/early-rust-status-two.log" 2>&1 || status=$?
  test "$status" = 2
  test "$(wc -l < "$calls")" = 1
  grep -Fq 'early affected Rust lane failed: commands-doctor' \
    "$temporary/early-rust-status-two.log"
)
# Successful affected Rust lanes are sealed by their admitted inputs. A retry
# reuses that result, while an explicit forced run executes it again.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
case "$*" in
  'run scripts/git_hooks/scripts/Plan.terls -- changes.txt')
    printf 'preflight:inventory\ncommands-doctor\n'
    ;;
  'test scripts/self_validation/InventoryTest.terl') exit 0 ;;
  *) exit 98 ;;
esac
TERLC
cat > "$temporary/early-bindings/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
if [[ "${1:-}" == -Vv ]]; then printf 'cargo fixture 1\n'; exit 0; fi
printf 'cargo:%s\n' "$*" >> "$HOOK_ORDER_CALLS"
printf 'running 1 test\n'
CARGO
chmod +x "$temporary/early-bindings/cache/debug/terlc" \
  "$temporary/early-bindings/bin/cargo"
(
  cd "$temporary/early-bindings"
  calls="$temporary/early-rust-cache.calls"
  : > "$calls"
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" HOOK_ORDER_CALLS="$calls" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/early-rust-cache-first.log" 2>&1
  test "$(wc -l < "$calls")" = 1
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" HOOK_ORDER_CALLS="$calls" \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/early-rust-cache-reused.log" 2>&1
  test "$(wc -l < "$calls")" = 1
  grep -Fq 'reuse affected Rust lane commands-doctor' \
    "$temporary/early-rust-cache-reused.log"
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" HOOK_ORDER_CALLS="$calls" \
    TERLAN_TEST_FORCE_RERUN=1 \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/early-rust-cache-forced.log" 2>&1
  test "$(wc -l < "$calls")" = 2
)
# Compiler-heavy Rust lanes must divide host capacity between the Rust test
# harness and each test's bounded compiler workers instead of multiplying both.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
case "$*" in
  'run scripts/git_hooks/scripts/Plan.terls -- changes.txt')
    printf 'preflight:inventory\ncommands\n'
    ;;
  'test scripts/self_validation/InventoryTest.terl') exit 0 ;;
  *) exit 98 ;;
esac
TERLC
cat > "$temporary/early-bindings/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
if [[ "${1:-}" == -Vv ]]; then printf 'cargo fixture 1\n'; exit 0; fi
printf 'cargo:%s:tests=%s:builds=%s\n' "$*" \
  "${RUST_TEST_THREADS:-unset}" "${TERLAN_BUILD_JOBS:-unset}" >> "$HOOK_ORDER_CALLS"
if [[ "${1:-}" == test ]]; then printf 'running 1 test\n'; fi
CARGO
chmod +x "$temporary/early-bindings/cache/debug/terlc" \
  "$temporary/early-bindings/bin/cargo"
(
  cd "$temporary/early-bindings"
  calls="$temporary/compiler-heavy-rust.calls"
  : > "$calls"
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" HOOK_ORDER_CALLS="$calls" \
    RUST_TEST_THREADS=2 TERLAN_BUILD_JOBS=3 TERLAN_TEST_FORCE_RERUN=1 \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/compiler-heavy-rust.log" 2>&1
  grep -Fq 'cargo:test --profile dev --locked -p terlan --lib commands:::tests=2:builds=3' "$calls"
  grep -Fq 'compiler-heavy Rust shape: tests=2 builds=3' \
    "$temporary/compiler-heavy-rust.log"
)
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
release_contract_body=$(sed -n '/^run_release_contract_lane() {/,/^}/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
bootstrap_body=$(sed -n '/^    bootstrap)/,/^      ;;/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
publication_test_pattern='cargo '
publication_test_pattern+='test --profile dev --locked -p terlan-test-orchestrator --test publish_preflight'
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
grep -Fq 'scripts/self_validation/RepositoryValidationMain.terl' \
  <<< "$release_contract_body"
grep -Fq 'repository_validator_image' <<< "$release_contract_body"
grep -Fq 'TERLAN_REPOSITORY_VALIDATION=$CARGO_TARGET_DIR/debug/terlan-vm run' \
  <<< "$release_contract_body"
if grep -Fq 'TERLAN_REPOSITORY_VALIDATION=\"$terlc\" run' <<< "$release_contract_body"; then
  echo 'Release contract recompiles the repository validator on every run' >&2; exit 1
fi
early_release_contract_line=$(grep -n -F "echo 'pre-commit: early release and install contract'" \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
early_artifact_bootstrap_line=$(grep -n -F \
  "echo 'pre-commit: early artifact bootstrap ownership'" \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
affected_rust_loop_line=$(grep -n -F 'run_cached_affected_rust_lane "$lane"' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
test -n "$early_release_contract_line"
test -n "$early_artifact_bootstrap_line"
test -n "$affected_rust_loop_line"
test "$early_release_contract_line" -lt "$affected_rust_loop_line"
test "$early_artifact_bootstrap_line" -lt "$affected_rust_loop_line"
test "$(grep -Fc 'bash scripts/build_artifact_bootstrap_test.sh' \
  "$(dirname "$driver")/git_hooks/check.sh")" = 1
repository_contract_recipe=$(awk '
  /^repository-build-release-contract-check:/ { active=1 }
  active && !/^repository-build-release-contract-check:/ \
    && /^[^[:space:]#][^=]*:/ { exit }
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
grep -Fq 'TERLAN_HOOK_MODE=full bash "$driver" check' \
  "$(dirname "$driver")/../.githooks/pre-commit"
grep -Fq "rg -qx 'preflight:inventory'" \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq '[[ "$hosted_source_preflight" == 0 ]]' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'candidate compiler produced an empty validation plan' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'early affected Rust lane passed' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'canonical Rust suite failed with status' \
  "$(dirname "$driver")/git_hooks/check.sh"
if rg -q '^export TERLAN_CUDA_DIR$$' "$(dirname "$driver")/../Makefile"; then
  echo 'CUDA package routing leaked into every validation recipe' >&2
  exit 1
fi
grep -Fq 'stopping after canonical Rust suite failure' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'duplicate affected Rust lane remained scheduled' \
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
early_affected_rust_line=$(rg -n -F 'early affected Rust lane passed' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
early_terlan_format_line=$(rg -n -F 'early Terlan formatting' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
early_grammar_line=$(rg -n -F 'early canonical grammar and summary' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
inventory_line=$(rg -n -F 'retained source inventory' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
test -n "$early_rust_reports_line"
test -n "$early_affected_rust_line"
test -n "$early_terlan_format_line"
test -n "$early_grammar_line"
test "$early_grammar_line" -lt "$early_terlan_format_line"
test "$early_terlan_format_line" -lt "$early_rust_reports_line"
test "$early_rust_reports_line" -lt "$early_terlan_line"
test "$early_terlan_line" -lt "$early_affected_rust_line"
test "$early_terlan_line" -lt "$inventory_line"

# Proof semantics depend on the exact staged compiler inputs, not Cargo's
# optimization profile. Reuse the already validated candidate compiler so the
# earliest proof gate cannot trigger a redundant release-profile rebuild.
proof_baseline_block=$(sed -n '/^if lane_has proofs;/,/^fi$/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
grep -Fq 'TERLC="$terlc" bash scripts/git_hooks/proof_baseline.sh' \
  <<< "$proof_baseline_block"
proof_baseline_driver="$(dirname "$driver")/git_hooks/proof_baseline.sh"
grep -Fq '"$quality" lean-proof-pr' "$proof_baseline_driver"
proof_pr_line=$(grep -n -F '"$quality" lean-proof-pr' "$proof_baseline_driver" | cut -d: -f1)
grep -Fq -- '--name repository_lean_proof_feature_map_is_current' "$proof_baseline_driver"
proof_feature_map_line=$(grep -n -F -- '--name repository_lean_proof_feature_map_is_current' \
  "$proof_baseline_driver" | cut -d: -f1)
proof_runtime_line=$(grep -n -F '"$quality" lean-proof-runtime' "$proof_baseline_driver" | cut -d: -f1)
test "$proof_pr_line" -lt "$proof_runtime_line"
test "$proof_pr_line" -lt "$proof_feature_map_line"
test "$proof_feature_map_line" -lt "$proof_runtime_line"
check_driver="$(dirname "$driver")/git_hooks/check.sh"
early_clippy_line=$(grep -n -F "echo 'pre-commit: early clippy'" "$check_driver" | cut -d: -f1)
affected_rust_line=$(grep -n -F 'while IFS= read -r lane; do' "$check_driver" | head -1 | cut -d: -f1)
test -n "$early_clippy_line"
test "$early_clippy_line" -lt "$affected_rust_line"
test "$(grep -Fc 'TERLC="$TERLAN_RUST_SUITE_TERLC" \' \
  "$(dirname "$driver")/git_hooks/check.sh")" -eq 3
if grep -Fq 'ensure_release_tools' <<< "$proof_baseline_block"; then
  echo 'Proof baseline rebuilt release-profile compiler unnecessarily' >&2
  exit 1
fi
if rg -q '\$\(RUST_TEST\).* -p terlan --lib.*lean_proof_' \
  "$(dirname "$driver")/../mk/lean-proof-checks.mk"; then
  echo 'Proof producers duplicated Rust tests owned by rust-test-suite' >&2
  exit 1
fi
native_boundary_security_block=$(sed -n \
  '/^native-boundary-security-check:/,/^[^[:space:]#][^=]*:/p' \
  "$(dirname "$driver")/../mk/accelerator-checks.mk")
grep -Fq '$(TERLAN_QUALITY) native-boundary-security' \
  <<< "$native_boundary_security_block"
if grep -Fq '$(RUST_TEST)' <<< "$native_boundary_security_block"; then
  echo 'Native-boundary proof producer duplicated rust-test-suite' >&2
  exit 1
fi
workspace_native_binding_block=$(sed -n \
  '/^if \[\[ "$workspace_lane_selected" == true \]\] && lane_has native-bindings;/,/^fi$/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
grep -Fq "rg -vx 'native-bindings'" <<< "$workspace_native_binding_block"
grep -Fq 'canonical Rust suite owns native binding contracts' \
  <<< "$workspace_native_binding_block"

# The executable budget owns four binaries in both profiles. Planner bootstrap
# already provides debug terlc and terlan-vm; the budget lane must explicitly
# provide the remaining debug native worker before invoking its validator.
native_worker_build_pattern='cargo '
native_worker_build_pattern+='build --locked -p terlan --bin terlan-native-worker'
native_worker_build_line=$(rg -n -F "$native_worker_build_pattern" \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1 | tail -1)
executable_budget_line=$(rg -n -F -- '-- --check-executables "$CARGO_TARGET_DIR"' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
test -n "$native_worker_build_line"
test -n "$executable_budget_line"
test "$native_worker_build_line" -lt "$executable_budget_line"

# Rust lanes that execute native capability integration tests must provision
# their declared companion once and pass its exact path to every test child.
grep -Fq 'build required native test companion once' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'reuse exact native test companion' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq '.terlan-native-worker-input' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'build --locked -p terlan --bin terlan-native-worker --features validation-union' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'export TERLAN_NATIVE_WORKER="$CARGO_TARGET_DIR/debug/terlan-native-worker"' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'reuse affected Rust lane' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'TERLAN_HOOK_COMPILER_INPUT_KEY="$compiler_validation_identity"' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'generated Rust reports regenerated; return them for staging' \
  "$(dirname "$driver")/git_hooks/check.sh"
invalidation_count=$(grep -Fc \
  'node scripts/git_hooks/invalidate_workspace_cache.cjs \' \
  "$(dirname "$driver")/git_hooks/check.sh")
if [[ "$invalidation_count" != 1 ]]; then
  echo 'Compiler rebuild cache must be invalidated exactly once before rebuilding' >&2
  exit 1
fi
grep -Fq '"$cache_metadata" "$CARGO_TARGET_DIR" "$PWD"' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'workflow_contract_cache="$CARGO_TARGET_DIR/terlan-validators/tvm-aot-platform-matrix"' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq -- '--cache-dir "$workflow_contract_cache/self-test-cache"' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq -- '--cache-dir "$workflow_contract_cache/test-cache"' \
  "$(dirname "$driver")/git_hooks/check.sh"
grep -Fq 'TERLAN_RUST_REPORT_MODE:-check' \
  "$(dirname "$driver")/git_hooks/check.sh"
reports_lane=$(sed -n '/^run_rust_reports_lane()/,/^}/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
grep -Fq 'TERLAN_RUST_REPORT_CACHE_DIR=' <<< "$reports_lane"
if grep -Eq 'ensure_|TERLAN_HOOK_ARTIFACTS_PREBUILT' <<< "$reports_lane"; then
  echo 'Rust report tools are provisioned before cache lookup' >&2
  exit 1
fi
terlan_lane=$(sed -n '/^if lane_has terlan;/,/^fi$/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
grep -Fq 'ensure_native_test_companions' <<< "$terlan_lane"
commands_lane=$(sed -n '/^    commands)/,/^      ;;/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
workspace_lane=$(sed -n '/^    workspace)/,/^      ;;/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
workspace_suite=$(sed -n '/^run_workspace_suite()/,/^}/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
proof_coverage=$(sed -n '/^run_proof_coverage()/,/^}/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
grep -Fq 'ensure_native_test_companions' <<< "$commands_lane"
grep -Fq 'ensure_test_program_snapshots' <<< "$workspace_lane"
if grep -Fq 'ensure_orchestrator' <<< "$workspace_lane"; then
  echo 'Workspace lane redundantly provisions the snapshot owner' >&2
  exit 1
fi
grep -Fq 'run_workspace_suite' <<< "$workspace_lane"
grep -Fq 'TERLAN_VALIDATION_BOOTSTRAPPED=1' <<< "$workspace_suite"
grep -Fq 'TERLAN_RUST_ORCHESTRATOR="$orchestrator_path"' \
  <<< "$workspace_suite"
grep -Fq 'TERLAN_CARGO_METADATA_OWNER="$orchestrator_path"' \
  <<< "$workspace_suite"
grep -Fq 'TERLAN_RUST_SUITE_REPORT="$report"' <<< "$workspace_suite"
grep -Fq 'TERLAN_RUST_PHASE_RECEIPT_DIR="$receipts"' <<< "$workspace_suite"
grep -Fq '"$orchestrator_path"' <<< "$workspace_suite"
grep -Fq -- '--cargo-metadata "$metadata" -- cargo' <<< "$workspace_suite"
if grep -Fq 'make --no-print-directory rust-test-suite' <<< "$workspace_suite"; then
  echo 'Canonical Rust suite still crosses a nested Make environment boundary' >&2
  exit 1
fi
grep -Fq 'canonical Rust suite failed with status' <<< "$workspace_suite"
grep -Fq 'bash scripts/git_hooks/proofs.sh' <<< "$proof_coverage"
grep -Fq 'workspace_lane_selected=true' \
  "$(dirname "$driver")/git_hooks/check.sh"
proof_baseline_line=$(grep -n "early proof source and policy contracts" \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
workspace_loop_line=$(grep -n 'run_cached_affected_rust_lane "$lane"' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
test -n "$proof_baseline_line"
test -n "$workspace_loop_line"
test "$proof_baseline_line" -lt "$workspace_loop_line"
test "$(grep -c 'bash scripts/git_hooks/proof_baseline.sh' \
  "$(dirname "$driver")/git_hooks/check.sh")" = 1
early_proof_coverage_block=$(sed -n \
  '/^if \[\[ "$workspace_lane_selected" == true \]\] && lane_has proofs;/,/^fi$/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
grep -Fq 'proof runtime coverage immediately follows canonical Rust suite' \
  <<< "$early_proof_coverage_block"
grep -Fq 'run_proof_coverage' <<< "$early_proof_coverage_block"
grep -Fq "rg -vx 'proofs'" <<< "$early_proof_coverage_block"
early_proof_coverage_line=$(grep -n \
  'proof runtime coverage immediately follows canonical Rust suite' \
  "$(dirname "$driver")/git_hooks/check.sh" | cut -d: -f1)
test "$workspace_loop_line" -lt "$early_proof_coverage_line"
test "$early_proof_coverage_line" -lt "$inventory_line"
proof_lane=$(sed -n '/^    proofs)/,/^      ;;/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
grep -Fq 'if [[ "$workspace_lane_selected" == false ]]' <<< "$proof_lane"
grep -Fq 'run_workspace_suite' <<< "$proof_lane"
grep -Fq 'stopping after canonical Rust suite failure' <<< "$proof_lane"
grep -Fq 'run_proof_coverage' <<< "$proof_lane"
grep -Fq -- '--with-cargo-coverage "$report"' \
  "$(dirname "$driver")/git_hooks/proofs.sh"
if grep -Fq 'PATH=' "$(dirname "$driver")/git_hooks/proofs.sh"; then
  echo 'Proof coverage changed the owner-provided canonical test PATH' >&2
  exit 1
fi
grep -Fq 'TERLAN_PROOF_RUNTIME_RUN=:' \
  "$(dirname "$driver")/git_hooks/proofs.sh"
grep -Fq 'TERLAN_PROOF_TRACK_RUN=:' \
  "$(dirname "$driver")/git_hooks/proofs.sh"
tail -20 "$(dirname "$driver")/git_hooks/check.sh" \
  | grep -Fq 'make --no-print-directory rust-artifact-retention-check'
orchestrator_provisioning=$(sed -n '/^ensure_orchestrator()/,/^}/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
grep -Fq 'validation-tools' <<< "$orchestrator_provisioning"
grep -Fq 'sha256sum "$source"' <<< "$orchestrator_provisioning"
grep -Fq 'install -m 755 "$source" "$temporary"' <<< "$orchestrator_provisioning"
snapshot_provisioning=$(sed -n '/^ensure_test_program_snapshots()/,/^}/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
snapshot_copy=$(sed -n '/^snapshot_test_programs()/,/^}/p' \
  "$(dirname "$driver")/git_hooks/check.sh")
grep -Fq 'ensure_native_test_companions' <<< "$snapshot_provisioning"
grep -Fq 'ensure_orchestrator' <<< "$snapshot_provisioning"
grep -Fq -- '--snapshot-test-programs "$CARGO_TARGET_DIR"' <<< "$snapshot_copy"
grep -Fq 'TERLAN_RUST_SUITE_TERLC=' <<< "$snapshot_provisioning"
grep -Fq 'TERLAN_RUST_SUITE_VM=' <<< "$snapshot_provisioning"
grep -Fq 'TERLAN_RUST_SUITE_NATIVE_WORKER=' <<< "$snapshot_provisioning"
if grep -Fq 'run_exact_cargo_test.sh' <<< "$workspace_lane"; then
  echo 'Workspace admission must use the canonical single-harness orchestrator' >&2
  exit 1
fi

# A regenerated dependency report returns immediately to the outer staging
# guard. If regeneration is a no-op, affected validation must continue.
mkdir -p "$temporary/report-handoff/scripts/git_hooks" \
  "$temporary/report-handoff/docs/quality" "$temporary/report-handoff/cache/debug" \
  "$temporary/report-handoff/bin"
cp "$exact_cargo_test" "$temporary/report-handoff/scripts/run_exact_cargo_test.sh"
printf 'old\n' > "$temporary/report-handoff/docs/quality/RUST_DEPENDENCY_IMPACT.json"
printf 'README.md\n' > "$temporary/report-handoff/changes.txt"
cat > "$temporary/report-handoff/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
case "$*" in
  'run scripts/git_hooks/scripts/Plan.terls -- changes.txt')
    printf 'preflight:inventory\nrust-reports\ncommands-doctor\n'
    ;;
  'test scripts/self_validation/InventoryTest.terl') exit 0 ;;
  *) exit 98 ;;
esac
TERLC
cat > "$temporary/report-handoff/scripts/git_hooks/rust_reports.sh" <<'REPORT'
#!/usr/bin/env bash
printf 'generated\n' > docs/quality/RUST_DEPENDENCY_IMPACT.json
REPORT
cat > "$temporary/report-handoff/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
if [[ "${1:-}" == -Vv ]]; then printf 'cargo fixture 1\n'; exit 0; fi
printf 'cargo:%s\n' "$*" >> "$REPORT_HANDOFF_CALLS"
exit 44
CARGO
chmod +x "$temporary/report-handoff/cache/debug/terlc" \
  "$temporary/report-handoff/scripts/git_hooks/rust_reports.sh" \
  "$temporary/report-handoff/scripts/run_exact_cargo_test.sh" \
  "$temporary/report-handoff/bin/cargo"
git init --quiet --initial-branch=main "$temporary/report-handoff"
git -C "$temporary/report-handoff" add .
(
  cd "$temporary/report-handoff"
  {
    printf '%s\n' terlan-staged-compiler-input-v4
    git ls-files -s -z -- Cargo.toml Cargo.lock rust-toolchain.toml .cargo
    git ls-files -s -z -- scripts \
      ':(exclude)crates/terlan/src/compiler/syntax/terlan_lalrpop.lalrpop'
  } | git hash-object --stdin > cache/.terlan-compiler-input
  : > "$temporary/report-handoff.calls"
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" \
    REPORT_HANDOFF_CALLS="$temporary/report-handoff.calls" \
    TERLAN_RUST_REPORT_MODE=--record-dependency-impact \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/report-handoff-generated.log" 2>&1
  test ! -s "$temporary/report-handoff.calls"
  grep -Fq 'generated Rust reports regenerated; return them for staging' \
    "$temporary/report-handoff-generated.log"
  git add docs/quality/RUST_DEPENDENCY_IMPACT.json
  status=0
  PATH="$PWD/bin:$PATH" CARGO_TARGET_DIR="$PWD/cache" \
    REPORT_HANDOFF_CALLS="$temporary/report-handoff.calls" \
    TERLAN_RUST_REPORT_MODE=--record-dependency-impact \
    bash "$(dirname "$driver")/git_hooks/check.sh" changes.txt unused \
    > "$temporary/report-handoff-unchanged.log" 2>&1 || status=$?
  test "$status" = 44
  grep -Fq 'cargo:test --profile dev --locked -p terlan --lib commands::doctor::doctor_test' \
    "$temporary/report-handoff.calls"
)

# A Rust compilation-input change regenerates its checked report from the exact
# staged snapshot and refuses admission until that generated output is staged.
git init --quiet --initial-branch=main "$temporary/dependency-report"
(
  cd "$temporary/dependency-report"
  git config user.name 'Dependency report test'
  git config user.email 'dependency-report@example.invalid'
  mkdir -p scripts/git_hooks docs/quality
  cp "$version_filter" scripts/git_hooks/
  cat > scripts/git_hooks/check.sh <<'CHECK'
set -eu
dependency=docs/quality/RUST_DEPENDENCY_IMPACT.json
headroom=docs/quality/RUST_FILE_HEADROOM.tsv
inventory=docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
if [[ ${TERLAN_RUST_REPORT_MODE:-check} == --record-dependency-impact ]]; then
  if [[ -f src/lib.rs ]]; then
    value=generated-rust
  else
    value=generated
  fi
  printf '%s\n' "$value" > "$dependency"
  printf '%s\n' "$value" > "$headroom"
  printf '%s\n' "$value" > "$inventory"
else
  if [[ -f src/lib.rs ]]; then
    value=generated-rust
  else
    value=generated
  fi
  test "$(cat "$dependency")" = "$value"
  test "$(cat "$headroom")" = "$value"
  test "$(cat "$inventory")" = "$value"
fi
CHECK
  printf 'old-lock\n' > Cargo.lock
  printf 'old-report\n' > docs/quality/RUST_DEPENDENCY_IMPACT.json
  printf 'old-headroom\n' > docs/quality/RUST_FILE_HEADROOM.tsv
  printf 'old-inventory\n' > docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
  git add .
  git commit --quiet -m Initial
  printf 'new-lock\n' > Cargo.lock
  git add Cargo.lock
  if bash "$driver" check > "$temporary/dependency-report-first.log" 2>&1; then
    echo 'Hook admitted a dependency change without its generated report' >&2
    exit 1
  fi
  test "$(cat docs/quality/RUST_DEPENDENCY_IMPACT.json)" = generated
  test "$(cat docs/quality/RUST_FILE_HEADROOM.tsv)" = generated
  test "$(cat docs/quality/RUST_STRING_ERROR_INVENTORY.tsv)" = generated
  test "$(git show :docs/quality/RUST_DEPENDENCY_IMPACT.json)" = old-report
  grep -Fq 'regenerated docs/quality/RUST_DEPENDENCY_IMPACT.json; stage it and retry' \
    "$temporary/dependency-report-first.log"
  git add docs/quality/RUST_DEPENDENCY_IMPACT.json \
    docs/quality/RUST_FILE_HEADROOM.tsv \
    docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
  bash "$driver" check > "$temporary/dependency-report-second.log" 2>&1
  git commit --quiet --no-verify -m Dependency
  mkdir -p src
  printf 'pub fn value() {}\n' > src/lib.rs
  printf 'manually-stale\n' > docs/quality/RUST_DEPENDENCY_IMPACT.json
  printf 'manually-stale\n' > docs/quality/RUST_FILE_HEADROOM.tsv
  printf 'manually-stale\n' > docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
  git add src/lib.rs docs/quality/RUST_DEPENDENCY_IMPACT.json \
    docs/quality/RUST_FILE_HEADROOM.tsv \
    docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
  if bash "$driver" check > "$temporary/dependency-report-rust-first.log" 2>&1; then
    echo 'Hook admitted a Rust source change with a stale staged report' >&2
    exit 1
  fi
  test "$(cat docs/quality/RUST_DEPENDENCY_IMPACT.json)" = generated-rust
  test "$(cat docs/quality/RUST_FILE_HEADROOM.tsv)" = generated-rust
  test "$(cat docs/quality/RUST_STRING_ERROR_INVENTORY.tsv)" = generated-rust
  test "$(git show :docs/quality/RUST_DEPENDENCY_IMPACT.json)" = manually-stale
  grep -Fq 'regenerated docs/quality/RUST_DEPENDENCY_IMPACT.json; stage it and retry' \
    "$temporary/dependency-report-rust-first.log"
  git add docs/quality/RUST_DEPENDENCY_IMPACT.json \
    docs/quality/RUST_FILE_HEADROOM.tsv \
    docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
  bash "$driver" check > "$temporary/dependency-report-rust-second.log" 2>&1
)

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
rg -q -- "^--cache-dir $temporary/terlan-lane-cache/[0-9a-f]{64} check src/Legacy.terl src/LegacyTest.terl src/SpecializedTest.terl$" "$temporary/terlan-lane.calls"
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
# rejected, and a failed unit never records its own success while an earlier
# independent source check remains reusable.
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
test "$(find "$temporary/terlan-lane-failure-receipts" -type f -name '*.passed' | wc -l)" = 1

# Cargo may relink the staged compiler for a different feature graph without
# changing its semantic source inputs. Project receipts use the admitted input
# identity and therefore survive that executable-byte churn.
stable_compiler_identity=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
stable_calls="$temporary/terlan-lane-stable.calls"
stable_receipts="$temporary/terlan-lane-stable-receipts"
stable_cache="$temporary/terlan-lane-stable-cache"
(
  cd "$temporary/terlan-lane"
  TERLAN_LANE_CALLS="$stable_calls" \
    TERLAN_HOOK_COMPILER_INPUT_KEY="$stable_compiler_identity" \
    TERLAN_HOOK_TERLAN_CACHE_DIR="$stable_cache" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$stable_receipts" \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      changes.txt unused "$PWD/terlc" false prechecked.txt specialized.txt \
    > "$temporary/terlan-lane-stable.log" 2>&1
)
stable_call_count=$(wc -l < "$stable_calls")
printf '\n# relinked feature graph\n' >> "$temporary/terlan-lane/terlc"
(
  cd "$temporary/terlan-lane"
  TERLAN_LANE_CALLS="$stable_calls" \
    TERLAN_HOOK_COMPILER_INPUT_KEY="$stable_compiler_identity" \
    TERLAN_HOOK_TERLAN_CACHE_DIR="$stable_cache" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$stable_receipts" \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      changes.txt unused "$PWD/terlc" false prechecked.txt specialized.txt \
    >> "$temporary/terlan-lane-stable.log" 2>&1
)
test "$(wc -l < "$stable_calls")" = "$stable_call_count"
grep -Fq 'reuse test validation for . ' "$temporary/terlan-lane-stable.log"
grep -Fq 'reuse check validation for . ' "$temporary/terlan-lane-stable.log"
if (
  cd "$temporary/terlan-lane"
  TERLAN_HOOK_COMPILER_INPUT_KEY=invalid \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      changes.txt unused "$PWD/terlc" false prechecked.txt specialized.txt \
    > "$temporary/terlan-lane-invalid-identity.log" 2>&1
); then
  echo 'Terlan lane accepted an invalid compiler input identity' >&2; exit 1
fi
grep -Fq 'Invalid staged compiler input identity' \
  "$temporary/terlan-lane-invalid-identity.log"

# Project checks must honor compiler-owned source roots instead of assuming
# every package has a `src` directory.
mkdir -p "$temporary/terlan-lane/manifest-only/tests"
cat > "$temporary/terlan-lane/manifest-only/terlan.toml" <<'MANIFEST'
[package]
name = "manifest-only"
version = "0.0.1"

[build]
source_roots = ["tests"]
MANIFEST
cat > "$temporary/terlan-lane/manifest-only/tests/Only.terl" <<'SOURCE'
module fixture.Only.
pub only(): Bool -> true.
SOURCE
printf '%s\n' manifest-only/tests/Only.terl > "$temporary/terlan-lane/manifest-only-changes.txt"
git -C "$temporary/terlan-lane" add manifest-only
(
  cd "$temporary/terlan-lane"
  TERLAN_LANE_CALLS="$temporary/terlan-manifest-only.calls" \
    TERLAN_HOOK_TERLAN_CACHE_DIR="$temporary/terlan-lane-cache" \
    TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$temporary/terlan-manifest-only-receipts" \
    bash "$(dirname "$driver")/git_hooks/terlan.sh" \
      manifest-only-changes.txt unused "$PWD/terlc" false /dev/null /dev/null \
    > "$temporary/terlan-manifest-only.log" 2>&1
)
rg -q -- "^--cache-dir $temporary/terlan-lane-cache/[0-9a-f]{64} check tests/Only.terl$" \
  "$temporary/terlan-manifest-only.calls"
if rg -q -- 'check src($| )' "$temporary/terlan-manifest-only.calls"; then
  echo 'Hook ignored manifest source roots and assumed src' >&2; exit 1
fi

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

# Source-only checks run in one batch before native tests. Changed compiler
# tests still run before downstream package tests once source admission passes.
mkdir -p "$temporary/terlan-order/compiler/self_host" \
  "$temporary/terlan-order/std/native" \
  "$temporary/terlan-order/tools" \
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
cat > "$temporary/terlan-order/tools/First.terls" <<'SOURCE'
assert(true).
SOURCE
cat > "$temporary/terlan-order/tools/Second.terls" <<'SOURCE'
assert(true).
SOURCE
printf '%s\n' compiler/self_host/Slow.terl std/native/Fast.terl \
  tools/First.terls tools/Second.terls \
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
  'check tools/First.terls tools/Second.terls'
test "$(sed -n '2p' "$temporary/terlan-order.calls")" = \
  'test compiler/self_host/SlowTest.terl --coverage --coverage-source compiler/self_host/Slow.terl --coverage-threshold 100'
test "$(sed -n '3p' "$temporary/terlan-order.calls")" = \
  'test std/native/FastTest.terl --coverage --coverage-source std/native/Fast.terl --coverage-threshold 100'

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

# Rust report reuse is bound to each report's staged inputs and Rust toolchain.
# Unrelated staged files reuse every report; corruption and failed validation
# never grant reuse.
mkdir -p "$temporary/report-cache/scripts/git_hooks" "$temporary/report-cache/bin" \
  "$temporary/report-cache/owned-target/debug" "$temporary/report-cache/docs/quality" \
  "$temporary/report-cache/src"
cp "$(dirname "$driver")/git_hooks/rust_reports.sh" \
  "$temporary/report-cache/scripts/git_hooks/"
real_timeout=$(command -v timeout)
printf 'first\n' > "$temporary/report-cache/input.txt"
printf 'pub fn value() {}\n' > "$temporary/report-cache/src/lib.rs"
printf 'initial-report\n' > \
  "$temporary/report-cache/docs/quality/RUST_DEPENDENCY_IMPACT.json"
printf 'initial-headroom\n' > \
  "$temporary/report-cache/docs/quality/RUST_FILE_HEADROOM.tsv"
printf 'initial-inventory\n' > \
  "$temporary/report-cache/docs/quality/RUST_STRING_ERROR_INVENTORY.tsv"
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
cat > "$temporary/report-cache/bin/timeout" <<'TIMEOUT'
#!/usr/bin/env bash
printf 'timeout:%s\n' "$*" >> "$REPORT_CACHE_CALLS"
exec "$REPORT_REAL_TIMEOUT" "$@"
TIMEOUT
cat > "$temporary/report-cache/owned-target/debug/terlan-quality" <<'QUALITY'
#!/usr/bin/env bash
printf 'quality:%s\n' "$*" >> "$REPORT_CACHE_CALLS"
QUALITY
cat > "$temporary/report-cache/owned-target/debug/terlan-rust-boundary-audit" <<'BOUNDARY'
#!/usr/bin/env bash
printf 'boundary:%s\n' "$*" >> "$REPORT_CACHE_CALLS"
mkdir -p target/quality
touch target/quality/rust-api-boundary-input.tsv \
  target/quality/rust-shared-helper-input.tsv
printf '{}\n' > target/quality/rust-structural-input.json
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
if [[ ${REPORT_VM_BLOCK:-0} == 1 ]]; then
  printf '%s\n' "$$" >> "$REPORT_VM_STARTED"
  trap 'exit 0' TERM INT HUP
  while :; do sleep 1; done
fi
if [[ -n ${REPORT_VM_SLEEP_SECONDS:-} ]]; then
  sleep "$REPORT_VM_SLEEP_SECONDS"
fi
command=${!#}
if [[ "$command" == dependency-impact-check \
  || "$command" == dependency-impact-record ]]; then
  test -s target/quality/rust-structural-input.json
fi
if [[ "$command" == dependency-impact-record ]]; then
  printf 'generated-report\n' > docs/quality/RUST_DEPENDENCY_IMPACT.json
elif [[ "$command" == file-headroom-record ]]; then
  printf 'generated-headroom\n' > docs/quality/RUST_FILE_HEADROOM.tsv
elif [[ "$command" == api-boundary-record ]]; then
  printf 'generated-inventory\n' > docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
fi
exit "${REPORT_VM_FAILURE:-0}"
VM
chmod +x "$temporary/report-cache/bin/"* \
  "$temporary/report-cache/owned-target/debug/"*
git init --quiet --initial-branch=main "$temporary/report-cache"
(
  cd "$temporary/report-cache"
  git config user.name 'Report cache test'
  git config user.email 'reports@example.invalid'
  git add input.txt src/lib.rs scripts/git_hooks/rust_reports.sh \
    docs/quality/RUST_DEPENDENCY_IMPACT.json \
    docs/quality/RUST_FILE_HEADROOM.tsv \
    docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
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
    REPORT_REAL_TIMEOUT="$real_timeout" \
    REPORT_VM_SLEEP_SECONDS="${REPORT_VM_SLEEP_SECONDS:-}" \
    TERLAN_RUST_REPORT_TIMEOUT_SECONDS="${TERLAN_RUST_REPORT_TIMEOUT_SECONDS:-}" \
    TERLAN_HOOK_ARTIFACTS_PREBUILT=1 \
    TERLAN_RUST_REPORT_TERLC="$PWD/owned-target/debug/terlc" \
    TERLAN_RUST_REPORT_VM="$PWD/owned-target/debug/terlan-vm" \
    TERLAN_RUST_REPORT_CACHE_DIR="$PWD/owned-target/validation-results/rust-reports" \
      bash scripts/git_hooks/rust_reports.sh "$@"
  )
}
run_report_cache > "$temporary/report-cache-first.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 6
test "$(grep -c '^timeout:--kill-after=5s 120s ' "$report_cache_calls")" = 6
test "$(grep -c '^quality:' "$report_cache_calls")" = 0
grep -Eq '^terlc:build --incremental .* --out-dir .*/owned-target/terlan-validators/rust-quality$' \
  "$report_cache_calls"
run_report_cache > "$temporary/report-cache-reused.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 6
test "$(grep -c 'Rust quality report unchanged:' "$temporary/report-cache-reused.log")" = 6
receipt=$(find "$temporary/report-cache/owned-target/validation-results/rust-reports" \
  -type f -name '*.passed' -print -quit)
printf 'corrupt\n' > "$receipt"
run_report_cache > "$temporary/report-cache-corrupt.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 7
printf 'second\n' > "$temporary/report-cache/input.txt"
git -C "$temporary/report-cache" add input.txt
run_report_cache > "$temporary/report-cache-tree.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 7
test "$(grep -c 'Rust quality report unchanged:' "$temporary/report-cache-tree.log")" = 6
printf '// unstaged source change\n' >> "$temporary/report-cache/src/lib.rs"
run_report_cache > "$temporary/report-cache-working-tree.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 13
git -C "$temporary/report-cache" restore src/lib.rs
run_report_cache > "$temporary/report-cache-working-tree-restored.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 13
printf '\n# changed toolchain executable\n' >> "$temporary/report-cache/bin/rustc"
run_report_cache > "$temporary/report-cache-tool.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 19
printf 'pub fn changed() {}\n' > "$temporary/report-cache/src/lib.rs"
git -C "$temporary/report-cache" add src/lib.rs
receipt_count=$(find "$temporary/report-cache/owned-target/validation-results/rust-reports" \
  -type f -name '*.passed' | wc -l)
if REPORT_VM_FAILURE=49 run_report_cache > "$temporary/report-cache-failure.log" 2>&1; then
  echo 'Failed Rust report validation sealed a cache receipt' >&2; exit 1
fi
test "$(find "$temporary/report-cache/owned-target/validation-results/rust-reports" \
  -type f -name '*.passed' | wc -l)" = "$receipt_count"
test "$(grep -c '^vm:' "$report_cache_calls")" = 25
run_report_cache > "$temporary/report-cache-recovery.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 31
TERLAN_TEST_FORCE_RERUN=1 run_report_cache > "$temporary/report-cache-forced.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 37
receipt_count=$(find "$temporary/report-cache/owned-target/validation-results/rust-reports" \
  -type f -name '*.passed' | wc -l)
TERLAN_TEST_FORCE_RERUN=1 \
  run_report_cache --record-dependency-impact > "$temporary/report-cache-record.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 43
test "$(find "$temporary/report-cache/owned-target/validation-results/rust-reports" \
  -type f -name '*.passed' | wc -l)" = "$((receipt_count + 3))"
git -C "$temporary/report-cache" add docs/quality/RUST_DEPENDENCY_IMPACT.json \
  docs/quality/RUST_FILE_HEADROOM.tsv \
  docs/quality/RUST_STRING_ERROR_INVENTORY.tsv
: > "$temporary/report-cache/target/quality/rust-structural-input.json"
: > "$report_cache_calls"
run_report_cache > "$temporary/report-cache-after-record.log"
test "$(grep -c '^vm:' "$report_cache_calls")" = 0
test "$(grep -c 'Rust quality report unchanged:' \
  "$temporary/report-cache-after-record.log")" = 6
test ! -s "$report_cache_calls"

# A report deadline is an explicit diagnostic, not an unexplained generic
# failure. Force execution so stale receipts from earlier fixture identities
# cannot accidentally select the cache path instead of the deadline path.
if REPORT_VM_SLEEP_SECONDS=2 TERLAN_RUST_REPORT_TIMEOUT_SECONDS=1 \
  TERLAN_TEST_FORCE_RERUN=1 \
  run_report_cache > "$temporary/report-cache-timeout.log" 2>&1; then
  echo 'Timed-out Rust report validation unexpectedly passed' >&2; exit 1
fi
grep -Fq 'Rust quality report exceeded 1s:' \
  "$temporary/report-cache-timeout.log"

# Report validation admits only two concurrent CPU-heavy workers. Interrupting
# the owner terminates that bounded set and removes its private job directory.
report_vm_started="$temporary/report-cache.started"
: > "$report_vm_started"
(
  cd "$temporary/report-cache"
  exec env \
    PATH="$PWD/bin:$PATH" \
    CARGO_TARGET_DIR="$PWD/owned-target" \
    REPORT_CACHE_CALLS="$report_cache_calls" \
    REPORT_REAL_TIMEOUT="$real_timeout" \
    REPORT_VM_BLOCK=1 \
    REPORT_VM_STARTED="$report_vm_started" \
    TERLAN_TEST_FORCE_RERUN=1 \
    TERLAN_HOOK_ARTIFACTS_PREBUILT=1 \
    TERLAN_RUST_REPORT_TERLC="$PWD/owned-target/debug/terlc" \
    TERLAN_RUST_REPORT_VM="$PWD/owned-target/debug/terlan-vm" \
    TERLAN_RUST_REPORT_CACHE_DIR="$PWD/owned-target/validation-results/rust-reports" \
    bash scripts/git_hooks/rust_reports.sh
) > "$temporary/report-cache-interrupt.log" 2>&1 &
report_owner=$!
for _attempt in {1..200}; do
  [[ $(wc -l < "$report_vm_started") == 2 ]] && break
  sleep 0.01
done
test "$(wc -l < "$report_vm_started")" = 2
kill -TERM "$report_owner"
report_status=0
wait "$report_owner" || report_status=$?
test "$report_status" = 143
while read -r report_vm_pid; do
  ! kill -0 "$report_vm_pid" 2>/dev/null
done < "$report_vm_started"
test -z "$(find "$temporary/report-cache/owned-target/terlan-validators" \
  -maxdepth 1 -type d -name '.rust-report-jobs.*' -print -quit)"

# Exact Cargo selection must stream compilation progress rather than holding all
# output in memory until a long-running test process exits.
mkdir -p "$temporary/exact-cargo/bin"
cat > "$temporary/exact-cargo/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
test "$1" = test
test "$2" = --profile
test "$3" = dev
printf 'Compiling visible-fixture v0.0.0\n'
case "$*" in
  *stream*)
    while [[ ! -f "$EXACT_CARGO_RELEASE" ]]; do sleep 0.01; done
    printf 'running 1 test\n'
    ;;
  *empty*) printf 'running 0 tests\n' ;;
  *failure*) exit 31 ;;
esac
CARGO
chmod +x "$temporary/exact-cargo/bin/cargo"
exact_stream_log="$temporary/exact-cargo-stream.log"
PATH="$temporary/exact-cargo/bin:$PATH" \
  EXACT_CARGO_RELEASE="$temporary/exact-cargo.release" \
  bash "$exact_cargo_test" stream > "$exact_stream_log" 2>&1 &
exact_stream_pid=$!
exact_stream_visible=false
for _attempt in $(seq 1 100); do
  if grep -q 'Compiling visible-fixture' "$exact_stream_log"; then
    exact_stream_visible=true
    break
  fi
  sleep 0.01
done
if [[ "$exact_stream_visible" != true ]]; then
  echo 'Exact Cargo runner withheld compilation progress' >&2
  touch "$temporary/exact-cargo.release"
  wait "$exact_stream_pid" || true
  exit 1
fi
touch "$temporary/exact-cargo.release"
wait "$exact_stream_pid"
if PATH="$temporary/exact-cargo/bin:$PATH" \
  bash "$exact_cargo_test" empty > "$temporary/exact-cargo-empty.log" 2>&1; then
  echo 'Exact Cargo runner accepted an empty selection' >&2; exit 1
fi
grep -q 'matched zero tests' "$temporary/exact-cargo-empty.log"
if PATH="$temporary/exact-cargo/bin:$PATH" \
  bash "$exact_cargo_test" failure > "$temporary/exact-cargo-failure.log" 2>&1; then
  echo 'Exact Cargo runner ignored Cargo failure' >&2; exit 1
fi

echo 'Git hook regression tests passed.'
