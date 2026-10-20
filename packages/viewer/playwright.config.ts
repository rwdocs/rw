import { defineConfig, devices } from "@playwright/test";

/**
 * Environment overrides for every `rw serve` below, keeping the developer's
 * shell out of the fixtures.
 *
 * `RW_DIAGRAMS_KROKI_URL` is a documented way to set `diagrams.kroki_url`, and
 * none of the fixture configs declare a `[diagrams]` section, so an exported
 * value reaches every server. A value that is not a URL fails config
 * validation outright, and `rw serve` refuses to boot.
 *
 * Playwright merges `env` into `process.env` and offers no way to clear it, so
 * every variable has to be named here; extend this list when `rw` gains another
 * `RW_*` fallback. An empty value works as a deletion because `rw` treats it as
 * unset.
 */
const hermeticEnv = { RW_DIAGRAMS_KROKI_URL: "" };

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "html",
  use: {
    baseURL: "http://127.0.0.1:8081",
    trace: {
      mode: "on-first-retry",
      snapshots: { dom: true, aria: true, screen: true },
    },
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
      testIgnore: ["embedded-*.spec.ts", "empty-nav.spec.ts"],
    },
    // Same server, under the preview prefix. A spec here must `goto` a
    // relative path ("./guide"): "/guide" leaves the prefix for the standalone
    // viewer, which renders the same docs, so the spec still passes.
    {
      name: "chromium-embedded",
      use: { ...devices["Desktop Chrome"], baseURL: "http://127.0.0.1:8081/_embedded/" },
      testMatch: ["embedded-*.spec.ts", "root-scope.spec.ts", "images.spec.ts"],
    },
    {
      name: "chromium-single",
      use: { ...devices["Desktop Chrome"], baseURL: "http://127.0.0.1:8083" },
      testMatch: "empty-nav.spec.ts",
    },
  ],
  webServer: [
    {
      command: "./target/debug/rw serve -c packages/viewer/e2e/fixtures/rw.toml",
      url: "http://127.0.0.1:8081",
      env: hermeticEnv,
      cwd: "../..",
      reuseExistingServer: !process.env.CI,
      timeout: 30000,
    },
    {
      command: "./target/debug/rw serve -c packages/viewer/e2e/fixtures/rw-single.toml",
      url: "http://127.0.0.1:8083",
      env: hermeticEnv,
      cwd: "../..",
      reuseExistingServer: !process.env.CI,
      timeout: 30000,
    },
    {
      command:
        "./target/debug/rw serve -c packages/viewer/e2e/fixtures/rw-comment-entity-decoding.toml",
      url: "http://127.0.0.1:8085",
      env: hermeticEnv,
      cwd: "../..",
      reuseExistingServer: !process.env.CI,
      timeout: 30000,
    },
    // Live reload enabled, same fixture docs. Kept on its own port so the
    // diagram-isolation spec can rewrite a fixture file without broadcasting a
    // reload into every other spec's page (the suite is fullyParallel).
    {
      command: "./target/debug/rw serve -c packages/viewer/e2e/fixtures/rw-live.toml",
      url: "http://127.0.0.1:8084",
      env: hermeticEnv,
      cwd: "../..",
      reuseExistingServer: !process.env.CI,
      timeout: 30000,
    },
  ],
});
