import { test } from 'node:test';
import assert from 'node:assert/strict';
import { definitionSyntax } from 'css-tree';
import { atomicValues, generateProperty, groupedValues, repeatedValues } from './generate.mjs';

const parse = definitionSyntax.parse;
const values = syntax => atomicValues(parse(syntax), new Map());

test('required sequences and all-of groups cannot emit invalid individual tokens', () => {
  assert.deepEqual(values('solid <length>'), []);
  assert.deepEqual(values('solid && <length>'), []);
  assert.deepEqual(values('<length>{2,4}'), []);
});

test('alternatives, any-of groups and optional tails admit complete single values', () => {
  for (const syntax of ['auto | <length>', 'auto || <length>', '[none,]* auto | <length>']) {
    assert.deepEqual(values(syntax).map(v => v.name), ['auto', 'length']);
  }
  assert.deepEqual(values('<length>{1,4}').map(v => v.name), ['length']);
});

test('referenced numeric ranges survive alias expansion', () => {
  const definitions = new Map([['Type:length-percentage', parse('<length> | <percentage>')]]);
  const atoms = atomicValues(parse('<length-percentage [0,100]>'), definitions);
  assert.deepEqual(atoms.map(a => a.range), [
    { type: 'Range', min: 0, max: 100 }, { type: 'Range', min: 0, max: 100 },
  ]);
});

test('scoped definitions resolve in the owning property and cycles terminate', () => {
  const definitions = new Map([
    ['Type:opacity-value for opacity', parse('<number> | <percentage>')],
    ['Type:loop', parse('<loop>')],
  ]);
  assert.equal(atomicValues(parse('<opacity-value>'), definitions, new Set(), 'opacity').length, 2);
  assert.deepEqual(atomicValues(parse('<loop>'), definitions), []);
});

test('outer ranges cannot weaken a referenced type constraint', () => {
  const definitions = new Map([['Type:bounded', parse('<number [10,20]>')]]);
  assert.deepEqual(atomicValues(parse('<bounded [0,100]>'), definitions)[0].range, {type: 'Range', min: 10, max: 20});
  assert.deepEqual(atomicValues(parse('<bounded [21,100]>'), definitions), []);
});

test('property bindings preserve nominal identity, property spelling, and range checks', () => {
  const { source } = generateProperty({name: 'margin-left', syntax: '<length [0,100]> | auto', href: 'spec'}, new Map());
  assert.match(source, /module std\.css\.properties\.MarginLeft/);
  assert.match(source, /pub opaque type Value = SerializedValue/);
  assert.match(source, /margin_left: Value/);
  assert.match(source, /length_amount\(value\) >= 0 and std\.css\.Values\.length_amount\(value\) <= 100/);
  assert.match(source, /Option\[Value\]/);
  assert.doesNotMatch(source, /from_string|trusted/);
});

test('keyword enums replace zero-argument wrappers in values, components, and grouped parts', () => {
  const definitions = new Map([
    ['Type:line-width', parse('thin | <length [0,100]>')],
    ['Type:line-style', parse('none | solid')],
  ]);
  for (const syntax of ['auto | contain', 'auto | <length [0,100]>', '[auto | <length>]{1,4}', '<line-width> || <line-style> || <color>']) {
    const { source } = generateProperty({ name: 'example', syntax, href: 'spec' }, definitions);
    assert.match(source, /pub type Css = /);
    assert.match(source, /pub value\(css: Css\): Value/);
    assert.doesNotMatch(source, /pub\s+[a-z][a-z_]*\(\):/);
    assert.doesNotMatch(source, /struct (Token|SerializedValue)/);
  }
  const names = ['length', 'value', 'some', 'text', 'binary', 'number', 'term', 'dynamic', 'never', 'atom', 'bool', 'unit'];
  const { source } = generateProperty({ name: 'example', syntax: names.join(' | ') + ' | <length>', href: 'spec' }, new Map());
  for (const name of names) assert.ok(source.includes(`pub type ${name[0].toUpperCase() + name.slice(1)}Keyword = Atom[`));
});

test('bounded space-separated repetitions generate exactly the permitted arities', () => {
  const property = {name: 'example', syntax: '[auto | <length>]{2,3}', href: 'spec'};
  const {source, repeated} = generateProperty(property, new Map());
  assert.equal(repeated.min, 2);
  assert.equal(repeated.max, 3);
  assert.match(source, /pub opaque type Component = SerializedValue/);
  assert.match(source, /pub type ComponentCss = Auto/);
  assert.match(source, /pub component\(css: ComponentCss\): Component/);
  assert.match(source, /pub component_length\(value: Length\): Component/);
  assert.match(source, /pub pair\(first: Component, second: Component\): Value/);
  assert.match(source, /pub triple\(first: Component, second: Component, third: Component\): Value/);
  assert.doesNotMatch(source, /pub (single|quad)\(/);
  assert.doesNotMatch(source, /pub component_(inherit|initial|unset|revert)/);
});

test('repetition discovery omits optional tails but never required tails or separators', () => {
  const repetition = syntax => repeatedValues(parse(syntax), new Map());
  assert.equal(repetition('<length>{1,4} [ / <length>{1,4} ]?').max, 4);
  for (const syntax of ['<length>#', '<length>#{1,4}', '<length>+', '<length>{1,5}', '<length>{1,4} solid', '<length>{1,4} && solid', '<length>{1,4} || solid', '<length>{1,4} | auto']) {
    assert.equal(repetition(syntax), null, syntax);
  }
});

test('repeated components retain property-scoped definitions and numeric constraints', () => {
  const definitions = new Map([
    ['Property:edge', parse('<bounded [0,100]> | auto')],
    ['Type:bounded for edge', parse('<length [10,20]>')],
    ['Type:loop', parse('<loop>')],
  ]);
  const property = {name: 'example', syntax: "<'edge'>{1,4}", href: 'spec'};
  const {source, repeated} = generateProperty(property, definitions);
  assert.deepEqual(repeated.atoms[0].range, {type: 'Range', min: 10, max: 20});
  assert.match(source, /pub component_length\(value: Length\): Option\[Component\]/);
  assert.match(source, /length_amount\(value\) >= 10 and std.css.Values.length_amount\(value\) <= 20/);
  assert.equal(repeatedValues(parse('<loop>'), definitions), null);
});

test('named unordered groups produce distinct part types and a complete composition function', () => {
  const definitions = new Map([
    ['Type:line-width', parse('thin | <length [0,100]>')],
    ['Type:line-style', parse('none | solid')],
  ]);
  for (const combinator of ['&&', '||']) {
    const {source, grouped} = generateProperty({name: 'example', syntax: `<line-width> ${combinator} <line-style> ${combinator} <color>`, href: 'spec'}, definitions);
    assert.equal(grouped.combinator, combinator);
    assert.match(source, /pub opaque type LineWidthPart = String/);
    assert.match(source, /pub type LineStylePart = None \| Solid/);
    assert.match(source, /pub line_width_length\(value: Length\): Option\[LineWidthPart\]/);
    assert.match(source, /length_amount\(value\) >= 0 and std.css.Values.length_amount\(value\) <= 100/);
    assert.match(source, /pub type Solid = Atom\["solid"\]/);
    assert.doesNotMatch(source, /pub line_style_solid\(/);
    assert.match(source, /pub compose\(line_width: LineWidthPart, line_style: LineStylePart, color: Color\): Value/);
    assert.match(source, /line_width_css\(line_width\) \+ " " \+ Atom.to_string\(line_style\) \+ " " \+ std.css.Values.color_css\(color\)/);
    assert.doesNotMatch(source, /pub (line_width|line_style)_(inherit|initial|unset|revert)/);
  }
});

test('grouped composition rejects unsupported terms, duplicate names, lists, and cycles', () => {
  const definitions = new Map([['Type:loop', parse('<loop>')]]);
  for (const syntax of ['<length> | <color>', '<length> <color>', '<length> || <url>', '<length> || solid', '<length> || [<color> | auto]', '<length> || <length>', '[<length> || <color>]#', '<loop>']) {
    assert.equal(groupedValues(parse(syntax), definitions), null, syntax);
  }
});

test('scoped grouped aliases retain constraints and CSS-wide keywords stay out of parts', () => {
  const definitions = new Map([
    ['Property:width', parse('<number [10,20]> | inherit')],
    ['Type:layout for example', parse("<'width'> && <color>")],
  ]);
  const {source, grouped} = generateProperty({name: 'example', syntax: '<layout>', href: 'spec'}, definitions);
  assert.equal(grouped.groups[0].atoms.length, 1);
  assert.match(source, /pub width_number\(value: Float\): Option\[WidthPart\]/);
  assert.match(source, /value >= 10 and value <= 20/);
  assert.doesNotMatch(source, /pub width_inherit/);
  assert.match(source, /pub compose\(width: WidthPart, color: Color\): Value/);
});
