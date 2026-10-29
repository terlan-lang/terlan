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

module.exports = { invalidateWorkspaceCache };
if (require.main === module) {
  const [metadata, target] = process.argv.slice(2);
  if (!metadata || !target || !path.isAbsolute(target)) throw new Error('metadata and absolute Cargo target required');
  const removed = invalidateWorkspaceCache(JSON.parse(fs.readFileSync(metadata, 'utf8')), target);
  console.log(`pre-commit: invalidated ${removed} workspace fingerprints; dependency caches and executables retained`);
}
