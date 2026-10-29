// Exercise actual Cargo reuse across two checkouts sharing a target directory.
const assert = require('node:assert/strict');
const cp = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { invalidateWorkspaceCache } = require('./invalidate_workspace_cache.cjs');

const root = fs.mkdtempSync(path.join(os.tmpdir(), 'terlan-hook-cache-'));
const target = path.join(root, 'target');
const first = path.join(root, 'first');
const second = path.join(root, 'second');
const executable = path.join(target, 'debug', `hook-cache-probe${process.platform === 'win32' ? '.exe' : ''}`);
const env = { ...process.env, CARGO_TARGET_DIR: target, CARGO_INCREMENTAL: '0', CARGO_PROFILE_DEV_DEBUG: '0' };
function cargo(cwd, args) {
  return cp.execFileSync('cargo', args, { cwd, env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
}
function output() { return cp.execFileSync(executable, [], { encoding: 'utf8' }).trim(); }
try {
  for (const [directory, value] of [[first, 'first'], [second, 'second']]) {
    fs.mkdirSync(path.join(directory, 'src'), { recursive: true });
    fs.writeFileSync(path.join(directory, 'Cargo.toml'), '[package]\nname="hook-cache-probe"\nversion="0.0.0"\nedition="2021"\n[workspace]\n');
    fs.writeFileSync(path.join(directory, 'src/main.rs'), `fn main() { println!("${value}"); }\n`);
  }
  const metadata = JSON.parse(cargo(first, ['metadata', '--offline', '--no-deps', '--format-version', '1']));
  cargo(first, ['build', '--offline']);
  assert.equal(output(), 'first');
  cargo(second, ['build', '--offline']);
  const unprotected = output();
  const marker = path.join(target, 'debug/.fingerprint/registry-dependency-0000000000000000/retained');
  fs.mkdirSync(path.dirname(marker), { recursive: true });
  fs.writeFileSync(marker, 'keep');
  assert.ok(invalidateWorkspaceCache(metadata, target) > 0);
  assert.ok(fs.existsSync(executable), 'invalidation must preserve installed executables');
  assert.equal(fs.readFileSync(marker, 'utf8'), 'keep', 'registry fingerprints must survive');
  cargo(second, ['build', '--offline']);
  assert.equal(output(), 'second', 'validation must compile the second checkout');
  invalidateWorkspaceCache(metadata, target);
  cargo(first, ['build', '--offline']);
  assert.equal(output(), 'first', 'returning to the original checkout must also compile its source');
  console.log(`Shared Cargo target regression passed (unprotected second checkout returned ${unprotected}).`);
} finally {
  fs.rmSync(root, { recursive: true, force: true });
}
