import { test } from 'node:test';
import assert from 'node:assert/strict';
import { verifyCompilations } from './compilation-results.mjs';

const compiled = { module: 'std.js.String', status: 0, signal: null };

test('browser preparation accepts successful consumer compilations', () => {
  assert.doesNotThrow(() => verifyCompilations([compiled, { ...compiled, module: 'std.js.Array' }]));
});

test('one failed consumer stops browser preparation with its compiler diagnostic', () => {
  assert.throws(() => verifyCompilations([compiled, {
    module: 'std.js.Array', status: 1, signal: null, diagnostics: 'unsupported empty-list cast',
  }]), /std\.js\.Array: status 1\nunsupported empty-list cast/);
});

test('a killed compiler cannot admit a browser run', () => {
  assert.throws(() => verifyCompilations([{ ...compiled, status: null, signal: 'SIGKILL' }]), /SIGKILL/);
});

test('browser preparation rejects missing compilation results', () => {
  assert.throws(() => verifyCompilations([]), /No browser compiler results/);
});
