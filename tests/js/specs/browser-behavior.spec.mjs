import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { output } from '../inventory.mjs';

let evidence;
test.beforeAll(async () => {
  const compilation = JSON.parse(await readFile(path.join(output, 'compilation.json')));
  evidence = compilation.results.find(result => result.module === 'browser.behavior');
});

test.beforeEach(async ({ page }) => {
  expect(evidence.status, evidence.diagnostics).toBe(0);
  await page.goto('/');
});

test('compiled Terlan string conversion preserves Unicode and empty strings', async ({ page }) => {
  const values = await page.evaluate(async artifact => {
    const api = await import(`/${artifact}`);
    return ['', 'Terlan', 'Rīga 🦊', '\u0000'].map(value => api.string_round_trip(value));
  }, evidence.artifact);
  expect(values).toEqual(['', 'Terlan', 'Rīga 🦊', '\u0000']);
});

test('compiled Terlan observes actual browser DOM state', async ({ page }) => {
  const observed = await page.evaluate(async artifact => {
    const api = await import(`/${artifact}`);
    const element = document.querySelector('main');
    const first = api.element_id(element);
    element.id = 'changed-by-browser';
    return [first, api.element_id(element)];
  }, evidence.artifact);
  expect(observed).toEqual(['fixture', 'changed-by-browser']);
});

test('compiled Terlan reads JavaScript array length after mutation', async ({ page }) => {
  const observed = await page.evaluate(async artifact => {
    const api = await import(`/${artifact}`);
    const values = [];
    const empty = api.array_length(values);
    values.push(11, 22, 33);
    return [empty, api.array_length(values)];
  }, evidence.artifact);
  expect(observed).toEqual([0, 3]);
});

test('compiled Terlan preserves the original URL property spelling', async ({ page }) => {
  const value = await page.evaluate(async artifact => (await import(`/${artifact}`)).document_url(document), evidence.artifact);
  expect(value).toBe(page.url());
});

test('compiled Terlan writes and reads DOM attributes including absent values', async ({ page }) => {
  const values = await page.evaluate(async artifact => {
    const api = await import(`/${artifact}`);
    const element = document.querySelector('main');
    const absent = api.get_attribute(element, 'data-state');
    api.set_attribute(element, 'data-state', 'ready');
    return [absent, api.get_attribute(element, 'data-state'), element.getAttribute('data-state')];
  }, evidence.artifact);
  expect(values).toEqual(['none', ['some', 'ready'], 'ready']);
});

test('compiled Terlan spreads rest arguments and preserves optional booleans', async ({ page }) => {
  const values = await page.evaluate(async artifact => {
    const api = await import(`/${artifact}`);
    const element = document.querySelector('main');
    api.add_classes(element);
    const initial = [api.has_class(element, 'active'), api.has_class(element, 'selected')];
    const removed = api.toggle_class(element, 'active', ['some', false]);
    const added = api.toggle_class(element, 'active', 'none');
    return [initial, removed, added, element.classList.contains('active')];
  }, evidence.artifact);
  expect(values).toEqual([[true, true], false, true, true]);
});

test('compiled Terlan matches nullable browser values through Option', async ({ page }) => {
  const values = await page.evaluate(async artifact => {
    const api = await import(`/${artifact}`);
    const event = new Event('terlan-test');
    const before = api.event_target(event);
    const beforeMatch = api.event_has_target(event);
    const element = document.querySelector('main');
    element.dispatchEvent(event);
    const after = api.event_target(event);
    let reads = 0;
    Object.defineProperty(event, 'target', { get() { reads++; return element; } });
    const afterMatch = api.event_has_target(event);
    return [before, after[0], after[1] === element, beforeMatch, afterMatch, reads];
  }, evidence.artifact);
  expect(values).toEqual(['none', 'some', true, false, true, 1]);
});
