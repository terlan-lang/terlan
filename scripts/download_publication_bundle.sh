#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

revision="${1:-}"
[[ "$revision" =~ ^[0-9a-f]{40}$ ]] || {
  echo "usage: $0 <full-release-commit-sha>" >&2
  exit 2
}
command -v gh >/dev/null 2>&1
command -v jq >/dev/null 2>&1

repository="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
status_json="$(gh api "repos/$repository/commits/$revision/status" \
  --jq '[.statuses[] | select(.context == "release-validation/run")] | sort_by(.created_at) | last // {}')"
target_url="$(jq -r 'select(.state == "success") | .target_url // ""' <<<"$status_json")"
[[ "$target_url" =~ /actions/runs/([1-9][0-9]*)$ ]] || {
  echo "release $revision has no successful publication-ready validation" >&2
  exit 1
}
run_id="${BASH_REMATCH[1]}"
run_json="$(gh api "repos/$repository/actions/runs/$run_id")"
jq -e --arg revision "$revision" '
  .head_sha == $revision and .conclusion == "success"
  and .path == ".github/workflows/release.yml"
' <<<"$run_json" >/dev/null

bundle="target/publication-bundle"
pending="target/publication-bundle.pending"
[[ ! -L target && ( ! -e target || -d target ) ]]
mkdir -p target
rm -rf -- "$pending"
mkdir -m 700 -- "$pending"
trap 'rm -rf -- "$pending"' EXIT
gh run download "$run_id" --name release-publication-bundle --dir "$pending"

[[ -f "$pending/publication-plan.json" && ! -L "$pending/publication-plan.json" ]]
[[ -f "$pending/bundle.sha256" && ! -L "$pending/bundle.sha256" ]]
[[ -z "$(find "$pending" ! -type d ! -type f -print -quit)" ]]
(
  cd "$pending"
  sha256sum --check --strict bundle.sha256
)
jq -e --arg revision "$revision" '
  . as $plan
  | .schema == "terlan.publication-plan.v1"
  and ($revision | startswith($plan.source_revision))
  and (.version | test("^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$"))
  and (.artifacts | type == "array" and length > 1)
  and all(.artifacts[]; .path | startswith("dist/") and (contains("..") | not))
' "$pending/publication-plan.json" >/dev/null

while IFS= read -r artifact; do
  name="${artifact#dist/}"
  [[ "$name" != "$artifact" && "$name" != */* && -f "$pending/dist/$name" && ! -L "$pending/dist/$name" ]]
done < <(jq -r '.artifacts[].path' "$pending/publication-plan.json")

while IFS= read -r file; do
  gh attestation verify "$file" \
    --repo "$repository" \
    --signer-workflow "$repository/.github/workflows/release.yml" \
    --source-digest "$revision" >/dev/null
done < <(find "$pending" -type f ! -name bundle.sha256 -print | LC_ALL=C sort)

rm -rf -- "$bundle"
mv "$pending" "$bundle"
trap - EXIT
mkdir -p dist
while IFS= read -r artifact; do
  cp -p "$bundle/$artifact" "$artifact"
done < <(jq -r '.artifacts[].path' "$bundle/publication-plan.json")
echo "restored sealed publication bundle from release run $run_id"
