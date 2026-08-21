#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate_root="$(cd "${script_dir}/.." && pwd)"
repo_root="$(cd "${crate_root}/../.." && pwd)"
terlc="${TERLC:-${repo_root}/target/debug/terlc}"

if [[ ! -x "${terlc}" ]]; then
  echo "TERLC is not executable: ${terlc}" >&2
  exit 1
fi

mkdir -p "${repo_root}/target/tmp"
check_root="$(mktemp -d "${repo_root}/target/tmp/terl-docs-search-policy.XXXXXX")"
trap 'find "${check_root}" -depth -delete' EXIT
touch "${check_root}/.terlan-disposable-validation-workspace"

"${terlc}" build \
  "${crate_root}/terlan/src/terl_docs/Search.terl" \
  --target js.browser \
  --out-dir "${check_root}"

generated="${check_root}/js/modules/terl_docs/Search.js"
checked="${crate_root}/src/search-policy.js"
if ! cmp --silent "${generated}" "${checked}"; then
  echo "generated Terlan search policy differs from ${checked}" >&2
  diff --unified "${checked}" "${generated}" >&2 || true
  exit 1
fi

echo "terl-docs Terlan search policy: OK"
