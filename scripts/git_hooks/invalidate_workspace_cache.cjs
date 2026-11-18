// Shared Cargo targets can retain dep-info pointing at a different checkout.
// Invalidate workspace fingerprints, preserving outputs and registry caches.
const fs = require('node:fs');
const path = require('node:path');

function invalidateWorkspaceCache(metadata, target) {
  const members = new Set(metadata.workspace_members);
  const names = new Set(metadata.packages.filter(pkg => members.has(pkg.id)).map(pkg => pkg.name));
  const roots = [target];
  if (fs.existsSync(target)) {
    for (const entry of fs.readdirSync(target, { withFileTypes: true })) {
      if (entry.isDirectory()) roots.push(path.join(target, entry.name));
    }
  }
  let removed = 0;
  for (const root of roots) {
    for (const profile of ['debug', 'release']) {
      const directory = path.join(root, profile, '.fingerprint');
      if (!fs.existsSync(directory) || fs.lstatSync(directory).isSymbolicLink()) continue;
      for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
        const match = /^(.*)-[a-f0-9]{16}$/.exec(entry.name);
        if (entry.isDirectory() && match && names.has(match[1])) {
          fs.rmSync(path.join(directory, entry.name), { recursive: true });
          removed++;
        }
      }
    }
  }
  return removed;
}

function prepareWorkspaceCache(metadata, target, sourceRoot) {
  if (!path.isAbsolute(target) || !path.isAbsolute(sourceRoot)) {
    throw new Error('absolute Cargo target and source root required');
  }
  const marker = path.join(target, '.terlan-workspace-source-root');
  let status;
  try {
    status = fs.lstatSync(marker);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  if (status) {
    if (!status.isFile() || status.isSymbolicLink()) {
      throw new Error('invalid workspace source-root marker');
    }
    if (fs.readFileSync(marker, 'utf8') === `${sourceRoot}\n`) {
      return { removed: 0, reused: true };
    }
  }
  const removed = invalidateWorkspaceCache(metadata, target);
  fs.mkdirSync(target, { recursive: true });
  const temporary = `${marker}.${process.pid}.tmp`;
  fs.writeFileSync(temporary, `${sourceRoot}\n`, { flag: 'wx' });
  fs.renameSync(temporary, marker);
  return { removed, reused: false };
}

module.exports = { invalidateWorkspaceCache, prepareWorkspaceCache };
if (require.main === module) {
  const [metadata, target, sourceRoot] = process.argv.slice(2);
  if (!metadata || !target || !sourceRoot) throw new Error('metadata, Cargo target and source root required');
  const result = prepareWorkspaceCache(
    JSON.parse(fs.readFileSync(metadata, 'utf8')),
    target,
    sourceRoot,
  );
  if (result.reused) {
    console.log(`pre-commit: reuse workspace fingerprints for stable source root ${sourceRoot}`);
  } else {
    console.log(`pre-commit: source root changed; invalidated ${result.removed} workspace fingerprints`);
  }
}
