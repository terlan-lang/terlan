import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

const output = fileURLToPath(new URL('../../target/js-browser/', import.meta.url));
const port = Number(process.env.TERLAN_JS_TEST_PORT ?? 4197);
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  throw new Error('TERLAN_JS_TEST_PORT must be an integer from 1 to 65535');
}
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: './specs',
  globalSetup: './prepare.mjs',
  fullyParallel: true,
  workers: 4,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  timeout: 30_000,
  outputDir: `${output}/test-results`,
  reporter: [['list'], ['json', { outputFile: `${output}/results.json` }]],
  use: {
    baseURL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: 'node server.mjs',
    url: baseURL,
    reuseExistingServer: false,
    timeout: 15_000,
  },
  projects: ['chromium', 'firefox', 'webkit'].map(name => ({
    name,
    use: { browserName: name },
  })),
});
