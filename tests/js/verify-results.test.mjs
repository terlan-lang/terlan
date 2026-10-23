import { test } from 'node:test';
import assert from 'node:assert/strict';
import { verify } from './verify-results.mjs';

function report() {
  const names = ['chromium', 'firefox', 'webkit'];
  return {
    config: { projects: names.map(name => ({ name })) },
    errors: [],
    suites: [{ specs: [{ file: 'bindings.spec.mjs', title: 'binding', tests: names.map(projectName => ({
      projectName, expectedStatus: 'passed', results: [{ status: 'passed' }],
    })) }] }],
  };
}

test('accepts a complete matrix', () => assert.equal(verify(report(), report()), 3));
test('rejects a missing browser case', () => {
  const actual = report();
  actual.suites[0].specs[0].tests.pop();
  assert.throws(() => verify(report(), actual), /complete browser suite/);
});
test('rejects a global error after completed cases', () => {
  const actual = report();
  actual.errors.push({ message: 'teardown failed' });
  assert.throws(() => verify(report(), actual), /global errors/);
});
test('rejects an empty discovered suite', () => {
  const expected = report();
  expected.suites = [];
  assert.throws(() => verify(expected, expected), /No Playwright cases/);
});
for (const status of ['failed', 'skipped', 'timedOut', 'interrupted']) {
  test(`rejects ${status} cases`, () => {
    const actual = report();
    actual.suites[0].specs[0].tests[0].results[0].status = status;
    assert.throws(() => verify(report(), actual), /did not pass/);
  });
}
test('rejects expected-failure annotations', () => {
  const actual = report();
  actual.suites[0].specs[0].tests[0].expectedStatus = 'failed';
  assert.throws(() => verify(report(), actual), /Expected-failure annotation/);
});
test('rejects retried cases', () => {
  const actual = report();
  actual.suites[0].specs[0].tests[0].results.push({ status: 'passed' });
  assert.throws(() => verify(report(), actual), /execute once/);
});
