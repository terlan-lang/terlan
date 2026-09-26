#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: package_stage.sh OUTPUT_DIRECTORY" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUTPUT=$1
TERLC=${TERLC:-target/debug/terlc}

"$TERLC" --incremental build \
    "$ROOT/compiler/self_host" \
    --target terlan-vm \
    --release \
    --out-dir "$OUTPUT"
