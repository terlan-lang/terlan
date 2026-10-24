#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: bootstrap_convergence.sh STAGE0_ARTIFACT STAGE1_ARTIFACT STAGE2_ARTIFACT" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE="${TERLAN_SELF_HOST_IMAGE:?set TERLAN_SELF_HOST_IMAGE to the retained compiler image}"
MARKER="$ROOT/target/quality/self-host-convergence.$$.marker"
trap 'rm -f "$MARKER"' EXIT

"$IMAGE" bootstrap-converge "$1" "$2" "$3" "$MARKER"
grep -qx 'fixed-point' "$MARKER"
