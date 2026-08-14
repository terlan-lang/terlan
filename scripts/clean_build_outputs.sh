#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [[ ! -f Cargo.toml || ! -d crates/terlan || ! -d .git ]]; then
  echo "build cleanup must run from the Terlan repository root" >&2
  exit 1
fi

dry_run=0
if [[ "${1:-}" == "--dry-run" ]]; then
  dry_run=1
elif [[ "$#" -ne 0 ]]; then
  echo "usage: scripts/clean_build_outputs.sh [--dry-run]" >&2
  exit 2
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
