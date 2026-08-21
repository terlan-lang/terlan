#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
site_root="$(cd "${script_dir}/.." && pwd)"
vendor_root="${site_root}/vendor/angular.css"

(
  cd "${vendor_root}"
  sha256sum --check angular.css.sha256
)

repo_root="$(cd "${site_root}/../.." && pwd)"
default_angular_css_root="$(cd "${repo_root}/../.." && pwd)/ng/angular.css"
angular_css_root="${TERLAN_ANGULAR_CSS_ROOT:-${default_angular_css_root}}"

if [[ -f "${angular_css_root}/dist/angular.css" ]]; then
  source_hash="$(sha256sum "${angular_css_root}/dist/angular.css" | cut -d ' ' -f 1)"
  vendor_hash="$(cut -d ' ' -f 1 "${vendor_root}/angular.css.sha256")"
  if [[ "${source_hash}" != "${vendor_hash}" ]]; then
    echo "angular.css local build differs from the vendored snapshot" >&2
    echo "  local:  ${source_hash}" >&2
    echo "  vendor: ${vendor_hash}" >&2
    exit 1
  fi
fi
