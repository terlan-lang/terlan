#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
    printf '%s\n' 'usage: surface_parity.sh RUST_ORACLE PROJECTED_ORACLE CORPUS_ROOT OUTPUT_ROOT' >&2
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
cached=0

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
terlc=${TERLC:-$root/target/debug/terlc}
oracle_identity="$({
    sha256sum "$rust_oracle" "$projected_oracle" "$terlc" "$0" \
        "$root/compiler/self_host/terlan.toml" \
        "$root/compiler/self_host_syntax_evidence/terlan.toml"
    find "$root/compiler/self_host/src" "$root/compiler/self_host_syntax_evidence/src" \
        -type f \( -name '*.terl' -o -name '*.terli' \) \
        -print0 | sort -z | xargs -0 sha256sum
} | sha256sum | cut -d ' ' -f 1)"

sources=()
relatives=()
rust_outputs=()
projected_outputs=()
stamps=()
case_identities=()

while IFS= read -r -d '' source; do
    relative=${source#"$corpus_root"/}
    artifact_root="$output_root/${relative%.*}"
    mkdir -p "$(dirname "$artifact_root")"
    rust_output="$artifact_root.rust.surface.json"
    projected_output="$artifact_root.projected.surface.json"
    stamp="$artifact_root.surface.identity.sha256"
    count=$((count + 1))
    source_identity="$(sha256sum "$source" | cut -d ' ' -f 1)"
    case_identity="$(printf '%s\n%s\n' "$oracle_identity" "$source_identity" | sha256sum | cut -d ' ' -f 1)"
    if [[ -s "$rust_output" && -s "$projected_output" && -f "$stamp" ]] \
        && [[ "$(cat "$stamp")" == "$case_identity" ]] \
        && cmp -s "$rust_output" "$projected_output"; then
        cached=$((cached + 1))
        continue
    fi
    if ! "$rust_oracle" "$source" "$rust_output"; then
        printf '%s\t%s\n' "$relative" 'rust-oracle-failed' >> "$failures"
        failed=$((failed + 1))
        continue
    fi
    sources+=("$source")
    relatives+=("$relative")
    rust_outputs+=("$rust_output")
    projected_outputs+=("$projected_output")
    stamps+=("$stamp")
    case_identities+=("$case_identity")
done < <(find "$corpus_root" -type f \( -name '*.terl' -o -name '*.terli' \) -print0 | sort -z)

if [[ ${#sources[@]} -gt 0 ]]; then
    batch=(parity-batch)
    for index in "${!sources[@]}"; do
        batch+=("${sources[$index]}" "${projected_outputs[$index]}")
    done
    if ! "$projected_oracle" "${batch[@]}"; then
        for relative in "${relatives[@]}"; do
            printf '%s\t%s\n' "$relative" 'projected-oracle-failed' >> "$failures"
            failed=$((failed + 1))
        done
    else
        for index in "${!sources[@]}"; do
            if ! cmp -s "${rust_outputs[$index]}" "${projected_outputs[$index]}"; then
                printf '%s\t%s\n' "${relatives[$index]}" 'surface-mismatch' >> "$failures"
                failed=$((failed + 1))
                continue
            fi
            printf '%s\n' "${case_identities[$index]}" > "${stamps[$index]}"
        done
    fi
fi

printf 'sources\t%s\ncached\t%s\nfailures\t%s\n' "$count" "$cached" "$failed" > "$output_root/summary.tsv"
if [[ $count -eq 0 ]]; then
    printf '%s\n' 'surface parity corpus contains no Terlan sources' >&2
    exit 2
fi
if [[ $failed -ne 0 ]]; then
    printf 'surface parity failed for %s of %s sources\n' "$failed" "$count" >&2
    exit 1
fi
printf 'surface parity holds for %s sources (%s cached)\n' "$count" "$cached"
