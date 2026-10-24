#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
CORPUS="$ROOT/compiler/self_host/corpus/fragments/FRAGMENT_CORPUS.tsv"
WORK="${TMPDIR:-/tmp}/terlan-self-host-fragments.$$"
PROJECTED_ORACLE="${TERLAN_SELF_HOST_IMAGE:-$ROOT/compiler/self_host/scripts/projected_oracle.sh}"
RUST_ORACLE="${TERLAN_FRAGMENT_ORACLE:-$ROOT/target/debug/terlan-fragment-oracle}"
mkdir -p "$WORK"
trap 'find "$WORK" -depth -delete' EXIT

cargo build --quiet -p terlan --bin terlan-fragment-oracle
inputs=()
modes=()
source_paths=()
rust_outputs=()
projected_outputs=()
batch=(fragments-batch)

while IFS=$'\t' read -r mode relative; do
    [[ -z "$mode" || "$mode" == \#* ]] && continue
    input="$ROOT/compiler/self_host/corpus/fragments/$relative"
    stem="${mode}-$(basename "$relative")"
    rust="$WORK/$stem.rust"
    projected="$WORK/$stem.projected"
    "$RUST_ORACLE" "$mode" "$input" "$rust"
    inputs+=("$mode $relative")
    modes+=("$mode")
    source_paths+=("$input")
    rust_outputs+=("$rust")
    projected_outputs+=("$projected")
    batch+=("$mode" "$input" "$projected")
done < "$CORPUS"

[[ ${#inputs[@]} -gt 0 ]] || { echo 'fragment corpus is empty' >&2; exit 2; }
if [[ "$PROJECTED_ORACLE" == *projected_oracle.sh ]]; then
    "$PROJECTED_ORACLE" "${batch[@]}"
else
    for index in "${!inputs[@]}"; do
        "$PROJECTED_ORACLE" "${modes[$index]}" "${source_paths[$index]}" "${projected_outputs[$index]}"
    done
fi

for index in "${!inputs[@]}"; do
    if ! cmp -s "${rust_outputs[$index]}" "${projected_outputs[$index]}"; then
        diff -u "${rust_outputs[$index]}" "${projected_outputs[$index]}" || true
        echo "fragment parity failed: ${inputs[$index]}" >&2
        exit 1
    fi
done

printf 'fragment syntax parity passed for %s sources\n' "${#inputs[@]}"
