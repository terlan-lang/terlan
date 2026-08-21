#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

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

revision="${1:-$(git rev-parse HEAD)}"
if [[ ! "$revision" =~ ^[0-9a-f]{40}$ ]]; then
  echo "validated artifact revision must be a full Git commit SHA: $revision" >&2
  exit 2
fi
repository="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
status_json="$(gh api "repos/{owner}/{repo}/commits/$revision/status" \
  --jq '[.statuses[] | select(.context == "release-validation/run")] | sort_by(.created_at) | last // {}')"
state="$(jq -r '.state // "missing"' <<<"$status_json")"
target_url="$(jq -r '.target_url // ""' <<<"$status_json")"
if [[ "$state" != "success" || ! "$target_url" =~ /actions/runs/([0-9]+)$ ]]; then
  echo "revision $revision has no successful release-validation/run artifact producer" >&2
  exit 1
fi
run_id="${BASH_REMATCH[1]}"

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

download_dir="$(mktemp -d)"
extract_dir="$(mktemp -d)"
trap 'rm -rf "$download_dir" "$extract_dir"' EXIT
gh run download "$run_id" --name release-distribution --dir "$download_dir"
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

echo "downloaded validated release artifacts from run $run_id for $revision"
