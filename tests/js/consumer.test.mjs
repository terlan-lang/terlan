import { test } from 'node:test';
import assert from 'node:assert/strict';
import { consumerSource } from './consumer.mjs';

const entry = { module: 'std.js.Boolean', members: [{ name: 'with_members', kind: 'mixin', return_type: 'BindingTarget', parameters: [{ type: '() -> Bool' }] }] };
const source = 'module std.js.BooleanTest.\n\nimport type std.js.Boolean.\n';

test('consumer adds a typed application call for each unrepresented mixin', () => {
  const consumer = consumerSource(source, entry);
  assert.match(consumer, /import std\.js\.Boolean\./);
  assert.match(consumer, /pub with_members_typechecks\[BindingTarget\]\(binding_target: BindingTarget, callback_0: \(\) -> Bool\)/);
  assert.match(consumer, /std\.js\.Boolean\.with_members\[BindingTarget\]\(binding_target, callback_0\)/);
  assert.equal(consumerSource(consumer, entry), consumer);
});

test('consumer preserves ordinary shipped tests byte for byte', () => {
  assert.equal(consumerSource(source, { members: [{ kind: 'property', name: 'value' }] }), source);
});

test('consumer rejects unsupported mixin signatures', () => {
  assert.throws(() => consumerSource(source, { ...entry, members: [{ ...entry.members[0], return_type: 'Other' }] }), /Unsupported mixin/);
});
