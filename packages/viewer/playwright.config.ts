import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "html",
  use: {
    baseURL: "http://127.0.0.1:8081",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
      testIgnore: "embedded-*.spec.ts",
    },
    {
      name: "chromium-embedded",
      use: { ...devices["Desktop Chrome"], baseURL: "http://127.0.0.1:8082" },
      testMatch: "embedded-*.spec.ts",
    },
  ],
  webServer: [
    {
      command: "./target/debug/rw serve -c packages/viewer/e2e/fixtures/rw.toml",
      url: "http://127.0.0.1:8081",
      cwd: "../..",
      reuseExistingServer: !process.env.CI,
      timeout: 30000,
    },
    {
      command:
        "./target/debug/rw serve --embedded -c packages/viewer/e2e/fixtures/rw-embedded.toml",
      url: "http://127.0.0.1:8082",
      cwd: "../..",
      reuseExistingServer: !process.env.CI,
      timeout: 30000,
    },
  ],
});
