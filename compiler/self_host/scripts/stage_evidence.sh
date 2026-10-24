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

mkdir -p "$OUTPUT/tokens" "$OUTPUT/syntax" "$OUTPUT/semantic-common" \
    "$OUTPUT/semantic-supplemental" "$OUTPUT/pending"
export IMAGE OUTPUT

for layer in tokens syntax semantic-common semantic-supplemental; do
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
            case "$LAYER" in
                tokens|syntax)
                    "$IMAGE" stage-layer "$LAYER" "$source" "$artifact"
                    ;;
                semantic-common)
                    "$IMAGE" semantic "$source" "$artifact"
                    ;;
                semantic-supplemental)
                    "$IMAGE" semantic-full "$source" "$artifact"
                    ;;
            esac
            test -s "$artifact"
        ' _ '{}' < "$pending"
    fi
done

expected=$(awk 'NF && $0 !~ /^#/ { count++ } END { print count + 0 }' "$SOURCES")
for layer in tokens syntax semantic-common semantic-supplemental; do
    actual=$(find "$OUTPUT/$layer" -maxdepth 1 -type f -size +0c | wc -l)
    if [[ "$actual" -ne "$expected" ]]; then
        echo "stage evidence count mismatch: layer=$layer expected=$expected actual=$actual" >&2
        exit 1
    fi
done

: > "$OUTPUT/tokens.bundle"
: > "$OUTPUT/syntax.bundle"
: > "$OUTPUT/semantics.bundle"

append_frame() {
    local identity=$1
    local artifact=$2
    local bundle=$3
    printf '%s\t%s\n' "$identity" "$(wc -c < "$artifact")" >> "$bundle"
    cat "$artifact" >> "$bundle"
    printf '\n' >> "$bundle"
}

while IFS= read -r source; do
    [[ -z "$source" || "$source" == \#* ]] && continue
    name=${source//\//_}
    name=${name%.terl}
    append_frame "$source" "$OUTPUT/tokens/$name.tokens" "$OUTPUT/tokens.bundle"
    append_frame "$source" "$OUTPUT/syntax/$name.syntax" "$OUTPUT/syntax.bundle"
    append_frame "$source:common" \
        "$OUTPUT/semantic-common/$name.semantic-common" "$OUTPUT/semantics.bundle"
    append_frame "$source:supplemental" \
        "$OUTPUT/semantic-supplemental/$name.semantic-supplemental" "$OUTPUT/semantics.bundle"
done < "$SOURCES"

test -s "$OUTPUT/tokens.bundle"
test -s "$OUTPUT/syntax.bundle"
test -s "$OUTPUT/semantics.bundle"

printf 'expected=%s tokens=%s syntax=%s common=%s supplemental=%s\n' \
    "$expected" "$expected" "$expected" "$expected" "$expected"
