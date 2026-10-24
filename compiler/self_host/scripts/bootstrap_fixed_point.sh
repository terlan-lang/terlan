#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: bootstrap_fixed_point.sh STAGE1_ARTIFACT STAGE2_ARTIFACT" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE="${TERLAN_SELF_HOST_IMAGE:?set TERLAN_SELF_HOST_IMAGE to the retained compiler image}"
MARKER="$ROOT/target/quality/self-host-fixed-point.$$.marker"
trap 'rm -f "$MARKER"' EXIT

"$IMAGE" bootstrap-compare "$1" "$2" "$MARKER"
grep -qx 'fixed-point' "$MARKER"
