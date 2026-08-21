#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
terlc="${TERLC:-${repo_root}/target/debug/terlc}"
fixture="${repo_root}/tests/fixtures/deploy-plan-v2"
expected_json="${repo_root}/tests/fixtures/deploy-plan-v2.expected.json"
expected_text="${repo_root}/tests/fixtures/deploy-plan-v2.expected.txt"
mkdir -p "${repo_root}/target/tmp"
output_root="$(mktemp -d "${repo_root}/target/tmp/cloud-deploy-plan-v2.XXXXXX")"
trap 'find "${output_root}" -depth -delete' EXIT
touch "${output_root}/.terlan-disposable-validation-workspace"
actual_json="${output_root}/cloud/deploy-plan.json"
actual_text="${output_root}/cloud/deploy-plan.txt"

test -x "${terlc}" || {
  echo "missing terlc: ${terlc}" >&2
  exit 1
}

"${terlc}" --experimental deploy plan "${fixture}" --out-dir "${output_root}"
cmp "${expected_json}" "${actual_json}"
cmp "${expected_text}" "${actual_text}"

first_json="$(sha256sum "${actual_json}" | awk '{print $1}')"
first_text="$(sha256sum "${actual_text}" | awk '{print $1}')"
"${terlc}" --experimental deploy plan "${fixture}" --out-dir "${output_root}"
test "${first_json}" = "$(sha256sum "${actual_json}" | awk '{print $1}')"
test "${first_text}" = "$(sha256sum "${actual_text}" | awk '{print $1}')"

for required in \
  '"release"' \
  '"target"' \
  '"services"' \
  '"process"' \
  '"routes"' \
  '"handler"' \
  '"web_assets"' \
  '"health_check"' \
  '"native_packages"' \
  '"environment"' \
  '"secrets"' \
  '"migrations"' \
  '"resources"' \
  '"outbound_network"' \
  '"sources"' \
  '"rollback"'; do
  rg -q "${required}" "${actual_json}"
done

if rg -q 'postgres://|"value"[[:space:]]*:' "${actual_json}"; then
  echo "semantic deploy plan leaked a secret value" >&2
  exit 1
fi
if rg -q '"path"[[:space:]]*:[[:space:]]*"/' "${actual_json}" \
  | rg -v '"path"[[:space:]]*:[[:space:]]*"/(health|packages)' >/dev/null; then
  echo "semantic deploy plan contains an absolute machine-local path" >&2
  exit 1
fi

echo "semantic deploy plan v2 is deterministic and complete"
