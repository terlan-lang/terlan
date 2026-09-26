#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
INPUT="$ROOT/compiler/self_host/corpus/semantic/Bindings.terl"
WORK="${TMPDIR:-/tmp}/terlan-self-host-semantic.$$"
mkdir -p "$WORK"
trap 'rm -rf "$WORK"' EXIT

rust="$WORK/rust.tsv"
projected="$WORK/projected.tsv"
supplemental="$WORK/projected-full.tsv"
cargo run --quiet -p terlan --bin terlan-semantic-oracle -- "$INPUT" "$rust"

if [[ -n "${TERLAN_SELF_HOST_SEMANTIC_IMAGE:-}" ]]; then
    image="$TERLAN_SELF_HOST_SEMANTIC_IMAGE"
else
    image_root="$WORK/image"
    terlc="${TERLAN_TERLC:-$ROOT/target/release/terlc}"
    if [[ ! -x "$terlc" ]]; then
        cargo build --quiet --release -p terlan --bin terlc
    fi
    "$terlc" --incremental build \
        "$ROOT/compiler/self_host_semantic_evidence" \
        --target terlan-vm \
        --release \
        --out-dir "$image_root"
    image="$image_root/bin/terlan-self-host-semantic-evidence"
fi

"$image" semantic "$INPUT" "$projected"
"$image" semantic-full "$INPUT" "$supplemental"

if ! cmp -s "$rust" "$projected"; then
    diff -u "$rust" "$projected" || true
    echo "semantic symbol and diagnostic parity failed" >&2
    exit 1
fi
test -s "$supplemental"
echo "semantic common parity passed; supplemental inferred-type evidence emitted"
