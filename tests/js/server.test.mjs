import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';

test('browser port defaults to automatic allocation and preserves explicit ports', () => {
  for (const [configured, expected] of [[undefined, 0], ['0', 0], ['4197', 4197]]) {
    const env = { ...process.env };
    delete env.TERLAN_JS_TEST_PORT;
    if (configured !== undefined) env.TERLAN_JS_TEST_PORT = configured;
    const child = spawnSync(process.execPath, ['--input-type=module', '-e',
      `import { port } from ${JSON.stringify(new URL('./server-config.mjs', import.meta.url).href)}; console.log(port);`,
    ], { env, encoding: 'utf8' });
    assert.equal(child.status, 0, child.stderr);
    assert.equal(Number(child.stdout.trim()), expected);
  }
});

test('browser port rejects invalid configuration before starting', () => {
  for (const value of ['-1', '65536', '1.5', 'invalid']) {
    const child = spawnSync(process.execPath, ['--input-type=module', '-e',
      `import ${JSON.stringify(new URL('./server-config.mjs', import.meta.url).href)};`,
    ], { env: { ...process.env, TERLAN_JS_TEST_PORT: value }, encoding: 'utf8' });
    assert.notEqual(child.status, 0);
    assert.match(child.stderr, /TERLAN_JS_TEST_PORT must be an integer/);
  }
});

async function start(t) {
  const child = spawn(process.execPath, [fileURLToPath(new URL('./server.mjs', import.meta.url))], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, TERLAN_JS_TEST_PORT: '0' },
  });
  t.after(async () => {
    if (child.exitCode !== null || child.signalCode !== null) return;
    const closed = once(child, 'close');
    child.kill();
    await closed;
  });
  return new Promise((resolve, reject) => {
    let output = '';
    child.on('error', reject);
    child.on('close', code => reject(new Error(`Browser server exited: ${code}`)));
    child.stdout.on('data', bytes => {
      output += bytes;
      const address = output.match(/Terlan browser server: (http:\/\/127\.0\.0\.1:\d+)\n/);
      if (address) resolve(address[1]);
    });
  });
}

test('concurrent browser servers own distinct live ports', { timeout: 10_000 }, async t => {
  const addresses = await Promise.all([start(t), start(t)]);
  assert.notEqual(addresses[0], addresses[1]);
  for (const address of addresses) {
    const response = await fetch(address);
    assert.equal(response.status, 200);
    assert.match(await response.text(), /Terlan browser tests/);
  }
});
