// Check independent property modules in bounded batches. A single directory
// containing every module needlessly repeats a large shared interface graph.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, mkdir, readdir, copyFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const run = promisify(execFile);
const root = fileURLToPath(new URL('../../', import.meta.url));
const compiler = process.env.TERLC ?? path.join(root, 'target/debug/terlc');
const source = path.join(root, 'std/css');
const files = (await readdir(path.join(source, 'properties'))).filter(name => name.endsWith('.terl')).sort();
const temporary = await mkdtemp(path.join(tmpdir(), 'terlan-css-types-'));
const batchSize = 16;
const batches = Array.from({length: Math.ceil(files.length / batchSize)}, (_, index) => files.slice(index * batchSize, (index + 1) * batchSize));
let next = 0;
try {
  const results = await Promise.allSettled(Array.from({length: 4}, async () => {
    while (next < batches.length) {
      const index = next++;
      const directory = path.join(temporary, String(index));
      const destination = path.join(directory, 'std/css/properties');
      await mkdir(destination, {recursive: true});
      // Check the new shared definitions in every batch, without relying on
      // whichever CSS interfaces happen to be embedded in the installed CLI.
      for (const name of ['Values.terl', 'Stylesheet.terl']) {
        await copyFile(path.join(source, name), path.join(directory, 'std/css', name));
      }
      const dependencies = index === 0
        ? ['Border.terl', 'Gap.terl', 'Height.terl', 'Margin.terl', 'MarginLeft.terl', 'Padding.terl', 'Width.terl']
        : [];
      await Promise.all([...new Set([...batches[index], ...dependencies])]
        .map(name => copyFile(path.join(source, 'properties', name), path.join(destination, name))));
      if (index === 0) {
        for (const name of ['ValuesTest.terl', 'PropertiesTest.terl']) {
          await copyFile(path.join(source, name), path.join(directory, 'std/css', name));
        }
      }
      try {
        await run(compiler, ['check', directory], {cwd: root, maxBuffer: 16 * 1024 * 1024});
      } catch (error) {
        throw new Error(`CSS typecheck batch ${index} failed:\n${error.stdout ?? ''}${error.stderr ?? ''}`, {cause: error});
      }
    }
  }));
  const failures = results.filter(result => result.status === 'rejected').map(result => result.reason);
  if (failures.length) throw new AggregateError(failures, failures.map(error => error.message).join('\n'));
  console.log(`Checked all ${files.length} generated CSS modules and handwritten CSS helpers`);
} finally {
  await rm(temporary, {recursive: true, force: true});
}
