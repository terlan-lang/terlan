import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

const output = fileURLToPath(new URL('../../target/js-browser/', import.meta.url));

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
    baseURL: 'http://127.0.0.1:4197',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: 'node server.mjs',
    url: 'http://127.0.0.1:4197',
    reuseExistingServer: false,
    timeout: 15_000,
  },
  projects: ['chromium', 'firefox', 'webkit'].map(name => ({
    name,
    use: { browserName: name },
  })),
});
