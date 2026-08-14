#!/usr/bin/env bash
set -euo pipefail

# Verifies that the source release does not track local caches, logs, or scratch
# outputs. Internal documentation is allowed in the source repository; the
# staged public-documentation manifest owns exclusion from installed artifacts.
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ ! -f Cargo.toml || ! -d crates/terlan ]]; then
  echo "release boundary check must run from the published repository root"
  exit 1
fi

scratch_file_pattern='(^|/)(__pycache__|\.terlan)(/|$)|\.pyc$|\.pyo$|\.log$|\.tmp$|\.beam$|^std/summaries/.*\.(erl|hrl)$'
internal_tree_pattern='^(\.agents/|\.codex/|scratch/|gen/|proofs/lean/\.lake/)'
internal_file_pattern='^\.github/README\.md$'

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
  git ls-files --others --exclude-standard \
    | grep -E "$scratch_file_pattern|$internal_tree_pattern|$internal_file_pattern|^scripts/check_0_0_[0-3]" \
    || true
)"

if [[ -n "$working_scratch" ]]; then
  echo "release boundary check failed: local scratch/cache output found"
  echo "$working_scratch"
  exit 1
fi
