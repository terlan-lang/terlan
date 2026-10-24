import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { bundleStyles, rebaseUrl } from './css-bundle.mjs';

test('bundles conditional imports in cascade order, rebases media, and writes maps', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'terlan-css-'));
  try {
    await mkdir(path.join(root, 'nested'));
    await writeFile(path.join(root, 'nested/base.css'), '.tile { color: red; background: url(./tile.png?v=1#image) }');
    const entry = path.join(root, 'entry.css');
    await writeFile(entry, "@import './nested/base.css' layer(base) screen; .tile { color: blue }");
    const output = path.join(root, 'output.css');
    const css = await bundleStyles(entry, output);
    assert.match(css, /@layer base/);
    assert.match(css, /@media screen/);
    assert.ok(css.indexOf('red') < Math.max(css.indexOf('blue'), css.indexOf('#00f')));
    assert.ok(css.includes(path.join(root, 'nested/tile.png') + '?v=1#image'));
    assert.doesNotMatch(css, /@import/);
    const map = JSON.parse(await readFile(output + '.map', 'utf8'));
    assert.ok(map.sources.some(source => source.endsWith('base.css')));
    assert.equal(await readFile(output, 'utf8'), css);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('invalid CSS and missing imports produce build errors', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'terlan-css-error-'));
  try {
    const entry = path.join(root, 'entry.css');
    const output = path.join(root, 'output.css');
    await writeFile(entry, "@import './missing.css';");
    await assert.rejects(bundleStyles(entry, output));
    await writeFile(entry, ".tile { color: '\n'; }");
    await assert.rejects(bundleStyles(entry, output));
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('external URLs and fragment references retain their meaning', () => {
  for (const url of ['/logo.svg', '#mask', 'data:image/png;base64,AAAA', 'https://example.com/a.png', '//example.com/a.png']) {
    assert.equal(rebaseUrl(url, '/styles/theme.css'), url);
  }
});
