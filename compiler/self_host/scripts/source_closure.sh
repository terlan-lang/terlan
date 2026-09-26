#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: source_closure.sh OUTPUT_MANIFEST" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
"$ROOT/compiler/self_host/scripts/projected_oracle.sh" source-closure "$1"
