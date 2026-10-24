#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: native_seed_image.sh OUTPUT_DIRECTORY" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUTPUT=$1
TERLC=${TERLC:-$ROOT/target/debug/terlc}
TERLAN_VM=${TERLAN_VM:-$ROOT/target/release/terlan-vm}
TERLAN_BUILD_JOBS=${TERLAN_BUILD_JOBS:-8}
TERLAN_BUILD_LOW_MEMORY=${TERLAN_BUILD_LOW_MEMORY:-1}
TERLAN_NATIVE_AOT_TIMINGS=${TERLAN_NATIVE_AOT_TIMINGS:-1}
export TERLAN_BUILD_JOBS TERLAN_BUILD_LOW_MEMORY TERLAN_NATIVE_AOT_TIMINGS
BUILD="$OUTPUT/build"
IMAGE_DIR="$OUTPUT/image"
RUNTIME_DIR="$OUTPUT/runtime"
BIN_DIR="$OUTPUT/bin"

for executable in "$TERLC" "$TERLAN_VM"; do
    if [[ ! -x "$executable" ]]; then
        echo "required bootstrap executable is missing: $executable" >&2
        exit 2
    fi
done

rm -rf "$BUILD"
mkdir -p "$BUILD" "$IMAGE_DIR" "$RUNTIME_DIR" "$BIN_DIR"

"$TERLC" --timings --incremental build \
    --cache-dir "$ROOT/target/quality/self-host-build-cache" \
    "$ROOT/compiler/self_host" \
    --target terlan-vm \
    --out-dir "$BUILD"

mapfile -t IMAGES < <(find "$BUILD/vm" -maxdepth 1 -type f -name '*.tvm' -print | sort)
if [[ ${#IMAGES[@]} -ne 1 ]]; then
    echo "expected one self-host native image, found ${#IMAGES[@]}" >&2
    exit 1
fi

install -m 0755 "$TERLAN_VM" "$RUNTIME_DIR/terlan-vm"
install -m 0644 "${IMAGES[0]}" "$IMAGE_DIR/terlan-self-host.tvm"

cat > "$BIN_DIR/terlan-self-host" <<'WRAPPER'
#!/usr/bin/env sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec "$ROOT/runtime/terlan-vm" run \
    "$ROOT/image/terlan-self-host.tvm" \
    --entry self_host.Main.main \
    -- "$@"
WRAPPER
chmod 0755 "$BIN_DIR/terlan-self-host"

printf '%s\n' "$BIN_DIR/terlan-self-host"
