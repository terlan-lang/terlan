import { readFile, readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

export const root = fileURLToPath(new URL('../../', import.meta.url));
export const output = path.join(root, 'target/js-browser');

async function sources(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(entries.map(async entry => {
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory()) return sources(filename);
    return entry.name.endsWith('.terl') && !entry.name.endsWith('Test.terl') ? [filename] : [];
  }));
  return nested.flat().sort();
}

export async function inventory() {
  const manifest = JSON.parse(await readFile(path.join(root, 'std/js/manifests/std_js_bindings.json')));
  const generated = new Map(manifest.outputs.map(module => [path.join(root, module.source), module]));
  const modules = [];
  for (const source of await sources(path.join(root, 'std/js'))) {
    const module = generated.get(source);
    if (module) modules.push(module);
    else {
      const text = await readFile(source, 'utf8');
      const name = text.match(/^module\s+([\w.]+)\./m)?.[1];
      if (!['std.js.String', 'std.js.Number', 'std.js.Array', 'std.js.Promise'].includes(name)) {
        throw new Error(`Missing browser test inventory entry: ${source}`);
      }
      modules.push({ module: name, source: path.relative(root, source), test: path.relative(root, source).replace('.terl', 'Test.terl'), bridge: true });
    }
  }
  if (modules.length !== manifest.outputs.length + 4) throw new Error('Browser inventory has missing or duplicate modules');
  const filter = process.env.TERLAN_JS_MODULES?.split(',').filter(Boolean);
  if (filter) {
    for (const name of filter) if (!modules.some(module => module.module === name)) throw new Error(`Unknown JS module selection: ${name}`);
  }
  return { total: modules.length, modules: filter ? modules.filter(module => filter.includes(module.module)) : modules };
}
