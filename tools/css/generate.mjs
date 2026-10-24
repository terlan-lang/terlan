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

export function generateProperty(property, definitions) {
  const name = typeName(property.name);
  const grammar = definitionSyntax.parse(property.syntax);
  const atoms = [...new Map(atomicValues(grammar, definitions, new Set(), property.name)
    .map(value => [JSON.stringify(value), value])).values()];
  const repeated = repeatedValues(grammar, definitions, new Set(), property.name);
  const grouped = groupedValues(grammar, definitions, new Set(), property.name);
  const keywords = [...new Set([...globals, ...atoms.filter(a => a.kind === 'keyword').map(a => a.name)])].sort();
  const constructors = [];
  const imports = new Set();
  for (const keyword of keywords) {
    constructors.push(`/** Returns the CSS keyword ${keyword}. */\npub ${callable(keyword)}(): Value ->\n    Value(Token {property: ${JSON.stringify(property.name)}, css: ${JSON.stringify(keyword)}}).`);
  }
  let needsOption = false;
  const token = (resultType, css) => `${resultType}(Token {property: ${JSON.stringify(property.name)}, css: ${css}})`;
  const emitPrimitives = (values, resultType, prefix = '', construct = css => token(resultType, css)) => {
    const emitted = new Set();
    for (const atom of values.filter(a => a.kind === 'primitive')) {
      const primitive = primitives.get(atom.name);
      const [valueType, render] = primitive ?? [atom.name === 'integer' ? 'Int' : 'Float', null];
      const range = atom.range;
      const min = typeof range?.min === 'number' ? range.min : null;
      const max = typeof range?.max === 'number' ? range.max : null;
      let fn = prefix + atom.name;
      if (emitted.has(fn)) fn += `_${emitted.size}`;
      emitted.add(fn);
      let argumentType = valueType;
      let expression;
      if (primitive) {
        imports.add(`import type std.css.Values.{${valueType}}.`);
        imports.add('import std.css.Values.');
        expression = `std.css.Values.${render}(value)`;
      } else {
        imports.add(`import std.core.${valueType}.`);
        expression = `std.core.${valueType}.to_string(value)`;
      }
      let check;
      if (primitive && (min !== null || max !== null)) {
        // Shared values retain a numeric magnitude, so grammar ranges are checked
        // before constructing a nominal property value.
        check = `std.css.Values.${atom.name}_amount(value)`;
      } else check = 'value';
      const conditions = [min !== null ? `${check} >= ${min}` : null, max !== null ? `${check} <= ${max}` : null].filter(Boolean);
      if (conditions.length) {
        needsOption = true;
        constructors.push(`/** Checks the specification range and constructs a ${property.name} value. */\npub ${fn}(value: ${argumentType}): Option[${resultType}] ->\n    case ${conditions.join(' and ')} {\n        true -> Some(${construct(expression)});\n        false -> None\n    }.`);
      } else {
        constructors.push(`/** Constructs a ${property.name} value from a typed ${atom.name}. */\npub ${fn}(value: ${argumentType}): ${resultType} ->\n    ${construct(expression)}.`);
      }
    }
  };
  emitPrimitives(atoms, 'Value');
  if (repeated) {
    for (const keyword of [...new Set(repeated.atoms.filter(a => a.kind === 'keyword').map(a => a.name))].sort()) {
      constructors.push(`/** Constructs one ${property.name} shorthand component. */\npub component_${callable(keyword)}(): Component ->\n    Component(Token {property: ${JSON.stringify(property.name)}, css: ${JSON.stringify(keyword)}}).`);
    }
    emitPrimitives(repeated.atoms, 'Component', 'component_');
    constructors.push('/** Serializes a library-owned shorthand component. */\ncomponent_css(value: Component): String ->\n    let token = component_token(value);\n    token.css.\n\ncomponent_token(value: Component): Token ->\n    value.');
    const parameters = ['first', 'second', 'third', 'fourth'];
    const arities = ['single', 'pair', 'triple', 'quad'];
    for (let count = repeated.min; count <= repeated.max; count++) {
      const names = parameters.slice(0, count);
      const args = names.map(name => `${name}: Component`).join(', ');
      const css = names.map(name => `component_css(${name})`).join(' + " " + ');
      constructors.push(`/** Combines ${count} typed components in specification order. */\npub ${arities[count - 1]}(${args}): Value ->\n    Value(Token {property: ${JSON.stringify(property.name)}, css: ${css}}).`);
    }
  }
  const partTypes = [];
  if (grouped) {
    const parameters = [];
    const expressions = [];
    for (const group of grouped.groups) {
      const parameter = callable(group.name);
      const [atom] = group.atoms;
      const direct = group.atoms.length === 1 && atom.kind === 'primitive'
        && atom.range?.min == null && atom.range?.max == null;
      if (direct) {
        const primitive = primitives.get(atom.name);
        const [valueType, render] = primitive ?? [atom.name === 'integer' ? 'Int' : 'Float', null];
        if (primitive) {
          imports.add(`import type std.css.Values.{${valueType}}.`);
          imports.add('import std.css.Values.');
          expressions.push(`std.css.Values.${render}(${parameter})`);
        } else {
          imports.add(`import std.core.${valueType}.`);
          expressions.push(`std.core.${valueType}.to_string(${parameter})`);
        }
        parameters.push(`${parameter}: ${valueType}`);
        continue;
      }
      const partType = `${typeName(group.name)}Part`;
      partTypes.push(`/** A non-global ${group.name} component of ${property.name}. */\npub opaque type ${partType} = String.\n\n`);
      const prefix = identifier(group.name) + '_';
      for (const keyword of [...new Set(group.atoms.filter(a => a.kind === 'keyword').map(a => a.name))].sort()) {
        constructors.push(`/** Constructs the ${group.name} keyword ${keyword}. */\npub ${prefix}${callable(keyword)}(): ${partType} ->\n    ${partType}(${JSON.stringify(keyword)}).`);
      }
      emitPrimitives(group.atoms, partType, prefix, css => `${partType}(${css})`);
      constructors.push(`${prefix}css(value: ${partType}): String ->\n    value.`);
      parameters.push(`${parameter}: ${partType}`);
      expressions.push(`${prefix}css(${parameter})`);
    }
    constructors.push(`/** Combines every named specification group in canonical order. */\npub compose(${parameters.join(', ')}): Value ->\n    ${token('Value', expressions.join(' + " " + '))}.`);
  }
  if (needsOption) imports.add('import std.core.Option.{None, Some}.');
  imports.add('import type std.core.Option.');
  const component = (repeated ? `/** A single non-global ${property.name} shorthand component. */\npub opaque type Component = Token.\n\n` : '') + partTypes.join('');
  const source = `/**\n * Generated from @webref/css 8.7.8. Do not edit.\n * Specification: ${property.href}\n * Grammar: ${property.syntax.replaceAll('*/', '* /')}\n * Constructors cover complete atomic alternatives; see bindings.json for scope.\n */\nmodule std.css.properties.${name}.\n\n${[...imports].sort().join('\n')}\n\n/** A nominal, specification-constrained ${property.name} value. */\npub opaque type Value = Token.\n\n${component}struct Token {\n    property: String,\n    css: String,\n}.\n\n/** Include this property in an application style struct. */\npub struct Property {\n    ${identifier(property.name)}: Value,\n}.\n\n/** Include an optional declaration in a shared style block. */\npub struct OptionalProperty {\n    ${identifier(property.name)}: std.core.Option[Value],\n}.\n\n${constructors.join('\n\n')}\n`;
  return { source, atoms, repeated, grouped };
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
  for (const property of data.properties.filter(p => /^[a-z][a-z0-9-]*$/.test(p.name) && p.syntax).sort((a, b) => a.name.localeCompare(b.name, 'en'))) {
    const { source, atoms, repeated, grouped } = generateProperty(property, definitions);
    files.set(`properties/${typeName(property.name)}.terl`, source);
    coverage.push({ name: property.name, field: identifier(property.name), module: `std.css.properties.${typeName(property.name)}`, syntax: property.syntax, href: property.href, constructors: atoms, ...(repeated ? {repetition: repeated} : {}), ...(grouped ? {composition: grouped} : {}) });
  }
  const snapshot = await readFile(new URL('node_modules/@webref/css/css.json', import.meta.url));
  files.set('bindings.json', JSON.stringify({ schema: 'terlan.css.bindings.v1', source: '@webref/css', version: '8.7.8', sha256: createHash('sha256').update(snapshot).digest('hex'), scope: 'CSS-wide keywords, complete atomic alternatives, bounded space-separated repetitions of up to four components, and all-term composition of named &&/|| groups; other compound values and CSS functions are not yet generated', properties: coverage }, null, 2) + '\n');
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
  for (const name of await readdir(`${directory}/properties`)) {
    if (!files.has(`properties/${name}`)) throw new Error(`untracked generated CSS binding: ${name}`);
  }
  process.stdout.write(`${check ? 'Verified' : 'Generated'} ${coverage.length} CSS property modules from @webref/css 8.7.8\n`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
