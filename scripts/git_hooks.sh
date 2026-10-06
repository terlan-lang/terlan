#!/usr/bin/env bash
# Git plumbing stays in Bash; check selection belongs to the Terlan planner.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"

case "${1:-check}" in
  install)
    previous=$(git config --get core.hooksPath || true)
    if [[ -n "$previous" && "$previous" != .githooks ]]; then
      echo "Existing core.hooksPath=$previous; integrate its hooks before installing." >&2
      exit 1
    fi
    if [[ -z "$previous" && -f $(git rev-parse --git-path hooks/pre-commit) ]]; then
      echo 'An existing pre-commit hook must be integrated before installing.' >&2
      exit 1
    fi
    test -x .githooks/pre-commit
    git config --local core.hooksPath .githooks
    echo 'Installed staged-source pre-commit checks for this checkout.'
    exit 0
    ;;
  check) ;;
  *) echo 'Usage: bash scripts/git_hooks.sh [install|check]' >&2; exit 2 ;;
esac

git diff --cached --check
tree=$(git write-tree)
base=$(git rev-parse --verify HEAD 2>/dev/null || git hash-object -w -t tree /dev/null)
if [[ "$tree" == "$(git rev-parse "$base^{tree}")" ]]; then
  echo 'pre-commit: no staged changes'
  exit 0
fi

cache="$root/target/git-hooks"
test ! -L "$root/target"
test ! -L "$cache"
mkdir -p "$cache"
if ! mkdir "$cache/lock" 2>/dev/null; then
  echo "Another staged validation owns $cache/lock; wait for it to finish." >&2
  exit 1
fi
trap 'rmdir "$cache/lock"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
changes="$cache/changed-paths.txt"
git diff --no-renames --name-only -z "$base" "$tree" > "$cache/changed-paths.nul"
: > "$changes"
while IFS= read -r -d '' path; do
  case "$path" in
    *$'\n'*|*$'\r'*) echo 'Pre-commit paths must not contain newlines.' >&2; exit 1 ;;
  esac
  printf '%s\n' "$path" >> "$changes"
done < "$cache/changed-paths.nul"

snapshot="$cache/source"
echo "pre-commit: validating staged tree $tree in $snapshot"
(
  # Git sets these for hooks (including a temporary index for `commit --only`).
  # Child Git commands must belong to the validation clone, not the user's index.
  while IFS= read -r variable; do unset "$variable"; done < <(git rev-parse --local-env-vars)
  test ! -L "$snapshot"
  if [[ ! -d "$snapshot" ]]; then
    git clone --quiet --shared --no-checkout -- "$root" "$snapshot"
  fi
  test "$(git -C "$snapshot" remote get-url origin)" = "$root"
  test -d "$snapshot/.git"
  # Only this hook-owned clone is reset/cleaned. Build caches live beside it.
  git -C "$snapshot" read-tree --reset -u "$tree"
  git -C "$snapshot" clean -ffdqx
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$cache/build}"
  case "$CARGO_TARGET_DIR" in /*) ;; *) export CARGO_TARGET_DIR="$root/$CARGO_TARGET_DIR" ;; esac
  unset TERLAN_UPDATE_PHASE_GOLDEN
  cd "$snapshot"
  bash scripts/git_hooks/check.sh "$changes"
  # Validation must never silently repair the source it is approving.
  git diff --exit-code
  test "$(git write-tree)" = "$tree"
)
if [[ "$(git write-tree)" != "$tree" || "$(git rev-parse --verify HEAD 2>/dev/null || git hash-object -t tree /dev/null)" != "$base" ]]; then
  echo 'The index or HEAD changed during validation; retry the commit.' >&2
  exit 1
fi
echo "pre-commit: passed staged tree $tree"
