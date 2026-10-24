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
  const name = typeName(property.name);
  const grammar = definitionSyntax.parse(property.syntax);
  const supported = atoms => adoptedAtoms === null ? atoms : atoms.filter(atom => atomAdopted(property.name, atom, adoptedAtoms));
  const atoms = [...new Map(supported(atomicValues(grammar, definitions, new Set(), property.name))
    .map(value => [JSON.stringify(value), value])).values()];
  let repeated = repeatedValues(grammar, definitions, new Set(), property.name);
  if (repeated) {
    repeated = { ...repeated, atoms: supported(repeated.atoms) };
    if (!repeated.atoms.length) repeated = null;
  }
  let grouped = groupedValues(grammar, definitions, new Set(), property.name);
  if (grouped) {
    grouped = { ...grouped, groups: grouped.groups.map(group => ({ ...group, atoms: supported(group.atoms) })) };
    if (grouped.groups.some(group => !group.atoms.length)) grouped = null;
  }
  const keywordAtoms = [...atoms, ...(repeated?.atoms ?? []), ...(grouped?.groups.flatMap(group => group.atoms) ?? [])]
    .filter(atom => atom.kind === 'keyword' && !globals.includes(atom.name));
  const reservedTypes = new Set(['Value', 'Css', 'CssWide', 'Property', 'Component', 'ComponentCss', 'SerializedValue', 'Absent', 'Some', 'Option', 'Length', 'Percentage', 'Color', 'Angle', 'Time',
    'Int', 'Float', 'Number', 'Binary', 'String', 'Text', 'Atom', 'Bool', 'Term', 'Dynamic', 'Never', 'Unit', 'List', 'Map', 'Set', 'Iterator', 'Task',
    ...(grouped?.groups.flatMap(group => [`${typeName(group.name)}Part`, `${typeName(group.name)}Css`]) ?? [])]);
  const keywordNames = new Map([...new Set(keywordAtoms.map(atom => atom.name))].sort().map(keyword => {
    const name = typeName(keyword);
    return [keyword, reservedTypes.has(name) ? name + 'Keyword' : name];
  }));
  const docs = documentation.properties[property.name];
  const declarations = [...keywordNames].map(([keyword, name]) => (docs?.values[keyword] ? comment(docs.values[keyword] + (docs.valueSources?.[keyword] ? `\n\nAuthor guide: ${docs.valueSources[keyword]}` : '')) + '\n' : '') + `pub type ${name} = Atom[${JSON.stringify(keyword)}].`);
  const keywordTypes = values => [...new Set(values.filter(atom => atom.kind === 'keyword' && !globals.includes(atom.name)).map(atom => keywordNames.get(atom.name)))];
  const constructors = [];
  const imports = new Set(['import std.core.Atom.', 'import std.css.Values.{serialize}.', 'import type std.css.Values.{Inherit, Initial, Unset, Revert, RevertLayer, SerializedValue}.']);
  const token = (resultType, css) => `${resultType}(serialize(${JSON.stringify(property.name)}, ${css}))`;
  const cssChoices = [...keywordTypes(atoms), 'Inherit', 'Initial', 'Unset', 'Revert', 'RevertLayer'];
  declarations.push(`pub type Css = ${cssChoices.join(' | ')}.`);
  constructors.push(`${comment(`Selects a keyword for ${property.name}. CSS-wide choices Inherit, Initial, Unset, Revert, and RevertLayer apply to the entire declaration.`)}\npub value(css: Css): Value ->\n    ${token('Value', 'Atom.to_string(css)')}.`);
  let needsOption = false;
  const emitPrimitives = (values, resultType, prefix = '', construct = css => token(resultType, css)) => {
    const emitted = new Set();
    for (const atom of values.filter(a => a.kind === 'primitive')) {
      const primitive = primitives.get(atom.name);
      const [valueType, render] = primitive ?? [atom.name === 'integer' ? 'Int' : 'Float', null];
      const min = typeof atom.range?.min === 'number' ? atom.range.min : null;
      const max = typeof atom.range?.max === 'number' ? atom.range.max : null;
      let fn = prefix + atom.name;
      if (emitted.has(fn)) fn += `_${emitted.size}`;
      emitted.add(fn);
      let expression;
      if (primitive) {
        imports.add(`import type std.css.Values.{${valueType}}.`);
        imports.add('import std.css.Values.');
        expression = `std.css.Values.${render}(value)`;
      } else {
        imports.add(`import std.core.${valueType}.`);
        expression = `std.core.${valueType}.to_string(value)`;
      }
      const check = primitive && (min !== null || max !== null) ? `std.css.Values.${atom.name}_amount(value)` : 'value';
      const conditions = [min !== null ? `${check} >= ${min}` : null, max !== null ? `${check} <= ${max}` : null].filter(Boolean);
      if (conditions.length) {
        needsOption = true;
        constructors.push(`${comment(`Sets ${property.name}${resultType === 'Component' ? ' shorthand component' : resultType !== 'Value' ? ' ' + resultType + ' part' : ''} from a typed ${atom.name}. Returns Some for ${min !== null ? 'amount >= ' + min : ''}${min !== null && max !== null ? ' and ' : ''}${max !== null ? 'amount <= ' + max : ''}; otherwise returns None.`)}\npub ${fn}(value: ${valueType}): Option[${resultType}] ->\n    case ${conditions.join(' and ')} {\n        true -> Some(${construct(expression)});\n        false -> Absent\n    }.`);
      } else {
        constructors.push(`${comment(`Sets ${property.name}${resultType === 'Component' ? ' shorthand component' : resultType !== 'Value' ? ' ' + resultType + ' part' : ''} from a typed ${atom.name}. ${atom.name === 'length' ? 'Use std.css.Values.px, root_em, em, vw, or vh to choose the unit.' : atom.name === 'color' ? 'Use std.css.Values.rgb or color to choose the paint.' : ''}`)}\npub ${fn}(value: ${valueType}): ${resultType} ->\n    ${construct(expression)}.`);
      }
    }
  };
  emitPrimitives(atoms, 'Value');
  if (repeated) {
    const choices = keywordTypes(repeated.atoms);
    if (choices.length) {
      declarations.push(`pub type ComponentCss = ${choices.join(' | ')}.`);
      constructors.push(`${comment(`Chooses one ${property.name} shorthand component. CSS-wide keywords are accepted only by value(css) for the whole declaration.`)}\npub component(css: ComponentCss): Component ->\n    ${token('Component', 'Atom.to_string(css)')}.`);
    }
    emitPrimitives(repeated.atoms, 'Component', 'component_');
    constructors.push('component_css(value: Component): String ->\n    let serialized = component_value(value);\n    serialized.css.\n\ncomponent_value(value: Component): SerializedValue ->\n    value.');
    const parameters = ['first', 'second', 'third', 'fourth'];
    const arities = ['single', 'pair', 'triple', 'quad'];
    for (let count = repeated.min; count <= repeated.max; count++) {
      const names = parameters.slice(0, count);
      const args = names.map(name => `${name}: Component`).join(', ');
      const css = names.map(name => `component_css(${name})`).join(' + " " + ');
      const fourSides = ['margin', 'padding', 'inset', 'border-color', 'border-width', 'border-style', 'scroll-margin', 'scroll-padding'].includes(property.name);
      const expansion = fourSides ? ['Applies to all four sides.', 'First sets top/bottom; second sets left/right.', 'First sets top; second sets left/right; third sets bottom.', 'Sets top, right, bottom, and left in that order.'][count - 1] : `Supplies ${count} components in the grammar order shown above.`;
      constructors.push(`${comment(expansion)}\npub ${arities[count - 1]}(${args}): Value ->\n    ${token('Value', css)}.`);
    }
  }
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
      const choices = keywordTypes(group.atoms);
      const keywordOnly = group.atoms.every(atom => atom.kind === 'keyword');
      const prefix = identifier(group.name) + '_';
      if (keywordOnly) {
        declarations.push(`pub type ${partType} = ${choices.join(' | ')}.`);
        expressions.push(`Atom.to_string(${parameter})`);
      } else {
        declarations.push(`pub opaque type ${partType} = String.`);
        if (choices.length) {
          const cssType = `${typeName(group.name)}Css`;
          declarations.push(`pub type ${cssType} = ${choices.join(' | ')}.`);
          constructors.push(`${comment(`Chooses the ${group.name} part for compose. See the keyword types for their visual behavior.`)}\npub ${parameter}(css: ${cssType}): ${partType} ->\n    ${partType}(Atom.to_string(css)).`);
        }
        emitPrimitives(group.atoms, partType, prefix, css => `${partType}(${css})`);
        constructors.push(`${prefix}css(value: ${partType}): String ->\n    value.`);
        expressions.push(`${prefix}css(${parameter})`);
      }
      parameters.push(`${parameter}: ${partType}`);
    }
    constructors.push(`${comment(`Sets ${property.name} by combining ${grouped.groups.map(group => group.name).join(', ')} in grammar order. Each argument describes a distinct part of the declaration.`)}\npub compose(${parameters.join(', ')}): Value ->\n    ${token('Value', expressions.join(' + " " + '))}.`);
  }
  if (needsOption) {
    imports.add('import std.core.Option.{None as Absent, Some}.');
    imports.add('import type std.core.Option.');
  }
  const component = repeated ? 'pub opaque type Component = SerializedValue.\n\n' : '';
  const overview = [docs?.description, propertyFacts(property.name), `Grammar: ${property.syntax}`, `Specification: ${property.href}`, ...(docs ? [`Author guide: ${docs.source}`, ...(docs.license ? ['Purpose and value descriptions adapted from MDN contributors (CC-BY-SA-2.5).'] : []), ...(docs.microsoftValues ? ['Additional value descriptions: Microsoft (MIT).'] : [])] : []), 'Generated from @webref/css 8.7.8. Do not edit.'].filter(Boolean).join('\n\n');
  const source = `${comment(overview)}\nmodule std.css.properties.${name}.\n\n${[...imports].sort().join('\n')}\n\n${declarations.join('\n\n')}\n\npub opaque type Value = SerializedValue.\n\n${component}pub struct Property {\n    ${identifier(property.name)}: Value,\n}.\n\n${constructors.join('\n\n')}\n`;
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
