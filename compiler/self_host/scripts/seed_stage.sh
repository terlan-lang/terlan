#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: seed_stage.sh OUTPUT_DIRECTORY" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE="${TERLAN_SELF_HOST_IMAGE:?set TERLAN_SELF_HOST_IMAGE to the retained compiler image}"
OUTPUT=$1
PACKAGE="$OUTPUT/package"
MANIFEST="$PACKAGE/sources.manifest"
EVIDENCE="$OUTPUT/evidence"

cd "$ROOT"
mkdir -p "$PACKAGE" "$EVIDENCE"
"$IMAGE" source-closure "$MANIFEST"
test -s "$MANIFEST"
"$ROOT/compiler/self_host/scripts/bounded_package_stage.sh" "$IMAGE" "$MANIFEST" "$PACKAGE"
TERLAN_SELF_HOST_IMAGE="$IMAGE" \
    "$ROOT/compiler/self_host/scripts/stage_evidence_batch.sh" "$MANIFEST" "$EVIDENCE"
cargo run --quiet -p terlan --bin terlan-self-host-parity -- \
    artifact-set self_host \
    "$EVIDENCE/tokens.bundle" \
    "$EVIDENCE/syntax.bundle" \
    "$EVIDENCE/semantics.bundle" \
    "$PACKAGE/modules" \
    "$OUTPUT/bootstrap-artifact.json"
MARKER="$OUTPUT/bootstrap-artifact.valid"
trap 'rm -f "$MARKER"' EXIT
"$IMAGE" bootstrap-compare \
    "$OUTPUT/bootstrap-artifact.json" \
    "$OUTPUT/bootstrap-artifact.json" \
    "$MARKER"
grep -qx 'fixed-point' "$MARKER"
