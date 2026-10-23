import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { inventory, output, root } from './inventory.mjs';

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

async function compile(compiler, entry) {
  const sourcePath = path.join(root, entry.test);
  const source = await readFile(sourcePath, 'utf8');
  const tests = [...source.matchAll(/@test\s+pub\s+(\w+)\s*\(\s*\)/g)].map(match => match[1]);
  const module = source.match(/^module\s+([\w.]+)\./m)?.[1];
  if (!module) throw new Error(`Missing Terlan module declaration: ${sourcePath}`);
  const destination = path.join(output, 'compiled', entry.module);
  await mkdir(destination, { recursive: true });
  // Compile an application that imports the shipped library, like a user's
  // project. The original binding checks supply its typed calls and provenance.
  const consumer = path.join(destination, 'Consumer.terl');
  await writeFile(consumer, source);
  const args = ['build', consumer, '--target', 'js.browser', '--out-dir', destination];
  const result = await new Promise((resolve, reject) => {
    const child = spawn(compiler, args, { cwd: root, stdio: ['ignore', 'pipe', 'pipe'] });
    const chunks = [];
    const timer = setTimeout(() => child.kill('SIGKILL'), 120_000);
    child.stdout.on('data', bytes => chunks.push(bytes));
    child.stderr.on('data', bytes => chunks.push(bytes));
    child.on('error', reject);
    child.on('close', (status, signal) => {
      clearTimeout(timer);
      resolve({ status, signal, diagnostics: Buffer.concat(chunks).toString() });
    });
  });
  let artifact = path.join(destination, 'js/modules', `${module.replaceAll('.', '/')}.js`);
  let artifactHash;
  let buildManifest;
  if (result.status === 0) {
    buildManifest = JSON.parse(await readFile(path.join(destination, 'js/manifest.json')));
    const emitted = buildManifest.modules.find(entry => entry.module === module);
    if (!emitted) throw new Error(`Build manifest does not contain ${module}`);
    if (buildManifest.target_profile !== 'js.browser' || buildManifest.module_format !== 'es-module'
        || emitted.target_profile !== 'js.browser' || emitted.validation_status !== 'ok') {
      throw new Error(`Build did not validate ${module} for js.browser`);
    }
    artifact = path.isAbsolute(emitted.artifact_path) ? emitted.artifact_path : path.resolve(root, emitted.artifact_path);
    artifactHash = sha256(await readFile(artifact));
  }
  return { ...entry, ...result, tests, moduleName: module, artifact: path.relative(output, artifact), sourceSha256: sha256(source), artifactSha256: artifactHash, command: [compiler, ...args] };
}

export default async function prepare() {
  const compiler = path.resolve(root, process.env.TERLC ?? 'target/debug/terlc');
  const compilerSha256 = sha256(await readFile(compiler));
  const { total, modules } = await inventory();
  const pending = [{ module: 'browser.behavior', test: 'tests/js/fixtures/Interop.terl' }, ...modules];
  const results = new Array(pending.length);
  let next = 0;
  const concurrency = Number(process.env.TERLAN_JS_COMPILE_JOBS ?? 4);
  if (!Number.isInteger(concurrency) || concurrency < 1 || concurrency > 8) throw new Error('TERLAN_JS_COMPILE_JOBS must be an integer from 1 to 8');
  await mkdir(output, { recursive: true });
  await Promise.all(Array.from({ length: concurrency }, async () => {
    while (next < pending.length) {
      const index = next++;
      results[index] = await compile(compiler, pending[index]);
      if ((index + 1) % 100 === 0) console.log(`Compiled ${index + 1}/${pending.length} browser fixtures`);
    }
  }));
  if (sha256(await readFile(compiler)) !== compilerSha256) throw new Error('Terlan compiler changed during browser compilation');
  const report = { schema: 'terlan.js.browser-compilation.v1', compilerSha256, totalModules: total, selectedModules: modules.length, results };
  await writeFile(path.join(output, 'compilation.json'), `${JSON.stringify(report, null, 2)}\n`);
  console.log(`Terlan browser compilation: ${results.filter(result => result.status === 0).length}/${results.length} succeeded`);
}
