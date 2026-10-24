#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: hardening_evidence.sh SOURCE_CLOSURE_MANIFEST RESOURCE_OBSERVATION" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TERLC=${TERLC:-$ROOT/target/bootstrap/terlc1/terlc}
SOURCE_CLOSURE=$1
RESOURCE_OBSERVATION=$2
SCRIPT="$ROOT/compiler/self_host/scripts/HardeningEvidence.terls"
OUTPUT="$ROOT/target/quality/self-host-phase9/candidate"

case "$TERLC" in
    */target/debug/terlc|*/target/bootstrap/terlc0/terlc|target/debug/terlc|target/bootstrap/terlc0/terlc)
        echo "recovery compiler is not admitted for production hardening evidence: $TERLC" >&2
        exit 2
        ;;
esac

test -s "$SOURCE_CLOSURE"
test -s "$RESOURCE_OBSERVATION"
mkdir -p "$OUTPUT"

"$TERLC" --incremental run "$SCRIPT" -- \
    analysis-resources \
    "$RESOURCE_OBSERVATION" \
    "$OUTPUT/self-analysis-resource-bounds.evidence"

"$TERLC" --incremental run "$SCRIPT" -- \
    bootstrap-workarounds \
    "$SOURCE_CLOSURE" \
    "$ROOT/compiler/self_host/phase/COMPATIBILITY_BOUNDARIES.tsv" \
    "$OUTPUT/bootstrap-workarounds-removed.evidence"

"$TERLC" --incremental run "$SCRIPT" -- \
    no-ad-hoc-exclusions \
    "$SOURCE_CLOSURE" \
    "$ROOT/compiler/self_host/phase/EXCLUSIONS.tsv" \
    "$OUTPUT/no-ad-hoc-exclusions.evidence"

"$TERLC" --incremental run "$SCRIPT" -- \
    regression-corpus \
    "$ROOT/compiler/self_host/corpus/REGRESSION_CORPUS.tsv" \
    "$ROOT/compiler/self_host/corpus" \
    "$OUTPUT/compiler-regression-corpus-complete.evidence"

"$TERLC" --incremental run "$SCRIPT" -- \
    trusted-base \
    "$ROOT/compiler/self_host/phase/TRUSTED_BASE.tsv" \
    "$OUTPUT/minimal-trusted-base.evidence"
cp "$OUTPUT/minimal-trusted-base.evidence" "$OUTPUT/trusted-base-documented.evidence"

"$TERLC" --incremental run "$SCRIPT" -- \
    normal-paths \
    "$ROOT/compiler/self_host/phase/NORMAL_PATHS.tsv" \
    "$OUTPUT/rust-frontend-removed-from-normal-paths.evidence"

for artifact in \
    self-analysis-resource-bounds.evidence \
    bootstrap-workarounds-removed.evidence \
    no-ad-hoc-exclusions.evidence \
    compiler-regression-corpus-complete.evidence \
    minimal-trusted-base.evidence \
    trusted-base-documented.evidence \
    rust-frontend-removed-from-normal-paths.evidence; do
    test -s "$OUTPUT/$artifact"
done
