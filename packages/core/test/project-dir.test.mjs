import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createSite } from "../index.js";

const require = createRequire(import.meta.url);

// createSite must root config discovery at projectDir, not at the Node
// process's cwd. This proves precedence directly: a decoy directory with its
// own real rw.toml (pointing at different docs content) sits at cwd while
// projectDir holds the site we actually asked for. If createSite ever went
// back to discovering rw.toml by walking up from process.cwd(), it would find
// the decoy's config and serve the decoy's page instead.
test("createSite roots at projectDir, not at a discoverable rw.toml in cwd", async () => {
  const projectDir = mkdtempSync(join(tmpdir(), "rw-project-dir-"));
  mkdirSync(join(projectDir, "docs"));
  writeFileSync(join(projectDir, "docs", "index.md"), "# Rooted here\n");

  const decoyDir = mkdtempSync(join(tmpdir(), "rw-decoy-dir-"));
  mkdirSync(join(decoyDir, "decoy-docs"));
  writeFileSync(join(decoyDir, "decoy-docs", "index.md"), "# Decoy content\n");
  writeFileSync(join(decoyDir, "rw.toml"), '[docs]\nsource_dir = "decoy-docs"\n');

  const cwdBefore = process.cwd();
  process.chdir(decoyDir);
  try {
    const site = createSite({ projectDir });
    const page = await site.renderPage("");
    assert.match(page.content, /Rooted here/);
    assert.doesNotMatch(page.content, /Decoy content/);
  } finally {
    process.chdir(cwdBefore);
  }
});

// A child process, so the variable never enters this one's environment.
test("createSite ignores RW_DIAGRAMS_KROKI_URL", () => {
  const projectDir = mkdtempSync(join(tmpdir(), "rw-project-dir-"));
  mkdirSync(join(projectDir, "docs"));
  const script = `
    const { createSite } = require(${JSON.stringify(require.resolve("../index.js"))});
    createSite({ projectDir: ${JSON.stringify(projectDir)} });
  `;

  const child = spawnSync(process.execPath, ["-e", script], {
    env: { ...process.env, RW_DIAGRAMS_KROKI_URL: "not-a-url" },
    encoding: "utf8",
  });

  assert.equal(child.status, 0, child.stderr);
});
