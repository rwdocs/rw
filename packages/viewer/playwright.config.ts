import path from "node:path";
import { defineConfig, devices } from "@playwright/test";

/** Absolute path to the workspace root (this file lives in packages/viewer). */
const workspaceRoot = path.resolve(import.meta.dirname, "../..");

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
    {
      command: "./target/debug/rw serve -c packages/viewer/e2e/fixtures/rw-single.toml",
      url: "http://127.0.0.1:8083",
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
      cwd: "../..",
      reuseExistingServer: !process.env.CI,
      timeout: 30000,
    },
  ],
});
