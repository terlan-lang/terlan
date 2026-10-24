#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: source_closure.sh OUTPUT_MANIFEST" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TERLC=${TERLC:-$ROOT/target/bootstrap/terlc1/terlc}
case "$TERLC" in
    */target/debug/terlc|*/target/bootstrap/terlc0/terlc|target/debug/terlc|target/bootstrap/terlc0/terlc)
        echo "recovery compiler is not admitted on the normal source-closure path: $TERLC" >&2
        exit 2
        ;;
esac
"$TERLC" --incremental run "$ROOT/compiler/self_host/scripts/SourceClosure.terls" -- "$1"
