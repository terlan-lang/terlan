#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 6 ]]; then
    echo 'usage: image_mode_parity.sh RUST_ORACLE IMAGE MODE CORPUS OUTPUT_ROOT SUFFIX' >&2
    exit 2
fi

rust_oracle=$1
image=$2
mode=$3
corpus=${4%/}
output=${5%/}
suffix=$6
mkdir -p "$output"
failures="$output/failures.tsv"
: > "$failures"
count=0
failed=0

while IFS= read -r -d '' source; do
    relative=${source#"$corpus"/}
    root="$output/${relative%.*}"
    mkdir -p "$(dirname "$root")"
    rust="$root.rust.$suffix.json"
    candidate="$root.image.$suffix.json"
    count=$((count + 1))
    if ! "$rust_oracle" "$source" "$rust"; then
        printf '%s\t%s\n' "$relative" 'rust-oracle-failed' >> "$failures"
        failed=$((failed + 1))
        continue
    fi
    if ! "$image" "$mode" "$source" "$candidate"; then
        printf '%s\t%s\n' "$relative" 'image-oracle-failed' >> "$failures"
        failed=$((failed + 1))
        continue
    fi
    if ! cmp -s "$rust" "$candidate"; then
        printf '%s\t%s\n' "$relative" "$mode-mismatch" >> "$failures"
        failed=$((failed + 1))
    fi
done < <(find "$corpus" -type f \( -name '*.terl' -o -name '*.terli' \) -print0 | sort -z)

printf 'sources\t%s\nfailures\t%s\n' "$count" "$failed" > "$output/summary.tsv"
[[ $count -gt 0 ]] || { echo "$mode parity corpus is empty" >&2; exit 2; }
[[ $failed -eq 0 ]] || { echo "$mode parity failed for $failed of $count sources" >&2; exit 1; }
printf '%s parity holds for %s sources through the current image\n' "$suffix" "$count"
