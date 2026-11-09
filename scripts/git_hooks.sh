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
    test -x .githooks/commit-msg
    git config --local core.hooksPath .githooks
    echo 'Installed staged-source and short-message commit checks for this checkout.'
    exit 0
    ;;
  check) ;;
  *) echo 'Usage: bash scripts/git_hooks.sh [install|check]' >&2; exit 2 ;;
esac

# Publication validation is tied to one exact main revision. Share the final
# publication lease with every worktree so a commit cannot silently invalidate
# an hours-long candidate while `make publish` is waiting for hosted evidence.
git_common=$(git rev-parse --path-format=absolute --git-common-dir)
publication_lock="$git_common/terlan-publication.lock"
if [[ ! -d "$git_common" || -L "$git_common"
  || -L "$publication_lock"
  || ( -e "$publication_lock" && ! -f "$publication_lock" ) ]]; then
  echo 'pre-commit: invalid shared publication lease path' >&2
  exit 1
fi
exec 7>>"$publication_lock"
if ! flock --exclusive --nonblock 7; then
  echo 'pre-commit: publication owns main; wait for it to finish' >&2
  exit 1
fi
[[ /dev/fd/7 -ef "$publication_lock" ]] || exit 1

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
raw_changes="$cache/changed-paths.raw.txt"
changes="$cache/changed-paths.txt"
git diff --no-renames --name-only -z "$base" "$tree" > "$cache/changed-paths.nul"
: > "$raw_changes"
while IFS= read -r -d '' path; do
  case "$path" in
    *$'\n'*|*$'\r'*) echo 'Pre-commit paths must not contain newlines.' >&2; exit 1 ;;
  esac
  printf '%s\n' "$path" >> "$raw_changes"
done < "$cache/changed-paths.nul"
node scripts/git_hooks/filter_version_changes.cjs "$base" "$tree" "$raw_changes" "$changes"

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
  # The staged checkout reuses ordinary Cargo artifacts, but incremental state
  # is both checkout-specific and disproportionately large. Default it off so
  # a workspace test lane cannot exhaust disk after Clippy and release builds.
  # An explicit caller setting remains authoritative for diagnostics.
  export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
  if [[ "$CARGO_TARGET_DIR" != "$cache/build" ]]; then
    export TERLAN_HOOK_SHARED_TARGET=1
  else
    unset TERLAN_HOOK_SHARED_TARGET
  fi
  # Unrelated global Git checkouts must not enter the staged resolver inventory.
  if [[ -z "${CARGO_HOME:-}" ]]; then
    test ! -L "$cache/cargo-home"
    export CARGO_HOME="$cache/cargo-home"
  else
    case "$CARGO_HOME" in /*) ;; *) export CARGO_HOME="$root/$CARGO_HOME" ;; esac
  fi
  unset TERLAN_UPDATE_PHASE_GOLDEN
  cd "$snapshot"
  bash scripts/git_hooks/check.sh "$changes" "$base"
  # Validation must never silently repair the source it is approving.
  git diff --exit-code
  test "$(git write-tree)" = "$tree"
)
if [[ "$(git write-tree)" != "$tree" || "$(git rev-parse --verify HEAD 2>/dev/null || git hash-object -t tree /dev/null)" != "$base" ]]; then
  echo 'The index or HEAD changed during validation; retry the commit.' >&2
  exit 1
fi
echo "pre-commit: passed staged tree $tree"
