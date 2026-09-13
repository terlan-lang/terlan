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
# One download/restore owner per worktree; concurrent writers share dist/.
# Keep fd 9 intact: the enclosing preparation owner lends it to nested Make.
mkdir -p target
exec 8>target/publication-inputs.lock
flock -n 8 || { echo "another publication input owner is running" >&2; exit 1; }

retire_previous_candidate() {
  if [[ -e dist/release-candidate.json || -L dist/release-candidate.json ]]; then
    [[ -f dist/release-candidate.json && ! -L dist/release-candidate.json ]] || {
      echo "previous candidate manifest must be a regular, non-symlink file" >&2
      return 1
    }
    mkdir -p target/publication-inputs
    retired_candidate="$(mktemp -d target/publication-inputs/retired-candidate.XXXXXX)"
    mv dist/release-candidate.json "$retired_candidate/"
    echo "preserved previous candidate in $retired_candidate"
  fi
}

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
  retire_previous_candidate
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
cache_stage=""
trap 'rm -rf "$download_dir" "$extract_dir" "$hosted_evidence_dir"; if [[ -n "$cache_stage" ]]; then rm -rf "$cache_stage"; fi' EXIT

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
if [[ ! "$candidate_details_url" =~ ^https://github.com/([^/]+/[^/]+)/actions/runs/([0-9]+)/job/([0-9]+)$ \
  || "${BASH_REMATCH[1]:-}" != "$repository" ]]; then
  echo "revision $revision has no successful canonical Compiler check and test" >&2
  exit 1
fi
candidate_run_id="${BASH_REMATCH[2]}"
candidate_job_id="${BASH_REMATCH[3]}"
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
# A signing-only rerun increments the workflow attempt, not the successful
# compiler job's attempt. Bind the producer job itself rather than replay tests.
candidate_job_json="$(gh api "repos/$repository/actions/jobs/$candidate_job_id")"
jq -e --arg revision "$revision" --argjson run "$candidate_run_id" \
  --argjson job "$candidate_job_id" --argjson workflow "$candidate_run_json" \
  '.id == $job and .run_id == $run and .head_sha == $revision
    and .name == "Compiler check and test" and .status == "completed" and .conclusion == "success"
    and (.run_attempt | type == "number" and . > 0 and floor == .)
    and .run_attempt <= $workflow.run_attempt' <<<"$candidate_job_json" >/dev/null || {
  echo "compiler coverage does not identify the successful producing job" >&2
  exit 1
}
candidate_attempt="$(jq -r .run_attempt <<<"$candidate_job_json")"
jq -n \
  --arg revision "$revision" \
  --argjson run_id "$candidate_run_id" \
  '{schema:"terlan.hosted-candidate-validation.v1",decision:"pass",source_revision:$revision,workflow:".github/workflows/ci.yml",run_id:$run_id}' \
  >"$hosted_evidence_dir/hosted-candidate-validation.json"
# Cache identity includes both successful producers, their attempts, and the
# verification implementation. Live workflow checks above are never cached.
cache_context="$(jq -cnS \
  --arg repository "$repository" --arg revision "$revision" \
  --arg implementation "$(sha256sum "${BASH_SOURCE[0]}" | awk '{print $1}')" \
  --argjson release "$run_json" --argjson compiler "$candidate_run_json" \
  --argjson compiler_job "$candidate_job_json" \
  '{schema:"terlan.hosted-download-cache.v1",repository:$repository,revision:$revision,implementation:$implementation,
    release:{id:$release.id,attempt:$release.run_attempt,path:$release.path},
    compiler:{id:$compiler.id,attempt:$compiler_job.run_attempt,job_id:$compiler_job.id,path:$compiler.path}}')"
jq -e 'all(.release, .compiler; (.id | type == "number" and . > 0) and (.attempt | type == "number" and . > 0))' \
  <<<"$cache_context" >/dev/null
cache_key="$(printf '%s\n' "$cache_context" | sha256sum | awk '{print $1}')"
download_cache="target/publication-downloads/$cache_key"
coverage_dir="$hosted_evidence_dir/compiler-coverage"
hosted_reports_dir="$hosted_evidence_dir/release-reports"
mkdir "$coverage_dir" "$hosted_reports_dir"
printf '%s\n' "$cache_context" > "$hosted_evidence_dir/context.json"
cache_hit=0
artifact_set_bootstrapped="${TERLAN_VALIDATION_BOOTSTRAPPED:-0}"
if [[ -e "$download_cache" || -L "$download_cache" ]]; then
  [[ -d "$download_cache" && ! -L "$download_cache" ]] || exit 1
  # Recompute the complete inventory, rejecting symlinks and extra files as
  # well as changed bytes. A corrupt cache is an error, not a silent download.
  [[ -z "$(find "$download_cache" ! -type d ! -type f -print -quit)" ]] || exit 1
  [[ "$(cat "$download_cache/context.json")" == "$cache_context" ]] || exit 1
  (
    cd "$download_cache"
    find . -type f ! -path './verified-files.sha256' -print0 \
      | LC_ALL=C sort -z | xargs -0 sha256sum
  ) > "$hosted_evidence_dir/cache-files.sha256"
  if ! cmp -s "$hosted_evidence_dir/cache-files.sha256" "$download_cache/verified-files.sha256"; then
    echo "verified hosted download cache is corrupt: $download_cache; quarantine it before retrying" >&2
    exit 1
  fi
  cp -p "$download_cache/archives/"* "$download_dir/"
  cp -a "$download_cache/extracted/." "$extract_dir/"
  cp -p "$download_cache/evidence/"* "$hosted_evidence_dir/"
  cp -p "$download_cache/coverage/"* "$coverage_dir/"
  cache_hit=1
  echo "reusing verified hosted downloads from run $run_id attempt $(jq -r .release.attempt <<<"$cache_context")"
else
  gh run download "$run_id" --name release-distribution --dir "$download_dir"
  gh run download "$run_id" --name release-hosted-validation-evidence --dir "$hosted_reports_dir"
  gh run download "$candidate_run_id" --name "compiler-test-coverage-$candidate_run_id-$candidate_attempt" --dir "$coverage_dir"
fi
hosted_evidence_files=(
  tvm-aot-platform-matrix-report.json
  tvm-aot-thread-sanitizer-report.json
  vm-multicore-thread-sanitizer-report.json
  vm-multicore-memory-model-tsan.json
)
for evidence in "${hosted_evidence_files[@]}"; do
  if [[ "$cache_hit" == 0 ]]; then
    [[ -f "$hosted_reports_dir/$evidence" && ! -L "$hosted_reports_dir/$evidence" ]] || exit 1
    cp -p "$hosted_reports_dir/$evidence" "$hosted_evidence_dir/$evidence"
  fi
  [[ -f "$hosted_evidence_dir/$evidence" && ! -L "$hosted_evidence_dir/$evidence" ]] || {
    echo "hosted release evidence is missing $evidence" >&2
    exit 1
  }
done
coverage_files=(
  rust-test-suite-report.json
  rust-test-suite-report.json.selections.json
  rust-test-suite-report.json.make-coverage.json
)
if [[ "$cache_hit" == 0 ]]; then
make --no-print-directory release-artifact-set-check \
  RELEASE_ARTIFACT_SET_ROOT="$download_dir"
artifact_set_bootstrapped=1
for artifact in "$download_dir"/terlc-*; do
  gh attestation verify "$artifact" \
    --repo "$repository" \
    --signer-workflow "$repository/.github/workflows/release.yml" \
    >/dev/null
done
for evidence in "${coverage_files[@]}"; do
  [[ -f "$coverage_dir/$evidence" && ! -L "$coverage_dir/$evidence" ]] || exit 1
  gh attestation verify "$coverage_dir/$evidence" \
    --repo "$repository" --signer-workflow "$repository/.github/workflows/ci.yml" \
    --source-ref refs/heads/main --source-digest "$revision" >/dev/null
done
fi

# Integrity is checked by the shared prebuilt reader on both cold and warm
# preparation. It is not itself authentication or local-current test coverage.
coverage_reader="target/validation-tools/terlan-test-orchestrator"
[[ -x "$coverage_reader" && ! -L "$coverage_reader" ]] || {
  echo "hosted coverage requires the bootstrapped Rust validation driver" >&2
  exit 1
}
"$coverage_reader" --check-hosted-coverage-records "$coverage_dir" \
  "$hosted_evidence_dir/context.json" > "$hosted_evidence_dir/coverage-records.json"
jq --arg cache_key "$cache_key" --argjson job "$candidate_job_json" \
  --slurpfile coverage "$hosted_evidence_dir/coverage-records.json" \
  '. + {coverage:{cache_key:$cache_key,producer_job_id:$job.id,producer_attempt:$job.run_attempt,
    authentication:"github-attestation",records:$coverage[0]}}' \
  "$hosted_evidence_dir/hosted-candidate-validation.json" > "$hosted_evidence_dir/candidate.pending"
mv -f "$hosted_evidence_dir/candidate.pending" "$hosted_evidence_dir/hosted-candidate-validation.json"

if [[ "$cache_hit" == 0 ]]; then

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

# Commit a complete verified checkpoint before any local distribution checks.
# An interrupted producer leaves no cache entry that another run could reuse.
mkdir -p target/publication-downloads
cache_stage="$(mktemp -d target/publication-downloads/.partial.XXXXXX)"
mkdir "$cache_stage/archives" "$cache_stage/extracted" "$cache_stage/evidence" "$cache_stage/coverage"
cp -p "$coverage_dir/"* "$cache_stage/coverage/"
cp -p "$download_dir/"* "$cache_stage/archives/"
cp -a "$extract_dir/." "$cache_stage/extracted/"
for evidence in "${hosted_evidence_files[@]}" hosted-candidate-validation.json; do
  cp -p "$hosted_evidence_dir/$evidence" "$cache_stage/evidence/"
done
printf '%s\n' "$cache_context" > "$cache_stage/context.json"
(
  cd "$cache_stage"
  find . -type f ! -path './verified-files.sha256' -print0 \
    | LC_ALL=C sort -z | xargs -0 sha256sum > verified-files.sha256
)
mv -T "$cache_stage" "$download_cache"
cache_stage=""
fi

mkdir -p target/quality
for evidence in "${hosted_evidence_files[@]}" hosted-candidate-validation.json; do
  install -m 0644 "$hosted_evidence_dir/$evidence" "target/quality/$evidence"
done

mkdir -p dist
retire_previous_candidate
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
  RELEASE_ARTIFACT_SET_LOCAL_PAYLOAD=1 \
  TERLAN_VALIDATION_BOOTSTRAPPED="$artifact_set_bootstrapped"

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
# Pin the exact verified cache generation; the candidate retains independent
# payload copies. Include this binding in the immutable input inventory.
[[ ! -L "$publication_inputs/download-cache-key" \
  && ! -L "$publication_inputs/download-cache-key.pending" ]] || exit 1
printf '%s\n' "$cache_key" > "$publication_inputs/download-cache-key.pending"
mv -f -T "$publication_inputs/download-cache-key.pending" "$publication_inputs/download-cache-key"
(
  cd "$publication_inputs"
  sha256sum terlc-* terlc terlan-vm terlan-native-worker terlan-lsp \
    terlan-release.json SHA256SUMS terlan-install-manifest.json download-cache-key \
    >verified-inputs.sha256
)

echo "downloaded validated release artifacts from run $run_id for $revision"
echo "verified exhaustive candidate validation from run $candidate_run_id"
