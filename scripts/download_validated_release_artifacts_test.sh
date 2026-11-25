#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
source_file="$root/scripts/download_validated_release_artifacts.sh"

require() {
  local pattern="$1"
  rg -F --quiet -- "$pattern" "$source_file" || {
    echo "missing release artifact contract: $pattern" >&2
    exit 1
  }
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

if rg -F --quiet -- 'gh run download' "$source_file"; then
  echo 'release artifacts must be downloaded through the REST API' >&2
  exit 1
fi

echo 'release artifact download contract passed'
