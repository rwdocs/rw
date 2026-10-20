import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { createSite } = require("../index.js");

const PNG = Buffer.from("\x89PNG\r\n\x1a\n", "latin1");
const SHA256 = createHash("sha256").update(PNG).digest("hex");

function projectSite(t, options = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "rw-core-files-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const projectDir = path.join(root, "project");
  fs.mkdirSync(path.join(projectDir, "docs", "img"), { recursive: true });
  fs.writeFileSync(path.join(projectDir, "docs", "index.md"), "# Home\n\n![Flow](img/flow.png)\n");
  fs.writeFileSync(path.join(projectDir, "docs", "img", "flow.png"), PNG);
  fs.writeFileSync(path.join(projectDir, ".env"), "SECRET=1\n");
  fs.writeFileSync(path.join(root, "outside.png"), PNG);
  return { site: createSite({ projectDir, ...options }), root };
}

test("getFile serves an allowlisted project file with its type and a SHA-256 etag", async (t) => {
  const { site } = projectSite(t);

  const file = await site.getFile("docs/img/flow.png");

  assert.ok(Buffer.isBuffer(file.content));
  assert.deepEqual(file.content, PNG);
  assert.equal(file.contentType, "image/png");
  assert.equal(file.etag, `"${SHA256}"`);
});

test("getFile resolves to null for refused, missing and escaping paths", async (t) => {
  const { site, root } = projectSite(t);

  for (const refused of [
    ".env",
    "docs/index.md",
    "docs/img/missing.png",
    "../outside.png",
    "docs/../../outside.png",
    path.join(root, "outside.png"),
    "",
  ]) {
    assert.equal(await site.getFile(refused), null, refused);
  }
});

test("renderPage prefixes local image srcs with filesBaseUrl, dropping one trailing slash", async (t) => {
  for (const [filesBaseUrl, src] of [
    [undefined, "/_api/files/docs/img/flow.png"],
    [
      "https://backstage.example.com/api/rw/files",
      "https://backstage.example.com/api/rw/files/docs/img/flow.png",
    ],
    [
      "https://backstage.example.com/api/rw/files/",
      "https://backstage.example.com/api/rw/files/docs/img/flow.png",
    ],
  ]) {
    const { site } = projectSite(t, { filesBaseUrl });
    const page = await site.renderPage("");
    assert.ok(page.content.includes(`<img src="${src}"`), String(filesBaseUrl));
  }
});

// One fake bucket shared by every site, so their render caches share it too.
async function s3Bucket(t) {
  const objects = new Map([
    [
      "manifest.json",
      {
        body: JSON.stringify({
          version: 1,
          documents: [{ path: "guide", title: "Guide", has_content: true, is_dir: false }],
          source_dir: "docs",
          files: { "docs/img/flow.png": SHA256 },
        }),
        etag: '"manifest-1"',
      },
    ],
    [
      "pages/guide.json",
      { body: JSON.stringify({ content: "# Guide\n\n![Flow](img/flow.png)\n" }) },
    ],
    [`files/${SHA256}.png`, { body: PNG }],
  ]);
  const server = createServer(async (req, res) => {
    const key = new URL(req.url, "http://localhost").pathname.replace("/bucket/docs/", "");
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
  const site = (filesBaseUrl) =>
    createSite({
      s3: {
        bucket: "bucket",
        entity: "docs",
        endpoint: `http://127.0.0.1:${server.address().port}`,
        accessKeyId: "dummy-key",
        secretAccessKey: "dummy-secret",
      },
      filesBaseUrl,
    });
  const cachedPages = () => [...objects.keys()].filter((key) => key.includes("/pages/"));
  return { site, cachedPages };
}

test("S3 getFile serves a published file", async (t) => {
  const { site } = await s3Bucket(t);

  const file = await site().getFile("docs/img/flow.png");

  assert.deepEqual(file.content, PNG);
  assert.equal(file.contentType, "image/png");
  assert.equal(file.etag, `"${SHA256}"`);
  assert.equal(await site().getFile("docs/img/missing.png"), null);
});

test("S3 sites with different filesBaseUrl never serve each other's cached pages", async (t) => {
  const bucket = await s3Bucket(t);
  const a = bucket.site("https://a.example.com/files");
  const b = bucket.site("https://b.example.com/files");

  for (let round = 0; round < 2; round++) {
    assert.ok(
      (await a.renderPage("guide")).content.includes(
        'src="https://a.example.com/files/docs/img/flow.png"',
      ),
    );
    assert.ok(
      (await b.renderPage("guide")).content.includes(
        'src="https://b.example.com/files/docs/img/flow.png"',
      ),
    );
  }
  assert.equal(bucket.cachedPages().length, 1);
});
