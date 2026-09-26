#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: bootstrap_fixed_point.sh STAGE1_ARTIFACT STAGE2_ARTIFACT" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MARKER="${TMPDIR:-/tmp}/terlan-self-host-fixed-point.$$"
trap 'rm -f "$MARKER"' EXIT
"$ROOT/compiler/self_host/scripts/projected_oracle.sh" bootstrap-compare "$1" "$2" "$MARKER"
grep -qx 'fixed-point' "$MARKER"
