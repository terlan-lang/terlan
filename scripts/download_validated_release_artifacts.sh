#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

revision="${1:-$(git rev-parse HEAD)}"
if [[ ! "$revision" =~ ^[0-9a-f]{40}$ ]]; then
  echo "validated artifact revision must be a full Git commit SHA: $revision" >&2
  exit 2
fi
publication_inputs="target/publication-inputs/$revision"
if [[ "${2:-}" == "--restore" ]]; then
  [[ "$(git rev-parse HEAD)" == "$revision" ]] || {
    echo "publication inputs must belong to the current commit" >&2
    exit 1
  }
  [[ -d "$publication_inputs" && ! -L "$publication_inputs" ]] || {
    echo "verified publication inputs are missing for $revision" >&2
    exit 1
  }
  (
    cd "$publication_inputs"
    sha256sum --check --quiet verified-inputs.sha256
  )
  for payload in "$publication_inputs"/terlc-* \
    "$publication_inputs"/terlc "$publication_inputs"/terlan-vm \
    "$publication_inputs"/terlan-native-worker "$publication_inputs"/terlan-lsp \
    "$publication_inputs"/terlan-release.json "$publication_inputs"/SHA256SUMS \
    "$publication_inputs"/terlan-install-manifest.json; do
    [[ -f "$payload" && ! -L "$payload" ]] || exit 1
    cp -p "$payload" "dist/$(basename "$payload")"
  done
  # Local target smokes leave these build inputs beside the public payload.
  rm -f dist/release-self-test.tvm
  rm -rf dist/release-self-test-source dist/release-self-test-build
  echo "restored verified hosted publication inputs for $revision"
  exit 0
fi
if [[ -n "${2:-}" ]]; then
  echo "usage: $0 <revision> [--restore]" >&2
  exit 2
fi
command -v gh >/dev/null 2>&1 || {
  echo "validated artifact download requires GitHub CLI" >&2
  exit 127
}
gh auth status >/dev/null 2>&1 || {
  echo "validated artifact download requires an authenticated GitHub CLI" >&2
  exit 1
}
command -v jq >/dev/null 2>&1 || {
  echo "validated artifact download requires jq" >&2
  exit 127
}
repository="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
status_json="$(gh api "repos/{owner}/{repo}/commits/$revision/status" \
  --jq '[.statuses[] | select(.context == "release-validation/run")] | sort_by(.created_at) | last // {}')"
state="$(jq -r '.state // "missing"' <<<"$status_json")"
target_url="$(jq -r '.target_url // ""' <<<"$status_json")"
run_id=""
if [[ "$state" == "success" && "$target_url" =~ /actions/runs/([0-9]+)$ ]]; then
  run_id="${BASH_REMATCH[1]}"
fi

# A later duplicate run can be cancelled after this exact revision has already
# passed. GitHub's mutable combined-status endpoint then points at the cancelled
# run. Fall back to the immutable successful `validate release` check run; the
# workflow path, revision, conclusion, artifacts, and attestations are still
# verified below.
if [[ -z "$run_id" ]]; then
  release_check_json="$(gh api \
    -H 'Accept: application/vnd.github+json' \
    "repos/$repository/commits/$revision/check-runs?filter=all&per_page=100" \
    --jq '[.check_runs[] | select(.name == "validate release" and .conclusion == "success" and .app.slug == "github-actions")] | sort_by(.completed_at) | last // {}')"
  release_details_url="$(jq -r '.details_url // ""' <<<"$release_check_json")"
  if [[ "$release_details_url" =~ /actions/runs/([0-9]+)/job/ ]]; then
    run_id="${BASH_REMATCH[1]}"
  fi
fi
if [[ -z "$run_id" ]]; then
  echo "revision $revision has no successful release-validation/run artifact producer" >&2
  exit 1
fi

download_dir="$(mktemp -d)"
extract_dir="$(mktemp -d)"
hosted_evidence_dir="$(mktemp -d)"
trap 'rm -rf "$download_dir" "$extract_dir" "$hosted_evidence_dir"' EXIT

run_json="$(gh api "repos/{owner}/{repo}/actions/runs/$run_id")"
run_revision="$(jq -r '.head_sha // ""' <<<"$run_json")"
run_conclusion="$(jq -r '.conclusion // ""' <<<"$run_json")"
run_path="$(jq -r '.path // ""' <<<"$run_json")"
if [[ "$run_revision" != "$revision" \
  || "$run_conclusion" != "success" \
  || "$run_path" != ".github/workflows/release.yml" ]]; then
  echo "release-validation status does not identify a successful release workflow for $revision" >&2
  exit 1
fi

# Publication consumes the exhaustive candidate validation instead of
# replaying it locally. Require the exact GitHub Actions check run and then
# resolve its workflow run so a similarly named third-party check cannot
# satisfy the contract.
candidate_check_json="$(gh api \
  -H 'Accept: application/vnd.github+json' \
  "repos/$repository/commits/$revision/check-runs?filter=latest&per_page=100" \
  --jq '[.check_runs[] | select(.name == "Compiler check and test" and .conclusion == "success" and .app.slug == "github-actions")] | first // {}')"
candidate_details_url="$(jq -r '.details_url // ""' <<<"$candidate_check_json")"
if [[ ! "$candidate_details_url" =~ /actions/runs/([0-9]+)/job/ ]]; then
  echo "revision $revision has no successful canonical Compiler check and test" >&2
  exit 1
fi
candidate_run_id="${BASH_REMATCH[1]}"
candidate_run_json="$(gh api "repos/$repository/actions/runs/$candidate_run_id")"
candidate_revision="$(jq -r '.head_sha // ""' <<<"$candidate_run_json")"
candidate_conclusion="$(jq -r '.conclusion // ""' <<<"$candidate_run_json")"
candidate_path="$(jq -r '.path // ""' <<<"$candidate_run_json")"
if [[ "$candidate_revision" != "$revision" \
  || "$candidate_conclusion" != "success" \
  || "$candidate_path" != ".github/workflows/ci.yml" ]]; then
  echo "Compiler check does not identify a successful canonical workflow for $revision" >&2
  exit 1
fi
jq -n \
  --arg revision "$revision" \
  --argjson run_id "$candidate_run_id" \
  '{schema:"terlan.hosted-candidate-validation.v1",decision:"pass",source_revision:$revision,workflow:".github/workflows/ci.yml",run_id:$run_id}' \
  >"$hosted_evidence_dir/hosted-candidate-validation.json"
mkdir -p target/quality
install -m 0644 \
  "$hosted_evidence_dir/hosted-candidate-validation.json" \
  target/quality/hosted-candidate-validation.json
gh run download "$run_id" --name release-distribution --dir "$download_dir"
gh run download "$run_id" --name release-hosted-validation-evidence --dir "$hosted_evidence_dir"
hosted_evidence_files=(
  tvm-aot-platform-matrix-report.json
  tvm-aot-thread-sanitizer-report.json
  vm-multicore-thread-sanitizer-report.json
  vm-multicore-memory-model-tsan.json
)
for evidence in "${hosted_evidence_files[@]}"; do
  [[ -f "$hosted_evidence_dir/$evidence" && ! -L "$hosted_evidence_dir/$evidence" ]] || {
    echo "hosted release evidence is missing $evidence" >&2
    exit 1
  }
  install -m 0644 "$hosted_evidence_dir/$evidence" "target/quality/$evidence"
done
make --no-print-directory release-artifact-set-check \
  RELEASE_ARTIFACT_SET_ROOT="$download_dir"
for artifact in "$download_dir"/terlc-*; do
  gh attestation verify "$artifact" \
    --repo "$repository" \
    --signer-workflow "$repository/.github/workflows/release.yml" \
    >/dev/null
done

if tar -tzf "$download_dir/terlc-linux-x86_64.tar.gz" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
  echo "validated Linux release archive contains an unsafe path" >&2
  exit 1
fi
if tar -tvzf "$download_dir/terlc-linux-x86_64.tar.gz" \
  | awk 'substr($1, 1, 1) == "l" || substr($1, 1, 1) == "h" { found = 1 } END { exit !found }'; then
  echo "validated Linux release archive contains a symbolic or hard link" >&2
  exit 1
fi
tar -xzf "$download_dir/terlc-linux-x86_64.tar.gz" -C "$extract_dir"
for required in terlc terlan-vm terlan-native-worker terlan-lsp terlan-release.json SHA256SUMS terlan-install-manifest.json; do
  [[ -f "$extract_dir/$required" && ! -L "$extract_dir/$required" ]] || {
    echo "validated Linux release archive is missing $required" >&2
    exit 1
  }
done
(
  cd "$extract_dir"
  checksum_paths="$(mktemp)"
  trap 'rm -f "$checksum_paths"' EXIT
  while IFS= read -r row || [[ -n "$row" ]]; do
    if [[ ! "$row" =~ ^[0-9a-fA-F]{64}\ \ (.+)$ ]]; then
      echo "validated Linux release archive contains a malformed SHA256SUMS row" >&2
      exit 1
    fi
    relative="${BASH_REMATCH[1]}"
    case "$relative" in
      ''|/*|../*|*/../*|*/..|..|*\\*)
        echo "validated Linux release archive contains an unsafe checksum path: $relative" >&2
        exit 1
        ;;
    esac
    [[ -f "$relative" && ! -L "$relative" ]] || {
      echo "validated Linux release archive checksum references an unsafe file: $relative" >&2
      exit 1
    }
    printf '%s\n' "$relative" >> "$checksum_paths"
  done < SHA256SUMS
  [[ -s "$checksum_paths" ]] || {
    echo "validated Linux release archive contains an empty SHA256SUMS" >&2
    exit 1
  }
  if [[ -n "$(LC_ALL=C sort "$checksum_paths" | uniq -d)" ]]; then
    echo "validated Linux release archive contains duplicate checksum paths" >&2
    exit 1
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c SHA256SUMS
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c SHA256SUMS
  else
    echo "validated artifact download requires sha256sum or shasum" >&2
    exit 127
  fi
)

mkdir -p dist
# The target-native packaging smoke creates these internal inputs in `dist/`.
# They are not publication payload and must not poison a retry after a later
# preflight failure.
rm -f dist/release-self-test.tvm
rm -rf dist/release-self-test-source dist/release-self-test-build
for artifact in "$download_dir"/terlc-*; do
  install -m 0644 "$artifact" "dist/$(basename "$artifact")"
done
for metadata in terlan-release.json SHA256SUMS terlan-install-manifest.json; do
  install -m 0644 "$extract_dir/$metadata" "dist/$metadata"
done
for executable in terlc terlan-vm terlan-native-worker terlan-lsp; do
  install -m 0755 "$extract_dir/$executable" "dist/$executable"
done
make --no-print-directory release-artifact-set-check \
  RELEASE_ARTIFACT_SET_ROOT=dist \
  RELEASE_ARTIFACT_SET_LOCAL_PAYLOAD=1

# Evidence refresh exercises the local packager, which writes to dist/ too.
# Retain the verified hosted bytes so final sealing cannot publish that local
# rebuild in place of the attested distribution, or require another download.
[[ ! -L "$publication_inputs" ]] || exit 1
mkdir -p "$publication_inputs"
for payload in dist/terlc-* dist/terlc dist/terlan-vm \
  dist/terlan-native-worker dist/terlan-lsp dist/terlan-release.json \
  dist/SHA256SUMS dist/terlan-install-manifest.json; do
  cp -p "$payload" "$publication_inputs/$(basename "$payload")"
done
(
  cd "$publication_inputs"
  sha256sum terlc-* terlc terlan-vm terlan-native-worker terlan-lsp \
    terlan-release.json SHA256SUMS terlan-install-manifest.json \
    >verified-inputs.sha256
)

echo "downloaded validated release artifacts from run $run_id for $revision"
echo "verified exhaustive candidate validation from run $candidate_run_id"
