#!/usr/bin/env bash
# Exercise the Git boundary with tiny repositories and a staged fixture checker.
set -euo pipefail
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
echo 'Git hook snapshot regression tests passed (8 scenarios).'
