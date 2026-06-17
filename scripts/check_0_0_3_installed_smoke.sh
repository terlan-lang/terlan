#!/usr/bin/env bash
set -euo pipefail

version="${VERSION:-0.0.3}"
terlc_bin="${TERLC_BIN:-terlc}"
tmp_dir="$(mktemp -d)"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

if ! command -v "$terlc_bin" >/dev/null 2>&1; then
  printf '0.0.3 installed smoke failed: %s is not on PATH\n' "$terlc_bin" >&2
  exit 1
fi

reported_version="$("$terlc_bin" version)"
if [[ "$reported_version" != "terlc $version" ]]; then
  printf '0.0.3 installed smoke failed: expected terlc %s, got %s\n' \
    "$version" "$reported_version" >&2
  exit 1
fi

cd "$tmp_dir"
"$terlc_bin" init hello-terlan >/dev/null

if [[ ! -f hello-terlan/src/hello_terlan/Main.terl ]]; then
  printf '0.0.3 installed smoke failed: init did not create Main.terl\n' >&2
  exit 1
fi

old_source_glob="*.t""l"
old_interface_glob="*.t""li"
if find hello-terlan -name "$old_source_glob" -o -name "$old_interface_glob" | grep -q .; then
  printf '0.0.3 installed smoke failed: init created stale source-extension files\n' >&2
  exit 1
fi

cd hello-terlan
"$terlc_bin" build >/dev/null

if [[ ! -x _build/bin/hello-terlan ]]; then
  printf '0.0.3 installed smoke failed: build did not create executable launcher\n' >&2
  exit 1
fi

actual_output="$("./_build/bin/hello-terlan")"
if [[ "$actual_output" != "hello from Terlan" ]]; then
  printf '0.0.3 installed smoke failed: launcher output mismatch\nexpected: hello from Terlan\nactual: %s\n' \
    "$actual_output" >&2
  exit 1
fi

"$terlc_bin" test >/dev/null
"$terlc_bin" doc std --check >/dev/null
"$terlc_bin" doc std >/dev/null

if [[ ! -f _build/index.html ]]; then
  printf '0.0.3 installed smoke failed: docs index was not generated\n' >&2
  exit 1
fi

printf '0.0.3 installed compiler smoke passed for %s.\n' "$reported_version"
