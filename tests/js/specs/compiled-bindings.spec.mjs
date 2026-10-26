import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { inventory, output } from '../inventory.mjs';

test.use({ baseURL: process.env.TERLAN_JS_BASE_URL });

const { modules } = await inventory();
let compilation;
test.beforeAll(async () => {
  compilation = JSON.parse(await readFile(path.join(output, 'compilation.json')));
});

for (const module of modules) {
  test(`compiled binding contract: ${module.module}`, async ({ page }, testInfo) => {
    const evidence = compilation.results.find(result => result.module === module.module);
    await testInfo.attach('terlan-compilation', { body: JSON.stringify(evidence, null, 2), contentType: 'application/json' });
    expect(evidence.status, evidence.diagnostics).toBe(0);
    await page.goto('/');
    const result = await page.evaluate(async evidence => {
      const bindings = await import(`/${evidence.artifact}`);
      for (const name of evidence.tests) {
        if (await bindings[name]() !== true) throw new Error(`Compiled Terlan test failed: ${name}`);
      }
      if (evidence.module === 'std.js.Promise') {
        if (await bindings.promise_round_trip(Promise.resolve(42)) !== 42) throw new Error('Promise value was lost');
        const failure = new Error('browser rejection');
        const reason = await bindings.promise_round_trip(Promise.reject(failure)).catch(error => error);
        if (reason !== failure) throw new Error('Promise rejection was lost');
      }
      const checked = [];
      const optionalInner = type => type.match(/^(?:(?:std\.core\.)?Option|std\.core\.Option\.Option)\[(.*)\]$/)?.[1];
      const nativeValue = (type, marker) => {
        const inner = optionalInner(type);
        if (inner) return nativeValue(inner, marker);
        if (type === 'Bool') return false;
        if (/JsNumber$/.test(type)) return 37.25;
        if (/JsString$/.test(type)) return marker;
        if (/(?:^|\.)Array\[/.test(type)) return [];
        return { marker };
      };
      const wrap = (type, value) => {
        const inner = optionalInner(type);
        return inner ? ['some', wrap(inner, value)] : value;
      };
      for (const member of evidence.members ?? []) {
        const helper = bindings[`${member.name}_typechecks`];
        if (typeof helper !== 'function') throw new Error(`Missing compiled helper: ${member.name}`);
        if (member.kind === 'mixin') {
          const receiver = { marker: member.name };
          const callbacks = (member.parameters ?? []).map((_, index) => function (...args) {
            if (this !== receiver) throw new Error('Mixin callback lost its receiver');
            return { index, args };
          });
          if (member.js_names?.length !== callbacks.length) throw new Error(`Incomplete mixin contract: ${member.name}`);
          const actual = helper(receiver, ...callbacks);
          if (actual !== receiver || actual.marker !== member.name) throw new Error('Mixin lost target identity or state');
          (member.js_names ?? []).forEach((key, index) => {
            if (actual[key] !== callbacks[index]) throw new Error(`Mixin installed the wrong member: ${key}`);
            const argument = { marker: key };
            const observed = actual[key](argument);
            if (observed.index !== index || observed.args[0] !== argument) throw new Error(`Mixin changed callback arguments: ${key}`);
          });
          checked.push(member.name);
          continue;
        }
        const calls = [];
        let expected = nativeValue(member.return_type, `${evidence.module}.${member.js_name}`);
        const receiver = new Proxy({}, {
          get(_target, key) {
            calls.push({ kind: 'get', key: String(key) });
            if (key !== member.js_name) throw new Error(`Wrong host member: ${String(key)}; expected ${member.js_name}`);
            if (member.kind === 'property') return expected;
            return function (...args) {
              if (this !== receiver) throw new Error('Lost JavaScript receiver');
              calls.push({ kind: 'call', args });
              return expected;
            };
          },
        });
        const nativeArguments = [];
        const arguments_ = (member.parameters ?? []).map((parameter, index) => {
          const value = parameter.rest ? ['rest-one', 'rest-two'] : nativeValue(parameter.type, `argument-${index}`);
          nativeArguments.push(...(parameter.rest ? value : [value]));
          const wrapped = wrap(parameter.type, value);
          return parameter.optional ? ['some', wrapped] : wrapped;
        });
        const actual = helper(receiver, ...arguments_);
        const option = Boolean(optionalInner(member.return_type));
        const unit = ['Unit', 'std.core.Unit', 'std.core.Unit.Unit'].includes(member.return_type);
        const value = option ? actual[1] : actual;
        if (unit ? value !== 'unit' : value !== expected) throw new Error(`Incorrect result for ${member.name}`);
        if (option && actual[0] !== 'some') throw new Error(`Incorrect Option tag for ${member.name}`);
        if (calls.filter(call => call.kind === 'get').length !== 1) throw new Error(`Host getter evaluated more than once: ${member.name}`);
        if (member.kind === 'method') {
          const call = calls.find(call => call.kind === 'call');
          if (!call || call.args.length !== nativeArguments.length) throw new Error(`Incorrect call arity: ${member.name}`);
          call.args.forEach((argument, index) => {
            if (argument !== nativeArguments[index]) throw new Error(`Reordered host arguments: ${member.name}`);
          });
        }
        if (option) {
          for (const missing of [null, undefined]) {
            expected = missing;
            const observed = helper(receiver, ...arguments_);
            if (missing === null && member.undefined_only) {
              if (!Array.isArray(observed) || observed[0] !== 'some' || observed[1] !== null) throw new Error(`Present null was lost: ${member.name}`);
            } else if (observed !== 'none') throw new Error(`Absent result was not converted to None: ${member.name}`);
          }
        }
        if (member.parameters?.some(parameter => parameter.optional)) {
          calls.length = 0;
          const absent = arguments_.map((argument, index) => member.parameters[index].optional ? 'none' : argument);
          helper(receiver, ...absent);
          const call = calls.find(call => call.kind === 'call');
          let index = 0;
          for (const parameter of member.parameters) {
            if (parameter.optional && call.args[index] !== undefined) throw new Error(`Optional argument was not absent: ${member.name}`);
            index += parameter.rest ? 2 : 1;
          }
        }
        checked.push(member.name);
      }
      return { checked, exports: Object.keys(bindings) };
    }, evidence);
    expect(result.checked).toEqual((module.members ?? []).map(member => member.name));
    if (!module.bridge && module.members.length === 0) {
      testInfo.annotations.push({ type: 'coverage', description: 'Type-only module: compiled and loaded; no runtime API to exercise.' });
    }
    if (module.bridge) expect(result.exports.length).toBeGreaterThan(0);
  });
}
