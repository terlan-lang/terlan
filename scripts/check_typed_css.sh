#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
terlc="${TERLC:-${repo_root}/target/debug/terlc}"
fixture="${repo_root}/tests/fixtures/typed-css"
output="$(mktemp -d "${TMPDIR:-/tmp}/terlan-typed-css.XXXXXX")"
trap 'rm -rf "${output}"' EXIT

cd "${repo_root}"
npm run --silent check --prefix tools/css
npm run --silent test --prefix tools/css
node --test tools/web-toolchain/css-bundle.test.mjs
TERLC="${terlc}" npm run --silent check:types --prefix tools/css
"${terlc}" test std/css/ValuesTest.terl std/css/PropertiesTest.terl --target terlan-vm

for source in WrongProperty RawString RawSerializedValue InvalidEnum InvalidComponentEnum InvalidPartEnum InvalidColorEnum WrongUnit WrongShorthandGlobal NestedShorthand WrongShorthandProperty ComponentAsValue WrongBorderParts WrongBorderGlobal WrongBorderProperty WrongBorderColor; do
  if "${terlc}" check "${fixture}/${source}.terl" > "${output}/${source}.log" 2>&1; then
    echo "typed CSS accepted invalid input: ${source}" >&2
    exit 1
  fi
  rg -q 'type_mismatch' "${output}/${source}.log"
done

if "${terlc}" check "${fixture}/ForgedValue.terl" > "${output}/ForgedValue.log" 2>&1; then
  echo 'typed CSS accepted an externally constructed opaque value' >&2
  exit 1
fi
rg -q 'cannot construct opaque type .*Width.Value outside defining module' "${output}/ForgedValue.log"

"${terlc}" build "${fixture}" --target js.browser --out-dir "${output}/build"
node --input-type=module - "${output}/build" <<'NODE'
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
const build = process.argv[2];
const manifest = JSON.parse(fs.readFileSync(path.join(build, 'web/manifest.json')));
const styles = manifest.assets.filter(asset => asset.kind === 'css');
assert.equal(styles.length, 1);
const css = fs.readFileSync(path.join(build, 'web', styles[0].web_relative_path), 'utf8');
assert.match(css, /width:30px/);
assert.match(css, /height:30px/);
assert.match(css, /margin:-1px 25% auto 2rem/);
assert.match(css, /padding:1rem 5%/);
assert.match(css, /border-radius:1px 2px 3px/);
assert.match(css, /border:2px solid #7f881a/);
assert.ok(!manifest.assets.some(asset => asset.module === 'typed_css.Styles'));
assert.ok(manifest.assets.some(asset => asset.web_relative_path.endsWith('.css.map')));
console.log('Typed CSS passed generation, value, type-rejection, and browser-build checks');
NODE
