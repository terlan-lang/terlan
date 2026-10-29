#!/usr/bin/env bash
# Exercise the Git boundary with tiny repositories and a staged fixture checker.
set -euo pipefail
unset TERLAN_HOOK_SHARED_TARGET
driver="$(cd "$(dirname "$0")" && pwd)/git_hooks.sh"
mkdir -p target
temporary=$(mktemp -d "$(pwd)/target/git-hooks-test.XXXXXX")
trap 'result=$?; if (( result != 0 )); then cat "$temporary"/*.log >&2; fi; rm -rf "$temporary"' EXIT
while IFS= read -r variable; do unset "$variable"; done < <(git rev-parse --local-env-vars)
git init --quiet --initial-branch=main "$temporary/unborn"
(
  cd "$temporary/unborn"
  mkdir -p scripts/git_hooks
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
cat > scripts/git_hooks/check.sh <<'CHECK'
set -eu
test "$(cat tracked.txt)" = good
test -z "${GIT_INDEX_FILE:-}"
test "$2" = "$(git -C "$(git remote get-url origin)" rev-parse HEAD)"
if test -n "${EXPECTED_HOOK_CARGO_HOME:-}"; then
  test "$CARGO_HOME" = "$EXPECTED_HOOK_CARGO_HOME"
fi
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

# Default Cargo inputs are isolated; explicit relative overrides remain supported.
env -u CARGO_HOME EXPECTED_HOOK_CARGO_HOME="$PWD/target/git-hooks/cargo-home" \
  bash "$driver" check > "$temporary/cargo-default.log" 2>&1
CARGO_HOME=custom-cache EXPECTED_HOOK_CARGO_HOME="$PWD/custom-cache" \
  bash "$driver" check > "$temporary/cargo-explicit.log" 2>&1

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
mkdir target/git-hooks/lock
if bash "$driver" check > "$temporary/locked.log" 2>&1; then
  echo 'Hook ignored the active snapshot lock' >&2; exit 1
fi
rmdir target/git-hooks/lock

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
cat > "$temporary/early-bindings/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
case " $* " in *' --release '*) exit 99 ;; esac
exit 0
CARGO
printf '#!/usr/bin/env bash\nexit 0\n' > "$temporary/early-bindings/bin/make"
printf 'exit 0\n' > "$temporary/early-bindings/scripts/build_artifact_bootstrap_test.sh"
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
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
if [[ "$*" == 'run scripts/self_validation/EbnfValidator.terls' ]]; then exit 0; fi
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
if [[ "$*" == 'run scripts/self_validation/EbnfValidator.terls' ]]; then exit 0; fi
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
# Native generation drift must stop before optimized builds. The real planner
# separately tests the changed-path classification; this checks execution order
# and failure propagation through the actual hook checker.
cat > "$temporary/early-bindings/cache/debug/terlc" <<'TERLC'
#!/usr/bin/env bash
set -eu
case "$*" in
  'run scripts/self_validation/EbnfValidator.terls'|'test scripts/self_validation/JsBindingsDriftTest.terl'|'run scripts/self_validation/StdlibValidation.terls -- negative-api'|'run scripts/self_validation/StdlibValidation.terls -- io-negative-api') exit 0 ;;
  'run scripts/git_hooks/scripts/Plan.terls -- changes.txt')
    printf 'shell\n'
    if [[ "$HOOK_NATIVE_SELECTED" == yes ]]; then printf 'native-bindings\n'; fi
    ;;
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
    expected=99 # The stand-in rejects the optimized build if reached.
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
echo 'Git hook regression tests passed (18 scenarios).'
