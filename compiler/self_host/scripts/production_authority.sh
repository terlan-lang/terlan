#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
    echo "usage: production_authority.sh SOURCE_CLOSURE RESOURCE_OBSERVATION STAGE2_ARTIFACT STAGE3_ARTIFACT" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE="${TERLAN_SELF_HOST_IMAGE:?set TERLAN_SELF_HOST_IMAGE to the retained self-hosted compiler image}"
SOURCE_CLOSURE=$1
RESOURCE_OBSERVATION=$2
STAGE2=$3
STAGE3=$4
PHASE9="$ROOT/target/quality/self-host-phase9"
CANDIDATE="$PHASE9/candidate"
CATALOG="$PHASE9/production-authority-sources.tsv"
DECISION="$PHASE9/production-authority-decision.tsv"
COMPLETION="$PHASE9/production-authority.complete"

test -x "$IMAGE"
rm -f "$COMPLETION"

"$ROOT/compiler/self_host/scripts/hardening_evidence.sh" \
    "$SOURCE_CLOSURE" \
    "$RESOURCE_OBSERVATION"

mkdir -p "$CANDIDATE"
TERLAN_SELF_HOST_IMAGE="$IMAGE" \
    "$ROOT/compiler/self_host/scripts/post_cleanup_convergence.sh" \
    "$STAGE2" \
    "$STAGE3" \
    "$CANDIDATE/post-cleanup-converged.evidence"

"$IMAGE" production-evidence \
    "$ROOT/compiler/self_host/phase/PRODUCTION_EVIDENCE_PLAN.tsv" \
    "$ROOT/compiler/self_host/phase/PRODUCTION_AUTHORITY_SOURCES.tsv" \
    "$CATALOG" \
    "$DECISION" \
    "$COMPLETION"

test -s "$CATALOG"
test -s "$DECISION"
grep -qx 'complete' "$COMPLETION"
