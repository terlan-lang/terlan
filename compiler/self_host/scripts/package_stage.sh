#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "usage: package_stage.sh OUTPUT_DIRECTORY [SEED_DIRECTORY]" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUTPUT=$1
SEED=${2:-}
TERLC=${TERLC:-target/debug/terlc}
PROFILE=${TERLAN_SELF_HOST_PROFILE:-release}
CACHE=${TERLAN_SELF_HOST_CACHE:-$ROOT/target/quality/self-host-build-cache}
JOBS=${TERLAN_SELF_HOST_JOBS:-8}

if [[ ! "$JOBS" =~ ^[1-9][0-9]*$ ]]; then
    echo "TERLAN_SELF_HOST_JOBS must be a positive integer" >&2
    exit 2
fi

case "$PROFILE" in
    development)
        PROFILE_ARGS=()
        ;;
    release)
        PROFILE_ARGS=(--release)
        ;;
    *)
        echo "unsupported self-host package profile: $PROFILE" >&2
        exit 2
        ;;
esac

if [[ -n "$SEED" ]]; then
    if [[ ! -d "$SEED" ]]; then
        echo "seed directory does not exist: $SEED" >&2
        exit 2
    fi
    if [[ -d "$OUTPUT" ]] && find "$OUTPUT" -mindepth 1 -print -quit | grep -q .; then
        echo "seeded output directory must be empty: $OUTPUT" >&2
        exit 2
    fi
    mkdir -p "$OUTPUT"
    cp -a --reflink=auto "$SEED"/. "$OUTPUT"/
fi

TERLAN_BUILD_JOBS="$JOBS" "$TERLC" --incremental build \
    --cache-dir "$CACHE" \
    "$ROOT/compiler/self_host" \
    --target terlan-vm \
    "${PROFILE_ARGS[@]}" \
    --out-dir "$OUTPUT"
