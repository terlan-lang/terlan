import { defineConfig, devices } from "@playwright/test";

const baseURL = "http://terlan.test/";

export default defineConfig({
  testDir: ".",
  testMatch: "terlan.test.ts",
  fullyParallel: false,
  workers: 1,
  reporter: "line",
  use: {
    baseURL,
    screenshot: "only-on-failure",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "firefox",
      use: { ...devices["Desktop Firefox"] },
    },
  ],
});
