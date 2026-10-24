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
  artifact/bin/terlan-registry; do
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
  (.artifact.files | length == 1) and
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

# The fixture owns HTTP routes, so exercise its deployed binary with bounded
# readiness and teardown rather than waiting indefinitely for the service.
node --input-type=module - "${bundle_a}/artifact/bin/terlan-registry" <<'NODE'
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { setTimeout as delay } from 'node:timers/promises';

const listener = createServer();
await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
const { port } = listener.address();
await new Promise(resolve => listener.close(resolve));
const child = spawn(process.argv[2], [], {
  env: { ...process.env, PORT: String(port), TERLAN_SERVE_PORT: String(port) },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let stopped = false;
let diagnostics = '';
const capture = bytes => { diagnostics = (diagnostics + bytes).slice(-65536); };
child.stdout.on('data', capture);
child.stderr.on('data', capture);
const done = new Promise(resolve => {
  child.once('exit', (code, signal) => {
    stopped = true;
    diagnostics += `\nservice exited: ${code ?? signal}`;
    resolve();
  });
  child.once('error', error => {
    stopped = true;
    diagnostics += `\n${error}`;
    resolve();
  });
});
try {
  const deadline = Date.now() + 15000;
  for (;;) {
    assert(!stopped, diagnostics);
    assert(Date.now() < deadline, `service readiness timed out: ${diagnostics}`);
    let response;
    try {
      response = await fetch(`http://127.0.0.1:${port}/health`, {
        signal: AbortSignal.timeout(1000),
      });
    } catch {
      await delay(50);
      continue;
    }
    assert.equal(response.status, 200, diagnostics);
    assert.equal(await response.text(), 'ok');
    break;
  }
} finally {
  if (!stopped) child.kill('SIGTERM');
  await Promise.race([done, delay(3000, undefined, { ref: false })]);
  if (!stopped) child.kill('SIGKILL');
  await done;
}
NODE
echo "Cloud release bundle v1 is deterministic, checksummed, portable, and executable"
