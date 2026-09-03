#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [[ ! -f Cargo.toml || ! -d crates/terlan || ! -e .git ]]; then
  echo "build cleanup must run from the Terlan repository root" >&2
  exit 1
fi

dry_run=0
check_partials=0
if [[ "${1:-}" == "--dry-run" ]]; then
  dry_run=1
elif [[ "${1:-}" == "--check-partials" ]]; then
  check_partials=1
elif [[ "$#" -ne 0 ]]; then
  echo "usage: scripts/clean_build_outputs.sh [--dry-run|--check-partials]" >&2
  exit 2
fi

partial_builds() {
  {
    if [[ -d target/self-validation ]]; then
      find "$repo_root/target/self-validation" \
        \( \
          -type f -name '*.tvm.partial' -o \
          -type d -name '*.tvm.lock' -o \
          -type f -name '*.inputs.sha256.tmp.*' -o \
          \( -path '*/.terlan/native-aot/*' -type f -name '.*.tmp' \) \
        \) \
        -print
    fi
    if [[ -d target/tmp ]]; then
      while IFS= read -r marker; do
        dirname "$marker"
      done < <(
        find "$repo_root/target/tmp" -type f \
          -name .terlan-disposable-validation-workspace -print
      )
    fi
    if [[ -d target ]]; then
      find "$repo_root/target" -mindepth 1 -maxdepth 1 \
        \( \
          -name 'terlan-file-*' -o \
          -name 'terlan-directory-*' -o \
          -name 'terlan-doc-format-*' \
        \) \
        -print
    fi
  } | LC_ALL=C sort -u
}

if [[ "$check_partials" -eq 1 ]]; then
  partials="$(partial_builds)"
  if [[ -n "$partials" ]]; then
    echo "validation lifecycle check found orphaned or active partial builds:" >&2
    while IFS= read -r partial; do
      printf '  - %s\n' "${partial#"$repo_root/"}" >&2
    done <<< "$partials"
    exit 1
  fi
  echo "validation lifecycle check found no partial builds"
  exit 0
fi

remove_tree() {
  local relative="$1" absolute
  absolute="$repo_root/$relative"
  [[ -e "$absolute" ]] || return 0
  case "$absolute" in
    "$repo_root"/*) ;;
    *)
      echo "refusing to clean path outside repository: $absolute" >&2
      return 1
      ;;
  esac
  if [[ "$dry_run" -eq 1 ]]; then
    printf 'would remove %s\n' "$relative"
  else
    find "$absolute" -depth -delete
    printf 'removed %s\n' "$relative"
  fi
}

for relative in \
  dist \
  _build \
  target/cloud-deploy-plan-v2-check \
  target/cloud-release-bundle-v1-a \
  target/cloud-release-bundle-v1-b \
  target/release-bundle-check-a \
  target/release-bundle-check-b \
  target/registry-cli-integration-check \
  target/registry-package-archive-check \
  target/registry-protocol-check \
  target/managed-web-toolchain-check \
  proofs/lean/.lake \
  tree-sitter-terlan/node_modules \
  editors/intellij/.gradle \
  editors/intellij/.intellijPlatform \
  editors/intellij/.kotlin \
  editors/intellij/build; do
  remove_tree "$relative"
done

for root in benchmarks crates editors proofs scripts std tests tools; do
  [[ -d "$root" ]] || continue
  while IFS= read -r cache; do
    relative="${cache#"$repo_root/"}"
    remove_tree "$relative"
  done < <(
    find "$repo_root/$root" \
      \( -type d -name .terlan -o -type d -name _build \) \
      -prune -print | LC_ALL=C sort
  )
done

# Interrupted typed-validator builds and marked temporary validation workspaces
# are never reusable. Valid image seals and explicitly owned caches remain
# available for a warm validation cycle.
while IFS= read -r partial; do
  [[ -n "$partial" ]] || continue
  relative="${partial#"$repo_root/"}"
  remove_tree "$relative"
done < <(partial_builds)

if [[ -d std/summaries ]]; then
  while IFS= read -r generated; do
    relative="${generated#"$repo_root/"}"
    if [[ "$dry_run" -eq 1 ]]; then
      printf 'would remove %s\n' "$relative"
    else
      find "$generated" -maxdepth 0 -type f -delete
      printf 'removed %s\n' "$relative"
    fi
  done < <(
    find "$repo_root/std/summaries" -maxdepth 1 -type f \
      \( -name '*.erl' -o -name '*.hrl' \) -print | LC_ALL=C sort
  )
fi
