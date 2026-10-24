import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { definitionSyntax } from 'css-tree';
import { atomAdopted, featureAdoption, propertyAdoption } from './adoption.mjs';
import { generateProperty, pruneGeneratedModules } from './generate.mjs';

const browsers = { browser: { releases: { '2': { status: 'retired' }, '10.2': { status: 'current' }, '11': { status: 'beta' } } } };
const dataset = { browsers };
const feature = (support, status = { standard_track: true }) => ({ status, support: { browser: support } });

test('one released standards-track implementation is sufficient, including experimental or deprecated labels', () => {
  for (const extra of [{}, { experimental: true }, { deprecated: true }]) {
    assert.deepEqual(featureAdoption(feature({ version_added: '2' }, { standard_track: true, ...extra }), dataset),
      { included: true, browsers: ['browser'] });
  }
});

test('unimplemented proposals, nonstandard properties, and missing data are excluded', () => {
  assert.equal(featureAdoption(undefined, dataset).reason, 'no-compatibility-data');
  assert.equal(featureAdoption(feature({ version_added: '1' }, { standard_track: false }), dataset).reason, 'not-standards-track');
  assert.equal(featureAdoption(feature({ version_added: false }), dataset).reason, 'no-released-browser-implementation');
});

test('flagged, prefixed, renamed, removed, future, and preview-only implementations do not qualify', () => {
  for (const statement of [
    { version_added: '1', flags: [{ type: 'preference', name: 'experimental' }] },
    { version_added: '1', prefix: '-webkit-' },
    { version_added: '1', alternative_name: 'other-property' },
    { version_added: '1', version_removed: '10.2' },
    { version_added: '11' }, { version_added: 'preview' }, { version_added: null },
  ]) assert.equal(featureAdoption(feature(statement), dataset).included, false, JSON.stringify(statement));
});

test('support alternatives and numeric version bounds preserve shipped implementations', () => {
  assert.equal(featureAdoption(feature([
    { version_added: '1', prefix: '-webkit-' },
    { version_added: '≤10.1', version_removed: '11' },
  ]), dataset).included, true);
  assert.equal(featureAdoption(feature({ version_added: true }), dataset).included, true);
  assert.equal(featureAdoption(feature({ version_added: '10.2.1' }), dataset).included, false);
});

test('implemented CSS with editor-draft URLs is retained while spatial-navigation drafts are excluded', () => {
  assert.equal(propertyAdoption('width').included, true);
  assert.equal(propertyAdoption('border').included, true);
  assert.equal(propertyAdoption('spatial-navigation-contain').included, false);
  assert.equal(propertyAdoption('spatial-navigation-action').included, false);
  assert.equal(propertyAdoption('not-a-css-property').included, false);
});

test('unimplemented value proposals are excluded from otherwise implemented properties', () => {
  const grammar = [{ kind: 'keyword', name: 'auto' }, { kind: 'primitive', name: 'length' }];
  assert.equal(atomAdopted('width', { kind: 'keyword', name: 'auto' }, grammar), true);
  assert.equal(atomAdopted('width', { kind: 'keyword', name: 'contain' }, grammar), false);
  assert.equal(atomAdopted('width', { kind: 'keyword', name: 'stretch' }, grammar), true);
  assert.equal(atomAdopted('width', { kind: 'primitive', name: 'length' }, grammar), true);
  assert.equal(atomAdopted('width', { kind: 'primitive', name: 'angle' }, grammar), false);
});

test('generation excludes proposed keywords from both complete values and grouped parts', () => {
  const grammar = [{ kind: 'keyword', name: 'auto' }, { kind: 'primitive', name: 'length' }];
  const width = generateProperty({ name: 'width', syntax: 'auto | contain | <length>', href: 'spec' }, new Map(), grammar);
  assert.match(width.source, /pub type Css = Auto \|/);
  assert.doesNotMatch(width.source, /pub type Contain =/);
  const definitions = new Map([
    ['Type:line-width', definitionSyntax.parse('hairline | thin | <length>')],
    ['Type:line-style', definitionSyntax.parse('solid')],
  ]);
  const border = generateProperty({ name: 'border', syntax: '<line-width> || <line-style> || <color>', href: 'spec' }, definitions,
    [{ kind: 'keyword', name: 'thin' }, { kind: 'keyword', name: 'solid' }, { kind: 'primitive', name: 'length' }, { kind: 'primitive', name: 'color' }]);
  assert.match(border.source, /pub compose\(/);
  assert.doesNotMatch(border.source, /pub type Hairline =/);
});

test('pruning removes obsolete generated sources and interfaces while check mode is read-only', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'terlan-css-prune-'));
  try {
    const source = path.join(root, 'properties');
    const summaries = path.join(root, 'summaries');
    await mkdir(source); await mkdir(summaries);
    const obsolete = path.join(source, 'SpatialNavigationContain.terl');
    const text = '/** Generated from @webref/css 8.7.8. */\nmodule std.css.properties.SpatialNavigationContain.\n';
    await writeFile(obsolete, text);
    const artifacts = ['.typi', '.typi.deps'].map(suffix => path.join(summaries, 'std.css.properties.SpatialNavigationContain' + suffix));
    for (const artifact of artifacts) await writeFile(artifact, 'obsolete');
    await writeFile(path.join(source, 'Width.terl'), 'retained');
    const retained = new Set(['Width.terl']);
    await assert.rejects(pruneGeneratedModules(source, retained, summaries, true), /obsolete/);
    assert.equal(await readFile(obsolete, 'utf8'), text);
    await pruneGeneratedModules(source, retained, summaries);
    for (const removed of [obsolete, ...artifacts]) await assert.rejects(readFile(removed), { code: 'ENOENT' });
    assert.equal(await readFile(path.join(source, 'Width.terl'), 'utf8'), 'retained');
    const manual = path.join(source, 'Notes.md');
    await writeFile(manual, 'author-owned notes');
    await assert.rejects(pruneGeneratedModules(source, retained, summaries), /not owned/);
    assert.equal(await readFile(manual, 'utf8'), 'author-owned notes');
  } finally { await rm(root, { recursive: true, force: true }); }
});
