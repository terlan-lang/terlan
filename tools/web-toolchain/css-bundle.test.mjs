import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createRsbuild } from '@rsbuild/core';
import { bundleStyles, rebaseUrl } from './css-bundle.mjs';

const cssPath = (...parts) => path.join(...parts).replaceAll('\\', '/');

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
    assert.ok(css.includes(cssPath(root, 'nested/tile.png') + '?v=1#image'));
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

test('external imports retain URLs and conditions while local imports are bundled', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'terlan-css-imports-'));
  try {
    await mkdir(path.join(root, 'nested'));
    await writeFile(path.join(root, 'nested/base.css'),
      '@import "https://example.com/fonts.css" layer(fonts) supports(display: grid) screen;'
      + '@import "//example.com/theme.css"; .tile { color: red }');
    const entry = path.join(root, 'entry.css');
    await writeFile(entry, '@import "./nested/base.css"; .tile { color: blue }');
    const css = await bundleStyles(entry, path.join(root, 'output.css'));
    assert.match(css, /@import "https:\/\/example.com\/fonts.css" layer\(fonts\) supports\(display: grid\) screen/);
    assert.match(css, /@import "\/\/example.com\/theme.css"/);
    assert.ok(css.indexOf('red') < Math.max(css.indexOf('blue'), css.indexOf('#00f')));
    assert.doesNotMatch(css, /nested\/base.css/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('image-set URLs resolve from imported stylesheets for both URL and string syntax', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'terlan-css-images-'));
  try {
    await mkdir(path.join(root, 'nested'));
    await writeFile(path.join(root, 'nested/base.css'),
      '@import "https://example.com/remote.css";'
      + '.tile { background-image: image-set(url(./small.png?v=1#icon) 1x, "./large.png" 2x,'
      + ' "https://example.com/remote.png" 3x, linear-gradient(red, blue) 4x) }');
    const entry = path.join(root, 'entry.css');
    await writeFile(entry, '@import "./nested/base.css";');
    const css = await bundleStyles(entry, path.join(root, 'output.css'));
    assert.ok(css.includes(cssPath(root, 'nested/small.png') + '?v=1#icon'));
    assert.ok(css.includes(cssPath(root, 'nested/large.png')));
    assert.ok(css.includes('https://example.com/remote.png'));
    assert.match(css, /linear-gradient\(/);
    const map = JSON.parse(await readFile(path.join(root, 'output.css.map'), 'utf8'));
    assert.ok(map.sources.some(source => source.endsWith('base.css')));

    // Run the next stage too: the browser bundler must copy both images and
    // replace the absolute intermediate URLs with deployed asset URLs.
    const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j4N8AAAAASUVORK5CYII=', 'base64');
    for (const name of ['small.png', 'large.png']) {
      await writeFile(path.join(root, 'nested', name), png);
    }
    const javascript = path.join(root, 'index.js');
    const web = path.join(root, 'web');
    await writeFile(javascript, 'import "./output.css";');
    const rsbuild = await createRsbuild({
      cwd: root,
      rsbuildConfig: {
        root,
        source: { entry: { index: javascript } },
        tools: { lightningcssLoader: true },
        output: {
          distPath: { root: web }, dataUriLimit: 0, assetPrefix: '/',
          filename: { image: 'static/[name][ext]', css: 'static/[name].css' },
        },
      },
    });
    await rsbuild.build();
    const files = await readdir(web, { recursive: true });
    for (const name of ['small.png', 'large.png']) {
      const asset = files.find(file => path.basename(file) === name);
      assert.ok(asset, `missing copied image: ${name}`);
      assert.deepEqual(await readFile(path.join(web, asset)), png);
    }
    const builtCss = await readFile(path.join(web, files.find(file => file.endsWith('.css'))), 'utf8');
    assert.ok(!builtCss.includes(cssPath(root)));
    assert.match(builtCss, /small\.png/);
    assert.match(builtCss, /large\.png/);
    assert.ok(builtCss.includes('https://example.com/remote.png'));
    assert.ok(builtCss.includes('https://example.com/remote.css'));
  } finally { await rm(root, { recursive: true, force: true }); }
});
