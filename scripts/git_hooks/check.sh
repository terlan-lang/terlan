#!/usr/bin/env bash
# Execute the Terlan-owned plan inside the staged-source validation clone.
set -euo pipefail
changes=$1
base=$2
echo 'pre-commit: Rust dependency security audit'
make --no-print-directory rust-security-audit-check
echo 'pre-commit: editor release contracts'
make --no-print-directory editor-release-contract-check
bash scripts/build_artifact_bootstrap_test.sh
echo 'pre-commit: Rust formatting'
cargo fmt --all -- --check
echo 'pre-commit: build the staged compiler, VM, native worker, and service runtime'
cargo build --locked -p terlan --bin terlc --bin terlan-vm --bin terlan-native-worker
cargo build --locked -p terlan --bin terlan-serve-runtime --no-default-features --features serve-runtime-bin
terlc="$CARGO_TARGET_DIR/debug/terlc"
export TERLC="$terlc"
echo 'pre-commit: canonical executable size budget'
cargo build --release --locked -p terlan -p terlan-test-orchestrator \
  --bin terlc --bin terlan-vm --bin terlan-native-worker --bin terlan-test-orchestrator
# Reuse the optimized compiler already required by the executable budget.
terlc="$CARGO_TARGET_DIR/release/terlc"
export TERLC="$terlc"
cargo build --locked -p terlan-test-orchestrator
TERLAN_REPOSITORY_ROOT="$PWD" "$terlc" run scripts/self_validation/BuildArtifactBudgetTest.terl \
  -- --check-executables "$CARGO_TARGET_DIR"
echo 'pre-commit: standard-library release manifest'
"$terlc" build tests/js/fixtures/JsConstructorTypesTest.terl --target js.browser \
  --out-dir target/git-hooks-js-constructor-types
TERLAN_RELEASE_MANIFEST_ROOT="$PWD" TERLAN_RELEASE_MANIFEST_TERLC="$terlc" \
  "$terlc" test std/scripts/ReleaseManifestTest.terl
echo 'pre-commit: proof release baseline'
cargo build --locked -p terlan --bin terlan-quality --features quality-tools
bash scripts/git_hooks/proof_baseline.sh
echo 'pre-commit: Terlan formatting'
make --no-print-directory -f crates/terlan/cli.mk terlan-format-check TERLAN_FORMAT_COMPILER="$terlc"
typed_css_checked=false
if rg -q '^(std/css/|tools/css/|tools/web-toolchain/css-bundle|scripts/check_typed_css\.sh)' "$changes"; then
  echo 'pre-commit: typed CSS generation, types, native values, and browser build'
  npm ci --ignore-scripts --prefix tools/css
  npm ci --ignore-scripts --prefix tools/web-toolchain
  TERLC="$terlc" bash scripts/check_typed_css.sh
  typed_css_checked=true
fi
echo 'pre-commit: hook policy and snapshot regression tests'
"$terlc" test scripts/git_hooks/src/git_hooks/PlanTest.terl
bash scripts/git_hooks_test.sh
bash scripts/git_commit_message_test.sh
TERLC="$terlc" bash scripts/git_hooks_formatting_test.sh
plan="${changes%.txt}.plan"
"$terlc" run scripts/git_hooks/scripts/Plan.terls -- "$changes" > "$plan"
while IFS= read -r lane; do
  echo "pre-commit: $lane"
  case "$lane" in
    proofs)
      cargo build --locked -p terlan --bin terlan-quality --features quality-tools
      bash scripts/git_hooks/proofs.sh
      ;;
    bootstrap)
      if [[ $(uname -s) == Linux ]]; then
        cargo test --locked -p terlan-build-cache --test support_bootstrap_make
        cargo test --locked -p terlan-test-orchestrator --test hosted_release_make
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
      bash scripts/git_hooks/terlan.sh "$changes" "$base" "$terlc" "$typed_css_checked"
      ;;
    workflow)
      go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 -color=false .github/workflows/*.yml
      ;;
    release-contract)
      cargo build --locked -p terlan-test-orchestrator
      "$CARGO_TARGET_DIR/debug/terlan-test-orchestrator" --install-snapshot "$PWD/target/validation-tools/terlan-test-orchestrator"
      make --no-print-directory repository-build-release-contract-check \
        TERLAN_VALIDATION_BOOTSTRAPPED=1 \
        "TERLAN_REPOSITORY_VALIDATION=\"$terlc\" run scripts/self_validation/RepositoryValidation.terls --"
      ;;
    '') ;;
    *) echo "Unknown pre-commit lane: $lane" >&2; exit 1 ;;
  esac
done < "$plan"

echo 'pre-commit: complete Playwright browser matrix'
bash scripts/git_hooks/js_browser.sh
