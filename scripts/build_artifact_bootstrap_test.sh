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
