#!/usr/bin/env bash
# Exercise the production image owner across disposable output cleanup.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$root/target"
temporary=$(mktemp -d "$root/target/build-artifact-bootstrap-test.XXXXXX")
trap 'rm -r -- "$temporary"' EXIT
awk '/^TERLAN_SELF_VALIDATION_(DIR|IMAGE) :=/ { print }
  /^terlan-artifact-measurement-bootstrap:/ { copying=1 }
  copying && /^$/ { exit }
  copying { print }' "$root/Makefile" > "$temporary/Makefile"
awk '/^release-candidate-check:/ { copying=1 }
  copying && /^$/ { exit }
  copying { print }' "$root/Makefile" >> "$temporary/Makefile"
cat >> "$temporary/Makefile" <<'MAKE'
.PHONY: terlan-compiler-bootstrap verify editor-release-contract-check terlan-quality-tools-bootstrap build-artifact-budget-record check release-proof-baseline-check
terlan-compiler-bootstrap:
editor-release-contract-check terlan-quality-tools-bootstrap:
verify: terlan-artifact-measurement-bootstrap
	test -s $(TERLAN_SELF_VALIDATION_IMAGE)
build-artifact-budget-record: terlan-artifact-measurement-bootstrap
	cargo clean --manifest-path Cargo.toml --target-dir target
	test ! -e target
check release-proof-baseline-check: verify
TERLAN_TYPED_VALIDATOR_BUILD := bash producer.sh
MAKE
mkdir -p "$temporary/scripts"
printf 'test "$1" = --check-partials\n' > "$temporary/scripts/clean_build_outputs.sh"
cat > "$temporary/producer.sh" <<'PRODUCER'
set -eu
image=$1
while (( $# > 0 )); do
  if [[ "$1" == --out-dir ]]; then destination=$2; break; fi
  shift
done
test "$image" = "$destination/vm/scripts_self_validation_BuildArtifactBudgetTest.tvm"
# Model the unchanged typed-validator cache's admission of a retained image.
if [[ -s "$image" ]]; then exit 0; fi
mkdir -p "$(dirname "$image")"
printf 'fixture image\n' > "$image"
printf 'built\n' >> builds
PRODUCER
(
  cd "$temporary"
  unset MAKEFLAGS MAKEOVERRIDES MFLAGS TERLAN_VALIDATION_BOOTSTRAPPED TERLAN_BUILD_ARTIFACTS_PREBUILT
  make --no-print-directory verify
  mkdir -p _build
  rm -r -- _build
  make --no-print-directory verify TERLAN_BUILD_ARTIFACTS_PREBUILT=1
  test "$(cat builds)" = built
  # Reproduce measurement's destructive boundary without compiling any Rust.
  mkdir -p src
  printf '[package]\nname = "budget-cleanup-fixture"\nversion = "0.0.0"\nedition = "2021"\n[workspace]\n' > Cargo.toml
  printf 'pub fn fixture() {}\n' > src/lib.rs
  cargo clean --manifest-path Cargo.toml --target-dir target
  test ! -e target
  make --no-print-directory verify TERLAN_BUILD_ARTIFACTS_PREBUILT=1
  test "$(wc -l < builds)" -eq 2
  make --no-print-directory verify TERLAN_VALIDATION_BOOTSTRAPPED=1
  test "$(wc -l < builds)" -eq 2
  # Exercise the production release recipe, including its recursive environment.
  make --no-print-directory release-candidate-check
  test "$(wc -l < builds)" -eq 3
  # A consumer that claims a completed bootstrap must fail closed if it is gone.
  rm -r -- target
  if make --no-print-directory terlan-artifact-measurement-bootstrap TERLAN_VALIDATION_BOOTSTRAPPED=1 > missing.log 2>&1; then
    echo 'Budget validation accepted a missing prebuilt image' >&2
    exit 1
  fi
)
echo 'Budget validator is rebuilt after Cargo cleanup and retained between consumers.'

# The optimized validator runtime uses the process owner and honors prebuilt mode.
mkdir "$temporary/runtime"
awk '/^TERLAN_VALIDATION_VM :=/ { print }
  /^terlan-validation-vm-bootstrap:/ { copying=1 }
  copying && /^$/ { exit }
  copying { print }' "$root/Makefile" > "$temporary/runtime/Makefile"
cat >> "$temporary/runtime/Makefile" <<'MAKE'
.PHONY: terlan-compiler-bootstrap
terlan-compiler-bootstrap:
TERLAN_BOOTSTRAP_LOCK :=
TERLAN_RUST_ORCHESTRATOR := bash owner.sh
TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS := 3600
CARGO := bash cargo.sh --locked
MAKE
cat > "$temporary/runtime/owner.sh" <<'OWNER'
set -eu
test "$1" = --run-owned
test "$2" = --timeout-seconds
test "$3" = 3600
test "$4" = --
shift 4
printf 'owned\n' >> owners
exec "$@"
OWNER
cat > "$temporary/runtime/cargo.sh" <<'CARGO'
set -eu
test "$*" = '--locked build --release -p terlan --bin terlan-vm'
mkdir -p target/release
printf '#!/bin/sh\nexit 0\n' > target/release/terlan-vm
chmod +x target/release/terlan-vm
printf 'built\n' >> builds
CARGO
(
  cd "$temporary/runtime"
  unset MAKEFLAGS MAKEOVERRIDES MFLAGS TERLAN_VALIDATION_BOOTSTRAPPED TERLAN_BUILD_ARTIFACTS_PREBUILT TERLAN_RELEASE_BINARIES_PREBUILT
  make --no-print-directory terlan-validation-vm-bootstrap
  for mode in TERLAN_VALIDATION_BOOTSTRAPPED TERLAN_BUILD_ARTIFACTS_PREBUILT TERLAN_RELEASE_BINARIES_PREBUILT; do
    make --no-print-directory terlan-validation-vm-bootstrap "$mode=1"
  done
  test "$(cat owners)" = owned
  test "$(cat builds)" = built
  rm target/release/terlan-vm
  for mode in TERLAN_VALIDATION_BOOTSTRAPPED TERLAN_BUILD_ARTIFACTS_PREBUILT TERLAN_RELEASE_BINARIES_PREBUILT; do
    if make --no-print-directory terlan-validation-vm-bootstrap "$mode=1" > missing.log 2>&1; then
      echo 'Validation accepted a missing prebuilt runtime' >&2
      exit 1
    fi
  done
)
echo 'Optimized validation runtime preserves process ownership and prebuilt checks.'

# The real Make consumer must admit the limits emitted by the typed producer.
mkdir -p "$temporary/contract/target/quality"
awk '/^repository-build-release-contract-check:/ { copying=1 }
  copying && /^$/ { exit }
  copying { print }' "$root/Makefile" > "$temporary/contract/Makefile"
cat >> "$temporary/contract/Makefile" <<'MAKE'
.PHONY: terlan-repository-validation-bootstrap
terlan-repository-validation-bootstrap:
TERLAN_REPOSITORY_VALIDATION := bash producer.sh
MAKE
cat > "$temporary/contract/producer.sh" <<'PRODUCER'
set -eu
test "$1" = build-release-contract
cp input.json target/quality/validation-build-plan-report.json
PRODUCER
{
  printf '{\n  "decision": "pass",\n  "duplicate_equivalent_build_count": 0,\n'
  printf '  "terlc_test_invocation_count": 31,\n  "incremental_terlc_build_invocation_count": 18,\n  "lifecycle_partial_check_count": 2,\n'
  awk '/\.put\("[a-z_]+", Json\.int\([0-9]+\)\)/ {
    key=$0; sub(/^.*\.put\("/, "", key); sub(/".*$/, "", key)
    value=$0; sub(/^.*Json\.int\(/, "", value); sub(/\).*$/, "", value)
    if (key ~ /_maximum$/) printf "  \"%s\": %s,\n", key, value
  }' "$root/scripts/self_validation/RepositoryValidation.terls"
  printf '  "fixture": true\n}\n'
} > "$temporary/contract/valid.json"
(
  cd "$temporary/contract"
  unset MAKEFLAGS MAKEOVERRIDES MFLAGS
  cp valid.json input.json
  make --no-print-directory repository-build-release-contract-check
  for key in terlc_build_invocation_maximum typed_validator_request_maximum; do
    sed "s/\"$key\": [0-9]*/\"$key\": 0/" valid.json > input.json
    if make --no-print-directory repository-build-release-contract-check > rejected.log 2>&1; then
      echo "Build-plan consumer accepted an invalid $key" >&2
      exit 1
    fi
  done
)
echo 'Build-plan Make consumer admits producer limits and rejects invalid limits.'
