#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
    printf '%s\n' 'usage: error_parity.sh RUST_ORACLE PROJECTED_ORACLE CORPUS_ROOT OUTPUT_ROOT' >&2
    exit 2
fi

rust_oracle=$1
projected_oracle=$2
corpus_root=${3%/}
output_root=${4%/}
failures="$output_root/failures.tsv"
mkdir -p "$output_root"
: > "$failures"
count=0
failed=0

while IFS= read -r -d '' source; do
    relative=${source#"$corpus_root"/}
    artifact_root="$output_root/${relative%.*}"
    mkdir -p "$(dirname "$artifact_root")"
    rust_output="$artifact_root.rust.error.json"
    projected_output="$artifact_root.projected.error.json"
    count=$((count + 1))
    if ! "$rust_oracle" "$source" "$rust_output"; then
        printf '%s\t%s\n' "$relative" 'rust-error-oracle-failed' >> "$failures"
        failed=$((failed + 1))
        continue
    fi
    if ! "$projected_oracle" errors "$source" "$projected_output"; then
        printf '%s\t%s\n' "$relative" 'projected-error-oracle-failed' >> "$failures"
        failed=$((failed + 1))
        continue
    fi
    if ! cmp -s "$rust_output" "$projected_output"; then
        printf '%s\t%s\n' "$relative" 'error-class-or-span-mismatch' >> "$failures"
        failed=$((failed + 1))
    fi
done < <(find "$corpus_root" -type f \( -name '*.terl' -o -name '*.terli' \) -print0 | sort -z)

printf 'sources\t%s\nfailures\t%s\n' "$count" "$failed" > "$output_root/summary.tsv"
if [[ $count -eq 0 ]]; then
    printf '%s\n' 'error parity corpus contains no Terlan sources' >&2
    exit 2
fi
if [[ $failed -ne 0 ]]; then
    printf 'error parity failed for %s of %s sources\n' "$failed" "$count" >&2
    exit 1
fi
printf 'error parity holds for %s sources\n' "$count"
