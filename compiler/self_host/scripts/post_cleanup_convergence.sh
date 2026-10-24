#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: post_cleanup_convergence.sh STAGE2_ARTIFACT STAGE3_ARTIFACT EVIDENCE" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE="${TERLAN_SELF_HOST_IMAGE:?set TERLAN_SELF_HOST_IMAGE to the retained self-hosted compiler image}"
STAGE2=$1
STAGE3=$2
EVIDENCE=$3
WORK=$(mktemp -d "${TMPDIR:-/tmp}/terlan-post-cleanup-convergence.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

test -x "$IMAGE"
test -s "$STAGE2"
test -s "$STAGE3"

MARKER="$WORK/fixed-point.marker"
"$IMAGE" bootstrap-compare "$STAGE2" "$STAGE3" "$MARKER"
grep -qx 'fixed-point' "$MARKER"

STAGE2_SHA256=$(sha256sum "$STAGE2" | cut -d' ' -f1)
STAGE3_SHA256=$(sha256sum "$STAGE3" | cut -d' ' -f1)
if [[ "$STAGE2_SHA256" != "$STAGE3_SHA256" ]]; then
    echo "post-cleanup artifacts differ despite semantic fixed-point marker" >&2
    exit 1
fi

mkdir -p "$(dirname "$EVIDENCE")"
TEMP="$WORK/evidence"
{
    printf 'schema\tterlan.self-host.post-cleanup-convergence/v1\n'
    printf 'stage2-sha256\t%s\n' "$STAGE2_SHA256"
    printf 'stage3-sha256\t%s\n' "$STAGE3_SHA256"
    printf 'semantic-fixed-point\tpass\n'
    printf 'artifact-fixed-point\tpass\n'
} > "$TEMP"
mv "$TEMP" "$EVIDENCE"
