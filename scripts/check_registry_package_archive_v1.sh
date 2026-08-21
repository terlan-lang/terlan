#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture="$repo_root/tests/fixtures/release-bundle-v1"
mkdir -p "$repo_root/target/tmp"
work="$(mktemp -d "$repo_root/target/tmp/registry-package-archive.XXXXXX")"
trap 'find "$work" -depth -delete' EXIT
touch "$work/.terlan-disposable-validation-workspace"
first="$work/first"
second="$work/second"
terlc="${TERLC:-$repo_root/target/debug/terlc}"

test -x "$terlc" || {
  echo "missing prebuilt terlc: $terlc" >&2
  exit 1
}

mkdir -p "$first" "$second"

"$terlc" package publish --dry-run "$fixture" --out-dir "$first"
"$terlc" package publish --dry-run "$fixture" --out-dir "$second"

first_archive="$first/package/terlan-registry-0.1.0.tar.zst"
first_request="$first/package/terlan-registry-0.1.0.publish-request.json"
second_archive="$second/package/terlan-registry-0.1.0.tar.zst"
second_request="$second/package/terlan-registry-0.1.0.publish-request.json"

cmp "$first_archive" "$second_archive"
cmp "$first_request" "$second_request"
archive_sha256="$(sha256sum "$first_archive" | cut -d ' ' -f 1)"
rg --fixed-strings "\"value\": \"$archive_sha256\"" "$first_request" >/dev/null
rg --fixed-strings '"format": "tar.zst"' "$first_request" >/dev/null
rg --fixed-strings '"symlinks": "reject"' "$first_request" >/dev/null
rg --fixed-strings '"path": "priv/migrations/001_packages.sql"' "$first_request" >/dev/null
if rg --fixed-strings "$repo_root" "$first_request"; then
  echo "publish metadata contains a workspace-local absolute path" >&2
  exit 1
fi
if rg --ignore-case 'secret.txt|DATABASE_URL=' "$first_request"; then
  echo "publish metadata contains secret material" >&2
  exit 1
fi

echo "registry package archive check passed"
