import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import { attrsS3Fixture } from "./helpers/attrs-s3-fixture.mjs";

const require = createRequire(import.meta.url);
const { createSite } = require("../index.js");

function assertAttrs(page, attrs) {
  if (Object.keys(attrs).length === 0) {
    assert.equal(Object.hasOwn(page.meta, "attrs"), false);
  } else {
    assert.deepEqual(page.meta.attrs, attrs);
  }
}

const initialLoad = ["GET manifest.json", "PUT cache/site/structure"];
const coldPage = (page) => [
  `GET cache/pages/${page}`,
  `GET pages/${page}.json`,
  `PUT cache/pages/${page}`,
];

for (const [label, attrs] of [
  ["empty", {}],
  [
    "populated",
    { owner: "old", nested: { audiences: ["operators"], nullable: null }, enabled: true },
  ],
]) {
  test(`S3 ${label} attrs use the same exact cold/warm requests and retain hits across reload`, async (t) => {
    const fixture = await attrsS3Fixture(t, attrs);
    const site = createSite(fixture.config);
    assert.deepEqual(fixture.takeRequests(), []);
    const first = await site.renderPage("selected");
    assert.deepEqual(fixture.takeRequests(), [...initialLoad, ...coldPage("selected")]);
    assertAttrs(first, attrs);
    const warm = await site.renderPage("selected");
    assert.deepEqual(fixture.takeRequests(), ["GET cache/pages/selected"]);
    assert.deepEqual(warm, first);
    const unrelated = await site.renderPage("unrelated");
    assert.deepEqual(fixture.takeRequests(), coldPage("unrelated"));
    assertAttrs(unrelated, {});
    assert.deepEqual(await site.renderPage("unrelated"), unrelated);
    assert.deepEqual(fixture.takeRequests(), ["GET cache/pages/unrelated"]);
    assertAttrs(await site.renderPage("virtual"), attrs);
    assert.deepEqual(fixture.takeRequests(), []);

    assert.equal(await site.reload(false), false);
    assert.deepEqual(fixture.takeRequests(), ["HEAD manifest.json"]);
    // Keep one Site and fixed render inputs to isolate attrs-only refresh.
    for (const [force, next] of [
      [false, { owner: "new" }],
      [true, { owner: "forced", values: [null, 2] }],
      [false, {}],
    ]) {
      const selectedCache = fixture.objects.get("cache/pages/selected");
      const unrelatedCache = fixture.objects.get("cache/pages/unrelated");
      const priorManifestEtag = fixture.objects.get("manifest.json").etag;
      fixture.setAttrs(next);
      assert.notEqual(fixture.objects.get("manifest.json").etag, priorManifestEtag);
      assert.equal(await site.reload(force), true);
      assert.deepEqual(fixture.takeRequests(), [
        ...(force ? [] : ["HEAD manifest.json"]),
        "GET cache/site/structure",
        "GET manifest.json",
        "PUT cache/site/structure",
      ]);
      const refreshed = await site.renderPage("selected");
      assert.deepEqual(fixture.takeRequests(), ["GET cache/pages/selected"]);
      assertAttrs(refreshed, next);
      assert.equal(refreshed.content, first.content);
      assert.deepEqual(
        { ...refreshed, meta: { ...refreshed.meta, attrs: undefined } },
        { ...first, meta: { ...first.meta, attrs: undefined } },
      );
      assert.deepEqual(await site.renderPage("unrelated"), unrelated);
      assert.deepEqual(fixture.takeRequests(), ["GET cache/pages/unrelated"]);
      assertAttrs(await site.renderPage("virtual"), next);
      assert.deepEqual(fixture.takeRequests(), []);
      assert.equal(fixture.objects.get("cache/pages/selected"), selectedCache);
      assert.equal(fixture.objects.get("cache/pages/unrelated"), unrelatedCache);
      assert.equal(await site.reload(false), false);
      assert.deepEqual(fixture.takeRequests(), ["HEAD manifest.json"]);
    }
  });

  test(`S3 ${label} metadata-only attrs need no page bundle or render-cache request even cold`, async (t) => {
    const fixture = await attrsS3Fixture(t, attrs);
    const site = createSite(fixture.config);
    const first = await site.renderPage("virtual");
    assert.deepEqual(fixture.takeRequests(), initialLoad);
    assertAttrs(first, attrs);
    assert.equal(first.content, "<h1>Virtual</h1>\n");
    assert.deepEqual(await site.renderPage("virtual"), first);
    assert.deepEqual(fixture.takeRequests(), []);
  });
}

// Catch extra Rust byte-decoder rounding, not ordinary JS large-integer limits.
test("finite attrs fractions agree across filesystem, S3 and structure-cache bytes", async (t) => {
  const attrs = { value: 51.248178375505404, small: 0.1, negative: -0.125 };
  const projectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rw-core-attrs-f64-"));
  t.after(() => fs.rmSync(projectDir, { recursive: true, force: true }));
  fs.writeFileSync(path.join(projectDir, "rw.toml"), "");
  fs.mkdirSync(path.join(projectDir, "docs"));
  fs.writeFileSync(
    path.join(projectDir, "docs", "selected.md"),
    "---\nattrs: {value: 51.248178375505404, small: 0.1, negative: -0.125}\n---\n# Selected\n",
  );
  assertAttrs(await createSite({ projectDir }).renderPage("selected"), attrs);

  const fixture = await attrsS3Fixture(t, attrs);
  const site = createSite(fixture.config);
  assertAttrs(await site.renderPage("selected"), attrs);
  assert.deepEqual(fixture.takeRequests(), [...initialLoad, ...coldPage("selected")]);

  // Change only the cache token so reload decodes the original Rust-produced float bytes.
  const structure = fixture.objects.get("cache/site/structure");
  assert.equal(structure.cacheEtag, "0");
  structure.cacheEtag = "1";
  assert.equal(await site.reload(true), true);
  assert.deepEqual(fixture.takeRequests(), ["GET cache/site/structure"]);
  assertAttrs(await site.renderPage("selected"), attrs);
  assert.deepEqual(fixture.takeRequests(), ["GET cache/pages/selected"]);
  assert.equal(fixture.objects.get("cache/site/structure"), structure);
});
