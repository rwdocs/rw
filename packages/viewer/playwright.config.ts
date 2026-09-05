import path from "node:path";
import { defineConfig, devices } from "@playwright/test";

/** Absolute path to the workspace root (this file lives in packages/viewer). */
const workspaceRoot = path.resolve(import.meta.dirname, "../..");

/**
 * Environment overrides for every `rw serve` below, keeping the developer's
 * shell out of the fixtures.
 *
 * `RW_DIAGRAMS_KROKI_URL` is a documented way to set `diagrams.kroki_url`, and
 * none of the fixture configs declare a `[diagrams]` section, so an exported
 * value reaches all four servers. A value that is not a URL fails config
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
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
      testIgnore: ["embedded-*.spec.ts", "empty-nav.spec.ts"],
    },
    {
      name: "chromium-embedded",
      use: { ...devices["Desktop Chrome"], baseURL: "http://127.0.0.1:8082" },
      testMatch: "embedded-*.spec.ts",
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
      command:
        "./target/debug/rw serve --embedded -c packages/viewer/e2e/fixtures/rw-embedded.toml",
      url: "http://127.0.0.1:8082",
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
    //
    // The config path must be absolute: `rw` resolves `source_dir` against the
    // config's directory as given, and the file watcher matches the absolute
    // paths the OS reports against that prefix — a relative `-c` yields a
    // relative `source_dir` and every watch event is discarded. Computed here
    // with `path.resolve` rather than relying on a shell's `$PWD` reflecting
    // the `cwd` below, which is shell-dependent.
    {
      command: `./target/debug/rw serve -c ${path.join(workspaceRoot, "packages/viewer/e2e/fixtures/rw-live.toml")}`,
      url: "http://127.0.0.1:8084",
      env: hermeticEnv,
      cwd: "../..",
      reuseExistingServer: !process.env.CI,
      timeout: 30000,
    },
  ],
});
