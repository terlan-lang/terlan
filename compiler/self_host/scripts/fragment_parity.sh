#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
CORPUS="$ROOT/compiler/self_host/corpus/fragments/FRAGMENT_CORPUS.tsv"
WORK="${TMPDIR:-/tmp}/terlan-self-host-fragments.$$"
mkdir -p "$WORK"
trap 'rm -rf "$WORK"' EXIT

while IFS=$'\t' read -r mode relative; do
    [[ -z "$mode" || "$mode" == \#* ]] && continue
    input="$ROOT/compiler/self_host/corpus/fragments/$relative"
    rust="$WORK/${mode}-$(basename "$relative").rust"
    projected="$WORK/${mode}-$(basename "$relative").projected"
    cargo run --quiet -p terlan --bin terlan-fragment-oracle -- "$mode" "$input" "$rust"
    "$ROOT/compiler/self_host/scripts/projected_oracle.sh" "$mode" "$input" "$projected"
    if ! cmp -s "$rust" "$projected"; then
        diff -u "$rust" "$projected" || true
        echo "fragment parity failed: $mode $relative" >&2
        exit 1
    fi
done < "$CORPUS"

echo "fragment syntax parity passed"
