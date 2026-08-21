#!/usr/bin/env bash
set -euo pipefail

# Publishes already-built Terlan release artifacts from dist/.
#
# Inputs:
# - First argument: release version without leading v.
# - dist/terlc-* artifacts downloaded from and smoke-tested by the exact
#   successful release-validation workflow.
# - CHANGELOG.md section matching the version.
# - GitHub CLI authenticated with permission to create releases and upload
#   assets.
#
# Outputs:
# - A GitHub release named v<version> with release notes from CHANGELOG.md.
# - An exact, verified set of uploaded release-candidate assets.
#
# Transformation:
# - Keeps publication local while consuming the native artifacts produced,
#   validated, and attested by GitHub Actions for the exact release commit.

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
repository="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{ print $1 }'
  else
    echo "publish requires sha256sum or shasum" >&2
    return 127
  fi
}

read_remote_assets() {
  gh api "repos/$repository/releases/tags/$tag" \
    --jq '.assets[] | [.name, (.size | tostring), (.digest // "")] | @tsv' \
    | LC_ALL=C sort
}

if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "local tag $tag is missing; run make publish" >&2
  exit 1
fi
if [[ "$(git cat-file -t "refs/tags/$tag")" != "tag" ]]; then
  echo "local tag $tag must be annotated" >&2
  exit 1
fi

tag_commit="$(git rev-parse "refs/tags/$tag^{commit}")"
head_commit="$(git rev-parse HEAD)"
if [[ "$tag_commit" != "$head_commit" ]]; then
  echo "local tag $tag points to $tag_commit, not HEAD $head_commit" >&2
  exit 1
fi

release_promotion=(
  target/debug/terlan-vm run
  target/self-validation/release-promotion/vm/scripts_ReleasePromotion.tvm
  --script-eval --
)

TERLAN_RELEASE_ROOT="$repo_root" "${release_promotion[@]}" verify --version "$version"
mapfile -t artifacts < <(
  TERLAN_RELEASE_ROOT="$repo_root" "${release_promotion[@]}" list --version "$version"
)

if [[ "${#artifacts[@]}" -eq 0 ]]; then
  echo "sealed release candidate contains no publishable artifacts" >&2
  exit 1
fi

work_dir="$(mktemp -d)"
notes="$work_dir/notes.md"
changelog_section="$work_dir/changelog.md"
expected_assets="$work_dir/expected-assets.txt"
actual_assets="$work_dir/actual-assets.txt"
expected_asset_metadata="$work_dir/expected-asset-metadata.tsv"
actual_asset_metadata="$work_dir/actual-asset-metadata.tsv"
published_body="$work_dir/published-body.md"
trap 'rm -rf "$work_dir"' EXIT
candidate_seal="$(TERLAN_RELEASE_ROOT="$repo_root" "${release_promotion[@]}" digest --version "$version")"
awk -v version="$version" '
  $0 == "## " version { in_section = 1; next }
  in_section && /^## / { exit }
  in_section { print }
' CHANGELOG.md > "$changelog_section"
if ! grep -q '[^[:space:]]' "$changelog_section"; then
  echo "CHANGELOG.md is missing release notes for $version" >&2
  exit 1
fi
printf 'Release candidate seal: `%s`\n\n' "$candidate_seal" > "$notes"
cat "$changelog_section" >> "$notes"

: > "$expected_assets"
: > "$expected_asset_metadata"
for artifact in "${artifacts[@]}"; do
  case "$artifact" in
    dist/*) ;;
    *)
      echo "release candidate contains an artifact outside dist/: $artifact" >&2
      exit 1
      ;;
  esac
  if [[ ! -f "$artifact" || -L "$artifact" ]]; then
    echo "release candidate artifact must be a regular, non-symlink file: $artifact" >&2
    exit 1
  fi
  artifact_name="$(basename "$artifact")"
  artifact_size="$(wc -c < "$artifact" | tr -d '[:space:]')"
  artifact_digest="$(sha256_file "$artifact")"
  printf '%s\n' "$artifact_name" >> "$expected_assets"
  printf '%s\t%s\tsha256:%s\n' "$artifact_name" "$artifact_size" "$artifact_digest" \
    >> "$expected_asset_metadata"
done
LC_ALL=C sort -o "$expected_assets" "$expected_assets"
LC_ALL=C sort -o "$expected_asset_metadata" "$expected_asset_metadata"

if [[ "$(wc -l < "$expected_assets" | tr -d ' ')" -ne "${#artifacts[@]}" ]]; then
  echo "release candidate contains duplicate artifact names" >&2
  exit 1
fi

if gh release view "$tag" >/dev/null 2>&1; then
  is_draft="$(gh release view "$tag" --json isDraft --jq .isDraft)"
  if [[ "$is_draft" != "true" ]]; then
    read_remote_assets > "$actual_asset_metadata"
    cut -f1 "$actual_asset_metadata" > "$actual_assets"
    gh release view "$tag" --json body --jq .body > "$published_body"
    if cmp -s "$expected_assets" "$actual_assets" \
      && cmp -s "$expected_asset_metadata" "$actual_asset_metadata" \
      && grep -Fq "Release candidate seal: \`$candidate_seal\`" "$published_body"; then
      echo "Release $tag is already public with the exact sealed asset set."
      exit 0
    fi
    echo "release $tag is public but does not match the sealed candidate; refusing to mutate it" >&2
    exit 1
  fi
  gh release edit "$tag" --title "$tag" --notes-file "$notes"
else
  gh release create "$tag" --draft --verify-tag --title "$tag" --notes-file "$notes"
fi

for artifact in "${artifacts[@]}"; do
  echo "Uploading $artifact"
  gh release upload "$tag" "$artifact" --clobber
done

read_remote_assets > "$actual_asset_metadata"
cut -f1 "$actual_asset_metadata" > "$actual_assets"
if ! cmp -s "$expected_assets" "$actual_assets"; then
  echo "uploaded asset set does not exactly match the sealed candidate" >&2
  diff -u "$expected_assets" "$actual_assets" >&2 || true
  echo "release remains a draft" >&2
  exit 1
fi
if ! cmp -s "$expected_asset_metadata" "$actual_asset_metadata"; then
  echo "uploaded asset sizes or SHA-256 digests do not match the sealed candidate" >&2
  diff -u "$expected_asset_metadata" "$actual_asset_metadata" >&2 || true
  echo "release remains a draft" >&2
  exit 1
fi

gh release edit "$tag" --draft=false

echo "Published $tag with ${#artifacts[@]} sealed artifact(s)."
