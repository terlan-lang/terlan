#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
    printf 'usage: %s <stage2-self-host> <stage3-self-host> <source.terl> <evidence.txt>\n' "$0" >&2
    exit 64
fi

stage2=$1
stage3=$2
source_file=$3
evidence_file=$4
script_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/terlan-native-fixed-point.XXXXXX")
trap 'rm -rf "$temporary_directory"' EXIT

stage2_object="$temporary_directory/stage2.o"
stage3_object="$temporary_directory/stage3.o"

"$script_directory/native_object_stage.sh" "$stage2" "$source_file" "$stage2_object"
"$script_directory/native_object_stage.sh" "$stage3" "$source_file" "$stage3_object"

if ! cmp -s "$stage2_object" "$stage3_object"; then
    printf 'native object fixed point failed for %s\n' "$source_file" >&2
    printf 'stage2 sha256: %s\n' "$(sha256sum "$stage2_object" | cut -d' ' -f1)" >&2
    printf 'stage3 sha256: %s\n' "$(sha256sum "$stage3_object" | cut -d' ' -f1)" >&2
    exit 1
fi

mkdir -p "$(dirname "$evidence_file")"
evidence_tmp="$temporary_directory/evidence.txt"
{
    printf 'schema=terlan.self-host.native-fixed-point/v1\n'
    printf 'source=%s\n' "$source_file"
    printf 'stage2_sha256=%s\n' "$(sha256sum "$stage2_object" | cut -d' ' -f1)"
    printf 'stage3_sha256=%s\n' "$(sha256sum "$stage3_object" | cut -d' ' -f1)"
    printf 'equal=true\n'
} > "$evidence_tmp"
mv "$evidence_tmp" "$evidence_file"
