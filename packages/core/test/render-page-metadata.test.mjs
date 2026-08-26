import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const require = createRequire(import.meta.url);
const { createSite } = require("../index.js");

const responseKeys = ["breadcrumbs", "content", "meta", "sectionAncestry", "toc"];
const requiredMetaKeys = ["lastModified", "path", "sectionRef", "sourceFile", "subpath", "title"];

function project(t) {
  const projectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rw-core-render-meta-"));
  t.after(() => fs.rmSync(projectDir, { recursive: true, force: true }));
  fs.writeFileSync(path.join(projectDir, "rw.toml"), "");
  fs.mkdirSync(path.join(projectDir, "docs"), { recursive: true });
  return projectDir;
}

test("renderPage returns resolved metadata without internal fields", async (t) => {
  const projectDir = project(t);
  const fixture = path.join(projectDir, "docs", "billing.md");
  fs.writeFileSync(
    fixture,
    [
      "---",
      "title: Billing",
      "description: Money stuff",
      "kind: domain",
      "---",
      "# Different H1",
      "",
    ].join("\n"),
  );
  const modified = new Date("2024-01-02T03:04:05.000Z");
  fs.utimesSync(fixture, modified, modified);

  const page = await createSite({ projectDir }).renderPage("billing");

  assert.equal(page.meta.title, "Billing");
  assert.equal(page.meta.path, "/billing");
  assert.equal(page.meta.sourceFile, "billing");
  assert.equal(page.meta.lastModified, "2024-01-02T03:04:05+00:00");
  assert.equal(page.meta.description, "Money stuff");
  assert.equal(page.meta.kind, "domain");
  assert.equal(page.meta.sectionRef, "domain:default/billing");
  assert.equal(page.meta.subpath, "");
  assert.deepEqual(page.sectionAncestry[page.meta.sectionRef][0], {
    sectionRef: "domain:default/billing",
    subpath: "",
  });
  assert.deepEqual(Object.keys(page).sort(), responseKeys);
  assert.deepEqual(Object.keys(page.meta).sort(), ["description", "kind", ...requiredMetaKeys]);
});

test("renderPage omits undeclared and internal metadata", async (t) => {
  const projectDir = project(t);
  fs.writeFileSync(path.join(projectDir, "docs", "index.md"), "Plain body.\n");

  const page = await createSite({ projectDir }).renderPage("");

  assert.equal(page.meta.title, "Index");
  assert.equal(page.meta.path, "/");
  assert.equal(page.meta.sourceFile, "");
  assert.equal(page.meta.sectionRef, "section:default/root");
  assert.equal(page.meta.subpath, "");
  assert.deepEqual(page.sectionAncestry[page.meta.sectionRef][0], {
    sectionRef: "section:default/root",
    subpath: "",
  });
  assert.deepEqual(Object.keys(page).sort(), responseKeys);
  assert.deepEqual(Object.keys(page.meta).sort(), requiredMetaKeys);
});
