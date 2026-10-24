#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE="${TERLAN_SELF_HOST_BACKEND_IMAGE:-$ROOT/target/quality/self-host-phase3-image-21/bin/terlan-self-host-semantic-evidence}"
OUTPUT="${TERLAN_SELF_HOST_PHASE3_EVIDENCE:-$ROOT/target/quality/self-host-phase3-evidence-21}/backend"
PARITY="$ROOT/target/debug/terlan-self-host-parity"
ADMIT="$ROOT/target/debug/terlan-self-host-abi1"

[[ -x "$IMAGE" ]] || {
    echo "retained Phase 3 backend image is missing: $IMAGE" >&2
    exit 2
}

cargo build --quiet -p terlan \
    --bin terlan-self-host-parity \
    --bin terlan-self-host-abi1
mkdir -p "$OUTPUT"
count=0

while IFS= read -r -d '' source; do
    relative=${source#"$ROOT"/}
    stem=$(basename "$source" .terl)
    projected="$OUTPUT/$stem.image.json"
    canonical="$OUTPUT/$stem.canonical.json"
    "$IMAGE" backend "$source" "$projected"
    test -s "$projected"
    "$PARITY" canonicalize "$projected" "$canonical"
    if ! cmp -s "$projected" "$canonical"; then
        diff -u "$projected" "$canonical" || true
        echo "ABI-1 backend contract is not canonical: $relative" >&2
        exit 1
    fi
    "$ADMIT" "$projected" >/dev/null
    count=$((count + 1))
done < <(find "$ROOT/compiler/self_host/corpus/backend" -type f -name '*.terl' -print0 | sort -z)

[[ $count -gt 0 ]] || { echo 'backend contract corpus is empty' >&2; exit 2; }
printf 'fixtures\t%s\nfailures\t0\n' "$count" > "$OUTPUT/summary.tsv"
printf 'ABI-1 backend contract parity and typed admission passed for %s fixtures\n' "$count"
