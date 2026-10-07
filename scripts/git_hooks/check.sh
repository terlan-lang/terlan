#!/usr/bin/env bash
# Execute the Terlan-owned plan inside the staged-source validation clone.
set -euo pipefail
changes=$1
echo 'pre-commit: Rust formatting'
cargo fmt --all -- --check
echo 'pre-commit: build the staged compiler and VM'
cargo build --locked -p terlan --bin terlc --bin terlan-vm
terlc="$CARGO_TARGET_DIR/debug/terlc"
echo 'pre-commit: hook policy and snapshot regression tests'
"$terlc" test scripts/git_hooks/src/git_hooks/PlanTest.terl
bash scripts/git_hooks_test.sh
tested_terlan=(scripts/git_hooks/src/git_hooks/PlanTest.terl)
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
plan="${changes%.txt}.plan"
"$terlc" run scripts/git_hooks/scripts/Plan.terls -- "$changes" > "$plan"
while IFS= read -r lane; do
  echo "pre-commit: $lane"
  case "$lane" in
    bootstrap)
      if [[ $(uname -s) == Linux ]]; then
        cargo test --locked -p terlan-build-cache --test support_bootstrap_make
      else
        echo 'pre-commit: Linux bootstrap integration tests require a Linux host'
      fi
      ;;
    clippy)
      make --no-print-directory rust-clippy-check
      ;;
    rust-reports)
      bash scripts/git_hooks/rust_reports.sh
      ;;
    summaries)
      bash scripts/git_hooks/stdlib_summaries.sh
      ;;
    shell)
      while IFS= read -r path; do
        if [[ -f "$path" && ( "$path" == *.sh || "$path" == .githooks/* ) ]]; then bash -n "$path"; fi
      done < "$changes"
      ;;
    phase)
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib tests::run_phase_contract_fixtures_match_golden -- --exact
      ;;
    syntax|compiler)
      filter=compiler::
      if [[ "$lane" == syntax ]]; then filter=compiler::syntax::; fi
      bash scripts/run_exact_cargo_test.sh --locked -p terlan --lib "$filter"
      ;;
    workspace)
      bash scripts/run_exact_cargo_test.sh --locked --workspace --all-targets
      ;;
    editor)
      (cd tree-sitter-terlan && npm ci && npm run check && npm run check:cli)
      ;;
    terlan)
      while IFS= read -r path; do
        if [[ "$path" == tests/fixtures/* || "$path" == docs/grammar/fixtures/* ]]; then continue; fi
        if [[ "$path" == *.terl || "$path" == *.terls || "$path" == *.terli || "$path" == *terlan.toml ]]; then
          check_terlan_source "$path"
          # Package builds do not include script entry files outside source roots.
          if [[ "$path" == *.terls && -f "$path" ]]; then "$terlc" check "$path"; fi
          if [[ "$path" == *Test.terl && -f "$path" ]]; then run_terlan_test "$path";
          elif [[ -f "${path%.terl}Test.terl" ]]; then run_terlan_test "${path%.terl}Test.terl"; fi
        fi
      done < "$changes"
      ;;
    workflow)
      go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 -color=false .github/workflows/*.yml
      ;;
    '') ;;
    *) echo "Unknown pre-commit lane: $lane" >&2; exit 1 ;;
  esac
done < "$plan"
