import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { once } from "node:events";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
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

test("diagrams.krokiUrl overrides rw.toml's kroki_url", async () => {
  const hits = [];
  const server = createServer((req, res) => {
    hits.push(req.url.split("/")[1]);
    res.writeHead(500).end();
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const base = `http://127.0.0.1:${server.address().port}`;
  const projectDir = mkdtempSync(join(tmpdir(), "rw-project-dir-"));
  mkdirSync(join(projectDir, "docs"));
  writeFileSync(join(projectDir, "docs", "index.md"), "```plantuml\nA -> B\n```\n");
  writeFileSync(join(projectDir, "rw.toml"), `[diagrams]\nkroki_url = "${base}/toml"\n`);
  try {
    await createSite({ projectDir, diagrams: { krokiUrl: `${base}/option` } }).renderPage("");
    assert.deepEqual(hits, ["option"]);
    await createSite({ projectDir }).renderPage("");
    assert.deepEqual(hits, ["option", "toml"]);
  } finally {
    server.close();
  }
});

// A host renders repos it does not own, and rw.toml also names the Kroki
// server that included files are sent to.
test("createSite rejects an rw.toml include dir outside projectDir", () => {
  const projectDir = mkdtempSync(join(tmpdir(), "rw-project-dir-"));
  mkdirSync(join(projectDir, "docs"));
  writeFileSync(join(projectDir, "rw.toml"), '[diagrams]\ninclude_dirs = ["/"]\n');

  assert.throws(() => createSite({ projectDir }), /include_dirs entry "\/" is outside the project/);
});
