#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE="${TERLAN_SELF_HOST_SEMANTIC_IMAGE:-$ROOT/target/quality/self-host-phase3-image-21/bin/terlan-self-host-semantic-evidence}"
OUTPUT="${TERLAN_SELF_HOST_PHASE3_EVIDENCE:-$ROOT/target/quality/self-host-phase3-evidence-21}/semantic"
RUST_ORACLE="$ROOT/target/debug/terlan-semantic-oracle"

[[ -x "$IMAGE" ]] || {
    echo "retained Phase 3 semantic image is missing: $IMAGE" >&2
    exit 2
}

cargo build --quiet -p terlan --bin terlan-semantic-oracle
mkdir -p "$OUTPUT"
count=0

while IFS= read -r -d '' source; do
    relative=${source#"$ROOT"/}
    stem=$(printf '%s' "$relative" | tr '/.' '__')
    rust="$OUTPUT/$stem.rust.tsv"
    projected="$OUTPUT/$stem.image.tsv"
    supplemental="$OUTPUT/$stem.supplemental.tsv"
    "$RUST_ORACLE" "$source" "$rust"
    "$IMAGE" semantic "$source" "$projected"
    "$IMAGE" semantic-full "$source" "$supplemental"
    if ! cmp -s "$rust" "$projected"; then
        diff -u "$rust" "$projected" || true
        echo "semantic parity failed: $relative" >&2
        exit 1
    fi
    test -s "$supplemental"
    count=$((count + 1))
done < <(find \
    "$ROOT/compiler/self_host/corpus/semantic" \
    "$ROOT/compiler/self_host/corpus/accepted" \
    -type f \( -name '*.terl' -o -name '*.terli' \) -print0 | sort -z)

[[ $count -gt 0 ]] || { echo 'semantic parity corpus is empty' >&2; exit 2; }
printf 'fixtures\t%s\nfailures\t0\n' "$count" > "$OUTPUT/summary.tsv"
printf 'semantic common parity passed for %s fixtures; supplemental inferred-type evidence emitted\n' "$count"
