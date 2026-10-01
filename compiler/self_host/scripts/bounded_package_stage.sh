#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: bounded_package_stage.sh SELF_HOST_IMAGE SOURCES_MANIFEST OUTPUT_DIRECTORY" >&2
    exit 2
fi

IMAGE=$1
SOURCES=$2
OUTPUT=$3
JOBS=${TERLAN_SELF_HOST_JOBS:-8}

if [[ ! -x "$IMAGE" ]]; then
    echo "self-host image is not executable: $IMAGE" >&2
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

mkdir -p "$OUTPUT"
mkdir -p "$OUTPUT/interfaces" "$OUTPUT/modules" "$OUTPUT/diagnostics"

awk 'NF { count += 1; printf "%06d\t%s\n", count, $0 }' "$SOURCES" > "$OUTPUT/units.tsv"
awk -F '\t' -v root="$OUTPUT" '{ print root "/interfaces/" $1 ".interface" }' \
    "$OUTPUT/units.tsv" > "$OUTPUT/interfaces.manifest"
awk -F '\t' -v root="$OUTPUT" '{ print root "/modules/" $1 ".abi1.json" }' \
    "$OUTPUT/units.tsv" > "$OUTPUT/artifacts.manifest"

export IMAGE OUTPUT
xargs -d '\n' -P "$JOBS" -I '{}' bash -c '
    row=$1
    unit=${row%%$'"'"'\t'"'"'*}
    source=${row#*$'"'"'\t'"'"'}
    interface="$OUTPUT/interfaces/$unit.interface"
    if [[ -s "$interface" ]]; then
        exit 0
    fi
    "$IMAGE" interface-unit "$source" "$interface"
    test -s "$interface"
' _ '{}' < "$OUTPUT/units.tsv"

export INTERFACE_MANIFEST="$OUTPUT/interfaces.manifest"
xargs -d '\n' -P "$JOBS" -I '{}' bash -c '
    row=$1
    unit=${row%%$'"'"'\t'"'"'*}
    source=${row#*$'"'"'\t'"'"'}
    artifact="$OUTPUT/modules/$unit.abi1.json"
    diagnostics="$OUTPUT/diagnostics/$unit.txt"
    if [[ -s "$artifact" && ! -s "$diagnostics" ]]; then
        exit 0
    fi
    : > "$diagnostics"
    "$IMAGE" backend-unit "$source" "$INTERFACE_MANIFEST" "$artifact" "$diagnostics"
    test -s "$artifact"
    test ! -s "$diagnostics"
' _ '{}' < "$OUTPUT/units.tsv"

expected=$(awk 'END { print NR + 0 }' "$OUTPUT/units.tsv")
interfaces=$(find "$OUTPUT/interfaces" -maxdepth 1 -type f -size +0c | wc -l)
artifacts=$(find "$OUTPUT/modules" -maxdepth 1 -type f -size +0c | wc -l)
if [[ "$interfaces" -ne "$expected" || "$artifacts" -ne "$expected" ]]; then
    echo "bounded package count mismatch: expected=$expected interfaces=$interfaces artifacts=$artifacts" >&2
    exit 1
fi

printf 'expected=%s interfaces=%s artifacts=%s\n' "$expected" "$interfaces" "$artifacts"
