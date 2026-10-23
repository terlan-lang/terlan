import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import path from 'node:path';
import { output } from './inventory.mjs';

function cases(report) {
  const result = new Map();
  function visit(suite) {
    for (const spec of suite.specs ?? []) {
      for (const test of spec.tests ?? []) {
        const key = JSON.stringify([test.projectName, spec.file, spec.title]);
        assert(!result.has(key), `Duplicate browser case: ${key}`);
        result.set(key, test);
      }
    }
    for (const child of suite.suites ?? []) visit(child);
  }
  for (const suite of report.suites) visit(suite);
  return result;
}

export function verify(expected, actual) {
  assert.deepEqual(expected.config.projects.map(project => project.name).sort(), ['chromium', 'firefox', 'webkit']);
  const selected = cases(expected);
  const executed = cases(actual);
  assert(selected.size > 0, 'No Playwright cases were discovered');
  assert.deepEqual([...executed.keys()].sort(), [...selected.keys()].sort(), 'The complete browser suite must execute');
  for (const [key, test] of executed) {
    assert.equal(test.expectedStatus, 'passed', `Expected-failure annotation is not passing coverage: ${key}`);
    assert.equal(test.results.length, 1, `Browser case must execute once: ${key}`);
    assert.equal(test.results[0].status, 'passed', `Browser case did not pass: ${key}`);
  }
  assert.equal(actual.errors?.length ?? 0, 0, 'Playwright reported global errors');
  return executed.size;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const expected = JSON.parse(await readFile(path.join(output, 'expected.json')));
  const actual = JSON.parse(await readFile(path.join(output, 'results.json')));
  console.log(`Complete Playwright matrix passed: ${verify(expected, actual)} cases`);
}
