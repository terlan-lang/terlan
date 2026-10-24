import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, copyFile, writeFile, readFile, rm } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { root } from './inventory.mjs';

async function run(mode) {
  const directory = await mkdtemp(path.join(root, 'target/browser-hook-test-'));
  try {
    const bin = path.join(directory, 'bin');
    await mkdir(bin);
    await mkdir(path.join(directory, 'tests/js'), { recursive: true });
    for (const file of ['inventory.mjs', 'verify-results.mjs', 'verify-results.test.mjs']) {
      await copyFile(path.join(root, 'tests/js', file), path.join(directory, 'tests/js', file));
    }
    await writeFile(path.join(bin, 'npm'), `#!/usr/bin/env bash\nexit ${mode === 'install-failure' ? 42 : 0}\n`, { mode: 0o755 });
    await writeFile(path.join(bin, 'npx'), `#!${process.execPath}
const fs = require('node:fs');
const assert = require('node:assert/strict');
assert.equal(process.env.TERLAN_JS_MODULES, undefined);
assert.equal(process.env.CI, '1');
assert.equal(process.env.TERLC, process.env.CARGO_TARGET_DIR + '/release/terlc');
const args = process.argv.slice(2);
const names = ['chromium', 'firefox', 'webkit'];
const report = {
  config: { projects: names.map(name => ({ name })) }, errors: [],
  suites: [{ specs: [{ file: 'bindings.spec.mjs', title: 'compiled binding',
    tests: names.map(projectName => ({ projectName, expectedStatus: 'passed',
      results: [{ status: ${JSON.stringify(mode === 'skipped' ? 'skipped' : 'passed')} }] }))
  }] }]
};
if (args.includes('--list')) process.stdout.write(JSON.stringify(report));
else {
  for (const name of names) assert(args.includes('--project=' + name));
  assert(args.includes('--forbid-only'));
  assert(args.includes('--retries=0'));
  fs.writeFileSync('../../target/js-browser/results.json', JSON.stringify(report));
}
`, { mode: 0o755 });
    const result = spawnSync('bash', [path.join(root, 'scripts/git_hooks/js_browser.sh')], {
      cwd: directory, encoding: 'utf8',
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, CARGO_TARGET_DIR: path.join(directory, 'build'),
        TERLC: 'wrong-compiler', TERLAN_JS_MODULES: 'std.js.String' },
    });
    const reportExists = await readFile(path.join(directory, 'target/js-browser/results.json')).then(() => true, () => false);
    return { ...result, reportExists };
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

test('browser hook clears filters and requires every browser with the staged release compiler', async () => {
  const result = await run('complete');
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /Complete Playwright matrix passed: 3 cases/);
});

test('browser hook rejects skips even when the browser command succeeds', async () => {
  const result = await run('skipped');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Browser case did not pass/);
});

test('browser hook stops when dependency installation fails', async () => {
  const result = await run('install-failure');
  assert.equal(result.status, 42);
  assert.equal(result.reportExists, false);
});
