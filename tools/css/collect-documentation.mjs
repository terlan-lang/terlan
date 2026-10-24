// Explicit snapshot refresh; normal generation never fetches documentation.
import { readFile, writeFile, readdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { extractArticle, normalize } from './documentation.mjs';
import webref from '@webref/css';
import mdn from 'mdn-data';
const directory = path.dirname(fileURLToPath(import.meta.url));
const checkout = process.argv[2];
if (!checkout) throw new Error('Usage: node collect-documentation.mjs /path/to/mdn/content');
const revision = execFileSync('git', ['-C', checkout, 'rev-parse', 'HEAD'], {encoding:'utf8'}).trim();
const bindings = JSON.parse(await readFile(path.join(directory, '../../std/css/bindings.json')));
const custom = JSON.parse(await readFile(path.join(directory, 'node_modules/@vscode/web-custom-data/data/browsers.css-data.json')));
const overrides = JSON.parse(await readFile(path.join(directory, 'documentation-overrides.json')));
const properties = {};
for (const property of bindings.properties) {
  const name = property.name;
  const page = name === 'color-adjust' ? 'print-color-adjust' : name;
  const article = await readFile(path.join(checkout, 'files/en-us/web/css/reference/properties', page, 'index.md'), 'utf8').catch(() => null);
  const fallback = custom.properties.find(p => p.name === name);
  const entry = article ? {...extractArticle(article), source: `https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/${page}`, license:'CC-BY-SA-2.5'} : {...overrides[name], values: overrides[name]?.values ?? {}};
  if (!entry.description) throw new Error(`Missing purpose documentation: ${name}`);
  for (const value of fallback?.values ?? []) {
    const description = typeof value.description === 'string' ? value.description : value.description?.value;
    if (!entry.values[value.name] && description) {
      entry.values[value.name] = normalize(description);
      entry.microsoftValues = true;
    }
  }
  properties[name] = entry;
}
const types = {};
const valueRoot = path.join(checkout, 'files/en-us/web/css/reference/values');
for (const name of await readdir(valueRoot)) {
  const article = await readFile(path.join(valueRoot, name, 'index.md'), 'utf8').catch(() => null);
  if (article) types[name] = {...extractArticle(article), source:`https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/${name}`};
}
const data = await webref.listAll();
const definitions = new Map();
for (const [kind, entries] of [['Type', data.types], ['Property', data.properties]]) {
  for (const entry of entries) {
    if (!entry.syntax) continue;
    const base = `${kind}:${entry.name.replace(/^<|>$/g, '')}`;
    for (const scope of entry.for ?? ['']) definitions.set(scope ? `${base} for ${scope}` : base, entry.syntax);
  }
}
function related(syntax, scope, seen = new Set()) {
  const result=[];
  for (const match of syntax.matchAll(/<('([^']+)'|([a-zA-Z][a-zA-Z0-9-]*))(?:[^>]*)>/g)) {
    const name = match[2] ?? match[3];
    const base = `${match[2] ? 'Property' : 'Type'}:${name}`;
    const key = definitions.has(`${base} for ${scope}`) ? `${base} for ${scope}` : base;
    if (seen.has(key)) continue;
    seen.add(key);
    const article = match[2] ? properties[name] : types[name];
    if (article) result.push(article);
    const reference = definitions.get(key);
    if (reference) result.push(...related(reference, match[2] ? name : scope, seen));
  }
  return result;
}
// Resolve documentation through the same grammar references that supply values.
// Longhands also carry keyword explanations omitted from shorthand articles.
for (const property of bindings.properties) {
  const entry = properties[property.name];
  const initial = mdn.css.properties[property.name]?.initial;
  const sources = [...related(property.syntax, property.name), ...(Array.isArray(initial) ? initial.map(name=>properties[name]).filter(Boolean) : [])];
  for (const source of sources) {
    for (const [keyword, meaning] of Object.entries(source.values)) {
      if (entry.values[keyword]) continue;
      entry.values[keyword] = meaning;
      (entry.valueSources ??= {})[keyword] = source.valueSources?.[keyword] ?? source.source;
      if (source.microsoftValues) entry.microsoftValues = true;
      entry.license = 'CC-BY-SA-2.5';
    }
  }
}
await writeFile(path.join(directory, 'documentation-data.json'), JSON.stringify({
  mdn: {repository:'https://github.com/mdn/content', revision, license:'CC-BY-SA-2.5'},
  microsoft: {package:'@vscode/web-custom-data', version:'0.6.3', license:'MIT'},
  properties,
}, null, 2) + '\n');
console.log(`Collected purpose documentation for ${Object.keys(properties).length} properties.`);
