#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
terlc="${TERLC:-${repo_root}/target/debug/terlc}"
fixture="${repo_root}/tests/fixtures/release-bundle-v1"
mkdir -p "${repo_root}/target/tmp"
work="$(mktemp -d "${repo_root}/target/tmp/cloud-release-bundle-v1.XXXXXX")"
trap 'find "${work}" -depth -delete' EXIT
touch "${work}/.terlan-disposable-validation-workspace"
output_a="${work}/a"
output_b="${work}/b"
bundle_a="${output_a}/.terlan/release"
bundle_b="${output_b}/.terlan/release"

test -x "${terlc}" || {
  echo "missing terlc: ${terlc}" >&2
  exit 1
}

"${terlc}" build "${fixture}" --release --out-dir "${output_a}"
"${terlc}" build "${fixture}" --release --out-dir "${output_b}"

for required in \
  manifest.json \
  checksums.json \
  deploy-plan.json \
  health.json \
  runtime.json \
  routes.json \
  capabilities.json \
  sources.json \
  artifact/bin/terlan-registry \
  artifact/bin/terlan-vm \
  artifact/bin/terlan-native-worker \
  artifact/vm/terlan_registry_Main.tvm; do
  test -f "${bundle_a}/${required}"
  test -f "${bundle_b}/${required}"
done

cmp "${bundle_a}/checksums.json" "${bundle_b}/checksums.json"
for metadata in \
  manifest.json \
  deploy-plan.json \
  health.json \
  runtime.json \
  routes.json \
  capabilities.json \
  sources.json; do
  cmp "${bundle_a}/${metadata}" "${bundle_b}/${metadata}"
done

while IFS=$'\t' read -r expected relative; do
  actual="$(sha256sum "${bundle_a}/${relative}" | awk '{print $1}')"
  test "${expected}" = "${actual}" || {
    echo "release checksum mismatch for ${relative}" >&2
    exit 1
  }
done < <(jq -r '.files[] | [.sha256, .path] | @tsv' "${bundle_a}/checksums.json")

jq -e '
  .schema == "terlan-cloud-release-bundle-v1" and
  .generated_by.tool == "terlc" and
  .toolchain.compiler_version != "" and
  .toolchain.stdlib_version != "" and
  .target.artifact == "terlan-vm" and
  .target.runtime == "terlan-vm" and
  (.artifact.files | length == 4) and
  (.routes | length == 2) and
  (.sources | length == 2) and
  (.migrations | length == 1) and
  (.health_checks | length == 1) and
  .rollback.policy == "migration-compatible"
' "${bundle_a}/manifest.json" >/dev/null

if rg -q -F "${repo_root}" "${bundle_a}"/*.json; then
  echo "release metadata contains an absolute machine-local path" >&2
  exit 1
fi
if rg -q 'postgres://|"value"[[:space:]]*:' "${bundle_a}"/*.json; then
  echo "release metadata contains a secret value" >&2
  exit 1
fi

"${bundle_a}/artifact/bin/terlan-registry"
echo "Cloud release bundle v1 is deterministic, checksummed, portable, and executable"
