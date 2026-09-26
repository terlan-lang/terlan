#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
INPUT="$ROOT/compiler/self_host/corpus/backend/Arithmetic.terl"
WORK="${TMPDIR:-/tmp}/terlan-self-host-backend.$$"
mkdir -p "$WORK"
trap 'find "$WORK" -depth -delete' EXIT

projected="$WORK/projected.json"
canonical="$WORK/canonical.json"

if [[ -n "${TERLAN_SELF_HOST_BACKEND_IMAGE:-}" ]]; then
    image="$TERLAN_SELF_HOST_BACKEND_IMAGE"
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

"$image" backend "$INPUT" "$projected"
test -s "$projected"
cargo run --quiet -p terlan --bin terlan-self-host-parity -- canonicalize "$projected" "$canonical"

if ! cmp -s "$projected" "$canonical"; then
    diff -u "$projected" "$canonical" || true
    echo "ABI-1 backend contract parity failed" >&2
    exit 1
fi

cargo run --quiet \
    --manifest-path "$ROOT/../terlan-vm/erts/rust/terlan_vm/Cargo.toml" \
    --bin terlan-self-host-accept -- "$projected" >/dev/null

echo "ABI-1 backend contract parity and VM consumption passed"
