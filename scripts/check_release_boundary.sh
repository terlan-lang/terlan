#!/usr/bin/env bash
set -euo pipefail

# Verifies that the published repository does not contain local caches, logs,
# scratch outputs, or internal planning artifacts that are not release-owned.
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ ! -f Cargo.toml || ! -d crates/terlan ]]; then
  echo "release boundary check must run from the published repository root"
  exit 1
fi

scratch_file_pattern='(^|/)(__pycache__)(/|$)|\.pyc$|\.pyo$|\.log$|\.tmp$|\.beam$|^std/summaries/.*\.(erl|hrl)$'
internal_tree_pattern='^(\.agents/|\.codex/|scratch/|gen/|proofs/|docs/roadmap/|docs/compiler/|docs/internal/|roadmap/)'
internal_file_pattern='(^|/)(ROADMAP|CHECKPOINT|BASELINE|BOOTSTRAP|TODO)[^/]*\.md$|^\.github/README\.md$'

tracked_scratch="$(
  git ls-files \
    | grep -E "$scratch_file_pattern|$internal_tree_pattern|$internal_file_pattern" \
    | while IFS= read -r path; do
        if [[ -e "$path" ]]; then
          echo "$path"
        fi
      done \
    || true
)"

if [[ -n "$tracked_scratch" ]]; then
  echo "release boundary check failed: tracked scratch/cache output found"
  echo "$tracked_scratch"
  exit 1
fi

stale_release_surface="$(
  {
    git ls-files 'scripts/check_0_0_[0-3]*' \
      | while IFS= read -r path; do
          if [[ -e "$path" ]]; then
            echo "$path"
          fi
        done \
      || true
    grep -En 'release-0-0-[0-3]' Makefile crates/terlan/cli.mk .github/workflows/*.yml 2>/dev/null || true
  } | sed '/^$/d'
)"

if [[ -n "$stale_release_surface" ]]; then
  echo "release boundary check failed: stale pre-0.0.4 release surface found"
  echo "$stale_release_surface"
  exit 1
fi

working_scratch="$(
  find . \
    -path './.git' -prune -o \
    -path './target' -prune -o \
    -path './dist' -prune -o \
    \( \
      -path '*/__pycache__/*' \
      -o -name '*.pyc' \
      -o -name '*.pyo' \
      -o -name '*.log' \
      -o -name '*.tmp' \
      -o -name '*.beam' \
      -o -path './std/summaries/*.erl' \
      -o -path './std/summaries/*.hrl' \
      -o -path './scripts/check_0_0_[0-3]*' \
      -o -path './scratch/*' \
      -o -path './gen/*' \
      -o -path './proofs/*' \
      -o -path './docs/roadmap/*' \
      -o -path './docs/compiler/*' \
      -o -path './docs/internal/*' \
      -o -path './roadmap/*' \
      -o -path './.github/README.md' \
    \) \
    -print \
    | sed 's#^\./##' \
    || true
)"

if [[ -n "$working_scratch" ]]; then
  echo "release boundary check failed: local scratch/cache output found"
  echo "$working_scratch"
  exit 1
fi
