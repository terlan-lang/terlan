#!/usr/bin/env bash
set -euo pipefail

# Executes a Terlan workspace binary without paying for `cargo run` on every
# validation recipe. A missing or stale binary is rebuilt once; subsequent
# invocations execute it directly.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

binary="${1:-}"
features="${2:-}"
if [[ -z "$binary" || "$#" -lt 3 || "${3:-}" != "--" ]]; then
  echo "usage: scripts/run_prebuilt_terlan_binary.sh <binary> <features-or-none> -- [arguments...]" >&2
  exit 2
fi
shift 3

case "$binary" in
  terlc|terlan-quality|terlan-benchmark) ;;
  *)
    echo "unsupported prebuilt Terlan binary: $binary" >&2
    exit 2
    ;;
esac

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) executable="target/debug/$binary.exe" ;;
  *) executable="target/debug/$binary" ;;
esac
profile_stamp="target/debug/.terlan-prebuilt-$binary.profile"

binary_is_stale() {
  [[ -x "$executable" ]] || return 0
  [[ -f "$profile_stamp" ]] || return 0
  [[ "$(<"$profile_stamp")" == "$features" ]] || return 0
  # Another Cargo invocation can replace target/debug/<binary> with a build
  # from a different feature set. The profile stamp makes that overwrite loud.
  [[ ! "$executable" -nt "$profile_stamp" ]] || return 0
  find \
    Cargo.toml Cargo.lock rust-toolchain.toml \
    crates/terlan/Cargo.toml crates/terlan/build.rs crates/terlan/src \
    crates/terlan-archive crates/terlan-runtime-abi \
    std/native/libpq/generated/native/rust \
    -type f -newer "$executable" -print -quit 2>/dev/null \
    | grep -q .
}

if binary_is_stale; then
  build=(cargo build --locked -p terlan --bin "$binary")
  if [[ -n "$features" && "$features" != "none" ]]; then
    build+=(--features "$features")
  fi
  "${build[@]}"
  profile_stamp_tmp="$profile_stamp.$$"
  printf '%s\n' "$features" > "$profile_stamp_tmp"
  mv "$profile_stamp_tmp" "$profile_stamp"
fi

if [[ ! -x "$executable" ]]; then
  echo "prebuilt Terlan binary was not produced: $executable" >&2
  exit 1
fi

exec "$executable" "$@"
