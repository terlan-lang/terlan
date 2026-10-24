#!/usr/bin/env bash
# Exercise the production image owner across disposable output cleanup.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
temporary=$(mktemp -d)
trap 'rm -r -- "$temporary"' EXIT
awk '/^TERLAN_SELF_VALIDATION_(DIR|IMAGE) :=/ { print }
  /^terlan-artifact-measurement-bootstrap:/ { copying=1 }
  copying && /^$/ { exit }
  copying { print }' "$root/Makefile" > "$temporary/Makefile"
cat >> "$temporary/Makefile" <<'MAKE'
.PHONY: terlan-compiler-bootstrap verify
terlan-compiler-bootstrap:
verify: terlan-artifact-measurement-bootstrap
	test -s $(TERLAN_SELF_VALIDATION_IMAGE)
TERLAN_TYPED_VALIDATOR_BUILD := bash producer.sh
MAKE
cat > "$temporary/producer.sh" <<'PRODUCER'
set -eu
image=$1
while (( $# > 0 )); do
  if [[ "$1" == --out-dir ]]; then destination=$2; break; fi
  shift
done
test "$image" = "$destination/vm/scripts_self_validation_BuildArtifactBudgetTest.tvm"
mkdir -p "$(dirname "$image")"
printf 'fixture image\n' > "$image"
printf 'built\n' >> builds
PRODUCER
(
  cd "$temporary"
  unset MAKEFLAGS MAKEOVERRIDES MFLAGS TERLAN_VALIDATION_BOOTSTRAPPED TERLAN_ARTIFACT_MEASUREMENT_ALREADY_BUILT
  make --no-print-directory verify
  mkdir -p _build
  rm -r -- _build
  make --no-print-directory verify TERLAN_ARTIFACT_MEASUREMENT_ALREADY_BUILT=1
  test "$(cat builds)" = built
  # Prebuilt mode must fail closed if the retained image is missing.
  rm -r -- target
  if make --no-print-directory verify TERLAN_ARTIFACT_MEASUREMENT_ALREADY_BUILT=1 > missing.log 2>&1; then
    echo 'Budget validation accepted a missing prebuilt image' >&2
    exit 1
  fi
)
echo 'Build artifact image survives disposable output cleanup.'

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
