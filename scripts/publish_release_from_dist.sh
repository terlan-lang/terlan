#!/usr/bin/env bash
set -euo pipefail

# Publishes already-built Terlan release artifacts from dist/.
#
# Inputs:
# - First argument: release version without leading v.
# - dist/terlc-* artifacts created and smoke-tested by make publish-preflight.
# - CHANGELOG.md section matching the version.
# - GitHub CLI authenticated with permission to create releases and upload
#   assets.
#
# Outputs:
# - A GitHub release named v<version> with release notes from CHANGELOG.md.
# - Uploaded dist/terlc-* assets, replacing existing assets with the same name.
#
# Transformation:
# - Keeps release artifact construction local and reproducible. GitHub Actions
#   may validate the tag, but it is not the artifact builder or publisher.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${1:-}"
if [[ -z "$version" ]]; then
  echo "usage: scripts/publish_release_from_dist.sh <version-without-v>" >&2
  exit 2
fi
if [[ "$version" == v* ]]; then
  echo "release version must not include leading v: $version" >&2
  exit 2
fi

tag="v$version"

if ! command -v gh >/dev/null 2>&1; then
  echo "publish requires GitHub CLI: install gh and run gh auth login" >&2
  exit 127
fi

if ! gh auth status >/dev/null 2>&1; then
  echo "publish requires authenticated GitHub CLI: run gh auth login" >&2
  exit 1
fi

if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "local tag $tag is missing; run make publish VERSION=$version" >&2
  exit 1
fi

python3 -B tools/release_promotion_pipeline.py verify --version "$version"
mapfile -d '' artifacts < <(
  python3 -B tools/release_promotion_pipeline.py list --version "$version"
)

notes="$(mktemp)"
trap 'rm -f "$notes"' EXIT
candidate_seal="$(python3 -B tools/release_promotion_pipeline.py digest --version "$version")"
printf 'Release candidate seal: `%s`\n\n' "$candidate_seal" > "$notes"
awk -v version="$version" '
  $0 == "## " version { in_section = 1; next }
  in_section && /^## / { exit }
  in_section { print }
' CHANGELOG.md >> "$notes"
if [[ ! -s "$notes" ]]; then
  echo "CHANGELOG.md is missing release notes for $version" >&2
  exit 1
fi

if gh release view "$tag" >/dev/null 2>&1; then
  gh release edit "$tag" --title "$tag" --notes-file "$notes"
else
  gh release create "$tag" --title "$tag" --notes-file "$notes"
fi

for artifact in "${artifacts[@]}"; do
  echo "Uploading $artifact"
  gh release upload "$tag" "$artifact" --clobber
done

echo "Published $tag with ${#artifacts[@]} local artifact(s)."
