#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: bounded_package_stage.sh SELF_HOST_IMAGE SOURCES_MANIFEST OUTPUT_DIRECTORY" >&2
    exit 2
fi

IMAGE=$1
SOURCES=$2
OUTPUT=$3
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
JOBS=${TERLAN_SELF_HOST_JOBS:-8}
BATCH_UNITS=${TERLAN_SELF_HOST_BATCH_UNITS:-8}

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
if [[ ! "$BATCH_UNITS" =~ ^[1-9][0-9]*$ ]]; then
    echo "TERLAN_SELF_HOST_BATCH_UNITS must be a positive integer" >&2
    exit 2
fi

mkdir -p "$OUTPUT"
mkdir -p "$OUTPUT/interfaces" "$OUTPUT/modules" "$OUTPUT/diagnostics"

UNITS_TEMP="$OUTPUT/units.tsv.tmp.$$"
INTERFACES_TEMP="$OUTPUT/interfaces.manifest.tmp.$$"
ARTIFACTS_TEMP="$OUTPUT/artifacts.manifest.tmp.$$"
trap 'rm -f "$UNITS_TEMP" "$INTERFACES_TEMP" "$ARTIFACTS_TEMP"' EXIT

awk 'NF { count += 1; printf "%06d\t%s\n", count, $0 }' "$SOURCES" > "$UNITS_TEMP"
if [[ ! -f "$OUTPUT/units.tsv" ]] || ! cmp -s "$UNITS_TEMP" "$OUTPUT/units.tsv"; then
    mv "$UNITS_TEMP" "$OUTPUT/units.tsv"
else
    rm -f "$UNITS_TEMP"
fi
awk -F '\t' -v root="$OUTPUT" '{ print root "/interfaces/" $1 ".interface" }' \
    "$OUTPUT/units.tsv" > "$INTERFACES_TEMP"
if [[ ! -f "$OUTPUT/interfaces.manifest" ]] || ! cmp -s "$INTERFACES_TEMP" "$OUTPUT/interfaces.manifest"; then
    mv "$INTERFACES_TEMP" "$OUTPUT/interfaces.manifest"
else
    rm -f "$INTERFACES_TEMP"
fi
awk -F '\t' -v root="$OUTPUT" '{ print root "/modules/" $1 ".abi1.json" }' \
    "$OUTPUT/units.tsv" > "$ARTIFACTS_TEMP"
if [[ ! -f "$OUTPUT/artifacts.manifest" ]] || ! cmp -s "$ARTIFACTS_TEMP" "$OUTPUT/artifacts.manifest"; then
    mv "$ARTIFACTS_TEMP" "$OUTPUT/artifacts.manifest"
else
    rm -f "$ARTIFACTS_TEMP"
fi

INTERFACE_ARGUMENTS="$OUTPUT/interface.arguments"
: > "$INTERFACE_ARGUMENTS"
INTERFACES_CHANGED=0
while IFS=$'\t' read -r unit source; do
    interface="$OUTPUT/interfaces/$unit.interface"
    if [[ -s "$interface" ]] \
        && grep -q $'^interface\tterlan.self-host.interface-catalog/v4\t' "$interface" \
        && [[ "$interface" -nt "$source" ]] \
        && [[ "$interface" -nt "$IMAGE" ]]; then
        continue
    fi
    INTERFACES_CHANGED=1
    rm -f "$interface"
    printf '%s\n%s\n' "$source" "$interface" >> "$INTERFACE_ARGUMENTS"
done < "$OUTPUT/units.tsv"

if [[ -s "$INTERFACE_ARGUMENTS" ]]; then
    xargs -d '\n' -P "$JOBS" -n "$((BATCH_UNITS * 2))" \
        "$IMAGE" interface-units < "$INTERFACE_ARGUMENTS"
fi

while IFS=$'\t' read -r unit _source; do
    test -s "$OUTPUT/interfaces/$unit.interface"
done < "$OUTPUT/units.tsv"

BACKEND_ARGUMENTS="$OUTPUT/backend.arguments"
: > "$BACKEND_ARGUMENTS"
while IFS=$'\t' read -r unit source; do
    artifact="$OUTPUT/modules/$unit.abi1.json"
    diagnostics="$OUTPUT/diagnostics/$unit.txt"
    if [[ "$INTERFACES_CHANGED" -eq 0 ]] \
        && [[ -s "$artifact" && ! -s "$diagnostics" ]] \
        && [[ "$artifact" -nt "$source" ]] \
        && [[ "$artifact" -nt "$OUTPUT/interfaces.manifest" ]] \
        && [[ "$artifact" -nt "$IMAGE" ]]; then
        continue
    fi
    rm -f "$artifact"
    : > "$diagnostics"
    printf '%s\n%s\n%s\n' "$source" "$artifact" "$diagnostics" >> "$BACKEND_ARGUMENTS"
done < "$OUTPUT/units.tsv"

if [[ -s "$BACKEND_ARGUMENTS" ]]; then
    xargs -d '\n' -P "$JOBS" -n "$((BATCH_UNITS * 3))" \
        "$IMAGE" backend-units "$OUTPUT/interfaces.manifest" < "$BACKEND_ARGUMENTS"
fi

while IFS=$'\t' read -r unit _source; do
    test -s "$OUTPUT/modules/$unit.abi1.json"
    test ! -s "$OUTPUT/diagnostics/$unit.txt"
done < "$OUTPUT/units.tsv"

expected=$(awk 'END { print NR + 0 }' "$OUTPUT/units.tsv")
interfaces=$(find "$OUTPUT/interfaces" -maxdepth 1 -type f -size +0c | wc -l)
artifacts=$(find "$OUTPUT/modules" -maxdepth 1 -type f -size +0c | wc -l)
if [[ "$interfaces" -ne "$expected" || "$artifacts" -ne "$expected" ]]; then
    echo "bounded package count mismatch: expected=$expected interfaces=$interfaces artifacts=$artifacts" >&2
    exit 1
fi

mapfile -t ABI1_ARTIFACTS < "$OUTPUT/artifacts.manifest"
if [[ -n "${TERLAN_SELF_HOST_ABI_VALIDATOR:-}" ]]; then
    "$TERLAN_SELF_HOST_ABI_VALIDATOR" "${ABI1_ARTIFACTS[@]}"
else
    cargo run --quiet --manifest-path "$ROOT/Cargo.toml" -p terlan \
        --bin terlan-self-host-abi1 -- "${ABI1_ARTIFACTS[@]}"
fi

printf 'expected=%s interfaces=%s artifacts=%s\n' "$expected" "$interfaces" "$artifacts"
