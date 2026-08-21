#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$repo_root/target/tmp"
work="$(mktemp -d "$repo_root/target/tmp/registry-protocol.XXXXXX")"
trap 'find "$work" -depth -delete' EXIT
touch "$work/.terlan-disposable-validation-workspace"
first="$work/first"
second="$work/second"
terlc="${TERLC:-$repo_root/target/debug/terlc}"

test -x "$terlc" || {
  echo "missing prebuilt terlc: $terlc" >&2
  exit 1
}

mkdir -p "$first/schemas" "$first/fixtures" "$second/schemas" "$second/fixtures"

"$terlc" package protocol --out-dir "$first"
"$terlc" package protocol --out-dir "$second"
diff --recursive --brief "$first" "$second"

test "$(find "$first/schemas" -maxdepth 1 -type f -name '*.schema.json' | wc -l)" -eq 9
test "$(find "$first/fixtures" -maxdepth 1 -type f -name '*.json' | wc -l)" -eq 9
test "$(find "$first" -type f | wc -l)" -eq 19

rg --fixed-strings '"additionalProperties": false' "$first/schemas" >/dev/null
rg --fixed-strings '"max_archive_bytes": 67108864' "$first/fixtures/publish-request.json" >/dev/null
rg --fixed-strings '"max_unpacked_bytes": 268435456' "$first/fixtures/publish-request.json" >/dev/null
rg --fixed-strings '"max_files": 4096' "$first/fixtures/publish-request.json" >/dev/null
rg --fixed-strings '"max_path_bytes": 240' "$first/fixtures/publish-request.json" >/dev/null
rg --fixed-strings '"symlinks": "reject"' "$first/fixtures/publish-request.json" >/dev/null
if rg --ignore-case 'hex compatibility|hex package|hex registry' "$first"; then
  echo "Registry protocol bundle unexpectedly advertises Hex compatibility" >&2
  exit 1
fi

echo "registry protocol schema check passed"
