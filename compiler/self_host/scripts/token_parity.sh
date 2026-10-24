#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
    printf '%s\n' \
        'usage: token_parity.sh TERLC SELF_HOST_IMAGE SOURCE_OR_CORPUS OUTPUT_ROOT' >&2
    exit 2
fi

terlc=$1
self_host_image=$2
source_or_corpus=${3%/}
output_root=${4%/}
failures="$output_root/failures.tsv"

mkdir -p "$output_root"
: > "$failures"

count=0
failed=0
compare_source() {
    local source=$1
    local relative artifact_root rust_output self_host_output
    if [[ -d "$source_or_corpus" ]]; then
        relative=${source#"$source_or_corpus"/}
    else
        relative=$(basename "$source")
    fi
    artifact_root="$output_root/${relative%.*}"
    mkdir -p "$(dirname "$artifact_root")"
    rust_output="$artifact_root.rust.tokens.tsv"
    self_host_output="$artifact_root.self-host.tokens.tsv"
    count=$((count + 1))

    if ! "$terlc" syntax-contract --tokens "$source" --out "$rust_output"; then
        printf '%s\t%s\n' "$relative" 'rust-oracle-failed' >> "$failures"
        failed=$((failed + 1))
        return
    fi
    if ! "$self_host_image" tokens "$source" "$self_host_output"; then
        printf '%s\t%s\n' "$relative" 'self-host-oracle-failed' >> "$failures"
        failed=$((failed + 1))
        return
    fi
    if ! cmp -s "$rust_output" "$self_host_output"; then
        printf '%s\t%s\n' "$relative" 'token-mismatch' >> "$failures"
        failed=$((failed + 1))
    fi
}

if [[ -d "$source_or_corpus" ]]; then
    while IFS= read -r -d '' source; do
        compare_source "$source"
    done < <(find "$source_or_corpus" -type f \( -name '*.terl' -o -name '*.terli' \) -print0 | sort -z)
elif [[ -f "$source_or_corpus" ]]; then
    compare_source "$source_or_corpus"
else
    printf 'token parity source does not exist: %s\n' "$source_or_corpus" >&2
    exit 2
fi

printf 'sources\t%s\nfailures\t%s\n' "$count" "$failed" > "$output_root/summary.tsv"
if [[ $count -eq 0 ]]; then
    printf '%s\n' 'token parity corpus contains no Terlan sources' >&2
    exit 2
fi
if [[ $failed -ne 0 ]]; then
    printf 'token parity failed for %s of %s sources\n' "$failed" "$count" >&2
    exit 1
fi
printf 'token parity holds for %s sources\n' "$count"
