#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
source_file="$root/scripts/download_validated_release_artifacts.sh"
workflow_file="$root/.github/workflows/release.yml"
reports_file="$root/scripts/self_validation/release_promotion/src/release_promotion/EvidenceReports.terl"

require_in() {
  local file="$1"
  local pattern="$2"
  rg -F --quiet -- "$pattern" "$file" || {
    echo "missing release artifact contract: $pattern" >&2
    exit 1
  }
}

require() {
  local pattern="$1"
  require_in "$source_file" "$pattern"
}

require 'actions/runs/$producer_run/artifacts?per_page=100'
require 'select(.name == $name and .expired == false)'
require '[[ "$matches" == 1 ]]'
require 'actions/artifacts/$artifact_id/zip'
require 'Reuse unchanged compiler validation'
require 'Validate release candidate'
require 'candidate_validation_mode" == full'
require 'candidate_validation_mode" == incremental'
require 'terlan.incremental-compiler-validation.v1'
require 'authentication:"github-job"'
require 'authentication:"github-attestation"'
require 'hosted release artifact is missing required report $evidence'

mapfile -t readiness_reports < <(
  sed -n '/pub readiness_reports/,/pub preflight_reports/p' "$reports_file" \
    | rg -o 'target/quality/[A-Za-z0-9._-]+\.json' \
    | sed 's#target/quality/##'
)
test "${#readiness_reports[@]}" -gt 0
for report in "${readiness_reports[@]}"; do
  require "$report"
  require_in "$workflow_file" "target/quality/$report"
done

if rg -F --quiet -- 'gh run download' "$source_file"; then
  echo 'release artifacts must be downloaded through the REST API' >&2
  exit 1
fi

echo 'release artifact download contract passed'
