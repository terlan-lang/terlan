#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
corpus="$root/compiler/self_host/corpus"
manifest="$corpus/CORPUS_PHASE_2.tsv"
candidate="${TERLAN_SELF_HOST_IMAGE:-$root/target/quality/self-host-bootstrap-current-39/bin/terlan-self-host}"
rust_oracle="${TERLAN_CONCRETE_ORACLE:-$root/target/debug/terlan-concrete-oracle}"
formatter="${TERLAN_COMPILER:-$root/target/debug/terlc}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

checked=0
while IFS=$'\t' read -r first second rest; do
    [[ -z "${first:-}" || "$first" == \#* ]] && continue
    if [[ "$first" == *.terl ]]; then
        fixture="$first"
        expectation="$second"
    else
        expectation="$first"
        fixture="$second"
    fi
    [[ "$expectation" == accepted ]] || continue
    [[ "$fixture" = /* ]] || fixture="$corpus/$fixture"

    checked=$((checked + 1))
    case_dir="$work/$checked"
    mkdir -p "$case_dir"
    source="$case_dir/$(basename "$fixture")"
    cp "$fixture" "$source"

    "$rust_oracle" "$source" "$case_dir/rust-before.json"
    "$candidate" concrete "$source" "$case_dir/candidate-before.json"
    cmp "$case_dir/rust-before.json" "$case_dir/candidate-before.json"

    "$formatter" fmt --write "$source"
    cp "$source" "$case_dir/formatted-once.terl"
    "$rust_oracle" "$source" "$case_dir/rust-after.json"
    "$candidate" concrete "$source" "$case_dir/candidate-after.json"
    cmp "$case_dir/rust-after.json" "$case_dir/candidate-after.json"

    "$formatter" fmt --write "$source"
    cmp "$case_dir/formatted-once.terl" "$source"
done < "$manifest"

[[ "$checked" -gt 0 ]] || { echo 'Phase 2 corpus contains no accepted fixtures' >&2; exit 1; }
printf 'Phase 2 syntax roundtrip: %d fixtures\n' "$checked"
