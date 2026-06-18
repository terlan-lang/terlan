#!/usr/bin/env bash
set -euo pipefail

# Verifies that release-facing version metadata matches the workspace version.
#
# Inputs:
# - Optional first argument: expected semantic version without a leading `v`.
# - `Cargo.toml`: workspace package version source of truth.
# - `install.sh`: default installer tag.
# - `CHANGELOG.md`: user-facing release section.
# - `README.md`: current published version text.
#
# Outputs:
# - Exit status 0 and a short success message when metadata is aligned.
# - Exit status 1 with a specific diagnostic when any metadata source drifts.
#
# Transformation:
# - Reads the workspace version from `Cargo.toml`.
# - Uses the optional expected version to validate publication commands.
# - Checks all user-facing release metadata against the selected version.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

expected_version="${1:-}"
workspace_version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

if [[ -z "$workspace_version" ]]; then
  echo "release version metadata check failed: Cargo.toml workspace version is missing"
  exit 1
fi

if [[ -n "$expected_version" && "$expected_version" == v* ]]; then
  echo "release version metadata check failed: expected version must not include leading v"
  exit 1
fi

if [[ -n "$expected_version" && "$workspace_version" != "$expected_version" ]]; then
  echo "release version metadata check failed: workspace package version $workspace_version != $expected_version"
  exit 1
fi

version="${expected_version:-$workspace_version}"

if ! grep -q 'VERSION="${TERLAN_VERSION:-v'"$version"'}"' install.sh; then
  echo "release version metadata check failed: install.sh default version is not v$version"
  exit 1
fi

if ! grep -q '^## '"$version"'$' CHANGELOG.md; then
  echo "release version metadata check failed: CHANGELOG.md is missing section ## $version"
  exit 1
fi

if ! grep -Eq '^Current version: `'"$version"'`\.?$' README.md; then
  echo "release version metadata check failed: README.md current version is not $version"
  exit 1
fi

echo "Release metadata matches workspace version $version."
