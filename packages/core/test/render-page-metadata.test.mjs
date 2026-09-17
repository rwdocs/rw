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

test("renderPage projects explicit section identity without exposing raw name", async (t) => {
  const projectDir = project(t);
  const fixture = path.join(projectDir, "docs", "billing.md");
  fs.writeFileSync(
    fixture,
    [
      "---",
      "title: Billing",
      "description: Money stuff",
      "kind: domain",
      "name: billing-api",
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
  assert.equal(page.meta.sectionRef, "domain:default/billing-api");
  assert.equal(page.meta.subpath, "");
  assert.deepEqual(page.sectionAncestry[page.meta.sectionRef][0], {
    sectionRef: "domain:default/billing-api",
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

// Losing the selected page's canonical attrs, using assignment for object keys,
// or converting unsigned JSON numbers to strings must fail at the JS boundary.
const attrsYaml = [
  "attrs:",
  "  audiences: [operators]",
  "  owner: null",
  "  large: 18446744073709551615",
  "  signed: -9223372036854775808",
  "  fraction: 1.25",
  "  enabled: true",
  "  disabled: false",
  '  "__proto__": {marker: root}',
  "  constructor: root-constructor",
  "  prototype: root-prototype",
  '  "key\\u0000tail": kept',
  "  recursive:",
  '    - [null, false, 3.5, "text\\u0000tail"]',
  '    - "__proto__": {marker: nested}',
  "      constructor: nested-constructor",
  "      prototype: nested-prototype",
  '      "key\\u0000tail": nested-kept',
  "    - {}",
  "    - []",
].join("\n");

for (const source of ["frontmatter", "metadata-only"]) {
  test(`renderPage exposes recursive attrs from ${source} as ordinary JS values`, async (t) => {
    const projectDir = project(t);
    const dir = path.join(projectDir, "docs", "selected");
    fs.mkdirSync(dir);
    const metadata = `title: Selected\nkind: domain\nname: selected-page\n${attrsYaml}\n`;
    if (source === "frontmatter") {
      fs.writeFileSync(path.join(dir, "index.md"), `---\n${metadata}---\n# Body\n`);
    } else {
      fs.writeFileSync(path.join(dir, "meta.yaml"), metadata);
    }

    const page = await createSite({ projectDir }).renderPage("selected");

    assert.ok(page.meta.attrs, "nonempty canonical attrs must be exposed");
    assert.deepEqual(page.meta.attrs.audiences, ["operators"]);
    assert.equal(page.meta.attrs.owner, null);
    assert.equal(typeof page.meta.attrs.large, "number");
    assert.equal(page.meta.attrs.large, Number("18446744073709551615"));
    assert.equal(typeof page.meta.attrs.signed, "number");
    assert.equal(page.meta.attrs.signed, Number("-9223372036854775808"));
    assert.equal(page.meta.attrs.fraction, 1.25);
    assert.equal(page.meta.attrs.enabled, true);
    assert.equal(page.meta.attrs.disabled, false);
    assert.equal(Object.hasOwn(page.meta.attrs, "__proto__"), true);
    assert.equal(Object.getPrototypeOf(page.meta.attrs), Object.prototype);
    assert.deepEqual(page.meta.attrs.__proto__, { marker: "root" });
    assert.equal(page.meta.attrs["key\u0000tail"], "kept");
    assert.deepEqual(page.meta.attrs.recursive[0], [null, false, 3.5, "text\u0000tail"]);
    const nested = page.meta.attrs.recursive[1];
    assert.equal(Object.hasOwn(nested, "__proto__"), true);
    assert.equal(Object.getPrototypeOf(nested), Object.prototype);
    assert.deepEqual(nested.__proto__, { marker: "nested" });
    assert.equal(nested["key\u0000tail"], "nested-kept");
    for (const [object, prefix] of [
      [page.meta.attrs, "root"],
      [nested, "nested"],
    ]) {
      for (const key of ["constructor", "prototype"]) {
        assert.equal(Object.hasOwn(object, key), true);
        assert.equal(object[key], `${prefix}-${key}`);
      }
      const descriptor = Object.getOwnPropertyDescriptor(object, "__proto__");
      assert.equal(descriptor.writable, true);
      assert.equal(descriptor.enumerable, true);
      assert.equal(descriptor.configurable, true);
    }
    assert.deepEqual(page.meta.attrs.recursive[2], {});
    assert.equal(Object.getPrototypeOf(page.meta.attrs.recursive[2]), Object.prototype);
    assert.deepEqual(page.meta.attrs.recursive[3], []);
    assert.equal(page.meta.title, "Selected");
    assert.equal(page.meta.path, "/selected");
    assert.equal(page.meta.sourceFile, source === "frontmatter" ? "selected" : "");
    assert.equal(page.meta.sectionRef, "domain:default/selected-page");
    assert.equal(page.meta.subpath, "");
    assert.deepEqual(Object.keys(page).sort(), responseKeys);
    assert.deepEqual(Object.keys(page.meta).sort(), ["attrs", "kind", ...requiredMetaKeys]);
  });
}

test("renderPage omits empty attrs on filesystem and metadata-only pages", async (t) => {
  const projectDir = project(t);
  // Parent attrs must not leak into either empty child.
  fs.writeFileSync(
    path.join(projectDir, "docs", "index.md"),
    "---\nattrs: {parent: true}\n---\nHome\n",
  );
  fs.writeFileSync(path.join(projectDir, "docs", "empty.md"), "---\nattrs: {}\n---\nEmpty\n");
  fs.mkdirSync(path.join(projectDir, "docs", "virtual"));
  fs.writeFileSync(path.join(projectDir, "docs", "virtual", "meta.yaml"), "attrs: {}\n");
  const site = createSite({ projectDir });
  for (const pagePath of ["empty", "virtual"]) {
    const emptyPage = await site.renderPage(pagePath);
    assert.equal(Object.hasOwn(emptyPage.meta, "attrs"), false);
  }
});
