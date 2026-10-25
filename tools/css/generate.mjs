// Generate a conservative, typed CSS API from a locked specification snapshot.
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { readFile, mkdir, mkdtemp, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { definitionSyntax } from 'css-tree';
import webref from '@webref/css';
import mdn from 'mdn-data';
import { atomAdopted, propertyAdoption } from './adoption.mjs';
import { comment, normalize } from './documentation.mjs';
import { renderProperty } from './property.mjs';
const documentation = JSON.parse(await readFile(new URL('documentation-data.json', import.meta.url)));
const cssLabels = JSON.parse(await readFile(new URL('node_modules/mdn-data/l10n/css.json', import.meta.url)));
const label = key => Array.isArray(key) ? key.map(name => `${name}: ${label(mdn.css.properties[name]?.percentages ?? 'see specification')}`).join('; ') : normalize(cssLabels[key]?.['en-US'] ?? String(key));
function propertyFacts(name) {
  const entry = mdn.css.properties[name];
  if (!entry) return '';
  const initial = Array.isArray(entry.initial) ? entry.initial.map(name => `${name}: ${mdn.css.properties[name]?.initial ?? 'see specification'}`).join('; ') : entry.initial;
  return `Initial: ${initial}. Inherited: ${entry.inherited ? 'yes' : 'no'}.\nPercentages: ${entry.percentages === 'no' ? 'not accepted' : label(entry.percentages)}.`;
}

export const identifier = name => name.replaceAll('-', '_');
export const typeName = name => name.split('-').filter(Boolean).map(s => s[0].toUpperCase() + s.slice(1)).join('');
const primitives = new Map([
  ['length', ['Length', 'length_css']],
  ['percentage', ['Percentage', 'percentage_css']],
  ['color', ['Color', 'color_css']],
  ['angle', ['Angle', 'angle_css']],
  ['time', ['Time', 'time_css']],
]);

function intersectRanges(left, right) {
  if (!left) return right;
  if (!right) return left;
  const min = Math.max(left.min ?? -Infinity, right.min ?? -Infinity);
  const max = Math.min(left.max ?? Infinity, right.max ?? Infinity);
  return { type: 'Range', min: Number.isFinite(min) ? min : null, max: Number.isFinite(max) ? max : null };
}

// Each result is a complete valid value, never a token taken out of a required
// sequence. Unsupported combinations remain visible in the coverage manifest.
export function atomicValues(node, definitions, seen = new Set(), scope = '') {
  if (node.type === 'Keyword') return [{ kind: 'keyword', name: node.name }];
  if (node.type === 'Property' || node.type === 'Type') {
    if (node.type === 'Type' && (primitives.has(node.name) || ['number', 'integer'].includes(node.name))) {
      return [{ kind: 'primitive', name: node.name, range: node.opts }];
    }
    const base = `${node.type}:${node.name}`;
    const key = definitions.has(`${base} for ${scope}`) ? `${base} for ${scope}` : base;
    if (seen.has(key) || !definitions.has(key)) return [];
    return atomicValues(definitions.get(key), definitions, new Set([...seen, key]), node.type === 'Property' ? node.name : scope)
      .map(value => value.kind === 'primitive' && node.opts ? { ...value, range: intersectRanges(value.range, node.opts) } : value)
      .filter(value => !value.range || (value.range.min ?? -Infinity) <= (value.range.max ?? Infinity));
  }
  if (node.type === 'Multiplier') {
    return node.min <= 1 ? atomicValues(node.term, definitions, seen, scope) : [];
  }
  if (node.type === 'Group') {
    if (['|', '||'].includes(node.combinator) || node.terms.length === 1) {
      return node.terms.flatMap(term => atomicValues(term, definitions, seen, scope));
    }
    const required = node.terms.filter(term => !(term.type === 'Multiplier' && term.min === 0));
    return required.length === 1 ? atomicValues(required[0], definitions, seen, scope) : [];
  }
  return [];
}

const globals = ['inherit', 'initial', 'unset', 'revert', 'revert-layer'];

// A fixed-arity API is safe for a bounded, space-separated repetition of
// complete values. Do not infer such an API from comma lists, required tails,
// alternatives, or unbounded repetitions. Optional tails (e.g. elliptical
// border radii) can be omitted exactly as the source grammar permits.
export function repeatedValues(node, definitions, seen = new Set(), scope = '') {
  if (node.type === 'Group') {
    if (node.terms.length === 1) return repeatedValues(node.terms[0], definitions, seen, scope);
    if (node.combinator !== ' ') return null;
    const required = node.terms.filter(term => !(term.type === 'Multiplier' && term.min === 0));
    return required.length === 1 ? repeatedValues(required[0], definitions, seen, scope) : null;
  }
  if (node.type === 'Property' || node.type === 'Type') {
    // Ranges on references to compound types need separate grammar handling.
    if (node.opts) return null;
    const base = `${node.type}:${node.name}`;
    const key = definitions.has(`${base} for ${scope}`) ? `${base} for ${scope}` : base;
    if (seen.has(key) || !definitions.has(key)) return null;
    return repeatedValues(definitions.get(key), definitions, new Set([...seen, key]), node.type === 'Property' ? node.name : scope);
  }
  if (node.type !== 'Multiplier' || node.comma || node.min < 1 || node.max < 2 || node.max > 4) return null;
  const atoms = [...new Map(atomicValues(node.term, definitions, seen, scope)
    .filter(value => value.kind !== 'keyword' || !globals.includes(value.name))
    .map(value => [JSON.stringify(value), value])).values()];
  return atoms.length ? {min: node.min, max: node.max, atoms} : null;
}

// Supplying every term is valid for both && and ||. Each named term gets its
// own input type; arbitrary Values (including CSS-wide keywords) cannot fill
// a component slot. Alternatives, lists, and unnamed/nested groups stay out.
export function groupedValues(node, definitions, seen = new Set(), scope = '') {
  if (node.type === 'Property' || node.type === 'Type') {
    if (node.opts) return null;
    const base = `${node.type}:${node.name}`;
    const key = definitions.has(`${base} for ${scope}`) ? `${base} for ${scope}` : base;
    if (seen.has(key) || !definitions.has(key)) return null;
    return groupedValues(definitions.get(key), definitions, new Set([...seen, key]), node.type === 'Property' ? node.name : scope);
  }
  if (node.type !== 'Group') return null;
  if (node.terms.length === 1) return groupedValues(node.terms[0], definitions, seen, scope);
  if (!['&&', '||'].includes(node.combinator) || node.terms.length > 4) return null;
  if (!node.terms.every(term => ['Property', 'Type'].includes(term.type))) return null;
  const names = node.terms.map(term => term.name);
  if (new Set(names.map(typeName)).size !== names.length) return null;
  const groups = node.terms.map(term => ({name: term.name, atoms: [...new Map(
    atomicValues(term, definitions, seen, scope)
      .filter(value => value.kind !== 'keyword' || !globals.includes(value.name))
      .map(value => [JSON.stringify(value), value]),
  ).values()]}));
  return groups.every(group => group.atoms.length) ? {combinator: node.combinator, groups} : null;
}

const reserved = new Set(['type', 'module', 'import', 'pub', 'struct', 'with', 'case', 'if', 'else', 'let', 'const', 'in', 'for', 'trait', 'impl', 'fn', 'true', 'false', 'rem', 'div', 'and', 'or', 'not', 'xor', 'band', 'bor', 'bnot', 'bxor', 'bsl', 'bsr', 'receive', 'try', 'catch', 'after', 'when', 'of', 'end', 'begin']);
const callable = name => {
  const value = identifier(name);
  if (!/^[a-z_][a-zA-Z\d_]*$/.test(value)) return `keyword_${value}`;
  return reserved.has(value) ? `${value}_value` : value;
};

export function generateProperty(property, definitions, adoptedAtoms = null) {
  return renderProperty(property, definitions, adoptedAtoms, {
    identifier, typeName, atomicValues, repeatedValues, groupedValues,
    documentation, propertyFacts, keywordModule: null
  });
}

async function main() {
  const root = fileURLToPath(new URL('../../', import.meta.url));
  const data = await webref.listAll();
  const definitions = new Map();
  for (const [kind, entries] of [['Type', data.types], ['Property', data.properties]]) {
    for (const item of entries) {
      if (!item.syntax) continue;
      const base = `${kind}:${item.name.replace(/^<|>$/g, '')}`;
      for (const scope of item.for ?? ['']) definitions.set(scope ? `${base} for ${scope}` : base, definitionSyntax.parse(item.syntax));
    }
  }
  const files = new Map();
  const coverage = [];
  const excluded = [];
  const adoptedDefinitions = new Map([
    ...Object.entries(mdn.css.syntaxes).map(([name, value]) => [`Type:${name}`, definitionSyntax.parse(value.syntax)]),
    ...Object.entries(mdn.css.properties).map(([name, value]) => [`Property:${name}`, definitionSyntax.parse(value.syntax)]),
  ]);
  for (const property of data.properties.filter(p => /^[a-z][a-z0-9-]*$/.test(p.name) && p.syntax).sort((a, b) => a.name.localeCompare(b.name, 'en'))) {
    const adoption = propertyAdoption(property.name);
    if (!adoption.included) {
      excluded.push({ name: property.name, reason: adoption.reason });
      continue;
    }
    const adoptedGrammar = adoptedDefinitions.get(`Property:${property.name}`);
    const candidates = adoptedGrammar ? [
      ...atomicValues(adoptedGrammar, adoptedDefinitions, new Set(), property.name),
      ...(repeatedValues(adoptedGrammar, adoptedDefinitions, new Set(), property.name)?.atoms ?? []),
      ...(groupedValues(adoptedGrammar, adoptedDefinitions, new Set(), property.name)?.groups.flatMap(group => group.atoms) ?? []),
    ] : [];
    const { source, atoms, repeated, grouped } = generateProperty(property, definitions, candidates);
    files.set(`properties/${typeName(property.name)}.terl`, source);
    coverage.push({ name: property.name, field: identifier(property.name), module: `std.css.properties.${typeName(property.name)}`, syntax: property.syntax, href: property.href, browsers: adoption.browsers, constructors: atoms, ...(repeated ? {repetition: repeated} : {}), ...(grouped ? {composition: grouped} : {}) });
  }
  const snapshot = await readFile(new URL('node_modules/@webref/css/css.json', import.meta.url));
  const compatibilitySnapshot = await readFile(new URL('node_modules/@mdn/browser-compat-data/data.json', import.meta.url));
  const syntaxSnapshot = await readFile(new URL('node_modules/mdn-data/css/properties.json', import.meta.url));
  const typesSnapshot = await readFile(new URL('node_modules/mdn-data/css/syntaxes.json', import.meta.url));
  files.set('bindings.json', JSON.stringify({ schema: 'terlan.css.bindings.v1', source: '@webref/css', version: '8.7.8', sha256: createHash('sha256').update(snapshot).digest('hex'), adoption: {
    policy: 'Standards-track CSS with an unprefixed, unflagged implementation in at least one current released browser. Values require a released implementation or evidence in the MDN property grammar.',
    compatibility: { source: '@mdn/browser-compat-data', version: '8.1.7', sha256: createHash('sha256').update(compatibilitySnapshot).digest('hex') },
    syntax: { source: 'mdn-data', version: '2.37.2', sha256: createHash('sha256').update(syntaxSnapshot).update(typesSnapshot).digest('hex') },
    excluded,
  }, documentation: { mdn: documentation.mdn, microsoft: documentation.microsoft, sha256: createHash('sha256').update(await readFile(new URL('documentation-data.json', import.meta.url))).digest('hex') }, scope: 'CSS-wide keywords, complete atomic alternatives, bounded space-separated repetitions of up to four components, and all-term composition of named &&/|| groups; other compound values and CSS functions are not yet generated', properties: coverage }, null, 2) + '\n');
  // Generated sources obey the same canonical formatting gate as handwritten
  // library modules. Format in a temporary directory so --check stays read-only.
  const temporary = await mkdtemp(path.join(tmpdir(), 'terlan-css-format-'));
  try {
    await mkdir(path.join(temporary, 'properties'));
    const sources = [...files.keys()].filter(name => name.endsWith('.terl'));
    await Promise.all(sources.map(name => writeFile(path.join(temporary, name), files.get(name))));
    for (let pass = 0; pass < 4; pass++) {
      await promisify(execFile)(process.env.TERLC ?? path.join(root, 'target/debug/terlc'),
        ['fmt', '--write', temporary], {maxBuffer: 16 * 1024 * 1024});
      let changed = false;
      for (const name of sources) {
        const formatted = await readFile(path.join(temporary, name), 'utf8');
        changed ||= files.get(name) !== formatted;
        files.set(name, formatted);
      }
      if (!changed) break;
      if (pass === 3) throw new Error('CSS source formatting did not converge');
    }
  } finally {
    await rm(temporary, {recursive: true, force: true});
  }
  const directory = `${root}std/css`;
  const check = process.argv.includes('--check');
  if (!check) await mkdir(`${directory}/properties`, { recursive: true });
  for (const [name, content] of files) {
    const target = `${directory}/${name}`;
    if (check) {
      if (await readFile(target, 'utf8').catch(() => null) !== content) throw new Error(`stale generated CSS binding: ${target}`);
    } else await writeFile(target, content);
  }
  await pruneGeneratedModules(`${directory}/properties`, new Set([...files.keys()].filter(name => name.startsWith('properties/')).map(name => path.basename(name))), `${root}std/summaries`, check);
  process.stdout.write(`${check ? 'Verified' : 'Generated'} ${coverage.length} CSS property modules from @webref/css 8.7.8\n`);
}

export async function pruneGeneratedModules(directory, retained, summaries, check = false) {
  for (const name of await readdir(directory)) {
    if (retained.has(name)) continue;
    if (check) throw new Error(`obsolete generated CSS binding: ${name}`);
    const filename = path.join(directory, name);
    const module = `std.css.properties.${name.slice(0, -5)}`;
    const source = name.endsWith('.terl') ? await readFile(filename, 'utf8') : '';
    if (!source.includes('Generated from @webref/css ') || !source.includes(`module ${module}.`)) {
      throw new Error(`refusing to remove a file not owned by the CSS generator: ${filename}`);
    }
    await rm(filename);
    await rm(path.join(summaries, `${module}.typi`), { force: true });
    await rm(path.join(summaries, `${module}.typi.deps`), { force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
