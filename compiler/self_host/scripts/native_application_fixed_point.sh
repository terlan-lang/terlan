#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 4 ]]; then
    printf 'usage: %s <stage2-self-host> <stage3-self-host> <evidence.txt> <source.terl> [source.terl ...]\n' "$0" >&2
    exit 64
fi

stage2=$1
stage3=$2
evidence_file=$3
shift 3
sources=("$@")
script_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/terlan-native-app-fixed-point.XXXXXX")
trap 'rm -rf "$temporary_directory"' EXIT

stage2_object="$temporary_directory/stage2-application.o"
stage3_object="$temporary_directory/stage3-application.o"

"$script_directory/native_application_stage.sh" "$stage2" "$stage2_object" "${sources[@]}"
"$script_directory/native_application_stage.sh" "$stage3" "$stage3_object" "${sources[@]}"

stage2_digest=$(sha256sum "$stage2_object" | cut -d' ' -f1)
stage3_digest=$(sha256sum "$stage3_object" | cut -d' ' -f1)
if ! cmp -s "$stage2_object" "$stage3_object"; then
    printf 'native application fixed point failed\n' >&2
    printf 'stage2 sha256: %s\n' "$stage2_digest" >&2
    printf 'stage3 sha256: %s\n' "$stage3_digest" >&2
    exit 1
fi

corpus_digest=$(
    for source_file in "${sources[@]}"; do
        printf '%s\0' "$source_file"
        sha256sum "$source_file" | cut -d' ' -f1
    done | sha256sum | cut -d' ' -f1
)
mkdir -p "$(dirname "$evidence_file")"
evidence_tmp="$temporary_directory/evidence.txt"
{
    printf 'schema=terlan.self-host.native-application-fixed-point/v1\n'
    printf 'corpus_sha256=%s\n' "$corpus_digest"
    printf 'source_count=%d\n' "${#sources[@]}"
    printf 'stage2_sha256=%s\n' "$stage2_digest"
    printf 'stage3_sha256=%s\n' "$stage3_digest"
    printf 'equal=true\n'
} > "$evidence_tmp"
mv "$evidence_tmp" "$evidence_file"
