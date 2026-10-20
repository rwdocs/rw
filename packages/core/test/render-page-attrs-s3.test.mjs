import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { createSite } = require("../index.js");

async function s3Site(t, attrs) {
  const objects = new Map([
    ["pages/selected.json", { body: JSON.stringify({ content: "# Selected\n" }) }],
  ]);
  let manifestVersion = 0;
  const setAttrs = (next) => {
    const documents = [
      { path: "selected", title: "Selected", has_content: true, is_dir: false, attrs: next },
    ];
    objects.set("manifest.json", {
      body: JSON.stringify({ version: 1, documents }),
      etag: `"manifest-${++manifestVersion}"`,
    });
  };
  setAttrs(attrs);
  const fixture = { setAttrs, manifestGets: 0, authorization: undefined };
  const server = createServer(async (req, res) => {
    const key = new URL(req.url, "http://localhost").pathname.replace("/bucket/docs/", "");
    fixture.authorization ??= req.headers.authorization;
    if (req.method === "GET" && key === "manifest.json") fixture.manifestGets++;
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    if (req.method === "PUT") {
      objects.set(key, {
        body: Buffer.concat(chunks),
        cacheEtag: req.headers["x-amz-meta-cache-etag"],
      });
      res.end();
      return;
    }
    const object = objects.get(key);
    if (!object) {
      res.writeHead(404);
      res.end();
      return;
    }
    res.writeHead(200, {
      ...(object.etag && { ETag: object.etag }),
      ...(object.cacheEtag && { "x-amz-meta-cache-etag": object.cacheEtag }),
    });
    res.end(object.body);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => {
    server.closeAllConnections();
    server.close();
  });
  fixture.site = createSite({
    s3: {
      bucket: "bucket",
      entity: "docs",
      endpoint: `http://127.0.0.1:${server.address().port}`,
      accessKeyId: "dummy-key",
      secretAccessKey: "dummy-secret",
    },
  });
  return fixture;
}

test("S3 renderPage exposes manifest attrs on fresh, cached and reloaded renders", async (t) => {
  const attrs = { owner: "team", nested: { audiences: ["operators"], nullable: null }, on: true };
  const fixture = await s3Site(t, attrs);
  const { site } = fixture;
  assert.deepEqual((await site.renderPage("selected")).meta.attrs, attrs);
  assert.deepEqual((await site.renderPage("selected")).meta.attrs, attrs);
  assert.match(fixture.authorization, /Credential=dummy-key\//);

  assert.equal(await site.reload(false), false);
  assert.equal(fixture.manifestGets, 1);
  fixture.setAttrs({ owner: "new" });
  assert.equal(await site.reload(false), true);
  assert.deepEqual((await site.renderPage("selected")).meta.attrs, { owner: "new" });
});

// Catch extra Rust byte-decoder rounding, not ordinary JS large-integer limits.
test("finite attrs fractions agree across filesystem and S3 bytes", async (t) => {
  const attrs = { value: 51.248178375505404, small: 0.1, negative: -0.125 };
  const projectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rw-core-attrs-f64-"));
  t.after(() => fs.rmSync(projectDir, { recursive: true, force: true }));
  fs.mkdirSync(path.join(projectDir, "docs"));
  fs.writeFileSync(
    path.join(projectDir, "docs", "selected.md"),
    "---\nattrs: {value: 51.248178375505404, small: 0.1, negative: -0.125}\n---\n# Selected\n",
  );
  const fsPage = await createSite({ projectDir }).renderPage("selected");
  assert.deepEqual(fsPage.meta.attrs, attrs);

  const s3Page = await (await s3Site(t, attrs)).site.renderPage("selected");
  assert.deepEqual(s3Page.meta.attrs, attrs);
});
