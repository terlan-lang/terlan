#!/usr/bin/env bash
set -euo pipefail

root_manifest="Cargo.toml"

if ! grep -Eq '^\[workspace\.package\]$' "$root_manifest"; then
  echo "workspace version check failed: $root_manifest is missing [workspace.package]" >&2
  exit 1
fi

if ! grep -Eq '^version = "[0-9]+\.[0-9]+\.[0-9]+"$' "$root_manifest"; then
  echo "workspace version check failed: $root_manifest must define one semantic workspace.package version" >&2
  exit 1
fi

if ! grep -Eq '^edition = "[0-9]{4}"$' "$root_manifest"; then
  echo "workspace version check failed: $root_manifest must define one workspace.package edition" >&2
  exit 1
fi

bad=0
while IFS= read -r manifest; do
  if ! grep -Eq '^version\.workspace = true$' "$manifest"; then
    echo "workspace version check failed: $manifest must use version.workspace = true" >&2
    bad=1
  fi

  if grep -Eq '^version = "[0-9]+\.[0-9]+\.[0-9]+"$' "$manifest"; then
    echo "workspace version check failed: $manifest defines its own package version" >&2
    bad=1
  fi

  if ! grep -Eq '^edition\.workspace = true$' "$manifest"; then
    echo "workspace version check failed: $manifest must use edition.workspace = true" >&2
    bad=1
  fi
done < <(find crates -mindepth 2 -maxdepth 2 -name Cargo.toml | sort)

if [[ "$bad" -ne 0 ]]; then
  exit 1
fi

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root_manifest" | head -1)"
echo "Workspace package metadata is centralized at version $version."
