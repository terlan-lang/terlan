#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "build/release contract failed: $*" >&2
  exit 1
}

required_files=(
  Makefile
  rust-toolchain.toml
  .github/workflows/ci.yml
  .github/workflows/docs.yml
  .github/workflows/release.yml
  .github/workflows/security.yml
  scripts/check_release_boundary.sh
  scripts/build_typed_validator.sh
  scripts/run_prebuilt_terlan_binary.sh
  scripts/clean_build_outputs.sh
  scripts/check_release_artifact_set.sh
  scripts/download_validated_release_artifacts.sh
  scripts/publish_release_from_dist.sh
)
for required_file in "${required_files[@]}"; do
  [[ -f "$required_file" ]] || fail "missing $required_file"
done

bash scripts/build_typed_validator.sh self-test
bash scripts/clean_build_outputs.sh --dry-run >/dev/null

grep -q '^channel = "1\.96\.0"$' rust-toolchain.toml \
  || fail "workspace Rust toolchain is not pinned to 1.96.0"
grep -q '^CARGO := cargo --locked$' Makefile \
  || fail "Make must default every Cargo invocation to locked dependency resolution"
grep -Fq '$(CARGO) clippy --locked --workspace --bins -- -D warnings' mk/code-quality.mk \
  || fail "default Clippy validation does not reject every compiler warning"
grep -Fq '$(CARGO) clippy --locked --workspace --bins --all-features -- -D warnings' mk/code-quality.mk \
  || fail "all-feature Clippy validation does not reject every compiler warning"
grep -Fq $'release-candidate-check: build-artifact-budget-record\n\t$(MAKE) check' Makefile \
  || fail "release-candidate validation must measure clean artifacts before the canonical suite"
grep -Fq 'VALIDATION_FEATURES: &str = "quality-tools,editor-lsp,benchmark-tools"' \
  crates/terlan-test-orchestrator/src/main.rs \
  || fail "feature-gated Rust validation does not share one compiled feature profile"
if grep -REn '\$\(CARGO\) run( --locked)? -p terlan --bin (terlc|terlan-quality|terlan-benchmark)' \
  Makefile crates/terlan/cli.mk editors/editor.mk mk std/stdlib.mk; then
  fail "Make recipes must execute freshness-checked prebuilt Terlan binaries"
fi

release_plan="$(mktemp)"
trap 'rm -f "$release_plan"' EXIT
make -n release-candidate-check > "$release_plan"
orchestrator_runs="$(grep -c 'target/debug/terlan-test-orchestrator' "$release_plan" || true)"
[[ "$orchestrator_runs" -eq 1 ]] \
  || fail "release candidate must execute exactly one canonical Rust test orchestrator"
replayed_cargo_tests="$({
  grep -Ec '(^|[[:space:]])cargo( --locked)? test([[:space:]]|$)' "$release_plan" || true
})"
[[ "$replayed_cargo_tests" -eq 0 ]] \
  || fail "release candidate replays $replayed_cargo_tests Cargo test commands after the canonical suite"
if grep -q 'run_exact_cargo_test.sh' "$release_plan"; then
  fail "release candidate replays exact Cargo tests after the canonical suite"
fi

action_yaml_files=(.github/workflows/*.yml .github/actions/*/action.yml)

mutable_actions="$({
  grep -HEn '^[[:space:]]*uses:[[:space:]]+[^[:space:]]+@(v[0-9]+|main|master)([[:space:]]|$)' \
    "${action_yaml_files[@]}" || true
})"
[[ -z "$mutable_actions" ]] \
  || fail "GitHub Actions must use immutable commit SHAs:\n$mutable_actions"

invalid_action_refs="$({
  grep -hE '^[[:space:]]*uses:' "${action_yaml_files[@]}" \
    | sed -E 's/^[[:space:]]*uses:[[:space:]]+//' \
    | sed -E 's/[[:space:]]+#.*$//' \
    | grep -Ev '^(\./.*|[^[:space:]]+@[0-9a-f]{40})$' || true
})"
[[ -z "$invalid_action_refs" ]] \
  || fail "external GitHub Action references must end in a 40-character SHA:\n$invalid_action_refs"

checkout_count="$(grep -hEc '^[[:space:]]*uses: actions/checkout@' .github/workflows/*.yml | awk '{ total += $1 } END { print total + 0 }')"
credential_opt_out_count="$(grep -hEc '^[[:space:]]*persist-credentials: false$' .github/workflows/*.yml | awk '{ total += $1 } END { print total + 0 }')"
[[ "$checkout_count" -eq "$credential_opt_out_count" ]] \
  || fail "every checkout must disable persisted Git credentials"

for workflow in .github/workflows/*.yml; do
  missing_timeouts="$(awk '
    /^jobs:$/ { in_jobs = 1; next }
    in_jobs && /^  [A-Za-z0-9_-]+:$/ {
      if (job != "" && !timeout) print job
      job = $1
      sub(/:$/, "", job)
      timeout = 0
      next
    }
    in_jobs && /^    timeout-minutes:/ { timeout = 1 }
    END { if (job != "" && !timeout) print job }
  ' "$workflow")"
  [[ -z "$missing_timeouts" ]] \
    || fail "$workflow jobs missing timeout-minutes: $missing_timeouts"
done

if grep -RFn 'rustup toolchain install stable' .github/workflows; then
  fail "workflows must not float the workspace Rust toolchain"
fi
if grep -REn '(>[[:space:]]*/tmp/|rm -rf[[:space:]]+/tmp/|DIR[[:space:]]*\?=[[:space:]]*/tmp/)' \
  Makefile crates/terlan/cli.mk editors/editor.mk mk std/stdlib.mk; then
  fail "Make recipes must keep owned temporary build trees under target/"
fi
grep -Fq 'npm ci --prefix tree-sitter-terlan --no-audit --no-fund' editors/editor.mk \
  || fail "Tree-sitter validation must bootstrap exact locked package dependencies"
grep -Fq 'NPM_PACK_CACHE ?= $(CURDIR)/target/tmp/npm-cache' editors/editor.mk \
  || fail "Node package caches must remain in the repository-owned build tree"

release_workflow="$(cat .github/workflows/release.yml)"
[[ "$release_workflow" != *'tags:'* ]] \
  || fail "validated commits must not rerun the full release workflow after tagging"
[[ "$release_workflow" == *'TERLAN_MULTICORE_CLOSEOUT_ALREADY_RUN=1'* ]] \
  || fail "final AOT closeout must reuse revision-checked multicore evidence"
[[ "$release_workflow" == *'multicore-runner-watchdog:'* ]] \
  || fail "release validation can queue forever without a controlled-runner watchdog"
[[ "$release_workflow" != *$'permissions:\n  actions: write'* ]] \
  || fail "workflow-level Actions write permission exposes every release job"
[[ "$release_workflow" == *'cancelWorkflowRun'* ]] \
  || fail "controlled-runner watchdog cannot terminate an abandoned queue"
[[ "$release_workflow" == *'      - multicore-runner-watchdog'* ]] \
  || fail "final validation can ignore a failed controlled-runner watchdog"
[[ "$release_workflow" == *'name: release-distribution-${{ matrix.target }}'* ]] \
  || fail "native platform jobs do not retain their release archives"
[[ "$release_workflow" == *'bash scripts/check_release_artifact_set.sh target/release-distribution'* ]] \
  || fail "final release validation does not verify the six-platform artifact set"
[[ "$release_workflow" == *'name: release-distribution'* ]] \
  || fail "final release validation does not retain the joined distribution"
[[ "$release_workflow" == *'actions/attest@508db95dd578ae2727ebd6217d5ba78e4fbda05d'* ]] \
  || fail "validated release archives are missing pinned build-provenance attestations"
[[ "$release_workflow" == *'artifact-metadata: write'* \
  && "$release_workflow" == *'attestations: write'* \
  && "$release_workflow" == *'id-token: write'* ]] \
  || fail "release attestation job is missing least-privilege provenance permissions"
[[ "$release_workflow" == *'name: release Rust dependency security audit'* \
  && "$release_workflow" == *'      - dependency-audit'* ]] \
  || fail "release validation can succeed without the RustSec dependency audit"
for producer in platform thread-sanitizer multicore-thread-sanitizer multicore-performance; do
  producer_block="$(awk -v job="  ${producer}:" '
    $0 == job { inside = 1 }
    inside && $0 ~ /^  [A-Za-z0-9_-]+:$/ && $0 != job { exit }
    inside { print }
  ' .github/workflows/release.yml)"
  [[ "$producer_block" == *'needs: release-validation-identity'* ]] \
    || fail "release producer $producer can race ahead of status initialization"
done

ci_workflow="$(cat .github/workflows/ci.yml)"
[[ "$ci_workflow" != *'make build-artifact-budget-record'* ]] \
  || fail "compiler CI must not duplicate the release candidate's artifact-budget producer"
[[ "$ci_workflow" == *'github.com/rhysd/actionlint/cmd/actionlint@v1.7.12'* ]] \
  || fail "compiler CI does not execute the workflow syntax validator"
[[ "$ci_workflow" == *"github.head_ref || github.ref_name, 'release-validation/'"* ]] \
  || fail "compiler CI does not suppress duplicate release-validation pushes and pull requests"
[[ "$ci_workflow" == *'name: Rust dependency security audit'* ]] \
  || fail "compiler CI does not audit the locked dependency graph"
security_workflow="$(cat .github/workflows/security.yml)"
[[ "$security_workflow" == *'schedule:'* \
  && "$security_workflow" == *'uses: ./.github/actions/security-audit'* ]] \
  || fail "new RustSec disclosures are not audited on an independent schedule"
security_action="$(cat .github/actions/security-audit/action.yml)"
[[ "$security_action" == *'cargo install cargo-audit --version 0.22.2 --locked --force'* \
  && "$security_action" != *'~/.cargo/bin/cargo-audit'* ]] \
  || fail "dependency audit must build the exact locked tool instead of trusting a cached executable"
makefile="$(cat Makefile)"
typed_validator_build_count="$(grep -c '\$(TERLAN_TYPED_VALIDATOR_BUILD) \$(TERLAN_' Makefile)"
[[ "$typed_validator_build_count" -eq 14 ]] \
  || fail "every typed validation image must use the content-addressed build wrapper"
[[ "$makefile" == *'$(TERLAN_TYPED_VALIDATOR_BUILD) fingerprint'* ]] \
  || fail "typed validation images rehash shared compiler inputs independently"
[[ "$makefile" == *'$(TERLAN_TYPED_VALIDATOR_BUILD) fingerprint-check'* ]] \
  || fail "reused typed validation images do not reject a changed compiler fingerprint"
for target in \
  tvm-aot-thread-sanitizer-check \
  release-staged-distribution-verification-refresh \
  adversarial-check \
  vm-multicore-thread-sanitizer-contract-check \
  vm-multicore-mc9-evidence-contract-check \
  vm-multicore-release-contract-check; do
  grep -Eq "^${target}:.*terlan-tvm-platform-matrix-bootstrap" Makefile \
    || fail "$target can execute the typed platform validator without bootstrapping it"
done
for target in release-version-metadata-check release-version-bump installer-contract-check; do
  grep -Eq "^${target}:.*terlan-compiler-bootstrap" Makefile \
    || fail "$target can execute terlc without bootstrapping it"
done
[[ "$makefile" == *'release-staged-distribution-verification-refresh'* ]] \
  || fail "publication preflight is not bound to refreshed staged evidence"
[[ "$makefile" == *'scripts/download_validated_release_artifacts.sh'* ]] \
  || fail "publication does not consume artifacts from the validated run"
[[ "$makefile" == *'local and remote annotated tag objects differ'* ]] \
  || fail "publication retry does not require an identical annotated tag object"

downloader="$(cat scripts/download_validated_release_artifacts.sh)"
[[ "$downloader" == *'release-validation/run'* ]] \
  || fail "publication does not require exact-commit release validation"
[[ "$downloader" == *'gh attestation verify'* ]] \
  || fail "publication does not verify release artifact provenance"

publisher="$(cat scripts/publish_release_from_dist.sh)"
[[ "$publisher" == *'--draft --verify-tag'* ]] \
  || fail "publisher must create a verified draft release"
[[ "$publisher" == *'uploaded asset set does not exactly match the sealed candidate'* ]] \
  || fail "publisher must verify the exact uploaded asset set"
[[ "$publisher" == *'uploaded asset sizes or SHA-256 digests do not match the sealed candidate'* ]] \
  || fail "publisher must verify remote asset bytes before making the release public"
[[ "$publisher" == *'--draft=false'* ]] \
  || fail "publisher must promote only after asset verification"

tracked_compiler_cache="$(
  git ls-files \
    | grep -E '(^|/)\.terlan/' \
    | while IFS= read -r path; do
        [[ -e "$path" ]] && printf '%s\n' "$path"
      done \
    || true
)"
[[ -z "$tracked_compiler_cache" ]] \
  || fail "tracked compiler caches found:\n$tracked_compiler_cache"
oversized_tracked_files="$({
  git ls-files -z \
    | while IFS= read -r -d '' path; do
        [[ -f "$path" ]] || continue
        size="$(wc -c < "$path" | tr -d '[:space:]')"
        (( size < 50 * 1024 * 1024 )) || printf '%s %s\n' "$size" "$path"
      done
} || true)"
[[ -z "$oversized_tracked_files" ]] \
  || fail "tracked files must remain below GitHub's 50 MiB warning threshold:\n$oversized_tracked_files"
git check-ignore -q tests/example/.terlan/native-aot/module.o \
  || fail "compiler caches are not ignored repository-wide"

bash scripts/check_release_artifact_set.sh self-test

echo "build/release contract passed"
