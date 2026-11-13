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
validation_lock="$cache/validation.lock"
if [[ -L "$validation_lock" || ( -e "$validation_lock" && ! -f "$validation_lock" ) ]]; then
  echo "Invalid staged validation lock path: $validation_lock" >&2
  exit 1
fi
exec 8>>"$validation_lock"
if ! flock --exclusive --nonblock 8; then
  echo "Another staged validation owns $validation_lock; wait for it to finish." >&2
  exit 1
fi
[[ /dev/fd/8 -ef "$validation_lock" ]] || exit 1
trap 'exit 130' INT
trap 'exit 143' TERM
validation_schema=terlan-staged-tree-validation-v1
validation_receipt_dir="$cache/validation-results/staged-trees"
if [[ -z ${CARGO_HOME:-} && -L "$cache/cargo-home" ]]; then
  echo "Invalid default Cargo cache path: $cache/cargo-home" >&2
  exit 1
fi
test ! -L "$cache/validation-results"
test ! -L "$validation_receipt_dir"
mkdir -p "$validation_receipt_dir"
validation_key="$({
  printf '%s\n' "$validation_schema" "$tree" "$base"
  printf '%s\n' \
    "CARGO_HOME=${CARGO_HOME:-<default>}" \
    "CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-<default>}" \
    "CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-<default>}"
  for executable in "$(command -v cargo)" "$(command -v rustc)"; do
    printf '%s:%s\n' "$executable" "$(git hash-object "$executable")"
  done
  cargo -Vv
  rustc -Vv
} | git hash-object --stdin)"
validation_receipt="$validation_receipt_dir/$validation_key.passed"
if [[ ${TERLAN_TEST_FORCE_RERUN:-0} != 1 \
  && -f "$validation_receipt" \
  && $(wc -l < "$validation_receipt") == 2 \
  && $(sed -n '1p' "$validation_receipt") == "$validation_schema" \
  && $(sed -n '2p' "$validation_receipt") == "$validation_key" ]]; then
  echo "pre-commit: reuse passed staged tree $tree"
  exit 0
fi
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

# Rust compilation inputs and the report generator own the checked
# dependency-impact report. Generate a missing update from the exact staged
# snapshot, copy it into the real working tree, and stop so the generated file
# cannot be omitted from the commit.
dependency_report=docs/quality/RUST_DEPENDENCY_IMPACT.json
dependency_report_refresh=false
if rg -q '\.rs$|(^|/)Cargo\.(toml|lock)$|(^|/)(rustfmt|clippy)\.toml$|^rust-toolchain\.toml$|^\.cargo/|^scripts/self_validation/rust_quality/|^scripts/git_hooks/rust_reports\.sh$' "$changes" \
  && ! rg -Fqx -- "$dependency_report" "$changes"; then
  if ! git diff --quiet -- "$dependency_report"; then
    echo "pre-commit: $dependency_report is already modified; stage it before retrying" >&2
    exit 1
  fi
  dependency_report_refresh=true
fi

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
  export TERLAN_HOOK_VALIDATION_RECEIPT_DIR="$cache/validation-results/terlan"
  # Commit admission intentionally uses the newest locally built scripting
  # compiler. Candidate validation separately proves the staged compiler.
  if [[ -x "$root/target/debug/terlc" && ! -L "$root/target/debug/terlc" ]]; then
    export TERLAN_HOOK_LATEST_TERLC="$root/target/debug/terlc"
  else
    unset TERLAN_HOOK_LATEST_TERLC
  fi
  # Repository-level Terlan tests use the conventional target/debug paths.
  # Share only the hook-owned binaries; keep generated evidence inside this
  # disposable staged checkout and never copy the external build directory.
  test ! -L "$snapshot/target"
  mkdir -p "$snapshot/target"
  test ! -e "$snapshot/target/debug"
  test ! -L "$snapshot/target/debug"
  ln -s "$CARGO_TARGET_DIR/debug" "$snapshot/target/debug"
  test "$(readlink "$snapshot/target/debug")" = "$CARGO_TARGET_DIR/debug"
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
  if [[ "$dependency_report_refresh" == true ]]; then
    export TERLAN_RUST_REPORT_MODE=--record-dependency-impact
  else
    unset TERLAN_RUST_REPORT_MODE
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
  if [[ ${TERLAN_HOOK_MODE:-fast} == fast ]]; then
    fast_timeout=${TERLAN_HOOK_FAST_TIMEOUT_SECONDS:-60}
    case "$fast_timeout" in ''|*[!0-9]*) echo 'Invalid fast admission timeout' >&2; exit 2 ;; esac
    timeout --kill-after=5s "${fast_timeout}s" \
      bash scripts/git_hooks/check.sh "$changes" "$base"
  else
    bash scripts/git_hooks/check.sh "$changes" "$base"
  fi
  if [[ "$dependency_report_refresh" == true ]] \
    && ! git diff --quiet -- "$dependency_report"; then
    test "$(git diff --name-only)" = "$dependency_report"
    test -f "$dependency_report"
    test -f "$root/$dependency_report"
    test ! -L "$dependency_report"
    test ! -L "$root/$dependency_report"
    cp -- "$dependency_report" "$root/$dependency_report"
    echo "pre-commit: regenerated $dependency_report; stage it and retry" >&2
    exit 1
  fi
  # Every other validation path remains read-only.
  git diff --exit-code
  test "$(git write-tree)" = "$tree"
)
if [[ "$(git write-tree)" != "$tree" || "$(git rev-parse --verify HEAD 2>/dev/null || git hash-object -t tree /dev/null)" != "$base" ]]; then
  echo 'The index or HEAD changed during validation; retry the commit.' >&2
  exit 1
fi
temporary_receipt=$(mktemp "$validation_receipt_dir/.staged-tree.XXXXXX")
printf '%s\n%s\n' "$validation_schema" "$validation_key" > "$temporary_receipt"
mv "$temporary_receipt" "$validation_receipt"
echo "pre-commit: passed staged tree $tree"
