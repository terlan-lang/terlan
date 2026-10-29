#!/usr/bin/env bash
# Build changed Terlan sources with their package context and execute affected tests.
set -euo pipefail
changes=$1
base=$2
terlc=$3
typed_css_checked=$4
tested_terlan=(scripts/git_hooks/src/git_hooks/PlanTest.terl scripts/self_validation/release_promotion/src/release_promotion/PreparationEnvironmentTest.terl)
run_terlan_test() {
  local previous
  for previous in "${tested_terlan[@]}"; do
    if [[ "$previous" == "$1" ]]; then return 0; fi
  done
  "$terlc" test "$1"
  tested_terlan+=("$1")
}
checked_projects=("")
check_terlan_source() {
  local project previous
  project=$(dirname "$1")
  while [[ ! -f "$project/terlan.toml" && "$project" != . ]]; do project=$(dirname "$project"); done
  if [[ -f "$project/terlan.toml" ]]; then
    for previous in "${checked_projects[@]}"; do
      if [[ "$previous" == "$project" ]]; then return 0; fi
    done
    # Build the package so configured source roots and sibling imports resolve.
    "$terlc" build "$project" --target terlan-vm --out-dir "target/git-hooks-terlan/$project"
    checked_projects+=("$project")
  elif [[ "$1" == *terlan.toml ]]; then
    "$terlc" check "$(dirname "$1")"
  elif [[ -f "$1" && "$1" != *Test.terl ]]; then
    "$terlc" check "$1"
  fi
}

while IFS= read -r path; do
  if [[ "$path" == tests/fixtures/* || "$path" == docs/grammar/fixtures/* ]]; then continue; fi
  # The typed CSS gate checks every generated property in bounded batches
  # and executes the handwritten CSS tests and browser fixture.
  if [[ "$typed_css_checked" == true && "$path" == std/css/* ]]; then continue; fi
  # JavaScript sources execute through the browser matrix below.
  if [[ "$path" == std/js/* || "$path" == tests/js/* ]]; then continue; fi
  if [[ "$path" == *.terl || "$path" == *.terls || "$path" == *.terli || "$path" == *terlan.toml ]]; then
    check_terlan_source "$path"
    # Entry builds resolve the package's configured source roots even when
    # a script lives outside them. Standalone `check` cannot do that.
    if [[ "$path" == *.terls && -f "$path" ]]; then
      "$terlc" build "$path" --target terlan-vm --out-dir "target/git-hooks-terlan/entries/${path%.terls}"
    fi
    if bash scripts/git_hooks/formatting_only.sh "$base" "$path" "$terlc"; then
      # Standalone test files also need type checking when their only
      # change is the formatter's canonical output.
      if [[ "$path" == *Test.terl ]]; then
        "$terlc" build "$path" --target terlan-vm --out-dir "target/git-hooks-terlan/entries/${path%.terl}"
      fi
      echo "pre-commit: canonical formatting only: $path"
      continue
    fi
    if [[ "$path" == *Test.terl && -f "$path" ]]; then run_terlan_test "$path";
    elif [[ -f "${path%.terl}Test.terl" ]]; then run_terlan_test "${path%.terl}Test.terl"; fi
  fi
done < "$changes"
