#!/usr/bin/env bash
set -euo pipefail

# Publishes already-built Terlan release artifacts from dist/.
#
# Inputs:
# - First argument: release version without leading v.
# - Optional --promote: run publication preflight and tag/push under the same
#   ownership scope. Without this flag the exact annotated tag must exist.
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
mode="${2:-}"
if [[ "$#" -gt 2 || ( -n "$mode" && "$mode" != --promote ) ]]; then
  echo "usage: scripts/publish_release_from_dist.sh <version-without-v> [--promote]" >&2
  exit 2
fi

tag="v$version"

# Serialize local publishers across worktrees, then exclude preparation and
# distribution restoration for the entire preflight/tag/upload interval. Other
# clones/hosts still require one coordinated publisher; this is not a remote
# GitHub compare-and-swap or distributed lease.
regular_lock() {
  [[ ! -L "$1" && ( ! -e "$1" || -f "$1" ) ]] || {
    echo "invalid publication lease path" >&2; return 1;
  }
}
git_common="$(git rev-parse --path-format=absolute --git-common-dir)"
[[ -d "$git_common" && ! -L "$git_common" ]] || exit 1
regular_lock "$git_common/terlan-publication.lock"
exec 7>>"$git_common/terlan-publication.lock"
flock --exclusive --nonblock 7 || { echo "another publisher owns this repository" >&2; exit 1; }
[[ /dev/fd/7 -ef "$git_common/terlan-publication.lock" ]] || exit 1
[[ ! -L target && ( ! -e target || -d target ) ]] || exit 1
mkdir -p target
[[ ! -L target/quality && ( ! -e target/quality || -d target/quality ) ]] || exit 1
mkdir -p target/quality
regular_lock target/quality/preparation.lock
case "${TERLAN_PREPARATION_LOCK_HELD:-}" in
  1) [[ /dev/fd/9 -ef target/quality/preparation.lock ]] || exit 1 ;;
  '') exec 9>>target/quality/preparation.lock ;;
  *) echo "invalid preparation lease scope" >&2; exit 1 ;;
esac
flock --exclusive --nonblock 9 || { echo "candidate preparation is active" >&2; exit 1; }
[[ /dev/fd/9 -ef target/quality/preparation.lock ]] || exit 1
regular_lock target/publication-inputs.lock
exec 8>>target/publication-inputs.lock
flock --exclusive --nonblock 8 || { echo "distribution restoration is active" >&2; exit 1; }
[[ /dev/fd/8 -ef target/publication-inputs.lock ]] || exit 1

# All files in this reserved namespace are regenerable publication scratch.
# Do not sweep unknown files or leave anonymous scratch behind after SIGKILL.
work_dir="$repo_root/target/quality/publication.pending"
retire_publication_scratch() {
  [[ ! -L target && ! -L target/quality ]] || return 1
  if [[ -e "$work_dir" || -L "$work_dir" ]]; then
    [[ -d "$work_dir" && ! -L "$work_dir" ]] || return 1
    (
      shopt -s nullglob dotglob
      for entry in "$work_dir"/*; do
        case "${entry##*/}" in
          publication-plan.json|notes.md|changelog.md|expected-assets.txt|actual-assets.txt|expected-asset-metadata.tsv|actual-asset-metadata.tsv|release.json|releases.json)
            [[ -f "$entry" && ! -L "$entry" ]] || exit 1 ;;
          *) exit 1 ;;
        esac
      done
    ) || { echo "unrecognized publication scratch contents" >&2; return 1; }
    rm -f -- "$work_dir/publication-plan.json" "$work_dir/notes.md" "$work_dir/changelog.md" \
      "$work_dir/expected-assets.txt" "$work_dir/actual-assets.txt" \
      "$work_dir/expected-asset-metadata.tsv" "$work_dir/actual-asset-metadata.tsv" \
      "$work_dir/release.json" "$work_dir/releases.json"
    rmdir -- "$work_dir"
  fi
}
retire_publication_scratch

if [[ "$mode" == --promote ]]; then
  # All prerequisites remain read-only verification: never prepare or refresh
  # a candidate during publication. Borrow fd 9 when a verifier needs its owner.
  export TERLAN_PREPARATION_LOCK_HELD=1
  make --no-print-directory publish-preflight VERSION="$version" </dev/null
  if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    git tag --annotate "$tag" --message "Terlan $tag"
  fi
  git push origin main
  git push origin "$tag"
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "publish requires GitHub CLI: install gh and run gh auth login" >&2
  exit 127
fi

if [[ "$mode" != --promote ]] && ! gh auth status >/dev/null 2>&1; then
  echo "publish requires authenticated GitHub CLI: run gh auth login" >&2
  exit 1
fi
repository="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
command -v jq >/dev/null 2>&1 || { echo "publish requires jq" >&2; exit 127; }

read_remote_assets() {
  # GitHub does not reliably expose drafts through the tag lookup endpoint.
  [[ "$release_id" =~ ^[0-9]+$ ]] || return 1
  gh api "repos/$repository/releases/$release_id" \
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

mkdir -m 700 -- "$work_dir"
trap retire_publication_scratch EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
publication_plan="$work_dir/publication-plan.json"
notes="$work_dir/notes.md"
changelog_section="$work_dir/changelog.md"
expected_assets="$work_dir/expected-assets.txt"
actual_assets="$work_dir/actual-assets.txt"
expected_asset_metadata="$work_dir/expected-asset-metadata.tsv"
actual_asset_metadata="$work_dir/actual-asset-metadata.tsv"
# One checked export owns verification, artifact inventory, sizes, and hashes.
# A failing producer cannot be hidden by process-substitution exit semantics.
TERLAN_RELEASE_ROOT="$repo_root" "${release_promotion[@]}" publication-plan --version "$version" > "$publication_plan"
jq -e --arg version "$version" '
  .schema == "terlan.publication-plan.v1" and .version == $version
  and (.candidate_seal | test("^sha256:[0-9a-f]{64}$"))
  and (.artifacts | type == "array" and length > 1)
  and all(.artifacts[];
    (.path | type == "string") and (.size_bytes | type == "number" and . > 0)
    and (.sha256 | test("^sha256:[0-9a-f]{64}$")))
' "$publication_plan" >/dev/null
mapfile -t artifacts < <(jq -r '.artifacts[].path' "$publication_plan")
awk -v version="$version" '
  $0 == "## " version { in_section = 1; next }
  in_section && /^## / { exit }
  in_section { print }
' CHANGELOG.md > "$changelog_section"
if ! grep -q '[^[:space:]]' "$changelog_section"; then
  echo "CHANGELOG.md is missing release notes for $version" >&2
  exit 1
fi
# Human-facing notes come only from the curated changelog. Cryptographic
# identity remains in the uploaded candidate manifest, not the announcement.
cp "$changelog_section" "$notes"

: > "$expected_assets"
for artifact in "${artifacts[@]}"; do
  case "$artifact" in
    dist/*/*|*\\*|*$'\t'*|*$'\n'*)
      echo "release candidate contains an unsafe artifact path: $artifact" >&2
      exit 1
      ;;
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
  printf '%s\n' "$artifact_name" >> "$expected_assets"
done
jq -r '.artifacts[] | [(.path | split("/") | last), (.size_bytes | tostring), .sha256] | @tsv' \
  "$publication_plan" > "$expected_asset_metadata"
LC_ALL=C sort -o "$expected_assets" "$expected_assets"
LC_ALL=C sort -o "$expected_asset_metadata" "$expected_asset_metadata"

if [[ "$(uniq "$expected_assets" | wc -l | tr -d ' ')" -ne "${#artifacts[@]}" ]]; then
  echo "release candidate contains duplicate artifact names" >&2
  exit 1
fi

# A failed lookup is not proof of absence: authentication/network errors must
# never be interpreted as permission to create another release. The list API
# also sees drafts when the tag lookup endpoint does not.
if ! gh release view "$tag" --json databaseId,isDraft,body > "$work_dir/release.json"; then
  gh api --paginate "repos/$repository/releases?per_page=100" \
    --jq '.[] | {tag_name, id, draft, body}' > "$work_dir/releases.json"
  jq -s --arg tag "$tag" '
    map(select(.tag_name == $tag)) |
    if length == 0 then null
    elif length == 1 then .[0] | {databaseId: .id, isDraft: .draft, body}
    else error("multiple releases found for the same tag") end
  ' "$work_dir/releases.json" > "$work_dir/release.json"
fi
jq -e '(. == null) or
  (type == "object" and (.databaseId | type == "number" and . > 0)
   and (.isDraft | type == "boolean"))' "$work_dir/release.json" >/dev/null
if jq -e '. != null' "$work_dir/release.json" >/dev/null; then
  release_id="$(jq -r '.databaseId' "$work_dir/release.json")"
  is_draft="$(jq -r '.isDraft' "$work_dir/release.json")"
  if [[ "$is_draft" != "true" ]]; then
    read_remote_assets > "$actual_asset_metadata"
    cut -f1 "$actual_asset_metadata" > "$actual_assets"
    # The exact manifest digest binds the candidate seal. Editorial wording is
    # not an integrity signal and must not prevent an otherwise valid retry.
    if cmp -s "$expected_assets" "$actual_assets" \
      && cmp -s "$expected_asset_metadata" "$actual_asset_metadata"; then
      echo "Release $tag is already public with the exact sealed asset set."
      exit 0
    fi
    echo "release $tag is public but does not match the sealed candidate; refusing to mutate it" >&2
    exit 1
  fi
  gh release edit "$tag" --title "Terlan $version" --notes-file "$notes"
else
  gh release create "$tag" --draft --verify-tag --title "Terlan $version" --notes-file "$notes"
  release_id="$(gh release view "$tag" --json databaseId --jq .databaseId)"
fi

read_remote_assets > "$actual_asset_metadata"
for artifact in "${artifacts[@]}"; do
  expected_row="$(awk -F '\t' -v name="$(basename "$artifact")" '$1 == name' "$expected_asset_metadata")"
  if grep -Fxq "$expected_row" "$actual_asset_metadata"; then
    echo "Reusing verified upload $artifact"
    continue
  fi
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
