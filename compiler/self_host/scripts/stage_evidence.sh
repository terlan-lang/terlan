#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: stage_evidence.sh SOURCES_MANIFEST OUTPUT_DIRECTORY" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE=${TERLAN_SELF_HOST_IMAGE:-}
SOURCES=$1
OUTPUT=$2
JOBS=${TERLAN_SELF_HOST_JOBS:-8}

if [[ -z "$IMAGE" || ! -x "$IMAGE" ]]; then
    echo "TERLAN_SELF_HOST_IMAGE must name an executable self-host image" >&2
    exit 2
fi
if [[ ! -s "$SOURCES" ]]; then
    echo "source manifest is missing or empty: $SOURCES" >&2
    exit 2
fi
if [[ ! "$JOBS" =~ ^[1-9][0-9]*$ ]]; then
    echo "TERLAN_SELF_HOST_JOBS must be a positive integer" >&2
    exit 2
fi

mkdir -p "$OUTPUT/tokens" "$OUTPUT/syntax" "$OUTPUT/semantics" "$OUTPUT/pending"
export IMAGE OUTPUT

for layer in tokens syntax semantics; do
    pending="$OUTPUT/pending/$layer.tsv"
    : > "$pending"
    while IFS= read -r source; do
        [[ -z "$source" || "$source" == \#* ]] && continue
        name=${source//\//_}
        name=${name%.terl}
        artifact="$OUTPUT/$layer/$name.$layer"
        if [[ ! -s "$artifact" ]]; then
            printf '%s\t%s\n' "$source" "$artifact" >> "$pending"
        fi
    done < "$SOURCES"
    if [[ -s "$pending" ]]; then
        export LAYER=$layer
        xargs -d '\n' -P "$JOBS" -I '{}' bash -c '
            row=$1
            source=${row%%$'"'"'\t'"'"'*}
            artifact=${row#*$'"'"'\t'"'"'}
            "$IMAGE" stage-layer "$LAYER" "$source" "$artifact"
            test -s "$artifact"
        ' _ '{}' < "$pending"
    fi
done

expected=$(awk 'NF && $0 !~ /^#/ { count++ } END { print count + 0 }' "$SOURCES")
for layer in tokens syntax semantics; do
    actual=$(find "$OUTPUT/$layer" -maxdepth 1 -type f -size +0c | wc -l)
    if [[ "$actual" -ne "$expected" ]]; then
        echo "stage evidence count mismatch: layer=$layer expected=$expected actual=$actual" >&2
        exit 1
    fi
done

printf 'expected=%s tokens=%s syntax=%s semantics=%s\n' \
    "$expected" "$expected" "$expected" "$expected"
