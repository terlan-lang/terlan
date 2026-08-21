#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$repo_root/target/tmp"
work="$(mktemp -d "$repo_root/target/tmp/registry-cli-integration.XXXXXX")"
trap 'find "$work" -depth -delete' EXIT
touch "$work/.terlan-disposable-validation-workspace"
mirror="$work/mirror"
module="$repo_root/tests/fixtures/registry-cli/module"
terlc="$repo_root/target/debug/terlc"

test -x "$terlc" || {
  echo "missing prebuilt terlc: $terlc" >&2
  exit 1
}
mkdir -p "$work/dry" "$work/live" "$work/duplicate"

"$terlc" package publish --dry-run "$module" --out-dir "$work/dry"
"$terlc" package publish --mirror "$mirror" "$module" --out-dir "$work/live"
cmp "$work/dry/package/registry_math-1.0.0.tar.zst" \
  "$work/live/package/registry_math-1.0.0.tar.zst"
cmp "$work/dry/package/registry_math-1.0.0.publish-request.json" \
  "$work/live/package/registry_math-1.0.0.publish-request.json"

if "$terlc" package publish --mirror "$mirror" "$module" --out-dir "$work/duplicate" \
  >"$work/duplicate.stdout" 2>"$work/duplicate.stderr"; then
  echo "duplicate Registry publication unexpectedly succeeded" >&2
  exit 1
fi
rg --fixed-strings 'error[registry_version_immutable]' "$work/duplicate.stderr" >/dev/null

echo "registry CLI integration check passed"
