#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
terlc="${TERLC:-${repo_root}/target/debug/terlc}"
toolchain_root="${repo_root}/tools/web-toolchain"
fixture="${repo_root}/tests/fixtures/managed-web-toolchain"
mkdir -p "${repo_root}/target/tmp"
output_root="$(mktemp -d "${repo_root}/target/tmp/managed-web-toolchain.XXXXXX")"
trap 'find "${output_root}" -depth -delete' EXIT
touch "${output_root}/.terlan-disposable-validation-workspace"
bindings_a="${output_root}/bindings-a"
bindings_b="${output_root}/bindings-b"
build_a="${output_root}/build-a"
build_b="${output_root}/build-b"

test -x "${terlc}" || {
  echo "missing terlc: ${terlc}" >&2
  exit 1
}

if test "${TERLAN_WEB_TOOLCHAIN_OFFLINE:-0}" = "1"; then
  test -d "${toolchain_root}/node_modules" || {
    echo "offline managed-web gate requires the compiler-owned toolchain to be provisioned" >&2
    exit 1
  }
else
  npm ci --prefix "${toolchain_root}" --ignore-scripts --no-audit --no-fund
fi

node - "${toolchain_root}" <<'NODE'
const fs = require('node:fs');
const path = require('node:path');
const root = process.argv[2];
const expected = {
  '@angular-wave/angular.ts': '0.32.0',
  '@rsbuild/core': '2.1.13',
  '@rspack/core': '2.1.10',
};
const declared = JSON.parse(fs.readFileSync(path.join(root, 'package.json'))).dependencies;
for (const [name, version] of Object.entries(expected)) {
  if (declared[name] !== version) throw new Error(`${name} declaration drifted`);
  const installed = JSON.parse(fs.readFileSync(path.join(root, 'node_modules', name, 'package.json'))).version;
  if (installed !== version) throw new Error(`${name} installation drifted: ${installed}`);
}
NODE

mkdir -p "${bindings_a}" "${bindings_b}"

TERLAN_WEB_TOOLCHAIN_ROOT="${toolchain_root}" \
  "${terlc}" bind angular-ts --out "${bindings_a}"
TERLAN_WEB_TOOLCHAIN_ROOT="${toolchain_root}" \
  "${terlc}" bind angular-ts --out "${bindings_b}"
diff -qr "${bindings_a}" "${bindings_b}" >/dev/null

facade="${bindings_a}/terlan/angular/Ng.terl"
test -f "${facade}"
rg -q -F '@source-package @angular-wave/angular.ts@0.32.0' "${facade}"
rg -q -F 'pub ng_module' "${facade}"

if find "${fixture}" -type f \( \
  -name 'package.json' -o \
  -name 'package-lock.json' -o \
  -name 'rspack.config.*' -o \
  -name 'rsbuild.config.*' -o \
  -name '*.js' -o \
  -name '*.ts' \
\) | grep -q .; then
  echo "managed web fixture owns JavaScript/TypeScript or bundler configuration" >&2
  exit 1
fi

TERLAN_WEB_TOOLCHAIN_ROOT="${toolchain_root}" \
  "${terlc}" build "${fixture}" --target js.browser --out-dir "${build_a}"
TERLAN_WEB_TOOLCHAIN_ROOT="${toolchain_root}" \
  "${terlc}" build "${fixture}" --target js.browser --out-dir "${build_b}"
diff -qr "${build_a}/web" "${build_b}/web" >/dev/null

bootstrap="${build_a}/terlan.angular.bootstrap.generated.js"
config="${build_a}/rsbuild.terlan.generated.mjs"
manifest="${build_a}/web/manifest.json"
test -f "${bootstrap}"
test -f "${config}"
rg -q -F "import { angular } from '@angular-wave/angular.ts';" "${bootstrap}"
rg -q -F 'process.env.TERLAN_WEB_TOOLCHAIN_ROOT' "${config}"
if rg -q -F "${toolchain_root}/node_modules" "${config}"; then
  echo "generated Rsbuild config embeds the compiler installation path" >&2
  exit 1
fi
jq -e '
  .schema == "terlan-web-build-v1" and
  .target_profile == "js.browser" and
  ([.assets[] | select(.kind == "javascript-module")] | length) == 1
' "${manifest}" >/dev/null

runtime_chunk="$(jq -r '.assets[] | select(.kind == "javascript-module") | .web_relative_path' "${manifest}" | head -n 1)"
test -n "${runtime_chunk}"
test "$(wc -c < "${build_a}/web/${runtime_chunk}")" -gt 100000

echo "Terlan generated deterministic Angular.ts bindings and bundled the compiler-managed Rspack toolchain"
