#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
terlc="${TERLC:-$repo_root/target/debug/terlc}"

test -x "$terlc" || {
  echo "missing prebuilt terlc: $terlc" >&2
  exit 1
}

help_text="$("$terlc" help package)"
rg --fixed-strings 'terlc package resolve --registry <url> --trust-root <pin.json>' <<<"$help_text" >/dev/null
rg --fixed-strings 'snapshot_sha256' "$repo_root/docs/package/TERLAN_PACKAGE_LOCKFILE.md" >/dev/null
rg --fixed-strings 'existing lock' "$repo_root/docs/package/TERLAN_PACKAGE_LOCKFILE.md" >/dev/null
rg --fixed-strings 'verified cache' "$repo_root/docs/package/TERLAN_PACKAGE_LOCKFILE.md" >/dev/null
rg --fixed-strings -- '--offline' "$repo_root/docs/package/TERLAN_PACKAGE_LOCKFILE.md" >/dev/null
echo "registry lockfile resolver check passed"
